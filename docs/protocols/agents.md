# Agent runtime, tools and permissions

**State: frozen for M3** (M3-ARCH-001, 2026-11-21). Decisions: ADR-0007, ADR-0015. Wire types:
`crates/protocol/src/agent.rs` (protocol 0.3, `ipc.md` §Protocol 0.3). Research:
`docs/research/agents.md`. Sections marked *as built* are filled in by the task named.

## Components

All in `crates/agent` (`axiom-agent`), hosted by `axiomd`. `agent` depends only on `protocol`
and `store`; `axiomd` registers the tools as thin adapters over the services (ADR-0015 §2).

```
ModelProvider (trait)  ── adapters: anthropic, openai_chat (Ollama, llama.cpp, LM Studio, vLLM
                          through capability profiles), scripted (tests only)
Session                ── JSONL journal, context assembly, compaction
AgentLoop              ── request → stream → tool calls → gate → checkpoint → execute → results → repeat
ToolRegistry           ── typed tools; implementations registered by axiomd
PermissionGate         ── policy evaluation, approval requests, audit log
Checkpoints            ── shadow git repository through the `git` CLI
mcp                    ── MCP codec: server half (used by `axiom mcp`) and client half
```

## Model provider interface

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError>;
    /// Streams one assistant turn. Dropping the stream or cancelling the token cancels the request.
    async fn stream(&self, req: TurnRequest, cancel: CancellationToken)
        -> Result<BoxStream<'static, Result<TurnEvent, ProviderError>>, ProviderError>;
}

pub struct TurnRequest { model, system: String, messages: Vec<Message>, tools: Vec<ToolSpec>,
                         max_output_tokens: Option<u32>, temperature: Option<f32> }
pub struct Message { role: Role, blocks: Vec<Block> }          // Role: User | Assistant
pub enum Block {
    Text(String),
    ToolCall { id, name, input: serde_json::Value },
    ToolResult { call_id, content: String, is_error: bool },
    /// Provider-native content sent back verbatim to the same provider kind and dropped for any
    /// other (Anthropic thinking / redacted_thinking).
    Opaque { provider: String, raw: serde_json::Value },
    /// A user image, base64 (ADR-0017): Anthropic `image` block, OpenAI `image_url` data URL.
    Image { media_type: String, data: String },
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

Rules:
- Adapters never retry; the loop decides (ADR-0015 §9 and M3-LOOP-001).
- `ModelInfo` and `Capabilities` are the wire types (`agent.rs`). Capability profiles come from
  the provider kind and server (detected by base URL), overridable per provider configuration.
  A model with `tools: no` converses without tools.
- Built-in profiles (M3-MODEL-002, `crates/agent/src/model/profiles.rs`):

  | Kind / server | tools | streamed args | parallel | context |
  |---------------|-------|---------------|----------|---------|
  | anthropic | yes | yes | yes | 200 000 |
  | openaiChat, api.openai.com | yes | yes | yes | 128 000 |
  | openaiChat, port 11434 (Ollama) | unknown | no (whole calls) | unknown | unknown |
  | openaiChat, port 8080 (llama.cpp), 1234 (LM Studio), other | unknown | unknown | unknown | unknown |

