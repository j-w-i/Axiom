# Axiom visual design (M1.6)

_Status: **approved by ADR-0013** (2026-11-17); **implemented (M1.6, 2026-11-18)**: the "Precision Instrument"
synthesis, with decisions D2–D10 as recommended. Nothing is implemented in the
production shell yet; Phase B tasks (`docs/tasks/M1.6/`) start now._

Milestone M1.6, **Axiom Visual Experience**, gives the Tauri shell its own
visual identity and interaction system. This directory holds the design
evidence, the approved proposal and the specifications that implementation tasks follow.

| Document | What it holds |
|----------|---------------|
| [creative-brief.md](creative-brief.md) | Goals, constraints, anti-patterns, success criteria |
| [research.md](research.md) | Sourced study of FL Studio, Resolve, Blender, Zed, Ableton, Bitwig (+ Islands, Linear); adopt/adapt/reject |
| [concepts.md](concepts.md) | Audit of the current shell; three concepts; weighted evaluation; the recommended synthesis; decisions D1–D10 (decided: ADR-0013) |
| [design-system.md](design-system.md) | Foundations (colour, material grammar, type, density, focus), tokens, components |
| [motion-system.md](motion-system.md) | Durations, curves, choreography, looping-animation policy, reduced motion |
| [workspace-spec.md](workspace-spec.md) | Every surface: workspace, console, palette, settings, states; M2/M3 surfaces prepared but not built |
| [frontend-architecture.md](frontend-architecture.md) | How the React shell carries the system: files, layers, rules, what stays untouched |
| [performance.md](performance.md) | Baseline, budgets, prototype measurements, procedures |
| [visual-qa.md](visual-qa.md) | Screenshot workflow, review checklist, visual acceptance criteria |
| `concepts/` | Screenshots of the prototypes (1440×900; `@2x` crops) and of the current shell |

The milestone itself (scope, epics, exit criteria) is in
`docs/roadmap/milestones.md` §M1.6, its tasks in `docs/tasks/M1.6/`, and the
prototypes in `experiments/e5-visual-concepts/` (isolated; nothing in the
product depends on them).

## Running the prototypes

```sh
cd experiments/e5-visual-concepts
pnpm install
pnpm dev                     # http://localhost:5190/ — index of every concept and state
node scripts/shots.mjs       # regenerate screenshots/ (headless Chrome, 1440x900)
node scripts/perf.mjs        # frame-time probe; node scripts/idle-cpu.mjs for idle CPU
```

URL parameters: `?c=r|a|b|c` (r = recommended), `theme=dark|light`,
`view=workspace|environments|settings|components`, `palette=commands|files`,
`dock=0`, `panel=0`, `toast`. Shortcuts work as in the shell: Ctrl+K, Ctrl+P,
Ctrl+B, Ctrl+J, Ctrl+L (agent dock), Ctrl+S (toast), Ctrl+Tab.
