//! The journal on two threads: one writes, one reads. Callers wait for the commit, or the page.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use axiom_journal::{Error, Journal, Reader, Result};
use axiom_proto::{AgentId, Event, SessionId, SessionInfo, limits};
use tokio::sync::{mpsc, oneshot};

/// An append waits with others and shares their commit.
struct Append {
    id: SessionId,
    events: Vec<Event>,
    done: oneshot::Sender<Result<()>>,
}
type Write = Box<dyn FnOnce(&mut Journal, &mut Outage) + Send>;
enum Job {
    Append(Append),
    Other(Write),
}
type Read = Box<dyn FnOnce(&Reader) + Send>;

/// Most appends in one commit: bounds how long the first one waits.
const GROUP: usize = 64;
/// How long the journal may be locked by another process before what wants to be written fails.
/// Sessions wait meanwhile, and say nothing.
const PATIENCE: Duration = Duration::from_secs(10);
/// How often a write the system failed is tried again: seldom of use, so soon over.
const TRIES: u32 = 3;

#[derive(Clone)]
pub struct Store {
    write: mpsc::Sender<Job>,
    read: mpsc::Sender<Read>,
}

impl Store {
    pub fn start(mut j: Journal, path: &Path) -> Result<Self> {
        let reader = Reader::open(path)?;
        j.hurry()?;
        let (write, mut rx) = mpsc::channel::<Job>(256);
        std::thread::Builder::new().name("journal".into()).spawn(move || {
            let mut outage = Outage::new(PATIENCE);
            while let Some(job) = rx.blocking_recv() {
                let (group, other) = gather(job, &mut rx);
                commit(&mut j, &mut outage, group);
                if let Some(f) = other {
                    f(&mut j, &mut outage);
                }
            }
        })?;
        let (read, mut rx) = mpsc::channel::<Read>(256);
        std::thread::Builder::new().name("journal reads".into()).spawn(move || {
            while let Some(f) = rx.blocking_recv() {
                f(&reader);
            }
        })?;
        Ok(Self { write, read })
    }

    async fn write<T: Send + 'static>(
        &self,
        f: impl Fn(&mut Journal) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        let job: Write = Box::new(move |j, outage| {
            let _ = tx.send(outage.over(|| f(j)));
        });
        self.write.send(Job::Other(job)).await.map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())?
    }

    async fn read_with<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Reader) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        // Once: whoever reads next does not wait for what this one could not read.
        let job: Read = Box::new(move |r| {
            let _ = tx.send(f(r));
        });
        self.read.send(job).await.map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())?
    }

    /// Answers once the events are committed.
    pub async fn append(&self, id: SessionId, events: Vec<Event>) -> Result<()> {
        let (done, rx) = oneshot::channel();
        let job = Job::Append(Append { id, events, done });
        self.write.send(job).await.map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())?
    }
    pub async fn create(
        &self,
        id: SessionId,
        title: String,
        workspace: Option<String>,
        at: u64,
    ) -> Result<()> {
        self.write(move |j| j.create(&id, &title, workspace.as_deref(), at).map(drop)).await
    }
    /// Where a session's history ends, after ending what its last writer left open. The caller
    /// knows that it has no writer now.
    pub async fn open(&self, id: SessionId, at: u64) -> Result<Option<u64>> {
        let s = id.clone();
        let Some((mut last, events)) = self.read_with(move |r| r.ending(&s, at)).await? else {
            return Ok(None);
        };
        if let Some(e) = events.last() {
            last = e.seq;
            self.append(id, events).await?;
        }
        Ok(Some(last))
    }
    /// A page of history.
    pub async fn read(&self, id: SessionId, after: u64) -> Result<Vec<Event>> {
        self.read_with(move |r| r.read(&id, after, limits::PAGE)).await
    }
    pub async fn sessions(&self, limit: u32, offset: u32) -> Result<Vec<SessionInfo>> {
        self.read_with(move |r| r.sessions(limit, offset)).await
    }
    pub async fn session(&self, id: SessionId) -> Result<Option<SessionInfo>> {
        self.read_with(move |r| r.session(&id)).await
    }
    pub async fn put_agent(&self, id: SessionId, agent: AgentId, setup: String) -> Result<()> {
        self.write(move |j| j.put_agent(&id, &agent, &setup)).await
    }
    pub async fn agents(&self, id: SessionId) -> Result<Vec<(AgentId, String)>> {
        self.read_with(move |r| r.agents(&id)).await
    }
    pub async fn put_message(
        &self,
        id: SessionId,
        agent: AgentId,
        n: u64,
        body: String,
    ) -> Result<()> {
        self.write(move |j| j.put_message(&id, &agent, n, &body)).await
    }
    pub async fn messages(&self, id: SessionId, agent: AgentId) -> Result<Vec<String>> {
        self.read_with(move |r| r.messages(&id, &agent)).await
    }
    pub async fn put_audit(&self, seq: u64, body: String) -> Result<()> {
        self.write(move |j| j.put_audit(seq, &body)).await
    }
    pub async fn last_audit(&self) -> Result<Option<String>> {
        self.write(|j| j.last_audit()).await
    }
    pub async fn audit(&self, after: u64, most: u32) -> Result<Vec<String>> {
        self.read_with(move |r| r.audit(after, most)).await
    }
}

