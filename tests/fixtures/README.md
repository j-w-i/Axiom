# Test fixtures

## `basic/`
A tiny workspace the daemon integration tests (`crates/axiomd/tests/`) copy
into a temp directory and open. Committed files:

| File | Purpose |
|------|---------|
| `README.md` | plain UTF-8 text |
| `src/main.rs`, `src/lib.rs` | nested directory, ordering |
| `.gitignore` | ignores `target/` and `*.log` |
| `notes/crlf.txt` | CRLF line endings that must survive read/write byte-for-byte |
| `data/invalid-utf8.bin` | bytes `ff fe 00 41`: not valid UTF-8 |

`.gitattributes` disables text conversion so the CRLF and binary files are
stored exactly. Not committed (because git would honour the fixture's own
`.gitignore`), but created by the test harness after copying: an empty `.git/`
directory, `target/debug.o`, and `build.log`.
