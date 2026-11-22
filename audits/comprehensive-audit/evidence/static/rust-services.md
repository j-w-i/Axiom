# Static review: Rust services (env, build, agent, lsp, inspect, inspectors/web, providers, cli)

Reviewer scope: `crates/env`, `crates/build`, `crates/agent`, `crates/lsp`, `crates/inspect`,
`inspectors/web`, `providers/`, `apps/cli`. Where an agent tool's behaviour is implemented in
`crates/axiomd/src/tools.rs` or a handler, that code was read too, because the gate's guarantees
depend on it. Read-only review on 2026-11-22 at `main` (751ffc2); nothing was built or run.
Confidence: **Confirmed** means the code path was traced end to end; **Suspected** means the
code fact is confirmed but exploitability depends on something outside the repo (a third-party
tool's behaviour, a dependency version); **Unverified** is not used below.

---

## F1. Language servers start in untrusted workspaces, and they run workspace code

- Category: Security (code execution on opening a repository)
- Priority: **P1**
- Confidence: Confirmed for the missing gate; the servers' code execution is documented upstream behaviour, not exercised here
- References: `crates/axiomd/src/handlers/lsp.rs:25-60` (definitions from every active provider, "the shell starts one when a document of its language opens"); `crates/axiomd/src/handlers/env.rs:27-36` (trust only decides whether `.axiom/providers` is loaded); `crates/lsp/src/server.rs:966-984` (spawn); `providers/lsp-rust-analyzer/provider.toml:46-50`, `providers/lsp-jdtls/provider.toml`, `providers/lsp-typescript/provider.toml`

```rust
// handlers/lsp.rs
// M5.5 (ADR-0021 §3): every active provider's servers, whether or not it detects the
// workspace; the shell starts one when a document of its language opens.
```

Failure scenario: someone clones a malicious repo, opens it in Axiom and opens a `.rs` file.
rust-analyzer starts with default settings, which build `build.rs` scripts and proc macros (it
runs `cargo check`). jdtls imports Gradle/Maven builds (it runs build scripts), and
typescript-language-server loads `typescript` from the workspace's `node_modules`. The repo's code
runs as the user with no prompt. Workspace trust exists (`env/trust`), but it only gates
workspace-local provider manifests. It does not gate built-in language servers, `.axiom/project.toml`
environment selection, or tasks.

Proposed fix: until the workspace is trusted, start no language server whose provider is flagged
as executing project code (add a `runs_project_code = true` flag to the `[[lsp]]` entries for
rust-analyzer, jdtls, typescript, gopls, kotlin, csharp and sourcekit). Alternatively, start them
with safe initialization options: for rust-analyzer, `cargo.buildScripts.enable = false` and
`procMacro.enable = false`. The shell then shows a trust prompt, as VS Code's restricted mode does.

Verification: an integration test in `crates/axiomd/tests` opens an untrusted temporary workspace
with a `build.rs` that writes a marker file, calls `lsp/request` for a `.rs` document, and asserts
that no server was started (or that the marker is absent). It then trusts the workspace and
asserts that the server starts.

---

## F2. "Allow for session" grants are wider than what the person approved

- Category: Security (permission gate)
- Priority: **P1**
- Confidence: Confirmed
- References: `crates/agent/src/gate/mod.rs:275-288` (`grant`), `crates/agent/src/gate/policy.rs:79-84` (argv *prefix* match), `crates/agent/src/gate/mod.rs:347` (`clear_grants`, no caller anywhere in the repo), `crates/axiomd/src/handlers/agent.rs:175-199` (MCP/CLI principals), `apps/cli/src/mcp.rs:37-45,101-103` (principal = client-chosen name), `docs/protocols/agents.md:162`

```rust
fn grant(&self, principal: &str, p: &Prepared) {
    let rule = PolicyRule {
        effect: Some(p.effect),
        tool: Some(p.name.clone()),
        path: (p.targets.paths.len() == 1).then(|| p.targets.paths[0].0.clone()),
        argv_prefix: p.targets.argv.clone(),   // matched as a prefix
        decision: Decision::Allow, };
    lock(&self.grants).entry(principal.to_string()).or_default().push(rule);
}
```

The spec says a session grant covers "that tool and target set". Four ways the code goes wider:
1. **Prefix, not exact argv.** Suppose the person approves `task.run ["rm", "build/tmp.o"]` (or
   `["find", ".", "-name", "*.o"]`) for the session. Every later
   `["rm", "build/tmp.o", "-rf", "<anything>"]` or `["find", ".", "-name", "*.o", "-delete"]`
   then runs without a prompt, because `matches` accepts any argv that starts with the granted one.
2. **Zero-target tools are granted wholesale.** `env.install` (`Targets::default()`) and every
   `mcp.<server>.<tool>` (argv `["mcp:<server>", "<tool>"]`, with the input not in the targets)
   get a grant that covers every later call with any input, such as any plan id or any MCP arguments.
3. **Grants never expire.** `clear_grants` is never called. For `session:<id>` they live as long
   as the daemon. For `mcp:<name>` and `cli` they are shared by every connection that claims the
   same name. The name is the MCP client's self-declared `clientInfo.name`.
4. **Grants are not scoped to a workspace.** They are keyed by principal only, and policy paths
   are workspace-relative. An `mcp:claude-code` grant for `task.run ["make"]` or for
   `fs.applyPatch` on `Makefile`, approved in workspace A, also applies in workspace B. The gate
   is one per daemon.

Proposed fix: store a session grant as an exact match: a new `argv_exact` field, or keep
`argv_prefix` and also require `argv.len() == prefix.len()`. Key grants by
`(principal, workspace_id)`. Do not offer "for session" (`can_remember = false`) for tools whose
targets do not describe the call (empty paths and no argv, or MCP tools). Clear grants when a
session ends, is deleted or is rewound. For `mcp:`/`cli` principals, scope grants to the
connection (clear them in the connection-closed hook).

Verification: unit tests in `gate/mod.rs`:
- (a) approve `["rm","a"]` with `Session`, then `check` on `["rm","a","-rf","/"]` must ask.
- (b) approve `env.install` with `Session`, then a second `env.install` must ask (or `can_remember`
  is false).
- (c) a grant made in workspace A does not allow the same call in workspace B.
- (d) after `clear_grants` (wired to session end), the call asks again.

---

## F3. ACP permission answers become grants that do not match what was approved

- Category: Security (permission gate, external agents)
- Priority: **P2**
- Confidence: Confirmed
- References: `crates/agent/src/acp.rs:911` (`effect_of(kind)`), `crates/agent/src/acp.rs:962-976`, `crates/agent/src/acp.rs:838-848` (`next_terminal` rule), `crates/agent/src/acp.rs:500-508` (`effect_of`)

```rust
Ok((via, _)) => {
    // The approval covers the writes to these paths and (for execute) the next command.
    for p in &paths { grants.rules.push(PolicyRule { tool: Some("acp.writeFile".into()),
        path: Some(p.0.clone()), decision: Decision::Allow, .. }); }
    if effect == Effect::Execute { grants.next_terminal = true; }
```

Failure scenarios:
- The grant is pushed whenever the decision is `Ok`, including `Via::Policy`, where no person saw
  anything. The effect comes from the agent's self-declared `kind`. A `session/request_permission`
  with `kind: "read"` (or `"think"`, `"search"`, `"switch_mode"`) and `locations: [src/main.rs]`
  is allowed by the default `read → allow` rule. It then installs an `acp.writeFile` allow rule
  for `src/main.rs` (specificity 2, which beats the default ask), so the next
  `fs/write_text_file` to that file in the turn is written without a prompt and without a diff
  shown.
- An approved execute permission sets `next_terminal`. That lets the next `terminal/create` run
  any argv, not the command the permission card described (its title or content).

Proposed fix: push write grants only when the approved effect is a write
(`WriteWorkspace`/`Delete`) and `via` is `Via::Approved(_)`. Bind `next_terminal` to the argv
the permission showed, taken from the tool call's `rawInput` command when present, and require
equality. Otherwise ask again for the terminal.

Verification: tests in `crates/axiomd/tests/agent_acp.rs` with the scripted ACP agent:
- (a) a read-kind permission for `a.txt` followed by `fs/write_text_file a.txt` must produce an
  approval request.
- (b) an approved execute permission for `ls` followed by `terminal/create rm -rf x` must produce
  an approval request.

---

## F4. `acp.terminal` approval hides the environment and working directory the agent sets

- Category: Security (approval integrity)
- Priority: **P2**
- Confidence: Confirmed
- References: `crates/axiomd/src/tools.rs:1528-1536` (`acp_prepare_terminal`: argv only), `crates/axiomd/src/tools.rs:1540-1556` (`acp_terminal`: agent `env` and `cwd` applied at execute)

```rust
let mut env = spawn_env(app, ctx).await?;
for e in input["env"].as_array().into_iter().flatten() {
    if let (Some(n), Some(v)) = (e["name"].as_str(), e["value"].as_str()) {
        env.insert(n.into(), v.into());
```

Failure scenario: the approval card and the policy see `["cargo", "test"]`, and a user rule
`argvPrefix = ["cargo"] → allow` lets it through. The process actually runs with `PATH=/tmp/x`,
`LD_PRELOAD=...`, `RUSTC_WRAPPER=...` or `CARGO_BUILD_RUSTC=...` supplied by the agent, which
turns an approved build into arbitrary code.

Proposed fix: put the env overrides and cwd in the `Targets`, which the approval card and the
audit log show. Make any env override, or any cwd other than the root, ask even when an argv rule
allows. Alternatively, refuse `LD_*`/`DYLD_*`/`PATH` and tool-wrapper variables.

Verification: a unit test of `acp_prepare_terminal` asserts that env names appear in the targets.
A gate test asserts that an argv-allow rule does not cover a call with env overrides.

---

## F5. `env.install` is approved blind

- Category: Security (approval integrity)
- Priority: **P2**
- Confidence: Confirmed
- References: `crates/axiomd/src/tools.rs:216-224` (prepare returns `Targets::default()`), `crates/axiomd/src/tools.rs:961-977`, `crates/agent/src/gate/mod.rs:168-182` (the request carries only targets/argv/preview)

```rust
"env.install", ..., Effect::Install,
|_, _, i| async move { parse::<inputs::InstallInput>(i).map(|_| Targets::default()) }.boxed(),
```

Failure scenario: the approval card for an agent's `env.install` shows only the tool name. It does
not show which provider or version, the download URLs, the commands the plan runs (rustup-init,
`uv python install`, npm), or whether it is a system plan. The planId refers to any unexpired
plan in the daemon-wide table (`Installer::plans`), including one the UI issued. With F2's
wholesale grant, one "for session" answer covers every later install.

Proposed fix: in `prepare`, look up the issued plan (`Installer::plan_provider`, or a new
`Installer::plan(&id)`) and return the plan's steps as the preview, and each `Run`/`SystemPackages`
argv as targets. Refuse system-scope plans from agent principals. Bind plans to the principal or
session that issued them.

Verification: an axiomd tools test asserts that the `ApprovalRequest` for `env.install` contains
the provider, version and each step's argv. A second test asserts that a plan issued over
`env/planInstall` by a client cannot be installed by `session:<id>`.

---

## F6. `build.run` runs a freshly resolved command, not the approved argv

- Category: Security / correctness (TOCTOU between approval and execution)
- Priority: **P3**
- Confidence: Confirmed
- References: `crates/axiomd/src/tools.rs:725-733` (prepare: argv with an empty environment), `crates/axiomd/src/tools.rs:780-806` (`build_run` ignores `_targets`, re-runs `build_spec` and `axiom_build::prepare` with the project environment)

Failure scenario: the person approves `cargo build ...`. Between approval and execution (an
approval can wait indefinitely), `.axiom/providers/*/provider.toml` (trusted workspace) or
`.axiom/project.toml` changes, for example through another writer or an earlier allowed agent
write. The re-resolved task, or the `PATH` prepend that decides which `cargo` runs, then differs
from what was shown. `b.provider = None` also means "first provider offering `task`", which can
change between the two lookups.

Proposed fix: execute `targets.argv` as approved, or compare the re-resolved argv with
`targets.argv` and refuse with `conflict` on any difference. Also show the resolved environment's
`PATH` prepend in the approval.

Verification: a test that prepares `build.run`, edits the workspace provider's task command, then
executes, and asserts a `conflict` error.

---

## F7. Agent `task.run` / `build.run` buffer all output in memory; `build.run` has no timeout

- Category: Correctness / DoS (unbounded buffer)
- Priority: **P2**
- Confidence: Confirmed
- References: `crates/axiomd/src/tools.rs:348-378` (`run_to_end`: `out.push_str` for up to 600 s), `crates/axiomd/src/tools.rs:810-826` (`build_run`: `out.push_str` with no deadline), `crates/axiomd/src/tools.rs:310-324` (only the tail is used)

Failure scenario: an approved `["yes"]`, `cat /dev/urandom | base64`-like tool output, or a build
in a verbose loop writes hundreds of MB/s. The daemon keeps all of it until exit, which can be
600 s for `task.run` and indefinitely for `build.run`. This takes gigabytes of RSS and can get the
daemon OOM-killed, taking every workspace with it. Only the last 200 lines / 16 KiB are ever used.

Proposed fix: keep a bounded tail (a ring of `TAIL_LINES` lines capped at `TAIL_BYTES`, plus a
`cut` flag) instead of the full `String`. Give `build.run` the same default and maximum timeout as
`task.run`.

Verification: a tools test runs a program that prints 200 MB and asserts that the daemon's RSS
growth stays bounded (or unit-test the tail accumulator with 10^6 lines). A test asserts that
`build.run` times out.

---

## F8. Inspect: workspace text search runs on Tokio worker threads, once per hover

- Category: Correctness (blocking I/O on async threads)
- Priority: **P2**
- Confidence: Confirmed
- References: `crates/inspect/src/session.rs:323,357-363` (`handle` is sync, called from the reader task), `crates/inspect/src/normalize.rs:107,122` (`chain`/`node` call `search`), `crates/inspect/src/normalize.rs:189-258` (`read_dir` + `read_to_string` of up to 2000 files of up to 1 MiB), `crates/inspect/src/lib.rs:191-230` (`node_details`, also on the async path)

```rust
"inspect/hovered" => { ...
    let node = p.node.map(|n| s.normalizer.node(n, None));   // -> self.search(text)
```

Failure scenario: in inspect mode the page sends `inspect/hovered` at mouse-move rate. Each
unstamped element with text (any component that does not forward the stamp, or a project without
Babel) walks the workspace and reads up to 2000 source files synchronously on a runtime worker.
A few such events in flight block the daemon's workers, so all RPCs (editor, LSP forwarding,
terminals) stall. A connected page (see F9) can also trigger this deliberately.

