//! Durable storage for settings, recent workspaces and per-workspace UI state
//! (ADR-0009). Synchronous; independent of the wire protocol.

use rusqlite::{params, Connection, ErrorCode, OptionalExtension};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Largest serialized value accepted, in bytes.
const MAX_VALUE_BYTES: usize = 1024 * 1024;

/// Forward-only migrations, ordered by version. Later tasks append here.
const MIGRATIONS: &[(u32, &str)] = &[(
    1,
    "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
     CREATE TABLE recent_workspace (root TEXT PRIMARY KEY, name TEXT NOT NULL, last_opened_ms INTEGER NOT NULL);
     CREATE TABLE workspace_state (root TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY (root, key));",
), (
    2,
    // M2-ENV-001: workspaces whose own providers (`.axiom/providers/`) the user trusts. Kept
    // apart from `workspace_state`, which clients write freely through `workspace/setState`.
    "CREATE TABLE workspace_trust (root TEXT PRIMARY KEY, trusted_ms INTEGER NOT NULL);",
)];

fn latest_version() -> u32 {
    MIGRATIONS.last().map_or(0, |(v, _)| *v)
}

#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error("database schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("value too large: {key} is {bytes} bytes (limit {limit})")]
    TooLarge {
        key: String,
        bytes: usize,
        limit: usize,
    },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recent {
    pub root: String,
    pub name: String,
    pub last_opened_ms: u64,
}

pub struct Store {
    conn: Mutex<Connection>,
    recovered: Option<PathBuf>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("recovered", &self.recovered)
            .finish_non_exhaustive()
    }
}

/// Default database path: `$AXIOM_DATA_DIR/axiom.db` if set, else the
/// platform data directory for `org.axiom.Axiom`.
pub fn default_db_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("AXIOM_DATA_DIR") {
        return PathBuf::from(dir).join("axiom.db");
    }
    match directories::ProjectDirs::from("org", "axiom", "Axiom") {
        Some(d) => d.data_dir().join("axiom.db"),
        None => PathBuf::from("axiom.db"),
    }
}

fn is_corruption(e: &StoreError) -> bool {
    matches!(
        e,
        StoreError::Sqlite(rusqlite::Error::SqliteFailure(f, _))
            if matches!(f.code, ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
    )
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// Opens, validates and migrates. Does not modify a too-new database.
fn init(conn: &Connection) -> Result<(), StoreError> {
    let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > latest_version() {
        return Err(StoreError::SchemaTooNew {
            found: version,
            supported: latest_version(),
        });
    }
    let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(StoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
            Some(check),
        )));
    }
    // `journal_mode` returns the resulting mode as a row.
    conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
    conn.execute_batch("PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;")?;
    for (v, sql) in MIGRATIONS.iter().filter(|(v, _)| *v > version) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {v};"))?;
        tx.commit()?;
    }
    Ok(())
}

