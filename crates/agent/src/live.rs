//! Open sessions: the journal, the event sequence and the broadcast to attached clients
//! (M3-LOOP-001). [`Sessions`] holds every session that has been opened since the daemon
//! started; a session is loaded from its journal on first use, which repairs a turn the daemon
//! died in (`interrupted` events, never a re-run).

use crate::model::Block;
use crate::session::{self, Journal, JournalError, Record, JOURNAL_VERSION};
use axiom_protocol::agent::{AgentEvent, EventBody, SessionId, TurnEndReason};
use axiom_protocol::WorkspaceId;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The workspace a session works in (none for a plain conversation).
#[derive(Debug, Clone)]
pub struct Binding {
    pub workspace_id: WorkspaceId,
    pub root: PathBuf,
}

#[derive(Default)]
struct State {
    records: Vec<Record>,
    /// The `seq` the next persisted event gets.
    next_seq: u64,
    turn: u32,
    running: Option<CancellationToken>,
}

pub struct Live {
    pub id: SessionId,
    pub journal: Journal,
    state: Mutex<State>,
    events: broadcast::Sender<AgentEvent>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Live {
    fn load(id: SessionId, journal: Journal) -> Result<Live, JournalError> {
        let records = journal.read()?;
        let mut st = State::default();
        for r in &records {
            if let Record::Event { event, .. } = r {
                st.next_seq = st.next_seq.max(event.seq + 1);
                st.turn = st.turn.max(event.turn);
            }
        }
        st.next_seq = st.next_seq.max(1);
        st.records = records;
        let (tx, _) = broadcast::channel(4096);
        let live = Live {
            id,
            journal,
            state: Mutex::new(st),
            events: tx,
        };
        live.repair()?;
        Ok(live)
    }

    /// A turn without its `turnEnd` was cut short by the daemon stopping: every tool call in
    /// it without a result becomes `interrupted` (with an error result for the model), and the
    /// turn ends.
    fn repair(&self) -> Result<(), JournalError> {
        let (open, turn, unanswered) = {
            let st = lock(&self.state);
            let mut open = false;
            let mut calls: Vec<String> = Vec::new();
            for r in &st.records {
                if let Record::Event { event, .. } = r {
                    match &event.event {
                        EventBody::User { .. } => {
                            open = true;
                            calls.clear();
                        }
                        EventBody::TurnEnd { .. } => open = false,
                        EventBody::ToolCall { call_id, .. } => calls.push(call_id.clone()),
                        EventBody::ToolResult { call_id, .. }
                        | EventBody::Interrupted { call_id } => calls.retain(|c| c != call_id),
                        _ => {}
                    }
                }
            }
            (open, st.turn, calls)
        };
        if !open {
            return Ok(());
        }
        // The assistant message that asked for the calls may be missing too if the daemon died
        // before writing it; then the calls are not in the context and need no results.
        let asked: Vec<String> = {
            let st = lock(&self.state);
            st.records
                .iter()
                .filter_map(|r| match r {
                    Record::Assistant {
                        turn: t, blocks, ..
                    } if *t == turn => Some(blocks),
                    _ => None,
                })
                .flatten()
                .filter_map(|b| match b {
                    Block::ToolCall { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .collect()
        };
        let mut results = Vec::new();
        for c in &unanswered {
            self.emit(EventBody::Interrupted { call_id: c.clone() })?;
            if asked.contains(c) {
                results.push(Block::ToolResult {
                    call_id: c.clone(),
                    content: "Interrupted: Axiom stopped while this tool was running. It was not re-run; check its effects before relying on them.".into(),
                    is_error: true,
                });
            }
        }
        if !results.is_empty() {
            self.record(Record::ToolResults {
                v: JOURNAL_VERSION,
                turn,
                at_seq: self.last_seq(),
                blocks: results,
            })?;
        }
        self.emit(EventBody::TurnEnd {
            reason: TurnEndReason::Error,
        })?;
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.events.subscribe()
    }

    /// Persisted events so far.
    pub fn events(&self) -> Vec<AgentEvent> {
        session::events(&lock(&self.state).records)
    }

    pub fn records(&self) -> Vec<Record> {
        lock(&self.state).records.clone()
    }

    pub fn turn(&self) -> u32 {
        lock(&self.state).turn
    }

    pub fn last_seq(&self) -> u64 {
        lock(&self.state).next_seq.saturating_sub(1)
    }

    /// Persists and broadcasts an event.
    pub fn emit(&self, body: EventBody) -> Result<AgentEvent, JournalError> {
        let ev = {
            let mut st = lock(&self.state);
            if let EventBody::User { .. } = body {
                st.turn += 1;
            }
            let ev = AgentEvent {
                session_id: self.id.clone(),
                seq: st.next_seq,
                ts_ms: now_ms(),
                turn: st.turn,
                event: body,
            };
            let rec = Record::Event {
                v: JOURNAL_VERSION,
                event: Box::new(ev.clone()),
            };
            self.journal.append(std::slice::from_ref(&rec))?;
            st.records.push(rec);
            st.next_seq += 1;
            ev
        };
        let _ = self.events.send(ev.clone());
        Ok(ev)
    }

    /// A rewind before the user event of an edit/regenerate: the next user event takes turn
    /// `from_turn`.
    pub fn rewind(&self, from_turn: u32) -> Result<AgentEvent, JournalError> {
        let ev = self.emit(EventBody::Rewind { from_turn })?;
        lock(&self.state).turn = from_turn.saturating_sub(1);
        Ok(ev)
    }

    /// Broadcasts an ephemeral event (`textDelta`): never persisted.
    pub fn emit_ephemeral(&self, body: EventBody) {
        let ev = {
            let st = lock(&self.state);
            AgentEvent {
                session_id: self.id.clone(),
                seq: st.next_seq,
                ts_ms: now_ms(),
                turn: st.turn,
                event: body,
            }
        };
        let _ = self.events.send(ev);
    }

    /// Persists an internal record.
    pub fn record(&self, r: Record) -> Result<(), JournalError> {
        let mut st = lock(&self.state);
        self.journal.append(std::slice::from_ref(&r))?;
        st.records.push(r);
        Ok(())
    }

    /// Marks a turn running; `None` if one already is.
    pub fn begin(&self) -> Option<CancellationToken> {
        let mut st = lock(&self.state);
        if st.running.is_some() {
            return None;
        }
        let t = CancellationToken::new();
        st.running = Some(t.clone());
        Some(t)
    }

    pub fn end(&self) {
        lock(&self.state).running = None;
    }

    pub fn running(&self) -> bool {
        lock(&self.state).running.is_some()
    }

    pub fn cancel(&self) {
        if let Some(t) = &lock(&self.state).running {
            t.cancel();
        }
    }
}

/// Every session opened since the daemon started.
pub struct Sessions {
    root: PathBuf,
    live: Mutex<HashMap<SessionId, Arc<Live>>>,
}

/// The directory holding a workspace's journals (`_chat` for conversations without one).
pub fn dir_for(sessions_root: &Path, workspace_root: Option<&Path>) -> PathBuf {
    match workspace_root {
        Some(r) => sessions_root.join(crate::workspace_key(r)),
        None => sessions_root.join("_chat"),
    }
}

impl Sessions {
    /// `root` is `<data dir>/sessions`.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            live: Mutex::default(),
        }
    }

    /// Opens (loading the journal once) the session `id` stored for `workspace_root`.
    pub fn open(
        &self,
        id: &SessionId,
        workspace_root: Option<&Path>,
    ) -> Result<Arc<Live>, JournalError> {
        if let Some(l) = lock(&self.live).get(id) {
            return Ok(l.clone());
        }
        let journal = Journal::new(&dir_for(&self.root, workspace_root), id);
        let live = Arc::new(Live::load(id.clone(), journal)?);
        Ok(lock(&self.live).entry(id.clone()).or_insert(live).clone())
    }

    /// Stops a running turn, forgets the session and deletes its journal.
    pub fn delete(
        &self,
        id: &SessionId,
        workspace_root: Option<&Path>,
    ) -> Result<(), JournalError> {
        if let Some(l) = lock(&self.live).remove(id) {
            l.cancel();
        }
        Journal::new(&dir_for(&self.root, workspace_root), id).delete()
    }

    pub fn get(&self, id: &SessionId) -> Option<Arc<Live>> {
        lock(&self.live).get(id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn live_repairs_an_interrupted_turn_once() {
        let d = tempfile::tempdir().unwrap();
        let ss = Sessions::new(d.path().into());
        let id = SessionId("s1".into());
        {
            let l = ss.open(&id, None).unwrap();
            l.emit(EventBody::User {
                text: "go".into(),
                attachments: vec![],
                images: vec![],
            })
            .unwrap();
            l.record(Record::Assistant {
                v: 1,
                turn: 1,
                at_seq: 1,
                blocks: vec![Block::ToolCall {
                    id: "c1".into(),
                    name: "task.run".into(),
                    input: json!({}),
                }],
            })
            .unwrap();
            l.emit(EventBody::ToolCall {
                call_id: "c1".into(),
                tool: "task.run".into(),
                input: json!({}),
                effect: axiom_protocol::agent::Effect::Execute,
                targets: vec![],
            })
            .unwrap();
        }
        // A new daemon: a fresh manager over the same directory.
        let ss2 = Sessions::new(d.path().into());
        let l = ss2.open(&id, None).unwrap();
        let kinds: Vec<String> = l
            .events()
            .iter()
            .map(|e| {
                serde_json::to_value(&e.event).unwrap()["type"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(kinds, ["user", "toolCall", "interrupted", "turnEnd"]);
        let seqs: Vec<u64> = l.events().iter().map(|e| e.seq).collect();
        assert_eq!(seqs, [1, 2, 3, 4]);
        assert!(l
            .records()
            .iter()
            .any(|r| matches!(r, Record::ToolResults { blocks, .. }
            if matches!(&blocks[0], Block::ToolResult { is_error: true, .. }))));
        // Opening again repairs nothing more.
        let ss3 = Sessions::new(d.path().into());
        assert_eq!(ss3.open(&id, None).unwrap().events().len(), 4);
    }

    #[test]
    fn live_rewind_reuses_the_turn_number() {
        let d = tempfile::tempdir().unwrap();
        let ss = Sessions::new(d.path().into());
        let l = ss.open(&SessionId("s".into()), None).unwrap();
        let u = |t: &str| EventBody::User {
            text: t.into(),
            attachments: vec![],
            images: vec![],
        };
        assert_eq!(l.emit(u("a")).unwrap().turn, 1);
        l.emit(EventBody::TurnEnd {
            reason: TurnEndReason::EndTurn,
        })
        .unwrap();
        assert_eq!(l.emit(u("b")).unwrap().turn, 2);
        l.emit(EventBody::TurnEnd {
            reason: TurnEndReason::EndTurn,
        })
        .unwrap();
        l.rewind(2).unwrap();
        assert_eq!(l.emit(u("b2")).unwrap().turn, 2);
        assert!(l.begin().is_some());
        assert!(l.begin().is_none());
        l.end();
        assert!(!l.running());
    }
}
