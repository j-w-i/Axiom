# Documentation index

Each document has one purpose. If you are implementing a task, start at
`../AGENTS.md`, not here.

| Folder | Purpose | Authority |
|--------|---------|-----------|
| `product/requirements.md` | What we are building, as numbered requirements | Stable |
| `architecture/overview.md` | The architecture on one page | Normative |
| `architecture/modules.md` | Every module: responsibility, interface, allowed dependencies | Normative |
| `architecture/processes.md` | Process model, lifecycles, threading | Normative |
| `architecture/data-flow.md` | Step-by-step message flows | Normative for M1 |
| `architecture/security.md` | Trust levels, controls, known gaps | Normative |
| `architecture/design-system.md` | UI tokens, controls, layout, accessibility rules | Normative |
| `architecture/testing.md` | Test layers, rules, CI jobs | Normative |
| `architecture/governance.md` | Frozen/draft contracts, forbidden changes, change requests, task states | Normative |
| `architecture/open-questions.md` | Unresolved questions and accepted risks | Living |
| `decisions/` | ADRs (why) | Immutable once accepted |
| `protocols/ipc.md` | Shell/CLI/agent ↔ daemon | Frozen (0.1) |
| `protocols/editor-bridge.md` | Shell ↔ editor surface | Frozen (1) |
| `protocols/providers.md` | Toolchain provider model | Draft until M2 |
| `protocols/agents.md` | Agent runtime, tools, permissions | Draft until M3 |
| `protocols/inspection.md` | Visual inspection and verification | Draft until M5 |
| `research/` | Sourced technology research; evidence for the ADRs | Reference (dated 2026-11-11) |
| `roadmap/current.md` | Where the project is right now | Living — update every session |
| `roadmap/milestones.md` | Milestones → epics → exit criteria | Living |
| `roadmap/dependency-graph.md` | Task/milestone dependencies, parallel waves, merge order | Living |
| `roadmap/verification/` | Recorded milestone exit verifications | Append-only |
| `tasks/` | Task specifications and their status | Living |
| `design/` | M1.6 visual design: brief, research, concepts and evaluation, design system, motion, surface specs, front-end architecture, performance budgets, visual QA | Normative for M1.6 (approved by ADR-0013) |
| `migration/qt-to-tauri/` | Qt → Tauri evaluation: audit, parity, performance, design system, prototype, recommendation, proposed M1.5 roadmap | Proposal — not normative until ADR-0012 is accepted |
| `contributing/dev-setup.md` | Toolchain setup and commands | Living |

Precedence when documents disagree: code in `crates/protocol` → `protocols/`
→ ADRs → `architecture/` → task files → `research/`. Fix the lower one.
