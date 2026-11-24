//! The journal on its own thread. Callers wait for the commit.

use axiom_journal::{Durability, Error, Journal, Result};
use axiom_proto::{Event, SessionId, SessionInfo};
use tokio::sync::{mpsc, oneshot};

/// An append waits with others and shares their commit.
struct Append {
    id: SessionId,
    events: Vec<Event>,
    how: Durability,
    done: oneshot::Sender<Result<Vec<Event>>>,
}
enum Job {
    Append(Append),
    Other(Box<dyn FnOnce(&mut Journal) + Send>),
}

/// Most appends in one commit: bounds how long the first one waits.
const GROUP: usize = 64;

#[derive(Clone)]
pub struct Store(mpsc::Sender<Job>);

impl Store {
    pub fn start(mut j: Journal) -> std::io::Result<Self> {
        let (tx, mut rx) = mpsc::channel::<Job>(256);
        std::thread::Builder::new().name("journal".into()).spawn(move || {
            while let Some(job) = rx.blocking_recv() {
                let mut group = Vec::new();
                let mut other = None;
                match job {
                    Job::Append(a) => group.push(a),
                    Job::Other(f) => other = Some(f),
                }
                while other.is_none() && group.len() < GROUP {
                    match rx.try_recv() {
                        Ok(Job::Append(a)) => group.push(a),
                        Ok(Job::Other(f)) => other = Some(f),
                        Err(_) => break,
                    }
                }
                commit(&mut j, group);
                if let Some(f) = other {
                    f(&mut j);
                }
            }
        })?;
        Ok(Self(tx))
    }

    async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Journal) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let gone = || Error::Damaged("the journal thread has stopped".into());
        let (tx, rx) = oneshot::channel();
        let job = Job::Other(Box::new(move |j| {
            let _ = tx.send(f(j));
        }));
        self.0.send(job).await.map_err(|_| gone())?;
        rx.await.map_err(|_| gone())?
    }

    pub async fn append(
        &self,
        id: SessionId,
        events: Vec<Event>,
        how: Durability,
    ) -> Result<Vec<Event>> {
        let gone = || Error::Damaged("the journal thread has stopped".into());
        let (done, rx) = oneshot::channel();
        let job = Job::Append(Append { id, events, how, done });
        self.0.send(job).await.map_err(|_| gone())?;
        rx.await.map_err(|_| gone())?
    }
    pub async fn create(
        &self,
        id: SessionId,
        title: String,
        workspace: Option<String>,
        at: u64,
    ) -> Result<()> {
        self.run(move |j| j.create(&id, &title, workspace.as_deref(), at).map(drop)).await
    }
    pub async fn read(&self, id: SessionId, after: u64, limit: usize) -> Result<Vec<Event>> {
        self.run(move |j| j.read(&id, after, limit)).await
    }
    pub async fn sessions(&self) -> Result<Vec<SessionInfo>> {
        self.run(|j| j.sessions()).await
    }
    pub async fn session(&self, id: SessionId) -> Result<Option<SessionInfo>> {
        self.run(move |j| j.session(&id)).await
    }
}

/// Nobody is answered before the commit that holds their events.
fn commit(j: &mut Journal, group: Vec<Append>) {
    if group.is_empty() {
        return;
    }
    let how = if group.iter().any(|a| a.how == Durability::Intent) {
        Durability::Intent
    } else {
        Durability::Stream
    };
    let r =
        j.batch(how, |j| group.iter().map(|a| j.append_in(&a.id, &a.events)).collect::<Vec<_>>());
    match r {
        Ok(each) => {
            for (a, r) in group.into_iter().zip(each) {
                let _ = a.done.send(r.map(|()| a.events));
            }
        }
        Err(e) => {
            let text = e.to_string();
            for a in group {
                let _ = a.done.send(Err(Error::Damaged(text.clone())));
            }
        }
    }
}
