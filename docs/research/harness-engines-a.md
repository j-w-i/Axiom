# Harness engines study A: Codex CLI, OpenCode, Pi, Goose

Date: 2026-11-24. Method: shallow clones (`--depth 1 --filter=blob:limit=1m`), read-only (cat/rg), nothing built or run.
Paths are repo-relative. "(inferred)" marks anything not read directly in code. Numbers are quoted from code constants unless stated.

| Project | Home now | Commit | Licence | Language / shape |
|---|---|---|---|---|
| Codex CLI | github.com/openai/codex (`codex-rs/`) | a0d5b0a661db3b34cd87bdf1c93736b90fd3694b (2026-11-24) | Apache-2.0 | Rust, ~150 crates, Responses API only |
| OpenCode | github.com/anomalyco/opencode (`sst/opencode` 301-redirects) | 678621a5dfb7a967e37a088f4a3b5708370f72f3 (2026-11-24) | MIT | TypeScript (Effect), rewritten into `packages/{core,ai,server,codemode,plugin,...}` |
| Pi | github.com/badlogic/pi-mono (npm scope now `@earendil-works/*`) | 0ab23c37b074252d04e31f5cfb99491f0613fb80 (2026-11-24) | MIT | TypeScript monorepo: `ai`, `agent`, `coding-agent`, `durable`, `codemode`, `mcp`, `tui` |
| Goose | github.com/aaif-goose/goose (`block/goose` 301-redirects) | 03a343f9fe309a2c2cfe63a5561cbfe9ab38762e (2026-11-24) | Apache-2.0 | Rust; loop extracted into `crates/oi-kit-*` (state-machine of Operations) |

Big picture: all four have moved well past "while(tool_calls)". Codex = production Responses-API client with remote compaction, guardian approval model and OS sandboxes. OpenCode = event-sourced SQLite sessions, background pre-computed compaction, cache-policy compiler. Pi = minimal core with tree-session JSONL, extensions, cache-warming economics. Goose = loop as ordered, re-entrant list of Operations over persisted conversation.

---

## 1. Codex CLI (`codex-rs`)

### Agent loop
- Entry `core/src/session/turn.rs::run_turn` (4052 lines). Outer `loop` at ~L553: each iteration drains `pending_input` (steers) from `sess.input_queue`, builds a `StepContext`, calls `run_sampling_request` -> `try_run_sampling_request`. Model needs follow-up if it emitted tool calls OR user input is pending (`needs_follow_up = model_needs_follow_up || has_pending_input`, L737).
- Steering: `core/src/session/input_queue.rs` (`TurnInput::{UserInput, FunctionCallOutput, ResponseItem, InterAgentCommunication}`). A steer arriving mid-turn is recorded into the same turn and re-opens sampling (comment L534: "A steer can reopen sampling within the same turn"). Mailbox for inter-agent messages has delivery phases (`MailboxDeliveryPhase`).
- Interruption: `CancellationToken` per turn; aborted tool calls produce `AbortedToolOutput`; next turn gets a `<turn_aborted>` fragment: "The user interrupted the previous turn on purpose. Any running unified exec processes may still be running in the background. If any tools/commands were aborted, they may have partially executed." (`core/src/context/turn_aborted.rs`).
- Parallel tools: tool calls are spawned while the model still streams (`FuturesOrdered in_flight`, `ResponseEvent::OutputItemDone` L3415 -> `in_flight.push_back(tool)` L3548). `core/src/tools/parallel.rs`: a `RwLock<()>`; tools flagged `supports_parallel` take the read lock, others take the write lock (exclusive). Outputs drained in order (`drain_in_flight`).
- Stop hooks can re-open a turn (`stop_hook_active`); pre-turn / mid-turn / post-turn compaction phases (`CompactionPhase::{PreTurn,MidTurn,PostTurn,StandaloneTurn}`).
- Retry: `core/src/responses_retry.rs` connection retry delay 5 s initial, 60 s max (`INITIAL_CONNECTION_RETRY_DELAY`, `MAX_CONNECTION_RETRY_DELAY`). Websocket transport reuses `previous_response_id` incremental requests only when the new request is a strict extension of the last (`client.rs` ~L382-560, reasons enumerated e.g. `prompt_cache_key_changed`).

### Provider abstraction
- `model-provider-info/src/lib.rs`: `enum WireApi { Responses }` is the ONLY variant (Chat Completions removed). Custom providers must be Responses-compatible; capabilities overrides (`ModelProviderCapabilities`: `remote_compaction`, `external_web_access`, ...). Auth/gateway crates: `model-provider/` (amazon_bedrock, gcp, gateway_auth, local_auth).
- Provider-specific features are preserved by construction: history is stored as native `ResponseItem` envelopes (reasoning items with encrypted content, `compaction` items) and replayed verbatim; `prompt_cache_key`, `previous_response_id`, `store` are first-class request fields (`client.rs`). No lowest-common-denominator layer.
- Implication: portability to Anthropic/Gemini is not a goal of this design.

### Tools
- Built in (names from `core/src/tools/handlers/*_spec.rs`, `spec_plan.rs`): `exec_command`, `write_stdin` (unified exec PTY sessions), legacy `shell`, `apply_patch` (freeform/Lark custom tool), `update_plan`, `request_user_input`, `request_permissions`, `view_image`, hosted `web_search`, `tool_search`, `new_context` (token-budget reset), `get_context_remaining`, `sleep`, MCP resource tools, multi-agent tools (`spawn_agent`, `send_input`, `send_message`, `followup_task`, `wait_agent`, `resume_agent`, `close_agent`, `interrupt_agent`, `list_agents`), code-mode `exec` + `wait`.
- Edit/patch format: `core/assets/tools/apply_patch.lark`. Envelope `*** Begin Patch` / `*** End Patch`; hunks `*** Add File: p` (all `+` lines), `*** Delete File: p`, `*** Update File: p` + optional `*** Move to: p`, then `@@ [context]` and ` `/`-`/`+` lines, `*** End of File` marker. Registered as a grammar-constrained freeform tool (`ToolSpec::Freeform`, `syntax: "lark"`); description: "This is a FREEFORM tool, so do not wrap the patch in JSON." Applier: `apply-patch/src/{parser,seek_sequence,streaming_parser}.rs` (fuzzy line seek; streaming parser so diffs render while the model types).
- Result truncation: `utils/output-truncation/src/lib.rs`; model default `truncation_policy = Bytes(10_000)` (`models-manager/src/model_info.rs:126`), approx 4 bytes/token (`approx_tokens_from_byte_count`), middle-truncation, header: `Warning: truncated output (original token count: N)\nTotal output lines: M\n\n`. Serialisation allowance x1.2. Unified exec: `DEFAULT_MAX_OUTPUT_TOKENS=10_000`, buffer cap 1 MiB, yield time 250 ms min / 30 s max, empty `write_stdin` poll min 5 s, background terminal timeout 300 s, max 64 processes (`core/src/unified_exec/mod.rs:68-76`). Error-detail budget 256 B each / 512 B total per fragment.
- `tool_search` default limit 8 (`tools/src/tool_discovery.rs`).

