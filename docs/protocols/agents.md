# Agent runtime, tools and permissions

**State: draft.** Frozen by the first task of M3. Research:
`docs/research/agents.md`. Decision: ADR-0007.

## Components (all in `crates/agent`, hosted by `axiomd`)

```
ModelProvider (trait)  ── adapters: anthropic, openai-responses, openai-chat (covers Ollama,
                          llama.cpp, LM Studio, vLLM, MLX-LM through per-server profiles)
Session                ── transcript (JSONL), context assembly, compaction
AgentLoop              ── request → stream → tool calls → gate → execute → results → repeat
ToolRegistry           ── typed tools backed by daemon services
PermissionGate         ── policy evaluation, approval requests to the client
Checkpoints            ── recoverable workspace snapshots
```

## Model provider interface

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError>;
    /// Streams one assistant turn. Dropping the stream cancels the request.
    async fn stream(&self, req: TurnRequest, cancel: CancellationToken)
        -> Result<BoxStream<'static, Result<TurnEvent, ProviderError>>, ProviderError>;
}

pub struct ModelInfo { id, display_name, context_window: Option<u32>, max_output: Option<u32>,
                       tools: Support, images: Support, streaming_tool_args: Support }
pub enum Support { Yes, No, Unknown }

pub struct TurnRequest { model, system: String, messages: Vec<Message>, tools: Vec<ToolSpec>,
                         max_output_tokens: Option<u32>, temperature: Option<f32> }
pub struct Message { role: Role, blocks: Vec<Block> }
pub enum Block {
    Text(String),
    Image { media_type, data_ref },
    ToolCall { id, name, input: serde_json::Value },
    ToolResult { call_id, content: Vec<Block>, is_error: bool },
    /// Provider-native content that must be sent back verbatim to the same
    /// provider (e.g. signed reasoning blocks) and dropped for any other.
    Opaque { provider: String, raw: serde_json::Value },
}
pub enum TurnEvent {
    TextDelta(String),
    ToolCallStart { id, name }, ToolCallArgsDelta { id, json_fragment }, ToolCallEnd { id },
    OpaqueBlock(Block),
    Usage { input_tokens, output_tokens, cached_input_tokens: Option<u32> },
    Finished(StopReason),   // EndTurn | ToolUse | MaxTokens | Refusal | Other(String)
}
pub enum ProviderError { Auth, RateLimited { retry_after }, ContextTooLong, Unavailable,
                         InvalidRequest(String), Transport(String) }
```

Rules: adapters never retry tool-executing turns themselves; the loop decides.
`Opaque` blocks are persisted in the transcript so a session can resume.
Capabilities are probed per server/model and may be overridden in settings,
because "OpenAI-compatible" servers differ in tool-call streaming and
parallel calls (research §1).

First adapters: Anthropic Messages and OpenAI Chat Completions (the latter
gives Ollama/llama.cpp/LM Studio immediately). Tests use a deterministic
`ScriptedProvider` that replays recorded `TurnEvent`s; no network in CI.

## Tools

A tool is a typed operation with a declared effect:

```rust
pub struct ToolSpec { name: &'static str,          // "fs.read", "build.run"
                      description: &'static str,
                      input_schema: schemars::Schema, output_schema: schemars::Schema,
                      effect: Effect, max_result_bytes: usize }
pub enum Effect { Read, WriteWorkspace, Execute, Install, Network,
                  WriteOutside, Delete, Credentials, Deploy }
