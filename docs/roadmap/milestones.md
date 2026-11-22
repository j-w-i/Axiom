# Roadmap

Hierarchy: **Milestone → Epic → Task → Acceptance criteria.** Only M1 is
broken down to tasks (`docs/tasks/M1/`). Later milestones are specified to
epic level with their exit criteria; the **first task of every milestone**
is "freeze the draft contract against what has been learned, and write this
milestone's task files" (an architect-mode task). This is deliberate
progressive elaboration: detailed specs written now for M4+ would be wrong
by the time they are used.

Milestones are ordered by dependency, not strictly sequential: see
`dependency-graph.md` for what can overlap.

---

## M1 — Platform foundation (first vertical slice)

**Outcome.** A cross-platform desktop app that opens a folder, shows its
files, edits and saves them in an embedded Monaco editor, runs a command with
streamed output, and remembers its state — with every platform operation
going through the daemon.

| Epic | Tasks |
|------|-------|
| 1.1 RPC foundation | M1-RPC-001 |
| 1.2 Persistence | M1-STORE-001 |
| 1.3 Workspace service | M1-WS-001, M1-WS-002 |
| 1.4 Execution service | M1-EXEC-001 |
| 1.5 Daemon | M1-DAEMON-001, M1-DAEMON-002 |
| 1.6 Editor surface | M1-EDITOR-001 |
| 1.7 Desktop shell | M1-SHELL-001 … 005 |
| 1.8 Guards and CI | M1-CI-001, M1-CI-002 |

**Exit criteria** (verified and recorded in `verification/M1.md` by
M1-CI-002). On a clean data directory:
1. The application launches and shows "Connected".
2. File ▸ Open Folder opens a directory as a workspace.
3. The explorer shows its files, lazily, with ignored files dimmed, and
   updates when files change on disk.
4. Activating a file opens it in the editor with syntax colouring.
5. Editing marks the tab dirty; Ctrl/Cmd+S saves; the bytes on disk match.
   An external modification of a dirty file produces the conflict dialog.
6. Every one of the above went through `axiomd` (no file access in the
   shell): demonstrated by the integration tests using the same protocol.
7. A command run from the Output panel streams stdout and stderr distinctly
   and shows its exit status; Stop cancels it including child processes.
8. A failing command and a non-existent program produce clear, structured
   errors in the UI.
9. After restart: theme and editor settings, the last workspace, open tabs,
   expanded folders and command history are restored.
Plus: `scripts/check.sh` green; CI green on Linux and green-or-explained on
macOS and Windows.

---

## M1.5 — Desktop shell on Tauri (replaces the Qt frontend)

**Outcome.** The Tauri 2 + React shell reaches full M1 parity on Linux, macOS
and Windows, is built and tested by `scripts/check.sh` and CI, ships as
installable packages, and is then made the default shell. The Qt shell keeps
building and passing its tests throughout and is deleted only at the end.

Decision: ADR-0012. Design, parity matrix, budgets and evaluation:
`docs/migration/qt-to-tauri/`. Tasks: `docs/tasks/M1.5/`.

| Epic | Tasks |
|------|-------|
| 1.1 Decisions and governance | T-01 |
| 1.2 Bridge and host in the product build | T-02, T-03, T-04 |
| 1.3 Parity (behaviour, UI, recovery) | T-05, T-06, T-07, T-08 |
| 1.4 Cross-platform verification | T-09, T-10 |
| 1.5 Spikes with a decision | T-11 (editor isolation), M5-PRE-001 (preview engine) |
| 1.6 Release engineering | T-13 |
| 1.7 Gate, switch, retire | T-12, T-14, T-15 |

**Exit criteria** (recorded in `verification/M1.5.md` by T-12):
1. All nine M1 exit criteria hold on the Tauri shell on Linux, macOS and
   Windows in CI, against the same daemon and protocol (no daemon changes).
2. Every row of `docs/migration/qt-to-tauri/feature-parity.md` is ✅ or
   explicitly waived by the owner.
