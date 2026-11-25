# Harness integration: structured interfaces to existing agents

Research date 2026-11-24. Companion: `subscription-auth.md` (who may use which plan, and how).
Nothing here was exercised against a live model. Evidence tags used throughout:

- **[S]** read in source/schema at the commit named in §9 · **[H]** `--help`/`--version` of a local binary
- **[D]** vendor documentation page fetched 2026-11-24 · **[R]** ACP registry manifest only · **[C]** community/forum

Pages were fetched through a tool that summarises; text in "quotes" is short and as extracted. Re-read the live
page before relying on wording. The source clones were read, never built or run.

## 1. Conclusions for the architecture

1. **Three native adapters plus one generic ACP adapter cover the market.** Native: Claude Code (stream-json),
   Codex (app-server), Copilot (Rust SDK). Generic: ACP v1 **and** v2, negotiated per connection. The ACP adapter
   reaches Antigravity, Cursor, Grok Build, OpenCode, Kimi Code, Qwen Code, Goose, Droid, Amp, Kiro, Junie, Vibe…
2. **ACP is now good enough to be the first integration for everything**, including Claude and Codex (official
   adapters co-authored by Anthropic/OpenAI/Zed/JetBrains). Go native only where ACP loses something we need:
   Codex rate limits, schema'd approvals, `expectedTurnId` steering; Claude priorities, hooks, rewind, rate-limit events.
3. **Plan-quota reporting is the biggest ACP gap**: no rate-limit/quota update exists in v1 or v2 stable; token usage
   per turn is still a Draft RFD. `usage_update` only carries context used/size and cumulative cost.
4. **Model Axiom's internal event schema on ACP v2 semantics** (id-addressed upserts, `state_update`, background list,
   child sessions, `requires_action`), not v1 ("prompt response ends the turn"). v1, Claude and Codex all map up to it.
5. **Never trust the harness to sandbox or gate.** Cursor SDK auto-approves headless tool calls; ACP permission requests
   are agent-initiated and optional; `claude -p` skips workspace trust and runs a repo's hooks and `.mcp.json`. Axiom
   must gate trust before spawn and own the OS sandbox.
6. **Identify honestly.** Set `clientInfo`/`info`/`CLAUDE_AGENT_SDK_CLIENT_APP` to `axiom/<ver>`; never set
   `CLAUDE_CODE_ENTRYPOINT` (undocumented; see subscription-auth.md §2) and never spoof User-Agent.
7. **Pin and probe.** Interfaces move weekly (Claude Agent SDK 0.3.330, Codex 110 client methods, ACP schema 1.35.0 all
   within 48 h of this note). Feature-detect via capabilities arrays, generate schema from the *installed* Codex binary,
   store raw frames, tolerate unknown variants.

## 2. Harness capability matrix

Split in two for width; every harness has all columns. "n/d" = not documented on the pages read.

### 2a. Control plane

