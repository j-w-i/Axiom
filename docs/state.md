# State (handover)

Updated 2026-11-25. Branch `m1/foundations`, pull request j-w-i/Axiom#14 into `main`.

## Where things stand
- **M0**: done except the owner's reading of decisions 0006 (Google plans via Antigravity; Gemini CLI's consumer tier has ended) and 0007 (subscription lanes; written confirmation from Anthropic before launch).
- **M1**: in progress. Design accepted by the owner at round 6 (`design/system.md`). Host, journal, wire, Dart client, transcript, live shell, gallery, tests, goldens, gate and CI exist.
- **M2 and later**: nothing. There is no agent engine, no provider, no harness adapter, no tool, no sandbox. The app can list sessions and show replays; it cannot run an agent.

## What was verified, and where
| What | How | OS |
|------|-----|----|
| v1 archive | refs compared; bundle restored, 357 commits | GitHub, macOS |
| Wire, journal, host (34 tests), including kill -9, slow clients, strangers | `scripts/check.sh rust`; seven deliberate breakages, each caught | macOS 27 arm64 |
| Client through the real host process, kill -9 and reconnect | `app/test/host/e2e_test.dart` | macOS 27 arm64 |
| Controls, contrast, transcript, shell, goldens (84 tests) | `scripts/check.sh app`, offscreen | macOS 27 arm64 |
| Release build | `flutter build macos` | macOS 27 arm64 |
| Host and Markdown benchmarks | `scripts/bench.sh` | M4 Pro |
| Linux, Windows | CI only, see the pull request | |

## Not verified
The real window has not been opened since the owner asked for none (K8); frame times on a display with many panes (K1); anything on Linux or Windows beyond CI; accessibility with a real screen reader.

## Next
1. Owner: say whether a real window may be opened once, when convenient, to check chrome and frame times.
2. Finish M1's exit list (`roadmap.md`), K1, K2, K7, K8.
3. M2 in its stated order: `axiom-policy` and `axiom-spawn` first (with the SI tests), then the Anthropic Messages codec on recorded fixtures, then the Chat codec against LM Studio on this Mac.

## How to resume
`AGENTS.md`, this file, `roadmap.md`, then the decision for the area. `scripts/check.sh` must pass before and after. Preferences the owner has stated are in `product/requirements.md` (PR-4, PR-44) and `design/system.md`.