3. The budgets in `docs/migration/qt-to-tauri/performance.md` §2 are met on
   all three OSes (CI + one real-GPU machine per OS).
4. `scripts/check.sh` runs the Tauri gates; CI is green on three OSes.
5. Manual verification (IME, clipboard, drag and drop, HiDPI, screen reader,
   real GPU) is recorded per OS.
6. Packages for each OS install, launch and connect.
7. T-15 ran only after the removal conditions in
   `docs/migration/qt-to-tauri/implementation-roadmap.md` §5.
   *Recorded exception:* T-15 ran on 2026-11-17 with §5.1, §5.2 and §5.5 waived by
   the owner (`verification/M1.5.md` §6b).

M2's backend epics (2.0–2.6, 2.8) do not touch the shell and may run in
parallel with M1.5; M2 epic 2.7 (UI) starts only after T-12 and is built in the
Tauri shell only, on the M1.6 design system (after M1.6-SYSTEM-004 at the
earliest; designed in `docs/design/workspace-spec.md` §5.1).

---

## M1.6 — Axiom Visual Experience

**Status (2026-11-18): complete** on Linux and macOS — every exit criterion met or met with the owner's written waivers (`verification/M1.6.md` §Exit review); Windows deferred by the owner. PR j-w-i/Axiom#2.

**Outcome.** The Tauri shell has a distinctive, coherent and approved visual
identity ("Precision Instrument", subject to approval): a generated token system
with a material grammar (raised = pressable, recessed = holds a value or accepts
input, flat = content), bundled typography, a task transport and hint line for
readable machine state, purposeful motion with a measured looping policy, and
light and dark themes. All M1/M1.5 behaviour, the IPC protocol and the daemon are
unchanged.

Design: `docs/design/` (brief, research, concepts with the evaluation and
**decisions D1–D10 for approval**, design system, motion, surfaces, front-end
architecture, performance, visual QA). Prototypes:
`experiments/e5-visual-concepts/`. Tasks: `docs/tasks/M1.6/`.

**Scope rule.** Only surfaces backed by M1 functionality are built. The AI
workspace (M3), Environment Manager (M2), activity rail (from M2), Problems
and Terminal views (M2/M4) are designed and prototyped here but built by their
own milestones (governance rule 9).