| Harness (evidence) | Best structured interface | Streaming granularity | Reasoning stream | Tool events | Permission interception | Steering mid-turn | Interrupt | Resume | Fork |
|---|---|---|---|---|---|---|---|---|---|
| **Claude Code** 2.1.325 [H,D,S] | Own CLI, `-p` bidirectional stream-json + control protocol; Agent SDK (TS/Py); ACP adapter | Raw Anthropic SSE deltas (`stream_event`) with `--include-partial-messages` | Yes: thinking deltas/blocks; subagent text via `--forward-subagent-text` | `tool_use`/`tool_result` blocks, `tool_progress`, hook events, task_* | **Best in class**: `can_use_tool` control request (allow/deny/`updatedInput`/`updatedPermissions`), plus PreToolUse hooks callback; never times out | Yes: user msg `priority` `now`/`next`/`later` | `interrupt` control request, receipt lists still-queued | `--resume <id\|path.jsonl>`, `--continue`, `--session-id` | `--fork-session`; also SDK `resume_session_at` |
| **Codex** main a0d5b0a [S,D] | `codex app-server` (JSONL JSON-RPC-like, stdio) | Token deltas per item (`item/agentMessage/delta`) | `item/reasoning/summaryTextDelta`, `/textDelta` | Typed items (19 kinds): commandExecution, fileChange, mcpToolCall, webSearch… + output deltas, `turn/diff/updated`, `turn/plan/updated` | Server→client requests: command, file change, permissions, user input, MCP elicitation | **Explicit** `turn/steer` with `expectedTurnId`; `activeTurnNotSteerable` error | `turn/interrupt` | `thread/resume` | `thread/fork` (`lastTurnId`), `thread/revert` |
| **Codex exec** [S] | `codex exec --json` | Item level, no token deltas | Item level | item.started/updated/completed | No (policy flags) | No | Kill | `exec resume` n/d here | `codex fork` |
| **Copilot SDK** v2 / protocol 4 [S,D] | Official **Rust** crate `copilot-sdk` (MIT); in-proc C-ABI, spawned stdio or TCP | `assistant.message_delta` | `assistant.reasoning_delta` | `tool.execution_*`, `tool.shell_output` (sequenced) | Permission handler + pre/post tool hooks | `mode:"immediate"` vs `"enqueue"` | abort n/d by us | session persistence/resume | `sessions.fork`, `session.history.rewind` |
| **Copilot CLI ACP** [D] | `copilot --acp [--stdio\|--port]`, public preview | ACP chunks | ACP | ACP | ACP | n/d | `session/cancel` | ACP caps | n/d |
| **Antigravity** (`agy`, registry 1.5.2) [D] | `agy acp` (ACP v1) | ACP chunks | ACP | ACP, diffs | ACP; existing rules still apply | **Yes**: prompting during a turn steers it | `session/cancel` | load, resume, list, close | n/d |
| **Gemini CLI** 0.73 nightly [S] | `gemini --acp`; `-o stream-json` | ACP chunks | ACP | ACP | ACP | n/d | ACP | ACP resume | n/d |
| **OpenCode** 2.4.1 [S,D]; local is 1.18.31 [H] | HTTP server + SSE (`/api/event`), OpenAPI 3.1, 178 ops; `opencode acp` | Part deltas over SSE | Yes | Yes | `GET/POST …/permission`, `…/{id}/reply` | Inbox delivery modes: steer vs queue | `POST …/interrupt?resume=` | server-side sessions | `POST …/fork` (`before`) |
| **Cursor** [D] | `@cursor/sdk` (TS/Py, public beta) or `agent acp` | SDK `stream()` + `onDelta`; ACP chunks | `thinking` events | `tool_call` (payload "not stable") | **SDK: none, headless auto-approves** (hooks files/sandbox only); ACP: yes | SDK `run.steer()` (local) | `run.cancel()` / ACP cancel | `Agent.resume`; ACP `session/load` | n/d |
| **Grok Build** 1.0.93 [D,R] | `grok agent stdio` (ACP); `grok -p --output-format streaming-json` | ACP chunks | n/d | ACP | n/d in docs | n/d | ACP | `-r`, `-s`, `-c`; ACP load n/d | n/d |
| **Amp** [D,R] | `@ampcode/sdk` (TS/Py) stream-json; ACP only via community `amp-acp` | Message level (`system/assistant/result`) | n/d | In assistant messages | Plugin based; no SDK callback documented | n/d | n/d | `continue: 'T-…'` | n/d |
| **Goose** 1.60.2 [S] | `goose acp` (stdio), ACP over HTTP | ACP chunks | ACP | ACP | ACP | `_goose/unstable/session/steer` | ACP | ACP | n/d; as ACP *client* fork/resume unsupported |
| **Qwen Code** 0.28.x [S] | `qwen --acp`; `qwen serve` HTTP/SSE daemon (v1, OpenAPI); stream-json ("input under construction") | Deltas | Yes | Yes | ACP / daemon | daemon n/d | Yes | `--continue/--resume` | n/d |
| **Kimi Code** 2.7.3 [D,R] (Python `kimi-cli` 1.52 archived) | `kimi acp` | ACP chunks | ACP | ACP | ACP | n/d | ACP | load/resume/list | **`session/fork`** |
| **Droid** 1.19 [D,R] | `droid exec -o stream-jsonrpc`; `-o acp-daemon`; TS/Py SDK | Notifications | n/d | Yes | SDK permissions; exec stops above `--auto` level | n/d | "interrupt work" over stream-jsonrpc | `-s <id>` | `--fork <id>` |
| **Kiro, Junie, Vibe, MiniMax Code, Auggie, Cline, Kilo, Devin, Qoder…** [R] | ACP | ACP | ACP | ACP | ACP | per agent | ACP | caps | caps |

### 2b. Configuration, reporting, commercial

