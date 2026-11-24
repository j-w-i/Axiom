# Vision

Axiom is an AI agent harness and multi-agent orchestration platform for the desktop.

**The user chooses the intelligence, the execution environment, the tools, the authentication
method and the orchestration strategy. Axiom makes them work together.**

Two products in one, both serious:

1. **A native agent engine** that competes with Claude Code and Codex on its own merits.
2. **A superior environment around the harnesses people already use**, through their structured
   interfaces, keeping what is specific to each.

What exists today and what Axiom does about it (evidence: `../research/`):

| Today | Axiom |
|-------|-------|
| Every orchestrator studied is Electron or a TUI; memory under many streams is their common complaint | Native client, Rust engine, flat memory, published benchmarks |
| Supervision means watching many scrolling logs | A deck of agents, one queue of things that need a person, and a score of what happened when |
| Permissions are a mode per thread, enforced by whichever harness is running | One capability model, visible per agent, child never above parent |
| "Usage" is a percentage of unclear origin | Every number carries its provenance: reported, measured, estimated, unknown |
| A model, a provider, a login and a harness are blurred into one dropdown | Five separate things, shown separately |
| Subscriptions are used by borrowing a first-party client's identity | Only authorised routes, named plainly; otherwise delegation to the vendor's own binary |

Not goals: an IDE, a chat wrapper for one CLI, a hosted service, a marketplace.
