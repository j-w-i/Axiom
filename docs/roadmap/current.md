# Current state

_Last updated: 2026-11-11 (end of Phase 0, architecture session)._

## Where we are

**Phase 0 (architecture and planning) is complete. Milestone M1 has not
started.** No product code exists yet except the protocol crate.

| Area | State |
|------|-------|
| Research (`docs/research/`) | Written, sourced; open questions listed per file |
| Architecture, ADRs 0001–0011 | Accepted |
| IPC protocol 0.1 | Frozen; `crates/protocol` compiles, 15 tests pass, schemas generated |
| Editor bridge 1 | Frozen (document only; code in M1-EDITOR-001 / M1-SHELL-003) |
| Provider / diagnostics / inspection contracts | Draft types + schemas + tests; reference `providers/rust/provider.toml` |
| Agent contract | Draft document |
| Experiments E1–E3 | Passed (results in `experiments/README.md`) |
| `crates/rpc`, `store`, `workspace`, `exec`, `axiomd` | Not started |
| `editor/`, `apps/desktop/` | Not started |
| CI | Workflow file exists for the Rust job; **never run** (no remote yet) |

## Latest handoff

`agents/handoffs/2026-11-11-phase0-architecture.md`

## Next task

Run `scripts/tasks.py next`. At the time of writing: **M1-RPC-001**, and
wave A (`M1-RPC-001`, `M1-STORE-001`, `M1-WS-001`, `M1-EXEC-001`,
`M1-EDITOR-001`) is ready to run in parallel.

## Blockers

None.

## Things the next agent should know

- This machine (Fedora 44) had no Rust and no Qt development files. Phase 0
  installed Rust via rustup (`~/.cargo`) and Qt 6.11.2 + 6.12.0 via aqtinstall
  into `.toolchains/` (git-ignored, ≈ 4 GB). `source scripts/dev-env.sh` picks
  them up. On another machine run `scripts/bootstrap-qt.sh`.
- `sudo` is not available non-interactively here; OpenGL headers were
  extracted locally (`scripts/bootstrap-qt.sh --gl-headers`).
- `cargo` is not on the default `PATH`: scripts add `~/.cargo/bin`
  themselves; in an interactive shell run `source scripts/dev-env.sh`.
- There is no git remote. Commits are local on `main`.

## Open questions (do not block M1)

See `docs/architecture/open-questions.md`.

## Documents to read for M1

`AGENTS.md` → `agents/IMPLEMENTATION_GUIDE.md` → the task file → only the
documents that task lists under *Context*.