### Context management
- Token counting: provider-reported total usage (`sess.get_total_token_usage()`), byte/4 estimate for unsent items.
- Trigger (`core/src/session/context_window.rs`): `auto_compact_token_limit = min(configured, 0.9 x context_window)` (`protocol/src/openai_models.rs:574`); `effective_context_window_percent` default 95 (L406). Compact when scope tokens >= limit (+ optional fallback buffer) OR active tokens >= 95% window. Separate `model_post_turn_compact_threshold_percent` for post-turn compaction. Scope may be `Total` or `BodyAfterPrefix`.
- Also compacts when switching to a smaller-window model or when the `comp_hash` (compaction-compat hash) changes (`turn.rs:1791-1892`).
- Two compaction paths: (a) local: prompt `prompts/templates/compact/prompt.md` ("You are performing a CONTEXT CHECKPOINT COMPACTION. Create a handoff summary for another LLM that will resume the task." with 4 bullets), summary prefixed by `summary_prefix.md`; user messages retained up to `COMPACT_USER_MESSAGE_MAX_TOKENS = 20_000` (`compact.rs:69`) plus the summary; (b) remote v2 (`RemoteCompactionSupport::V2`): `compaction_trigger` item sent over the Responses endpoint, server returns opaque replacement history (encrypted; inferred from "remote" naming + `replacement_history`). Background/async compaction candidates are precomputed and swapped in (`session/async_compaction.rs`, `take_ready`).
- Pruning: no separate tool-result pruning pass found; relies on truncation at ingest + compaction. Items can be flagged `retain_across_windows`.

### Session persistence
- JSONL "rollouts": `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`, `archived_sessions/` (`rollout/src/list.rs:420`). Line = `RolloutLine{timestamp, RolloutItem}`; items: `SessionMeta`, `ResponseItem`, `Compacted{message, replacement_history, window ids, token usage snapshot}`, `TurnContext`, `TokenUsageRecord`, `InterAgentCommunication`, `DynamicToolsUpdated` (`history/src/lib.rs:424`). SQLite state DB (`state_db.rs`) indexes them; reverse JSONL scanner for fast tail reads; optional compression; writer lock file.
- Resume = replay rollout (`session/rollout_reconstruction.rs`, 730 lines + 3000 lines of tests: compaction replay is the hard part). Fork = copy selected history to new rollout (`thread_rollout_truncation.rs`).

### Subagents
- `core/src/agent/`, `tools/handlers/multi_agents_v2`. Defaults: `DEFAULT_AGENT_MAX_THREADS = 6`, `DEFAULT_AGENT_MAX_DEPTH = 1` (`core/src/config/mod.rs:281,291`). Agent roles (`agent/role.rs`, `agent-roles` crate). Child = separate thread/rollout; messages via mailbox (`send_message`, `followup_task`, `wait_agent`). Also "message board" crates (agent-message-board-*) for shared coordination.

### Sandbox and approvals
- `AskForApproval`: `UnlessTrusted`("untrusted"), `OnRequest`(default), `Granular(..)`, `Never`. `SandboxPolicy`: `DangerFullAccess`, `ReadOnly{network_access}`, `ExternalSandbox`, `WorkspaceWrite{writable_roots,...}` (`protocol/src/protocol.rs:1206,1293`).
- Enforcement: macOS Seatbelt (`sandboxing/src/seatbelt*.rs`, `.sbpl` base + network policies), Linux bubblewrap + landlock/seccomp (`linux-sandbox/`, `bwrap`, managed TCP proxy `managed_tcp*.rs`, `network-proxy`), Windows (`windows-sandbox-rs`, service). `execpolicy` crate = Starlark-like rules for command prefix allow/forbid/prompt. "Guardian" (`core/src/guardian/`, `ext/guardian-*`): an isolated LLM reviewer session that auto-reviews approval requests with its own input budget and cancellation.
- Network policy decisions, approval prefix saving ("approved_command_prefix_saved" context fragments).

### MCP / hooks / skills / instructions
- MCP: `codex-mcp`, `rmcp-client` crates; can also run as an MCP server. MCP tool specs capped (`MAX_AGENT_PLUGIN_MCP_SPEC_BYTES 8_000`, total 64_000; code-mode `DEFAULT_INPUT_SCHEMA_MAX_BYTES 16_000`). Deferred MCP tools via `tool_search` (BM25) when many.
- Hooks (`hooks/src`): events `PreToolUse, PermissionRequest, PostToolUse, PreCompact, PostCompact, SessionStart, SessionEnd, UserPromptSubmit, SubagentStart, Stop...`; command and MCP executors; oversized hook output spills (`output_spill.rs`).
- Instructions: `AGENTS.md` (+ `project_doc_fallback_filenames`), `project_doc_max_bytes` default 32 KiB (`config/src/config_toml.rs:76`). Skills crate + `ext/skills`. Memories (`memories/` write pipeline). Plugins (`plugin*`, `core-plugins`).

### Deferred loading / code-mode
- Code mode: public tool `exec` runs model-written JavaScript that calls nested tools (`tools.xxx`), plus `wait` to resume a yielded cell; output via `text()`/`image()`; deferred nested tools discoverable through `await tools.tool_search({query, limit:8})` (BM25) (`code-mode-protocol/src/description.rs`). Runtime is out-of-process (gRPC session host, `code-mode-host`, `code-mode-runtime`; V8 per `v8-poc`; inferred).