| Harness | Model / effort selection | MCP injection | Usage reporting | Subscription auth | Licence / terms | Stability notes |
|---|---|---|---|---|---|---|
| Claude Code | `--model`, `--effort low..max`, `set_model` control | `--mcp-config` JSON + `--strict-mcp-config`; in-process `sdk` servers over `mcp_message` | `result.usage/modelUsage/total_cost_usd`; `rate_limit_event`; `get_context_usage`; `/usage` → `usage_report` (experimental) | Claude plan login inside unmodified CLI only | CLI proprietary; Py SDK code MIT, use under Commercial Terms; don't brand as "Claude Code" | Weekly. `--bare` (no OAuth) "will become the default for `-p`" |
| Codex | `model`, `effort` per `turn/start`; `model/list` | `thread/start.config` overrides (`mcp_servers`), config RPCs; dynamic tools experimental | `account/rateLimits/read`, `account/rateLimits/updated`, `account/usage/read`, `thread/tokenUsage/updated` | ChatGPT via SIWC token (preferred) or app-server login | Apache-2.0 | Schema generated per version; experimental gated by `experimentalApi`; WS unsupported |
| Copilot SDK | per session; `models.list` incl. price | stdio + HTTP/SSE | `assistant.usage`, `session.usage.getMetrics`, `account.getQuota`, `session.quota.get` (exp.) | GitHub sign-in or own OAuth/GitHub App | MIT SDK; runtime terms separate | Renaming to Microsoft Copilot SDK; v2 breaking changes |
| Copilot CLI ACP | launch flags `--effort`, not per session | ACP | none standard | GitHub login or BYOK | proprietary | "public preview and subject to change" |
| Antigravity | Model + thinking pickers (config options); Gemini only outside listed IDEs | ACP (`--process-per-session` for HTTP) | n/d | Google OAuth inside `agy` | proprietary; §6 third-party clause | New (mid-2026) |
| Gemini CLI | flags/ACP | ACP, settings | stream-json stats | **Not** consumer plans since 2026-06-18; Code Assist Std/Ent | Apache-2.0 | Enterprise/API-key only |
| OpenCode | session model/variant | config | session cost/tokens | its own providers | MIT | v1→v2 API rewrite; many routes `experimental` |
| Cursor | SDK/ACP | inline (SDK); `.cursor/mcp.json` (ACP); team MCP unsupported in ACP | `usage` event, `agent.getUsage()` | Cursor login / user API key billed to plan | proprietary | SDK public beta |
| Grok Build | `-m` | n/d | `/usage` in session | SuperGrok / X Premium+ login or `XAI_API_KEY` | proprietary | Young |
| Amp | mode | `options.mcpConfig` | n/d | Amp credits; ChatGPT plan for OpenAI models | proprietary | — |
| Goose | provider | extensions→MCP | yes | delegates to claude-acp/codex-acp/… | Apache-2.0 | — |
| Qwen Code | `/model` | ACP/settings | result stats | API-key plans; OAuth free tier ended 2026-04-15 | Apache-2.0 | — |
| Kimi Code | `session/set_model` ext. | http/sse/stdio; drops `acp` type | n/d | Kimi membership login/API key | MIT | — |
| Droid | flags | yes | `usage` in JSON | `FACTORY_API_KEY`/login | proprietary | — |

## 3. Claude Code: stream-json + control protocol

Launch (flags marked † are used by the SDK but hidden from `--help`; [S] `subprocess_cli.py`, [H] rest):

```
claude -p --input-format stream-json --output-format stream-json --verbose \
  --include-partial-messages --forward-subagent-text --replay-user-messages \
  --permission-prompt-tool† stdio --permission-mode manual \
  --model opus --effort high --session-id <uuid> \
  --mcp-config '{"mcpServers":{…}}' --strict-mcp-config --setting-sources user,project
env: CLAUDE_AGENT_SDK_CLIENT_APP=axiom/0.1.0   CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1
```

Host → CLI (one JSON per line on stdin):

```json
{"type":"control_request","request_id":"r1","request":{"subtype":"initialize","hooks":null,"forwardSubagentText":true}}
{"type":"user","uuid":"<uuid>","session_id":"","parent_tool_use_id":null,
 "message":{"role":"user","content":"Fix the failing test"},"origin":{"kind":"human"}}
{"type":"user","uuid":"<uuid2>","parent_tool_use_id":null,"priority":"next",
 "message":{"role":"user","content":"Skip integration tests"},"origin":{"kind":"human"}}
{"type":"control_request","request_id":"r2","request":{"subtype":"interrupt"}}
{"type":"control_request","request_id":"r3","request":{"subtype":"set_model","model":"sonnet"}}
```