fn stopped() -> Error {
    Error::Unavailable("journal: its thread has stopped".into())
}

/// The appends that share the next commit, and what comes after them.
fn gather(first: Job, rx: &mut mpsc::Receiver<Job>) -> (Vec<Append>, Option<Write>) {
    let mut group = Vec::new();
    let mut next = Some(first);
    while let Some(job) = next {
        match job {
            Job::Append(a) => group.push(a),
            Job::Other(f) => return (group, Some(f)),
        }
        next = if group.len() < GROUP { rx.try_recv().ok() } else { None };
    }
    (group, None)
}

/// Nobody is answered before the commit that holds their events.
fn commit(j: &mut Journal, outage: &mut Outage, group: Vec<Append>) {
    if group.is_empty() {
        return;
    }
    let parts: Vec<_> = group.iter().map(|a| (&a.id, a.events.as_slice())).collect();
    let each = match outage.over(|| j.append_group(&parts)) {
        Ok(each) => each,
        Err(e) if e.busy() => {
            let text = e.to_string();
            group.iter().map(|_| Err(Error::Unavailable(text.clone()))).collect()
        }
        // Something in the group cannot be written. Alone, it costs nobody else theirs.
        Err(_) => {
            group.iter().map(|a| outage.over(|| again(|| j.append(&a.id, &a.events)))).collect()
        }
    };
    for (a, r) in group.into_iter().zip(each) {
        let _ = a.done.send(r);
    }
}

/// Tries a few times what the system failed.
fn again(mut f: impl FnMut() -> Result<()>) -> Result<()> {
    let mut r = f();
    for n in 1..TRIES {
        match &r {
            Err(e) if e.transient() && !e.busy() => {
                std::thread::sleep(Duration::from_millis(20 << n));
                r = f();
            }
            _ => break,
        }
    }
    r
}

/// A time in which another process holds the journal. All who want to write wait for its end
/// together: there is one limit, not one for each, and after it everybody is told at once.
struct Outage {
    patience: Duration,
    /// When it is too long, and when it was last met: one not met for as long is another one.
    until: Option<(Instant, Instant)>,
}

