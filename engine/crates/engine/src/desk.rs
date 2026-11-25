//! Where questions wait for a person. One for each session: an answer reaches the run that asked, or
//! nobody.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use axiom_policy::{Answer, Approval, Ask, NotAnswered, Requests, request::Grants};
use axiom_proto::RequestId;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Default)]
pub struct Desk(Arc<Mutex<Inner>>);

#[derive(Debug, Default)]
struct Inner {
    requests: Requests,
    waiting: HashMap<RequestId, oneshot::Sender<Result<Approval, String>>>,
    grants: Grants,
}

/// A question that is open until it is answered or this is dropped.
#[derive(Debug)]
pub struct Open {
    desk: Desk,
    pub id: RequestId,
    answer: oneshot::Receiver<Result<Approval, String>>,
    /// Why a person said no, if they said so. Never a yes to anything.
    pub why: String,
}

impl Desk {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether a person allowed exactly this for the session.
    pub fn allowed(&self, ask: &Ask) -> bool {
        self.lock().grants.holds(ask)
    }

    pub fn open(&self, ask: &Ask) -> Result<Open, NotAnswered> {
        let mut i = self.lock();
        let id = i.requests.open(ask)?;
        let (tx, answer) = oneshot::channel();
        i.waiting.insert(id.clone(), tx);
        Ok(Open { desk: self.clone(), id, answer, why: String::new() })
    }

    /// A person's answer, from an authenticated client. `shown` is the digest of what they saw.
    pub fn answer(&self, id: &RequestId, a: Answer, shown: &str) -> Result<(), NotAnswered> {
        let mut i = self.lock();
        let approval = i.requests.answer(id, a, shown)?;
        let tx = i.waiting.remove(id).ok_or(NotAnswered::Gone)?;
        // Whoever asked may have gone this moment: then it was for nothing.
        tx.send(approval.ok_or_else(String::new)).map_err(|_| NotAnswered::Gone)
    }

    /// No, and why, for whoever asked to read.
    pub fn deny(&self, id: &RequestId, why: String) -> Result<(), NotAnswered> {
        let mut i = self.lock();
        i.requests.answer(id, Answer::Deny, "")?;
        let tx = i.waiting.remove(id).ok_or(NotAnswered::Gone)?;
        tx.send(Err(why)).map_err(|_| NotAnswered::Gone)
    }

    /// Spends an approval on what it was given for.
    pub fn spend(&self, a: Approval, ask: &Ask) -> bool {
        self.lock().grants.spend(a, ask)
    }

    /// Nothing is allowed for the session any more: its agents' tools or access changed.
    pub fn forget(&self) {
        self.lock().grants.clear();
    }
}

impl Open {
    /// Waits for a person, however long.
    pub async fn wait(&mut self) -> Option<Approval> {
        match (&mut self.answer).await {
            Ok(Ok(a)) => Some(a),
            Ok(Err(why)) => {
                self.why = why;
                None
            }
            Err(_) => None,
        }
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        let mut i = self.desk.lock();
        i.requests.withdraw(&self.id);
        i.waiting.remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn ask(subject: &str) -> Ask {
        Ask {
            chain: vec![],
            workspace: "w".into(),
            tool: "shell".into(),
            schema: "s".into(),
            subject: subject.into(),
            scope: "c".into(),
            unsandboxed: false,
        }
    }

    #[tokio::test]
    async fn an_answer_reaches_who_asked() {
        let d = Desk::default();
        let (a, b) = (ask("a"), ask("b"));
        let (mut oa, mut ob) = (d.open(&a).unwrap(), d.open(&b).unwrap());
        assert_eq!(d.answer(&oa.id, Answer::Once, &b.digest()), Err(NotAnswered::Changed));
        d.answer(&ob.id, Answer::Session, &b.digest()).unwrap();
        d.answer(&oa.id, Answer::Deny, "").unwrap();
        assert!(oa.wait().await.is_none() && oa.why.is_empty());
        let mut oc = d.open(&a).unwrap();
        d.deny(&oc.id, "not now".into()).unwrap();
        assert!(oc.wait().await.is_none());
        assert_eq!((oc.why.as_str(), d.allowed(&a)), ("not now", false));
        assert_eq!(d.deny(&oc.id, String::new()), Err(NotAnswered::Gone));
        let ap = ob.wait().await.unwrap();
        assert!(d.spend(ap, &b) && d.allowed(&b) && !d.allowed(&a));
        d.forget();
        assert!(!d.allowed(&b));
        assert_eq!(d.answer(&ob.id, Answer::Once, &b.digest()), Err(NotAnswered::Gone));
    }

    #[test]
    fn a_question_nobody_waits_for_is_closed() {
        let d = Desk::default();
        let o = d.open(&ask("a")).unwrap();
        let id = o.id.clone();
        drop(o);
        assert_eq!(d.answer(&id, Answer::Once, &ask("a").digest()), Err(NotAnswered::Gone));
    }
}
