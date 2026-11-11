# Security architecture

## Trust levels

| Component | Trust | Why |
|-----------|-------|-----|
| `axiom`, `axiomd`, bundled editor assets | trusted | we build and ship them |
| Workspace contents (source, build scripts, config) | **untrusted data** | opening a folder must be safe; *running* it is a user decision |
| Task/build processes | run with the user's full privileges (M1) | same as a terminal; isolation is opt-in later (ADR-0010) |
| Provider manifests | untrusted data, interpreted | a manifest cannot execute anything but its declared argv, each through the permission gate |
| Provider helper processes, language servers, MCP servers | untrusted code | separate processes; started only after the user enabled that provider/server |
| Model output, tool results, file contents read by an agent | untrusted data | text can never grant permissions (prompt-injection rule) |
| Pages shown in the preview (M5) | untrusted | separate WebEngine profile, no bridge object |

## Paths (M1, enforced in `axiom-workspace`)

`Workspace::resolve(path)`:
1. Reject unless `WorkspacePath::is_lexically_valid()` → `invalid_params`.
2. Join onto the canonical root.
3. Canonicalize the longest existing prefix (this resolves symlinks) and
   re-append the non-existing tail.
4. Reject unless the result starts with the canonical root →
   `permission_denied`.

Consequences: `..` never reaches the filesystem; a symlink inside the
workspace that points outside it cannot be read or written through the
protocol; `fs/list` reports symlinks as `symlink` and does not follow them.
There is a time-of-check/time-of-use window between step 3 and the file
operation; closing it needs `openat2`/`O_NOFOLLOW`-style APIs and is tracked
as a hardening item, not an M1 requirement.

## Editor surface (M1)

- Loaded only from `axiom://editor/…`, a custom scheme registered as
  `SecureScheme | CorsEnabled | FetchApiAllowed` (not `LocalAccessAllowed`).
  The handler serves files from the bundled asset directory only and rejects
  any path that escapes it (E1 implements this check).
- Navigation away from `axiom://editor/` is blocked
  (`onNavigationRequested`); links open in the system browser.
- The page gets exactly one QWebChannel object, `host`, whose slots are the
  bridge contract. The bridge exposes **no** generic file, process or RPC
  access: the page can only say "save this document you gave me".
- No remote content, no CDN: all assets are bundled.
- `QTWEBENGINE_REMOTE_DEBUGGING` is never set by release builds.

## Daemon endpoint

- M1: stdio of a child process. Only the parent can talk to it.
- M3+: local socket. Unix: created inside a `0700` directory under
  `$XDG_RUNTIME_DIR` (or the per-user temp dir on macOS), socket mode `0600`.
  Windows: named pipe with an ACL granting only the current user. Clients
  must present a per-launch token (read from a `0600` file next to the
  socket) in `initialize`. No TCP listener, ever, by default.

## Processes

- `task/start` with `argv` involves no shell. The `shell` form exists for the
  user's own typed commands and is labelled as such; agents (M3) get only the
  `argv` form through their tool unless policy allows otherwise.
- Environment passed to tasks is the daemon's environment plus the request's
  additions. From M3, variables matching the secret patterns
  (`*_TOKEN`, `*_KEY`, `*_SECRET`, `*PASSWORD*`, plus provider API key names)
  are stripped from agent-initiated tasks unless explicitly allowed.
- Every started task is recorded (command, cwd, time, initiator) in the audit
  log from M3.

## Agent permissions (M3; contract in `docs/protocols/agents.md`)

Each tool declares an *effect class*: `read`, `write_workspace`, `execute`,
`install`, `network`, `write_outside`, `delete`, `credentials`, `deploy`.
A policy maps effect class (and optional argument patterns) to
`allow | ask | deny`. Defaults: `read` → allow; `write_workspace` → ask (allow
after a checkpoint exists, if the user chose "auto-edit"); everything else →
ask; `credentials` and `deploy` → ask every time, not rememberable.
The gate lives in the tool registry in the daemon, so GUI-hosted agents, MCP
clients and ACP agents all pass through the same code.

API keys are stored in the OS keychain (`keyring` crate), are referenced by
name in settings, and are never returned by any protocol method or shown to a
model.

## Isolation (ADR-0010)

Axiom uses three distinct words and the UI must too:
- **Managed** — installed under Axiom's data directory. Says nothing about
  what the tool can do when run.
- **Contained** — runs in a container (Podman/Docker, Dev Container). Isolation
  strength is the container runtime's; we state which.
- **Sandboxed** — runs under an OS sandbox we configure (Landlock+seccomp or
  bubblewrap on Linux, Seatbelt on macOS, AppContainer on Windows) with a
  stated policy. Only offered where a startup probe proves the mechanism
  works; otherwise the option is absent, not silently downgraded.

## Supply chain

- Managed installs require a SHA-256 for every artifact (`provider.rs`
  `Checksum`); no checksum, no install. Archives are extracted with path
  traversal and link checks into a staging directory, then renamed.
- The editor bundle pins exact npm versions with a lockfile; Rust uses
  `Cargo.lock`; `cargo deny` (licences + advisories) is added in M1-CI-001.

## Known gaps (tracked, not hidden)

- M1 has no isolation at all for tasks: equivalent to a terminal.
- TOCTOU on path containment (above).
- WebEngine's Chromium lags upstream (Qt 6.11/6.12 ship Chromium 140 per E1's
  user agent); the editor surface loads no remote content, which bounds the
  exposure; the M5 preview does load arbitrary pages and must use an isolated
  profile and track Qt security releases.