  Only a definite `tools: no` stops the loop offering tools.
- Provider configurations live in settings `agent.providers` (a JSON array of
  `ProviderConfig`); API keys live only in the OS credential store under `axiom/<provider id>`,
  or are read from the environment variable named by `apiKeyEnv`. A key never reaches settings,
  SQLite, the journal, the audit log, logs or test fixtures.
- Base URLs are HTTPS, or HTTP to a loopback host (local servers).
- Tests never use the network: `ScriptedProvider` replays turns; adapters are tested against a
  loopback fixture server with recorded SSE streams. The daemon offers a `scripted` provider
  only when `AXIOM_TEST_SCRIPTED_MODEL=<script file>` is set.

## Tools

A tool is a typed operation with a declared effect (`Effect` in `agent.rs`):
`read`, `writeWorkspace`, `execute`, `install`, `network`, `writeOutside`, `delete`,
`credentials`, `deploy`.

| Tool | Backed by | Effect |
|------|-----------|--------|
| `workspace.info` | workspace | read |
| `workspace.search` (literal or regex, glob filter, paged) | workspace search | read |
| `fs.list`, `fs.read` (line range, paged, numbered lines) | workspace | read |
| `fs.applyPatch` (search/replace blocks against the current version; create; delete) | workspace | writeWorkspace |
| `task.run` (argv only; waits up to a timeout; exit status, output tail, diagnostics) | exec, build | execute |
| `build.run` (a provider task; waits; status + diagnostics), `build.diagnostics` | build | execute / read |
| `env.detect`, `env.planInstall` | env | read |
| `env.install` (an issued plan id only) | env | install |
| `editor.open` (reveal a location: the `agent/reveal` notification) | clients | read |
| `mcp.<server>.<tool>` (user-configured MCP servers) | MCP client | execute (+ network) |

Later milestones add `preview.*`, `ui.*` (M5), `lsp.*` (M4), `debug.*` (M6).

Tool result rules: results are bounded (`max_result_bytes`, default 32 KiB) with a
`truncated` flag and a continuation cursor where the tool pages; errors are structured
(`kind`, a one-sentence `message`, a `hint` telling the model what to do differently);
`fs.applyPatch` fails when a search block does not match exactly once, and its hint shows the
closest region. Every path goes through `Workspace::resolve`.

## Execution pipeline (every tool call, every principal)

```
validate input against the tool's schema                     → invalid_params + hint
→ resolve targets (workspace paths, argv)
→ PermissionGate.check(principal, tool, effect, targets)    → Allow | Deny | Ask
   Ask → agent/approvalRequested to approving connections → first agent/approve wins
         no approving connection → approval_required (does not run)
→ if effect ≠ read and this turn has no checkpoint yet: create one (git missing → prerequisite_missing)
→ journal: tool intent
→ execute through the service
→ if effect ≠ read: post-state snapshot (what the agent left)
→ journal: tool result; audit log: principal, tool, targets, decision, rule, outcome, duration
→ bound and return the result
```

Principals: `session:<id>` (the loop), `mcp:<client name>` (the MCP front end through
`agent/toolCall`), `cli`.

## Permission policy

User policy: settings `agent.policy` (the `Policy` wire type). Workspace rules:
`.axiom/project.toml`, which may only make the user policy **stricter**:

```toml
[agent.policy]
default = "ask"
[[agent.policy.rule]]
effect = "execute"
argvPrefix = ["cargo", "publish"]
decision = "deny"
```

- A rule matches when every field it sets matches: `effect`; `tool` (exact, or a prefix ending
  in `.*`); `path` (workspace-relative glob, all targets must match); `argvPrefix` (for
  `execute`).
- Among matching rules the most specific wins (more of `tool`, `path`, `argvPrefix` set; then
  the longer `argvPrefix`); among equals, deny > ask > allow. No match → `default`.
- The built-in user policy: `default = "ask"`, `read → allow`.
- Workspace rules are merged so they can only turn allow into ask/deny or ask into deny.
- "Allow for session" adds an in-memory rule for that tool and target set; never for
  `credentials` or `deploy`.
- Content a model reads (files, tool results, web pages) cannot change policy.
- Audit log: `<data dir>/audit/<workspace hash>.jsonl`, append-only.

## Checkpoints

ADR-0015 §5. A **shadow git repository** at `<data dir>/checkpoints/<workspace hash>/`, driven
through the `git` program with an isolated environment (its own `GIT_DIR` and index,
`GIT_WORK_TREE` = workspace root, no system/global config, hooks disabled). It never reads or
writes the user's `.git`, index, refs or stash, and works for workspaces that are not git
repositories. The workspace's `.gitignore` files apply; files over 16 MiB are skipped and listed.

- A checkpoint (`refs/axiom/<session>/<n>`) is taken before the first tool with an effect other
  than `read` in each turn.
- Restore to checkpoint C: every file that differs between C and the workspace now is restored
  if its content equals what the agent last left; otherwise it is a conflict
  (`modifiedSinceAgent`, `deletedSinceAgent`, `createdSinceAgent`). With conflicts nothing is
  restored unless `skipConflicts`. Restore takes a checkpoint first.
- Concurrent write-capable agents on one workspace are not offered in M3 (worktrees: M6).

## Sessions

Workspace-free conversations (ADR-0017) live in `<data dir>/sessions/_chat/`; images are
content-addressed blobs in `<session dir>/blobs/<sha256>`. A `rewind` event (edit/regenerate)
drops turns from `fromTurn` on from the context; the journal keeps them.


`<data dir>/sessions/<workspace hash>/<session id>.jsonl`, append-only, one versioned record
per line (`v`, `seq`, `ts`, `turn`, the event). The journal stores everything needed to resume:
the wire `AgentEvent`s plus internal records (provider-native opaque blocks, the exact
messages sent). Store migration 4 keeps an index (id, workspace root, title, provider, model,
created, updated).

- Tool intent is written before execution and the result after; a resumed session whose last
  tool call has no result gets an `interrupted` event and the call is never re-run.
- Compaction: when a turn's input tokens exceed 75 % of the context window (32 768 when
  unknown), the loop asks the model for a summary of everything but the last two exchanges and
  appends a `compaction` event; the journal keeps the original events.
- Context: system prompt (tool rules, workspace name and root-relative layout), the workspace's
  `AGENTS.md` if present (capped at 32 KiB), the latest compaction summary, then the messages.

## External agents and MCP

ADR-0015 §7.
- **Server:** `axiom mcp` (also `axiomd --mcp`) speaks MCP over stdio and is a protocol client
  of a daemon (the shared one on the local socket, else one it starts). `tools/list` →
  `agent/tools`; `tools/call` → `agent/toolCall` with principal `mcp:<clientInfo.name>`. Same
  gate, same audit log; an `ask` surfaces in an attached GUI, otherwise it is denied with a hint.
- **Client:** settings `agent.mcpServers` (`[{name, argv, env}]`); servers start per session on
  first use; their tools are `mcp.<server>.<tool>` with effect `execute` (annotations never lower
  it). Workspace-configured MCP servers are not supported in M3.
- **ACP** (M6+): deferred; the UI consumes the generic `agent/event` stream, not loop internals.

## Client-facing protocol (protocol 0.3)

Requests: `agent/models`, `agent/configureProvider`, `agent/sessionCreate`,
`agent/sessionList`, `agent/sessionOpen`, `agent/sessionRename`, `agent/sessionDelete`, `agent/image`, `agent/send`, `agent/cancel`, `agent/approve`,
`agent/checkpoints`, `agent/restore`, `agent/tools`, `agent/toolCall`, `agent/policy`,
`agent/setPolicy`. Notifications: `agent/event`, `agent/approvalRequested`,
`agent/approvalResolved`, `agent/reveal`. Exact types: `crates/protocol/src/agent.rs`.