CLI → host:

```json
{"type":"control_response","response":{"subtype":"success","request_id":"r1","response":{}}}
{"type":"system","subtype":"init","session_id":"…","model":"…","tools":[],"mcp_servers":[],"capabilities":["interrupt_receipt_v1"]}
{"type":"stream_event","uuid":"…","session_id":"…","parent_tool_use_id":null,
 "event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Look"}}}
{"type":"control_request","request_id":"c7","request":{"subtype":"can_use_tool","tool_name":"Bash",
 "input":{"command":"cargo test"},"tool_use_id":"toolu_1","permission_suggestions":[],"blocked_path":null}}
{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1795600000,"utilization":0.83},"uuid":"…","session_id":"…"}
{"type":"result","subtype":"success","is_error":false,"num_turns":3,"total_cost_usd":0.12,"usage":{},"modelUsage":{},
 "user_message_uuids":["<uuid>"],"terminal_reason":"completed","session_id":"…"}
```

Permission answers:

```json
{"type":"control_response","response":{"subtype":"success","request_id":"c7","response":{"behavior":"allow","updatedInput":{"command":"cargo test"}}}}
{"type":"control_response","response":{"subtype":"success","request_id":"c7","response":{"behavior":"deny","message":"User declined","interrupt":false}}}
```

Adapter rules:
- Control subtypes host→CLI seen in the SDK: `initialize, interrupt, set_permission_mode, set_model, rewind_files,
  mcp_status, mcp_reconnect, mcp_toggle, stop_task, get_context_usage`; CLI→host: `can_use_tool, hook_callback,
  mcp_message`; plus `control_cancel_request` (drop the pending request, send nothing). [S]
- **Permission prompts never time out** — persist pending ones; on stdin EOF Claude Code cancels them. [D]
- `updatedInput` must echo the input when unchanged (the SDK always sends it). [S]
- `interrupt` before SIGTERM; SIGTERM exits 143 and leaves the turn without a result. [D]
- Match results to prompts by `user_message_uuids`, not by order; `idle` and `result` may arrive in either order. [D]
- Steering: default/`next` is absorbed after running tool calls; `later` is a new turn; `now` interrupts (or backgrounds
  work if `origin.kind=human`, ≥2.1.286). [D]
- `permission_denied` system messages and `result.permission_denials` list non-prompted denials. [D]
- Hooks: register callbacks in `initialize.hooks`; `on_failure:"block"` makes a guard fail closed on new CLIs. [S]
- **Trust**: `-p` shows no trust dialog and runs project hooks/`.mcp.json` "even in a folder you've never trusted". Use
  Axiom's own trust gate, then `--setting-sources`, `--strict-mcp-config`, or `--restricted`/`--safe-mode`. [D,H]
- **`--bare`** never reads OAuth/keychain. When it becomes the `-p` default, subscription delegation breaks unless an
  opt-out exists; add a startup check that `claude auth status` method is what the user chose. [D]
- MCP secrets on the command line are visible to other local users; prefer a 0600 `--mcp-config` file. [S: claude-acp README]
- The control wire format is **not** a published spec; docs steer non-Python/TS hosts to `-p` + JSON. Reference
  implementations: `claude-agent-sdk-python` (MIT), Goose `claude_code.rs` (Apache-2.0, Rust, same technique). Fallback:
  `claude-agent-acp` v0.110.1 (Apache-2.0 adapter code; bundles CLI; ACP v1+v2; standalone binaries).

Permission mode vocabulary on 2.1.325: `acceptEdits, auto, bypassPermissions, manual, dontAsk, plan` [H].
Other useful flags [H]: `--json-schema`, `--max-budget-usd`, `--fallback-model`, `--add-dir`, `--agents`, `--worktree`,
`--bg` + `attach/logs/stop/rm` (managed background sessions), `--include-hook-events`, `--no-session-persistence`.

## 4. Codex app-server

- Transport: stdio JSONL (default); `unix://`; `ws://` "experimental and unsupported". Messages omit `"jsonrpc"` —
  "we do not do true JSON-RPC 2.0" [S rpc.rs]. WS overload error `-32001`. Shutdown `-32600 data.reason=serverShuttingDown`.
