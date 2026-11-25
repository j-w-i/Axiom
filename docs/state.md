# State (handover)

Updated 2026-11-25. M1 is merged (j-w-i/Axiom#14). M2 so far is merged (j-w-i/Axiom#15). Branch `m3/accounts`.

## Where things stand
- **M0**: done except the owner's reading of decisions 0006 (Google plans via Antigravity; Gemini CLI's consumer tier has ended) and 0007 (subscription lanes; written confirmation from Anthropic before launch).
- **M1**: merged. What its exit still needs a real window for is K1, K2 and K8.
- **M2**: agents run. Policy, spawn with a sandbox, ten tools (read, write, edit, patch, list, glob, search, shell, job, fetch), codecs for Anthropic, OpenAI (Responses and Chat), Gemini and OpenRouter, the loop with compaction, approvals, accounts, CLI and app. A correctness review is filed with what became of each finding (`reviews/2026-11-25-m2-correctness.md`). Left of it: seccomp on Linux, branching, Bedrock. **The independent security review has not been done** (K32).
- **M3**: begun. `axiom-harness` drives Claude Code, Codex and ACP agents; host and app set them up, ask before each start, show and journal what they do. No real one has been started (K40). Claude Code can have several accounts, each in a folder of its own where a person signs in themselves (K44). Left: MCP for them, Copilot.
- **M5**: begun. Several agents in a session, of any harness, added in the app; an agent gives others work with the tool `agent` (list, tell, wait, read), and a person is asked whenever that would widen what it may. Left: a worktree for each writer, conflicts, task graphs.
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
| The packaged application | `scripts/package.sh`; its host started and answered its CLI. Its window has not been opened | macOS 27 arm64 |
| Anthropic | fixtures from its documentation only; no key has been used | |
| Release build | `flutter build macos` | macOS 27 arm64 |
| Host and Markdown benchmarks | `scripts/bench.sh` | M4 Pro |
| Linux | CI only, see the pull request | |
| Windows | CI only, see the pull request (K31: until 2026-11-25 the app's tests had hung there, unnoticed) | |

## Not verified
The real window has not been opened since the owner asked for none (K8); frame times on a display with many panes (K1); anything on Linux or Windows beyond CI; accessibility with a real screen reader.

## Next
1. Owner: say whether a real window may be opened once, when convenient, to check chrome and frame times.
2. Owner: say whether the real Claude Code or Codex on this computer may be started once, to see a delegated agent work (K40).
3. Somebody with the standing to do it: the security review (K32).
4. M3's and M5's rest, as above.

## How to resume
`AGENTS.md`, this file, `roadmap.md`, then the decision for the area. `scripts/check.sh` must pass before and after. Preferences the owner has stated are in `product/requirements.md` (PR-4, PR-44) and `design/system.md`.
