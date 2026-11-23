# Security architecture

## Trust levels

| Component | Trust | Why |
|-----------|-------|-----|
| `axiom`, `axiomd`, bundled fonts and icons | trusted | we build and ship them |
| Workspace contents (source, build scripts, config) | **untrusted data** | opening a folder must be safe; *running* it is a user decision |
| Task/build processes | run with the user's full privileges (M1) | same as a terminal; isolation is opt-in later (ADR-0010) |
| Provider manifests | untrusted data, interpreted | a manifest cannot execute anything but its declared argv, each through the permission gate |
| Provider helper processes, language servers, MCP servers | untrusted code | separate processes; started only after the user enabled that provider/server; a built-in language server that would run the workspace's code waits for the folder to be trusted (§Terminals and language servers) |
| Model output, tool results, file contents read by an agent | untrusted data | text can never grant permissions (prompt-injection rule) |
| Pages shown in the preview (M5) | untrusted | opened in the person's browser, not in the shell (ADR-0020 §8); the injected adapter talks only to the daemon |

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

**Between resolving and acting** (a local process that swaps a directory for a symlink; review
N14 and R5, as built in `axiom-workspace`):
- Every file operation (`write_text`, `remove_file`, `create_file`, `create_dir`, `rename`,
  `trash`) resolves its path again inside its per-path lock, right before acting
  (`Workspace::recheck`). A path that now leads elsewhere is refused (`permission_denied` when
  outside the workspace, else `conflict` with no `currentVersion`), never redirected.
- On Unix the operation's directory is then opened from the root one component at a time with
  `O_NOFOLLOW` (`beneath.rs`), and the operation acts relative to it: the version check reads
  with `openat`, the new text goes to an exclusively created temporary file renamed into place
  (`renameat`). A symlink swapped in after the re-check makes the operation fail. Writes
  replace the entry, so a hard link (to a git hook, say) is never written through.
- The agent's writes (`fs.applyPatch`, `acp.writeFile`) also refuse unless the path still
  resolves to the one the gate judged and the card showed (`write_text_checked`,
  `remove_file_checked`), so a swap while the person reads the card cannot turn an approved
  ordinary path into `.git/hooks/…`.
- Checkpoint restores (`axiom_agent::checkpoint`, outside `Workspace`) write a new file beside
  the entry and rename it over, after re-checking containment: no hard link is written through.

What remains (defence in depth against redirection, not a sandbox; a process that can already
write the workspace can usually write `.git/hooks` itself):
- Windows: only the re-check applies, so a swap between the re-check and the operation is not
  caught.
- Unix, still by path: `create_dir` with `parents` creates the intermediate directories by
  path (the last one no-follow); `trash` moves by path after its re-check; `fs/list` and the
  LSP's reads of files outside the workspace (`read_regular_file`) read by path; checkpoint
  restores check containment, that no directory on the way is a symlink and that the path is
  not inside a git directory (before anything changes, and again right before the rename),
  then rename by path.
- Descendants of an external agent or MCP server that leave its process group (`setsid`, a
  daemonising launcher) are neither killed with it nor watched.
- The root itself is opened by its canonical path: swapping the root or an ancestor is out of
  scope.
