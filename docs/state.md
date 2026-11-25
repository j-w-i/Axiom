# State (handover)

Updated 2026-11-25. Branch `m1/foundations`, pull request j-w-i/Axiom#14 into `main`.

## Where things stand
- **M0**: done except the owner's reading of decisions 0006 (Google plans via Antigravity; Gemini CLI's consumer tier has ended) and 0007 (subscription lanes; written confirmation from Anthropic before launch).
- **M1**: built. Design accepted by the owner at round 10 (`design/system.md`). Host, journal, wire, Dart client, transcript, live shell, gallery, tests, goldens, gate and CI exist. Engine and client each went through three passes of an independent review (`reviews/`); no P0 or P1 is known. What its exit still needs a real window for is K1, K2 and K8.
- **M2**: under way on `m2/native-engine` (pull request j-w-i/Axiom#15, stacked on #14). Agents run: policy, spawn with a sandbox, eight tools, two provider codecs, the loop with compaction, approvals, accounts, CLI and app. Left of it: more codecs (Responses and Gemini are being written), background jobs, the fetch tool with the egress proxy, seccomp on Linux, the five-task comparison of a local model with a recorded cloud one, branching. **The independent security review has not been done** (K32).
- **M3 and later**: nothing.

## What was verified, and where
| What | How | OS |
|------|-----|----|
| v1 archive | refs compared; bundle restored, 357 commits | GitHub, macOS |
| Wire, journal, host, CLI (137 tests), including kill -9, locked and damaged journals, slow and silent clients, strangers, signals | `scripts/check.sh rust`; 117 deliberate breakages in two rounds; three survivors, judged equivalent or harmless; of the reviewer's own 46, seven survived, four of them real gaps (K30) | macOS 27 arm64 |
| Client through the real host process, kill -9 and reconnect | `app/test/host/e2e_test.dart` | macOS 27 arm64 |
| Controls, contrast, transcript, shell, goldens, with the host tests above 293 tests in all | `scripts/check.sh app`, offscreen | macOS 27 arm64 |
| Policy, spawn, tools, model, engine, agents through the host's socket | `scripts/check.sh rust`; escapes are tried for real (`spawn/tests`), the host is "killed" at every point of a run (`engine/tests/run.rs`), the shell reader is compared with bash, zsh and sh | macOS 27 arm64 |
| An agent on a model of this machine: reads, edits, writes a file | by hand, ignored tests `live_on_lm_studio` (Qwen3 4B, LM Studio 0.4.25) | macOS 27 arm64 |
| Anthropic | fixtures from its documentation only; no key has been used | |
| Release build | `flutter build macos` | macOS 27 arm64 |
| Host and Markdown benchmarks | `scripts/bench.sh` | M4 Pro |
| Linux | CI only, see the pull request | |
| Windows | CI only, see the pull request (K31: until 2026-11-25 the app's tests had hung there, unnoticed) | |

## Not verified
The real window has not been opened since the owner asked for none (K8); frame times on a display with many panes (K1); anything on Linux or Windows beyond CI; accessibility with a real screen reader.

## Next
1. Owner: say whether a real window may be opened once, when convenient, to check chrome and frame times.
2. Finish M1's exit list (`roadmap.md`), K1, K2, K7, K8.
3. M2's rest, as above; then M3 (delegated harnesses).

## How to resume
`AGENTS.md`, this file, `roadmap.md`, then the decision for the area. `scripts/check.sh` must pass before and after. Preferences the owner has stated are in `product/requirements.md` (PR-4, PR-44) and `design/system.md`.
