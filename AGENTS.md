# AGENTS.md

Read this, then `docs/state.md` (where things stand, what is verified, what is next). The repository is the source of truth; conversation history is not.

Axiom is a desktop AI agent harness and multi-agent orchestrator: a Flutter client (`app/`) over a Rust host (`engine/`). Vision, requirements: `docs/product/`. Architecture: `docs/architecture/overview.md`. Why: `docs/decisions/`.
v1 is archived (`docs/archive.md`); do not import its code or decisions.

## Commands
```sh
scripts/check.sh          # everything CI runs: Rust, Dart, generated files, licences
scripts/check.sh rust|app # one half
scripts/bench.sh          # host and headless client benchmarks
cd app && flutter test    # also writes build/shots/*.png (offscreen)
```

## Rules
- **Never open a window on the owner's desktop**, take focus, or start a GUI from a test, benchmark or screenshot. Render offscreen. If a real display is needed, ask and wait.
- Show a screen before a redesign; the owner's verdicts are in `docs/design/system.md`.
- Never claim what was not run. Say on which OS. Placeholders, mock integrations and invented numbers are defects. Specimen content lives only in `app/lib/gallery/` and tests.
- Security invariants SI-1…SI-26 (`docs/security/threat-model.md`) are requirements. One spawn path. No secret in context, log, argv, env or journal. Fail closed.
- Only authorised provider routes (decision 0007).
- Provider, model, connection, harness, environment are five things. Several connections per provider is normal.
- No GPL/AGPL code. New dependency = reason in the commit. New crate edge = update `docs/architecture/overview.md`.
- Client: widgets layer only (no Material/Cupertino); colours, durations and springs only in `lib/ui/tokens.dart`; every control has a name, a role and a keyboard path; nothing expensive on the UI isolate; no unbounded buffer.
- Rust: `unsafe` forbidden outside `axiom-spawn`; no `unwrap` outside tests; errors are typed; every queue is bounded.
- The wire is `engine/crates/proto`; `docs/protocol/fixtures.jsonl` is written by it and decoded by the Dart tests. Change them together.
- Tests: never skip, weaken or re-record to get green. Fix the cause.
- Work on a branch; one concern per commit; imperative summary. No attribution lines.
- Update `docs/state.md` before you stop.