- Schema: `codex app-server generate-json-schema --out D` / `generate-ts --out D`, "specific to the Codex version you ran".
  Vendored copy: `codex-rs/app-server-protocol/schema/json`. At a0d5b0a (stable export): 110 client requests, 10 server
  requests, 87 notifications. Generate Rust types from JSON Schema at dev time; validate leniently at run time.
- Handshake: `initialize` → response → `initialized`. `clientInfo.name` identifies us (compliance logs, originator, UA):
  keep it constant (`axiom`) and equal to the SIWC `agent_name_hint`.

```json
{"id":1,"method":"initialize","params":{"clientInfo":{"name":"axiom","title":"Axiom","version":"0.1.0"},
  "capabilities":{"experimentalApi":false,"optOutNotificationMethods":[]}}}
{"method":"initialized"}
{"id":2,"method":"account/read","params":{"refreshToken":false}}
{"id":3,"method":"account/rateLimits/read"}
{"id":4,"method":"model/list","params":{}}
{"id":5,"method":"thread/start","params":{"cwd":"/repo","model":"gpt-…","sandbox":"workspace-write",
  "approvalPolicy":"on-request","config":{"mcp_servers":{"axiom":{"command":"/path/axiom-mcp"}}}}}
{"id":6,"method":"turn/start","params":{"threadId":"thr_1","effort":"high","input":[{"type":"text","text":"Fix it","text_elements":[]}]}}
{"method":"item/started","params":{"threadId":"thr_1","turnId":"t1","startedAtMs":1795500000000,"item":{"type":"commandExecution","id":"i1"}}}
{"method":"item/agentMessage/delta","params":{"threadId":"thr_1","turnId":"t1","itemId":"i2","delta":"Look"}}
{"id":90,"method":"item/commandExecution/requestApproval","params":{"threadId":"thr_1","turnId":"t1","itemId":"i1",
  "startedAtMs":1795500000000,"command":"cargo test","cwd":"/repo","reason":"needs network","proposedExecpolicyAmendment":["cargo","test"]}}
{"id":90,"result":{"decision":"accept"}}
{"method":"serverRequest/resolved","params":{"threadId":"thr_1","requestId":90}}
{"id":7,"method":"turn/steer","params":{"threadId":"thr_1","expectedTurnId":"t1","input":[{"type":"text","text":"Skip e2e","text_elements":[]}]}}
{"id":8,"method":"turn/interrupt","params":{"threadId":"thr_1","turnId":"t1"}}
{"method":"thread/tokenUsage/updated","params":{"threadId":"thr_1","turnId":"t1","tokenUsage":{"last":{"inputTokens":1},"total":{"totalTokens":9},"modelContextWindow":400000}}}
{"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":25,"windowDurationMins":300,"resetsAt":1795510000}}}}
{"method":"turn/completed","params":{"threadId":"thr_1","turn":{"id":"t1","status":"completed","items":[]}}}
```

- Command decisions: `accept | acceptForSession | {acceptWithExecpolicyAmendment} | {applyNetworkPolicyAmendment} | decline | cancel`;
  file change: `accept | acceptForSession | decline | cancel`. `turn.status`: only `completed` is success. [S]
- `kind:"humanCommand"` checkpoints require `experimentalApi`+`requiredHumanApproval`; only accept/decline/cancel valid. [S]
- `rateLimits/updated` is "sparse rolling": merge into the last `read`; null does not clear. Prefer `rateLimitsByLimitId`. [S]
- Account errors arrive as `turn.error.codexErrorInfo`: `usageLimitExceeded, rateLimitExceeded, unauthorized, serverOverloaded, …`. [S]
- Login types: `apiKey`, `chatgpt` (returns `authUrl`,`loginId`), `chatgptDeviceCode` (`verificationUrl`,`userCode`).
  `chatgptAuthTokens` is marked "[UNSTABLE] FOR OPENAI INTERNAL USE ONLY - DO NOT USE" — do not use it. [S]
- Dynamic (client-executed) tools: `thread/start.dynamicTools` + `item/tool/call`, experimental. MCP is the stable path.
- `codex exec --json` events: `thread.started, turn.started, turn.completed{usage}, turn.failed, item.started|updated|completed, error`. Good for CI only.
- `codex mcp-server`: **no such crate or subcommand found at a0d5b0a**; treat MCP-server mode as gone. SDKs: TS, Python (`openai-codex`).

## 5. Agent Client Protocol (ACP)

