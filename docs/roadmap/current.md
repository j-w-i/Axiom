# Current state

_Last updated: 2026-11-21._

## M2 — Environments and builds: complete (2026-11-21)

All 14 tasks tested; CI green on Linux and macOS (PR j-w-i/Axiom#3, the exit scenario `e2e/env.mjs` 6/6 on Ubuntu). Merged to main under the owner's standing permission to merge a milestone once complete. Owed: the owner's by-hand run of the exit scenario. Windows deferred (owner): known gaps in `verification/M2.md` §Not done. **Next: M3 (agent runtime)**, then M4 and M5, worked autonomously (owner, 2026-11-21).

## M3 — Agent runtime: planned (2026-11-21)

ADR-0015 records the decisions (protocol 0.3 `agent/*`, `crates/agent`, `apps/cli`, keychain credentials, shadow-git checkpoints through the `git` CLI, Unix socket + token for more clients, MCP server as a protocol client, in-house MCP codec). 12 tasks in `docs/tasks/M3/`. Branch `task/m3`, draft PR j-w-i/Axiom#4. Next task: the first READY `M3-*` by order.

- **M3-ARCH-001 tested (2026-11-21):** protocol 0.3 (`crates/protocol/src/agent.rs`: 14 methods, 4 notifications, 3 error kinds, optional `authToken`/`capabilities` on initialize); `agents.md` frozen; `agent`/`cli` edges. `axiomd::NOT_YET_SERVED` lists the agent methods until wired.
- **M3-MODEL-001 tested (2026-11-21):** `crates/agent` with the message model, `ModelProvider`, an in-house SSE parser, the Anthropic Messages adapter (text, tools, opaque thinking, errors, cancel) and `ScriptedProvider`; tested against a loopback fixture server.



## M1.6 — Axiom Visual Experience: approved, Phase B ready (2026-11-17)

The owner approved the recommended direction, **"Precision Instrument"**, and
accepted decisions D2–D10 as recommended. **ADR-0013** records them, together with the
material grammar, the motion policy and the scope rule, and amends ADR-0011 to allow
OFL-1.1 font assets (IBM's unmodified files only). M1.6-DESIGN-001 is tested. Work
lives on branch `task/m1.6`.

| # | Decision |
|---|----------|
| D1 | Synthesis "Precision Instrument" (A + B's signals + C's restraint) |
| D2 | Bundle IBM Plex Sans/Mono (IBM's unmodified Latin1 subsets) |
| D3 | macOS overlay title bar; Windows/Linux native decorations themed with the setting |
| D4 | No activity rail until M2 |
| D5 | `appearance.motion` Standard/Reduced/Off |
| D6 | Stderr not painted red |
| D7 | Drag-to-reorder tabs in M1.6 |
| D8 | Radix Tooltip |
| D9 | Restyle in place, task by task; no feature flag |
| D10 | Light theme with equal care |

- Design package: `docs/design/` (normative for M1.6). Prototypes:
  `experiments/e5-visual-concepts/`. Reference screenshots: `docs/design/concepts/r-*.png`.
- Milestone: `docs/roadmap/milestones.md` §M1.6; 27 tasks in `docs/tasks/M1.6/`;
  graph and waves: `dependency-graph.md` §M1.6.
- **M1.6-QA-001 is tested (2026-11-17):** screenshot harness (`scripts/e2e.sh shots.mjs`,
  `e2e/shots-compare.mjs`) and the Linux visual baseline in
  `docs/roadmap/verification/M1.6/baseline/linux/`.
- **M1.6-QA-002 is tested (2026-11-17):** performance baseline `bench/results/JWI-Fedora-20261117-061550.json`; six new bench scenarios; `docs/design/performance.md` §1 is now measured. Both baselines exist.
- **M1.6-SYSTEM-001 is tested (2026-11-17):** generated theme tokens (`scripts/gen-themes.mjs` + `themes.json`, freshness step in `check.sh`), legacy aliases until QA-006, extended guard tests. Owner accepted CR-M1.6-SYSTEM-001-line-contrast (option A: `--line-input` for field borders).
- **M1.6-SHELL-001 is tested (2026-11-17):** `App.tsx` 433 → 117 lines, shell modules in `src/shell/`, 0 px change. **Wave B is complete.**
- **M1.6-SYSTEM-002 is tested (2026-11-17):** IBM Plex bundled (143 kB, unmodified IBM files). `bench/run.sh --ab <baseline>` now compares two builds interleaved, so before/after checks no longer need an idle machine.
- **M1.6-SYSTEM-003 is tested (2026-11-17):** control primitives in the material grammar, component gallery (`pnpm run gallery`, S17 in the shots harness), `ui/` import guard.
- **M1.6-SYSTEM-004 is tested (2026-11-17):** overlays (Menu, ContextMenu, Tooltip via Radix Tooltip 1.4.1, Dialog, Sheet, Toast, Banner) in `src/ui/overlays.tsx`.
- **M1.6-INTERACTION-001 is tested (2026-11-17):** motion level setting, presence, loop gate; idle CPU with a task running 8.0% → ~2.8% focused, 7.0% → ~0.3% unfocused.
- **M1.6-SHELL-002 is tested (2026-11-18):** workbench frame (Panel, Splitter, layout v2 + Reset Layout, focused region); one React commit per splitter drag (was 63–68).
- **M1.6-SHELL-003 is tested (2026-11-18):** title row with menus, workspace switcher, command field and task transport; macOS overlay title bar configured (drag permission pending, see handoff).
- **M1.6-SHELL-004 is tested (2026-11-18):** explorer tree restyle (guides, glyph chips, focus marker, unsaved dot); S15 harness now captures real keyboard focus.
- **M1.6-SHELL-005 is tested (2026-11-18):** status bar segments and the hint line (data-hint on every shell control).
- **M1.6-SHELL-006 is tested (2026-11-18):** the Console (header meta and tools, recessed `$` prompt, stderr muted not red, D6). The bench `output` scenario had silently measured nothing since SHELL-002 and is repaired.
- **M1.6-SHELL-007 is tested (2026-11-18):** Welcome, backend, opening, empty-folder, listing-error and no-document states; the explorer's error row no longer overlaps the next row.
- **M1.6-EDITOR-001 is tested (2026-11-18):** tab strip (raised active key, transform-placed lit edge, glyph chips, dirty dot/close slot) and path breadcrumbs.
- **M1.6-EDITOR-002 is tested (2026-11-18):** Monaco themes generated from `--syn-*`/`--editor-*`, Plex Mono 13/20 default, region focus Mod+0/1/\`; fixed a load-dependent race in a stores test (likely the unexplained SYSTEM-003 gate failure).
- **M1.6-INTERACTION-002 is tested (2026-11-18):** side bar / console show-hide motion (content slides and fades, column changes once, Monaco lays out once per toggle). E2E and bench runs on Linux now use a private webview profile per suite/run (they wrote the developer's real localStorage before).
- **M1.6-SURFACES-001 is tested (2026-11-18):** command palette redesign (mode chip, gliding selection, keycaps, file match highlight, footer, recent-first commands, empty states). Found: WebKit loses a localStorage write made < ~1 s before quit (host follow-up).
- **M1.6-SURFACES-002 is tested (2026-11-18):** two-pane Settings dialog (Appearance with the Motion control, Editor, Workspace, About folded in; AboutSheet removed); modal overlays now layer above the workbench (the console and splitters had painted over the scrim).
- **M1.6-INTERACTION-003 is tested (2026-11-18):** tab lit edge glides; menus, dialogs, palette, settings, tooltips and toasts enter and exit (Radix data-state); focus returns to the opener as an overlay starts closing. Resting screens unchanged (68/68).
- **M1.6-SURFACES-003 is tested (2026-11-18):** light contrast matrix in the guard (dark output pinned), light press depth, native title bar follows the theme on Windows/Linux (`core:window:allow-set-theme`). Open for the owner: light state texts on `--surface-chrome` are 4.1–4.5:1 (not in §2.3; proposed values in the handoff).
- **M1.6-INTERACTION-004 is tested (2026-11-18):** editor tabs reorder by drag (lift, siblings make room, brass slot line, settle) and by Mod+Shift+PageUp/PageDown (announced); the order persists with the tab state (D7).
- **M1.6-QA-003 is tested (2026-11-18):** the accessibility suite `e2e/a11y.mjs` exists and runs (15/19 on Linux); 4 defects (D-A11Y-1..4, light primary button and chrome state-text contrast, Tab in the editor, small targets) await the owner, and the Orca manual pass needs a person. Record: `docs/roadmap/verification/M1.6.md` §A11y.
- **M1.6-QA-004 is tested (2026-11-18):** performance measured against the QA-002 build (interleaved): P1, P2, P6, P7, P9–P12 met; P3, P4 frame p95, P5, P6 cold open and P8 (4.1 %) not met and waived by the owner in writing (`M1.6.md` §Performance).
- **M1.6-QA-005 is tested (2026-11-18):** the automated half passes on macOS and Windows in GitHub Actions (run 44250978913; screenshot sets in `verification/M1.6/{macos,windows}/`); two platform defects (D-X-1 caret drift, D-X-2 Windows localStorage on quit) and the manual hardware items (VoiceOver, NVDA, High Contrast, title bar, HiDPI) remain. Record: `M1.6.md` §Cross-platform. Since then the owner's Mac session fixed D-X-1 (a test measurement), the traffic lights, drag, zoom and the full-screen inset; screen-reader passes waived; Windows deferred.
- **M2 planned (2026-11-18):** 14 task files in `docs/tasks/M2/` (contracts, env registry/detection, archive/delegate/system installs, environments, build engine and diagnostics, rail + Environment page, install-plan dialog, Problems view, node/cmake+ninja/python providers, exit QA). Decisions in `docs/decisions/ADR-0014-environments-and-builds.md` — **accepted 2026-11-18**; M2 in progress. Work branch `task/m2`.
- **M2-ARCH-001 tested (2026-11-18):** protocol 0.2 defined (`crates/protocol/src/{env,build}.rs`, four error kinds, `ipc.md` §Protocol 0.2); provider manifest and diagnostics frozen for M2; `env`/`build` edges in `modules.md`. The daemon reports 0.2 but serves none of the additions yet: `axiomd::NOT_YET_SERVED` lists them, and each M2 task removes what it wires.
- **M2-ENV-001 tested (2026-11-18):** `crates/env` (registry, validation, detection; built-ins compiled in); `axiomd` serves `env/providers`, `env/detect`, `env/trust` (trust stored per workspace, store migration 2). Gap for ENV-003: delegate methods have no `bin_dirs`.
- **M2-ENV-002 tested (2026-11-18):** managed archive installs (HTTPS + rustls/native roots, SHA-256 before use, content-addressed cache, safe extraction, staging + atomic rename, single-use 10-minute plans, cancel cleanup, exact uninstall); `env/versions`, `env/planInstall`, `env/install` (+ `env/installProgress`, `env/installed`), `env/uninstall` served. Test-only `AXIOM_TEST_INSECURE_LOOPBACK_HTTP=1`. Draft PR j-w-i/Axiom#3 runs CI on the branch.
- **M2-ENV-003 tested (2026-11-18):** delegate installs. Each managed Rust version root bootstraps its own rustup from rustup-init (published SHA-256), homes inside the root, `RUSTUP_TOOLCHAIN` pinned; uninstall runs `rustup toolchain uninstall` then removes the root. Real install on Linux: 30 s, 543 MiB, HOME untouched; CI runs it on all three OSes (`--features ci-install`). Additive manifest fields: `file` format, archive `provides`/`setup`, delegate `bin_dirs`/`versions`.
- **M2-ENV-004 tested (2026-11-18):** system package installs — manager detection (distro-native first), the exact elevated argv in the plan (`pkexec`, or `sudo -A` with an askpass helper; brew unelevated; winget/UAC), approved plans only, output streamed as progress, PATH verification. Real apt install through a pkexec shim runs in Linux CI.
- **M2-ENV-005 tested (2026-11-18):** `.axiom/project.toml` → `env/resolve` (`Environment`: PATH prepend, vars, toolchains); `.axiom/project.lock` written atomically through the workspace only on change (versions + download digests from install receipts); `env/detect` reports the selection; processes get the environment through the env overlay. All `env/*` methods are served.
- **M2-BUILD-001 tested (2026-11-18):** `crates/build` — provider tasks in the project environment, pure parsers (cargo_json, gcc_text, regex, sarif; golden over real cargo/gcc output + fuzz), 10 Hz diagnostics batches with the complete batch before `task/exited`, last 20 runs per workspace in the store. `build/tasks` and `build/run` served: the daemon now serves every protocol 0.2 method (`NOT_YET_SERVED` removed, the exact registry checks restored).
- **M2-UI-001 + M2-UI-002 tested (2026-11-19):** activity rail (Files, Environments), Environment Manager page, install-plan dialog showing the daemon's plan verbatim, install progress on the page and in the transport, Remove with confirmation, trust and error states. Smoke 24a–e, a11y S18/S18d, shots S18–S18d.
- **M2-UI-003 tested (2026-11-19):** console view strip (Output / Problems), Problems grouped by file with Enter → file at line/column (bridge-1 `revealPosition`), provider tasks in the palette and as the transport's Rerun target. Note for e2e: run `shots.mjs` in its own `e2e.sh` invocation (it switches Xvfb to 2880x1800, where keyboard focus does not reach the app; UI-001 handoff).
- **M2-PROV-001 tested (2026-11-19):** built-in `node` provider (nodejs.org archives verified against SHASUMS256.txt; npm tasks); `strip_prefix` on JSON version sources; pre-releases never offered. Node 22.11.0 round trip: 3.7 s, 29.2 MB download.
- **M2-PROV-002 tested (2026-11-19):** `cmake` (Kitware archives, SHA-256.txt) and `ninja` (pinned per-version digests from GitHub's asset digests); a C project builds with the Ninja generator from the project environment.
- **M2-PROV-003 tested (2026-11-19):** `python` via a bootstrapped uv, every uv location inside the root, HOME untouched; installer now re-points absolute symlinks from staging to the final root.
- **M2-QA-001 tested (2026-11-19):** the exit scenario passes through the UI on a PATH without Rust (`e2e/env.mjs` 6/6, also in CI on Ubuntu); P2 +17/+5 ms, P7 unchanged. Owed: the owner's by-hand run. `docs/roadmap/verification/M2.md`.
- **M1.6-QA-006 is tested (2026-11-18): M1.6 is complete** on Linux and macOS (deviations approved by the owner); legacy aliases removed (pixel-identical); every exit criterion in `M1.6.md` §Exit review. Windows is deferred. Merged to main (PR j-w-i/Axiom#2, 2026-11-18).
- **Next:** see `scripts/tasks.py | grep '^M1.6-' | grep READY` (`scripts/tasks.py | grep '^M1.6-' | grep READY`).
- _(Wave B history:)_ M1.6-QA-002 (performance baseline and new scenarios),
  M1.6-SYSTEM-001 (generated tokens), M1.6-SHELL-001 (split `App.tsx`, no visual
  change). The QA baselines must be captured before the first restyle merges.
- **Inherited blocker:** M1.6-QA-005 (macOS/Windows review) needs real hosts or a
  CI run, as the M1.5 gate did.

## Qt shell retired (T-15), 2026-11-17

At the owner's instruction the **Qt shell is removed**. `apps/desktop/`, its CI
job, `bootstrap-qt.sh`, `check_qml.py`, `smoke.sh` and `dev-env.sh` are deleted;
`scripts/check.sh` has no Qt branch and `scripts/run.sh` launches the Tauri shell.
Recovery: tag `qt-shell-final` (`c9f3d82`), e.g.
`git checkout qt-shell-final -- apps/desktop scripts`. Earlier the same day T-14
made Tauri the default.

**Owner waivers** (recorded in `verification/M1.5.md` §6b): T-14's precondition
that T-12 be passed; §5.1 (a milestone as default), §5.2 (30 days of green CI;
the window started 2026-11-16 and keeps running as evidence) and §5.5 (the manual
accessibility run). Risk accepted: no real macOS or Windows launch of the
installers and no screen-reader pass preceded the removal; those remain the first
things to do (`agents/MANUAL_VERIFICATION.md`). The `emit` test helper and the Qt-vs-Tauri
comparison in `apps/desktop-tauri/bench/` are left in place and are now dead weight.

## Where we are

**M1 (platform foundation) is complete; M1.5 (the Qt → Tauri migration) is
closed by owner decision with the T-12 parity gate not formally passed.**
ADR-0012 is accepted: the desktop shell is Tauri 2 + React and the Qt shell is
gone (tag `qt-shell-final`).

| Area | State |
|------|-------|
| Architecture, ADRs 0001–0012 | Accepted (0012 supersedes the Qt shell decision) |
| IPC protocol 0.1 | Frozen; `crates/protocol`, schemas generated and checked |
| IPC protocol 0.2 (M2 additions) | Types frozen by M2-ARCH-001; handlers land task by task (`axiomd::NOT_YET_SERVED`) |
| Editor bridge 1 | Frozen; implemented in `editor/` (43 Vitest tests) and shared by both shells |
| Backend (`crates/{rpc,store,workspace,exec,axiomd}`) | M1, 227 tests, unmodified by M1.5 |
| `crates/desktop-bridge` | Tested: 13 behaviour tests against the real daemon (incl. stderr forwarding) + 12 unit tests (framing, daemon lookup order) |
| `apps/desktop-tauri` | M1.5 shell: root-workspace Rust host + React UI; 5 host-surface + 3 release-guard tests, 51 Vitest tests, 51 end-to-end checks in 4 suites (smoke 22, quit 8, geometry 5, parity 13) through two WebDriver servers |
| CI | The repository is public (checked 2026-11-18), so GitHub-hosted runners, macOS and Windows included, have no minute cap; the 2026-11-12 "budget nearly exhausted" note no longer constrains runs. `.github/workflows/qa-cross-platform.yml` runs QA-005's automated half on pushes to `qa/**`. Last run 43146126579: Linux+macOS green, Windows at typing-fixed/commands-pending |
| Qt shell | Retired by T-15 (2026-11-17); recover from tag `qt-shell-final` |
| M1.5 gate (T-12) | **Not passed** (superseded by the owner's T-14/T-15 decisions above): on Linux everything is verified (parity 52/56, all ten performance budgets, 48 end-to-end checks, package launches/finds its sidecar/connects) and macOS went green end to end in run 43141643434; Windows still fails at typing-free stages and KD-WIN-1 (Qt) is unchanged; macOS/Windows budgets, the manual protocol and the E-17 waiver are open — `docs/roadmap/verification/M1.5.md` |

## Latest handoff

`agents/handoffs/2026-11-19-M2-QA-001.md`. Earlier: `agents/handoffs/2026-11-19-M2-PROV-003.md`, `agents/handoffs/2026-11-19-M2-PROV-002.md`, `agents/handoffs/2026-11-19-M2-PROV-001.md`, `agents/handoffs/2026-11-19-M2-UI-003.md`, `agents/handoffs/2026-11-19-M2-UI-001.md` (+ `-M2-UI-002.md`), `agents/handoffs/2026-11-18-M2-BUILD-001.md`, `agents/handoffs/2026-11-18-M2-ENV-005.md`, `agents/handoffs/2026-11-18-M2-ENV-004.md`, `agents/handoffs/2026-11-18-M2-ENV-003.md`, `agents/handoffs/2026-11-18-M2-ENV-002.md`, `agents/handoffs/2026-11-18-M2-ENV-001.md`, `agents/handoffs/2026-11-18-M2-ARCH-001.md`, `agents/handoffs/2026-11-18-M1.6.md` (milestone), `agents/handoffs/2026-11-18-M1.6-QA-004.md`, `agents/handoffs/2026-11-18-M1.6-QA-003.md`, `agents/handoffs/2026-11-18-M1.6-INTERACTION-004.md`, `agents/handoffs/2026-11-18-M1.6-SURFACES-003.md`, `agents/handoffs/2026-11-18-M1.6-INTERACTION-003.md`, `agents/handoffs/2026-11-18-M1.6-SURFACES-002.md`, `agents/handoffs/2026-11-18-M1.6-SURFACES-001.md`, `agents/handoffs/2026-11-18-M1.6-INTERACTION-002.md`, `agents/handoffs/2026-11-18-M1.6-EDITOR-002.md`, `agents/handoffs/2026-11-18-M1.6-EDITOR-001.md`, `agents/handoffs/2026-11-18-M1.6-SHELL-007.md`, `agents/handoffs/2026-11-18-M1.6-SHELL-006.md`, `agents/handoffs/2026-11-18-M1.6-SHELL-005.md`, `agents/handoffs/2026-11-18-M1.6-SHELL-004.md`, `agents/handoffs/2026-11-18-M1.6-SHELL-003.md`, `agents/handoffs/2026-11-18-M1.6-SHELL-002.md`, `agents/handoffs/2026-11-17-M1.6-INTERACTION-001.md`, `agents/handoffs/2026-11-17-M1.6-SYSTEM-004.md`, `agents/handoffs/2026-11-17-M1.6-SYSTEM-003.md`, `agents/handoffs/2026-11-17-M1.6-SYSTEM-002.md`, `agents/handoffs/2026-11-17-M1.6-SHELL-001.md`, `agents/handoffs/2026-11-17-M1.6-SYSTEM-001.md`, `agents/handoffs/2026-11-17-M1.6-QA-002.md`, `agents/handoffs/2026-11-17-M1.6-QA-001.md`, `agents/handoffs/2026-11-17-M1.6-DESIGN-001.md` (M1.6 approval), `agents/handoffs/2026-11-12-M1.5.md`.

## Next task

**M2**, on branch `task/m2`: take the first `READY` task whose id starts with `M2-`, in `order`
(`scripts/tasks.py | grep '^M2-'`). M2 is implemented; next is the owner's review and merge of PR j-w-i/Axiom#3, then M3 planning.

## M1.5 follow-ups (open by owner waiver; not blocking M1.6)

These need macOS/Windows hardware or a CI run. T-14 and T-15 already ran by
waiver; the items stay open as evidence for the M1.5 record:

1. **Spend one CI run on the branch as it stands** (`task/m1.5`, 7 commits
   ahead of origin). It should turn the Windows cell green — typing now works
   there (`editContext: false`) and the command checks no longer use a POSIX
   shell — re-confirm macOS (green in run 43141643434), and produce the NSIS
   package. Expect exactly one known red cell after it: the Qt `desktop` job on
   Windows (KD-WIN-1), which disappears with T-15.
2. **Measure the budgets on macOS and Windows** (`bench/run.sh --runs 3
   --heavy-runs 3`, quoting the `medians` block and the load) — exit criterion
   3 needs them there, and Linux is done (all ten rows met).
3. **Run `agents/MANUAL_VERIFICATION.md`** on one real machine per OS and
   record the results in `docs/roadmap/verification/M1.5.md`. This is also
   where trusted keyboard input, IME, clipboard, drag and drop, HiDPI, screen
   readers, the 720x480 drag minimum and installing a package are verified on
   macOS and Windows.
4. **Close the three open parity rows**: S-8's screen reader and S-12's native
   folder dialog are manual-protocol items; T-4 is the bench's `output`
   scenario. (E-17 was waived by the owner on 2026-11-12.)

(T-14 and T-15 ran on 2026-11-17 by owner waiver; see the top of this file.)

## Blockers

Two, both outside the code:

- **GitHub Actions budget** for the month is spent (owner, 2026-11-12). CI is
  not available until it resets, so every local-only item is done and the rest
  waits for one batched run.
- **No macOS or Windows machine** in this environment: their budgets, package
  installs and the manual protocol need real hosts.

## Things the next agent should know

- This machine (Fedora 44) had no Rust and no Qt development files. Phase 0
  installed Rust via rustup (`~/.cargo`) and Qt 6.11.2 + 6.12.0 via aqtinstall
  into `.toolchains/` (git-ignored, ≈ 4 GB). `source scripts/dev-env.sh` picks
  them up. On another machine run `scripts/bootstrap-qt.sh`.
- `sudo` is not available non-interactively here; OpenGL headers were
  extracted locally (`scripts/bootstrap-qt.sh --gl-headers`).
- `cargo` is not on the default `PATH`: scripts add `~/.cargo/bin`
  themselves; in an interactive shell run `source scripts/dev-env.sh`.
- There *is* a remote: `origin` = github.com/j-w-i/Axiom. M1.5 work lives on
  `task/m1.5` (draft PR #1); `main` is untouched by the migration.

## M1.5 — Qt → Tauri migration (in progress, 2026-11-12)

ADR-0012 is **accepted**: the desktop shell becomes Tauri 2 + React. Tasks:
`docs/tasks/M1.5/`. Evidence and design: `docs/migration/qt-to-tauri/`.
Verification record: `docs/roadmap/verification/M1.5.md`.

Done and verified on Linux: T-01 (decisions), T-02 (bridge in
`crates/desktop-bridge`), T-03 (host in the root workspace, `--version`), T-04
(generated wire types), T-05 (test parity, 107 Qt functions mapped, 52 of 56
matrix rows ✅), T-06 (UI gaps), T-07 (in-window menus + macOS menu, the latter
compiled by cross-check), T-08 (crash recovery; the macOS hook is built but
never triggered), T-09 (CI wiring plus the end-to-end suites through two
WebDriver servers), T-13 (deb + rpm + macOS `.app` built in CI; a `.deb` also
verified as an installed copy: it finds its sidecar and completes the
handshake).

Done as recorded experiments: M5-PRE-001 (E4: preview over CDP, option (a)
adopted), T-11 (analysis; recommends *not* adopting editor isolation until one
of three stated triggers occurs).

Open: T-10 and T-12 (manual protocol and the gate — macOS and Windows need real
hosts). T-14 and T-15 are done by owner waiver (see the top of this file).

## Open questions (do not block M1)

See `docs/architecture/open-questions.md`.

## Documents to read for M1

`AGENTS.md` → `agents/IMPLEMENTATION_GUIDE.md` → the task file → only the
documents that task lists under *Context*.