### Performance techniques
Streaming tool execution during sampling; websocket incremental requests with `previous_response_id`; connection prewarm (`prewarmed_client_session`, `startup_prewarm.rs`); async compaction; shared MCP definition cache (`SharedMcpDefinitions`, Weak refs); reverse-JSONL scan; direct tool timing instrumentation (`direct-tool-timing`).

### Strengths / Weaknesses
- + Most complete safety stack (3 OS sandboxes, exec policy, guardian reviewer); freeform grammar-constrained patch; robust compaction with replay tests; strict cache discipline.
- - Responses-API only; enormous surface (core/src is 80k lines + 13.5k-line session tests), tightly tied to OpenAI server features (remote compaction, encrypted reasoning); hard to lift pieces.

### Adopt / Avoid / Reusable
- Adopt: unified-exec semantics (yield window, 1 MiB ring buffer, process cap 64); `<turn_aborted>` note after interrupt; RwLock parallel gate (read = parallel-safe, write = exclusive); apply_patch grammar + seek_sequence fuzzy matching; compaction trigger = min(configured, 90%) and hard 95% cap; truncation header with original token count; async/background compaction candidate; execpolicy prefix rules.
- Avoid: Responses-only abstraction; 100+ crate fragmentation; features gated on server-held state.
- Reusable (Apache-2.0, can legally reuse with NOTICE/attribution; patent grant): `apply-patch` crate (standalone, small, worth it), `execpolicy`, `linux-sandbox`/`sandboxing` seatbelt policies (worth it, but need Rust engine; if engine is not Rust only the .sbpl policies and algorithms transfer), `utils/output-truncation`. Everything else is coupled to `core`.

---

## 2. OpenCode (`packages/core`, `packages/ai`)

### Agent loop
- Two layers. `packages/ai/src/loop.ts` (214 lines): pure step functions (`pendingCalls`, `stepMessages`, `nextRequest`); a step with `finishReason=length` runs no tools (truncated args); invalid JSON tool input is returned to the model as `Invalid tool arguments: the input is not valid JSON. Received: ...`; an "answer tool" ends a step even alongside other calls; forced `toolChoice` applies to first step only. Session runner `core/src/session/runner/{index,step,llm,retry,max-steps}.ts`.
- Steering/queue: `packages/schema/src/session-inbox.ts`: `Delivery = "steer" | "queue"`; inbox items persisted in SQLite (`session/inbox.ts`), kinds user / synthetic / media / compaction / move. Interrupt via `session-interrupt` tool and run coordinator (`session/run-coordinator.ts`).
- Max steps: when reached, tools disabled and `MAX_STEPS_PROMPT` injected ("CRITICAL - MAXIMUM STEPS REACHED ... Respond with text only", `runner/max-steps.ts`).
- Retry (`core/src/model-retry.ts`): Effect schedule; separate budget for "unreachable" network (window 1 h, gap max 30 s), `MAX_TIMEOUT_RETRIES=3`, one-shot calls 3 retries only when provider clearly did not run (429, 503/529, ECONNREFUSED/ENOTFOUND/EAI_AGAIN), honours `x-should-retry` header.

### Provider abstraction
- Own LLM layer `packages/ai` (not Vercel AI SDK anymore; `core/src/aisdk*.ts` are adapters): schema'd `LLMRequest/Message/ContentPart`, `protocols/` + `route/` + `providers/`. Per-part `providerMetadata` round-trips (reasoning signatures, tool call metadata: `Message.tool({providerMetadata})`). Models can declare `compaction: {type: "native"}` to use provider-native compaction.
- Cache: `ai/src/cache-policy.ts` compiles `LLMRequest.cache` into `CacheHint`s before per-protocol lowering. Default "auto" = breakpoints at last tool, first and last distinct system parts, conversation tail (`messages:{tail:1}`); cap 4 breakpoints (Anthropic); manual hints count toward cap; `"none"` and `{ttl}` options; protocols with implicit cache ignore markers.
- Tool loadout changes are declared in-transcript as `Message.tools({add, remove})` markers (`tool-updates.ts`) so the leading tool declaration (and cache prefix) stays fixed; same idea for effort updates (`effort-updates.ts`). Pi does the same (see section 3).

### Tools
- Built-in plugins (`core/src/tool/plugin/*`): `read`, `write`, `edit`, `patch`, `shell`, `glob`, `grep`, `webfetch`, `websearch`, `todo`, `question`, `skill`, `subagent`, `cd`, `session-search`, `session-interrupt`, `artifact_*`, `automation`, `machine`, `image-generate`, `video-generate`, `list_mcp_resources`, `read_mcp_resource`, `list_mcp_events`, worktree/move/rename (`opencode` namespace). Tools default into CodeMode (`codemode: true`) unless `codemode:false`.
- `edit` (`tool/plugin/edit.ts`): `{path, oldString, newString, replaceAll}`; match cascade: exact, then ignoring typographic quote/dash/space variants, then trailing whitespace per line; preserves CRLF/LF; multiple matches FAIL unless `replaceAll`. Error: `Could not find oldString in <path>. It must match exactly, including whitespace and indentation.` plus a hint (newString already present = edit already applied, else the closest lines with numbers, found by lines voting where oldString starts, >= half must agree) (L192-343).
- `patch` uses Codex-style envelope (`tool/patch.txt`, `patch-grammar.txt`): `*** Begin Patch`, Add/Delete/Update File, `*** Move to:`, `@@`.
- Output limits: `core/src/tool-output.ts`: `MAX_LINES = 2000`, `MAX_BYTES = 50 KiB`, overflow written to retained file `tool_<id>` (retention 7 days) and path returned. Shell default timeout 2 min (10 min in code mode). Webfetch timeout 30 s default / 120 s max, 10 redirects.

