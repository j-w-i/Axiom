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
| Pages shown in the preview (M5) | untrusted | isolated webview/profile, no bridge object, no host commands |

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

## Managed installs (M2, enforced in `axiom-env`; as built by M2-ENV-002)

Layout under the data directory: `cache/<sha256>` (verified downloads, content-addressed;
`cache/.part-<planId>` while downloading) and `toolchains/<provider>/<version>/` (published
installs; `toolchains/<provider>/.staging-<version>-<planId>/` while extracting). Nothing else
is written.

| Rule | How | Test (`crates/env/tests/install.rs` unless noted) |
|------|-----|------|
| Approval is a protocol fact | `env/install` takes only a `planId`; the daemon executes its own stored copy of the plan, once, within 10 minutes, and only if the provider's manifest is unchanged; otherwise `approval_denied` | `plan_immutable_and_expiring`; `axiomd/tests/install.rs` (spent plan) |
| HTTPS only | URLs come only from a validated manifest; non-HTTPS URLs and redirects to them are refused; the OS trust store (rustls, no native TLS). The test-only `AXIOM_TEST_INSECURE_LOOPBACK_HTTP=1` allows plain HTTP to loopback hosts only | `fetch::tests::only_https_unless_testing`; `axiomd/tests/install.rs` (refused without the switch) |
| Verify before use | The download is hashed while streaming and compared with the manifest's digest (or the checksum file's entry) before it is renamed into the cache; nothing is extracted or run before that. A cached file is re-hashed before reuse | `checksum_mismatch_cleans_up` |
| Safe extraction | Our own path logic, never the library's unpacker: absolute paths, drive prefixes and `..` rejected; no entry written through a symbolic link; link targets relative and inside the root (lexically, from the link's directory); hard links only to earlier regular files; special files skipped; only `rwx` mode bits kept | `traversal_rejected`, `symlink_escape_rejected`, `archive::tests` |
| Atomic publish | Extract and verify in the staging directory, then one `rename`; any failure or cancellation removes staging and the partial download | `install_publishes_atomically`, `cancel_removes_staging`; `axiomd/tests/install.rs` (cancel through `$/cancel`) |
| Uninstall removes exactly one directory | Provider id and version validated as names; the version path must be a real directory whose canonical parent is the provider directory under the toolchains root (a link out of the root is refused, its target kept) | `uninstall_removes_only_root` (before/after snapshot) |

Installs run as their own task in the daemon, so a cancelled request that the RPC layer stops
waiting for still completes its own cleanup.

## System installs (M2, `axiom-env` `system`; as built by M2-ENV-004)

| Rule | How | Test (`crates/env/tests/system.rs`) |
|------|-----|------|
| Never by default | A `system_package` method is planned only when the client names it (`method`); default selection skips it | `plan_lists_exact_argv` (default → `unsupported_platform`) |
| The exact command is in the plan | `SystemPackages.argv` is the full command including the elevation wrapper; `scope: system`; effects `systemChange` and `elevation`. Non-interactive flags (`-y`, `--noconfirm`, winget's agreement flags) are part of that argv, so the user sees them | `plan_lists_exact_argv` |
| Only approved plans run | The same single-use, 10-minute `planId` as managed installs; planning runs nothing | `runs_only_approved_plan` (shim log empty until the approved install) |
| Elevation is the OS's prompt | Linux: `pkexec` (polkit's dialog); without pkexec, `sudo -A` only when `SUDO_ASKPASS` names the desktop's askpass helper, else `prerequisite_missing`. macOS: Homebrew unelevated (it refuses root). Windows: winget, which raises UAC itself. Axiom never asks for, sees or stores a password, and never uses `sudo` without `-A` | `plan_lists_exact_argv` (pkexec, sudo -A, refusal, brew, winget) |
| Manager choice | The distribution's own manager (os-release `ID`/`ID_LIKE`) when present, else the first of apt, dnf, pacman; brew on macOS; winget on Windows | `detects_manager` |
| Verified | Each required tool is re-detected on `PATH` afterwards | `runs_only_approved_plan`; CI: `ci_install.rs` `system_package_install_through_pkexec_shim` (real apt `hello` on Ubuntu) |

## Shell webview (Tauri, M1.5)

- Assets are bundled and served by Tauri's asset protocol from `dist/`; there
  is no custom scheme handler of ours and no filesystem path reaches the page.
- **CSP** (in `tauri.conf.json`): `default-src 'self'`, `script-src 'self'`,
  `style-src 'self' 'unsafe-inline'` (Monaco injects styles),
  `worker-src 'self' blob:` (Monaco workers), `connect-src ipc:
  http://ipc.localhost`, `object-src 'none'`, `base-uri 'none'`,
  `frame-src 'none'`. No remote origin is allowed.