- Reads through hard links and bind mounts: containment is by path, so a hard link inside the
  workspace to a file outside it (the person's credentials, say), or a bind mount, is read by
  `fs/readFile` and the agent's read tools like any workspace file. Telling them apart needs
  the file's identity (device, inode, link count) checked against the workspace, which is not
  done; writes are not affected (they replace the entry).

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
| Bounded (AUD-108) | One extraction writes at most 50 × the archive's size (between 1 GiB and 32 GiB) and a million entries, else `UnsafeArchive`; a download is at most its announced `Content-Length` plus 1 MiB, and never more than 8 GiB | `archive::tests::a_decompression_bomb_stops_at_the_limit` |
| Checksum files are pinned on first use (AUD-107) | A `sha256_url` checksum comes from the artifact's own origin, so it detects corruption, not a compromised release. The digest first verified for an artifact URL is kept in `cache/pins.json`; a later, different published digest for the same URL is refused (`ChecksumMismatch`). Built-in bootstraps name one release in their URLs (uv 0.9.0, rustup 1.28.2), never a moving `latest`; Node and CMake name the version. Residual: the first install of a URL trusts its origin's TLS and checksum file | `a_changed_checksum_file_for_the_same_url_is_refused`; `provider_contract.rs` `no_artifact_or_checksum_tracks_latest` |
| Private working directory (AUD-044) | Version probes, `list_versions` programs and system package managers run in `<data dir>/work` (`0700`, kept empty), never the shared temp directory, where another local user could plant a `rust-toolchain.toml` (its `path` names a toolchain to run), `global.json`, `.npmrc` or `go.work` | `detect::tests::probes_do_not_run_in_the_shared_temp_dir`, `tests::the_work_dir_is_private_and_not_the_shared_temp_dir` |
| Bounded program output (AUD-109) | Installer and `list_versions` output is read line by line, each line cut at 64 KiB | `install::tests::program_output_lines_are_bounded` |
| Atomic publish | Extract and verify in the staging directory, then one `rename`; any failure or cancellation removes staging and the partial download | `install_publishes_atomically`, `cancel_removes_staging`; `axiomd/tests/install.rs` (cancel through `$/cancel`) |
| Uninstall removes exactly one directory | Provider id and version validated as names; the version path must be a real directory whose canonical parent is the provider directory under the toolchains root (a link out of the root is refused, its target kept) | `uninstall_removes_only_root` (before/after snapshot) |

Installs run as their own task in the daemon, so a cancelled request that the RPC layer stops
waiting for still completes its own cleanup.

## System installs (M2, `axiom-env` `system`; as built by M2-ENV-004)

| Rule | How | Test (`crates/env/tests/system.rs`) |
|------|-----|------|
| Never by default | A `system_package` method is planned only when the client names it (`method`); default selection skips it | `plan_lists_exact_argv` (default → `unsupported_platform`) |
| The exact command is in the plan | `SystemPackages.argv` is the full command including the elevation wrapper; `scope: system`; effects `systemChange` and `elevation`. Non-interactive flags (`-y`, `--noconfirm`, winget's agreement flags) are part of that argv, so the user sees them | `plan_lists_exact_argv` |
| No package is an option (AUD-105) | A manifest's package names must start with a letter or digit and hold only letters, digits and `+._:@/-` (`manifest::valid_package`); apt and pacman get `--` before the packages (dnf is left without it: dnf5's parser was not verified to accept it, and the names are validated) | `manifest::tests::system_package_names_are_validated`, `system::tests::packages_follow_an_end_of_options_marker` |
| Only approved plans run | The same single-use, 10-minute `planId` as managed installs; planning runs nothing | `runs_only_approved_plan` (shim log empty until the approved install) |
| Elevation is the OS's prompt | Linux: `pkexec` (polkit's dialog); without pkexec, `sudo -A` only when `SUDO_ASKPASS` names the desktop's askpass helper, else `prerequisite_missing`. macOS: Homebrew unelevated (it refuses root). Windows: winget, which raises UAC itself. Axiom never asks for, sees or stores a password, and never uses `sudo` without `-A` | `plan_lists_exact_argv` (pkexec, sudo -A, refusal, brew, winget) |
| Manager choice | The distribution's own manager (os-release `ID`/`ID_LIKE`) when present, else the first of apt, dnf, pacman; brew on macOS; winget on Windows | `detects_manager` |
| Verified | Each required tool is re-detected on `PATH` afterwards | `runs_only_approved_plan`; CI: `ci_install.rs` `system_package_install_through_pkexec_shim` (real apt `hello` on Ubuntu) |

## The shell process (Flutter, since FL-QA-001)

- **No web view.** The shell renders its own widgets (ADR-0020); it loads no HTML, runs no
  JavaScript and has no CSP, asset protocol or host-command list. Its assets (fonts, icons) are
  bundled in the build.
- **One native process with the person's rights.** Unlike the Tauri page, the UI code is not
  sandboxed from the file system: what keeps it from touching workspace files or running
  processes is the architecture rule (`modules.md`, review), not a capability boundary. What it
  does outside the protocol:
  - spawns and supervises `axiomd` (`lib/daemon/client.dart`);
  - opens an `http`/`https` link the person clicked in the system browser, through the OS
    opener (`xdg-open`, `open`; `lib/app/open_link.dart`), refusing every other scheme;
  - shows the system folder picker and image picker (`file_selector`) and reads only the
    images the person picked (Chat attachments);
  - reads and writes its own layout file, `shell-local.json` (`lib/app/local_prefs.dart`).
- **Protocol access.** Any part of the UI can issue any method a client may issue (including
  `fs/writeFile` and `task/start`), as the Tauri page could; the code is our own, with no
  remote content.