### Context management
- Token estimate chars/4 (`util/token.ts`), images 1_500, PDFs 2_000 tokens (`compaction.ts:148`); prefers provider-measured prompt tokens from last response + estimate for tail (`Estimate{measured, estimated}`).
- Trigger: ceiling = window - max(15% of window, 16_000) (`calculateCeiling`, `RESERVE_SHARE=0.15`, `RESERVE_MIN=16_000`); configurable `buffer` (tokens or "20%"); `keep: 15_000` tokens verbatim tail; `auto:true`; `background:true`.
- Background pre-computation (the standout idea, `compaction.ts:155-168`): start summarising when within `PRECOMPUTE_LEAD = 60_000` tokens of the ceiling (summary takes 30-60 s; growth p99 ~22-25k per step), only while >= `CACHE_MARGIN = 30_000` ms of the 5-min `CACHE_LIFETIME` remains (cache-aware), held summary swapped in when due if tail <= `PRECOMPUTED_TAIL_SHARE = 0.4` of budget; stops after `PRECOMPUTE_MAX_FAILURES = 2`.
- Overflow handling: on provider "context too long" rejection, retry aiming at `SHRINK_STEPS = [0.85, 0.7, 0.5, 0.35]` of the rejected size. Summary output floor 32_000 tokens (`SUMMARY_OUTPUT_FLOOR`).
- Prompt strategy (`buildPrompt`): fixed markdown template (Objective, Requirements, Decisions, Work State {Completed, Active, Blocked}, Next Move, Relevant Files (max 15), Important Context), rules ("Preserve exact file paths, symbols, commands, error strings", "Do not mention the summary process"), explicit "leave out AGENTS.md/setup the assistant was given", separate UPDATE prompt that merges into the previous checkpoint ("Newer history always takes precedence"). A nudge re-prompt if template not filled. Dedicated `compaction` agent can pick a cheaper model.
- Pruning of tool results is built into the summariser transcript: 5 levels per exchange `[whole, outputs clipped, outputs removed + inputs cut, user text only, omitted]`, clip = 2_000 chars from start and end (`TRANSCRIPT_CLIP`).

### Session persistence
- Event-sourced: events (`session/event.ts`) projected by `session/projector.ts` (immer) into SQLite via drizzle (`session/sql.ts`: session, message, part, inbox tables); blobs in `blob.ts`. Large media in blob store. Fork: `Session.fork` (ForkInput, `ForkEmptyError`), `revert.ts`, `transfer.ts`, `move.ts`. Compaction messages reference kept messages by `metadata.keepID` (history rebuilt from the log; nothing is destroyed). SQLite floor 3.39.4 documented in `packages/core/AGENTS.md` (macOS system sqlite).

### Subagents
- `tool/plugin/subagent.ts`: subagents ALWAYS run in background; call returns `sessionID`; parent receives `<background-result kind="subagent">` message each time the child goes idle; follow-up to existing child by sessionID. Depth limit `experimental.subagent_depth` default 1 (`session/subagent-depth.ts`). Child is a normal session with `parentID`.

### Sandbox and approvals
- No OS sandbox found (grep "sandbox" hits only cwd-instructions/mcp-stdio/file-access; inferred: none). Permission service: rules `{action, resource, effect(allow|deny|ask)}` per agent (`permission.ts`, `permission/policy.ts`, `saved.ts`); typed outcomes `DeclinedError`, `CorrectedError{feedback}` (decline with feedback continues the turn), `BlockedError{rules}`; `Unattended` ruleset for scheduled runs (unresolved asks become denies); saved approvals persisted. `managed-policy.ts` for org policy.

### MCP / hooks / skills / instructions
- `core/src/mcp/` (stdio, socket, SSE event-stream, oauth, pool). Plugin system with hooks (`plugin/hooks.ts`; e.g. `session.retry` decision hook), `plugin-browser`, `gui-plugins`. Skills `skill/` + `skill` tool. Instructions: `instruction-discovery.ts` walks for `AGENTS.md`, `CLAUDE.md` used only where no AGENTS.md in that dir (and `~/.claude/CLAUDE.md` if global AGENTS.md missing); per-cwd instruction state `cwd-instructions.ts`.

### Code mode
- `packages/codemode` + external `psychoscript` package: own interpreter for a JS-like language, NOT a sandboxed JS engine ("Rather than trying to sandbox arbitrary JavaScript, CodeMode only runs the language features we implement"); tools exposed by namespace; host limits time, call count, output.

### Performance
Cache-policy compiler; background compaction; tool-loadout markers keep prefix stable; `packages/core/perf/`; blob store for large payloads; keyed mutex per session.

### Strengths / Weaknesses
- + Best-documented compaction engineering with real p99 numbers; replayable event log; edit-miss hint; background subagents as first-class sessions.
- - No OS sandbox; heavy Effect/TypeScript stack; fast-moving rewrite (v1 dir still present) so APIs unstable.

### Adopt / Avoid / Reusable
- Adopt: background precompute with cache-lifetime gate and 60k lead; reserve = max(15%, 16k); SHRINK_STEPS on overflow; 5-level transcript clipping for summariser; "update" summary prompt; edit-miss hints and tiered fuzzy matching (quotes/dashes/trailing whitespace); steer/queue inbox delivery modes; "decline with feedback" outcome; tool loadout as transcript markers; Unattended ruleset.
- Avoid: reliance on system SQLite versions; letting subagents be background-only without a blocking wait mode (Codex offers `wait_agent`); no sandbox.
- Reusable (MIT, free to reuse): `packages/ai` cache-policy and compaction logic as reference (TypeScript; Axiom engine language open, so port algorithms not code); `patchmark` unrelated (markdown streaming heal). Low direct reuse if engine is Rust/Dart.

---

## 3. Pi (`packages/agent`, `packages/coding-agent`, `packages/ai`)

### Agent loop
- `packages/agent/src/agent-loop.ts` (952 lines). Nested loops: outer for follow-ups, inner while `hasMoreToolCalls || pendingMessages.length`. Hooks on config: `getSteeringMessages`, `getFollowUpMessages`, `prepareNextTurn` (can swap context/model/thinking level, used for compaction), `prepareRequest`, `finishTurn` (returns `end`/`continue`), `transformContext`, `convertToLlm`.
- Steering enters AFTER the current assistant turn's tools complete (not mid-tool); follow-ups only when agent would otherwise stop; modes `steeringMode`/`followUpMode` = "one-at-a-time" default (`settings-defaults.ts`). Abort returns queued messages to editor.
- Tools: default `toolExecution: "parallel"` (prepare sequentially, execute allowed tools concurrently); any tool with `executionMode:"sequential"` forces the whole batch sequential (`executeToolCalls`).
- `stopReason=length`: all tool calls in that message are failed without execution: `Tool call "X" was not executed: the response hit the output token limit, so its arguments may be truncated. Re-issue the tool call with complete arguments.`
- Retry default: enabled, 3 retries, base 2 s, agent delay cap 60 s, provider cap 60 s.