fn open_checked(path: &Path) -> Result<Connection, StoreError> {
    let conn = Connection::open(path)?;
    init(&conn)?;
    Ok(conn)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

impl Store {
    /// Opens or creates the database file, applies migrations. A corrupt file
    /// is moved aside (see [`Store::recovered_from_corruption`]).
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        match open_checked(path) {
            Ok(conn) => Ok(Self::wrap(conn, None)),
            Err(e) if is_corruption(&e) => {
                let ts = now_unix_ms();
                let moved = sibling(path, &format!(".corrupt-{ts}"));
                std::fs::rename(path, &moved)?;
                for suffix in ["-wal", "-shm"] {
                    let side = sibling(path, suffix);
                    if side.exists() {
                        std::fs::rename(&side, sibling(&moved, suffix))?;
                    }
                }
                let conn = open_checked(path)?;
                Ok(Self::wrap(conn, Some(moved)))
            }
            Err(e) => Err(e),
        }
    }

    pub fn open_in_memory() -> Result<Store, StoreError> {
        let conn = Connection::open_in_memory()?;
        init(&conn)?;
        Ok(Self::wrap(conn, None))
    }

    fn wrap(conn: Connection, recovered: Option<PathBuf>) -> Store {
        Store {
            conn: Mutex::new(conn),
            recovered,
        }
    }

    /// Where the corrupt database was moved to, if recovery happened on open.
    pub fn recovered_from_corruption(&self) -> Option<PathBuf> {
        self.recovered.clone()
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn setting_get(&self, key: &str) -> Result<Option<Value>, StoreError> {
        let text: Option<String> = self
            .conn()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        text.map(|t| serde_json::from_str(&t).map_err(Into::into))
            .transpose()
    }

    pub fn settings_all(&self) -> Result<BTreeMap<String, Value>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        collect_values(rows)
    }

    /// `None` deletes the key. All changes in one transaction.
    pub fn settings_set(&self, values: &BTreeMap<String, Option<Value>>) -> Result<(), StoreError> {
        let encoded = encode_all(values)?;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for (key, text) in &encoded {
            match text {
                Some(t) => tx.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, t],
                )?,
                None => tx.execute("DELETE FROM settings WHERE key = ?1", [key])?,
            };
        }
        tx.commit()?;
        Ok(())
    }

    /// Inserts or updates the entry with the given time.
    pub fn recent_touch(&self, root: &str, name: &str, now_ms: u64) -> Result<(), StoreError> {
        self.conn().execute(
            "INSERT INTO recent_workspace (root, name, last_opened_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(root) DO UPDATE SET name = excluded.name,
                                             last_opened_ms = excluded.last_opened_ms",
            params![root, name, now_ms as i64],
        )?;
        Ok(())
    }

    /// Newest first, at most `limit`. Does not check the filesystem.
    pub fn recent_list(&self, limit: usize) -> Result<Vec<Recent>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT root, name, last_opened_ms FROM recent_workspace
             ORDER BY last_opened_ms DESC, root ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            Ok(Recent {
                root: r.get(0)?,
                name: r.get(1)?,
                last_opened_ms: r.get::<_, i64>(2)? as u64,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn recent_remove(&self, root: &str) -> Result<(), StoreError> {
        self.conn()
            .execute("DELETE FROM recent_workspace WHERE root = ?1", [root])?;
        Ok(())
    }

    /// Whether the user trusts this workspace's own providers.
    pub fn workspace_trusted(&self, root: &str) -> Result<bool, StoreError> {
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM workspace_trust WHERE root = ?1",
            [root],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// Records (or withdraws) trust in this workspace's own providers.
    pub fn set_workspace_trusted(
        &self,
        root: &str,
        trusted: bool,
        now_ms: u64,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        if trusted {
            conn.execute(
                "INSERT INTO workspace_trust (root, trusted_ms) VALUES (?1, ?2)
                 ON CONFLICT(root) DO UPDATE SET trusted_ms = excluded.trusted_ms",
                params![root, now_ms as i64],
            )?;
        } else {
            conn.execute("DELETE FROM workspace_trust WHERE root = ?1", [root])?;
        }
        Ok(())
    }

    /// Empty `keys` returns every key for the workspace.
    pub fn workspace_state_get(
        &self,
        root: &str,
        keys: &[String],
    ) -> Result<BTreeMap<String, Value>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT key, value FROM workspace_state WHERE root = ?1")?;
        let rows = stmt.query_map([root], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut all = collect_values(rows)?;
        if !keys.is_empty() {
            all.retain(|k, _| keys.contains(k));
        }
        Ok(all)
    }

    /// `None` deletes the key. All changes in one transaction.
    pub fn workspace_state_set(
        &self,
        root: &str,
        values: &BTreeMap<String, Option<Value>>,
    ) -> Result<(), StoreError> {
        let encoded = encode_all(values)?;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for (key, text) in &encoded {
            match text {
                Some(t) => tx.execute(
                    "INSERT INTO workspace_state (root, key, value) VALUES (?1, ?2, ?3)
                     ON CONFLICT(root, key) DO UPDATE SET value = excluded.value",
                    params![root, key, t],
                )?,
                None => tx.execute(
                    "DELETE FROM workspace_state WHERE root = ?1 AND key = ?2",
                    params![root, key],
                )?,
            };
        }
        tx.commit()?;
        Ok(())
    }
}

/// Serialises every value up front so an oversized one aborts the whole batch
/// before anything is written.
fn encode_all(
    values: &BTreeMap<String, Option<Value>>,
) -> Result<Vec<(&String, Option<String>)>, StoreError> {
    values
        .iter()
        .map(|(k, v)| {
            let text = match v {
                Some(v) => {
                    let t = serde_json::to_string(v)?;
                    if t.len() > MAX_VALUE_BYTES {
                        return Err(StoreError::TooLarge {
                            key: k.clone(),
                            bytes: t.len(),
                            limit: MAX_VALUE_BYTES,
                        });
                    }
                    Some(t)
                }
                None => None,
            };
            Ok((k, text))
        })
        .collect()
}

fn collect_values(
    rows: impl Iterator<Item = rusqlite::Result<(String, String)>>,
) -> Result<BTreeMap<String, Value>, StoreError> {
    let mut out = BTreeMap::new();
    for row in rows {
        let (k, v) = row?;
        out.insert(k, serde_json::from_str(&v)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
