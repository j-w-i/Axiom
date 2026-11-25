//! What waits for a person, in all sessions: the questions that are open and can be answered, kept by
//! the sessions as they write, so that it is never what was not written, and gone with whoever asked;
//! and what agents came to that nobody has seen, which the journal has too.

use std::{
    collections::BTreeMap,
    sync::{Mutex, MutexGuard, PoisonError},
};

use axiom_journal::Unseen;
use axiom_proto::{AgentId, Answerable, Body, Event, Need, RequestId, SessionId, Waits, limits};
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
    unseen: Mutex<BTreeMap<(SessionId, AgentId), Unseen>>,
    changed: watch::Sender<usize>,
}
impl Default for Waiting {
    fn default() -> Self {
        Self { all: Mutex::default(), unseen: Mutex::default(), changed: watch::Sender::new(0) }
    }
}

impl Waiting {
    fn lock(&self) -> MutexGuard<'_, BTreeMap<(u64, RequestId), One>> {
        self.all.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn ends(&self) -> MutexGuard<'_, BTreeMap<(SessionId, AgentId), Unseen>> {
        self.unseen.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// As the journal has it, when the host begins.
    pub fn load(&self, all: Vec<Unseen>) {
        self.ends().extend(all.into_iter().map(|u| ((u.session.clone(), u.agent.clone()), u)));
    }

    pub fn unseen(&self, u: Unseen) {
        let all = self.lock();
        self.ends().insert((u.session.clone(), u.agent.clone()), u);
        self.said(&all);
    }

    /// Whether there was something.
    pub fn seen(&self, session: &SessionId, agent: &AgentId) -> bool {
        let all = self.lock();
        let was = self.ends().remove(&(session.clone(), agent.clone())).is_some();
        if was {
            self.said(&all);
        }
        was
    }

    fn said(&self, all: &BTreeMap<(u64, RequestId), One>) {
        let n = all.len() + self.ends().len();
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
                    let subject = begins(subject);
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

    /// Who asked it.
    pub fn asked(&self, session: &SessionId, request: &RequestId) -> Option<AgentId> {
        let all = self.lock();
        let one = all.iter().find(|(k, o)| k.1 == *request && o.session == *session);
        one.map(|(_, o)| o.agent.clone())
    }

    /// Told of each change, how many there are.
    pub fn watch(&self) -> watch::Receiver<usize> {
        self.changed.subscribe()
    }

    /// The longest first. Names are the caller's to add.
    pub fn list(&self) -> Vec<Waits> {
        let one = |(k, o): (&(u64, RequestId), &One)| Waits {
            need: Need::Asks,
            session: o.session.clone(),
            title: String::new(),
            agent: o.agent.clone(),
            name: String::new(),
            request: Some(k.1.clone()),
            what: o.what.clone(),
            subject: o.subject.clone(),
            at: o.at,
        };
        let mut out: Vec<Waits> = self.lock().iter().map(one).collect();
        let mut ends: Vec<Unseen> = self.ends().values().cloned().collect();
        // What went wrong before what is only to be read.
        ends.sort_by_key(|u| (need(&u.kind) == Need::Finished, u.at));
        out.extend(ends.into_iter().map(|u| Waits {
            need: need(&u.kind),
            session: u.session,
            title: String::new(),
            agent: u.agent,
            name: String::new(),
            request: None,
            what: String::new(),
            subject: u.words,
            at: u.at,
        }));
        out
    }
}

pub const FINISHED: &str = "finished";
pub const FAILED: &str = "failed";
pub const LIMIT: &str = "limit";

fn need(kind: &str) -> Need {
    match kind {
        FINISHED => Need::Finished,
        FAILED => Need::Failed,
        LIMIT => Need::Limit,
        _ => Need::Unknown,
    }
}

/// The beginning of some words, for a list.
pub fn begins(words: &str) -> String {
    let first = words.lines().map(str::trim).find(|l| !l.is_empty());
    let mut s = first.unwrap_or_default().to_owned();
    limits::clip(&mut s, SHOWN);
    s
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
        w.list().into_iter().map(|w| format!("{} {}", w.session.0, w.request.unwrap().0)).collect()
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
        assert_eq!(w.asked(&t, &RequestId("q2".into())), Some(AgentId("a".into())));
        assert_eq!(w.asked(&s, &RequestId("q2".into())), None);
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
        // What agents came to stands after the questions, what went wrong first.
        let end = |a: &str, kind: &str, at| Unseen {
            session: s.clone(),
            agent: AgentId(a.into()),
            kind: kind.into(),
            words: "w".into(),
            at,
        };
        told.borrow_and_update();
        w.load(vec![end("x", FINISHED, 1)]);
        w.unseen(end("y", FAILED, 9));
        w.unseen(end("z", LIMIT, 4));
        assert_eq!(*told.borrow_and_update(), 4);
        let needs: Vec<(Need, String)> =
            w.list().into_iter().map(|w| (w.need, w.agent.0)).collect();
        assert_eq!(
            needs,
            [
                (Need::Asks, "a".into()),
                (Need::Limit, "z".into()),
                (Need::Failed, "y".into()),
                (Need::Finished, "x".into())
            ]
        );
        for a in ["x", "y", "z"] {
            assert!(w.seen(&s, &AgentId(a.into())));
        }
        assert!(!w.seen(&s, &AgentId("x".into())));
        assert_eq!(*told.borrow_and_update(), 1);
        // Opened and closed at once.
        w.note(&s, &[opened(10, "q4", Answerable::Live), closed(11, "q4")]);
        assert_eq!(asked(&w), ["s q3"]);
    }
}