- **Release builds.** The end-to-end suites (`integration_test/`) are compiled only by
  `flutter test` / `flutter drive`; the release build has no test driver, and Flutter's VM
  service (the debugging port) exists only in debug and profile builds. `--self-test` (package verification) only connects to the daemon, prints the
  result and quits.

## Shell webview (Tauri, M1.5; retired by FL-QA-001, tag `tauri-shell-final`)

The Tauri shell's controls, kept as the record of how it was built:

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

## Editor surface (Qt, retired by T-15)

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
- M3+: local socket (as built by M3-DAEMON-001).
  - Unix: `<data dir>/run/axiomd.sock`, mode `0600`, in the `0700` directory `run/`.
  - Clients must present the per-launch token (`run/axiomd.token`, `0600`, 32 random bytes
    from `/dev/urandom` as hex) in `initialize.authToken`. A connection without it, or with a
    wrong one, gets `permission_denied` and is closed before anything else is served.
  - Anyone who can read the token file (the same user) can drive the daemon: the same trust
    boundary as the user's files.
  - Windows named pipe: deferred.
  - No TCP listener, ever, by default.

## Processes

- `task/start` with `argv` involves no shell. The `shell` form exists for the
  user's own typed commands and is labelled as such; agents (M3) get only the
  `argv` form through their tool unless policy allows otherwise.
- Environment passed to tasks is the daemon's environment plus the request's
  additions. The person's own tasks, builds and terminals get it whole.
- **Agent-started processes do not get the daemon's secrets** (as built by AUD-010;
  `axiom_agent::secrets`). `task.run`, `build.run` and `acp.terminal` withhold every variable
  whose name ends in `_TOKEN`, `_KEY`, `_SECRET`, `_CREDENTIALS`, `_PAT`, `_AUTH` or
  `DATABASE_URL`, contains `PASSWORD` or `PASSWD`, or is `SSH_AUTH_SOCK` (any case; review N7),
  and every model provider's `apiKeyEnv` variable, unless settings
  `agent.passEnv` (a list of names, set by the person, never by a workspace) names it. The
  approval card says so ("environment: 3 secret variables withheld (GITHUB_TOKEN, …)"). The
  removal is done by the executor (`SpawnSpec::remove_key`). Withholding `SSH_AUTH_SOCK` means
  an agent's `git push` over SSH needs `agent.passEnv: ["SSH_AUTH_SOCK"]`: the SSH agent signs
  for any process that can reach its socket. An ACP terminal's own `env` entries
  are set as the agent asks, and are on the card (§External agents).
- External agents and MCP servers keep the daemon's environment (they sign in with it), except
  the `apiKeyEnv` variables: Axiom's model keys never reach a process it did not start for that
  provider. A provider or server that needs one names it in its own `env`.
- The agent's `preview.start` runs the dev server without the withheld variables too (a preview
  the person starts from the Inspector keeps the full environment).
- Toolchain installer commands (`env.install`, from the Environment view or an agent's call;
  `axiom_env::install::run_program`) get the daemon's full environment: nothing is withheld.
  What they run is the plan shown on the card (§Agent permissions).