### Provider abstraction
- `packages/ai/src/api/*`: protocol adapters `anthropic-messages`, `openai-responses`, `openai-completions`, `openai-codex-responses`, `azure-openai-responses`, `google-generative-ai`, `google-vertex`, `bedrock-converse-stream`, `mistral-conversations`, ... plus ~70 provider files with generated model catalogs (`providers/*.models.ts`, `models.generated.ts`).
- Preservation: `thinkingSignature` (opaque provider replay data, Anthropic `signature_delta` accumulated, OpenAI encrypted reasoning), `redacted: true` with encrypted payload kept in `thinkingSignature`, `textSignature` (OpenAI message metadata), usage `cacheWrite`, `cacheWrite1h` (Anthropic split). `api/transform-messages.ts` normalises cross-provider handoffs (placeholders such as "(image omitted: model does not support images)"; Anthropic rejects images inside error tool results so they are split out after the results). Anthropic: empty signature from aborted streams is repaired or block dropped (L1719-1739).
- Cache control: `cache_control` on last tool, system, last user block (`anthropic-messages.ts:1494-1938`); `cacheRetention` option; `cache-warming.ts` (+ `core/cache-warmer.ts`): economic decision: warm only if `continuationProbability * missCost - warmCost >= CACHE_WARMING_MINIMUM_EXPECTED_SAVINGS`.
- Tool loadout deltas declared as `toolsAdded/toolsRemoved` on system messages in the transcript (`declareToolChanges`, agent-loop.ts L~335); replay yields exactly `context.tools`.

### Tools
- Names (`coding-agent/src/core/tools/index.ts`): `read`, `bash`, `powershell`, `edit`, `write`, `grep`, `find`, `ls`. Default active: `read, bash, edit, write` (`system-prompt.ts:64`). Extensions add `tool_search`, `codemode` (registered inactive), MCP tools, subagent example.
- `edit`: `{path, edits:[{oldText,newText}]}`; each oldText matched against the ORIGINAL file (not incrementally), must be unique and non-overlapping; errors: `Could not find edits[i] in <path>. The oldText must match exactly including all whitespace and newlines.` / `Found N occurrences of edits[i] in <path>. Each oldText must be unique. Please provide more context to make it unique.` Returns unified patch + `firstChangedLine`. `file-mutation-queue.ts` serialises writes per file (needed because tools run in parallel).
- Limits (`tools/truncate.ts`): `DEFAULT_MAX_LINES = 2000`, `DEFAULT_MAX_BYTES = 50 KiB`, `GREP_MAX_LINE_LENGTH = 500`. Read truncates head ("use offset/limit"); bash keeps TAIL, full output saved to temp file (`full_output_path`). bash timeout optional, no default. In codemode bash returns up to 1 MiB (first/last 512 KiB around marker).

### Context management
- Token estimate chars/4 per message (`compaction.ts:298`) + last provider usage. Trigger: `contextTokens > contextWindow - reserveTokens` with defaults `reserveTokens = 16384`, `keepRecentTokens = 20000` (`DEFAULT_COMPACTION_SETTINGS`).
- Cut point: walk backward until `keepRecentTokens` accumulated; may split a turn (`isSplitTurn`, summarises turn prefix separately). Summary prompt: fixed markdown (Goal, Constraints & Preferences, Progress {Done, In Progress, Blocked}, Key Decisions, Next Steps, Critical Context) "Preserve exact file paths, function names, and error messages", and an UPDATE variant ("PRESERVE all existing information ... move In Progress to Done"). Summarisation calls: `cacheRetention:"none"`, retried, fail closed if `stopReason==="length"` ("the summary is incomplete") so a partial summary is never persisted. Branch summaries for tree navigation (`compaction/branch-summarization.ts`).
- No tool-result pruning pass; truncation at ingest only.

### Session persistence
- JSONL tree, `CURRENT_SESSION_VERSION = 3` (`session-manager.ts:41`). Every entry has `id` and `parentId`; types `session` header, `message`, `thinking_level_change`, `model_change`, `compaction`, `branch_summary`, `custom`, `label`. Leaf pointer = active branch; continuing from an earlier entry branches in the same file; "fork"/"clone" copy history to a new file. Compaction entry replaces older messages in model context but originals stay. Migrations v1->v2 (add tree), v3. Docs: `coding-agent/docs/session-format.md`.
- `packages/durable` (experimental): storage-committed-before-display, crash-resume, failed model request counts as attempt so a request that kills the process eventually stops.

### Subagents
- Not built in by design; example extension `examples/extensions/subagent` spawns separate `pi` processes with isolated context, parallel streaming, agents as markdown files (scout, planner, reviewer, worker). Plan mode and permission-gate are extension examples.

### Sandbox and approvals
- None built in: `docs/security.md`: "does not ask for approval before every tool call"; recommends container/VM; project-trust gate (`defaultProjectTrust: "ask"`) controls loading project resources/extensions only. `tool_call` extension event can block/modify (permission-gate example).

### MCP / hooks / skills / instructions
- MCP: stdio + streamable HTTP, `~/.pi/agent/mcp.json` + `.pi/mcp.json` (project only after trust), per-server `exposure` (deferred via codemode/tool_search). Extensions: TypeScript modules in-process; events include `before_agent_start, context, context_with_system, before_provider_request/headers, after_provider_response, tool_call, tool_result, session_before_compact, session_before_fork, session_before_tree, input, user_bash, cache_warming_decision, mcp_servers_change`. Skills load on demand (descriptions only in prompt). Context files: `AGENTS.override.md, AGENTS.md, CLAUDE.md` walked from ancestors (`resource-loader.ts:215`); `SYSTEM.md` override; prompt templates.