**Versions (2026-11-24)** [S]: protocol **v1 stable** (schema crate 1.35.0, 2026-11-23); **v2 Preview** since 2026-11-20
(schema 2.0.0-beta.4): "ready to build on, and it is not frozen". Version negotiated by integer in `initialize`.
Governance: own GitHub org, lead maintainers from Zed and JetBrains. Apache-2.0.

**SDKs**: Rust `agent-client-protocol` **5.5.1** (2026-11-21, schema 1.21; v2 still behind `unstable_protocol_v2`,
`unstable_mcp_over_acp`) [D docs.rs]; TypeScript `@agentclientprotocol/sdk` 2.9.1; also Python, Kotlin, Java. Rust had
five majors in 2026 (1.0 on 06-25 → 5.x): **wrap it**, or generate types from `schema/v1|v2/schema.json`. **Dart: community
`acp_dart` 0.5.0, MIT, v1 only, unverified uploader** — unnecessary if ACP lives in the Rust daemon and Dart sees only our protocol.

**v1 surface** [S meta.json]. Agent: `initialize, authenticate, logout, session/new, load, resume, list, delete, close,
prompt, cancel, set_mode, set_config_option`. Client: `session/update, session/request_permission, fs/read_text_file,
fs/write_text_file, terminal/{create,output,release,wait_for_exit,kill}, elicitation/{create,complete}`. Both: `$/cancel_request`.
Unstable in both versions: `session/fork, session/options, session/status, session/archive, set_title, auth/status,
providers/{list,set,disable}, mcp/message` (MCP-over-ACP). Fork is still an RFD, not stable.

**Update kinds (v1)**: `user_message_chunk, agent_message_chunk, agent_thought_chunk, tool_call, tool_call_update, plan,
plan_update†, plan_removed†, available_commands_update, current_mode_update, config_option_update, session_info_update,
usage_update, notice†, compaction_update†, compaction_summary_chunk†, subagent_update†, session_message(_chunk)†`
(† only if the client opted in). Tool kinds: `read edit delete move search execute think fetch switch_mode other`.
Config categories: `mode, model, model_config, thought_level, other` (`speed` in Preview) — this is model/effort selection.

```json
{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":1,
 "clientInfo":{"name":"axiom","title":"Axiom","version":"0.1.0"},
 "clientCapabilities":{"fs":{"readTextFile":true,"writeTextFile":true},"terminal":true,"auth":{"terminal":true},
  "session":{"notices":{},"compaction":{},"configOptions":{"boolean":{}}},"subagents":{},"plan":{},"elicitation":{"form":{},"url":{}}}}}
{"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"mcpCapabilities":{"http":true},
  "sessionCapabilities":{"resume":{},"list":{},"close":{},"systemPrompt":{}}},"authMethods":[{"id":"agent-login","name":"Agent login"}]}}
{"jsonrpc":"2.0","id":1,"method":"session/new","params":{"cwd":"/repo","mcpServers":[{"name":"axiom","command":"/path/axiom-mcp","args":[],"env":[]}]}}
{"jsonrpc":"2.0","id":2,"method":"session/prompt","params":{"sessionId":"s1","prompt":[{"type":"text","text":"Fix it"}]}}
{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_thought_chunk","messageId":"m1","content":{"type":"text","text":"…"}}}}
{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"tool_call","toolCallId":"c1","title":"Run tests","kind":"execute","status":"pending"}}}
{"jsonrpc":"2.0","id":5,"method":"session/request_permission","params":{"sessionId":"s1","toolCall":{"toolCallId":"c1"},
 "options":[{"optionId":"allow-once","name":"Allow once","kind":"allow_once"},{"optionId":"no","name":"Reject","kind":"reject_once"}]}}
{"jsonrpc":"2.0","id":5,"result":{"outcome":{"outcome":"selected","optionId":"allow-once"}}}
{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"usage_update","used":53000,"size":200000,"cost":{"amount":0.045,"currency":"USD"}}}}
{"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"s1"}}
{"jsonrpc":"2.0","id":2,"result":{"stopReason":"cancelled"}}
```

Capability markers are objects (`{}`), not booleans, except legacy `fs`/`terminal`/`auth.terminal`; checked against schema 1.35.0.