```

Tool names use `.`; they map 1:1 onto service operations and reuse the
protocol's types, so a tool result is the same structure the GUI gets.

| Tool | Backed by | Effect | Milestone |
|------|-----------|--------|-----------|
| `workspace.info` | workspace | Read | M3 |
| `workspace.search` (text/regex, glob filter, paged) | workspace (`grep-searcher`) | Read | M3 |
| `fs.list`, `fs.read` (line range, paged) | workspace | Read | M3 |
| `fs.applyPatch` (search/replace blocks against `expectedVersion`) | workspace | WriteWorkspace | M3 |
| `task.run` (argv only; waits; returns exit status + truncated tail + diagnostics) | exec, build | Execute | M3 |
| `build.run`, `build.diagnostics` | build | Execute / Read | M3 |
| `env.detect`, `env.planInstall` | env | Read | M3 |
| `env.install` | env | Install | M3 |
| `editor.open` (reveal a location in the user's editor) | client notification | Read | M3 |
| `preview.launch`, `preview.capture` | inspect | Execute / Read | M5 |
| `ui.snapshot`, `ui.hitTest`, `ui.nodeDetails`, `ui.resolveSource` | inspect | Read | M5 |
| `ui.verify` (evaluate `Check[]`) | inspect | Read | M5 |
| `lsp.definition`, `lsp.references`, `lsp.diagnostics` | lsp | Read | M4+ |
| `debug.*` | DAP | Execute | M6 |

Tool result rules (from research §5): results are bounded
(`max_result_bytes`, default 32 KiB) with an explicit `truncated` flag and a
continuation cursor; errors are structured (`kind`, one-sentence message, and
a `hint` field telling the model what to do differently); file edits fail
loudly when the search block does not match exactly once.

## Execution pipeline (every tool call, every client)

```
validate input against schema
→ resolve targets (WorkspacePath → host path via Workspace::resolve)
→ PermissionGate.check(effect, tool, resolved targets)  → Allow | Deny | Ask
   Ask → notification `agent/approvalRequested` to the client → user decides
→ if effect ≥ WriteWorkspace and no checkpoint exists for this turn: create one
→ execute through the service
→ append to audit log (who, tool, targets, decision, outcome)
→ bound and return the result
```

## Permission policy

Stored in settings (`agent.policy`), overridable per workspace in
`.axiom/project.toml` only towards *stricter*.

```toml
[agent.policy]
default = "ask"
[[agent.policy.rule]]
effect = "read";            decision = "allow"
[[agent.policy.rule]]
effect = "write_workspace"; decision = "ask"      # "allow" in auto-edit mode
[[agent.policy.rule]]
effect = "execute"; argv_prefix = ["cargo", "build"]; decision = "allow"
[[agent.policy.rule]]
effect = "credentials";     decision = "ask"; remember = false
```

First matching rule wins; rules with patterns are evaluated before rules
without. "Allow for this session" adds an in-memory rule. `credentials` and
`deploy` can never be remembered. Content read from files, tool results or
web pages cannot change policy.

## Checkpoints

Mechanism: a **shadow git repository** at
`<data dir>/checkpoints/<workspace hash>/` using the workspace as its work
tree (`GIT_DIR` + `GIT_WORK_TREE`), honouring the workspace's ignore rules.
It never touches the user's `.git`, index, branches or stash, and works for
workspaces that are not git repositories. A checkpoint is a commit in the
shadow repo; restore checks out that tree for the files the agent changed
since, after confirming none of them were modified by someone else in the
meantime (hash comparison) — otherwise it reports a conflict per file instead
of overwriting.

Multiple concurrent agents (M6+) each work in their own `git worktree` of the
user's repository when the workspace is a git repo; merging back is an
explicit, reviewed step. Without git, concurrent write-capable agents on one
workspace are not offered.

## Sessions

`<data dir>/sessions/<workspace hash>/<session id>.jsonl`: one event per line
(`user`, `assistant_block`, `tool_call`, `tool_result`, `approval`,
`checkpoint`, `compaction`, `usage`). Append-only; a session resumes by
replaying the file. An index row in SQLite (id, workspace, title, updated)
supports listing. Compaction replaces a prefix with a summary event and keeps
the original lines in the file.

Project memory for agents is the repository's own `AGENTS.md` (read at
session start) — the same convention this repository uses.

## External agents and MCP

- **MCP server** (M3): `axiomd --mcp` (stdio) exposes the ToolRegistry so
  Claude Code, Codex CLI, etc. can drive Axiom. Same gate, same audit log;
  approvals surface in the Axiom GUI if it is attached, else follow policy
  with `ask` treated as `deny`.
- **MCP client** (M3+): user-configured MCP servers appear as additional
  tools with effect `Execute` + `Network` unless annotated.
- **ACP** (M6+): hosting external agents inside the Axiom UI. Deferred;
  nothing in M3 may preclude it (the UI consumes a generic event stream, not
  loop internals).

## Client-facing protocol additions (M3)

Requests: `agent/sessionCreate`, `agent/sessionList`, `agent/send`,
`agent/cancel`, `agent/approve`, `agent/checkpointList`,
`agent/checkpointRestore`, `agent/providersList`, `agent/providerConfigure`.
Notifications: `agent/event` (streamed transcript events),
`agent/approvalRequested`. Exact types are written into `crates/protocol`
by M3-AGENT-001.
