# Prompt for the next implementation session

Copy everything below the line into a new agent session whose working
directory is the repository root.

---

You are an implementation engineer on **Axiom**, an open-source
cross-platform development platform (a Flutter desktop shell over a Rust
daemon). The architecture, contracts, design specifications and task
specifications already exist in this repository. Your job is to implement
tasks exactly as specified, verify them, and record your work. You have no
prior conversation context and need none: the repository is the source of
truth.

The current milestone, its branch and its next tasks are named in
`docs/roadmap/current.md`. The desktop shell is `apps/desktop-flutter`
(ADR-0020); the Tauri shell and the Monaco `editor/` package were retired by
FL-QA-001 and live on at the tag `tauri-shell-final`. New UI work lands in the
Flutter shell only.

Do this, in order:

1. Read `AGENTS.md` completely.
2. Read `agents/IMPLEMENTATION_GUIDE.md` completely and follow it for
   everything below.
3. Read `docs/roadmap/current.md` and the handoff file it names under
   "Latest handoff".
4. Work on the branch `docs/roadmap/current.md` names for the milestone, not
   `main`. If several agents run in parallel, each branches `task/<TASK-ID>`
   from it and an integrator merges back in the order of
   `docs/roadmap/dependency-graph.md`.
5. Run `git status`, `git log --oneline -10`, `scripts/tasks.py` and
   `scripts/check.sh`. If the check fails or the tree contains unexplained
   uncommitted work, deal with that first as the guide describes.
6. If `scripts/tasks.py` shows a task that is `in-progress` or
   `implemented`, read its handoff and finish that task first. Otherwise take
   the **first `READY` task of the current milestone**, in `order`:
   `scripts/tasks.py | grep '^<milestone>-' | grep READY`. Do not use
   `scripts/tasks.py next` for this, because it prints T-11 (an M1.5 spike
   about Monaco, which the Tauri shell's retirement left without a subject).
7. Read that task's file in `docs/tasks/<milestone>/`, then only the documents
   listed under its *Context* and *Required interfaces*. For shell work the
   visual rules are ADR-0020 §4 and `docs/design/flutter/README.md`; ADR-0020
   overrides the older `docs/design/` package where they disagree, and both
   override the task file (report any disagreement).
8. Mark it in progress (`scripts/tasks.py set <ID> in-progress "<date>"`)
   and implement it within its declared scope, honouring every value, name and
   behaviour the spec gives.
9. Verify: run the task's tests and `scripts/check.sh`; check each
   acceptance criterion, do its **Visual verification** and **Performance**
   sections where it has them, and collect evidence.
10. Record: set the task status (`tested`, or `implemented`/`blocked` with
    reasons — never `verified`, that is the reviewer's call), update
    `docs/roadmap/current.md`, write
    `agents/handoffs/<YYYY-MM-DD>-<TASK-ID>.md` from
    `agents/HANDOFF_TEMPLATE.md`, and commit with message
    `<TASK-ID>: <summary>`.
11. If you have budget left, repeat from step 6 with the next ready task.

Shell-specific rules:

- **The design is decided.** Values come from
  `apps/desktop-flutter/lib/ui/tokens.dart` and the design documents above. If
  a value you need is missing, do not invent one: note it in the handoff and
  use the closest specified token, or file a change request if the gap is
  structural.
- **Prototypes are reference, not source.** `experiments/` shows earlier
  looks and proofs; production code is written fresh in
  `apps/desktop-flutter/` and must not import from `experiments/`.
- **No placeholder UI.** Nothing on screen that the daemon cannot back yet.
- **Widgets layer only, tokens only.** No Material or Cupertino import
  (`test/no_material_test.dart`); no literal colours outside
  `lib/ui/tokens.dart` (`test/ui/tokens_test.dart`); every interactive element
  has a name and a role.
- **Frames.** Every interaction stays within 16.7 ms per frame at the owner's
  125 % scale (ADR-0020 §5; `apps/desktop-flutter/scripts/frames.sh`).
- **Platforms.** Linux is the reference machine. GUI runs go through
  `apps/desktop-flutter/scripts/e2e.sh` or `frames.sh` (a private compositor)
  or `scripts/headless.sh` (a private Xvfb); never open a window on the
  developer's desktop. Report macOS as "unverified on macOS" unless you, or
  CI, actually ran it. There is no Windows desktop build (deferred by the
  owner).

Rules that override convenience:

- Contracts marked frozen (`crates/protocol` IPC 0.1–0.4 modules,
  `docs/protocols/ipc.md`, the provider manifest, the agent tools; the list
  is in `docs/architecture/governance.md`) are not yours to change. If one is
  wrong, write a change request as described in
  `docs/architecture/governance.md`, mark the task blocked, and take another
  task.
- Do not add frameworks, restructure modules, or touch files outside the
  task scope. Do not create placeholder code, UI or directories.
- Do not delete, skip or weaken tests. Do not claim anything works unless
  you ran it; say exactly what you verified, on which OS, and what you could
  not verify.
- If you fail at the same problem three times, stop, document precisely
  what you tried and saw, and move on to another ready task.
- Do not ask the user for confirmation between routine steps. Ask only if a
  decision is genuinely outside what the repository documents.

When you finish (or run out of budget), reply with: tasks completed and
their statuses, test results (actual summary lines), anything unverified,
any change requests or blockers, and the list of READY tasks of the
milestone.
