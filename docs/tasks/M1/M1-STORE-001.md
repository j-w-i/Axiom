---
id: M1-STORE-001
title: Implement the axiom-store crate (SQLite persistence)
status: tested
status_note: 2026-11-11; see handoff
depends_on: []
order: 11
scope: crates/store/, Cargo.toml (workspace members and dependencies only)
---

# M1-STORE-001: Implement the `axiom-store` crate

## Objective
Durable storage for settings, recent workspaces and per-workspace UI state,
with schema versioning and corruption recovery.

## Context
Read: `docs/decisions/ADR-0009-persistence.md` (schema is given there),
`docs/architecture/modules.md` §`crates/store`.

## Implementation requirements
```rust
pub struct Store { /* owns one rusqlite::Connection behind a Mutex */ }

impl Store {
    /// Opens or creates the database file, applies migrations.
    pub fn open(path: &Path) -> Result<Store, StoreError>;
    pub fn open_in_memory() -> Result<Store, StoreError>;

    pub fn setting_get(&self, key: &str) -> Result<Option<serde_json::Value>, StoreError>;
    pub fn settings_all(&self) -> Result<BTreeMap<String, serde_json::Value>, StoreError>;
    /// `None` deletes the key. All changes in one transaction.
    pub fn settings_set(&self, values: &BTreeMap<String, Option<serde_json::Value>>) -> Result<(), StoreError>;

    /// Inserts or updates the entry with the current time.
    pub fn recent_touch(&self, root: &str, name: &str, now_ms: u64) -> Result<(), StoreError>;
    /// Newest first, at most `limit`.
    pub fn recent_list(&self, limit: usize) -> Result<Vec<Recent>, StoreError>;
    pub fn recent_remove(&self, root: &str) -> Result<(), StoreError>;

    pub fn workspace_state_get(&self, root: &str, keys: &[String]) -> Result<BTreeMap<String, serde_json::Value>, StoreError>; // empty keys = all
    pub fn workspace_state_set(&self, root: &str, values: &BTreeMap<String, Option<serde_json::Value>>) -> Result<(), StoreError>;
}

pub struct Recent { pub root: String, pub name: String, pub last_opened_ms: u64 }

#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error("database schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("value too large: {key} is {bytes} bytes (limit {limit})")]
    TooLarge { key: String, bytes: usize, limit: usize },
    #[error(transparent)] Sqlite(#[from] rusqlite::Error),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error(transparent)] Io(#[from] std::io::Error),
}

/// Default database path: $AXIOM_DATA_DIR/axiom.db if set, else
/// directories::ProjectDirs::from("org", "axiom", "Axiom").data_dir()/axiom.db.
pub fn default_db_path() -> PathBuf;
```

1. Migrations: `PRAGMA user_version`; version 1 creates the three tables of
   ADR-0009. Structure the code as an ordered list of `(version, sql)` so
   later tasks append.
2. On open: `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`.
3. `user_version` greater than the newest known migration → `SchemaTooNew`
   (do not modify the file).
4. Corruption: if opening or `PRAGMA quick_check` fails with a
   corruption-class error, rename the file to `axiom.db.corrupt-<unix ms>`
   (and its `-wal`/`-shm` siblings), create a fresh database, and expose
   `store.recovered_from_corruption() -> Option<PathBuf>` so the daemon can
   log it once.
5. Values are stored as compact JSON text. A serialized value over 1 MiB →
   `TooLarge` and nothing in that call is written.
6. `recent_list` does not check the filesystem (the daemon filters missing
   directories).
7. Creates the parent directory of the database if missing.

## Constraints
- Dependencies: `rusqlite` (feature `bundled`), `serde_json`, `thiserror`,
  `directories`. Dev: `tempfile`.
- Must not depend on `axiom-protocol` (see `modules.md`: persistence is
  independent of the wire format).
- Synchronous API; no Tokio.

## Acceptance criteria
- Round trips for all three areas, including delete-by-`None` and
  "empty keys = all".
- Data survives closing and reopening the file.
- Tests for: `SchemaTooNew` (file left untouched, byte-identical),
  corruption recovery (write garbage over a database file; open succeeds;
  `.corrupt-*` file exists; fresh store is empty), `TooLarge` atomicity
  (a batch with one oversized value writes none of the batch),
  `recent_list` ordering and limit, `default_db_path` honouring
  `AXIOM_DATA_DIR`.

## Tests
`scripts/check.sh rust` passes. `cargo test -p axiom-store` shows ≥ 10 tests.

## Failure conditions
`rusqlite` with `bundled` fails to build on the host: report the compiler
error; do not switch to a system SQLite or another crate without a CR.

## Documentation updates
`docs/roadmap/current.md`.

## Handoff
Record the `rusqlite` version chosen and the bundled SQLite version
(`SELECT sqlite_version()`).
