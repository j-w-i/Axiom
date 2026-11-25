//! One task per session in use. It alone gives events their numbers. It ends when nobody holds the
//! session, or when the journal fails it; the next use opens it again from the journal.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use axiom_proto::{Body, Event, ItemId, SessionId, events_line, limits, now_ms};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Instant, sleep_until},
};

use crate::{
    faults::{Access, Faults},
    outbox::{Follower, Line, Put},
    store::Store,
    waiting::Waiting,
};

/// Deltas wait at most this long to be joined with others of their item.
pub const COALESCE: Duration = Duration::from_millis(33);
/// Deltas of one item are committed early when this much has gathered.
const FLUSH_BYTES: usize = 64 * 1024;
/// Events kept to join a follower without reading the journal.
const RECENT: usize = 1024;
/// How much of what was said faster than a commit is made may share the next one.
const TOGETHER: usize = 64;
/// Says that text is being dropped, until the item ends and the number is known.
const CUT: &str = "\n[more is not kept]";

pub use axiom_engine::Say;

pub enum Joined {
    Yes,
    /// The follower must read on after this.
    Behind(u64),
    /// The follower remembers more than there is.
    Ahead,
}

enum Cmd {
    Say(Vec<Say>),
    Join(Follower, u64, oneshot::Sender<Joined>),
    Sync(oneshot::Sender<u64>),
}

/// The session takes nothing more: its journal failed. [`Faults`] has the words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the session is closed")]
pub struct Closed;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error(transparent)]
    Closed(#[from] Closed),
    #[error(transparent)]
    TooLong(#[from] limits::TooLong),
}

/// Keeps the session open.
#[derive(Clone)]
pub struct Session {
    pub id: SessionId,
    tx: mpsc::Sender<Cmd>,
}

/// Does not keep it open.
pub struct Opened {
    id: SessionId,
    tx: mpsc::WeakSender<Cmd>,
    task: JoinHandle<()>,
}

impl Opened {
    pub fn session(&self) -> Option<Session> {
        let tx = self.tx.upgrade().filter(|tx| !tx.is_closed())?;
        Some(Session { id: self.id.clone(), tx })
    }
    pub fn ended(&self) -> bool {
        self.task.is_finished()
    }
    /// After everything it had to write is written.
    pub async fn end(self) {
        let _ = self.task.await;
    }
}

impl Session {
    pub fn open(
        id: SessionId,
        last_seq: u64,
        store: Store,
        faults: Arc<Faults>,
        waiting: Arc<Waiting>,
    ) -> (Self, Opened) {
        let (tx, rx) = mpsc::channel(256);
        let task = tokio::spawn(Actor::new(id.clone(), last_seq, store, faults, waiting).run(rx));
        let opened = Opened { id: id.clone(), tx: tx.downgrade(), task };
        (Self { id, tx }, opened)
    }

    /// Waits while the session is behind: this is the producer's backpressure. All of it is taken,
    /// or none.
    pub async fn say(&self, all: Vec<Say>) -> Result<(), Refused> {
        for s in &all {
            if let Say::Event(b) = s {
                b.check()?;
            }
        }
        Ok(self.tx.send(Cmd::Say(all)).await.map_err(|_| Closed)?)
    }
    /// Resolves once everything said before is in the journal.
    pub async fn sync(&self) -> Result<u64, Closed> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Cmd::Sync(tx)).await.map_err(|_| Closed)?;
        rx.await.map_err(|_| Closed)
    }
    /// Resolves when the session takes nothing more.
    pub async fn closed(&self) {
        self.tx.closed().await;
    }
    /// Sends what came after `after`, then everything, if that is not too long ago.
    pub async fn join(&self, f: Follower, after: u64) -> Result<Joined, Closed> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Cmd::Join(f, after, tx)).await.map_err(|_| Closed)?;
        rx.await.map_err(|_| Closed)
    }
}

/// Encoded events as lines of about a page each, with the number of the last event in each.
pub fn lines<'a>(
    id: &SessionId,
    events: impl IntoIterator<Item = &'a (u64, Box<[u8]>)>,
) -> serde_json::Result<Vec<(u64, Line)>> {
    let mut out = Vec::new();
    let (mut page, mut size, mut last) = (Vec::<&[u8]>::new(), 0, 0);
    for (seq, json) in events {
        if !page.is_empty() && size + json.len() > limits::PAGE {
            out.push((last, events_line(id, page.drain(..))?.into()));
            size = 0;
        }
        size += json.len() + 1;
        last = *seq;
        page.push(json);
    }
    if !page.is_empty() {
        out.push((last, events_line(id, page)?.into()));
    }
    Ok(out)
}

