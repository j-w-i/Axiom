//! The append-only journal (docs/decisions/0004-journal.md).
//!
//! One writer. Every append is one transaction: events, the session's row, what is open, and the
//! folding of an ended item's deltas into its final text. What is on disk replays to what was shown.

use std::{path::Path, time::Duration};

use axiom_proto::{
    AgentId, AgentState, Body, Event, ItemId, ItemKind, Outcome, RequestId, RunId, SessionId,
    SessionInfo, limits,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("journal: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("journal: {0}")]
    Json(#[from] serde_json::Error),
    #[error("journal: {0}")]
    File(#[from] std::io::Error),
    #[error("journal: made by a newer Axiom (schema {0})")]
    TooNew(i64),
    #[error("journal: damaged: {0}")]
    Damaged(String),
    #[error("no such session")]
    NoSession(SessionId),
    #[error("{0}")]
    Unavailable(String),
}
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Another try may succeed: someone else holds the file, or the system failed to read or write.
    pub fn transient(&self) -> bool {
        use rusqlite::ErrorCode::SystemIoFailure;
        self.busy() || self.code() == Some(SystemIoFailure)
    }
    pub fn busy(&self) -> bool {
        use rusqlite::ErrorCode::{DatabaseBusy, DatabaseLocked};
        matches!(self.code(), Some(DatabaseBusy | DatabaseLocked))
    }
    fn code(&self) -> Option<rusqlite::ErrorCode> {
        match self {
            Error::Sql(e) => e.sqlite_error_code(),
            _ => None,
        }
    }
}

const SCHEMA: i64 = 3;
/// How long opening waits for another process.
const OPENING: Duration = Duration::from_secs(1);
/// How long a read does. It needs to only while another process recovers the file.
const READING: Duration = Duration::from_millis(100);

/// What began and has not ended. Recovery needs nothing else.
const OPEN_TABLE: &str = "create table open(
       session text not null references session(id) on delete cascade,
       kind integer not null, id text not null, seq integer not null,
       primary key(session, kind, id)) without rowid;";
/// Beside the events, what only the host needs: how each agent is set up, its conversation as the
/// model has it (its own words must go back to it byte for byte), and the audit record. A message
/// or a setup goes with its session; the audit record stays.
const HOST_TABLES: &str = "create table agent(
       session text not null references session(id) on delete cascade,
       id text not null, setup text not null,
       primary key(session, id)) without rowid;
     create table message(
       session text not null references session(id) on delete cascade,
       agent text not null, n integer not null, body text not null,
       primary key(session, agent, n)) without rowid;
     create table audit(seq integer primary key, body text not null);";
/// The planner would scan the whole session for these; `indexed by` makes it an error not to use
/// the index.
const FOLD: &str = "delete from event indexed by event_delta where session=? and delta_of=?";
const DELTAS: &str = "select body from event indexed by event_delta
     where session=? and delta_of=? order by seq";

/// Kinds of open things, in the order in which recovery ends them.
const ITEM: i64 = 0;
const REQUEST: i64 = 1;
const RUN: i64 = 2;
const AGENT: i64 = 3;

/// The writer. There is one per file (the host holds a lock).
pub struct Journal {
    db: Connection,
    #[cfg(test)]
    synced: Vec<i64>,
}

/// How much a commit must survive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// A crash of the process loses nothing; power loss may lose what followed the last Intent.
    Stream,
    /// Power loss loses nothing. For intents before effects, and for what a person did.
    Intent,
}

pub fn durability(b: &Body) -> Durability {
    match b {
        Body::SessionCreated { .. }
        | Body::SessionTitled { .. }
        | Body::AgentAdded { .. }
        | Body::RunStarted { .. }
        | Body::ItemStarted { kind: ItemKind::Prompt | ItemKind::Tool, .. }
        | Body::RequestOpened { .. }
        | Body::RequestClosed { .. } => Durability::Intent,
        _ => Durability::Stream,
    }
}

impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        private(path)?;
        Self::setup(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self> {
        Self::setup(Connection::open_in_memory()?)
    }

    fn setup(mut db: Connection) -> Result<Self> {
        db.busy_timeout(OPENING)?;
        let v: i64 = db.query_row("pragma user_version", [], |r| r.get(0))?;
        if v > SCHEMA {
            return Err(Error::TooNew(v));
        }
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "NORMAL")?;
        // macOS: a FULL commit asks the drive to flush its cache, or it is not durable.
        db.pragma_update(None, "fullfsync", true)?;
        db.pragma_update(None, "foreign_keys", true)?;
        if v < SCHEMA {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if v == 0 {
                tx.execute_batch(
                    "create table session(
                       id text primary key, title text not null, workspace text,
                       created integer not null, updated integer not null,
                       last_seq integer not null);
                     create table event(
                       session text not null references session(id) on delete cascade,
                       seq integer not null, at integer not null,
                       delta_of text, body text not null,
                       primary key(session, seq)) without rowid;
                     create index event_delta on event(session, delta_of) where delta_of is not null;",
                )?;
                tx.execute_batch(OPEN_TABLE)?;
            }
            if v == 1 {
                from_1(&tx)?;
            }
            tx.execute_batch(HOST_TABLES)?;
            tx.pragma_update(None, "user_version", SCHEMA)?;
            tx.commit()?;
        }
        Ok(Self {
            db,
            #[cfg(test)]
            synced: Vec::new(),
        })
    }

    /// From here on nothing waits for another process: the caller decides how long to try.
    pub fn hurry(&self) -> Result<()> {
        Ok(self.db.busy_timeout(Duration::ZERO)?)
    }

    pub fn create(
        &mut self,
        id: &SessionId,
        title: &str,
        workspace: Option<&str>,
        at: u64,
    ) -> Result<Event> {
        let mut body =
            Body::SessionCreated { title: title.into(), workspace: workspace.map(Into::into) };
        body.fit();
        self.commit(durability(&body), |j| {
            j.db.prepare_cached(
                "insert into session(id, title, workspace, created, updated, last_seq)
                 values(?,?,?,?,?,1)",
            )?
            .execute(params![id.0, title, workspace, as_i64(at), as_i64(at)])?;
            j.db.prepare_cached("insert into event(session, seq, at, body) values(?,1,?,?)")?
                .execute(params![id.0, as_i64(at), serde_json::to_string(&body)?])?;
            Ok(())
        })?;
        Ok(Event { seq: 1, at, body })
    }

    /// Appends in order. Sequence numbers must follow on; nothing is written if they do not.
    pub fn append(&mut self, id: &SessionId, events: &[Event]) -> Result<()> {
        self.append_group(&[(id, events)])?.pop().unwrap_or(Ok(()))
    }

    /// Appends for several sessions in one commit. One that is refused undoes only itself; after
    /// an `Err` of the whole, nothing of any of them is on disk.
    pub fn append_group(&mut self, group: &[(&SessionId, &[Event])]) -> Result<Vec<Result<()>>> {
        let intent = |e: &Event| durability(&e.body) == Durability::Intent;
        let how = if group.iter().any(|(_, events)| events.iter().any(intent)) {
            Durability::Intent
        } else {
            Durability::Stream
        };
        self.commit(how, |j| {
            let mut each = Vec::with_capacity(group.len());
            for (id, events) in group {
                let r = j.append_in(id, events);
                // After some failures (a full disk, a failed write) SQLite has already rolled the
                // whole transaction back. Going on would commit the next append by itself.
                if j.db.is_autocommit() {
                    return Err(match r {
                        Err(e) => e,
                        Ok(()) => Error::Unavailable("journal: the transaction was lost".into()),
                    });
                }
                each.push(r);
            }
            Ok(each)
        })
    }

    /// Runs `f` in one commit, or undoes it.
    fn commit<T>(&mut self, how: Durability, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if how == Durability::Intent {
            self.db.pragma_update(None, "synchronous", "FULL")?;
        }
        let r = (|| {
            self.db.execute_batch("begin immediate")?;
            let r = f(self).and_then(|r| {
                #[cfg(test)]
                self.synced.push(self.db.query_row("pragma synchronous", [], |r| r.get(0))?);
                self.db.execute_batch("commit")?;
                Ok(r)
            });
            if r.is_err() && !self.db.is_autocommit() {
                self.db.execute_batch("rollback")?;
            }
            r
        })();
        if how == Durability::Intent {
            // If this fails, the next commits are only more durable than they need to be.
            let _ = self.db.pragma_update(None, "synchronous", "NORMAL");
        }
        r
    }

    fn append_in(&mut self, id: &SessionId, events: &[Event]) -> Result<()> {
        let Some(last) = events.last() else {
            return Ok(());
        };
        let tx = self.db.savepoint()?;
        let have: i64 = tx
            .prepare_cached("select last_seq from session where id=?")?
            .query_row([&id.0], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::NoSession(id.clone()))?;
        let mut want = have;
        {
            let mut put = tx.prepare_cached(
                "insert into event(session, seq, at, delta_of, body) values(?,?,?,?,?)",
            )?;
            for e in events {
                want += 1;
                if as_i64(e.seq) != want {
                    return Err(Error::Damaged(format!("event {} where {want} is next", e.seq)));
                }
                let mut delta_of = None;
                match &e.body {
                    Body::ItemDelta { item, .. } => delta_of = Some(item.0.as_str()),
                    Body::ItemEnded { item, .. } => {
                        tx.prepare_cached(FOLD)?.execute(params![id.0, item.0])?;
                    }
                    Body::SessionTitled { title } => {
                        tx.prepare_cached("update session set title=? where id=?")?
                            .execute(params![title, id.0])?;
                    }
                    _ => {}
                }
                track(&tx, id, e.seq, &e.body)?;
                put.execute(params![
                    id.0,
                    as_i64(e.seq),
                    as_i64(e.at),
                    delta_of,
                    serde_json::to_string(&e.body)?
                ])?;
            }
            tx.prepare_cached("update session set last_seq=?, updated=? where id=?")?
                .execute(params![as_i64(last.seq), as_i64(last.at), id.0])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn read(&self, id: &SessionId, after: u64, bytes: usize) -> Result<Vec<Event>> {
        read(&self.db, id, after, bytes)
    }
    pub fn sessions(&self, limit: u32, offset: u32) -> Result<Vec<SessionInfo>> {
        sessions(&self.db, limit, offset)
    }
    pub fn session(&self, id: &SessionId) -> Result<Option<SessionInfo>> {
        session(&self.db, id)
    }

    /// Ends what was open when the last host went away. Nothing is run again. Says how many
    /// sessions it ended, and which it could not: those do not keep the others from opening.
    pub fn recover(&mut self, at: u64) -> Result<(usize, Vec<(SessionId, Error)>)> {
        let unfinished: Vec<String> = {
            let mut q = self.db.prepare("select distinct session from open")?;
            let rows = q.query_map([], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        let (mut n, mut failed) = (0, Vec::new());
        for id in unfinished.into_iter().map(SessionId) {
            match self.recover_session(&id, at) {
                Ok(()) => n += 1,
                Err(e) => failed.push((id, e)),
            }
        }
        Ok((n, failed))
    }

    /// Ends what is open in one session. The caller knows that nothing runs in it.
    fn recover_session(&mut self, id: &SessionId, at: u64) -> Result<()> {
        match ending(&self.db, id, at)? {
            Some((_, events)) => self.append(id, &events),
            None => Ok(()),
        }
    }

    /// Keeps how an agent is set up.
    pub fn put_agent(&mut self, id: &SessionId, agent: &AgentId, setup: &str) -> Result<()> {
        self.commit(Durability::Intent, |j| {
            j.db.prepare_cached("insert or replace into agent(session, id, setup) values(?,?,?)")?
                .execute(params![id.0, agent.0, setup])?;
            Ok(())
        })
    }

    /// Adds to an agent's conversation. `n` is its next number, from 0: a gap or a repeat is refused.
    pub fn put_message(
        &mut self,
        id: &SessionId,
        agent: &AgentId,
        n: u64,
        body: &str,
    ) -> Result<()> {
        self.commit(Durability::Intent, |j| {
            let have =
                j.db.prepare_cached("select count(*) from message where session=? and agent=?")?
                    .query_row(params![id.0, agent.0], |r| r.get(0).map(as_u64))?;
            if have != n {
                return Err(Error::Damaged(format!("message {n} where {have} is next")));
            }
            j.db.prepare_cached("insert into message(session, agent, n, body) values(?,?,?,?)")?
                .execute(params![id.0, agent.0, as_i64(n), body])?;
            Ok(())
        })
    }

    /// Adds to the audit record, on the drive before this returns. `seq` follows the last.
    pub fn put_audit(&mut self, seq: u64, body: &str) -> Result<()> {
        self.commit(Durability::Intent, |j| {
            let last: Option<i64> =
                j.db.query_row("select max(seq) from audit", [], |r| r.get(0))?;
            let next = last.map_or(1, |l| as_u64(l) + 1);
            if next != seq {
                return Err(Error::Damaged(format!("audit entry {seq} where {next} is next")));
            }
            j.db.prepare_cached("insert into audit(seq, body) values(?,?)")?
                .execute(params![as_i64(seq), body])?;
            Ok(())
        })
    }

    /// The last entry of the audit record.
    pub fn last_audit(&self) -> Result<Option<String>> {
        Ok(self
            .db
            .query_row("select body from audit order by seq desc limit 1", [], |r| r.get(0))
            .optional()?)
    }

    pub fn delete(&mut self, id: &SessionId) -> Result<bool> {
        Ok(self.db.execute("delete from session where id=?", [&id.0])? > 0)
    }
}

/// Reads while the [`Journal`] writes.
pub struct Reader {
    db: Connection,
}

impl Reader {
    pub fn open(path: &Path) -> Result<Self> {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let db = Connection::open_with_flags(path, flags)?;
        db.busy_timeout(READING)?;
        Ok(Self { db })
    }
    /// Events after `after`, in order, of about `bytes` as they are encoded, and at least one if
    /// there is one. Deltas of ended items are gone; their text is in `item.ended`.
    pub fn read(&self, id: &SessionId, after: u64, bytes: usize) -> Result<Vec<Event>> {
        read(&self.db, id, after, bytes)
    }
    /// The last changed first.
    pub fn sessions(&self, limit: u32, offset: u32) -> Result<Vec<SessionInfo>> {
        sessions(&self.db, limit, offset)
    }
    pub fn session(&self, id: &SessionId) -> Result<Option<SessionInfo>> {
        session(&self.db, id)
    }
    /// Where a session's history ends, and the events that would end what is open in it. `None` if
    /// there is no such session.
    pub fn ending(&self, id: &SessionId, at: u64) -> Result<Option<(u64, Vec<Event>)>> {
        ending(&self.db, id, at)
    }
    /// The agents of a session, with how they are set up.
    pub fn agents(&self, id: &SessionId) -> Result<Vec<(AgentId, String)>> {
        let mut q =
            self.db.prepare_cached("select id, setup from agent where session=? order by id")?;
        let rows = q.query_map([&id.0], |r| Ok((AgentId(r.get(0)?), r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
    /// An agent's conversation, in order.
    pub fn messages(&self, id: &SessionId, agent: &AgentId) -> Result<Vec<String>> {
        let mut q = self
            .db
            .prepare_cached("select n, body from message where session=? and agent=? order by n")?;
        let rows = q.query_map(params![id.0, agent.0], |r| Ok((as_u64(r.get(0)?), r.get(1)?)))?;
        let mut out = Vec::new();
        for r in rows {
            let (n, body) = r?;
            if n != out.len() as u64 {
                return Err(Error::Damaged(format!("message {} is missing", out.len())));
            }
            out.push(body);
        }
        Ok(out)
    }
    /// The audit record after `after`, at most `most` entries.
    pub fn audit(&self, after: u64, most: u32) -> Result<Vec<String>> {
        let mut q =
            self.db.prepare_cached("select body from audit where seq>? order by seq limit ?")?;
        let rows = q.query_map(params![as_i64(after), most], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
    /// Looks through the whole file.
    pub fn check(&self) -> Result<()> {
        let ok: String = self.db.query_row("pragma quick_check(1)", [], |r| r.get(0))?;
        if ok == "ok" { Ok(()) } else { Err(Error::Damaged(ok)) }
    }
}

/// Around a body in its event: names, numbers, a comma.
const AROUND: usize = 64;

fn read(db: &Connection, id: &SessionId, after: u64, bytes: usize) -> Result<Vec<Event>> {
    let mut q = db
        .prepare_cached("select seq, at, body from event where session=? and seq>? order by seq")?;
    let mut rows = q.query(params![id.0, as_i64(after)])?;
    let (mut out, mut size) = (Vec::new(), 0);
    while let Some(r) = rows.next()? {
        let (seq, at) = (as_u64(r.get(0)?), as_u64(r.get(1)?));
        let body = r.get_ref(2)?.as_str().unwrap_or_default();
        size += body.len() + AROUND;
        if !out.is_empty() && size > bytes {
            break;
        }
        match decode(body) {
            Some(body) => out.push(Event { seq, at, body }),
            // What comes before a damaged event can still be read.
            None if !out.is_empty() => break,
            None => return Err(Error::Damaged(format!("event {seq} cannot be read"))),
        }
    }
    Ok(out)
}

/// `None` if this build would have written the type otherwise. A newer type decodes.
fn decode(text: &str) -> Option<Body> {
    let mut b: Body = serde_json::from_str(text).ok()?;
    if b.malformed() {
        return None;
    }
    // What was written before there were limits.
    b.fit();
    Some(b)
}

fn ending(db: &Connection, id: &SessionId, at: u64) -> Result<Option<(u64, Vec<Event>)>> {
    let Some(last) = session(db, id)?.map(|s| s.last_seq) else {
        return Ok(None);
    };
    let open: Vec<(i64, String)> = {
        let mut q =
            db.prepare_cached("select kind, id from open where session=? order by kind, seq")?;
        let rows = q.query_map([&id.0], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    let mut out = Vec::new();
    for (kind, it) in open {
        let body = match kind {
            ITEM => Body::ItemEnded {
                text: text_so_far(db, id, &it)?,
                item: ItemId(it),
                outcome: Outcome::Interrupted,
                result: None,
                result_cut: 0,
            },
            REQUEST => Body::RequestClosed { request: RequestId(it), answer: "expired".into() },
            RUN => Body::RunEnded { run: RunId(it), outcome: Outcome::Interrupted, error: None },
            AGENT => Body::AgentState { agent: AgentId(it), state: AgentState::Paused },
            k => return Err(Error::Damaged(format!("an open thing of kind {k}"))),
        };
        out.push(Event { seq: last + 1 + out.len() as u64, at, body });
    }
    Ok(Some((last, out)))
}

/// Stands where a part of an unfinished item's text cannot be read.
const LOST: &str = "\n[a part of this is damaged and could not be read]\n";

fn text_so_far(db: &Connection, id: &SessionId, item: &str) -> Result<String> {
    let mut q = db.prepare_cached(DELTAS)?;
    let mut rows = q.query(params![id.0, item])?;
    let mut text = String::new();
    while let Some(r) = rows.next()? {
        match serde_json::from_str(r.get_ref(0)?.as_str().unwrap_or_default()) {
            Ok(Body::ItemDelta { text: t, .. }) => text.push_str(&t),
            _ if text.ends_with(LOST) => {}
            _ => text.push_str(LOST),
        }
    }
    Ok(text)
}

const INFO: &str = "select id, title, workspace, created, updated, last_seq,
    exists(select 1 from open where session=session.id and kind=2) from session";

fn sessions(db: &Connection, limit: u32, offset: u32) -> Result<Vec<SessionInfo>> {
    let mut q =
        db.prepare_cached(&format!("{INFO} order by updated desc, id desc limit ? offset ?"))?;
    let rows = q.query_map([limit, offset], info)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

fn session(db: &Connection, id: &SessionId) -> Result<Option<SessionInfo>> {
    let mut q = db.prepare_cached(&format!("{INFO} where id=?"))?;
    Ok(q.query_row([&id.0], info).optional()?)
}

fn info(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessionInfo> {
    let mut i = SessionInfo {
        id: SessionId(r.get(0)?),
        title: r.get(1)?,
        workspace: r.get(2)?,
        created: as_u64(r.get(3)?),
        updated: as_u64(r.get(4)?),
        last_seq: as_u64(r.get(5)?),
        running: r.get(6)?,
    };
    limits::clip(&mut i.title, limits::TITLE);
    if let Some(w) = &mut i.workspace {
        limits::clip(w, limits::WORKSPACE);
    }
    Ok(i)
}

/// Keeps the table of open things.
fn track(db: &Connection, id: &SessionId, seq: u64, body: &Body) -> Result<()> {
    let (kind, it, open) = match body {
        Body::ItemStarted { item, .. } => (ITEM, &item.0, true),
        Body::ItemEnded { item, .. } => (ITEM, &item.0, false),
        Body::RequestOpened { request, .. } => (REQUEST, &request.0, true),
        Body::RequestClosed { request, .. } => (REQUEST, &request.0, false),
        Body::RunStarted { run, .. } => (RUN, &run.0, true),
        Body::RunEnded { run, .. } => (RUN, &run.0, false),
        Body::AgentState { agent, state } => {
            (AGENT, &agent.0, matches!(state, AgentState::Running | AgentState::Asking))
        }
        _ => return Ok(()),
    };
    if open {
        db.prepare_cached("insert or replace into open(session, kind, id, seq) values(?,?,?,?)")?
            .execute(params![id.0, kind, it, as_i64(seq)])?;
    } else {
        db.prepare_cached("delete from open where session=? and kind=? and id=?")?
            .execute(params![id.0, kind, it])?;
    }
    Ok(())
}

/// Schema 1 had a counter of open runs, and recovery read every event.
fn from_1(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(OPEN_TABLE)?;
    let mut q = tx.prepare("select session, seq, body from event order by session, seq")?;
    let mut rows = q.query([])?;
    while let Some(r) = rows.next()? {
        // A damaged event opens nothing here; reading it later is what reports it.
        if let Ok(body) = serde_json::from_str(r.get_ref(2)?.as_str().unwrap_or_default()) {
            track(tx, &SessionId(r.get(0)?), as_u64(r.get(1)?), &body)?;
        }
    }
    drop(rows);
    drop(q);
    tx.execute_batch("alter table session drop column open_runs;")?;
    Ok(())
}

/// The journal holds everything anyone said; SQLite gives its side files the same mode.
#[cfg(unix)]
fn private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let file = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(path)?;
    let side = |s: &str| {
        let mut p = path.as_os_str().to_owned();
        p.push(s);
        std::fs::File::open(p)
    };
    for f in [Ok(file), side("-wal"), side("-shm")].into_iter().flatten() {
        if f.metadata()?.permissions().mode() & 0o077 != 0 {
            f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(())
}
#[cfg(not(unix))]
fn private(_: &Path) -> std::io::Result<()> {
    Ok(())
}

fn as_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}
fn as_u64(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use axiom_proto::{Answerable, Execution, Provenance, Usage};

    use super::*;

    fn bodies() -> Vec<(Body, Durability)> {
        use Durability::{Intent, Stream};
        let (a, r, i, q) = (AgentId::new(), RunId::new(), ItemId::new(), RequestId::new());
        let item = |kind| Body::ItemStarted {
            item: i.clone(),
            run: r.clone(),
            agent: a.clone(),
            kind,
            name: String::new(),
            args: None,
            args_cut: 0,
        };
        vec![
            (Body::SessionTitled { title: "t".into() }, Intent),
            (
                Body::AgentAdded {
                    agent: a.clone(),
                    name: "n".into(),
                    execution: Execution {
                        harness: "h".into(),
                        provider: None,
                        model: None,
                        connection: None,
                        effort: None,
                        environment: None,
                    },
                    parent: None,
                },
                Intent,
            ),
            (Body::RunStarted { run: r.clone(), agent: a.clone() }, Intent),
            (item(ItemKind::Prompt), Intent),
            (item(ItemKind::Tool), Intent),
            (item(ItemKind::Prose), Stream),
            (item(ItemKind::Reasoning), Stream),
            (Body::ItemDelta { item: i.clone(), text: "t".into() }, Stream),
            (
                Body::RequestOpened {
                    request: q.clone(),
                    agent: a.clone(),
                    what: "w".into(),
                    subject: "s".into(),
                    scope: "c".into(),
                    answerable: Answerable::Live,
                },
                Intent,
            ),
            (Body::RequestClosed { request: q, answer: "a".into() }, Intent),
            (Body::AgentState { agent: a.clone(), state: AgentState::Running }, Stream),
            (
                Body::Usage { agent: a, usage: Usage::default(), provenance: Provenance::Measured },
                Stream,
            ),
            (
                Body::ItemEnded {
                    item: i,
                    outcome: Outcome::Completed,
                    text: "t".into(),
                    result: None,
                    result_cut: 0,
                },
                Stream,
            ),
            (Body::RunEnded { run: r, outcome: Outcome::Completed, error: None }, Stream),
        ]
    }

    /// `pragma synchronous` as it was when each commit was made: 1 is NORMAL, 2 is FULL.
    #[test]
    fn intents_are_committed_for_good_and_the_stream_is_not_made_to_wait() {
        let mut j = Journal::in_memory().unwrap();
        let now = |j: &Journal, p| j.db.query_row(p, [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(now(&j, "pragma fullfsync"), 1);
        let (a, b) = (SessionId::new(), SessionId::new());
        j.create(&a, "A", None, 1).unwrap();
        j.create(&b, "B", None, 1).unwrap();
        assert_eq!(std::mem::take(&mut j.synced), [2, 2], "session.created");
        let mut seq = 1;
        for (body, how) in bodies() {
            seq += 1;
            j.append(&a, &[Event { seq, at: 1, body: body.clone() }]).unwrap();
            let want = if how == Durability::Intent { 2 } else { 1 };
            assert_eq!(std::mem::take(&mut j.synced), [want], "{body:?}");
            assert_eq!(now(&j, "pragma synchronous"), 1, "what follows is a stream again");
        }
        // One intent anywhere in a group is enough.
        let state = AgentState::Done;
        let quiet = Body::AgentState { agent: AgentId::new(), state };
        let one = [Event { seq: seq + 1, at: 1, body: quiet }];
        let two = [Event { seq: 2, at: 1, body: Body::SessionTitled { title: "t".into() } }];
        j.append_group(&[(&a, &one), (&b, &two)]).unwrap();
        assert_eq!(j.synced, [2]);
    }

    #[test]
    fn a_rollback_by_sqlite_leaves_nothing_of_the_group() {
        let d = tempfile::tempdir().unwrap();
        let mut j = Journal::open(&d.path().join("j")).unwrap();
        let ids: Vec<_> = ["a", "b", "c"].into_iter().map(|s| SessionId(s.into())).collect();
        for s in &ids {
            j.create(s, "t", None, 1).unwrap();
        }
        j.db.pragma_update(None, "max_page_count", 30).unwrap();
        let t = |n| [Event { seq: 2, at: 1, body: Body::SessionTitled { title: "x".repeat(n) } }];
        let (small, big) = (t(10), t(400_000));
        let r = j.append_group(&[(&ids[0], &small), (&ids[1], &big), (&ids[2], &small)]);
        assert!(r.is_err(), "{r:?}");
        for s in &ids {
            assert_eq!(j.session(s).unwrap().unwrap().last_seq, 1, "{s}");
        }
        // Alone, only the one that does not fit is refused.
        assert!(j.append(&ids[0], &small).is_ok());
        assert!(j.append(&ids[1], &big).is_err());
        assert!(j.append(&ids[2], &small).is_ok());
    }

    #[test]
    fn deltas_are_found_by_their_index() {
        let j = Journal::in_memory().unwrap();
        for sql in [FOLD, DELTAS] {
            let plan: String =
                j.db.query_row(&format!("explain query plan {sql}"), ["s", "i"], |r| r.get(3))
                    .unwrap();
            assert!(plan.contains("event_delta (session=? AND delta_of=?)"), "{plan}");
        }
    }

    #[test]
    fn another_process_that_holds_the_file_is_a_passing_error() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("j");
        let mut j = Journal::open(&p).unwrap();
        j.hurry().unwrap();
        let s = SessionId::new();
        j.create(&s, "t", None, 1).unwrap();
        let other = Connection::open(&p).unwrap();
        other.execute_batch("begin immediate").unwrap();
        let e = [Event { seq: 2, at: 1, body: Body::SessionTitled { title: "u".into() } }];
        let r = j.append(&s, &e);
        assert!(matches!(&r, Err(e) if e.transient() && e.busy()), "{r:?}");
        other.execute_batch("rollback").unwrap();
        j.append(&s, &e).unwrap();
        let again = j.append(&s, &e).unwrap_err();
        assert!(!again.transient(), "{again}");
    }
}
