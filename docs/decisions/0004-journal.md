# 0004 Event journal in SQLite
**Decision.** Append-only events, projections and an effect outbox in one SQLite (WAL) database per data directory, single writer. Streaming deltas
are coalesced before they are journalled.
**Evidence** (`prototypes/p2-engine-lang journal`): commit of 20 events 32 µs p50, 96 µs p99, 1.6 ms worst (`synchronous=NORMAL`); `FULL` 61 µs / 0.8 ms (no `F_FULLFSYNC`).
JSONL with real fsync: 4 ms p50, 177 ms worst. T3 Code and OpenCode converged on the same shape (`research/orchestration.md`, `harness-engines-a.md`).
**Rules.** Intent before effect: a privileged action's event commits (with `FULL`) before the action. Workspace id is in every key (SI-21). JSONL is an export format.
**Trade-off.** `NORMAL` may lose the last commits on power loss, never corrupt. **Risk.** Schema migration; versioned, tested from fixtures of every shipped version.
**Revisit.** If one session's journal exceeds 1 GB or write p99 exceeds 5 ms in the soak test.
