//! The append-only journal (docs/decisions/0004-journal.md).
//!
//! One writer. Every append is one transaction: events, the session's row, and the folding of an
//! ended item's deltas into its final text. What is on disk replays to what was shown.

use std::{collections::BTreeMap, path::Path};

use axiom_proto::{Body, Event, ItemId, Outcome, RequestId, RunId, SessionId, SessionInfo};
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("journal: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("journal: {0}")]
    Json(#[from] serde_json::Error),
    #[error("journal: made by a newer Axiom (schema {0})")]
    TooNew(i64),
    #[error("journal: damaged: {0}")]
    Damaged(String),
    #[error("no such session: {0}")]
    NoSession(SessionId),
}
pub type Result<T> = std::result::Result<T, Error>;

const SCHEMA: i64 = 1;

pub struct Journal {
    db: Connection,
}

/// How much a commit must survive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// A crash of the process loses nothing; power loss may lose the last commits.
    Stream,
    /// Power loss loses nothing. For intents before effects, and for what a person did.
    Intent,
}

impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        Self::setup(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self> {
        Self::setup(Connection::open_in_memory()?)
    }

    fn setup(db: Connection) -> Result<Self> {
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "NORMAL")?;
        // macOS: a FULL commit asks the drive to flush its cache, or it is not durable.
        db.pragma_update(None, "fullfsync", true)?;
        db.pragma_update(None, "foreign_keys", true)?;
        let v: i64 = db.query_row("pragma user_version", [], |r| r.get(0))?;
        if v > SCHEMA {
            return Err(Error::TooNew(v));
        }
        if v == 0 {
            db.execute_batch(&format!(
                "begin immediate;
                 create table session(
                   id text primary key, title text not null, workspace text,
                   created integer not null, updated integer not null,
                   last_seq integer not null, open_runs integer not null default 0);
                 create table event(
                   session text not null references session(id) on delete cascade,
                   seq integer not null, at integer not null,
                   delta_of text, body text not null,
                   primary key(session, seq)) without rowid;
                 create index event_delta on event(session, delta_of) where delta_of is not null;
                 pragma user_version = {SCHEMA};
                 commit;"
            ))?;
        }
        let ok: String = db.query_row("pragma quick_check(1)", [], |r| r.get(0))?;
        if ok != "ok" {
            return Err(Error::Damaged(ok));
        }
        Ok(Self { db })
    }

    pub fn create(
        &mut self,
        id: &SessionId,
        title: &str,
        workspace: Option<&str>,
        at: u64,
    ) -> Result<Event> {
        let first = Event {
            seq: 1,
            at,
            body: Body::SessionCreated {
                title: title.into(),
                workspace: workspace.map(Into::into),
            },
        };
        let tx = self.db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute(
            "insert into session(id, title, workspace, created, updated, last_seq) values(?,?,?,?,?,1)",
            params![id.0, title, workspace, as_i64(at), as_i64(at)],
        )?;
        tx.execute(
            "insert into event(session, seq, at, body) values(?,1,?,?)",
            params![id.0, as_i64(at), serde_json::to_string(&first.body)?],
        )?;
        tx.commit()?;
        Ok(first)
    }

    /// Appends in order. Sequence numbers must follow on; nothing is written if they do not.
    pub fn append(&mut self, id: &SessionId, events: &[Event], how: Durability) -> Result<()> {
        self.batch(how, |j| j.append_in(id, events))?
    }

    /// Runs `f` in one commit. An [`Journal::append_in`] that fails inside undoes only itself.
    pub fn batch<T>(&mut self, how: Durability, f: impl FnOnce(&mut Self) -> T) -> Result<T> {
        if how == Durability::Intent {
            self.db.pragma_update(None, "synchronous", "FULL")?;
        }
        let r = (|| {
            self.db.execute_batch("begin immediate")?;
            let r = f(self);
            if let Err(e) = self.db.execute_batch("commit") {
                let _ = self.db.execute_batch("rollback");
                return Err(e.into());
            }
            Ok(r)
        })();
        if how == Durability::Intent {
            self.db.pragma_update(None, "synchronous", "NORMAL")?;
        }
        r
    }

    /// As [`Journal::append`], inside a [`Journal::batch`].
    pub fn append_in(&mut self, id: &SessionId, events: &[Event]) -> Result<()> {
        let Some(last) = events.last() else {
            return Ok(());
        };
        let tx = self.db.savepoint()?;
        let have: i64 = tx
            .query_row("select last_seq from session where id=?", [&id.0], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::NoSession(id.clone()))?;
        let mut want = have;
        let mut runs = 0i64;
        {
            let mut put = tx.prepare_cached(
                "insert into event(session, seq, at, delta_of, body) values(?,?,?,?,?)",
            )?;
            let mut fold = tx.prepare_cached("delete from event where session=? and delta_of=?")?;
            let mut title = tx.prepare_cached("update session set title=? where id=?")?;
            for e in events {
                want += 1;
                if as_i64(e.seq) != want {
                    return Err(Error::Damaged(format!(
                        "{id}: event {} where {want} is next",
                        e.seq
                    )));
                }
                let mut delta_of = None;
                match &e.body {
                    Body::ItemDelta { item, .. } => delta_of = Some(item.0.as_str()),
                    Body::ItemEnded { item, .. } => {
                        fold.execute(params![id.0, item.0])?;
                    }
                    Body::RunStarted { .. } => runs += 1,
                    Body::RunEnded { .. } => runs -= 1,
                    Body::SessionTitled { title: t } => {
                        title.execute(params![t, id.0])?;
                    }
                    _ => {}
                }
                put.execute(params![
                    id.0,
                    as_i64(e.seq),
                    as_i64(e.at),
                    delta_of,
                    serde_json::to_string(&e.body)?
                ])?;
            }
        }
        tx.execute(
            "update session set last_seq=?, updated=?, open_runs=open_runs+? where id=?",
            params![as_i64(last.seq), as_i64(last.at), runs, id.0],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Events after `after`, in order. Deltas of ended items are gone; their text is in `item.ended`.
    pub fn read(&self, id: &SessionId, after: u64, limit: usize) -> Result<Vec<Event>> {
        let mut q = self.db.prepare_cached(
            "select seq, at, body from event where session=? and seq>? order by seq limit ?",
        )?;
        let rows = q.query_map(
            params![id.0, as_i64(after), i64::try_from(limit).unwrap_or(i64::MAX)],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)),
        )?;
        rows.map(|r| {
            let (seq, at, body) = r?;
            Ok(Event { seq: as_u64(seq), at: as_u64(at), body: serde_json::from_str(&body)? })
        })
        .collect()
    }

    pub fn sessions(&self) -> Result<Vec<SessionInfo>> {
        let mut q = self.db.prepare_cached(
            "select id,title,workspace,created,updated,last_seq,open_runs from session order by updated desc",
        )?;
        let rows = q.query_map([], info)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn session(&self, id: &SessionId) -> Result<Option<SessionInfo>> {
        Ok(self
            .db
            .query_row(
                "select id,title,workspace,created,updated,last_seq,open_runs from session where id=?",
                [&id.0],
                info,
            )
            .optional()?)
    }

    /// Ends what was open when the last host went away. Nothing is run again.
    pub fn recover(&mut self, at: u64) -> Result<usize> {
        let mut n = 0;
        let unfinished: Vec<String> = {
            let mut q = self.db.prepare("select id from session")?;
            let rows = q.query_map([], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for s in unfinished {
            let id = SessionId(s);
            let open = self.open_things(&id)?;
            if open.is_empty() {
                continue;
            }
            let mut seq = self.session(&id)?.ok_or_else(|| Error::NoSession(id.clone()))?.last_seq;
            let mut out = Vec::new();
            let mut say = |body| {
                seq += 1;
                out.push(Event { seq, at, body });
            };
            for (item, text) in open.items {
                say(Body::ItemEnded { item, outcome: Outcome::Interrupted, text, result: None });
            }
            for request in open.requests {
                say(Body::RequestClosed { request, answer: "expired".into() });
            }
            for run in open.runs {
                say(Body::RunEnded { run, outcome: Outcome::Interrupted, error: None });
            }
            n += 1;
            self.append(&id, &out, Durability::Intent)?;
        }
        Ok(n)
    }

    fn open_things(&self, id: &SessionId) -> Result<Open> {
        let mut o = Open::default();
        let mut after = 0;
        loop {
            let page = self.read(id, after, 2000)?;
            let Some(last) = page.last() else {
                return Ok(o);
            };
            after = last.seq;
            for e in page {
                match e.body {
                    Body::ItemStarted { item, .. } => {
                        o.items.insert(item, String::new());
                    }
                    Body::ItemDelta { item, text } => {
                        if let Some(t) = o.items.get_mut(&item) {
                            t.push_str(&text);
                        }
                    }
                    Body::ItemEnded { item, .. } => {
                        o.items.remove(&item);
                    }
                    Body::RunStarted { run, .. } => o.runs.push(run),
                    Body::RunEnded { run, .. } => o.runs.retain(|r| *r != run),
                    Body::RequestOpened { request, .. } => o.requests.push(request),
                    Body::RequestClosed { request, .. } => o.requests.retain(|r| *r != request),
                    _ => {}
                }
            }
        }
    }

    pub fn delete(&mut self, id: &SessionId) -> Result<bool> {
        Ok(self.db.execute("delete from session where id=?", [&id.0])? > 0)
    }
}

#[derive(Default)]
struct Open {
    items: BTreeMap<ItemId, String>,
    runs: Vec<RunId>,
    requests: Vec<RequestId>,
}
impl Open {
    fn is_empty(&self) -> bool {
        self.items.is_empty() && self.runs.is_empty() && self.requests.is_empty()
    }
}

fn info(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessionInfo> {
    Ok(SessionInfo {
        id: SessionId(r.get(0)?),
        title: r.get(1)?,
        workspace: r.get(2)?,
        created: as_u64(r.get(3)?),
        updated: as_u64(r.get(4)?),
        last_seq: as_u64(r.get(5)?),
        running: r.get::<_, i64>(6)? > 0,
    })
}
fn as_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}
fn as_u64(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}
