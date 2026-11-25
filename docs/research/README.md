# Research (M0, 2026-11-24)

| Report | Evidence | Confidence |
|--------|----------|------------|
| [harness-engines-a](harness-engines-a.md) Codex, OpenCode, Pi, Goose | source at named commits | high; nothing was run |
| [harness-engines-b](harness-engines-b.md) Gemini CLI, Cline, Roo, Aider, Continue; Cursor and Claude Code from documentation | source / docs | high / medium |
| [orchestration](orchestration.md) T3 Code, Emdash, Vibe Kanban, Zed, Sculptor and others | source; creative-tools section is from knowledge | medium–high |
| [harness-integration](harness-integration.md) interfaces and **harness capability matrix** | schemas, SDK source, `--help` | high; no harness was driven |
| [subscription-auth](subscription-auth.md) | policy pages, dated | verdicts carry their own confidence |
| [providers](providers.md) codecs and **provider capability matrix** | adapter source, live catalogues, vendor docs | tagged per claim; second pass (the first was rejected as recall) |
| [local-inference](local-inference.md) | server source, LM Studio docs | Ollama, llama.cpp, mlx-lm, vLLM high; the rest unverified |
| [sandboxing](sandboxing.md), [libraries](libraries.md) | advisories, source, Seatbelt probes on this Mac | Linux and Windows untested |

## Competitive summary

| Project | Best at | Weak at | Adopt | Avoid |
|---------|---------|---------|-------|-------|
| Codex | OS sandboxes; app-server protocol; tools start while the model streams | one wire API only | `apply_patch` grammar, rw-lock tool gate, execpolicy | native-items-only history |
| Claude Code | hooks, subagents, deferred tools, permission callback | closed; unsandboxed when its sandbox is missing | hook schema as superset, tool search | fail-open |
| OpenCode | event-sourced sessions, cache-aware background compaction | no sandbox | compaction numbers, cache breakpoints, spill files | listener without auth (CVEs) |
| Pi | smallest clear loop; best provider-feature preservation; JSONL tree | no permissions in core | message model, per-file mutation queue, steer/follow-up | first-party client identity |
| Goose | loop as a function of persisted state; per-model cache semantics | auto-approve default | operation pipeline, tool-pair compaction | 9-field JSON summaries |
| Gemini CLI | tiered policy engine; folder trust; loop guards | one provider | policy tiers, loop detection | compaction at 50 % |
| Cline | stateless loop, stateful core, shared hub | blanket subagent approval | layering | approvals that cover children |
| Aider | repo map (tree-sitter + PageRank) | dormant, no tools | the repo map, edit fallback ladder | commits on the user's branch |
| T3 Code | event-sourced server, coalescing, wire budgets, exactly-once delivery | Electron | all four | collisions on one checkout |
| Vibe Kanban | Rust adapters and normalisation | company gone | shapes, with attribution | hard-coded CLI versions; cloud dependency |
| Zed | terminal and ACP threads in one list | GPL agent crates | the idea only | reading code from it |

Roo Code is archived and Continue unmaintained; they are references, not upstreams.

## Where Axiom can be better (and how it will be shown)
1. Memory and frame time with many agents: a published benchmark nobody else has.
2. One queue of what needs a person, across every harness.
3. Effective permission shown across a tree of agents, child never above parent.
4. Deferred tools and programmatic calling native to the engine: none of the open engines defers MCP schemas end to end.
5. Provenance on every usage number.
6. Honest lanes: which account, which harness, which policy, on screen.

`t3code-comparison.md` (2026-11-25): what T3 Code does better than what we have built, ranked, and what we keep.

## Reusable, with attribution
Codex `apply-patch` and sandbox policies (Apache-2.0); Vibe Kanban executors (Apache-2.0); T3 coalescer and delegated-mode logic (MIT); Aider's model settings and repo-map method (Apache-2.0); models.dev data (MIT); Monty (MIT).

## Biggest unknowns
Anthropic's view of an open-source orchestrator driving the user's own `claude`; Windows containment; ACP v2 (preview); real local tool-call reliability; rate-limit header names for the long tail. Each report ends with its own list.