Proposed fix: run `search` in `spawn_blocking`, cache results by text per snapshot generation,
and debounce or skip search for hover. Search only for `selected` and `nodeDetails`, or build a
file index once per preview.

Verification: a test with a workspace of 2000 files and a fake adapter sending 50 hover events
asserts that a concurrent `ping` RPC answers within 100 ms.

---

## F9. Inspection WebSocket: no Origin check; the "one-time" token is reusable and served to any page

- Category: Security (local service exposure)
- Priority: **P2**
- Confidence: Confirmed for the code facts; Suspected for cross-site exploitation (depends on the project's Vite version and CORS settings)
- References: `crates/inspect/src/session.rs:215-246` (only the token query is checked), `crates/inspect/src/lib.rs:251-258` (`take_token` does not remove; the comment says one-time), `crates/inspect/src/lib.rs:265-269` (`adopt` replaces the existing session), `inspectors/web/plugin.mjs:112-117` (token inlined into the served module `/@id/__x00__virtual:axiom-adapter`), `crates/axiomd/src/handlers/preview.rs:141-160,188` (config containing the token written under the data dir with default permissions)

```rust
fn take_token(&self, token: &str) -> Option<Registration> {
    // One-time: ...
    lock(&self.tokens).get(token).map(|r| Registration { ... })   // never removed
```

Failure scenarios:
- Anything that can read the dev server's adapter module learns the token. Examples: a website,
  when the project's Vite predates the CORS/`allowedHosts` hardening of 6.0.9/5.4.12, or when
  `server.cors` is set permissively; another local user who can read the generated
  `vite.config.mjs`. The attacker can then connect to `ws://127.0.0.1:<port>/?token=...`, and the
  upgrade does not check `Origin`.
- The new connection is adopted and the real page's session is closed. The attacker's page then
  answers `inspect/snapshot` and `ui.verify`, so it can report false `verified` verdicts, choose
  the "selected" element the agent edits, and put text into the agent's context through
  `ui.snapshot`/`ui.selection`.

Proposed fix: on upgrade, require `Origin` to be the preview's dev-server origin (known from the
preview URL), or absent for non-browser tests. Make the token single-use and issue a fresh one
per page load: the adapter fetches it from a dev-server endpoint that checks `Sec-Fetch-Site`, or
gets a per-connection nonce. Do not replace a live session silently; emit an event and require
the same origin. Write the generated preview directory with mode 0700.