### Code mode / deferred tools
- `codemode` tool (inactive by default): raw JS body in QuickJS sandbox (no Node/fs/net/timers), globals `tools.*`, `text()`, `image()`, `store/load`, `ALL_TOOLS`, `searchTools` (BM25, limit 8), `describeTool`; `max_output_tokens` default 10_000, hard cap 16,777,216 chars / 100,000 output calls; `timeout_ms` unset by default; inline declaration budget 3000 estimated tokens (`codemode.inlineBudget`); deferred tools omitted from description so it stays cache-stable as MCP servers connect.

### Performance
Lazy provider module loading (`*.lazy.ts`), cache warming economics, minimal system prompt, per-file mutation queue, native TUI components (`tui` build:native).

### Strengths / Weaknesses
- + Smallest, clearest loop; cleanest cross-provider message model (signatures/redaction); tree sessions give cheap branching; extension event surface is the richest; honest security doc.
- - No sandbox/approvals/subagents/plan in core; no tool-result pruning; steering only at turn boundaries; in-process extensions are full trust.

### Adopt / Avoid / Reusable
- Adopt: message model with `thinkingSignature`/`redacted`/`textSignature`; truncated-tool-call guard; edit with multiple disjoint edits matched against the original; per-file mutation queue; tree JSONL sessions; cache-warming expected-savings rule; fail-closed summaries; tool-loadout markers; `finishTurn` decision hook.
- Avoid: no default isolation; in-process untrusted extensions.
- Reusable (MIT; fully reusable): `packages/ai` (TypeScript), `packages/codemode` (QuickJS). Engine language open: if TypeScript/Node sidecar were acceptable, `pi-ai` is the most reusable provider layer of the four; otherwise port the transform-messages logic.

---

## 4. Goose (`crates/goose`, `crates/oi-kit-*`)

### Agent loop
- The loop is now an ordered list of `Operation`s over persisted conversation (`oi-kit-agent/README.md`; `goose/src/agents/state_machine/mod.rs`): machine walks the list, applies the first operation that applies, restarts from the top; behaviour = f(persisted conversation), not in-memory state (crash-safe, resumable). Operations: inference, toolcalling, tool approval/confirmation, compaction, tool-pair compaction, retry, thinking recovery, steer (`oi-kit-agent/src/steer.rs`: `SteerQueue = Arc<Mutex<VecDeque<Message>>>`, drained "between model and tool turns"), max_turns, stop hook (`DEFAULT_BLOCK_CAP = 8`), subagent, skills, recipe, final output, auto-effort, context relevance (observation-only experiment), subdirectory hints, expired tools.
- Max turns: `GOOSE_MAX_TURNS` (no default found in code; unset = unbounded; gateway default 5 `gateway/handler.rs:28`). At limit: "I've reached the maximum number of actions I can do without user input. Would you like me to continue?"; model gets `<turn-budget>N/M used</turn-budget>` once half the budget is used (`oi-kit-agent/src/max_turns.rs`).
- Recovery: thinking-only reply -> "You did some thinking but did not produce an answer. Please continue." up to `MAX_THINKING_RECOVERY_ATTEMPTS = 3`; context-overflow compactions capped `MAX_CONTEXT_ERROR_COMPACTIONS = 2`; empty-response recovery.
- Parallel tool calls: executed concurrently per message (inferred from `ops_toolcalling.rs`; not verified).

### Provider abstraction
- `oi-kit-providers` (anthropic, google, databricks, azure_foundry, openai-compatible declarative JSON providers, live/voice) over `oi-kit-provider-types` canonical `Message`/`MessageContent` with `Thinking{thinking, signature}` and `RedactedThinking{data}` variants (`conversation/message.rs:234,603-610`), `ThinkingEffort`. Declared `CacheSemantics::for_model(provider, model)` (`cache_semantics.rs`): `ExplicitBreakpoints{max_breakpoints:4}` (anthropic, minimax, zai, kimi_code, claude on bedrock/vertex/databricks/openrouter), `ImplicitTolerant` (longest-prefix: moonshot, deepseek, groq, together, fireworks, mistral, ...), `ImplicitStrict` (OpenAI Responses models; unknown default), `Uncached`. This is a clean idea: policy by (provider, model), not by wire format.
- Retry/backoff in `oi-kit-provider-types/src/retry.rs` (max_retries, provider-supplied retry delay honoured).

### Tools
- Built-in "developer" platform extension (`agents/platform_extensions/developer/mod.rs`): `write`, `edit`, `shell`, `tree`, `read_image` (no text-read tool: "To read text files, use the shell tool (for example, cat or type)"). Others: `analyze`, `todo`, `summon` (delegate/subagents), `summarize`, `chatrecall`, `scheduler`, `code_execution` (JS), `javascript`, `apps`, `tom`.
- `edit`: `{path, before, after}` "before text must match exactly and uniquely. Use empty after text to delete."; miss: "No match found for the specified text." (`developer/edit.rs:135`). Shell: stdout and stderr separate fields (output schema), each stream limited to 2000 lines, longer saved to temp file; GOOSE_SHELL override; cmd.exe newline warning. Image max 20 MiB.
- Global tool-response guard: any MCP/tool text over 200,000 chars (`GOOSE_MAX_TOOL_RESPONSE_SIZE`) is written to a temp file and replaced with "The response returned from the tool call was larger (N characters) and is stored in the file which you can use other tools to examine or search in: <path>" (`agents/large_response_handler.rs`).

### Context management
- Token counting: `token_counter.rs` (tokenizer-based, not chars/4; inferred from `count_chat_tokens`) plus provider usage. Trigger: `DEFAULT_COMPACTION_THRESHOLD = 0.8` of context limit, additionally capped by `DEFAULT_AUTO_COMPACT_TOKEN_LIMIT = 225_000` tokens (`context_mgmt/mod.rs:10`, `capped_compaction_threshold`).
- Compaction prompt (`oi-kit-context-management/src/prompts/compaction.md`): asks for ONE JSON block with fields `user_intent[], technical_concepts[], files[{path,summary,key_code}], errors_and_fixes[], problem_solving[], user_messages[], pending_tasks[], current_work, next_step`; errors quoted verbatim; "ok to make it much longer than a normal summary"; parsed by `structured.rs` (`</analysis>` terminator handling). After compaction, the conversation gets a continuation text ("Your context was compacted... Do not mention that you read a summary... Continue calling tools as necessary" for tool loops, `compaction/layout.rs`).
- Summariser fallback when history too big: `REMOVAL_PERCENTAGES = [0, 10, 20, 50, 100]` of tool responses removed from the middle outwards (`summarize.rs`).
- Tool-pair compaction (distinctive): older tool request/response pairs are replaced by LLM-written one-line summaries in batches of `TOOLCALL_SUMMARIZATION_BATCH_SIZE = 10`, once tool-call count exceeds `compute_tool_call_cutoff` which scales with window (tests: 128k@0.8 -> 15, 200k -> 24, 1M -> 120, 50k -> 10).

