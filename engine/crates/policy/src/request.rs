//! Asking a person, and what their answer is worth. An [`Approval`] comes only from
//! [`Requests::answer`], for a request that is open, shown exactly as it is (SI-2). It is spent once,
//! on exactly what was asked (SI-19).

use std::collections::{BTreeMap, BTreeSet};

use axiom_proto::{AgentId, RequestId};
use serde::{Deserialize, Serialize};

/// What an approval is about. Any difference is another thing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ask {
    /// Who asks, with everyone who made it: the first is the session's own agent.
    pub chain: Vec<AgentId>,
    pub workspace: String,
    /// A tool, as `server/tool` if it is not Axiom's own.
    pub tool: String,
    /// Of the tool's description and schema, as the model was given them.
    pub schema: String,
    /// What it would do: the command, the path, the arguments. Whole.
    pub subject: String,
    /// Where it would happen and what it could reach.
    pub scope: String,
    /// Whether it would run outside every sandbox.
    #[serde(default)]
    pub unsandboxed: bool,
}

impl Ask {
    /// Names this ask and no other. A client sends back the one it showed.
    pub fn digest(&self) -> String {
        let mut h = blake3::Hasher::new_derive_key("axiom 2026-11 request");
        // As JSON, where every part has its own quotes: two asks never run together.
        if let Ok(b) = serde_json::to_vec(self) {
            h.update(&b);
        }
        h.finalize().to_hex().to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Once,
    /// Exactly this, by this agent, until the session's host ends.
    Session,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NotAnswered {
    #[error("there is no such question any more")]
    Gone,
    #[error("what was shown is not what is asked")]
    Changed,
    #[error("too many questions are open")]
    Full,
    #[error("running outside every sandbox is allowed once at a time")]
    OnlyOnce,
}

/// A person's yes. Not copied, not made from data, spent by [`Grants::spend`].
#[derive(Debug)]
pub struct Approval {
    digest: String,
    session: bool,
    unsandboxed: bool,
}

/// The token for starting a process outside every sandbox.
#[derive(Debug)]
pub struct Unsandboxed(());

impl Approval {
    /// Of an ask that said so.
    pub fn unsandboxed(self, ask: &Ask) -> Option<Unsandboxed> {
        (self.unsandboxed && ask.unsandboxed && self.digest == ask.digest())
            .then_some(Unsandboxed(()))
    }
}

/// More would be a flood, not questions.
pub const MOST_OPEN: usize = 256;

#[derive(Debug, Default)]
pub struct Requests {
    open: BTreeMap<RequestId, (String, bool)>,
}

impl Requests {
    pub fn open(&mut self, ask: &Ask) -> Result<RequestId, NotAnswered> {
        if self.open.len() >= MOST_OPEN {
            return Err(NotAnswered::Full);
        }
        let id = RequestId::new();
        self.open.insert(id.clone(), (ask.digest(), ask.unsandboxed));
        Ok(id)
    }

    /// `shown` is the digest of what the person saw. A refusal that fits nothing changes nothing:
    /// the question stays open.
    pub fn answer(
        &mut self,
        id: &RequestId,
        a: Answer,
        shown: &str,
    ) -> Result<Option<Approval>, NotAnswered> {
        let (digest, unsandboxed) = self.open.get(id).ok_or(NotAnswered::Gone)?;
        // No is no, whatever was shown.
        if a == Answer::Deny {
            self.open.remove(id);
            return Ok(None);
        }
        if digest != shown {
            return Err(NotAnswered::Changed);
        }
        if *unsandboxed && a == Answer::Session {
            return Err(NotAnswered::OnlyOnce);
        }
        let ap = Approval {
            digest: digest.clone(),
            session: a == Answer::Session,
            unsandboxed: *unsandboxed,
        };
        self.open.remove(id);
        Ok(Some(ap))
    }

    /// The asker is gone.
    pub fn withdraw(&mut self, id: &RequestId) -> bool {
        self.open.remove(id).is_some()
    }

    pub fn is_open(&self, id: &RequestId) -> bool {
        self.open.contains_key(id)
    }
}

/// What has been allowed for the session.
#[derive(Debug, Default)]
pub struct Grants(BTreeSet<String>);

pub const MOST_GRANTS: usize = 10_000;

impl Grants {
    /// Whether this approval is for this ask. A session's one is kept.
    pub fn spend(&mut self, a: Approval, ask: &Ask) -> bool {
        if a.digest != ask.digest() || a.unsandboxed {
            return false;
        }
        if a.session && self.0.len() < MOST_GRANTS {
            self.0.insert(a.digest);
        }
        true
    }

    pub fn holds(&self, ask: &Ask) -> bool {
        !ask.unsandboxed && self.0.contains(&ask.digest())
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    fn ask() -> Ask {
        Ask {
            chain: vec![AgentId("a1".into()), AgentId("a2".into())],
            workspace: "/w".into(),
            tool: "shell".into(),
            schema: "s1".into(),
            subject: "cargo test".into(),
            scope: "in /w, no network".into(),
            unsandboxed: false,
        }
    }

    #[test]
    fn an_answer_is_for_what_was_shown() {
        let mut r = Requests::default();
        let id = r.open(&ask()).unwrap();
        assert_eq!(
            r.answer(&RequestId::new(), Answer::Once, &ask().digest()).unwrap_err(),
            NotAnswered::Gone
        );
        assert_eq!(r.answer(&id, Answer::Once, "other").unwrap_err(), NotAnswered::Changed);
        assert!(r.is_open(&id), "a wrong answer does not use it up");
        let a = r.answer(&id, Answer::Once, &ask().digest()).unwrap().unwrap();
        assert_eq!(r.answer(&id, Answer::Once, &ask().digest()).unwrap_err(), NotAnswered::Gone);
        let mut g = Grants::default();
        assert!(g.spend(a, &ask()));
        assert!(!g.holds(&ask()), "once is once");
    }

    #[test]
    fn no_is_always_possible() {
        let mut r = Requests::default();
        let id = r.open(&ask()).unwrap();
        assert!(r.answer(&id, Answer::Deny, "whatever").unwrap().is_none());
        assert!(!r.is_open(&id));
        let id = r.open(&ask()).unwrap();
        assert!(r.withdraw(&id) && !r.withdraw(&id));
        assert_eq!(r.answer(&id, Answer::Once, &ask().digest()).unwrap_err(), NotAnswered::Gone);
    }

    /// SI-19.
    #[test]
    fn any_change_is_another_thing() {
        let a = ask();
        let others = [
            Ask { chain: vec![AgentId("a1".into())], ..a.clone() },
            Ask { chain: vec![AgentId("a2".into()), AgentId("a1".into())], ..a.clone() },
            Ask { workspace: "/w2".into(), ..a.clone() },
            Ask { tool: "x/shell".into(), ..a.clone() },
            Ask { schema: "s2".into(), ..a.clone() },
            Ask { subject: "cargo test ".into(), ..a.clone() },
            Ask { scope: "network".into(), ..a.clone() },
            Ask { unsandboxed: true, ..a.clone() },
            Ask { tool: "shel".into(), schema: "ls1".into(), ..a.clone() },
        ];
        let mut g = Grants::default();
        let mut r = Requests::default();
        let id = r.open(&a).unwrap();
        let ap = r.answer(&id, Answer::Session, &a.digest()).unwrap().unwrap();
        assert!(g.spend(ap, &a) && g.holds(&a));
        for o in &others {
            assert_ne!(o.digest(), a.digest(), "{o:?}");
            assert!(!g.holds(o));
            let id = r.open(&a).unwrap();
            let ap = r.answer(&id, Answer::Once, &a.digest()).unwrap().unwrap();
            assert!(!g.spend(ap, o), "{o:?}");
        }
        g.clear();
        assert!(!g.holds(&a));
    }

    #[test]
    fn outside_every_sandbox_is_its_own_question_and_once() {
        let out = Ask { unsandboxed: true, ..ask() };
        let mut r = Requests::default();
        let mut g = Grants::default();
        let id = r.open(&out).unwrap();
        assert_eq!(
            r.answer(&id, Answer::Session, &out.digest()).unwrap_err(),
            NotAnswered::OnlyOnce
        );
        let ap = r.answer(&id, Answer::Once, &out.digest()).unwrap().unwrap();
        assert!(!g.spend(ap, &out), "it is no ordinary grant");
        let id = r.open(&out).unwrap();
        let ap = r.answer(&id, Answer::Once, &out.digest()).unwrap().unwrap();
        assert!(ap.unsandboxed(&out).is_some());
        let id = r.open(&ask()).unwrap();
        let ap = r.answer(&id, Answer::Once, &ask().digest()).unwrap().unwrap();
        assert!(ap.unsandboxed(&out).is_none(), "an ordinary yes is not that yes");
        let id = r.open(&out).unwrap();
        let ap = r.answer(&id, Answer::Once, &out.digest()).unwrap().unwrap();
        assert!(ap.unsandboxed(&Ask { subject: "rm".into(), ..out }).is_none());
    }

    #[test]
    fn a_flood_of_questions_is_refused() {
        let mut r = Requests::default();
        for _ in 0..MOST_OPEN {
            r.open(&ask()).unwrap();
        }
        assert_eq!(r.open(&ask()).unwrap_err(), NotAnswered::Full);
    }
}
