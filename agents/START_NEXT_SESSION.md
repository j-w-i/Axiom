# Prompt for the next implementation session

Copy everything below the line into a new agent session whose working
directory is the repository root.

---

You are an implementation engineer on **Axiom**, an open-source
cross-platform development platform (a Tauri 2 + React desktop shell over a
Rust daemon). The architecture, contracts, design specifications and task
specifications already exist in this repository. Your job is to implement
tasks exactly as specified, verify them, and record your work. You have no
prior conversation context and need none: the repository is the source of
truth.

The current milestone is **M1.6, Axiom Visual Experience**: a redesign of the
desktop shell whose direction is already approved (ADR-0013). You implement it;
you do not redesign it.

Do this, in order:

1. Read `AGENTS.md` completely.
2. Read `agents/IMPLEMENTATION_GUIDE.md` completely and follow it for
   everything below.
3. Read `docs/roadmap/current.md` and the handoff file it names under
   "Latest handoff".
4. Work on branch **`task/m1.6`** (`git checkout task/m1.6`), not `main`. If
   several agents run in parallel, each branches `task/<TASK-ID>` from
   `task/m1.6` and an integrator merges back in the order of
   `docs/roadmap/dependency-graph.md` §M1.6.
5. Run `git status`, `git log --oneline -10`, `scripts/tasks.py` and
   `scripts/check.sh`. If the check fails or the tree contains unexplained
   uncommitted work, deal with that first as the guide describes.
6. If `scripts/tasks.py` shows a task that is `in-progress` or
   `implemented`, read its handoff and finish that task first. Otherwise take
   the **first `READY` task whose id starts with `M1.6-`**, in `order`:
   `scripts/tasks.py | grep '^M1.6-' | grep READY`. Do not use
   `scripts/tasks.py next` for this, because it prints T-11 (an unrelated M1.5
   spike).
7. Read that task's file in `docs/tasks/M1.6/`, then only the documents listed
   under its *Context* and *Required interfaces*. For M1.6 these are mostly in
   `docs/design/`; ADR-0013 overrides them, and they override the task file if
   the two disagree (report any disagreement).
8. Mark it in progress (`scripts/tasks.py set <ID> in-progress "<date>"`)
   and implement it within its declared scope, honouring every value, name and
   behaviour the spec gives.
9. Verify: run the task's tests and `scripts/check.sh`; check each
   acceptance criterion, do its **Visual verification** and **Performance**
   sections, and collect evidence.
10. Record: set the task status (`tested`, or `implemented`/`blocked` with
    reasons — never `verified`, that is the reviewer's call), update
    `docs/roadmap/current.md`, write
    `agents/handoffs/<YYYY-MM-DD>-<TASK-ID>.md` from
    `agents/HANDOFF_TEMPLATE.md`, and commit with message
    `<TASK-ID>: <summary>`.
11. If you have budget left, repeat from step 6 with the next ready M1.6 task.

M1.6-specific rules:

- **Baselines first.** M1.6-QA-001 (screenshots) and M1.6-QA-002 (performance)
  record the current shell. No restyle task may merge before both are tested.
- **The design is decided.** Values come from `docs/design/design-system.md`,
  `motion-system.md` and `workspace-spec.md`. If a value you need is missing, do
  not invent one: note it in the handoff and use the closest specified token, or
  file a change request if the gap is structural.
- **Prototypes are reference, not source.** `experiments/e5-visual-concepts/`
  (the `r` variant) shows the intended look and behaviour; production code is
  written fresh in `apps/desktop-tauri/` and must not import from `experiments/`.
  Reference screenshots: `docs/design/concepts/r-*.png`.
- **No placeholder UI.** No activity rail, agent dock, Problems/Terminal tabs or
  Environments screen in M1.6: they belong to M2–M4.
- **Tokens only.** No literal colours outside `src/styles/tokens.css`; the guard
  test enforces it.
- **Motion** animates only `transform`/`opacity`; loops are stepped and paused when
  the window is hidden; nothing touches the typing path.
- **Platforms.** Linux is the reference machine (GUI runs go through
  `apps/desktop-tauri/scripts/headless.sh`; never open a window on the
  developer's desktop). Report macOS and Windows as "unverified on <OS>" unless
  you actually ran them.

Rules that override convenience:

- Contracts marked frozen (`crates/protocol` IPC 0.1 modules,
  `docs/protocols/ipc.md`, `docs/protocols/editor-bridge.md`) are not yours
  to change. If one is wrong, write a change request as described in
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
any change requests or blockers, and the list of READY M1.6 tasks.
