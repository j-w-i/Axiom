# 0010 Block-incremental transcript
**Decision.** Closed blocks are immutable and cached; only the open tail is re-split and re-laid out, once per frame, inside its own repaint boundary; the list is lazy and anchored to its end.
**Evidence** (`prototypes/p3-transcript`, profile build, M4 Pro, 120 Hz, one pane at 100 tokens/s plus ten live strips)
| | build p50 / p99 / max | raster p50 / p99 | frames over 8.33 ms |
|-|-|-|-|
| whole-document, 200 blocks | 1.41 / 2.79 / 4.08 ms | 0.93 / 1.56 | 0 of 796 |
| whole-document, 5 000 blocks | 260 / 283 / 283 ms | 0.98 / 4.63 | 29 of 29 |
| incremental, 200 blocks | 0.35 / 1.62 / 1.78 ms | 0.94 / 1.38 | 0 of 781 |

**Not yet measured:** incremental at 50 000 blocks and with ten visible panes; the run was stopped at the owner's request (it opened windows). Owed by CI (`engineering/performance.md`).