### Session persistence
- SQLite `sessions.db` (sqlx, WAL), `CURRENT_SCHEMA_VERSION = 18`; tables `sessions` (id, name, working_dir, extension_data JSON, token counters, ...) and `messages` (id, message_id, session_id, role, `content_json`, tokens, `metadata_json`) (`session/session_manager.rs:32-1225`). `copy_session` implements fork. `BEGIN IMMEDIATE` for schema init (cross-process race fix). Import formats from other tools (`session/import_formats`), export HTML/Markdown, chat-history search. Recipes (YAML) and scheduler for repeatable runs.

### Subagents
- `summon` platform extension: `delegate` with `{instructions|source (recipe/agent/subrecipe), parameters, extensions, provider, model, temperature, max_turns, context, working_dir, resume}`; per-delegate provider/model override; `resume` continues a prior subagent; `DELEGATED_META_KEY` marks delegated runs; `subagent_execution_tool/` for subrecipes (parallel execution; inferred). Agent/recipe descriptions budgeted at 160 chars in the tool description.

### Sandbox and approvals
- `GooseMode`: `auto` (default, approve all), `approve` (ask for every tool call; alias `chat`), `smart_approve` (ask only for sensitive; LLM-judged `permission_judge.rs`; read-only annotation based, inferred). Tool annotations (read-only/destructive/idempotent/open-world) drive it. Inspector pipeline (`tool_inspection.rs`): `Allow | Deny | RequireApproval` from permission, security (`security/`: regex threat patterns with risk levels + ML prompt-injection classifier, confidence threshold default 0.8), adversary and egress inspectors. No OS sandbox (inferred from absence in crate list).

### MCP / hooks / skills / instructions
- MCP via `rmcp`; extension kinds: builtin, platform (in-process), stdio, streamable HTTP, inline Python; `ToolDeferral`: MCP tools listed in the prompt only until model calls `enable_tools` (disabled when code-mode is on) with `always_load_tools` allowlist. Hooks (`hooks/mod.rs`): `PreToolUse, PostToolUse, SessionStart, UserPromptSubmit, Stop` (Claude-Code-compatible JSON config). Instructions: `.goosehints`/AGENTS.md via `hints/` with `import_files`, subdirectory hints loaded lazily when tools touch subdirs (`ops_subdirectory_hints.rs`). Skills (`skills/`, builtin skills, slash commands), plugins (`plugins/` discovery incl. MCP servers), ACP (`goose-acp-*`) for editor integration.

### Performance
Operations are small and cancellable (`finalize_cancellation`); `oi-kit` compiles to wasm32 (`cfg(target_arch="wasm32")` async_trait variants); OTel/GenAI telemetry; local inference crate.

### Strengths / Weaknesses
- + Loop is a pure function of persisted state (resumable, testable); declarative cache semantics; tool-pair compaction; broadest extension model (MCP + recipes + ACP); inspector pipeline.
- - No OS sandbox; compaction relies on a huge JSON summary; default mode `auto`; heavy codebase churn (extracted into oi-kit alpha crates, version 0.1.0-alpha.15).

### Adopt / Avoid / Reusable
- Adopt: Operation pipeline for loop concerns (retry, thinking recovery, max-turn budget notice, stop-hook block cap); `CacheSemantics` per (provider, model); tool-pair compaction with window-scaled cutoff; 200k-char large-response spill; compaction cap `min(0.8 x window, 225k)`; continuation text after compaction; `<turn-budget>` hint; schema-versioned SQLite with `BEGIN IMMEDIATE`.
- Avoid: ML classifier dependence for prompt-injection; JSON-only huge summaries as sole carrier; auto-approve default.
- Reusable (Apache-2.0): `oi-kit-agent`, `oi-kit-provider-types`, `oi-kit-context-management`, `oi-kit-providers` are separable crates, alpha, and the closest thing to a reusable engine library here. Worth evaluating if Axiom's engine is Rust (inferred: they depend on rmcp/tokio/reqwest; licence per crate `license.workspace = true` = Apache-2.0 root). Needs a spike to confirm API stability; do not vendor before a 1-day trial.

---

## 5. Cross-project comparison (numbers)

| Item | Codex | OpenCode | Pi | Goose |
|---|---|---|---|---|
| Compaction trigger | min(cfg, 90% window); hard 95%; +post-turn % | window - max(15%,16k) | window - 16,384 | 80% window, cap 225k |
| Kept after compaction | user msgs up to 20k tok + summary | 15k tok tail + summary | 20k tok tail + summary | summary only (+continuation text) |
| Summary format | free text handoff | markdown template + update prompt | markdown template + update prompt | JSON schema, 9 fields |
| Background compaction | yes (async candidate) | yes (60k lead, cache-aware) | no | no |
| Tool output cap | 10k bytes model default; 1 MiB PTY | 2000 lines / 50 KiB, spill 7 d | 2000 lines / 50 KiB, tail for bash | 2000 lines/stream; 200k chars global |
| Token estimate | bytes/4 | chars/4 | chars/4 | tokenizer (inferred) |
| Edit format | Lark apply_patch (freeform) | oldString/newString + patch tool | edits[] oldText/newText | before/after |
| Parallel tools | RwLock, start while streaming | per ai loop (not verified) | default parallel, sequential override | per message (inferred) |
| Session store | JSONL + SQLite index | event-sourced SQLite | JSONL tree | SQLite messages |
| Fork | copy rollout prefix | Session.fork | tree branch / clone | copy_session |
| Subagent depth/threads | depth 1, 6 threads | depth 1, background | none (ext) | delegate w/ model override |
| Sandbox | seatbelt, bwrap+landlock, windows | none | none | none |
| Approval | 4 policies + guardian LLM + execpolicy | rules allow/deny/ask + feedback | none (hook) | auto/approve/smart + inspectors |
| Retry | conn 5 s..60 s | schedule, unreachable window 1 h | 3x, 2 s base, 60 s cap | provider delay honoured |