pub fn encode(events: &[Event]) -> serde_json::Result<Vec<(u64, Box<[u8]>)>> {
    events.iter().map(|e| Ok((e.seq, serde_json::to_vec(e)?.into()))).collect()
}

struct Item {
    text: String,
    /// Encoded bytes that may still be added to `text`.
    room: usize,
    pending: String,
    dropped: usize,
}

struct Actor {
    id: SessionId,
    seq: u64,
    store: Store,
    faults: Arc<Faults>,
    waiting: Arc<Waiting>,
    items: HashMap<ItemId, Item>,
    /// Items with pending text, oldest first.
    order: Vec<ItemId>,
    /// The last events, as they were sent.
    recent: VecDeque<(u64, Box<[u8]>)>,
    recent_bytes: usize,
    /// `recent` holds every event after this one.
    floor: u64,
    followers: Vec<Follower>,
    due: Option<Instant>,
}

impl Actor {
    fn new(
        id: SessionId,
        seq: u64,
        store: Store,
        faults: Arc<Faults>,
        waiting: Arc<Waiting>,
    ) -> Self {
        Self {
            id,
            seq,
            store,
            faults,
            waiting,
            items: HashMap::new(),
            order: Vec::new(),
            recent: VecDeque::new(),
            recent_bytes: 0,
            floor: seq,
            followers: Vec::new(),
            due: None,
        }
    }

    async fn run(mut self, rx: mpsc::Receiver<Cmd>) {
        self.work(rx).await;
        // Nobody is there any more who would hear an answer.
        self.waiting.over(&self.id);
    }

    async fn work(&mut self, mut rx: mpsc::Receiver<Cmd>) {
        loop {
            let due = self.due;
            let timer = async move {
                match due {
                    Some(t) => sleep_until(t).await,
                    None => std::future::pending().await,
                }
            };
            let r = tokio::select! {
                biased;
                () = timer => self.flush(Vec::new()).await,
                c = rx.recv() => match c {
                    Some(c) => self.handle(c, &mut rx).await,
                    None => {
                        let _ = self.flush(Vec::new()).await;
                        return;
                    }
                },
            };
            if r.is_err() {
                return;
            }
        }
    }

