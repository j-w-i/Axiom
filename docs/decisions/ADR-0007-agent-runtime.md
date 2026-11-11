# ADR-0007: In-house agent loop over a shared, gated tool registry

Status: Accepted (2026-11-11); interfaces are draft until M3

## Context
Research: `docs/research/agents.md`. Existing harnesses (Codex CLI, Goose,
Gemini CLI, OpenCode, Aider…) are applications, not libraries with stable
embedding APIs; their loops are small compared with their tool, policy and
persistence layers — which are exactly the parts Axiom must own because they
bind to its services, its permission model and its visual-inspection data.

## Decision
1. **Build the agent loop in Rust inside the daemon** (`crates/agent`).
2. **Model access behind a `ModelProvider` trait** with a normalised
   block-based message model that can round-trip provider-native opaque
   content. First adapters: Anthropic Messages, OpenAI Chat Completions
   (which also reaches Ollama, llama.cpp, LM Studio, vLLM, MLX-LM, subject to
   per-server capability profiles). OpenAI Responses and Gemini follow.
   Adapters are written directly on `reqwest` + SSE parsing; no third-party
   multi-provider SDK crate in the core path.
3. **Tools are service operations.** One `ToolRegistry`; each tool declares
   input/output schemas and an effect class. The GUI's own actions and the
   agent's tools call the same service functions.
4. **One permission gate**, in the daemon, in front of every tool execution,
   regardless of which client or agent asked.
5. **Expose the registry over MCP** (`axiomd --mcp`) so external agents can
   use Axiom; consume MCP servers as additional tools. Host external agents
   in the UI through ACP later; do not design against it.
6. **Checkpoints** use a shadow git repository; **parallel agents** use git
   worktrees. Sessions are append-only JSONL.

Contract: `docs/protocols/agents.md`.

## Alternatives considered
- *Embed an existing harness as the runtime.* Faster to a demo; but the loop
  would live outside our permission gate and tool model, and we would
  inherit its release cadence and assumptions.
- *Be only an MCP server and let external agents do everything.* Attractive
  and partially adopted (point 5), but the product requires an integrated,
  model-independent experience including local models, approvals in the UI
  and the visual closed loop.
- *A multi-provider SDK crate.* Lowest-common-denominator message models
  lose provider-specific blocks that must be replayed; churn risk.

## Consequences
- More code to own (two adapters, loop, compaction). Bounded and well
  understood; tested with a scripted provider, no network in CI.
- Because tools equal services, every new platform capability becomes
  agent-accessible by registering one spec, and is testable without a model.
