//! The journal on two threads: one writes, one reads. Callers wait for the commit, or the page.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use axiom_journal::{Error, Journal, Reader, Result};
use axiom_proto::{Event, SessionId, SessionInfo, limits};
use tokio::sync::{mpsc, oneshot};

/// An append waits with others and shares their commit.
struct Append {
    id: SessionId,
    events: Vec<Event>,
    done: oneshot::Sender<Result<()>>,
}
type Write = Box<dyn FnOnce(&mut Journal) + Send>;
enum Job {
    Append(Append),
    Other(Write),
}
type Read = Box<dyn FnOnce(&Reader) + Send>;

/// Most appends in one commit: bounds how long the first one waits.
const GROUP: usize = 64;
/// How long an error that may pass is tried again. Sessions wait meanwhile, and say nothing.
const PATIENCE: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct Store {
    write: mpsc::Sender<Job>,
    read: mpsc::Sender<Read>,
}

impl Store {
    pub fn start(mut j: Journal, path: &Path) -> Result<Self> {
        let reader = Reader::open(path)?;
        let (write, mut rx) = mpsc::channel::<Job>(256);
        std::thread::Builder::new().name("journal".into()).spawn(move || {
            while let Some(job) = rx.blocking_recv() {
                let (group, other) = gather(job, &mut rx);
                commit(&mut j, group);
                if let Some(f) = other {
                    f(&mut j);
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
        let job: Write = Box::new(move |j| {
            let _ = tx.send(patiently(|| f(j)));
        });
        self.write.send(Job::Other(job)).await.map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())?
    }

    async fn read_with<T: Send + 'static>(
        &self,
        f: impl Fn(&Reader) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        let job: Read = Box::new(move |r| {
            let _ = tx.send(patiently(|| f(r)));
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
        self.write(move |j| j.recover_session(&id, at)).await
    }
    /// A page of history.
    pub async fn read(&self, id: SessionId, after: u64) -> Result<Vec<Event>> {
        self.read_with(move |r| r.read(&id, after, limits::PAGE)).await
    }
    pub async fn sessions(&self, limit: u32, offset: u32) -> Result<Vec<SessionInfo>> {
        self.read_with(move |r| r.sessions(limit, offset)).await
    }
}

fn stopped() -> Error {
    Error::Unavailable("its thread has stopped".into())
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
fn commit(j: &mut Journal, group: Vec<Append>) {
    if group.is_empty() {
        return;
    }
    let parts: Vec<_> = group.iter().map(|a| (&a.id, a.events.as_slice())).collect();
    let each = match patiently(|| j.append_group(&parts)) {
        Ok(each) => each,
        Err(e) if e.busy() || group.len() == 1 => {
            let text = e.to_string();
            group.iter().map(|_| Err(Error::Unavailable(text.clone()))).collect()
        }
        // Something in the group cannot be written. Alone, it costs nobody else theirs.
        Err(_) => group.iter().map(|a| j.append(&a.id, &a.events)).collect(),
    };
    for (a, r) in group.into_iter().zip(each) {
        let _ = a.done.send(r);
    }
}

/// Tries again, within limits, what may succeed if tried again.
fn patiently<T>(mut f: impl FnMut() -> Result<T>) -> Result<T> {
    let (until, mut pause) = (Instant::now() + PATIENCE, Duration::from_millis(20));
    loop {
        match f() {
            Err(e) if e.transient() && Instant::now() < until => {
                std::thread::sleep(pause);
                pause = (pause * 2).min(Duration::from_millis(500));
            }
            r => return r,
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
        tx.try_send(Job::Other(Box::new(|_| {}))).ok().unwrap();
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
        commit(&mut j, jobs);
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
        // Longer than SQLite itself waits.
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
}