impl Outage {
    fn new(patience: Duration) -> Self {
        Self { patience, until: None }
    }
    fn over<T>(&mut self, mut f: impl FnMut() -> Result<T>) -> Result<T> {
        let mut pause = Duration::from_millis(1);
        loop {
            let r = f();
            if !r.as_ref().is_err_and(Error::busy) {
                self.until = None;
                return r;
            }
            let now = Instant::now();
            let until = match self.until {
                Some((until, met)) if now < met + self.patience => until,
                _ => now + self.patience,
            };
            self.until = Some((until, now));
            if now >= until {
                return r;
            }
            std::thread::sleep(pause.min(until - now));
            pause = (pause * 2).min(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use axiom_proto::Body;

    use super::*;

    fn titled(seq: u64) -> Vec<Event> {
        vec![Event { seq, at: 1, body: Body::SessionTitled { title: "x".into() } }]
    }
    fn job(id: &SessionId, events: Vec<Event>) -> (Append, oneshot::Receiver<Result<()>>) {
        let (done, rx) = oneshot::channel();
        (Append { id: id.clone(), events, done }, rx)
    }

    #[test]
    fn a_commit_takes_a_bounded_number_and_stops_at_other_work() {
        let (tx, mut rx) = mpsc::channel(256);
        let s = SessionId::new();
        for _ in 0..GROUP + 10 {
            tx.try_send(Job::Append(job(&s, vec![]).0)).ok().unwrap();
        }
        tx.try_send(Job::Other(Box::new(|_, _| {}))).ok().unwrap();
        tx.try_send(Job::Append(job(&s, vec![]).0)).ok().unwrap();
        for (n, other) in [(GROUP, false), (10, true), (1, false)] {
            let first = rx.try_recv().unwrap();
            let (group, f) = gather(first, &mut rx);
            assert_eq!((group.len(), f.is_some()), (n, other));
        }
    }

    #[test]
    fn one_that_cannot_be_written_costs_the_others_nothing() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("j");
        let mut j = Journal::open(&p).unwrap();
        let ids: Vec<_> = (0..3).map(|_| SessionId::new()).collect();
        for s in &ids {
            j.create(s, "t", None, 1).unwrap();
        }
        // As a full disk does, it makes SQLite give up the whole transaction.
        rusqlite::Connection::open(&p)
            .unwrap()
            .execute_batch(&format!(
                "create trigger full before insert on event when new.session='{}'
                 begin select raise(rollback, 'full'); end",
                ids[1]
            ))
            .unwrap();
        let (jobs, answers): (Vec<_>, Vec<_>) = ids.iter().map(|s| job(s, titled(2))).unzip();
        commit(&mut j, &mut Outage::new(PATIENCE), jobs);
        let ok: Vec<_> = answers.into_iter().map(|mut a| a.try_recv().unwrap().is_ok()).collect();
        assert_eq!(ok, [true, false, true]);
        for (s, seq) in ids.iter().zip([2, 1, 2]) {
            assert_eq!(j.session(s).unwrap().unwrap().last_seq, seq, "as they were told");
        }
    }

    #[tokio::test]
    async fn a_lock_held_for_a_while_is_waited_for() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("j");
        let store = Store::start(Journal::open(&p).unwrap(), &p).unwrap();
        let s = SessionId::new();
        store.create(s.clone(), "t".into(), None, 1).await.unwrap();
        let other = rusqlite::Connection::open(&p).unwrap();
        other.execute_batch("begin immediate").unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1600));
            drop(other);
        });
        store.append(s.clone(), titled(2)).await.unwrap();
        assert_eq!(store.open(s, 1).await.unwrap(), Some(2));
    }

    #[tokio::test]
    async fn reading_does_not_wait_for_writing() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("j");
        let store = Store::start(Journal::open(&p).unwrap(), &p).unwrap();
        let s = SessionId::new();
        store.create(s.clone(), "t".into(), None, 1).await.unwrap();
        let (go, wait) = std::sync::mpsc::channel::<()>();
        let wait = std::sync::Mutex::new(wait);
        let writer = store.clone();
        let busy = tokio::spawn(async move {
            writer.write(move |_| wait.lock().unwrap().recv().map_err(|_| stopped())).await
        });
        tokio::task::yield_now().await;
        let append = store.append(s.clone(), titled(2));
        tokio::pin!(append);
        let soon = Duration::from_millis(50);
        assert!(tokio::time::timeout(soon, &mut append).await.is_err());
        assert_eq!(store.read(s.clone(), 0).await.unwrap().len(), 1);
        assert_eq!(store.sessions(9, 0).await.unwrap().len(), 1);
        go.send(()).unwrap();
        busy.await.unwrap().unwrap();
        append.await.unwrap();
        assert_eq!(store.read(s, 0).await.unwrap().len(), 2);
    }

    #[test]
    fn an_outage_has_one_limit_for_everybody() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("j");
        let mut j = Journal::open(&p).unwrap();
        j.hurry().unwrap();
        let ids: Vec<_> = (0..20).map(|_| SessionId::new()).collect();
        for s in &ids {
            j.create(s, "t", None, 1).unwrap();
        }
        let lock = rusqlite::Connection::open(&p).unwrap();
        lock.execute_batch("begin immediate").unwrap();
        let mut outage = Outage::new(Duration::from_millis(300));
        let t = Instant::now();
        for (n, s) in ids.iter().enumerate() {
            let (one, mut answer) = job(s, titled(2));
            commit(&mut j, &mut outage, vec![one]);
            let e = answer.try_recv().unwrap().unwrap_err().to_string();
            assert_eq!(e, "journal: database is locked");
            if n == 0 {
                assert!(t.elapsed() >= Duration::from_millis(300));
            }
        }
        assert!(t.elapsed() < Duration::from_millis(600), "{:?}", t.elapsed());
        lock.execute_batch("rollback").unwrap();
        let (one, mut answer) = job(&ids[0], titled(2));
        commit(&mut j, &mut outage, vec![one]);
        answer.try_recv().unwrap().unwrap();
        // The next outage is another one, whether something was written in between or not.
        lock.execute_batch("begin immediate").unwrap();
        for _ in 0..2 {
            let t = Instant::now();
            let (one, mut answer) = job(&ids[0], titled(3));
            commit(&mut j, &mut outage, vec![one]);
            assert!(answer.try_recv().unwrap().is_err());
            assert!(t.elapsed() >= Duration::from_millis(300));
            std::thread::sleep(Duration::from_millis(350));
        }
    }

    #[tokio::test]
    async fn a_session_is_opened_without_waiting_for_the_writer() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("j");
        let store = Store::start(Journal::open(&p).unwrap(), &p).unwrap();
        let s = SessionId::new();
        store.create(s.clone(), "t".into(), None, 1).await.unwrap();
        let lock = rusqlite::Connection::open(&p).unwrap();
        lock.execute_batch("begin immediate").unwrap();
        let append = store.append(s.clone(), titled(2));
        tokio::pin!(append);
        assert!(tokio::time::timeout(Duration::from_millis(50), &mut append).await.is_err());
        // Nothing is open in it, so nothing is written to open it.
        let open = tokio::time::timeout(Duration::from_secs(1), store.open(s, 1)).await;
        assert_eq!(open.unwrap().unwrap(), Some(1));
        lock.execute_batch("rollback").unwrap();
        append.await.unwrap();
    }

    #[test]
    fn what_the_system_fails_is_tried_a_few_times() {
        use rusqlite::{Error as Sql, ffi};
        let io = || Error::Sql(Sql::SqliteFailure(ffi::Error::new(ffi::SQLITE_IOERR), None));
        let mut tries = 0;
        let t = Instant::now();
        let r = again(|| {
            tries += 1;
            Err(io())
        });
        assert!(r.is_err() && t.elapsed() < Duration::from_secs(1));
        assert_eq!(tries, TRIES);
        let mut tries = 0;
        again(|| {
            tries += 1;
            if tries == 1 { Err(io()) } else { Ok(()) }
        })
        .unwrap();
        // What will not pass is not tried again, and is nobody's outage.
        let mut tries = 0;
        let mut outage = Outage::new(PATIENCE);
        let t = Instant::now();
        let r: Result<()> = outage.over(|| {
            tries += 1;
            Err(Error::Damaged("d".into()))
        });
        assert!(r.is_err() && tries == 1 && t.elapsed() < Duration::from_secs(1));
    }
}
