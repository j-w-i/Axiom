//! The record of what was done with authority, written before it is done (SI-23). Each entry holds
//! the digest of the one before it, so a line taken out or changed shows (T48). Where it is kept is
//! the journal's business.

use axiom_proto::{AgentId, RequestId, SessionId};
use serde::{Deserialize, Serialize};

use crate::rules::Decision;

/// Who contains what was started (SI-26).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Containment {
    Axiom,
    Harness,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub at: u64,
    pub session: SessionId,
    pub workspace: String,
    /// Who did it, from the session's own agent down.
    pub chain: Vec<AgentId>,
    pub tool: String,
    /// What was done, whole; what is longer is cut by whoever makes the entry, with its length.
    pub subject: String,
    pub decision: Decision,
    /// The rules that decided.
    pub rules: Vec<String>,
    /// The question a person answered, and the digest of what they saw.
    pub answered: Option<(RequestId, String)>,
    pub containment: Containment,
    /// Digest of the entry before; of nothing for the first.
    pub before: String,
    pub digest: String,
}

impl Entry {
    fn compute(&self) -> String {
        let mut h = blake3::Hasher::new_derive_key("axiom 2026-11 audit");
        let bare = Self { digest: String::new(), ..self.clone() };
        if let Ok(b) = serde_json::to_vec(&bare) {
            h.update(&b);
        }
        h.finalize().to_hex().to_string()
    }

    /// The entry as it follows `last`.
    pub fn after(mut self, last: Option<&Entry>) -> Self {
        self.seq = last.map_or(1, |l| l.seq + 1);
        self.before = last.map_or_else(String::new, |l| l.digest.clone());
        self.digest = self.compute();
        self
    }
}

/// The first entry that does not follow from those before it.
pub fn broken<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> Option<u64> {
    let (mut seq, mut before) = (0, String::new());
    for e in entries {
        seq += 1;
        if e.seq != seq || e.before != before || e.digest != e.compute() {
            return Some(seq);
        }
        before.clone_from(&e.digest);
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    fn entry(s: &str) -> Entry {
        Entry {
            seq: 0,
            at: 7,
            session: SessionId("s".into()),
            workspace: "/w".into(),
            chain: vec![AgentId("a".into())],
            tool: "shell".into(),
            subject: s.into(),
            decision: Decision::Allow,
            rules: vec!["r".into()],
            answered: Some((RequestId("q".into()), "d".into())),
            containment: Containment::Axiom,
            before: String::new(),
            digest: String::new(),
        }
    }

    fn chain() -> Vec<Entry> {
        let mut v: Vec<Entry> = Vec::new();
        for s in ["a", "b", "c", "d"] {
            let e = entry(s).after(v.last());
            v.push(e);
        }
        v
    }

    #[test]
    fn a_whole_chain_is_whole() {
        assert_eq!(broken(&chain()), None);
        assert_eq!(broken(&[]), None);
        let s = serde_json::to_string(&chain()).unwrap();
        assert_eq!(broken(&serde_json::from_str::<Vec<Entry>>(&s).unwrap()), None);
    }

    #[test]
    fn whatever_is_done_to_it_shows() {
        let changes: [fn(&mut Entry); 10] = [
            |e| e.at += 1,
            |e| e.session = SessionId("t".into()),
            |e| e.workspace = "/x".into(),
            |e| e.chain.clear(),
            |e| e.tool = "read".into(),
            |e| e.subject.push('x'),
            |e| e.decision = Decision::Ask,
            |e| e.rules.clear(),
            |e| e.answered = None,
            |e| e.containment = Containment::None,
        ];
        for f in changes {
            let mut v = chain();
            f(&mut v[1]);
            assert_eq!(broken(&v), Some(2));
            // Even with its own digest made anew, the next one no longer follows.
            v[1].digest = v[1].compute();
            assert_eq!(broken(&v), Some(3));
        }
        let mut v = chain();
        v.remove(1);
        assert_eq!(broken(&v), Some(2));
        let mut v = chain();
        v.remove(0);
        assert_eq!(broken(&v), Some(1));
        let mut v = chain();
        v.swap(1, 2);
        assert_eq!(broken(&v), Some(2));
    }
}
