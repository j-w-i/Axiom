//! What waits for a person, in all sessions: the questions that are open and can be answered. Kept by
//! the sessions as they write, so that it is never what was not written, and gone with whoever asked.

use std::{
    collections::BTreeMap,
    sync::{Mutex, MutexGuard, PoisonError},
};

use axiom_proto::{AgentId, Answerable, Body, Event, RequestId, SessionId, Waits, limits};
use tokio::sync::watch;

/// Of what is asked about, what is listed. Whoever answers sees all of it where it is asked.
const SHOWN: usize = 300;

struct One {
    session: SessionId,
    agent: AgentId,
    what: String,
    subject: String,
    at: u64,
}

pub struct Waiting {
    /// By when they were asked.
    all: Mutex<BTreeMap<(u64, RequestId), One>>,
    changed: watch::Sender<usize>,
}
impl Default for Waiting {
    fn default() -> Self {
        Self { all: Mutex::default(), changed: watch::Sender::new(0) }
    }
}

impl Waiting {
    fn lock(&self) -> MutexGuard<'_, BTreeMap<(u64, RequestId), One>> {
        self.all.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn said(&self, all: &BTreeMap<(u64, RequestId), One>) {
        let n = all.len();
        // Also one for another: how many there are is not all that is asked after.
        let _ = self.changed.send_replace(n);
    }

    /// What a session has written of questions.
    pub fn note(&self, session: &SessionId, events: &[Event]) {
        if events.is_empty() {
            return;
        }
        let mut all = self.lock();
        let mut changed = false;
        for e in events {
            match &e.body {
                Body::RequestOpened {
                    request,
                    agent,
                    what,
                    subject,
                    answerable: Answerable::Live,
                    ..
                } => {
                    let mut subject = subject.lines().next().unwrap_or_default().to_owned();
                    limits::clip(&mut subject, SHOWN);
                    let one = One {
                        session: session.clone(),
                        agent: agent.clone(),
                        what: what.clone(),
                        subject,
                        at: e.at,
                    };
                    all.insert((e.at, request.clone()), one);
                    changed = true;
                }
                Body::RequestClosed { request, .. } => {
                    let n = all.len();
                    all.retain(|k, o| k.1 != *request || o.session != *session);
                    changed |= all.len() != n;
                }
                _ => {}
            }
        }
        if changed {
            self.said(&all);
        }
    }

    /// Nobody of this session waits any more.
    pub fn over(&self, session: &SessionId) {
        let mut all = self.lock();
        let n = all.len();
        all.retain(|_, o| o.session != *session);
        if all.len() != n {
            self.said(&all);
        }
    }

    /// Told of each change, how many there are.
    pub fn watch(&self) -> watch::Receiver<usize> {
        self.changed.subscribe()
    }

    /// The longest first. Names are the caller's to add.
    pub fn list(&self) -> Vec<Waits> {
        let one = |(k, o): (&(u64, RequestId), &One)| Waits {
            session: o.session.clone(),
            title: String::new(),
            agent: o.agent.clone(),
            name: String::new(),
            request: k.1.clone(),
            what: o.what.clone(),
            subject: o.subject.clone(),
            at: o.at,
        };
        self.lock().iter().map(one).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened(seq: u64, r: &str, answerable: Answerable) -> Event {
        let body = Body::RequestOpened {
            request: RequestId(r.into()),
            agent: AgentId("a".into()),
            what: "asks".into(),
            subject: format!("first {r}\nsecond"),
            scope: String::new(),
            answerable,
            digest: String::new(),
            item: None,
            session: false,
        };
        Event { seq, at: seq, body }
    }
    fn closed(seq: u64, r: &str) -> Event {
        let body = Body::RequestClosed { request: RequestId(r.into()), answer: "denied".into() };
        Event { seq, at: seq, body }
    }
    fn asked(w: &Waiting) -> Vec<String> {
        w.list().into_iter().map(|w| format!("{} {}", w.session.0, w.request.0)).collect()
    }

    #[test]
    fn what_is_open_and_can_be_answered() {
        let w = Waiting::default();
        let mut told = w.watch();
        let (s, t) = (SessionId("s".into()), SessionId("t".into()));
        w.note(&s, &[opened(5, "q1", Answerable::Live), opened(6, "no", Answerable::No)]);
        w.note(&t, &[opened(3, "q2", Answerable::Live)]);
        assert_eq!(asked(&w), ["t q2", "s q1"], "the longest first");
        assert_eq!(w.list()[0].subject, "first q2");
        assert!(told.has_changed().unwrap());
        assert_eq!(*told.borrow_and_update(), 2);
        // Another session's question of the same name is another.
        w.note(&t, &[closed(7, "q1")]);
        assert!(!told.has_changed().unwrap());
        w.note(&s, &[closed(8, "q1"), opened(9, "q3", Answerable::Live)]);
        assert!(told.has_changed().unwrap(), "one for another is told");
        assert_eq!(asked(&w), ["t q2", "s q3"]);
        w.over(&t);
        assert_eq!(asked(&w), ["s q3"]);
        assert_eq!(*told.borrow_and_update(), 1);
        w.over(&t);
        assert!(!told.has_changed().unwrap());
        // Opened and closed at once.
        w.note(&s, &[opened(10, "q4", Answerable::Live), closed(11, "q4")]);
        assert_eq!(asked(&w), ["s q3"]);
    }
}
