# Test matrix

Legend: **Passed** · **Failed** (defect found; issue id) · **Partial** (some states tested) ·
**Blocked** (could not be run; reason) · **Not tested** (reason).

"Explore" = `evidence/harness/audit_explore_test.dart` (debug, private KWin, real daemon),
"Repro" = `audit_repro_test.dart`, "Perf" = `audit_perf_test.dart` (profile), "e2e" = the
repository's 15 suites (`apps/desktop-flutter/scripts/e2e.sh`), "Gate" = `scripts/check.sh`.

## Environments

| Environment | Status | Notes |
|-------------|--------|-------|
| Linux (Fedora 44), Wayland (private KWin), scale 1 | Passed/Failed per row | all GUI runs |
| Linux, scale 1.25 (GTK renders at dpr 2) | Partial | perf, frames, startup only |
| Linux, X11 / Xvfb | Not tested | `e2e.sh` prefers KWin when present; the X11 path of the runner (no direct commit) was not exercised |
| Linux packaged .deb/.rpm (`check.sh package`) | Not tested | time budget; covered by CI per `verification/FL.md` |
| macOS | Blocked | no macOS host available |
| Windows | Blocked | no desktop build exists (deferred by the owner) |
| Light / dark theme | Passed (visual) | `02-*`, `36`–`39` |
| dpr 2 | Passed | `41-dpr2.png` |
| Text scale 1.5 | Failed | AUD-020 |

## Automated suites

| Suite | Status | Notes |
|-------|--------|-------|
| `check.sh rust` (fmt, clippy, 532 tests, schema, deny) | Passed | actionlint skipped (not installed); `preview.rs` may skip silently (AUD-060) |
| `check.sh flutter` (format, analyze, 89 tests, licences) | Passed | |
| `check.sh inspectors` | Passed | run as part of `check.sh rust` (pnpm present) |
| e2e: a11y, agent, chat, console, editor, env, git, inspect_loop, inspector, lsp_install, search, shell | Passed | |
| e2e: languages (11), lsp (2 of 3), env_rust | Passed-by-skip | need network installs (`AXIOM_TEST_LSP_INSTALL=1`); reported as passed (AUD-060) |
| `frames.sh` | Failed | palette open/close 6 frames over (AUD-062), splitter 1 |
| `check.sh e2e` on macOS / Windows | Blocked | no host |

## Screens and features

