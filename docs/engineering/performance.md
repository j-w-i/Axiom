# Performance budgets and benchmarks

Budgets come from measurement where one exists (marked ✔) and are targets otherwise. Reference machine: M4 Pro, 48 GB, 120 Hz. Report p50, p99, worst. Display-bound benchmarks run on CI or with the owner's explicit yes, never on the owner's desktop unasked.

| Scenario | Budget | Now |
|----------|--------|-----|
| Frame build + raster, one agent at 100 tok/s + 10 live strips | p99 ≤ 4 ms, none over 8.33 ms | ✔ 0.35/1.62 + 0.94/1.38 ms (prototype) |
| Same, 10 visible transcripts, 2 000 blocks each | p99 ≤ 8.33 ms | not measured |
| Transcript of 50 000 blocks, append | same as 200 blocks ± 20 % | not measured |
| Client decode | ≤ 2 µs/event, ≤ 1 % core at 1 k events/s | ✔ 0.8 µs (prototype) |
| Host: 10 agents × 100 tok/s, journalled | ≤ 5 % of a core, ≤ 40 MB | see `engine` bench |
| Host: 100 agents × 100 tok/s | ≤ 35 % of a core, no subscriber over budget | see `engine` bench |
| Journal commit, 20 events | p99 ≤ 1 ms | ✔ 0.096 ms (prototype) |
| Host start to ready | ≤ 50 ms | see `engine` bench |
| Client cold start to first frame | ≤ 700 ms | not measured |
| Idle, 1 session open | ≤ 0.5 % CPU, ≤ 180 MB client + 20 MB host | not measured |
| Markdown, 1 MB document, incremental | ≤ 16 ms per 4 KB chunk on the decode isolate | see `app/benchmark` |
| Soak: 8 h, 10 agents | memory slope < 1 MB/h after the first hour | M7, short version weekly |

Run: `scripts/bench.sh` (host and headless client benchmarks).
