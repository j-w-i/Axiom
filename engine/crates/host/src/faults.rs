//! What the host cannot read or write, in words for `host.status`.

use std::{
    collections::BTreeMap,
    sync::{Mutex, MutexGuard, PoisonError},
};

use axiom_proto::{Fault, SessionId, limits};

/// Most of them kept, and in one answer.
const KEPT: usize = 200;

#[derive(Default)]
pub struct Faults(Mutex<BTreeMap<SessionId, Words>>);

#[derive(Default)]
struct Words {
    read: Option<String>,
    write: Option<String>,
}

#[derive(Clone, Copy)]
pub enum Access {
    Read,
    Write,
}

impl Faults {
    fn all(&self) -> MutexGuard<'_, BTreeMap<SessionId, Words>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
    /// Notes a fault, and gives its words as they may be shown.
    pub fn note(&self, id: &SessionId, what: Access, why: &dyn std::fmt::Display) -> String {
        let mut words = why.to_string();
        limits::clip(&mut words, limits::TITLE);
        eprintln!("axiom-host: session {id}: {words}");
        let mut all = self.all();
        if all.len() < KEPT || all.contains_key(id) {
            *all.entry(id.clone()).or_default().of(what) = Some(words.clone());
        }
        words
    }
    /// It works again.
    pub fn clear(&self, id: &SessionId, what: Access) {
        let mut all = self.all();
        if let Some(w) = all.get_mut(id) {
            *w.of(what) = None;
            if w.read.is_none() && w.write.is_none() {
                all.remove(id);
            }
        }
    }
    pub fn list(&self) -> Vec<Fault> {
        let all = self.all();
        let words = |w: &Words| w.write.clone().or_else(|| w.read.clone()).unwrap_or_default();
        all.iter().map(|(s, w)| Fault { session: s.clone(), words: words(w) }).collect()
    }
    pub fn words(&self, id: &SessionId) -> Option<String> {
        self.all().get(id).and_then(|w| w.write.clone().or_else(|| w.read.clone()))
    }
}
impl Words {
    fn of(&mut self, what: Access) -> &mut Option<String> {
        match what {
            Access::Read => &mut self.read,
            Access::Write => &mut self.write,
        }
    }
}
