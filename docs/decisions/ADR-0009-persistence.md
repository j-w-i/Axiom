# ADR-0009: Persistence

Status: Accepted (2026-11-11)

## Decision
| Data | Store | Location |
|------|-------|----------|
| Application settings, recent workspaces, per-workspace UI state, session index, audit log | SQLite (`rusqlite`, bundled), owned exclusively by `crates/store` | `<data dir>/axiom.db` |
| Project configuration that should travel with the code (environment selection, tasks, agent policy overrides, preview config) | TOML, human-edited, committed | `<workspace>/.axiom/project.toml` (+ generated `project.lock`) |
| Agent transcripts | JSONL, append-only | `<data dir>/sessions/…` |
| Checkpoints | shadow git repositories | `<data dir>/checkpoints/…` |
| Managed toolchains | plain directories | `<data dir>/toolchains/…` |
| Download cache, screenshots | files | `<cache dir>/…` |
| Secrets | OS keychain | – |
| Window geometry | `QSettings` in the shell | platform default |

`<data dir>` / `<cache dir>` come from the `directories` crate
(`ProjectDirs::from("org", "axiom", "Axiom")`) and can be overridden with
`AXIOM_DATA_DIR` / `AXIOM_CACHE_DIR` (tests always override).

SQLite schema is versioned with `PRAGMA user_version` and forward-only
migrations embedded in `crates/store`. Values are stored as JSON text. WAL
mode. One writer (the daemon).

Nothing important lives only in the database: deleting `axiom.db` loses
preferences and history, never project configuration or code.

## M1 schema (version 1)
```sql
CREATE TABLE settings        (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE recent_workspace(root TEXT PRIMARY KEY, name TEXT NOT NULL, last_opened_ms INTEGER NOT NULL);
CREATE TABLE workspace_state (root TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY (root, key));
```
Workspace state is keyed by canonical root path, not by `WorkspaceId`
(ids are per daemon run).

## Alternatives considered
- *JSON files for settings.* Simple, but concurrent writers (multiple
  windows/clients from M3) and partial writes are real problems SQLite
  solves.
- *sqlx.* Async and compile-time checked queries, but heavier and unneeded
  for a handful of statements.
