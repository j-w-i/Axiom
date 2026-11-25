# State (handover)

Updated 2026-11-25. Merged: M1 (j-w-i/Axiom#14), M2 so far (j-w-i/Axiom#15), installed programs and signing in with ChatGPT (j-w-i/Axiom#16), copies for agents (j-w-i/Axiom#18), what needs a person (j-w-i/Axiom#19), results that reach who asked (j-w-i/Axiom#20), the design spec's steps 1–16 (j-w-i/Axiom#21), the redesign (j-w-i/Axiom#22), the groundwork for taking back what was said (j-w-i/Axiom#23). What is worth building (`product/worth-building.md`) is in the roadmap.

## Where things stand
- **M0**: done except the owner's reading of decisions 0006 (Google plans via Antigravity; Gemini CLI's consumer tier has ended) and 0007 (subscription lanes; written confirmation from Anthropic before launch).
- **M1**: merged. What its exit still needs a real window for is K1, K2 and K8.
- **M2**: agents run. Policy, spawn with a sandbox, ten tools (read, write, edit, patch, list, glob, search, shell, job, fetch), codecs for Anthropic, OpenAI (Responses and Chat), Gemini and OpenRouter, the loop with compaction, approvals, accounts, CLI and app. A correctness review is filed with what became of each finding (`reviews/2026-11-25-m2-correctness.md`). What `product/worth-building.md` has for it is built but for branching (W-54), dropping a folder on the window (W-17: needs a plugin or code of each system's own, and a real window to see) and parts of the settings (W-14: zoom, code font, notifications, limits). Left of it: seccomp on Linux, branching, Bedrock. **The independent security review has not been done** (K32).
- **M3**: begun. `axiom-harness` drives Claude Code, Codex and ACP agents; host and app set them up, ask before each start, show and journal what they do. No real one has been started (K40). Claude Code can have several accounts, each in a folder of its own where a person signs in themselves (K44). They are given the tool `agent` by a server of the host's own (K41). Claude Code can work with Axiom's tools, questions and sandbox in place of its own (K46). Axiom's own loop can run on a ChatGPT plan: signing in, renewing, using, leaving (0013); nobody has signed in yet (K47). Left: Copilot.
- **M5**: begun. Several agents in a session, of any harness, added in the app; an agent gives others work with the tool `agent` (list, tell, wait, read), and a person is asked whenever that would widen what it may. An agent can work in a copy of the folder of its own; a person sees what it changed, with clashes, and brings it in (0014; reviewed for lost work, `reviews/2026-11-25-copies.md`). What waits for a person is one queue over all sessions (`host.waiting`), with what failed or finished and nobody has seen. What an agent was told to do reaches who asked by itself, once, also over a restart. Left: task graphs, copies that go by themselves (K48), and what `research/t3code-comparison.md` ranks (owner, 2026-11-25: look at what T3 Code does better).
- **M4**: what has been changed in a folder is shown (git in the sandbox, not on Windows). Nothing else.
- **M6, M7**: nothing but `scripts/package.sh`.

## What was verified, and where
| What | How | OS |
|------|-----|----|
| v1 archive | refs compared; bundle restored, 357 commits | GitHub, macOS |
| Wire, journal, host, CLI (137 tests), including kill -9, locked and damaged journals, slow and silent clients, strangers, signals | `scripts/check.sh rust`; 117 deliberate breakages in two rounds; three survivors, judged equivalent or harmless; of the reviewer's own 46, seven survived, four of them real gaps (K30) | macOS 27 arm64 |
| Client through the real host process, kill -9 and reconnect | `app/test/host/e2e_test.dart` | macOS 27 arm64 |
| Controls, contrast, transcript, shell, goldens, with the host tests above 293 tests in all | `scripts/check.sh app`, offscreen | macOS 27 arm64 |
| Policy, spawn, tools, model, engine, agents through the host's socket | `scripts/check.sh rust`; escapes are tried for real (`spawn/tests`), the host is "killed" at every point of a run (`engine/tests/run.rs`), the shell reader is compared with bash, zsh and sh | macOS 27 arm64 |
| An agent on a model of this machine | by hand, ignored tests `live_on_lm_studio`, `five_tasks_on_lm_studio` (Qwen3 4B, LM Studio 0.4.25): 4 of 5 tasks, three times; the fifth it answers without running the command it was asked to run | macOS 27 arm64 |
| OpenAI, Gemini, OpenRouter | fixtures from their documentation only; no key has been used | |
| Delegated agents | scripted programs through the real host (`host/tests/delegated.rs`, `peers.rs`); no real harness | macOS 27 arm64, Linux by CI |
| Copies for agents | `tools/copy_tests.rs`, `host/tests/copies.rs`, `delegated.rs` (a scripted program begins in its copy); 53 deliberate breakages, three survived (two behind another check, one without effect); after the review 33 more, one survives behind another check | macOS 27 arm64 |
| Signing in with ChatGPT | against a server of the tests that does as documented (`host/tests/signin.rs`, `model/src/chatgpt_tests.rs`), through the real host from the client (`e2e_test.dart`); 50 deliberate breakages, all caught (four only after the tests were made stronger). No real account, no browser | macOS 27 arm64 |
| The packaged application | `scripts/package.sh`; its host started and answered its CLI. Its window has not been opened | macOS 27 arm64 |
| Anthropic | fixtures from its documentation only; no key has been used | |
| Release build | `flutter build macos` | macOS 27 arm64 |
| Host and Markdown benchmarks | `scripts/bench.sh` | M4 Pro |
| Linux | CI only, see the pull request | |
| Windows | CI only, see the pull request (K31: until 2026-11-25 the app's tests had hung there, unnoticed) | |

## Not verified
The real window has not been opened since the owner asked for none (K8); frame times on a display with many panes (K1); anything on Linux or Windows beyond CI; accessibility with a real screen reader.

## The design spec (`design/spec-fable.md`)
Steps 1–16 are done; the owner chose B for D1, D3, D4 and D5 (2026-11-25). D2 stays as it is: step 17 is not built, and in `workbench-dark` it would have changed nothing (no run there is over four calls). One departure: at a ChatGPT plan's limit "Manage usage" is the primary key, as OpenAI's own page for whoever uses a plan asks; the spec says never. Eleven goldens were recorded again with the owner's yes (2026-11-25). From steps 3 and 4: `controls-*` (the empty box, step 4), `workbench-*` and `execution-dark` ("Deny" is 2 px further left, step 3; with it seven pixels of one letter in the sidebar's "web-store" in the wider ones, which come and go with where "Deny" stands and with nothing else). From steps 13 to 15: the subject of a request in `ink1`, the fill of the selected row and tab, the outline of a disabled key (`status-*` too, for its selected row).

## In progress: what was said and not heard
Number 5 of `research/t3code-comparison.md`. Done: in the journal what waits is taken back or heard and never both (`take_back`, `heard`, `put_message`); the loop and the delegated programs tell nobody what was taken back and do not wait for an answer to it. Not done: the call a client takes it back with, the event that says what waits and what does not any more, the run that would begin (`Slot::after`) for something that is no longer there, what other agents have said (not a person's to take back), changing it, and all of the client. Nothing of it can be reached yet. The loop's part is tested (`engine/tests/run.rs`; three deliberate breakages, all caught); the delegated programs' part has no test of its own until there is a call to take back with.

Design for the rest, agreed with nobody yet. A call `prompt.back {session, agent, item}` that answers with the words, refused for what another agent said (`FROM_AGENT` at its beginning in `pending`). An event `prompt.state {item, state}` with `waiting` (in one batch with the prompt's own events), `heard` and `back`; a call that comes too late says `heard` again, which mends a mark that a crash left. `Slot::after` asks the journal whether anything still waits before it begins another run, under its lock; `go` ends at once if the conversation's last turn is the agent's, for a word taken back after that. The client marks the item and puts the words into the draft, never over what is written.

## M2 from `product/worth-building.md` (`m2/talking`)
Commands are one table (`lib/ui/commands.dart`): a control that can be used by a key says so with `OnCommand` while it is on the screen. A yes by key works only for a question that has been seen to its end and has stood for half a second; four deliberate breakages of that, all caught. Find searches on another isolate and goes to a place by making it the origin. An item keeps its source beside its blocks, for copying, quoting and ↑; a session's words are therefore in memory twice. `agent.compact` is refused while something waits to be heard, or it would be summarised and never answered. The CLI's `say --wait` ends with 0 done, 2 failed, 3 a person is asked. What a person chose of the window is in `window.json` in the data folder; the host does not read it. Accounts keep ⌘, and settings have ⇧⌘, until the owner says otherwise.

## The look, round 12 (`design/flat`)
Built 2026-11-25 on the owner's yes to pictures (`design/system.md`, history). In the pictures and not in the app, because the host has nothing for it: a plan, levels of use per service in the header, a terminal, renaming and removing sessions. The folder window (`file_selector`, by the Flutter team, BSD: a person had to type a whole path) has been opened by nobody. `spec-fable.md` is older than this and describes what was before.

## The material, round 13 (`design/tactile`, on `design/flat`)
Unitable's depth on round 12's layout, at the owner's word (2026-11-25). Not measured: what the outlines cost a frame on a display (K1).

## Modern, rounds 14 to 19 (`design/modern`, on `design/tactile`)
Built on the owner's word (2026-11-25). In the pictures and not in the app, because the host has nothing for it: the plan, the terminals and the levels of use. The panel of the app holds what has changed. Accepted by the owner at round 19 ("this looks great"). The fifteen goldens were recorded again with their yes (2026-11-25): every one of them shows the new look. Opened once in a real window with the owner's yes (2026-11-25, macOS 27 arm64, release, specimen content, 120 Hz, Impeller): scrolling, build p99 2.0 ms and raster p99 3.3 ms; everything painted again, 3.7 and 4.5 ms; the first frame 63 ms. Opened again with their yes and photographed: the buttons are on the header's line (26, where they were 16 against 33), shades show no rings. Dragging by the header reaches the system (`MoveArea`; nothing did before), shown by events sent to a real window, not by a hand. Full screen has been looked at by nobody (K8).

## Next
1. Owner: drag the window by its header and try full screen (K8).
2. Owner: say whether the real Claude Code or Codex on this computer may be started once, to see a delegated agent work (K40).
3. Owner: sign in with ChatGPT once, if wanted (K47).
4. Somebody with the standing to do it: the security review (K32).
5. M3's and M5's rest, as above.

## How to resume
`AGENTS.md`, this file, `roadmap.md`, then the decision for the area. `scripts/check.sh` must pass before and after. Preferences the owner has stated are in `product/requirements.md` (PR-4, PR-44) and `design/system.md`.
