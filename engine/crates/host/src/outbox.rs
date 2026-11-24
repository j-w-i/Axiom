//! What waits to be written to one client, bounded in bytes, and what the client follows.
//!
//! Answers always go in: whoever answers first waits for [`Outbox::room`]. Events go in through a
//! [`Follower`], which says whether there was room.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use axiom_proto::{ErrorCode, MAX_LINE, Message, SessionId, WireError};
use tokio::sync::Notify;

/// One line without its newline, shared by everyone it is sent to.
pub type Line = Arc<[u8]>;

pub struct Outbox {
    q: Mutex<Q>,
    ready: Notify,
    room: Notify,
    limit: usize,
}
#[derive(Default)]
struct Q {
    lines: VecDeque<Line>,
    bytes: usize,
    closed: bool,
    /// Which follow of a session may speak: the last one.
    follows: HashMap<SessionId, u64>,
    turn: u64,
}
impl Q {
    fn push(&mut self, line: Line) {
        self.bytes += line.len() + 1;
        self.lines.push_back(line);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Put {
    Done,
    Full,
    /// Replaced, ended, or the client has gone.
    Gone,
}

impl Outbox {
    pub fn new(limit: usize) -> Self {
        Self { q: Mutex::default(), ready: Notify::new(), room: Notify::new(), limit }
    }
    fn q(&self) -> MutexGuard<'_, Q> {
        self.q.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// An answer. Goes in whatever waits already; see [`Outbox::room`].
    pub fn answer(&self, id: u64, result: Result<serde_json::Value, WireError>) {
        let m = match result {
            Ok(result) => Message::Ok { id, result },
            Err(error) => Message::Err { id, error },
        };
        let fits = |l: &Vec<u8>| l.len() <= MAX_LINE;
        let Some(line) = serde_json::to_vec(&m).ok().filter(fits).or_else(|| {
            let error = WireError::new(ErrorCode::Internal, "the answer is too long to send");
            serde_json::to_vec(&Message::Err { id, error }).ok()
        }) else {
            return;
        };
        let mut q = self.q();
        if !q.closed {
            q.push(line.into());
            drop(q);
            self.ready.notify_one();
        }
    }

    /// Waits until another answer may be put in. False if the client has gone.
    pub async fn room(&self) -> bool {
        loop {
            let room = self.room.notified();
            tokio::pin!(room);
            room.as_mut().enable();
            {
                let q = self.q();
                if q.closed {
                    return false;
                }
                if q.bytes < self.limit {
                    return true;
                }
            }
            room.await;
        }
    }

    /// The client follows this session no more. Nothing of it goes in after this.
    pub fn unfollow(&self, session: &SessionId) {
        self.q().follows.remove(session);
        self.room.notify_waiters();
    }

    /// Everything waiting, or `None` once closed and drained.
    pub async fn take(&self) -> Option<Vec<u8>> {
        loop {
            let ready = self.ready.notified();
            {
                let mut q = self.q();
                if !q.lines.is_empty() {
                    let mut all = Vec::with_capacity(q.bytes);
                    for l in q.lines.drain(..) {
                        all.extend_from_slice(&l);
                        all.push(b'\n');
                    }
                    q.bytes = 0;
                    drop(q);
                    self.room.notify_waiters();
                    return Some(all);
                }
                if q.closed {
                    return None;
                }
            }
            ready.await;
        }
    }

    /// Nothing more goes in; what waits is still written.
    pub fn close(&self) {
        let mut q = self.q();
        q.closed = true;
        q.follows.clear();
        drop(q);
        self.ready.notify_one();
        self.room.notify_waiters();
    }

    /// The client reads no more.
    pub fn abandon(&self) {
        self.close();
        let mut q = self.q();
        q.lines = VecDeque::new();
        q.bytes = 0;
    }
}

/// One follow of one session by one client.
#[derive(Clone)]
pub struct Follower {
    out: Arc<Outbox>,
    session: SessionId,
    turn: u64,
}

impl Follower {
    /// Takes the place of the client's follow of this session before it, if there was one: from now
    /// on nothing of that one goes in, wherever it has got to.
    pub fn begin(out: &Arc<Outbox>, session: &SessionId) -> Self {
        let mut q = out.q();
        q.turn += 1;
        let turn = q.turn;
        q.follows.insert(session.clone(), turn);
        drop(q);
        out.room.notify_waiters();
        Self { out: out.clone(), session: session.clone(), turn }
    }

    pub fn current(&self) -> bool {
        self.speaks(&self.out.q())
    }
    fn speaks(&self, q: &Q) -> bool {
        !q.closed && q.follows.get(&self.session) == Some(&self.turn)
    }

    /// Never waits.
    pub fn offer(&self, line: &Line) -> Put {
        if line.len() > MAX_LINE {
            self.fault("an event is too long to send");
            return Put::Gone;
        }
        let mut q = self.out.q();
        if !self.speaks(&q) {
            return Put::Gone;
        }
        if q.bytes > 0 && q.bytes + line.len() >= self.out.limit {
            return Put::Full;
        }
        q.push(line.clone());
        drop(q);
        self.out.ready.notify_one();
        Put::Done
    }

    /// Waits for room. For history, which a client asked for. False if nobody wants it any more.
    pub async fn send(&self, line: &Line) -> bool {
        loop {
            let room = self.out.room.notified();
            tokio::pin!(room);
            room.as_mut().enable();
            match self.offer(line) {
                Put::Done => return true,
                Put::Gone => return false,
                Put::Full => room.await,
            }
        }
    }

    /// The client fell behind at `seq`, or with 0 is ahead of this history. Ends the follow.
    pub fn resync(&self, seq: u64) {
        self.last(&Message::Resync { session: self.session.clone(), resync: seq });
    }
    /// The session cannot be read or written. Ends the follow.
    pub fn fault(&self, words: &str) {
        self.last(&Message::Fault { session: self.session.clone(), fault: words.into() });
    }
    /// Goes in whatever waits already: it is short, and one per follow.
    fn last(&self, m: &Message) {
        let Ok(line) = serde_json::to_vec(m) else {
            return;
        };
        let mut q = self.out.q();
        if self.speaks(&q) {
            q.follows.remove(&self.session);
            q.push(line.into());
            drop(q);
            self.out.ready.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn line(n: usize) -> Line {
        vec![b'x'; n].into()
    }
    async fn soon<T>(f: impl Future<Output = T>) -> Option<T> {
        tokio::time::timeout(Duration::from_millis(20), f).await.ok()
    }

    async fn got(out: &Outbox) -> Option<Vec<u8>> {
        tokio::time::timeout(Duration::from_secs(5), out.take()).await.expect("something")
    }

    #[tokio::test]
    async fn history_waits_for_the_client_and_live_events_do_not() {
        let out = Arc::new(Outbox::new(100));
        let f = Follower::begin(&out, &SessionId("s".into()));
        assert!(soon(f.send(&line(60))).await.unwrap());
        assert_eq!(f.offer(&line(60)), Put::Full);
        assert_eq!(soon(f.send(&line(60))).await, None, "no room yet");
        assert_eq!(got(&out).await.unwrap().len(), 61);
        assert!(soon(f.send(&line(60))).await.unwrap());
        // One line may be longer than the limit, or it could never be sent.
        got(&out).await.unwrap();
        assert_eq!(f.offer(&line(1000)), Put::Done);
    }

    #[tokio::test]
    async fn no_more_is_asked_for_while_answers_wait() {
        let out = Outbox::new(100);
        assert!(out.room().await);
        out.answer(1, Ok("x".repeat(200).into()));
        assert_eq!(soon(out.room()).await, None);
        got(&out).await.unwrap();
        assert!(out.room().await);
        out.answer(2, Ok("x".repeat(200).into()));
        out.close();
        assert!(!out.room().await, "the client has gone");
        assert!(got(&out).await.is_some());
        assert!(got(&out).await.is_none());
    }

    #[tokio::test]
    async fn the_second_follow_silences_the_first() {
        let out = Arc::new(Outbox::new(1000));
        let s = SessionId("s".into());
        let first = Follower::begin(&out, &s);
        let other = Follower::begin(&out, &SessionId("t".into()));
        assert_eq!(first.offer(&line(1)), Put::Done);
        let second = Follower::begin(&out, &s);
        assert_eq!(first.offer(&line(1)), Put::Gone);
        assert!(!first.send(&line(1)).await);
        first.resync(1);
        first.fault("no");
        assert_eq!(got(&out).await.unwrap(), b"x\n", "not even its last word");
        assert_eq!(second.offer(&line(1)), Put::Done);
        assert_eq!(other.offer(&line(1)), Put::Done);
        out.unfollow(&s);
        assert_eq!(second.offer(&line(1)), Put::Gone);
        other.resync(7);
        assert_eq!(other.offer(&line(1)), Put::Gone, "nothing after its last word");
        assert_eq!(got(&out).await.unwrap(), b"x\nx\n{\"session\":\"t\",\"resync\":7}\n");
    }

    #[tokio::test]
    async fn a_follow_that_waits_for_room_ends_when_replaced() {
        let out = Arc::new(Outbox::new(10));
        let s = SessionId("s".into());
        let first = Follower::begin(&out, &s);
        assert_eq!(first.offer(&line(9)), Put::Done);
        let waits = tokio::spawn(async move { first.send(&line(9)).await });
        tokio::task::yield_now().await;
        let _second = Follower::begin(&out, &s);
        assert!(!soon(waits).await.unwrap().unwrap());
    }

    #[tokio::test]
    async fn a_line_no_reader_would_take_is_not_sent() {
        let out = Arc::new(Outbox::new(1000));
        out.answer(4, Ok("x".repeat(MAX_LINE).into()));
        let sent = String::from_utf8(got(&out).await.unwrap()).unwrap();
        assert!(sent.starts_with(r#"{"id":4,"error":{"code":"internal""#), "{sent}");
        let f = Follower::begin(&out, &SessionId("s".into()));
        assert_eq!(f.offer(&line(MAX_LINE + 1)), Put::Gone);
        assert!(got(&out).await.unwrap().starts_with(br#"{"session":"s","fault":"#));
    }

    #[tokio::test]
    async fn a_client_that_has_gone_holds_nothing() {
        let out = Arc::new(Outbox::new(1000));
        let f = Follower::begin(&out, &SessionId("s".into()));
        assert_eq!(f.offer(&line(900)), Put::Done);
        out.abandon();
        assert_eq!(out.q().bytes, 0);
        assert_eq!(out.q().lines.len(), 0);
        assert_eq!(f.offer(&line(1)), Put::Gone);
        out.answer(1, Ok(1.into()));
        assert!(got(&out).await.is_none());
    }
}
