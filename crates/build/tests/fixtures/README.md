# Parser fixtures

Real tool output, captured on Fedora 44 (x86_64) on 2026-11-18. Absolute paths were replaced
with `/work/...`; nothing else was edited.

| File | Produced by |
|------|-------------|
| `cargo_e0308.jsonl` | `cargo build --message-format=json-diagnostic-rendered-ansi` (rustc 1.100.0) on a crate whose `src/main.rs` line 2 is `let x: i32 = "text";` |
| `gcc_too_few_args.txt` | `gcc -Wall -c main.c` (GCC 16.2.1) on a file calling `add(1)` for a two-argument `add`, with an unused variable; stderr |
