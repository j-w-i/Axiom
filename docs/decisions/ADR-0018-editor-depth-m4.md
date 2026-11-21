# ADR-0018: Editor depth (M4): protocol 0.4, language servers, terminal, Git, search

Status: **Accepted** (2026-11-22). Written by the M4 planning pass and accepted under the owner's
standing instruction to work through M5 autonomously ("Work all the way through to M5
autonomously", 2026-11-21). The owner may reopen any point; a change is a new ADR, not a quiet
edit. Refines ADR-0004 §Language intelligence (its "detailed design is the first task of M4").

## Context
M4 brings the editor to daily-driver level (`docs/roadmap/milestones.md` §M4). It needs:
- language intelligence through language servers (R10);
- an integrated terminal and Git status and diff (R11);
- split panes, quick open and workspace text search, and Problems that merge language-server
  and build diagnostics.

Exit: rust-analyzer and typescript-language-server give completion, diagnostics,
go-to-definition and rename; the terminal runs an interactive shell; Git changes are visible and
diffable. Windows is deferred by the owner. Nothing here may preclude it (ConPTY is
`portable-pty`'s Windows backend), but Windows-only work is not in M4.

## Decisions

### 1. Protocol 0.4: additive `lsp/*`, `terminal/*`, `git/*`, `workspace/search`
IPC 0.1–0.3 stay frozen. The daemon reports **0.4**; clients feature-detect through
`InitializeResult.methods`. The exact types are written by M4-ARCH-001 into
`crates/protocol/src/{lsp,terminal,git}.rs` and `workspace.rs`.

| Method / notification | Purpose |
|-----------------------|---------|
| `lsp/servers` | the language servers for a workspace: id, languages, state (`starting`/`ready`/`failed`/`missing`/`stopped`), install hint, server capabilities once ready |
| `lsp/request` | one LSP request to a server (`method`, `params`) → its `result` or an LSP error mapped to `RpcError` (`lsp_error`) |
| `lsp/notify` | one LSP notification to a server (document sync: `textDocument/didOpen|didChange|didClose|didSave`) |
| `lsp/restart` | stop and start a server |
| `lsp/notification` (notification) | a server notification relayed to the workspace's clients: `textDocument/publishDiagnostics`, `$/progress`, `window/showMessage`, `window/logMessage` |
| `lsp/serverState` (notification) | a server's state changed |
| `terminal/create` | a PTY running the user's shell (or an argv) in the workspace root, in the project's environment; `cols`/`rows` |
| `terminal/write` / `terminal/resize` / `terminal/close` | input (UTF-8), size, end |
| `terminal/list` | the workspace's terminals (survive a webview reload; not a daemon restart) |
| `terminal/output` (notification) | output as UTF-8 text (a streaming decoder never splits a character), with a sequence number |
| `terminal/exited` (notification) | exit code |
| `git/status` | branch, ahead/behind, and per-path state (`modified`, `added`, `deleted`, `renamed`, `untracked`, `ignored`, `conflicted`; index and worktree side) |
| `git/show` | a file's text at `HEAD` (or the index), for diff and gutter markers |
| `git/didChange` (notification) | status changed (from the file watcher and after `.git` changes) |
| `workspace/search` | text or regex search over the workspace (the service the M3 `fs.search` tool already uses), bounded, with matches as line/column ranges |

The error kinds are `lsp_error`, `server_unavailable` and `git_unavailable` (the next free codes).

### 2. Language servers: the daemon supervises and owns the protocol; the editor is a client of the daemon, not of the server
- **New crate `crates/lsp` (`axiom-lsp`).** It owns one server process per (workspace, server
  definition), started on first use (the first `didOpen` of a matching language, or
  `lsp/request`). It does LSP base-protocol framing (`Content-Length`), `initialize` /
  `initialized` with one fixed client-capability set (the union of what the editor implements),
  `shutdown` / `exit`, and a restart policy: at most 3 crashes in 3 minutes, then `failed` with
  the last stderr lines.
- **Many clients share one server.** The daemon assigns its own request ids and keeps the
  pending map. It answers server→client requests itself:
  - `workspace/configuration`: from settings `lsp.<server>.settings`, else `null`;
  - `client/registerCapability`: recorded and accepted;
  - `window/workDoneProgress/create`: accepted;
  - `workspace/applyEdit`: refused for M4, `applied: false`. Edits come back to the editor as
    results; it applies them.
- **Document state** is mirrored from the sync notifications that pass through, so the daemon
  knows each open document's version and text. The editor owns unsaved text (ADR-0004); agents
  later read the same view.
- **Paths:** the wire carries absolute `file://` URIs, as LSP does. The editor maps its
  workspace-relative models to them with the workspace root it already has. A location outside
  the workspace (a standard-library definition) opens **read-only** through `fs/readFile` on that
  absolute path: the daemon allows reads of a path that one of its servers returned in a
  location, and nothing else (recorded in `security.md`).
- **The server definitions** are in the provider manifests (`providers/<id>/provider.toml`, a new
  `[[lsp]]` table: `id`, `languages`, `command`, `args`, `rootMarkers`, `initializationOptions`).
  So `rust` → `rust-analyzer` (found in the managed or system toolchain through the M2
  resolution, so the project's environment applies) and `node` → `typescript-language-server
  --stdio`. A server that cannot be found is `missing` with an install hint. It is not installed
  silently: rust-analyzer is a `rustup component add` step and typescript-language-server an
  `npm` package, both offered through the M2 install-plan flow where the provider can describe
  them.
- **Diagnostics** from `publishDiagnostics` are kept per URI by the daemon, as a source next to
  build diagnostics. Problems shows both, labelled by source, and the counts in the status bar
  include both.

### 3. The editor's LSP client is in-house TypeScript over Monaco's provider API
`monaco-languageclient` requires `monaco-vscode-api` (rejected by ADR-0004), so the client is
written for M4 in `apps/desktop-tauri/src/editor/lsp/`. It is small and typed:
- document sync, incremental where the server offers it;
- Monaco providers for completion (with resolve), hover, signature help, definition, references,
  document highlight, rename (prepare + edit), formatting (document and range), code actions
  (edits only; commands are not executed in M4), and document symbols (breadcrumb symbols);
- diagnostics markers.

LSP ↔ Monaco conversions are pure functions with unit tests. Requests are cancelled
(`$/cancelRequest` through `lsp/notify`) when Monaco cancels them. The editor-bridge-1 message set
is unchanged: the client talks to the daemon client directly, as the stores do.

### 4. Terminal: `portable-pty` in the daemon, xterm.js in the console
- **New crate `crates/term` (`axiom-term`)** wraps `portable-pty` 0.9 (MIT, wezterm's; ConPTY
  on Windows, used later) behind a small `Pty` trait for tests. One PTY per terminal. The reader
  is a blocking thread that feeds a streaming UTF-8 decoder. Output is coalesced to at most one
  notification per 16 ms per terminal; a 1 MiB backlog is kept for re-attach.
- **The shell:** `$SHELL` (else `/bin/sh`) with `-l` on macOS, in the workspace root, with the
  project's environment from M2 (`env/resolve`) prepended. `TERM=xterm-256color`.
- **Gate:** a terminal is the person's own shell and runs no agent command, so it is not gated.
  Agents keep `task.run` (gated).
- **UI:** `@xterm/xterm` 6.0.0 + `@xterm/addon-fit` 0.11.0 (MIT) as a **Terminal** view in the
  console (workspace-spec §5.3). Colours come from tokens (an ANSI palette per theme, added to
  the token generator), the code font, several terminals as tabs, and Mod+\` focuses or creates
  one.

### 5. Git through the `git` CLI, not `gix`
`git` is already required for checkpoints (ADR-0015), and its porcelain output is stable and
documented:
- status: `git status --porcelain=v2 -z --branch`;
- file text: `git show HEAD:path` / `:path`.

`gix` would add a large, fast-moving dependency tree for the same three operations. It is
reconsidered if status latency on large repositories needs it (budget: `git/status` < 300 ms on
this repository, measured in M4-GIT-001).

The service lives in `crates/workspace` (a `git` module; it already owns the watcher), run in
`spawn_blocking` with the isolated environment rule ADR-0015 set for checkpoints: no user hooks,
no pager, `GIT_OPTIONAL_LOCKS=0` so status never takes the index lock. With no `git`, or outside
a repository, `git/status` says so and the UI shows nothing.

The UI shows:
- status letters and colours in the explorer (fixed semantic slots);
- gutter change markers: a line diff against `HEAD` text in the webview with the existing
  `diff`-free algorithm, or Monaco's own diff computation;
- a **diff view**: Monaco's diff editor, opened from Source Control, the explorer, or a chat
  diff block's "Open in diff editor";
- the branch in the status bar;
- a Source Control side-bar page listing the changes.

Commit, stage and push are **not** in M4 (milestones: "visible and diffable").

### 6. Editor UX
- **Split:** up to two editor groups side by side (Mod+\\), each with its own tabs. Models are
  shared, so one buffer can be in both groups. The tab state stored in the daemon gains the
  group.
- **Quick open:** the existing Go to File palette ranks with a fuzzy scorer: subsequence with
  bonuses for separators and case, as the common editors rank. It lists files through
  `workspace/search`'s file mode, bounded at 20 000 paths, honouring `.gitignore` as the
  explorer does.
- **Search** side-bar page: query, regex, case and whole-word, include/exclude globs; results
  grouped by file, Enter opens at the match. Search runs in the daemon (`workspace/search`) and
  is cancelled when the query changes.

### 7. Module edges
- `lsp → protocol`.
- `term → protocol`.
- `axiomd → lsp, term`.
- `workspace` gains the `git` module; no new edge, since it runs the `git` program as checkpoints
  do.

`modules.md` and `check_deps.py` change with M4-ARCH-001.

## Consequences
- The daemon is now the single owner of language servers, so agents (a later `lsp.*` tool set)
  and the editor see the same diagnostics and the same document versions.
- The editor's LSP client is our code to maintain. It covers the features above, and anything
  else (semantic tokens, inlay hints, call hierarchy) is a later task, each a small provider.
- Terminals do not survive a daemon restart; they are processes of the daemon.
- Windows: ConPTY, `cmd`/PowerShell as the default shell, and `.cmd` launchers for
  typescript-language-server are owed when Windows support is picked up.

## Alternatives rejected
- **Raw LSP relay to the webview** (ADR-0004's outline: `lsp/send` / `lsp/receive` of whole
  messages). One server shared by several clients needs id remapping and server→client request
  handling in one place, and the daemon is that place. The relay would push both into every
  client.
- **`monaco-languageclient`:** requires `monaco-vscode-api` (ADR-0004).
- **`gix`:** see §5.
- **A terminal in the host (Tauri plugin):** the shell may not run processes (ADR-0002/0012), and
  terminals belong to a workspace in the daemon.