    async fn handle(&mut self, c: Cmd, rx: &mut mpsc::Receiver<Cmd>) -> Result<(), Closed> {
        let mut next = Some(c);
        while let Some(c) = next.take() {
            match c {
                Cmd::Say(all) => {
                    let mut bodies = Vec::new();
                    all.into_iter().for_each(|s| self.say(s, &mut bodies));
                    // What waits already shares the commit: each may be a flush of the drive.
                    for _ in 1..TOGETHER {
                        match rx.try_recv() {
                            Ok(Cmd::Say(all)) => {
                                all.into_iter().for_each(|s| self.say(s, &mut bodies));
                            }
                            Ok(c) => {
                                next = Some(c);
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                    let heavy = self.items.values().any(|i| i.pending.len() > FLUSH_BYTES);
                    if !bodies.is_empty() || heavy {
                        self.flush(bodies).await?;
                    }
                }
                Cmd::Join(f, after, joined) => {
                    let _ = joined.send(self.join(f, after));
                }
                Cmd::Sync(tx) => {
                    self.flush(Vec::new()).await?;
                    let _ = tx.send(self.seq);
                }
            }
        }
        Ok(())
    }

    fn join(&mut self, f: Follower, mut after: u64) -> Joined {
        if after > self.seq {
            return Joined::Ahead;
        }
        if after < self.floor {
            return Joined::Behind(after);
        }
        let Ok(gap) = lines(&self.id, self.recent.iter().filter(|(seq, _)| *seq > after)) else {
            return Joined::Behind(after);
        };
        for (last, line) in gap {
            match f.offer(&line) {
                Put::Done => after = last,
                Put::Full => return Joined::Behind(after),
                // Nobody waits for the answer.
                Put::Gone => return Joined::Yes,
            }
        }
        self.followers.retain(Follower::current);
        self.followers.push(f);
        Joined::Yes
    }

    /// Text waits; anything else goes out after the text that came before it.
    fn say(&mut self, s: Say, bodies: &mut Vec<Body>) {
        match s {
            Say::Text(item, text) => {
                let Some(i) = self.items.get_mut(&item) else {
                    return;
                };
                // Once something is dropped, so is everything after it: no silent gaps.
                let kept = if i.dropped > 0 { "" } else { limits::head(&text, i.room) };
                i.room -= limits::text_len(kept);
                i.text.push_str(kept);
                let fresh = i.pending.is_empty();
                i.pending.push_str(kept);
                if kept.len() < text.len() {
                    if i.dropped == 0 {
                        i.pending.push_str(CUT);
                    }
                    i.dropped += text.len() - kept.len();
                }
                if fresh && !i.pending.is_empty() {
                    self.order.push(item);
                    self.due.get_or_insert_with(|| Instant::now() + COALESCE);
                }
            }
            Say::End(item, outcome, result) => {
                let Some(mut i) = self.items.remove(&item) else {
                    return;
                };
                // Its own pending text is in `text`, which replaces it. Other items' came before.
                self.order.retain(|o| *o != item);
                self.drain(bodies);
                if i.dropped > 0 {
                    i.text.push_str(&limits::cut_note(i.dropped));
                }
                bodies.push(Body::ItemEnded { item, outcome, text: i.text, result, result_cut: 0 });
            }
            Say::Event(b) => {
                self.drain(bodies);
                if let Body::ItemStarted { item, .. } = &b {
                    let new = Item {
                        text: String::new(),
                        room: limits::TEXT - limits::NOTE,
                        pending: String::new(),
                        dropped: 0,
                    };
                    self.items.insert(item.clone(), new);
                }
                bodies.push(b);
            }
        }
    }

    fn drain(&mut self, bodies: &mut Vec<Body>) {
        for item in self.order.drain(..) {
            if let Some(i) = self.items.get_mut(&item) {
                let text = std::mem::take(&mut i.pending);
                bodies.push(Body::ItemDelta { item, text });
            }
        }
        self.due = None;
    }

    async fn flush(&mut self, mut bodies: Vec<Body>) -> Result<(), Closed> {
        if self.due.is_some_and(|d| d <= Instant::now()) || bodies.is_empty() {
            self.drain(&mut bodies);
        }
        if bodies.is_empty() {
            return Ok(());
        }
        let at = now_ms();
        let events: Vec<_> = bodies
            .into_iter()
            .zip(self.seq + 1..)
            .map(|(mut body, seq)| {
                body.fit();
                Event { seq, at, body }
            })
            .collect();
        let sent = async {
            let encoded = encode(&events)?;
            let lines = match self.followers.len() {
                0 => Vec::new(),
                _ => lines(&self.id, &encoded)?,
            };
            // Nothing is sent before it is in the journal.
            let asked = |e: &&Event| {
                matches!(e.body, Body::RequestOpened { .. } | Body::RequestClosed { .. })
            };
            let waits: Vec<Event> = events.iter().filter(asked).cloned().collect();
            self.store.append(self.id.clone(), events).await?;
            self.waiting.note(&self.id, &waits);
            Ok::<_, axiom_journal::Error>((encoded, lines))
        }
        .await;
        let (encoded, lines) = match sent {
            Ok(x) => x,
            Err(e) => {
                // What would not survive is not shown: the session ends here, and says so.
                let words = self.faults.note(&self.id, Access::Write, &e);
                for f in self.followers.drain(..) {
                    f.fault(&words);
                }
                return Err(Closed);
            }
        };
        self.seq += encoded.len() as u64;
        self.followers.retain(|f| {
            for (_, line) in &lines {
                match f.offer(line) {
                    Put::Done => {}
                    Put::Full => {
                        f.resync(self.seq);
                        return false;
                    }
                    Put::Gone => return false,
                }
            }
            true
        });
        self.recent_bytes += encoded.iter().map(|(_, j)| j.len()).sum::<usize>();
        self.recent.extend(encoded);
        while self.recent.len() > RECENT || self.recent_bytes > limits::PAGE {
            let Some((seq, json)) = self.recent.pop_front() else {
                break;
            };
            self.recent_bytes -= json.len();
            self.floor = seq;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use axiom_journal::Journal;
    use axiom_proto::{AgentId, ItemKind, Message, RunId};

    use super::*;
    use crate::outbox::Outbox;

    struct Bench {
        a: Actor,
        out: Arc<Outbox>,
        f: Follower,
        path: std::path::PathBuf,
        _dir: tempfile::TempDir,
    }
    impl Bench {
        async fn new(limit: usize) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("j");
            let store = Store::start(Journal::open(&path).unwrap(), &path).unwrap();
            let id = SessionId::new();
            store.create(id.clone(), "t".into(), None, 1).await.unwrap();
            let out = Arc::new(Outbox::new(limit));
            let f = Follower::begin(&out, &id);
            let mut a = Actor::new(id, 1, store, Arc::default(), Arc::default());
            assert!(matches!(a.join(f.clone(), 1), Joined::Yes));
            Self { a, out, f, path, _dir: dir }
        }
        async fn say(&mut self, s: Vec<Say>) {
            let (_tx, mut none) = mpsc::channel(1);
            self.a.handle(Cmd::Say(s), &mut none).await.unwrap();
        }
        /// What the follower has been sent since it last looked.
        async fn seen(&self) -> Vec<Message> {
            let Ok(b) = tokio::time::timeout(Duration::ZERO, self.out.take()).await else {
                return vec![];
            };
            let text = String::from_utf8(b.unwrap_or_default()).unwrap();
            text.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
        }
        async fn bodies(&self) -> Vec<Body> {
            let all = self.seen().await.into_iter().map(|m| match m {
                Message::Events { events, .. } => events,
                m => panic!("{m:?}"),
            });
            all.flatten().map(|e| e.body).collect()
        }
    }
    fn id(i: &str) -> ItemId {
        ItemId(i.into())
    }
    fn start(i: &str) -> Say {
        Say::Event(Body::ItemStarted {
            item: id(i),
            run: RunId("r".into()),
            agent: AgentId("a".into()),
            kind: ItemKind::Prose,
            name: String::new(),
            args: None,
            args_cut: 0,
        })
    }
    fn text(i: &str, t: impl Into<String>) -> Say {
        Say::Text(id(i), t.into())
    }
    fn end(i: &str) -> Say {
        Say::End(id(i), axiom_proto::Outcome::Completed, None)
    }
    fn delta(i: &str, t: &str) -> Body {
        Body::ItemDelta { item: id(i), text: t.into() }
    }

    #[tokio::test]
    async fn text_waits_to_be_joined_but_not_for_long() {
        let mut b = Bench::new(1 << 20).await;
        b.say(vec![start("p")]).await;
        b.bodies().await;
        let before = Instant::now();
        b.say(vec![text("p", "a")]).await;
        b.say(vec![text("p", "b")]).await;
        let wait = b.a.due.unwrap().duration_since(before);
        assert!(wait >= COALESCE && wait < COALESCE + Duration::from_millis(20), "{wait:?}");
        assert!(COALESCE >= Duration::from_millis(16) && COALESCE <= Duration::from_millis(33));
        assert_eq!(b.bodies().await, [], "not yet");
        tokio::time::sleep_until(b.a.due.unwrap()).await;
        b.a.flush(Vec::new()).await.unwrap();
        assert_eq!(b.bodies().await, [delta("p", "ab")]);
        assert_eq!(b.a.due, None);
    }

    #[tokio::test]
    async fn much_text_does_not_wait() {
        let mut b = Bench::new(1 << 20).await;
        b.say(vec![start("p")]).await;
        b.bodies().await;
        let half = "x".repeat(FLUSH_BYTES / 2 + 1);
        b.say(vec![text("p", half.as_str())]).await;
        assert_eq!(b.bodies().await, []);
        b.say(vec![text("p", half.as_str())]).await;
        assert_eq!(b.bodies().await, [delta("p", &half.repeat(2))]);
    }

    #[tokio::test]
    async fn what_was_said_first_is_written_first() {
        let mut b = Bench::new(1 << 20).await;
        b.say(vec![start("p"), start("t"), text("p", "before"), text("t", "own"), end("t")]).await;
        let seen = b.bodies().await;
        assert_eq!(seen[2], delta("p", "before"));
        assert!(
            matches!(&seen[3], Body::ItemEnded { item, text, .. } if item.0 == "t" && text == "own")
        );
        assert_eq!(seen.len(), 4, "an ended item needs no last delta");
    }

    #[tokio::test]
    async fn the_text_of_an_item_is_bounded_as_it_is_encoded_and_has_no_gaps() {
        for unit in ["x", "\0", "é"] {
            let mut b = Bench::new(64 << 20).await;
            b.say(vec![start("p")]).await;
            let part = unit.repeat(100_000);
            let mut said = String::new();
            for _ in 0..12 {
                b.say(vec![text("p", part.as_str())]).await;
                said.push_str(&part);
            }
            // Room for one byte is not used after something was dropped.
            b.say(vec![text("p", "y")]).await;
            said.push('y');
            b.a.flush(Vec::new()).await.unwrap();
            b.say(vec![end("p")]).await;
            let seen = b.bodies().await;
            let shown: String = seen
                .iter()
                .filter_map(|b| match b {
                    Body::ItemDelta { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            let Some(Body::ItemEnded { text, .. }) = seen.last() else { panic!() };
            assert!(limits::text_len(text) <= limits::TEXT);
            let kept = text.split("\n[").next().unwrap();
            assert!(said.starts_with(kept) && kept.len() > 100_000);
            assert_eq!(text[kept.len()..], limits::cut_note(said.len() - kept.len()));
            assert!(shown == format!("{kept}{CUT}"), "a crash leaves the mark in the deltas");
        }
    }

    #[tokio::test]
    async fn one_flush_is_many_lines_if_it_must_be() {
        let mut b = Bench::new(64 << 20).await;
        let mut all = Vec::new();
        for n in 0..20 {
            let i = format!("i{n}");
            all.extend([start(&i), text(&i, "\0".repeat(170_000)), end(&i)]);
        }
        b.say(all).await;
        let seen = b.seen().await;
        assert!(seen.len() >= 20);
        for m in &seen {
            assert!(limits::encoded_len(m) < limits::PAGE + limits::TEXT + 1024);
        }
        assert_eq!(b.a.seq, 1 + 40);
        assert!(b.a.recent_bytes <= limits::PAGE);
        assert_eq!(b.a.recent_bytes, b.a.recent.iter().map(|(_, j)| j.len()).sum::<usize>());
    }

    #[tokio::test]
    async fn a_follower_joins_from_what_is_recent_or_is_sent_to_the_journal() {
        let mut b = Bench::new(1 << 20).await;
        for n in 0..RECENT + 5 {
            b.say(vec![start(&format!("i{n}"))]).await;
        }
        assert_eq!((b.a.floor, b.a.recent.len()), (6, RECENT));
        let next = |b: &Bench| Follower::begin(&b.out, &b.a.id);
        let f = next(&b);
        assert!(!b.seen().await.is_empty());
        assert!(matches!(b.a.join(f.clone(), 5), Joined::Behind(5)));
        assert!(matches!(b.a.join(f.clone(), b.a.seq + 1), Joined::Ahead));
        assert_eq!(b.seen().await, []);
        assert!(matches!(b.a.join(f, b.a.seq - 2), Joined::Yes));
        assert_eq!(b.bodies().await.len(), 2);
        assert_eq!(b.a.followers.len(), 1, "the one it replaced is forgotten");
        // Nor does a connection that is gone stay.
        let f = next(&b);
        b.out.abandon();
        assert!(matches!(b.a.join(f, 6), Joined::Yes));
        b.say(vec![start("last")]).await;
        assert_eq!(b.a.followers.len(), 0);
    }

    #[tokio::test]
    async fn one_that_falls_behind_is_told_once_and_dropped() {
        let mut b = Bench::new(2000).await;
        for n in 0..20 {
            b.say(vec![start(&format!("i{n}"))]).await;
        }
        assert_eq!(b.a.followers.len(), 0);
        assert!(!b.f.current());
        let seen = b.seen().await;
        let resyncs = seen.iter().filter(|m| matches!(m, Message::Resync { .. })).count();
        assert_eq!(resyncs, 1);
        assert!(matches!(seen.last(), Some(Message::Resync { resync: 1.., .. })));
    }

    #[tokio::test]
    async fn a_session_the_journal_refuses_ends_and_says_why() {
        let b = Bench::new(1 << 20).await;
        // Another writer takes the next number.
        let e = Event { seq: 2, at: 1, body: Body::SessionTitled { title: "u".into() } };
        Journal::open(&b.path).unwrap().append(&b.a.id, &[e]).unwrap();
        let faults = b.a.faults.clone();
        let (tx, rx) = mpsc::channel(4);
        let s = Session { id: b.a.id.clone(), tx };
        let task = tokio::spawn(b.a.run(rx));
        s.say(vec![start("p")]).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), task).await.unwrap().unwrap();
        assert_eq!(s.say(vec![start("q")]).await, Err(Closed.into()));
        assert_eq!(s.sync().await, Err(Closed));
        assert!(matches!(s.join(b.f.clone(), 1).await, Err(Closed)));
        let words = &faults.list()[0].words;
        let sent = tokio::time::timeout(Duration::from_secs(5), b.out.take()).await;
        let line = String::from_utf8(sent.unwrap().unwrap()).unwrap();
        let want = serde_json::json!({ "session": s.id, "fault": words });
        assert_eq!(serde_json::from_str::<serde_json::Value>(&line).unwrap(), want);
    }

    #[tokio::test]
    async fn a_session_nobody_holds_ends_with_its_text_written() {
        let b = Bench::new(1 << 20).await;
        let (id, store) = (b.a.id.clone(), b.a.store.clone());
        let (s, opened) =
            Session::open(id.clone(), 1, store.clone(), Arc::default(), Arc::default());
        let again = opened.session().unwrap();
        s.say(vec![start("p"), text("p", "pending")]).await.unwrap();
        drop(s);
        tokio::task::yield_now().await;
        assert!(opened.session().is_some() && !opened.ended(), "someone holds it");
        drop(again);
        assert!(opened.session().is_none());
        tokio::time::timeout(Duration::from_secs(5), opened.end()).await.unwrap();
        let last = store.read(id, 2).await.unwrap();
        assert_eq!(last.into_iter().map(|e| e.body).collect::<Vec<_>>(), [delta("p", "pending")]);
    }

    #[tokio::test]
    async fn a_burst_shares_its_commits() {
        let mut b = Bench::new(1 << 20).await;
        let (tx, mut rx) = mpsc::channel(256);
        for n in 1..100 {
            tx.try_send(Cmd::Say(vec![start(&format!("i{n}"))])).ok().unwrap();
        }
        let (done, synced) = oneshot::channel();
        tx.try_send(Cmd::Sync(done)).ok().unwrap();
        let first = Cmd::Say(vec![start("i0")]);
        b.a.handle(first, &mut rx).await.unwrap();
        assert_eq!(b.seen().await.len(), 1, "one commit, one message");
        assert_eq!(b.a.seq, 1 + TOGETHER as u64);
        let next = rx.try_recv().ok().unwrap();
        b.a.handle(next, &mut rx).await.unwrap();
        assert_eq!(synced.await, Ok(101), "and what waited behind it is done after it");
    }

    #[tokio::test]
    async fn a_request_that_cannot_be_shown_whole_is_refused_with_all_that_came_with_it() {
        let b = Bench::new(1 << 20).await;
        let (s, _opened) =
            Session::open(b.a.id.clone(), 1, b.a.store.clone(), Arc::default(), Arc::default());
        let ask = |n, answerable| {
            Say::Event(Body::RequestOpened {
                request: axiom_proto::RequestId::new(),
                agent: AgentId("a".into()),
                what: "Run".into(),
                subject: format!("ls{}; rm -rf ~", " ".repeat(n)),
                scope: String::new(),
                answerable,
                digest: String::new(),
                item: None,
                session: false,
            })
        };
        use axiom_proto::Answerable::{Live, No};
        let r = s.say(vec![start("p"), ask(70_000, Live)]).await;
        assert!(matches!(r, Err(Refused::TooLong(_))), "{r:?}");
        assert_eq!(s.sync().await, Ok(1), "nothing of it is journalled");
        // One that nobody can answer is only told.
        s.say(vec![ask(70_000, No)]).await.unwrap();
        s.say(vec![ask(60_000, Live)]).await.unwrap();
        assert_eq!(s.sync().await, Ok(3));
        let subjects: Vec<_> = b.a.store.read(s.id.clone(), 1).await.unwrap();
        let ends: Vec<_> = subjects
            .iter()
            .map(|e| match &e.body {
                Body::RequestOpened { subject, .. } => subject.ends_with("rm -rf ~"),
                b => panic!("{b:?}"),
            })
            .collect();
        assert_eq!(ends, [false, true]);
    }
}