**v2 changes that matter** [S migration.mdx]: `session/prompt` returns on *insertion*; progress/stop reason via
`state_update` (`running|idle|requires_action|unknown` + `background[]`); **prompts allowed in any state** ("steering … or
starting new work, is up to the Agent" — no capability yet says which); `session/load` → `session/resume{replayFrom:{type:"start"}}`
+ `replay_complete`; `tool_call` folded into upsert `tool_call_update`; `tool_call_content_chunk`, `terminal_update`,
`terminal_output_chunk`; `tool_call/cancel`, `terminal/cancel`; `idempotencyKey`; modes → config options; subagents need no
opt-in; unknown enum values must be preserved. **Client fs/* and terminal/* are removed** → if Axiom wants to mediate
file/terminal access it must do so through injected MCP servers or OS sandboxing, not ACP callbacks. Design for that now.

**Permission semantics**: agent decides when to ask; agent owns `allow_always` scope; on cancel the client MUST answer
pending requests with `cancelled`. Build approval UI from `kind`, `locations`, `rawInput`; treat `title` as agent-authored.

**MCP passthrough**: v1 stdio (+http/sse by capability). MCP-over-ACP (`type:"acp"`) is draft, targets MCP 2026-07-28 only;
Claude/Codex adapters already take it, Kimi drops it. **Auth**: `agent` type (agent opens browser) via `authenticate`;
`terminal` type (stable 2026-08-20) = client runs agent's own binary interactively in a PTY, then reconnects. Axiom's
terminal can host this: no secret ever crosses the protocol.

**Transport**: stdio stable; Streamable HTTP/WebSocket is an RFD (Goose ships it; Rust `agent-client-protocol-http`).

**Registry** [S]: `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`; 46 manifests, 4 quarantined;
distribution `npx|uvx|binary(sha256)`; updated hourly and "preview distributions are not verified". First-party entries:
antigravity-acp (Google), cursor, devin, factory-droid, gemini, github-copilot(-cli), goose, grok-build (xAI), junie, kiro,
kimi-code, minimax-code, mistral-vibe, muse-code (Meta), opencode, poolside, qwen-code, kilo, cline, auggie, bob (IBM).
Adapters: claude-acp 0.110.0, codex-acp 2.11.0 (drives `codex app-server`; exposes steering, subagents, usage), amp-acp
(community), pi-acp (community), glm-acp-agent (community). Two are copyleft: corust-agent GPL-3.0+, minion-code AGPL-3.0
(subprocess only, never link). Installing from it = running third-party code: show publisher, version, hash; pin; no auto-update.

**ACP gaps for an orchestrator**: no quota/rate-limit event; per-turn token usage Draft; no stable fork; no standard
steer/queue declaration; no hook/pre-tool veto beyond agent-initiated permission; no standard rewind/checkpoint; no
cross-agent sandbox policy; effort/model only as agent-chosen config options; session IDs are agent-private.

## 6. Copilot SDK, OpenCode, others

- **Copilot SDK** [S 2dfe56f]: six languages incl. **Rust (`cargo add copilot-sdk`)**. Protocol-4 JSON-RPC; default host is
  the native runtime loaded *in-process* through a C ABI ("neither crash isolation nor a sandbox") → in Axiom choose the
  **spawned stdio** connection. GA 2026-06-02; repo is being renamed "Microsoft Copilot SDK" (v2 package names changed).
  `client_info` (applicationName/Version) attributes traffic honestly. Send modes: `immediate` (steer) / `enqueue`.
- **OpenCode** [S 678621a, D]: v2 HTTP API at `/api/*`, SSE `/api/event`, session `prompt` returns once queued,
  `interrupt?resume=`, `fork`, `permission/{id}/reply`, inbox steering, pairing codes + token auth, OpenAPI 3.1 in
  `packages/protocol/openapi.json`. Installed local binary is **1.18.31** (`serve`, `acp`, `run --format json`, basic auth
  via `OPENCODE_SERVER_PASSWORD`): v1 API ≠ v2 API. Use ACP for OpenCode; avoid binding to either HTTP API.
- **Cursor ACP extensions** [D]: blocking `cursor/ask_question`, `cursor/create_plan`; notifications `cursor/update_todos`,
  `cursor/task`, `cursor/generate_image`. Unanswered blocking calls stall the agent → implement or return an error.
- **Antigravity**: "`agy -p`" headless must not be wrapped (policy); `agy acp` only. See subscription-auth.md §4.
- **Goose**: as a harness (`goose acp`), and as precedent: it deprecated its CLI wrappers in favour of ACP providers.
- **Pi** [S 0ab23c3]: first-party `pi --mode rpc` (JSONL commands/events; docs recommend it for other languages) and TS SDK; ACP
  only via community `pi-acp`. Pi's Anthropic OAuth path is non-compliant (subscription-auth.md §6): do not enable it through Axiom.

## 7. A2A (brief)

Linux Foundation; v1.0 released March 2026, first stable spec; 150+ organisations (LF, 2026-04-09). HTTP agent-card
discovery, tasks, streaming, push. Aimed at remote opaque agents between organisations; none of the coding harnesses above
documents A2A as its local control interface. No permission interception or workspace semantics. **Defer**; if Axiom ever
exposes itself as a remote agent, A2A is a facade over our own task model. Rust SDK status unverified.

## 8. Suggested adapter plan

| Order | Adapter | Why | Size |
|---|---|---|---|
| 1 | ACP client (v1 then v2) in Rust daemon | One adapter, most agents, vendor-blessed auth | M–L |
| 2 | Codex app-server | Typed approvals, steer, rate limits; pairs with SIWC token | M |
| 3 | Claude stream-json (feature-gated, pinned min version) | Permissions, priorities, rate-limit events | M |
| 4 | Copilot Rust SDK (spawned) | Official Rust, quota API | S–M |
| — | Cursor SDK, Amp SDK, OpenCode HTTP, Droid JSON-RPC | Only if ACP proves insufficient | defer |

Adapter contract must include: `probe()` (binary, version, auth state *as reported by the harness*), capability set
(per column above, so the UI can hide steer/fork), raw-frame journal, unknown-event passthrough, `quota()` optional,
explicit `auth.login()` that only launches the vendor flow, and a kill path that interrupts first, then terminates the group.

## 9. Sources

Clones (shallow, 2026-11-24): openai/codex a0d5b0a · agentclientprotocol/agent-client-protocol d3ac0e6 · …/registry 8f398f4 ·
…/claude-agent-acp 37241a9 (0.110.1) · anthropics/claude-agent-sdk-python 6fc539e (0.2.178) · github/copilot-sdk 2dfe56f ·
QwenLM/qwen-code c56424fc · MoonshotAI/kimi-cli 9ab1286 (archived) · sst/opencode 678621a · block/goose 03a343f ·
google-gemini/gemini-cli 03862c4 · pingdotgg/t3code f692811b. Local binaries: Claude Code 2.1.325, OpenCode 1.18.31 (help only).
Docs (fetched 2026-11-24): code.claude.com/docs/en/{headless,agent-sdk/overview,agent-sdk/typescript,authentication} ·
learn.chatgpt.com/docs/app-server · developers.openai.com/siwc/token-sharing-open-source/codex-app-server ·
docs.github.com/en/copilot/reference/copilot-cli-reference/acp-server · antigravity.google/docs/{cli/acp,ide/extensions/acp} ·
cursor.com/docs/{cli/acp,sdk/typescript} · docs.x.ai/build/cli/headless-scripting · ampcode.com/docs/sdk ·
docs.factory.com/cli/droid-exec/overview · opencode.ai/docs/server · moonshotai.github.io/kimi-code/en/reference/kimi-acp.html ·
goose-docs.ai/docs/guides/acp-providers · docs.rs/crate/agent-client-protocol/latest/source/CHANGELOG.md · pub.dev/packages/acp_dart ·
github.com/agentclientprotocol/codex-acp · linuxfoundation.org A2A release (2026-04-09).

## 10. Unverified

- No harness was run; every sample is assembled from schemas/SDK source and may miss required fields for a given version.
- Claude: exact `--permission-prompt-tool stdio` behaviour on 2.1.325 (hidden flag, seen in Goose and the Python SDK);
  how to opt out once `--bare` becomes the `-p` default; whether `/usage` works over stream-json for every plan.
- Codex: `thread/start.config.mcp_servers` shape is inferred from config.toml keys + the free-form `config` field; not tested.
- Copilot SDK abort method, permission-handler wire names; licence terms of the bundled native runtime (SDK itself is MIT).
- ACP: which registry agents actually answer v2 today (Claude and Codex adapters do per RFD update; others unknown).
- Grok Build ACP permission/resume; Amp ACP parity; Kiro/Junie/Vibe/MiniMax/Muse ACP depth; Qwen daemon steering.
- Antigravity FAQ vs ACP docs tension (subscription-auth.md §4). TypeScript Agent SDK page was truncated; AccountInfo not read.
- Pi RPC command coverage, Warp, Devin CLI, Windsurf/other closed harnesses were not examined beyond registry manifests.