- **Host commands** (capability-gated, `AppManifest::commands` pins the list):

  | Command | Exposes |
  |---------|---------|
  | `host_subscribe(channel)` | the ordered event stream (state, notifications, responses, logs, close-requested) |
  | `daemon_request(key, method, params, timeoutMs?)` | one protocol 0.1 request; `initialize`/`shutdown` are refused (the host owns the lifecycle) |
  | `daemon_cancel(key)` | `$/cancel` for a forwarded request |
  | `daemon_restart()` | replace the daemon process |
  | `app_info()` | versions, data dir, uptime (read-only) |
  | `open_external(url)` | system browser, `http`/`https` only |
  | `app_quit()` | confirmed quit |
  | `host_heartbeat()` | proves the page is alive (crash/hang detection) |
  | `snapshot_put(path, text)` / `snapshot_clear(path)` | the latest *unsaved* text of a document, held in host memory only (never on disk, never read back into the file by the host) |
  | `recovery_state()` | those snapshots, for a page that must rebuild after a crash |

  Besides these, the capability (`capabilities/main.json`, pinned by
  `src-tauri/tests/surface.rs`) grants these Tauri core/plugin permissions:
  `dialog:allow-open` (the folder picker), `core:event:allow-listen` (host
  events), `core:app:allow-version`, and since M1.6-SURFACES-003
  `core:window:allow-set-theme`: the page may set **its own window's**
  title-bar theme (light, dark or follow the system) so the native title bar
  matches the app on Windows and Linux (D3). It reads nothing and affects only
  that window's decorations. Since M1.6 (macOS overlay title bar, `faea29e`) also
  `core:window:allow-start-dragging` and `core:window:allow-internal-toggle-maximize`: the page may
  start a drag of its own window from the title row and toggle its zoom with a double-click there
  (Tauri's drag-region script). Neither reads anything or reaches beyond that window.

  There is **no** command that reads or writes workspace files, spawns a
  process, opens an arbitrary path, or grants network access.
- **Wider than Qt by design:** in the Qt shell only C++ could issue protocol
  requests; here the UI page can issue any protocol method a client may issue
  (including `fs/writeFile` and `task/start`). The page is our own bundled
  code with no remote content, which bounds the exposure; spike T-11 evaluates
  isolating Monaco in its own webview with a narrower capability.
- `freezePrototype` is disabled because Monaco 0.57 assigns to built-in
  prototypes at load (observed: `Attempted to assign to readonly property`).
- Links open in the system browser; navigation away is impossible (single
  page, no `frame-src`).
- Webview remote debugging is never enabled by a release build.

## Editor surface (Qt, until T-15)

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

## Agent permissions (M3; contract in `docs/protocols/agents.md`; as built by M3-GATE-001)
- **One gate for every call.** Every tool call, from a session, `agent/toolCall` (MCP front
  end, CLI) or, later, an ACP agent's request, goes through `axiom_agent::gate::call_tool`:
  prepare (validate, resolve targets inside the workspace) → policy → ask → hook (the
  checkpoint) → execute → audit.
  - Tools run only with an `Allowed` token, which only the gate can create (`pub(crate)`). The
    dev-dependency feature `test-support` exposes a constructor to tests.
- **Policy.** The user's comes from settings `agent.policy`; built in: ask, reads allowed.
  `.axiom/project.toml` `[agent.policy]` can only make a decision stricter (the stricter of
  the two wins).
- **Asking.** `ask` goes to connections that declared `capabilities.approvals`. With none, the
  call fails `approval_required` and does not run. The first answer wins.
- **"Allow for session".** It adds an exact rule (this tool, this argv or this single path)
  for that principal only; never for `credentials`/`deploy`.
- **Principals.** `agent/toolCall` accepts only `cli` and `mcp:<name>`, so an external client
  can never use a session's grants.
- **Audit log.** `<data dir>/audit/<workspace key>.jsonl`, append-only: time, principal, tool,
  effect, targets, decision, rule, outcome, duration. Tool inputs and outputs are not logged
  (they may hold secrets); only a one-line summary.
- **Limit.** Approval is consent, not isolation: an allowed `task.run` runs with the user's
  rights (ADR-0010 isolation backends are M6).

## Model credentials (M3; as built by M3-MODEL-002)
- API keys live only in the OS credential store (`keyring` 3.6: Secret Service over pure-Rust
  D-Bus on Linux, Keychain on macOS, Credential Manager on Windows), service `axiom`, account =
  provider id; or the provider names an environment variable (`apiKeyEnv`) read by the daemon.
- `agent.providers` (settings) holds configurations only; `agent/models` reports `hasKey`, never
  the key. Adapters' `Debug` output redacts keys; errors and logs never include them
  (integration test `agent_models…keeps_the_key_out_of_settings_and_logs` scans the data dir and
  a debug log).
- Model base URLs are HTTPS, or HTTP to a loopback host (local servers).
- Credential calls run on their own thread (the Secret Service backend drives its own executor).
- TEST ONLY: `AXIOM_TEST_CREDENTIALS_FILE` replaces the store with a plain JSON file (daemon
  integration tests); `AXIOM_TEST_SCRIPTED_MODEL` offers the scripted model.

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


## Test-only code in the desktop shell

The end-to-end suites drive the app through an **embedded W3C WebDriver
server** (`tauri-plugin-wdio-webdriver`, MIT, pinned `=2.0.0-next.0`). It can
run arbitrary JavaScript in the page, so it is treated as a capability that
must never reach a user:

- It is an **optional** dependency, enabled only by the `e2e` Cargo feature;
  the default feature set does not include it.
- It listens on **127.0.0.1** only (verified in its source and with `ss` on a
  running instance), on a port the test harness chooses.
- It registers **no IPC commands** and no capability permissions, so the
  page's command surface (`capabilities/main.json`, `tests/surface.rs`) is the
  same with and without it.
- `src-tauri/tests/release_guard.rs` fails if the manifest or the resolved
  dependency graph of a default build includes it (shown to fail by enabling
  it in `default`).
- `scripts/bundle.sh` never passes a feature and **refuses to package** a
  binary that contains the server (shown to detect an `e2e` build).

## Known gaps (tracked, not hidden)

- M1 has no isolation at all for tasks: equivalent to a terminal.
- TOCTOU on path containment (above).
- The shell now depends on three web engines (WebKitGTK, WebView2, WKWebView)
  whose patch cadence it does not control. The UI loads no remote content,
  which bounds the exposure. The M5 preview *does* load arbitrary pages and
  must use an isolated profile/webview; its engine on Tauri is unresolved
  (ADR-0012 Consequences, spike M5-PRE-001).
