# Performance budgets and benchmarks

Budgets come from measurement where one exists (marked ✔) and are targets otherwise. Reference machine: M4 Pro, 48 GB, 120 Hz. Report p50, p99, worst. Display-bound benchmarks run on CI or with the owner's explicit yes, never on the owner's desktop unasked.

| Scenario | Budget | Now |
|----------|--------|-----|
| Frame build + raster, one agent at 100 tok/s + 10 live strips | p99 ≤ 4 ms, none over 8.33 ms | ✔ 0.35/1.62 + 0.94/1.38 ms (prototype) |
| Same, 10 visible transcripts, 2 000 blocks each | p99 ≤ 8.33 ms | not measured |
| Transcript of 50 000 blocks, append | ≤ 2 × the cost at 300 blocks | ✔ widget test, debug, offscreen; display run owed (K1) |
| Client decode | ≤ 2 µs/event, ≤ 1 % core at 1 k events/s | ✔ 0.8 µs (prototype) |
| Host: 10 agents × 100 tok/s, journalled | ≤ 6 % of a core, ≤ 40 MB (first target: 5 %) | ✔ 2.9–3.1 % over three runs, 9.2–9.5 MB |
| Host: 100 agents × 100 tok/s | ≤ 35 % of a core, no resync, commit to client p99 ≤ 50 ms | ✔ 16–18 %, 30.5 MB, p99 19–22 ms, worst 23–43 ms, 0 resyncs |
| Journal commit, 20 events | p99 ≤ 1 ms | ✔ 0.096 ms (prototype) |
| Host start to ready | p99 ≤ 50 ms | ✔ 17 / 17–23 ms |
| Client cold start to first frame | ≤ 700 ms | not measured |
| Idle, 1 session open | ≤ 0.5 % CPU, ≤ 180 MB client + 20 MB host | not measured |
| Markdown, incremental, 4 KB chunk | p99 ≤ 1 ms at any document size | ✔ p50 49 µs, p99 ≤ 205 µs at 100 KB, 1 MB and 10 MB (worst 3.8 ms) |
| Soak: 8 h, 10 agents | memory slope < 1 MB/h after the first hour | M7, short version weekly |

Run: `scripts/bench.sh` (host and headless client benchmarks).

Host figures: M4 Pro, macOS 27, 2026-11-25, `cargo run --release --example bench`, replayed scripts (their parsing and timers are included). Three runs interleaved with the build before the review fixes, on a machine that was not idle (load 6–7); no figure got worse beyond its spread. Before group commit the 100-agent worst case was 366 ms.