| Epic | Tasks |
|------|-------|
| 1.6.1 DESIGN — research, concepts, approval | M1.6-DESIGN-001 (records the owner's decision; Phase A already produced `docs/design/`) |
| 1.6.2 SYSTEM — tokens and component foundations | M1.6-SYSTEM-001 … 004 |
| 1.6.3 SHELL — main workspace and application chrome | M1.6-SHELL-001 … 007 |
| 1.6.4 EDITOR — editor integration and surrounding UI | M1.6-EDITOR-001, 002 |
| 1.6.5 INTERACTION — motion, panels, microinteractions | M1.6-INTERACTION-001 … 004 |
| 1.6.6 SURFACES — palette, settings, light theme | M1.6-SURFACES-001 … 003 |
| 1.6.7 QA — visual, accessibility, performance, platforms | M1.6-QA-001 … 006 |

**Exit criteria** (recorded in `verification/M1.6.md` by M1.6-QA-006):
1. ADR-0013 records the approved visual direction and decisions D1–D10.
2. The production shell uses only the new tokens (legacy aliases deleted; guard
   test green), with both themes generated from `themes.json`.
3. Every surface in `docs/design/workspace-spec.md` §2–§4 and §6 matches its
   specification; deviations are listed and approved.
4. Controls follow the material grammar and the state table of
   `design-system.md` §8 (gallery screenshots, both themes).
5. Motion follows `motion-system.md`; reduced and off modes verified; no
   running animations when idle with motion off.
6. Performance budgets P1–P12 (`docs/design/performance.md` §3) met on the
   reference Linux machine against the QA-002 baseline; any regression is quantified
   and accepted in writing.
7. All M1 and M1.5 Vitest, e2e and host tests pass; `scripts/check.sh` green.
8. Accessibility: keyboard traversal of every surface, visible focus, contrast
   pairs, Orca pass on Linux, forced-colours check (QA-003).
9. Screenshots of states S1–S17 reviewed on Linux, macOS and Windows (QA-005;
   macOS/Windows need hardware or CI, the same blocker as M1.5).
10. The component system's designs for M2–M7 surfaces (`workspace-spec.md` §5)
    need no new region or foundation (reviewed in QA-006).

---

## M2 — Environments and builds

**Status (2026-11-21): complete** on Linux and macOS — all 14 tasks in `docs/tasks/M2/` tested; the exit scenario passes through the UI (`e2e/env.mjs`, also in CI on Ubuntu, 6/6), evidence in `docs/roadmap/verification/M2.md`. Merged to main under the owner's standing permission (2026-11-21). Still owed: the owner's by-hand run; Windows deferred. Decisions: `docs/decisions/ADR-0014-environments-and-builds.md`.

**Outcome.** Axiom recognises what a project needs, shows what is installed,
installs toolchains into managed storage with user-approved plans, and runs
provider-defined build/test tasks with structured, navigable diagnostics.

Draft contracts: `docs/protocols/providers.md`,
`crates/protocol/src/{provider,diagnostics}.rs`, `providers/rust/`.

| Epic | Scope |
|------|-------|
| 2.0 Contract freeze + task files | Review drafts against M1 reality; add `env/*`, `build/*` methods to the protocol; write `docs/tasks/M2/` |
| 2.1 Provider registry (`crates/env`) | Manifest loading/validation from the three locations; contract test suite over `providers/*` |
| 2.2 Detection | Tool detection (PATH + managed roots), project detection; `env/detect` |
| 2.3 Managed installation | Download cache with SHA-256, safe extraction, staging + atomic publish, uninstall, `InstallPlan` + approval flow, progress notifications; `delegate` installs with redirected homes |
| 2.4 System installation | Package-manager detection and plan generation; elevation through the platform prompt; never silent |
| 2.5 Environments | `Environment` resolution per project (`.axiom/project.toml` + lock); tasks run inside an environment (PATH/vars) |
| 2.6 Build engine (`crates/build`) | Task templates → `exec`; diagnostics parsers `cargo_json`, `gcc_text`, `regex`, `sarif`; build history; `build/diagnostics` |
| 2.7 UI | Environment Manager view (installed / available / versions / progress / remove); Problems panel with click-to-source; Run targets from provider tasks |
| 2.8 Providers | `rust` (reference, complete), `node` (archive install with official checksums), `cmake`+`ninja` (archive), `python` (delegate to `uv`) |

**Exit criteria.** On a machine without Rust: open a Cargo project → Axiom
reports Rust missing → shows an install plan → installs a managed toolchain
→ `cargo build` runs inside it → a compile error appears in Problems and
clicking it opens the file at the line → uninstall removes the directory and
nothing else. Provider contract suite passes for all bundled providers.

---

## M3 — Agent runtime

**Status (2026-11-22):** complete — all 15 tasks in `docs/tasks/M3/` tested; decisions in ADR-0015 (agent runtime), ADR-0016 (external agents through ACP) and ADR-0017 (Chat, a first-class full-window surface). Exit scenario `e2e/agent.mjs` 13/13 on Linux (both drivers, CI on Ubuntu); P2 +19.6 / +7.5 ms against M2. Owed by the owner: a real local model and real Claude Code / Codex through ACP. Record: `docs/roadmap/verification/M3.md`. Windows deferred.

**Outcome.** A user can converse with a local or remote model that reads the
workspace, proposes and applies patches under a permission policy, runs
builds, and whose session can be resumed later. External agents can drive
Axiom through MCP.

Draft contract: `docs/protocols/agents.md`.

| Epic | Scope |
|------|-------|
| 3.0 Contract freeze + task files | |
| 3.1 Model providers | `ModelProvider` trait; Anthropic and OpenAI-chat adapters; `ScriptedProvider` for tests; capability profiles; keychain storage |
| 3.2 Tool registry | Specs + schemas; read tools; `fs.applyPatch`; `task.run`; `build.*`; `env.*` |
| 3.3 Permission gate | Policy model, evaluation, approval round trip to clients, audit log |
| 3.4 Checkpoints | Shadow git repo; create/list/restore with conflict detection |
| 3.5 Agent loop + sessions | Streaming loop, cancellation, JSONL transcripts, resume, compaction |
| 3.6 Multi-client daemon | Local socket transport with token auth; `apps/cli` (thin client: `axiom task run`, `axiom agent …`) |
| 3.7 MCP | `axiomd --mcp` server over the registry; MCP client for user-configured servers |
| 3.7b External agents (ADR-0016) | ACP client: Claude Code, Codex, Gemini CLI as provider kind `acp`, their fs/terminal/permissions through Axiom's gate and checkpoints |
| 3.8 UI | **Chat** (full-window, no folder needed: Markdown, code, streaming, edit/regenerate, images, first-run setup; ADR-0017) and the AI dock beside the editor: tool activity, diff review, approvals, checkpoints, model configuration |

**Exit criteria.** With a scripted provider in CI and a real local model
manually: "add a function and fix the build" completes through tools only;
every write was preceded by a checkpoint and can be restored; denied
operations do not execute; the session resumes after restart; an external
MCP client can list and call tools subject to the same policy; Claude Code / Codex run inside
Axiom through ACP under the same gate (ADR-0016); with no folder open, a person can configure a
model and hold a first-rate conversation in Chat entirely by keyboard (ADR-0017).

---

## M4 — Editor depth

**Status (2026-11-22):** planned — 10 task files in `docs/tasks/M4/`; decisions in `docs/decisions/ADR-0018-editor-depth-m4.md` (protocol 0.4; the daemon owns language servers and the editor is its client; in-house LSP client over Monaco; `portable-pty` + xterm.js; Git through the `git` CLI instead of `gix`). Windows deferred.

**Outcome.** Language intelligence, terminal and Git, bringing the editing
experience to daily-driver level.

| Epic | Scope |
|------|-------|
| 4.0 Design + task files | LSP relay design (ADR-0004 §Language intelligence) |
| 4.1 `crates/lsp` | Server supervision, initialize, relay channel, restart policy; server definitions in provider manifests |
| 4.2 Editor LSP client | Document sync, diagnostics, completion, hover, definition, references, rename, formatting, code actions |
| 4.3 Terminal | PTY service (`portable-pty` behind a trait; ConPTY verified on Windows), xterm.js surface or QML terminal — decided in 4.0 |
| 4.4 Git | Status in explorer, gutter change markers, diff view (Monaco diff editor), branch display — via `gix` |
| 4.5 Editor UX | Split panes, quick open (fuzzy file search), workspace text search, problems from LSP merged with build diagnostics |

**Exit criteria.** rust-analyzer and typescript-language-server provide
completion, diagnostics, go-to-definition and rename in the editor; the
integrated terminal runs an interactive shell on all three OSes; Git changes
are visible and diffable.

---

## FL — Flutter shell

**Status (2026-11-22):** planned — ADR-0020 (owner's decision: a full Flutter rewrite of the
desktop shell); 12 task files in `docs/tasks/FL/`. Windows deferred.

**Outcome.** The desktop shell is a Flutter app (`apps/desktop-flutter`) over the unchanged
daemon, in a visual language that combines Precision Instrument with Temporal Tactility, with
Chat under Files reaching Axiom's agent and external agent harnesses, and every interaction at
the display's refresh rate.

**Exit criterion.** The M1–M4 exit scenarios pass in the Flutter shell on Linux and macOS; no
frame over 16.7 ms in the listed interactions at 125 % scale; a11y in both themes; packages
verified. Then `scripts/run.sh` launches it and the Tauri shell is retired
(`tauri-shell-final`).

---

## M5 — Visual intelligence (web)

**Outcome.** The closed loop of the product vision, for React + Vite apps.

Draft contract: `docs/protocols/inspection.md`,
`crates/protocol/src/inspection.rs`. Seed: experiment E3.

| Epic | Scope |
|------|-------|
| 5.0 Contract freeze, CDP-on-WebEngine validation, task files | Repeat E3's CDP calls against Qt WebEngine (the one unverified link) |
| 5.1 Preview host | Launch dev server as a task; isolated WebEngine profile; preview view with URL bar, reload, device size |
| 5.2 `crates/inspect` | Sessions, adapter connections, snapshot store, coordinate model |
| 5.3 Web adapter | CDP-driven tree/hit-test/details/highlight; screenshot capture |
| 5.4 Source instrumentation | `@axiom/vite-plugin-source` (from E3), injected at preview launch without editing user config; owner resolution; stable keys |
| 5.5 Source resolution | Path mapping, generated-code flagging, source-map and heuristic candidates, confidence enforcement |
| 5.6 Verification | Quiescence, re-identification, checks, pixel diff, `VerificationReport` |
| 5.7 Agent integration | `preview.*`, `ui.*` tools; context assembly for a selection; assertion-first patch workflow |
| 5.8 UI | Visual Inspector: preview, hover highlight, click-select, hierarchy tree, properties, source jump, before/after comparison, "Ask AI about this element" |
| 5.9 Fixtures + tests | `tests/fixtures/inspect-react` with expected source map; end-to-end loop test with the scripted model |

**Exit criteria.** On the fixture app: click a button → correct source
opens (exact line/column) → scripted agent applies a spacing fix → hot
reload → report `verified` with a passing geometry check; a deliberately
wrong patch yields `failed`; an element from an uninstrumented library is
presented as uncertain, not exact.

---

## M5.5 — Every language

**Status (2026-11-22):** complete on Linux — all 9 tasks in `docs/tasks/M5.5/` implemented; decisions in `docs/decisions/ADR-0021-every-language-m5-5.md`; P2 −10.0 / +18.0 ms and P7 +2.4 MiB against M5. Deferred: inlay hints, server folding ranges, the outline (ADR-0021 §8); .NET detected, not installed (§6). Owed: the opt-in real-server scenario per language, run by the owner or CI; macOS CI; Windows deferred. Record: `docs/roadmap/verification/M5.5.md`.

**Outcome.** Any file Axiom opens is highlighted; the languages people use most have a language
server that Axiom installs on request and points at the project's own SDK, so completion, hover
docs, signature help and navigation reach the standard library and the project's dependencies
the way VS Code's do; completion behaves like VS Code's (as you type, ranked, documented,
snippets, auto-imports).

| Epic | Scope |
|------|-------|
| 5.5.0 Design + task files | ADR: the language registry, server install recipes (reusing M2's checksummed installs), SDK hand-off to servers, completion UX; licence review of each server (external programs, not linked) |
| 5.5.1 Language registry | Every highlight.js grammar `re_highlight` ships (195) mapped by extension, file name, shebang and first line, like VS Code's language contributions; a language picker in the status bar; plain text for the rest |
| 5.5.2 Semantic highlighting | LSP semantic tokens over the grammar's colours where a server provides them; large-file guard |
| 5.5.3 Server registry + installs | `[[lsp]]` entries with install recipes and versions for each tier-1 language; "Install the Python language server" offered when such a file opens, with the M2 plan/approve flow; servers updated and removed like toolchains |
| 5.5.4 SDK awareness | Servers launched with the project's detected SDK (Python venv/interpreter, JDK, Go, .NET, Dart/Flutter SDK, Swift toolchain, `compile_commands.json` for clangd); new providers where none exists (Java, .NET, Go, Dart, Swift, Kotlin, Ruby, PHP, Lua, Zig) |
| 5.5.5 Completion like VS Code | As you type and on trigger characters; fuzzy filter and ranking; kind icons; documentation beside the list (`completionItem/resolve`); snippet tab stops; auto-import edits; commit characters; word completion where there is no server |
| 5.5.6 More language features | Inlay hints, outline (document symbols), workspace symbols (Mod+T), folding ranges, format on save, organize imports, call hierarchy |
| 5.5.7 Fixtures + tests | A fixture project per tier-1 language; end-to-end: open → offered install → completion of an SDK symbol with its docs → hover → definition into the SDK's source |

**Tier 1 (servers installed and verified):** Python (basedpyright), TypeScript/JavaScript
(typescript-language-server), Rust (rust-analyzer), Go (gopls), C/C++ (clangd), Java (Eclipse
JDT LS), C# (csharp-ls), Kotlin, Swift (sourcekit-lsp, macOS), Dart/Flutter (`dart
language-server`), HTML/CSS/JSON (vscode-langservers-extracted), YAML, TOML (taplo), Markdown
(marksman), Bash, Dockerfile. **Tier 2 (registry entries, best effort):** Ruby, PHP, Lua, Zig,
Haskell, Elixir, Svelte, Vue, Terraform, SQL.

**Exit criteria.** In fixture projects for each tier-1 language on Linux and macOS, starting with
no server installed: opening a source file offers the server; after the approved install,
completion lists a standard-library symbol with its documentation within 300 ms of typing its
prefix, hover shows its docs, and go-to-definition opens the SDK's source; every file in the
grammar fixture set is highlighted with its language named in the status bar; typing and
scrolling stay within the frame budget on a 10,000-line file.

---

## M6 — Framework and platform expansion

| Epic | Scope |
|------|-------|
| 6.1 QML inspection adapter | Probe library (from E2), per-Qt-minor builds, public-API fallback, `qrc` path mapping, fixture + loop test — proves the protocol is not web-specific |
| 6.2 Apple via xtool | `apple-xtool` provider with helper process: detect, guided setup (user-supplied Xcode.xip), build, device install/launch; honest capability matrix (`docs/research/xtool.md`) |
| 6.3 More providers | Swift (swiftly), Go, .NET, Java/Gradle, Android SDK/NDK, Flutter — each a manifest (+helper where needed) passing the contract suite |
| 6.4 Debugging | DAP client in the daemon, debug UI (breakpoints, stack, variables), adapters: codelldb, debugpy, js-debug |
| 6.5 Isolation backends | `Sandboxed` (Linux bwrap/Landlock, macOS Seatbelt, Windows) and `Contained` (Podman/Docker, Dev Containers) behind `ExecBackend`, each with a self-test (ADR-0010) |
| 6.6 Remote execution | `axiomd` over SSH; path mapping; remote workspace open |
| 6.7 Multi-agent | Worktree-per-agent, task ownership, merge review |
| 6.8 Generic Level-2 adapter | OS accessibility trees (AT-SPI, UIA, AX) for otherwise opaque apps |

## M7 — Production quality

| Epic | Scope |
|------|-------|
| 7.1 Extension compatibility | `monaco-vscode-api` spike (ADR-0004 stage 3), Open VSX client, automated compatibility suite, decision ADR on a Node extension host |
| 7.2 Packaging | Windows installer (MSVC, windeployqt), macOS signed + notarised dmg (macdeployqt), Linux AppImage and Flatpak; LGPL compliance bundle; size budget |
| 7.3 Reliability | Crash reporting (opt-in), daemon auto-restart, session recovery, update mechanism |
| 7.4 Performance | Startup time, memory per window, large-workspace explorer, large-output streaming |
| 7.5 Accessibility | Screen-reader pass on all three OSes, high contrast, keyboard audit |
| 7.6 Security review | Threat model walkthrough, sandbox escape tests, dependency audit, legal review of licensing (ADR-0011) |
| 7.7 Contributor docs | Provider authoring guide, adapter authoring guide, architecture tour |
