# Testing

Nothing is complete until its tests exist and pass on the OSes CI runs. Tests never open a window (PR-44), never use the network, never use a real account, never touch the user's data directory.

| Layer | How |
|-------|-----|
| Rust unit, property (`proptest`) | beside the code |
| Protocol contract | `proto` fixtures decoded by Rust **and** Dart; round-trip; unknown fields tolerated |
| Journal | crash injection (kill -9 mid-commit), truncation, replay equals projections, migration fixtures |
| Host | real process, real socket: auth, slow subscriber, resync, reconnect, abrupt client death |
| Streams | chunk-invariance: any split of a stream yields the same items; duplicates, reordering, truncation, garbage |
| Providers (M2) | recorded, sanitised SSE fixtures per codec; a scripted local server for faults (429, 529, mid-stream cut, malformed JSON) |
| Harnesses (M3) | replayed frame journals through the real adapter |
| Widgets | every control: name, role, keyboard path, focus, disabled, reduced motion, 2× text |
| Tokens | contrast of every text colour on every surface it may sit on, both themes; no literal colour elsewhere |
| Goldens | component sheets and the workbench, dark/light/narrow, on one OS (macOS) with a small tolerance |
| Security | one test per SI invariant as its code lands; fuzz parsers (frames, markdown, shell, patch) |
| Performance | `performance.md`; regressions fail CI |

A failing test is fixed or the code is; it is never skipped, weakened or re-recorded to get green. Flaky is failing.
