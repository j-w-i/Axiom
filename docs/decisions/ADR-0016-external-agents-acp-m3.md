# ADR-0016: External agents (Claude Code, Codex, Gemini CLI) hosted in M3 through ACP

Status: **Accepted** (2026-11-21). Requested by the owner during M3 ("Make sure you support
integration with applications like Claude Code and Codex, as well as obviously direct API
connectors", 2026-11-21). Amends ADR-0007 §5 and ADR-0015 §7, which deferred hosting external
agents to M6.

## Context
ADR-0015 gives Axiom its own loop over direct model APIs (Anthropic, OpenAI-compatible) and an
MCP server, so external agents can drive Axiom's tools from their own terminals. The owner also
wants the agents people already use, Claude Code and Codex, to work **inside** Axiom: chosen in
the AI dock like a model, with their file edits, commands and permission prompts going through
Axiom's gate and checkpoints, and using their own sign-in (subscription or key).

The Agent Client Protocol (ACP, agentclientprotocol.com; JSON-RPC 2.0 over the agent's stdio) is
the standard for this. Gemini CLI speaks it natively (`gemini --experimental-acp`). Claude Code
and Codex speak it through official ACP-project adapters: `@agentclientprotocol/claude-agent-acp`
(command `claude-agent-acp`) and `@agentclientprotocol/codex-acp` (command `codex-acp`). Zed
hosts them the same way.

## Decision
1. **ACP agents are a provider kind.** `ProviderKind` gains `acp`. `ProviderConfig` gains an
   optional `command` (argv) and `env` (both additive, protocol 0.3). An ACP provider has one
   model, `default`. A session created with an ACP provider is run by an ACP bridge instead of
   Axiom's loop. The client sees the same `agent/*` methods and `agent/event` stream, so the UI
   does not branch on the kind.
2. **Built-in entries.**
   - `claude-code`: `claude-agent-acp`.
   - `codex`: `codex-acp`.
   - `gemini`: `gemini --experimental-acp`.
   An entry is listed by `agent/models` when its program is on `PATH`; otherwise it is listed
   with an error naming what to install (`npm install -g @agentclientprotocol/claude-agent-acp`,
   and the same for Codex). Axiom does not install or download agents itself in M3. The user
   can override an entry or add one in `agent.providers`.
3. **Axiom is the ACP client:**
   - **Advertised capabilities:** `fs.readTextFile`, `fs.writeTextFile`, `terminal`. File reads
     and writes go through `Workspace::resolve` and the workspace service (paths outside the
     workspace are refused). Writes are gated as `writeWorkspace` and checkpointed like
     `fs.applyPatch`. Terminals are gated as `execute` and run through `exec`.
   - **`session/request_permission`** goes through the same `PermissionGate`. The effect comes
     from the tool call's `kind` (read/search → read; edit/move → writeWorkspace;
     delete → delete; execute → execute; fetch → network; other → execute). `allow` selects the
     agent's allow-once option, `deny` its reject-once option, and `ask` becomes the usual
     approval card. "Allow for session" maps to allow-always only for that session's rule.
   - **`session/new`** passes Axiom's MCP server (`axiom mcp`, ADR-0015 §7) in `mcpServers`, so
     the agent also gets `workspace.search`, `build.run`, `env.*` and later `ui.*` (M5).
   - **Events:**
     - `agent_message_chunk` → `textDelta`, with `assistantText` at the turn's end;
     - `tool_call` / `tool_call_update` → `toolCall` / `toolResult`;
     - `plan` and `agent_thought_chunk` → `assistantText` blocks marked as a plan or thought;
     - the stop reason → `turnEnd`.
     They are journalled like any session.
   - **Lifecycle:** `agent/cancel` → `session/cancel` (pending permissions answered
     `cancelled`). Resume uses `session/load` when the agent advertises `loadSession`; else the
     transcript replays for the UI and a new agent session starts, and the UI says so.
4. **Authentication stays the agent's.** Axiom passes no keys to ACP agents; Claude Code and
   Codex use their own sign-in. An agent's `authenticate` methods are listed and the user is told
   to sign in with the agent's own CLI. Axiom never sees the credential.
5. **Implementation:** in-house over the NDJSON codec (as MCP), in `crates/agent` (`acp`
   module) with the daemon providing fs/terminal/gate. The `agent-client-protocol` Rust crate
   was considered and not adopted, for the same reason as `rmcp` (ADR-0015 §3): one JSON-RPC
   implementation.
6. **Tests:** an in-repo fake ACP agent (a test binary speaking the spec) drives a session
   through fs reads, a gated write with a checkpoint, a terminal, a permission request and
   cancel. Real Claude Code / Codex runs are manual: they need the owner's sign-in.

## Consequences
- The UI's model picker offers Claude Code / Codex / Gemini next to API models.
- An external agent's own internal tools that do not go through ACP (for example an agent
  writing files directly instead of through `fs/write_text_file`) are outside Axiom's gate. The
  adapters route Claude Code's and Codex's edits and shell commands through the client when the
  client advertises the capabilities, but this is the adapter's choice. The permission card
  states that the agent is external, and `security.md` records the limit.
- Protocol 0.3 gains two optional `ProviderConfig` fields and one enum value: additive.