---

## 6. What Axiom's native engine should do (ranked)

1. **Persist canonical, provider-native messages, not a lowest-common-denominator.** Pi's `thinkingSignature`/`redacted`/`textSignature`/`cacheWrite1h` and Codex's native `ResponseItem` envelopes both show that replaying opaque reasoning is mandatory. Store per-block `providerMetadata` (opaque string/JSON) and a `provider` tag; transform on handoff (Pi `transform-messages.ts`) rather than at write time.
2. **Declare cache semantics per (provider, model)** (Goose `CacheSemantics`) and compile breakpoints in one place (OpenCode `cache-policy`: last tool, first/last system, tail; <=4). Keep prefix stable: tool loadout changes and effort changes as in-transcript markers (OpenCode, Pi), never by mutating the leading tool list. Add Pi's expected-savings rule if cache warming is ever added.
3. **Compaction v1 = OpenCode numbers, Pi/OpenCode template.** Trigger at `window - max(15%, 16k)` with hard 95% guard; keep 15-20k tokens verbatim; markdown template + separate update prompt; fail closed on `length`; summariser `cacheRetention none`; shrink steps `[0.85,0.7,0.5,0.35]` on provider overflow; transcript clipping levels with 2,000-char head/tail clip. Phase 2: background precompute (60k lead, only inside cache lifetime). Add Goose-style tool-pair compaction (batch 10, window-scaled cutoff) as the cheap first line of defence before full summarisation.
4. **Loop as persisted-state function with explicit, small operations** (Goose) but keep Pi's simple vocabulary: steer (after current tool batch, mid-turn into same turn like Codex), follow-up (when idle), `finishTurn` decision hook, stop-hook with block cap 8, `length` => fail tool calls with Pi's wording, thinking-only recovery (3), max-turn budget notice at 50%.
5. **Tool execution:** parallel by default with RwLock-style gate (Codex) and per-file mutation queue (Pi); start executing as soon as a tool call's arguments complete while the model is still streaming (Codex). Interrupt => inject `<turn_aborted>`-style note that processes may still run.
6. **Edit/patch:** offer both (a) `edit` with `edits[]` matched against original, unique, tiered fuzzy (exact, typographic quotes/dashes/space, trailing whitespace) and miss hints (OpenCode) and (b) Codex `apply_patch` grammar for models trained on it; port `apply-patch` + `seek_sequence` (Apache-2.0).
7. **Output limits:** 2000 lines / 50 KiB inline (OpenCode/Pi) with full output spilled to a retained file (7 days) and path returned; head-keep for read, tail-keep for shell; global 200k-char guard for MCP responses (Goose); truncation header with original token count (Codex). Shell: yield window 250 ms-30 s, 1 MiB ring buffer, <=64 live processes.
8. **Sessions:** append-only event log (OpenCode/Codex) projected to SQLite for queries; JSONL export for portability; every message gets id + parentId (Pi) so fork/branch is free; compaction entries reference kept ids (OpenCode `keepID`) so history is never destroyed. Version schemas (Goose v18 lesson) and use `BEGIN IMMEDIATE` for migrations. Because Axiom has multiple processes (daemon + shell), design for single-writer daemon.
9. **Safety:** the only project with OS sandboxes is Codex; Axiom should ship workspace-write sandbox policy as data (seatbelt `.sbpl`, bwrap args) in the daemon, plus rules `{action, resource, allow|deny|ask}` (OpenCode), "decline with feedback" outcome, `Unattended` rule set for scheduled runs, and a prefix-rule exec policy (Codex `execpolicy`). Defer guardian-LLM reviewer; add inspector pipeline hook points (Goose) for later.
10. **Subagents:** child = ordinary session with parentID; depth default 1; max concurrent threads default 6; async by default with explicit `wait`; per-delegate model override (Goose); report back as a synthetic message each time the child goes idle (OpenCode).
11. **Deferred tools / code-mode:** implement `tool_search` (BM25, default 8) first; defer MCP tools when their schemas exceed a budget (Pi: 3000 est. tokens inline; Codex: 8k per spec, 64k total). Code-mode is optional phase 2; if built, use an interpreter that exposes only tools (OpenCode/Pi), output default 10k tokens.
12. **Delegation to external harnesses:** treat Codex (app-server, `codex-rs/app-server*`), Goose (ACP, `goose-acp-*`), Pi (RPC mode JSONL, `rpc.md`) and OpenCode (server, `packages/server`) as protocol peers; normalise on ACP-like events. (Pi RPC and Codex app-server verified to exist; protocol depth not read.)
13. **Instruction files:** AGENTS.md first, CLAUDE.md fallback only where no AGENTS.md in that directory (OpenCode), 32 KiB cap (Codex), ancestor walk (Pi), lazy subdirectory hints (Goose), project trust gate before loading project executables (Pi).

## 7. Unverified

- Codex remote-compaction wire details (opaque/encrypted replacement history is inferred from names); whether `compaction_trigger` is accepted by non-OpenAI providers.
- Codex guardian reviewer's real prompt/budget; exec-policy rule syntax; Windows sandbox behaviour.
- Codex "code-mode" runtime engine (V8 vs other) inferred from crate names.
- OpenCode: parallel tool execution policy in `ai/src/tool-runtime.ts` not read; steer vs queue semantics of the runner not read beyond schema; native compaction behaviour; event-sourcing replay specifics; whether any sandboxing exists outside `packages/core`.
- Pi: no tool-result pruning found, but not exhaustively searched; durable package semantics from README only.
- Goose: concurrency of tool calls, tokenizer used by `token_counter.rs`, smart-approve judge prompt, default max turns (none found), subrecipe parallelism, and oi-kit crate API stability/licence per crate.
- No project was built, run or benchmarked; no latency or memory numbers were measured. All limits are code constants at the commits above and change quickly (all four repos moved daily).
- Marketing claims, issues and docs outside the repo were not consulted.
