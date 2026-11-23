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
| `build.run` (a provider task; waits up to `timeoutSecs`, default 600, at most 3600 (additive, AUD-035); status + diagnostics; `conflict` if the task changed after approval, AUD-103), `build.diagnostics` | build | execute / read |
| `env.detect`, `env.planInstall` (without `version`: the newest the manifest lists; never runs a provider's `list_versions` program, AUD-109) | env | read |
| `env.install` (a plan id this principal got from `env.planInstall`; never a client's plan or a system plan; the card shows the plan, AUD-041) | env | install |
| `editor.open` (reveal a location: the `agent/reveal` notification) | clients | read |
| `mcp.<server>.<tool>` (user-configured MCP servers) | MCP client | execute (+ network) |
| `preview.start` (the dev server with the inspector; gated as the command it runs; returns the URL and whether a page is connected) | preview | execute |
| `ui.snapshot` (the page's tree: id, type, text, stable key, bounds, best source), `ui.inspect` (one node by stable key or id: properties, accessibility, every source candidate), `ui.selection` (the person's selection in inspect mode, with ancestors) | inspect | read |
| `ui.verify` (the checks stated before the patch, against the reloaded page; `verified` / `failed` / `inconclusive` and every check) | inspect | read |

`ui.*` default to the workspace's running preview and refuse another workspace's.
They are **assertion-first**:
- the system prompt and `ui.verify`'s description tell the model to decide, before patching, the
  checks that would show success;
- the model calls `ui.verify` after `fs.applyPatch` and reports the verdict as it is.

The person's selection reaches a chat turn as an `Attachment`: the selected node's source file
and lines (the shell's Inspector, M5-UI-001).

Later milestones add `debug.*` (M6).

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
`agent/toolCall`), `cli`. The daemon makes an external principal its connection's own:
cards and the audit log show `mcp:<client name>#c<n>` / `cli#c<n>` (AUD-009).

Processes an agent starts (`task.run`, `build.run`, `acp.terminal`) do not get the daemon's
secret-looking variables or the providers' `apiKeyEnv` variables, unless settings
`agent.passEnv` names them; the card says how many are withheld (`security.md` §Processes;
AUD-010). `SSH_AUTH_SOCK` is among them (review N7), so an agent's `git push` or `fetch` over
SSH, SSH commit signing and `git+ssh` dependencies fail unless the person adds it to
`agent.passEnv`; external ACP agents and MCP servers keep it. When it is withheld, the card
names it first and says how to pass it. The result text of `task.run`, `build.run` and
`preview.start` ends with the same line (review E5), so an auto-allowed command that shows no
card still says what it did not get. Output is kept as a bounded tail while a command
runs.

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
- "Allow for session" records exactly that call: tool, effect, paths, argv (exact, not a
  prefix) and its conditions (an ACP terminal's agent-set variables and working directory,
  `build.run`'s provider and `PATH` prepend), in that workspace, for that principal. It is not
  offered for `credentials`, `deploy`, protected paths, opaque calls (MCP tools, installs) or
  calls without targets other than reads; it ends when the session is deleted, rewound or
  restored, or the external principal's connection closes (AUD-009).
- Protected paths: a write or delete under `.axiom/`, a `.git` directory at any depth, a bare
  repository's `<name>.git/`, `.husky/`, or the workspace's configured `core.hooksPath` (read
  from its own `.git/config`) always asks, whatever the policy or a grant says; the path is
  judged through symlinks (AUD-104, review B4 and D-round).
- A call whose conditions change what an approved command does (an agent-set variable, a
  working directory other than the root) asks even where a rule allows its argv (AUD-039).
- `path` globs are matched in linear time per candidate (AUD-106).
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
- A restore writes a new file and renames it over the entry (a hard link is never written
  through), never writes through a directory that is a symlink (unless the restore itself
  removes that symlink first), and never writes into a git directory: a path inside `.git`,
  or inside a `<name>.git` that is a repository (or one of git's own entries in it), is skipped
  and left out of `restored`, and the rest is restored (review E3). `.axiom/` is restored like
  any file (a restore is the person's own undo). A restored file keeps the permission bits of the file it
  replaces, without setuid, setgid or sticky (review R5, D4).
- As built (M3-CKPT-001): the private index is `axiom-index` in the shadow dir; the agent's
  latest state is `refs/axiom/<session>/post`, taken after each writing tool; a restore with
  conflicts changes nothing (not even the undo checkpoint) unless `skipConflicts`. Nested
  repositories are recorded as git links and never restored. First checkpoint of this
  repository: 0.94 s; later ones: 21 ms.
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
- As built (M3-LOOP-001):
  - Journal records are `event` (a wire `AgentEvent`), `assistant` (the exact blocks of one
    model round, opaque ones included), `toolResults` and `attached` (attached file contents),
    each with `v: 1`. The internal records carry `atSeq`, the newest event's seq.
  - Every write is `sync_data`'d. A torn last line is dropped on read; any other bad line is an
    error.
  - **The loop:**
    - It journals a `toolCall` (with its resolved targets) before the gate, and `toolResult`
      after it.
    - The turn's checkpoint is taken in the gate's before-execute hook, so a refused call
      takes none. The post-state is recorded after each successful non-read call.
    - At most 50 tool rounds per turn.
    - A rate limit before any tool ran is retried once when the wait is ≤ 30 s.
    - An incomplete round (an error, a cancel, or a stream that ended early) keeps its text
      but drops its unconfirmed tool calls.
  - **Compaction** when the last `usage` exceeds 75 % of the window, keeping the last two
    turns.
  - **Resume:** a session loaded with an open turn gets `interrupted` for each unanswered call
    (and an error result for the model), then `turnEnd` (`error`).
  - **External calls:** `agent/toolCall` writes are checkpointed too, under
    `ext-<principal>`.

## External agents and MCP

ADR-0015 §7.
- **Server:** `axiom mcp` (also `axiomd --mcp`) speaks MCP over stdio and is a protocol client
  of a daemon (the shared one on the local socket, else one it starts). `tools/list` →
  `agent/tools`; `tools/call` → `agent/toolCall` with principal `mcp:<clientInfo.name>`. Same
  gate, same audit log; an `ask` surfaces in an attached GUI, otherwise it is denied with a hint.
- **Client:** settings `agent.mcpServers` (`[{name, argv, env}]`); servers start per session on
  first use; their tools are `mcp.<server>.<tool>` with effect `execute` (annotations never lower
  it). Workspace-configured MCP servers are not supported in M3.
- As built (M3-MCP-001):
  - **Server.** `axiom mcp [--workspace DIR]` (apps/cli) speaks MCP revisions 2025-06-18,
    2025-03-26 and 2024-11-05: initialize, ping, `tools/list` (with annotations derived from
    the effect: readOnly, destructive, openWorld), `tools/call` (`content` text +
    `structuredContent`; a gate refusal is a result with `isError`, not a protocol error).
    `axiomd --mcp` was not built: the CLI is the one front end.
  - **Client.** `agent.mcpServers` servers start on a workspace session's first turn and stop
    when the session is deleted or the daemon exits. Paged `tools/list` is followed. Server
    tool errors (`isError`) are tool errors. Conversations without a workspace get no MCP
    tools in M3 (ADR-0017 §2 said they would; recorded in the MCP-001 handoff).
  - Usage: `docs/contributing/connect-agents.md`.
- **ACP:** external agents inside Axiom (ADR-0016; as built by M3-ACP-001).
  - **Built-in providers.** `claude-code` (`claude-agent-acp`), `codex` (`codex-acp`) and
    `gemini` (`gemini --experimental-acp`) are listed by `agent/models`: available when the
    program is on `PATH` or in `<data dir>/agents/node_modules/.bin`, else with an install
    hint. A user configuration with the same id overrides one.
  - **Installing (ADR-0020 §6).** `agent/installAgent { providerId }` runs
    `npm install --prefix <data dir>/agents <package>`, bounded to 10 minutes. The packages
    are `@agentclientprotocol/claude-agent-acp`, `@agentclientprotocol/codex-acp` and
    `@google/gemini-cli`. The answer is the installed program. The person still signs in to the
    agent themselves.
  - **Without a folder (ADR-0020 §6).** A conversation without a workspace may use an external
    agent. Its first turn creates `<data dir>/scratch/<session id>` and opens it as a workspace
    inside the daemon; the agent runs there, with the gate, checkpoints and terminals as in a
    folder.
  - **Connection.** ACP protocol version 1. The client capabilities are fs read/write and
    terminal. `session/new` gets the workspace as `cwd` and Axiom's own MCP server
    (`axiom mcp`, when the CLI is installed next to the daemon). `session/load` is used when
    the agent offers it, else a notice says it starts fresh. The agent's session id is kept
    next to the journal (`<id>.acp`).
  - **Agent requests.** `fs/read_text_file`, `fs/write_text_file` and `terminal/create` are the
    hidden tools `acp.readFile`, `acp.writeFile` and `acp.terminal` (never listed or offered to
    a model), so they get the gate, the turn's checkpoint and the audit log. Paths outside the
    workspace are refused. `terminal/output|wait_for_exit|kill|release` operate on the
    executor task.
  - **Permission requests.** `session/request_permission` is decided by the gate as the policy
    tool `<provider id>.<agent tool name>`, with the effect from the ACP `kind`. An approval
    the person gave adds one-turn grants: for an edit or delete, `acp.writeFile` on the
    requested paths; for `execute`, the next terminal if it runs the command the request showed
    (`rawInput.command`). A request the policy allowed unseen (a `read`) adds none (AUD-040).
    So the person is asked once, not twice, and never for less than what runs.
  - **Terminals.** `acp.terminal` shows the variables the agent sets and its working directory
    on the card (they ask even under an argv rule); `outputByteLimit` is capped at 16 MiB;
    non-string `args` are refused. A deleted session's terminals, and a closed workspace's, are
    dropped (AUD-039, AUD-099). A terminal belongs to the session that created it (its
    principal), not to the agent process: after an idle reap and resume the agent still
    reaches its terminals; another session's agent is refused (review N5, D5). On the card, the
    variables an agent sets are one per line, a name or value with whitespace, quotes or control
    characters quoted with `\` escapes; the CLI shows each line of a card's preview after
    `  | ` (review E6).
  - **Lifetime.** An agent whose process ended is started again on the next turn; agents and
    MCP servers start without holding the daemon-wide maps (AUD-032).
  - **Updates.** Message and thought chunks become `textDelta` + `assistantText` (thoughts with
    `kind: thought`). The final `plan` becomes `assistantText` with `kind: plan`. `tool_call`
    and `tool_call_update` become `toolCall`/`toolResult` (diffs rendered as unified diffs).
    The stop reason becomes `turnEnd`. A biased select handles every update the agent sent
    before its prompt response.
  - **Cancel and edits.** Cancel sends `session/cancel`; a pending permission is answered
    `cancelled`; the agent is killed if it has not answered after 10 s. Edit/regenerate
    (`fromTurn`) is `unsupported` for external agents: they keep their own history.
  - **Limit.** An agent that changes files without the ACP fs methods (its own internal
    tools) is outside the gate. The adapters route Claude Code's and Codex's edits and shell
    through the client when it advertises the capabilities, as Axiom does.

## Client-facing protocol (protocol 0.3)

Requests: `agent/models`, `agent/configureProvider`, `agent/sessionCreate`,
`agent/sessionList`, `agent/sessionOpen`, `agent/sessionRename`, `agent/sessionDelete`, `agent/image`, `agent/send`, `agent/cancel`, `agent/approve`,
`agent/checkpoints`, `agent/restore`, `agent/tools`, `agent/toolCall`, `agent/policy`,
`agent/setPolicy`. Notifications: `agent/event`, `agent/approvalRequested`,
`agent/approvalResolved`, `agent/reveal`. Exact types: `crates/protocol/src/agent.rs`.