| Area | Feature / state | Status | Evidence / issue |
|------|-----------------|--------|------------------|
| Startup | connect, welcome | Passed | `01`, `02`; startup 255–480 ms (PERFORMANCE_REPORT) |
| Startup | first-ever cold launch ≤ 1 s | Failed (marginal) | 1042 ms, known in `FL.md` |
| Title row | File/View/Help menus open, Escape closes | Passed | `03-*` |
| Title row | menu placement near edges | Not tested | code finding AUD-013; not reachable at the tested sizes |
| Title row | narrow (< 1040 px) title row | Passed | `40-size-1024x700.png` |
| Rail | each destination with and without a folder | Passed | `04-*`, exploratory steps |
| Explorer | lazy listing, 3 000 entries, ignored entries, Git colours | Passed | `05`, `06` |
| Explorer | deep nesting (18 levels) | Failed | AUD-016 |
| Explorer | keyboard (arrows) | Passed | e2e shell_test; exploratory |
| Explorer | Tab traversal | Failed | AUD-021 |
| Explorer | create/rename/delete | Failed (absent) | AUD-022 |
| Explorer | external add/remove reflected | Partial | deletion of the whole folder: AUD-066; single-file changes via e2e |
| Editor | open, highlight, edit, save (Mod+S), dirty mark | Passed | `08`, `14`, exploratory |
| Editor | undo / redo | Passed | exploratory |
| Editor | CRLF file not dirty | Passed | exploratory |
| Editor | mixed line breaks | Not tested | code finding AUD-023 |
| Editor | binary file | Partial | message shown; banner persists (AUD-063) |
| Editor | 20 000-char line | Passed | `12-long-line.png` |
| Editor | 50 000 / 100 000-line file: open | Failed (perf) | AUD-015 |
| Editor | 100 000-line file: typing | Failed (perf) | AUD-005 |
| Editor | close dirty → prompt; Escape keeps it | Passed | `15` |
| Editor | external change (clean reload, dirty conflict) | Partial | works; unsafe default AUD-018 |
| Editor | file deleted while dirty | Failed | AUD-019 |
| Editor | split right | Passed at 1440 px; Failed narrow | `18`; AUD-004 |
| Editor | Git gutter marks | Failed | AUD-003 |
| Editor | many tabs | Failed | AUD-017 |
| Editor | find/replace bar | Not tested | code finding AUD-053; e2e covers basic find |
| Editor | completion, hover, F12, rename (real servers) | Blocked | rust-analyzer not installed for this toolchain (rustup stub) and network installs were not run; fake-lsp e2e passed |
| Editor | LSP install offer / Not now | Partial | banner seen; Install not clicked (downloads) |
| Palette | open, query, empty result, Escape | Passed | `19`–`21` |
| Palette | 30× rapid open/close | Passed (functional) | `45`; frames AUD-062 |
| Go to File | 3 000 files fuzzy | Passed | `22` |
| Go to line | Ctrl+G | Passed | e2e editor_test |
| Search | 3 000 files, empty result, odd query | Passed | `23`, `24`, `24b` |
| Search | long-line performance (daemon) | Failed | AUD-006 |
| Source Control | changes list, diff, untracked | Passed | `25`, `26` |
| Source Control | stage/commit | Not tested | e2e git_test covers; not exercised in the exploratory run |
| Environments | detection, catalogue | Passed (functional) | `27`; AUD-068 |
| Environments | install / remove | Not tested | downloads; covered by CI install jobs |
| Inspector | start preview | Not tested in exploratory run | e2e inspect_loop_test passed (real Vite page) |
| Chat | setup page, no model | Passed | `04-nofolder-chat`, `29` |
| Chat | conversation with a real model | Blocked | no API key / agent configured; e2e agent/chat tests use fakes and passed |
| Chat | restart mid-reply | Not tested | code finding AUD-025 |
| AI dock | open/close | Passed | `30`; narrow layout AUD-110 |
| Console | run command, exit status, failure | Passed | `33` |
| Console | 200 000-line flood | Passed | perf `console_flood` |
| Console | output without newlines (30 MB) | Not tested | code finding AUD-026 |
| Console | background grandchild | Failed | AUD-007 (CLI) |
| Terminal | new terminal, prompt | Passed | `34` |
| Settings | open, sections, theme/motion | Passed | `35`, `39`; AUD-046 by code |
| Settings | at minimum window | Not tested | code finding AUD-047 |
| Layout | window 1440×900, 1180×760, 1024×700 | Passed | |
| Layout | 900×560 (minimum) with split / console max | Failed | AUD-004, AUD-014 |
| Layout | 800×600, 640×480 | Not applicable | below the enforced minimum; recorded only |
| Layout | side bar + dock at max on default window | Failed | AUD-014 |
| Folder switch | old documents cleared | Failed | AUD-002 |
| Folder deleted while open | message, recovery | Failed | AUD-066 |
| Daemon crash | failure page, restart | Partial | page and restart work (416 ms); unsaved work lost (AUD-001) |
| Restart | folder, tabs, theme restored | Passed | repro R9; e2e shell_test |
| Window size restored | — | Failed (known) | owed in `FL.md` |
| Shell crash recovery | — | Not tested | owed in `FL.md` (no implementation) |
| Shutdown | quits, stops daemon | Passed | 3–9 ms (release) |
| Accessibility | names/roles | Passed | 0 unlabelled actionable nodes; e2e a11y_test |
| Accessibility | reduced motion (in-app) | Partial | "Off" works; "Reduced"/OS pref AUD-054 |
| Accessibility | screen reader | Not tested | no AT available in the private compositor |
| Idle | CPU with folder + document | Passed | 0 % |
| Idle | CPU with a running task, focused/unfocused | Not tested | time; code finding AUD-080 |
| Memory | repeated open/close | Passed (plateau) | AUD-083 |
| Multi-client daemon (CLI + GUI) | — | Not tested at runtime | code findings AUD-028–030 |
| Agent permission gate | — | Not tested at runtime | needs a model; code findings AUD-009, 010, 039–041 |
| Security: untrusted workspace with build scripts | — | Not tested | would execute untrusted code; AUD-008 by code |
| Packaging | deb/rpm install and self-test | Not tested | see Environments |