Verification: tests in `crates/inspect/tests/adapter.rs`:
- (a) an upgrade with `Origin: https://evil.example` is rejected with 403.
- (b) a second connection with a used token is rejected, or does not replace a live session.

---

## F10. Version probes and system installs run with the shared temp directory as cwd

- Category: Security (local privilege boundary on multi-user Linux)
- Priority: **P2**
- Confidence: Suspected (the code is confirmed; the effect depends on each tool's cwd-based config discovery)
- References: `crates/env/src/detect.rs:323-337` (`run_version`: `current_dir(std::env::temp_dir())`, called by `detect_on_path` for every provider tool), `crates/env/src/install.rs:358-364` (`list_versions`), `crates/env/src/install.rs:678-684` (system package manager, elevated)

Failure scenario: on Linux `temp_dir()` is the world-writable `/tmp`. Many probed tools read
project configuration from cwd and its ancestors:
- rustup proxies (`cargo --version`, `rustc --version`) read `rust-toolchain.toml`, whose `path`
  key names a custom toolchain directory to execute.
- `dotnet --version` reads `global.json`, whose `sdk.paths` (.NET 10) names SDK search paths.
- `npm` reads `.npmrc`, and `go` reads `go.mod`/`go.work` (the `toolchain` directive).

Another local user who plants `/tmp/rust-toolchain.toml` with `path = "/tmp/evil"` gets their
binary executed as the Axiom user on the next `env/detect`, which also runs from the agent's
Read-effect `env.detect`. The elevated package-manager run inherits the same cwd.

Proposed fix: run probes and installers in a private empty directory, for example
`<data dir>/run/empty` (0700), or the staging root for installs.

Verification: a unit test asserts that `run_version` uses a directory under the data dir. An
integration test with a fake `cargo` script that prints its cwd asserts that the cwd is not
`/tmp`.

---

## F11. CLI approval prompt prints untrusted text without neutralising terminal controls

- Category: Security (approval spoofing)
- Priority: **P2**
- Confidence: Confirmed
- References: `apps/cli/src/main.rs:436-466` (`ask`: prints `r.preview`, `r.tool`, `r.targets.join(", ")`), `apps/cli/src/main.rs:383-398` (model text, tool names and targets streamed raw)

```rust
if let Some(p) = &r.preview { eprintln!("{p}"); }
eprint!("Allow {} ({effect}) on {}? [y] once ... ", r.tool, r.targets.join(", "), ...);
```

Failure scenario: the preview is a diff of file content (a repo file can contain
`ESC[2K`, `ESC[1A`, `\r`). Targets include the argv joined with spaces, and the model chooses the
argv. Such sequences can erase or overwrite the diff and the command line, so the prompt reads
"Allow task.run (execute) on cargo test?" while the argv really continues with destructive
arguments. Arguments that contain spaces also make the joined display ambiguous.

Proposed fix: before printing to a TTY, replace C0/C1 control characters (except `\n` and `\t`)
in preview, targets and streamed text with visible escapes. Show argv as a quoted list
(`shell_words::join`-style) instead of a space join.

Verification: a unit test of a `sanitize_for_tty` helper. A CLI test feeds an approval request
whose argv contains `"\x1b[2K"` and asserts the printed prompt contains `\x1b` escaped.

---

## F12. ACP agents and MCP servers are never stopped or evicted until the session is deleted

- Category: Correctness (process leak, stuck sessions, lock held across slow I/O)
- Priority: **P2**
- Confidence: Confirmed
- References: `crates/axiomd/src/handlers/sessions.rs:542-571` (ACP connection cached in `agent.acp`; the global mutex is held across `acp::connect`, up to 2 × 60 s), `crates/axiomd/src/handlers/sessions.rs:656-670` (MCP servers per session), `crates/axiomd/src/handlers/sessions.rs:241-244` (the only removal: `delete`), `crates/agent/src/acp.rs:686-690` (an ignored cancel kills the agent but leaves it cached)

Failure scenarios:
- (a) Every session that ever ran a turn keeps its external agent process (node, claude-agent-acp)
  and its MCP server processes for the daemon's lifetime, which over a day is dozens of processes
  and hundreds of MB.
- (b) If an agent crashes, or is killed by the cancel path, the dead `Connection` stays in the
  map. Every later turn of that session fails with "is not answering" or "exited" until the
  daemon restarts or the session is deleted.
- (c) A slow agent start (no network or sign-in, 60 s timeouts) holds `agent.acp`, which blocks
  every other session's ACP turn and `sessions/delete`.

Proposed fix: evict the cached `AcpSession` when its reader ends or after a kill, and reconnect
on the next turn. Stop agents and MCP servers after an idle timeout or when no client has the
session open. Do the `connect` outside the map lock, using a per-session `OnceCell` or a
"connecting" placeholder.

Verification: an `agent_acp` test with a scripted agent that exits mid-turn asserts that the next
turn reconnects. A test asserts that a second session's turn is not delayed by a first session
whose agent hangs in `initialize`.

---

## F13. The agent can relax workspace policy or plant workspace providers with an ordinary write

- Category: Security (policy integrity)
- Priority: **P3**
- Confidence: Confirmed (design)
- References: `crates/axiomd/src/handlers/agent.rs:106-118` (workspace policy re-read from `.axiom/project.toml` on every call), `crates/axiomd/src/tools.rs:601-614` (`fs.applyPatch` treats `.axiom/*` like any file), `crates/env/src/registry.rs:176-183` (trusted workspace providers define tasks and LSP commands)

Failure scenario: a workspace deny such as `argvPrefix = ["cargo","publish"] → deny` is removed
by one approved edit of `.axiom/project.toml`. Under a broad user rule like
`effect = "writeWorkspace" → allow`, no prompt is shown at all. The next call is then evaluated
without it. The same goes for adding `.axiom/providers/x/provider.toml` in a trusted workspace:
the next `build.run` or language-server start runs the new command. The spec's "content a model
reads cannot change policy" holds for reads but not for writes.

Proposed fix: make writes to `.axiom/**` (and `.git/**`, if the workspace layer allows them)
always ask, with a rule that no user rule or grant can override. Alternatively, snapshot the
workspace policy at session start and apply changes only after a person confirms them.

Verification: a gate test with the user policy `writeWorkspace → allow` asserts that
`fs.applyPatch` on `.axiom/project.toml` still produces an approval request.

---

## F14. System-package names are not validated before being placed after an elevated package manager

- Category: Security (argument injection into a root command)
- Priority: **P3**
- Confidence: Confirmed (code); low exposure, because installs only use built-in and user-dir manifests (`crates/axiomd/src/handlers/env.rs:160-163`)
- References: `crates/env/src/manifest.rs:230-234` (only checks that the list is non-empty), `crates/env/src/system.rs:94-101,129-137` (`pkexec <apt-get> install -y <packages...>`)

Failure scenario: a provider in `<data dir>/providers` (written by another tool, a dotfile sync,
or anything that can write the data dir) lists `"-oAPT::Update::Pre-Invoke::=sh -c ..."` or
`"--option=Dpkg::Pre-Invoke=..."` as a package. The plan shows it, but in the polkit dialog it
reads like a package list, and the command runs as root.

Proposed fix: validate package names per manager (`^[a-z0-9][a-z0-9+.\-_:@/]*$`, never a leading
`-`) in `manifest::validate`. Insert `--` before the package list for apt, dnf and pacman.

Verification: a `manifest.rs` unit test asserts that a package name starting with `-` is a
validation error. A `system.rs` test asserts that `install_commands` puts `--` before the
packages.

---

## F15. Policy glob matching is exponential in the number of `*`

- Category: Correctness / DoS (algorithmic complexity on the gate path)
- Priority: **P3**
- Confidence: Confirmed
- References: `crates/agent/src/gate/policy.rs:166-190` (`seg` and `segs` recurse over every split point)

Failure scenario: a workspace policy (from the repository) contains
`path = "*a*a*a*a*a*a*a*a*a*b"`, and the agent touches a file whose name is 200 `a`s. The repo can
contain one, and a prompt can steer the agent to it. `seg` explores about n^k states
synchronously inside `Gate::check` on a runtime worker, which hangs that worker and every
approval behind it. `**` has the same shape over segments.

Proposed fix: use an iterative wildcard matcher (two-pointer with backtrack to the last `*`,
which is O(n·m)), or the `globset` crate already common in the ecosystem. Also limit pattern
length and star count when parsing a policy.

Verification: a unit test that matches the pattern above against a 255-character name finishes in
under 10 ms.

---

## F16. Providers: unpinned "latest" bootstraps and same-origin checksum files

- Category: Supply chain / reproducibility
- Priority: **P3**
- Confidence: Confirmed
- References: `providers/python/provider.toml:40-80` (uv from `releases/latest/download/...`, checksum from the same `latest` URL, version `"latest"`), `providers/rust/provider.toml:44-81` (rustup-init with `sha256_url` on the same host), `providers/node/provider.toml:39-74`, `providers/cmake/provider.toml:37-65`, `crates/env/src/install.rs:951-962`

Failure scenarios:
- For `sha256_url` artifacts, the expected digest is fetched from the same origin over the same
  TLS as the artifact. A compromised release or CDN serves a matching pair, so the check only
  detects corruption.
- For uv, the artifact and checksum both track `latest`. A release published between the
  checksum fetch and the download makes the install fail with `ChecksumMismatch`, and two
  machines installing "the same" Python get different uv versions. The receipt records only
  `latest`.

Proposed fix: pin uv (and rustup-init) to explicit versions with digests in the manifest, as
`go`, `dart`, `java` and others already do. For `sha256_url` sources, record the digest in
`project.lock` on first install and verify against it on later installs.

Verification: the contract test (`crates/env/tests/provider_contract.rs`) asserts that no
artifact URL contains `/latest/` and that every archive method's static versions have pinned
digests, or that an allow-list names the exceptions.

---

## F17. Archive extraction has no size or entry-count limit

- Category: Correctness / DoS (decompression bomb, disk fill)
- Priority: **P3**
- Confidence: Confirmed
- References: `crates/env/src/archive.rs:219-235` (`io::copy` with no cap), `crates/env/src/archive.rs:277-380` (no entry limit), `crates/env/src/fetch.rs:125-164` (download with no size cap)

Path safety is good: `..`, absolute paths, symlink parents, link targets (lexical and resolved
afterwards) and hard links are all checked, and setuid bits are dropped. But a gz/xz/zip entry
expands without limit. The digest comes before extraction, so the risk is limited to `sha256_url`
artifacts (F16) and to upstream mistakes. The effect would be filling the data partition rather
than escaping the root.

Proposed fix: cap the total extracted bytes (for example 20× the archive size or a per-provider
`max_install_bytes`) and the entry count. Cap the streamed download at a generous limit, or at
`Content-Length` plus a margin.

Verification: an `archive.rs` unit test with a gz of 10 GiB of zeros asserts an `UnsafeArchive`
error before 1 GiB is written.

---

## F18. Smaller robustness issues (each P3, Confirmed)

- **Blocking file I/O on async threads in the gate's audit log.** `crates/agent/src/gate/mod.rs:380-398`
  (`audit_append` runs `OpenOptions::open` and `write_all` from `finish`, on every tool call, inside
  the async pipeline). On slow disks or NFS homes this stalls a worker. Fix: send records to a
  dedicated writer task (mpsc) or use `spawn_blocking`.
- **Unbounded line buffers from child processes.** `crates/env/src/install.rs:1345-1346`
  (`BufReader::lines()` on installer output: one line without a newline grows without bound),
  `crates/agent/src/acp.rs:191-194` and `crates/agent/src/mcp_client.rs:98-99` (agent and MCP
  stdout lines, plus an unbounded mpsc channel in ACP), and `crates/agent/src/model/sse.rs:162-176`
  (the SSE buffer for a line without `\n` grows without bound and is rescanned from the start on
  every chunk). Fix: cap the line length (for example 16 MiB) and fail the stream past it. In
  SSE, scan only the newly appended bytes.
- **Test credential backend writes API keys with default permissions.** `crates/agent/src/credentials.rs:99-102,123-131`
  (`std::fs::write`, usually 0644 under the default umask, whenever `AXIOM_TEST_CREDENTIALS_FILE`
  is set in any build). Fix: create it with 0600, and compile the switch only with a `test-support`
  feature.
- **Environment inherited by every child.** No `env_clear`/scrub in `crates/env/src/install.rs:1326-1333`,
  `crates/agent/src/acp.rs:174-183`, `crates/agent/src/mcp_client.rs:83-92`, or the exec path used
  by `task.run`, `build.run` and `acp.terminal`. Only the LSP (`crates/lsp/src/server.rs:976`) and
  git paths scrub. A key configured as `api_key_env` (for example `ANTHROPIC_API_KEY`) is visible
  to installers, MCP servers, external agents and agent-run commands, and one approved
  `task.run ["env"]` puts it in the session journal. This contradicts `credentials.rs:1-4`
  ("A key never reaches ... the journal"). Fix: remove the configured `api_key_env` variables
  (and `AXIOM_*` internals) from children's environments by default.
- **MCP principal is self-declared.** `apps/cli/src/mcp.rs:101-103,145` uses the client's
  `clientInfo.name`, so two different MCP clients that both claim `claude-code` share grants
  (see F2) and an audit identity. Fix: add a per-connection nonce to the principal, or tie grants
  to the RPC connection.
- **`env.planInstall` is declared `read` but can do more than read.** It makes network HEAD
  requests (`crates/env/src/install.rs:528-535`). With a user provider whose delegate has no
  `versions`, it also runs the delegate's `list_versions` program
  (`crates/env/src/install.rs:309-316,349-365`, reached from `crates/axiomd/src/tools.rs:930-944`
  when `version` is omitted). No built-in provider hits the program path today; all declare
  `versions`. Fix: declare `env.planInstall` as `network`, and never run `list_versions` from a
  read tool.

---

## Things checked and found sound (not findings)

- Archive path validation (`entry_path`, `check_parents`, `link_stays_inside` + post-pass
  `links_stay_inside`, hard links restricted to earlier regular files, special files skipped,
  `mode & 0o777`).
- Digest verified before the cache rename and before any extraction. The cache is re-hashed on a
  hit. Version strings are validated before they become directory names. `uninstall`
  canonicalises and confines the target.
- HTTPS only, including on redirects (`crates/env/src/fetch.rs:45-62`). Model base URLs: HTTPS, or
  HTTP to loopback only.
- A plan is single-use, expires, and is invalidated if the manifest changes. Installs ignore
  workspace providers.
- Checkpoint git runs fully isolated (GIT_* scrubbed, no system/global config, hooks disabled,
  fsmonitor off), and restore paths are confined (`contained`).
- The LSP supervisor uses `env_clear`, a process group, a 64 MiB frame cap, a bounded outgoing
  queue with hang detection, a crash-loop limit, and refuses `workspace/applyEdit`.
- The `Allowed` token can only be minted by the gate (`pub(crate)`). Unknown tools and invalid
  inputs never reach execution. `fs.applyPatch` re-checks the file version approved.
- The inspection listener binds `127.0.0.1` with a 128-bit token from `getrandom`. The preview
  config forces `host: "127.0.0.1"`. The Vite plugin is `apply: "serve"` only.

## Areas reviewed and not reviewed

Reviewed:
- `crates/env/src/*`: archive, fetch, install, system, manifest, registry, detect, project.
- `crates/build/src/run.rs`.
- `crates/agent/src`: gate, policy, tools registry, `agent_loop` (first half), acp, mcp_client,
  credentials, model/http, model/sse, checkpoint (git invocation and restore).
- `crates/lsp/src`: framing, `server.rs` spawn/reader/exit paths.
- `crates/inspect/src`: lib, session, normalize (resolve/search).
- `inspectors/web`: `plugin.mjs` and the first third of `adapter.js`.
- `apps/cli/src`: mcp, conn, the approval and stream parts of main.
- `providers/*`: checksum kinds and the python, rust and jdtls manifests in full.
- The agent tool implementations in `crates/axiomd/src/tools.rs` that the gate depends on, plus
  `handlers/agent.rs`, `handlers/env.rs` and `handlers/lsp.rs`, and the ACP/MCP parts of
  `handlers/sessions.rs`.

Not reviewed or only skimmed:
- `crates/build/src/parse.rs` (diagnostic parsers).
- `crates/inspect/src/verify.rs`.
- `crates/agent/src`: `session.rs` (journal/compaction), `live.rs`, `model/anthropic.rs` and
  `model/openai.rs` (stream event decoding, panics on malformed JSON not traced),
  `model/scripted.rs`, `model/profiles.rs`, `tools/patch.rs` and `tools/inputs.rs`.
- `crates/lsp/src/docs.rs`, `lib.rs`.
- The rest of `inspectors/web/adapter.js`.
- `crates/axiomd/src/acp.rs` (terminal output buffer and `outputByteLimit` handling).
- The `crates/workspace` path resolver: F13 and the ACP path handling rely on `Workspace::resolve`,
  which is another reviewer's scope.
- The Windows-specific branches.
- Dependency versions and advisories (`cargo deny`).
- Every provider manifest's digests against upstream: not compared, no network.

Nothing was compiled, run or tested. All findings come from reading the code.