- **The filter is hygiene, not a boundary.** It keeps secrets out of the obvious place (an
  agent's command printing its environment), and matches names only: a secret under a name it
  does not match passes. A process the agent starts runs as the same user as the daemon and can
  read the daemon's environment directly (`/proc/<daemon pid>/environ` on Linux,
  `ps eww <pid>` on macOS). Secrets an agent must not reach belong outside the daemon's
  environment (the credential store, §Model credentials); isolation is ADR-0010's (M6).
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
  the two wins). Path globs are matched in `O(pattern × path)` (AUD-106: the recursive matcher
  was exponential in the number of `*`, and the workspace's patterns are untrusted).
- **Protected paths** (AUD-104, review B4 and D-round). A write or delete under `.axiom/` (the
  workspace policy, its providers' tasks and language servers), a `.git` directory at any depth
  (hooks run on the person's next commit), a bare repository's `<name>.git/` (its `hooks/` run
  on a push to it), `.husky/` at any depth, or the directory the workspace's own `.git/config`
  names in `core.hooksPath` (when inside the workspace) always asks, whatever a policy rule, a
  session grant or an ACP permission says, and cannot be allowed for the session. The gate
  judges the path through symlinks inside the workspace, and the write refuses if it no longer
  resolves there (§Paths). Reading them is not restricted. Not covered: a `core.hooksPath` set
  by an included file, the global or system configuration, `git -c`/`GIT_CONFIG_*` or
  `config.worktree`; the configuration of a repository a `.git` file points to (worktrees,
  submodules); and a `hooksPath` naming the workspace root itself.
- **What the card shows is what is approved.** The targets are the paths, the argv (arguments
  with spaces or control characters quoted) and the call's *conditions*: an environment
  variable an ACP agent sets, a working directory other than the root, `build.run`'s provider
  and `PATH` prepend, an install's provider and version. A call whose conditions change what an
  approved command does (an agent-set `LD_PRELOAD`, `PATH`, `NODE_OPTIONS`, a working directory)
  asks even where a rule allows the bare argv (AUD-039). `build.run` refuses with `conflict` if
  the task resolved at execution differs from what was approved (AUD-103). `env.install` shows
  the plan (provider, version, every download and how it is verified, every command) and runs
  only a plan the same principal made with `env.planInstall`; never a system plan (AUD-041).
- **Asking.** `ask` goes to connections that declared `capabilities.approvals`. With none, the
  call fails `approval_required` and does not run. The first answer wins. The prompt is
  delivered to every approver concurrently with waiting for the answer and the cancel; each
  delivery waits at most `APPROVAL_DELIVERY` (10 s) for room on a full queue, and the call fails
  `approval_required` only when the delivery reached no approver. An approver that attaches
  meanwhile is kept, and the resolution is never waited for on a connection that is not reading
  (review R4, C3, D1).
- **"Allow for session"** (as built by AUD-009). It records exactly the approved call: the same
  tool, effect, paths, argv (not a prefix: `["rm", "x"]` never covers `["rm", "x", "-rf", "/"]`)
  and conditions, in the same workspace, for that principal only. It is not offered (and an
  answer "session" counts as "once") for `credentials`/`deploy`, protected paths, calls whose
  targets do not say what later calls may do (no path and no argv, other than reads) and
  *opaque* calls (an MCP tool, whose arguments decide what it does; an install). Grants end
  when the session is deleted, rewound (edit/regenerate) or restored to a checkpoint, when the
  connection of an external principal closes, and with the daemon.
  - Connection close is handled lazily: a closed connection's grants are dropped at the start
    of the next `agent/toolCall` (from any connection), before that call's principal is
    numbered, so a reused `#c<n>` never inherits them. Until then they stay in the daemon's
    memory, unusable, since no open connection has that principal.
- **Principals.** `agent/toolCall` accepts only `cli` and `mcp:<name>`, so an external client
  can never use a session's grants. The daemon makes the declared name the connection's own
  (`mcp:<name>#c<n>`, shown on cards and in the audit log): two clients that both call
  themselves `claude-code` share no grants.
- **Audit log.** `<data dir>/audit/<workspace key>.jsonl`, append-only: time, principal, tool,
  effect, targets, decision, rule, outcome, duration. Tool inputs and outputs are not logged
  (they may hold secrets); only a one-line summary. Records are written on the blocking pool,
  never on a runtime worker (AUD-031), and are on disk when the call returns.
- **Limit.** Approval is consent, not isolation: an allowed `task.run` runs with the user's
  rights (ADR-0010 isolation backends are M6).

## Model output in the shell (Chat; M3-UI-001, FL-CHAT-001)
- **Markdown.** Assistant text is parsed by the `markdown` package (GitHub-flavoured) and drawn
  with Axiom's own widgets (`lib/chat/markdown_view.dart`). There is no HTML renderer: raw HTML
  in model output stays text, never elements.
- **Links.** Only `http`/`https` links are tappable; they open through `openExternal` in the
  system browser. Other links are shown, not followed.
- **Images.** Images in Markdown are never loaded (the renderer draws no image element, so no
  tracking pixels).
- **Code blocks.** They are coloured with `re_highlight` into text spans; nothing is
  interpreted.
- **Images the user attaches.** They are read by the shell from an image file the user picks
  (PNG, JPEG, GIF or WebP, at most 5 MiB); they go to the daemon as base64 and are stored
  content-addressed next to the journal.
- Tests: `test/chat/markdown_test.dart` covers the structure; the HTML, link and image rules
  above have no Flutter test yet (the Tauri suite `tests/chat.test.tsx`, which covered them,
  retired with that shell).

## External agents (ACP; M3-ACP-001)
- **Inside the gate.** Their file reads, writes and terminals are gated tools (`acp.*`) under
  the session's principal, checkpointed and audited. Paths outside the workspace are refused.
- **Their permission prompts are Axiom's approval cards.** An approval the person gave covers,
  for that turn, the writes to the requested paths when it was for an edit or delete, and the
  next terminal only if it runs the command the permission showed (`rawInput.command`, as a line
  or an argv); a permission the policy allowed unseen (a `read`) covers nothing more (AUD-040).
- **Bounded.** An agent's output lines are at most 64 MiB (longer ends the connection); at most
  4096 of its messages wait unhandled (more are dropped with a warning); an ACP terminal keeps
  at most 16 MiB of output whatever `outputByteLimit` asks; non-string `args` are refused.
- **A dead agent is not reused.** An agent whose process ended (crashed, or killed by a cancel
  it ignored) is started again on the next turn; agents and MCP servers are started without
  holding the daemon-wide maps, so a slow one holds up no other session (AUD-032). A session's MCP
  servers and its external agent are stopped after 15 minutes unused; the agent only when it
  can resume the conversation (`loadSession`), otherwise it is kept so nothing is lost.
- **Credentials.** Axiom passes no credentials; agents use their own sign-in. Their processes
  get no stdin other than the ACP stream and are killed with the session or the daemon.
- **Limit.** What an agent does without the ACP fs/terminal methods is outside Axiom's gate.
  This is stated in ADR-0016 and must be stated in the approval UI (M3-UI-002).

## Model credentials (M3; as built by M3-MODEL-002)
- API keys live only in the OS credential store (`keyring` 3.6: Secret Service over pure-Rust
  D-Bus on Linux, Keychain on macOS, Credential Manager on Windows), service `axiom`, account =
  provider id; or the provider names an environment variable (`apiKeyEnv`) read by the daemon.
  That variable is withheld from every process Axiom starts for an agent, and from external
  agents and MCP servers (§Processes; AUD-010).
- `agent.providers` (settings) holds configurations only; `agent/models` reports `hasKey`, never
  the key. Adapters' `Debug` output redacts keys; errors and logs never include them
  (integration test `agent_models…keeps_the_key_out_of_settings_and_logs` scans the data dir and
  a debug log).
- Model base URLs are HTTPS, or HTTP to a loopback host (local servers).
- Credential calls run on their own thread (the Secret Service backend drives its own executor).
- TEST ONLY: `AXIOM_TEST_CREDENTIALS_FILE` replaces the store with a plain JSON file (daemon
  integration tests), written `0600` (AUD-109); `AXIOM_TEST_SCRIPTED_MODEL` offers the scripted
  model.
- Model streams: an SSE line or event over 16 MiB ends the stream (AUD-109).

## Terminals and language servers (M4; ADR-0018)
- **Terminals** run the person's own shell (`terminal/*`): they are not agent tools, pass no
  gate and are never offered to a model. They run in the workspace root with the project's
  environment; closing one hangs up its process group, and all end with the workspace or the
  daemon.
- **Language servers** are commands from provider manifests, resolved in the workspace's
  environment like build tools; they run with the person's rights, as the build does. The
  daemon refuses `workspace/applyEdit` (a server cannot change files by itself in 0.4).
- **Opening a folder runs none of its code** (as built by AUD-008; `axiom_lsp::restricted`).
  Until the person trusts the folder (`env/trust`, the same flag that gates `.axiom/providers`),
  a *built-in* server is one of:
  - as configured, when it only reads and parses: bash, clangd, docker, marksman, taplo, the
    HTML/CSS/JSON servers, yaml;
  - **restricted**: jdtls without Gradle or Maven import (`java.import.gradle.enabled`,
    `java.import.maven.enabled` false, in its initialization options and its settings). It does
    not turn annotation processing off, which jdtls has no setting for (its only one,
    `java.import.gradle.annotationProcessing.enabled`, is for the Gradle import that is already
    off): an Eclipse-format project (`.project`, `.classpath`) whose
    `.settings/org.eclipse.jdt.apt.core.prefs` enables APT can still run the processor JARs its
    `.factorypath` names, when jdtls builds or reconciles. This is a known residual;
    typescript-language-server with the TypeScript it was installed with (`tsserver.path`), never
    the folder's `node_modules/typescript`, and no plugins (refused if its own is not found);
    gopls with `GOTOOLCHAIN=local` (the `go.mod` `toolchain` line cannot fetch and run another
    Go). Its `detail` says "Restricted until you trust this folder: …" while it runs;
  - **not started**: rust-analyzer (build scripts, proc macros, `cargo check`; a repository's
    `rust-analyzer.toml`, `.cargo/config.toml` or `rust-toolchain.toml` can turn them back on or
    name the binaries it runs, so no option is a boundary), basedpyright (may run the folder's
    Python), csharp-ls (MSBuild evaluation), dart (analyzer plugins), kotlin-lsp (Gradle),
    sourcekit-lsp (`Package.swift`), and any built-in server not classified. It is `stopped`
    with a `detail` that starts "Trust this folder to enable"; requests fail at once with that
    message.
  Trusting the folder takes effect on the next `lsp/servers` or request: refused servers become
  `idle`, restricted ones restart as configured. Revoking trust applies from the next
  `lsp/servers` or `lsp/restart`. Servers the person added (user or workspace providers) run as
  configured: they are the person's choice. Residual: a trusted folder runs its code, as a build
  does; a restricted server's own bugs are not covered.
- **Reads outside the workspace** (`fs/readFile` with an absolute `file://` path) are allowed
  only for a path that one of the workspace's servers returned in a location result, read-only,
  so go-to-definition into a standard library works without opening the file system.

## Previews and the inspection adapter (M5; ADR-0019)
- **The adapter socket** is a WebSocket the daemon listens on at `127.0.0.1` only, one port per
  daemon. A page connects with its preview's **token** in the URL (128 random bits, minted by
  `preview/start`, given to the page only through the generated Vite config). An unknown or
  missing token is refused at the handshake (HTTP 401); so is an `Origin` other than an
  `http(s)` loopback one (`127.0.0.1`, `localhost`, `[::1]`; HTTP 403), so a website open in the
  person's browser cannot connect even with the token (AUD-043). The daemon then sends
  `inspect/hello`; only after its answer is the connection the preview's session. The token
  stays valid while the preview runs, so a reload reconnects; but a connection is adopted only
  once the previous page has gone (waiting up to 3 s for a reload's close): a second page never
  replaces a live one. `preview/stop` voids the token. The token is never logged or sent to a
  client.
- **The adapter module** (it carries the token) is served by the plugin only to the dev
  server's own page (`Sec-Fetch-Site: same-origin`/`none`, or no foreign `Origin`), never to
  another site's page through a permissive CORS setting (HTTP 403). The generated preview
  config (it holds the token) is written in a directory only the person can enter (`0700`).
- **Hover is cheap.** A hover event never runs the workspace text search (a click does, on the
  blocking pool, with results cached per page load; AUD-042).
- **The adapter is the user's app's code's peer, not trusted.** What it reports (tree, text,
  source stamps, errors) is data: source stamps are resolved only to paths inside the
  workspace (a stamp outside it is dropped), sizes are capped (5000 nodes, 200 characters of
  text per node), and nothing it sends is executed or shown as markup.
- **Injection is development-only and does not edit the project.** The dev server runs with a
  config generated under the data directory that imports the user's own; the plugin is
  `apply: "serve"`, so production builds never contain the adapter.
- **The dev server** is the project's own (a provider task or `npm run dev`), run as a task with
  the person's rights, like a build.

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
- The shell pins its Dart packages with `pubspec.lock` (`flutter pub get --enforce-lockfile`
  in `scripts/check.sh`), and `scripts/check_dart_licences.py` rejects any licence but MIT,
  BSD or Apache-2.0; Rust uses `Cargo.lock`; `cargo deny` (licences + advisories) is added in
  M1-CI-001.


## Test-only code in the desktop shell

Flutter shell: the end-to-end suites are `integration_test` programs that link the app's own
library and drive it in-process; nothing test-only is compiled into the release build, so there
is no driver to guard against. The section below is the Tauri shell's (retired by FL-QA-001).

The Tauri end-to-end suites drove the app through an **embedded W3C WebDriver
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
- TOCTOU on path containment: narrowed, not closed (§Paths, "What remains").
- The shell's UI code runs with the person's full rights and no sandbox (above); the
  no-files, no-processes rule is enforced by review, not by a capability boundary.
- Unsaved text is kept across a crash of the shell or the daemon (the recovery folder under
  the shell's directory, AUD-001); it is plain text there, readable by the person only as far as
  their home directory's permissions allow.
- Since FL-QA-001 the shell depends on no web engine. The M5 preview *does* load arbitrary
  pages; it opens in the person's browser (ADR-0020 §8), whose isolation is that browser's.
- The CLI's approval prompt shows untrusted text with control characters escaped (`\x1b`;
  AUD-045); the Flutter shell draws text as text.
