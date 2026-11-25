# Multi-agent orchestration: prior art and information architecture

Researched 2026-11-24. Sources: shallow clones (`--depth 1`, scratchpad `research/orch/<name>`; read only, nothing run or built) and vendor docs. Commit SHAs are the clone heads. Claims marked (docs) come from vendor pages, (src) from source, (web) from third-party articles. Anything I could not confirm is listed under Unverified at the end.

## 0. Roster and licences

| Project | Commit | Licence | Stack | State |
|---|---|---|---|---|
| T3 Code `pingdotgg/t3code` | f692811 | MIT (marks excluded, TRADEMARK.md) | Node/Effect server, React web, Electron, RN mobile | very active (76k stars; "500k users" per its AGENTS.md) |
| Vibe Kanban `BloopAI/vibe-kanban` | 3f095f0 | Apache-2.0 | Rust (axum, sqlx), React, Tauri | sunset announced 2026-04-10 (company closing, no business model); community-maintained, remote features removed (web) |
| Claude Squad `smtg-ai/claude-squad` | 070eb93 | **AGPL-3.0** | Go, tmux, Bubble Tea TUI | maintained lightly |
| Crystal -> Nimbalyst `stravu/crystal` | 1e18e0b | MIT | Electron, TS | README now points to Nimbalyst; issue "Project dead?" open |
| Superset, now **Paradev** (`superset-sh/superset`, `paradevai/paradev`) | 627d606 | MIT | Electron, Bun, tRPC, Expo, pty-daemon | very active, macOS first, Linux experimental, no Windows |
| Sculptor `imbue-ai/sculptor` | cdd6dc6 | MIT | Python backend, React/Electron, jj support | "experimental research preview" |
| Emdash `generalaction/emdash` | c876989 | Apache-2.0 | Electron, drizzle, workspace-server (remote), ACP | very active, ADR-driven |
| opencode `sst/opencode` | 678621a | MIT | Bun/Effect, SolidJS app + Electron desktop, TUI | extremely active; core rewritten (see 6) |
| Zed `zed-industries/zed` | 2b248e6 | GPL-3.0 for `acp_thread`, `agent_ui` (Cargo.toml); Apache for protocol crate | Rust/GPUI | Parallel Agents shipped 0.233.5 (2026-04-22), Terminal Threads 2026-05-20 (web) |
| Conductor | closed | closed | Mac app | docs only |
| Cursor 3 Agents Window, Codex app, Claude Code agent teams, Antigravity Agent Manager | n/a | closed | | docs/web only |

GPL/AGPL rule for Axiom: Zed's agent UI crates and Claude Squad are read-for-ideas only. Everything MIT/Apache is reusable with attribution (dependency policy: decisions/0011).

## 1. Cross-project summary (one table)

| | Process architecture | Harness driving | Isolation | Persistence |
|---|---|---|---|---|
| T3 Code | one server owns providers, terminals, git, state; web/desktop/mobile are remote clients over WS RPC | per-provider adapter: Claude Agent SDK `query()`, Codex `app-server` (generated schema), ACP, opencode server, Cursor SDK, Pi | optional worktree per thread, hidden-ref checkpoints | SQLite `statev2.sqlite`, event log + projections + outbox |
| Vibe Kanban | one Rust server (+ Tauri/npx) with REST + WS JSON-patch streams | child process: Claude stream-json stdin/stdout with `--permission-prompt-tool=stdio` and hooks; Codex `app-server`; ACP harness v2 for Gemini/Qwen/Copilot; per-CLI log normalisers | worktree per workspace | SQLite (sqlx migrations), `execution_processes` rows |
| Claude Squad | single Go TUI, tmux sessions per agent | PTY scraping: `tmux new-session`, SHA-256 of pane content each tick to detect "updated", prompt-string match to detect "needs input"; `AutoYes` presses enter | worktree per session, branch kept on pause | `~/.claude-squad` JSON |
| Crystal | Electron main with services; IPC to React | spawns CLI via node-pty; Claude permission via a local MCP "permission bridge" server (`mcpPermissionBridge.ts`, `permissionIpcServer.ts`); Codex panel manager | `git worktree add -b` under project | SQLite via better-sqlite3 |
| Paradev/Superset | desktop + `host-service` (local or remote machine) + `pty-daemon` + cloud API/relay/mobile | terminals first: any CLI in a PTY, with injected wrapper scripts and hook files per agent (`agent-setup` has ~20 wrappers) posting status to a notify hook; chat mode over ACP (`agent-harness`) | worktree per workspace; ports found by scanning process tree, not allocated | local SQLite + host db |
| Sculptor | Python backend sidecar + web/Electron frontend | native Claude SDK harness, Pi harness, and "terminal agents" (PTY, status via local HTTP signal API from Claude hooks) | workspace = isolated copy, optional container | SQLite, per-task state machine |
| Emdash | desktop + `workspace-server` on each Host (local or SSH) over typed `wire` protocol | provider plugins: CLI in PTY, or ACP chat sessions (`packages/core` ACP runtime with SessionManager/Cell/Router) | worktree per task; host inventory authoritative | drizzle SQLite on desktop, host registry |
| opencode | server (HTTP + SSE event feed) with core in Effect; TUI, web app and desktop are clients | native agent loop plus plugin providers (a `claude-agent` provider exists) | `worktree.ts`, `persistent-pty` | SQLite, sessions with `parentID` |
| Zed | single GPUI process; ACP agents as subprocesses over stdio | ACP v1 and v2 (SubagentUpdate, state_update), plus terminal threads | optional git worktree per thread, archived and restorable | local sqlite `ThreadMetadataStore` |

Observed pattern: the projects that use structured protocols (T3, VK, Emdash, Zed, Sculptor-native) win on rich UI (approvals, diffs, usage, queueing). PTY-only projects (Claude Squad, Superset terminal mode) win on "works with anything on day one" and lose on every structured feature. The winners run both: structured where a protocol exists, PTY plus hooks elsewhere (Superset, Sculptor, Zed Terminal Threads).

## 2. T3 Code deep dive (f692811)

### 2.1 Process and layering
- Monorepo: `apps/{server,web,desktop,mobile,marketing}`, `packages/{contracts,client-runtime,provider-*,source-control-*,effect-acp,effect-codex-app-server,...}`. `docs/internals/01_architecture.md` states the rule: the server owns provider processes, terminals, git, checkpoints, durable state and files; clients never substitute their own filesystem or credentials. Desktop bundles a server but its renderer follows the same remote boundary, so local and remote are one code path.
- Native modules never load on the Electron main startup path; crash-prone helpers run in child processes with deadlines (arch doc).
- Provider adapters sit behind `packages/provider-core/src/server/ProviderAdapter.ts` (`ProviderAdapterV2`: `openSession`, `ensureThread`, turn start, `steer`, resume, interrupt). Rule in `docs/internals/03_providers.md`: orchestration and clients read declared capabilities, never provider names. "Declare only what the provider supports. A capability left enabled that fails at runtime is a bug." "Never emulate missing permission modes."
- Drivers (src): `apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts` (Claude Agent SDK `query()`, 15.6k lines incl. SDK quirks such as an unbounded boot of about a minute), `CodexAdapterV2.ts` (`effect-codex-app-server` client generated from the app-server schema, 56k-line generated file), `AcpAdapterV2` over `packages/effect-acp` (generated from ACP schema), plus Cursor, Grok, Kiro, Muse, Pi, opencode, Antigravity, fx. ACP policy: strict spec compliance for third-party agents, no vendor workarounds in the shared adapter; first-party flavours get dedicated packages.

### 2.2 Event model and persistence (`packages/contracts/src/orchestrationV2.ts`, 4.9k lines)
- CQRS with an outbox. `Orchestrator.ts` serialises commands and decides events with no I/O. `EventSink.ts` commits events + projections + command receipts + outbox effects in ONE DB transaction, then notifies subscribers. `EffectWorker.ts` runs side effects after commit and feeds results back as commands. Ack means "intent committed", not "provider finished". Effects tied to a lost provider process are retired on recovery, not replayed.
- Event log keeps only a bounded replay window, projections are the truth, retained events must stay decodable across upgrades.
- Domain events (about 40 types): `thread.created`, `thread.{archived,settled,snoozed,pinned,...}`, `run.created/updated`, `run-attempt.*`, `node.updated`, `subagent.updated`, `provider-session.attached/updated/detached`, `provider-thread.updated`, `provider-turn.updated`, `runtime-request.updated`, `message.updated`, `turn-item.updated`, `plan.updated`, `checkpoint.captured`, `run.background-work-cancelled`.
- Entities: Thread (durable, survives provider exit) -> Run (`preparing|queued|starting|running|waiting|completed|interrupted|failed|cancelled|rolled_back`) -> RunAttempt -> ExecutionNode tree (`root_turn, assistant_message, reasoning, plan, todo_list, tool_call, approval_request, user_input_request, subagent, hook, system`, with `parentNodeId`, `rootNodeId`, `countsForRun`). TurnItem union: `user_message, assistant_message, reasoning, proposed_plan, todo_list, user_input_request, file_change, command_execution, file_search, web_search, approval_request, checkpoint, run_interrupt_*, system_notice, error, compaction, handoff, fork, subagent, dynamic_tool, notification`.
- App ids are primary, provider ids are refs (`nativeItemRef`). "Never let child provider turn completions close a parent run."
- Run completes when the provider turn ends. The checkpoint is a later outbox effect; readers treat a missing checkpoint as pending, and steers/queued turns/rollbacks line up behind it. Late finalisation must not keep the UI showing "active".
- Checkpoints are hidden git refs (never commits on the user's branch). Revert is rejected before touching files if the provider cannot roll its conversation back.
- `RuntimeRequest` (approvals, user input, auth refresh): `status pending|resolved|expired|cancelled`, `responseCapability: live | message | not_resumable`. So a request survives a process restart as either "answer via a new message" or "cannot resume", never as a dangling promise. Provider-native option ids pass through unchanged and every request must be declinable.
- Settlement is server-owned (`thread.settle/unsettle/snooze/auto-settle`); clients render persisted state, never local clocks.

### 2.3 Streaming at scale (the part to copy)
- `docs/internals/16_performance-regressions.md` makes wire budgets a CI test (`transferBudgetV2.integration.test.ts`). Invariants: bounded windows (byte and row limits before decode); no raw tool bodies on snapshots (fetch on demand); **live command output bypasses the event store** via `CommandOutputHub` and clients subscribe only while a command row is expanded ("never route output deltas through turn_item.updated"); incremental shell sync (initial shell = active nav rows only, resumes send deltas).
- `ThreadLiveEventCoalescer.ts`: 50 ms window, max 512 pending, coalesce key = thread + run + event type + entity id, only for running streaming items (assistant/reasoning/command/file/search/dynamic tool); survivor keeps its original sequence so resume cursors cannot skip. `LiveStreamBudget.ts`: 1000 items / 8 MiB per subscriber, overflow fails the subscriber (resync) instead of growing. `ProviderTextDeltaCoalescer.ts` coalesces at the adapter.
- Web: `@legendapp/list` virtualised transcript, React Compiler everywhere (lint bans manual memo), `useStableCallback` for big components to avoid V8 shared-closure-scope leaks (doc 10), per-feature stores (`composerDraftStore` 4.9k lines, `threadSelectionStore`, `rightPanelStore`). `scripts/server-metrics.ts` benchmarks server CPU/wakeups/RSS against a fake Anthropic API and the real `claude` CLI, with a `working` scenario of many worktree threads editing at once and a soak mode for leaks.
- Startup phases come from real protocol signals only, never timers (`startupPhase` on provider session).

### 2.4 Delegation, messaging, permissions
- T3 injects an authenticated per-thread MCP server (`apps/server/src/mcp/`, `McpHttpServer`, `McpInvocationContext`, `McpToolAccess`) into each agent. Orchestrator tools (`toolkits/orchestrator/tools.ts`): `orchestrator_capabilities`, `delegate_task` (child gets only `task`, no parent history; `isolation:'worktree'`), `task_status` (`workState: working | waiting_for_children | result_available`; reading marks delivered), `task_cancel` (cascades to child's turns, delegates, PR watches), `t3_background_wait`, `create_threads`, `t3_thread_{list,read,update,send,wait,interrupt}`, `schedule_task`, `request_secret`, plus worktree handoff and workflow tools. Same tools are exposed to outside agents (Claude Code in your terminal, ChatGPT) over OAuth'd remote MCP with a chosen capability level (`docs/user/outside-agents.md`); `delegate_task` etc. are hidden from outside clients via `EnabledWhen`.
- Permission propagation rule (`mcp/delegatedTaskModes.ts`): child mode = requested or inherited, **never broader than the parent** (`runtime_mode_escalation_denied`), ranked approval-required < auto-accept-edits < auto < full-access; plan interaction mode likewise. If the inherited mode is unsupported by the child's provider it steps DOWN to the broadest supported one; an explicit unsupported request is refused. Admin `managed-policy` can cap modes per machine. Subagent approval requests render in the PARENT conversation labelled with the subagent name.
- Completion wake: subagent has `completionWake: always | settled_only` and `completionDelivery: pending|claimed|acknowledged|delivered|disposed` with `observedByRunId`, so a child's result is delivered exactly once to the parent's next run (`DelegatedCompletionDelivery.test.ts`, 6k lines of tests). A turn that ended with live child work is "not done" (tool description says so).
- Child is its own thread (`childThreadId`) or provider-native (`origin: provider_native | app_owned`), also remote child on another environment.
- Workflows: TypeScript scripts (Labs) chaining agents, schedules, webhooks, issue/PR events, `ask()` questions; durable, each step result saved, `t3 workflow start`. Scheduled tasks and PR watch reactors exist. No explicit task DAG object outside workflows.

### 2.5 Worktrees
- Per-thread choice (local checkout vs "New worktree"), branch name prefix settings, submodules initialised recursively, `.t3/setup-running` marker (`Wait while .t3/setup-running names a live PID`), setup script runs in a terminal and reports `exitCode/durationMs` (`ProjectSetupScriptRunner.ts`), `t3.json` actions, `worktreeCleanup`, `ResourceCleanupService`, handoff tool (`WorktreeMcpService`), fork, multi-repo projects (a worktree per repo). Threads sharing a checkout refuse branch switches while one works. Multi-model fan-out: shift-click several models, each gets thread + worktree.
- Known bugs (issues): permanent worktree left when draft never sent; forking a worktree thread reuses (and clobbers) the source checkout; Claude in T3 worktrees misses CLAUDE.md that `claude --worktree` loads; `t3_worktree_handoff` drops git's error text; bare repos rejected. No automatic port allocation found.

### 2.6 UI information architecture
- Sidebar of threads grouped by project; states are first-class and server-owned: **Pinned**, **Active** (user-ordered, drag between sections with the drop action shown as a badge), **Working** (beta fold-away), **Snoozed shelf** (needs wake time), **Settled** shelf (auto-settle by PR state), archived. Every transition shows a 5 s **Undo** toast and `mod+z`. "Reverse states: every way in needs a way out" is a stated rule. "Settle and advance" jumps to the next thread needing you.
- Per-thread permission mode shown next to model only when non-default; `mod+shift+a` switches. Tool calls collapse to summaries ("Ran 2 commands and sent messages to 3 threads"), settings Hide successful / Hide all / Zen. Usage page (`mod+u`): tokens, cache savings, estimated API-equivalent cost from provider transcripts on disk, 92-day retention. Screen reader announcement levels. Command palette and keybindings on every action.
- Issues seen (open tracker, 2026-11): provider retry loops, idle-release making first turn slow (20-60 s), Cursor plugin update blocking run.start, Windows update killing npm, i18n gaps. Lesson: most complaints are provider-process lifecycle, not UI.

## 3. Second deep dive: Emdash (c876989) plus Vibe Kanban as the normalisation reference

Why Emdash: best-documented domain model of the group (CONTEXT.md glossary of ~60 terms, 8 ADRs, `agents/architecture/*`), and the closest to Axiom's shape (desktop shell + host daemon + typed wire).

- Inventory-and-command model (ADR 0001/0005/0006): the host's inventory is authoritative, the desktop registry converges toward it, no standing desired state; mutations are plain fail-fast RPCs; only deletion intent survives unreachability as a tombstone swept by reconcile. Creation never converges.
- Scripts: `prepare` and `setup` run in the session plane (stamped workspace initialisation) so a host verb never fails from user code; `teardown` runs inside deactivate, non-fatal and time-boxed ("a broken teardown script must never make a workspace undeletable", ADR 0003, superseded in placement by 0005 but constraints kept). Workspace artifacts (git-ignored deps/build) are listed as reclaimable and cleaned in place, preserving `.env*` and user patterns.
- ACP runtime (`agents/architecture/acp-runtime.md`): `SessionManager` (directory + coordinator), `ConversationHandle` (aggregate root with epoch so stale async work is invalidated), `LifecycleCell` (coalesced starts, leases, interrupt-before-drain), stateless `SessionMaterializer`, `SessionRouter` (routes by ACP sessionId, drops unknown/stale updates), `SessionCell` (state machine, transcript reducer, permission broker, prompt queue, turn quiescence). Vocabulary worth stealing: Conversation (durable) vs Session (process) vs Prompt vs Prompt attempt, **Provider dispatch reservation**, **Provider terminal proof**, **Outcome unknown**, **Command receipt**, **Session generation**. They model "did the provider receive this?" explicitly, which T3 also does with receipts.
- Complaints (issues): workspace server reading untracked files in a loop (1.6 TB sent in 5 days, 58 GB RAM), quadratic registry sync on main thread, background `git fetch` every 2 min on large repos, remote terminal lag behind background traffic, "closing an agent tab does not kill the agent", no overview of running tasks/automations. All are watcher/poller hygiene and lifecycle clarity.

Vibe Kanban reference points (src, Apache-2.0, Rust, closest language match to the Axiom daemon):
- `crates/executors/src/logs/mod.rs`: `NormalizedEntry{timestamp, entry_type, content, metadata}`; entry types `UserMessage, UserFeedback{denied_tool}, AssistantMessage, ToolUse{tool_name, action_type, status}, SystemMessage, ErrorMessage{SetupRequired|Other}, Thinking, Loading, NextAction, TokenUsageInfo{total_tokens, model_context_window}, UserAnsweredQuestions`. `ActionType`: `FileRead, FileEdit{changes}, CommandRun{command,result,category}, Search, WebFetch, Tool{...}, TaskCreate/PlanPresentation/TodoManagement`. `ToolStatus`: `Created, Success, Failed, Denied{reason}, PendingApproval{approval_id}, TimedOut`. A small, good schema; the `category` on commands (read/search/edit/build) enables summary grouping.
- Streams: `services/events/patches.rs` sends RFC-6902 JSON patches (`/execution_processes/{id}`) over WS, so the client store is a mirror that applies patches; cheap and resumable per stream.
- `crates/executors/src/approvals.rs`: `ExecutorApprovalService` trait (`create_tool_approval` returns id immediately, `wait_tool_approval(id, cancel)` blocks) and `QuestionStatus`; Claude is driven with `--output-format=stream-json --input-format=stream-json --permission-prompt-tool=stdio` plus PreToolUse hooks; Codex via `app-server`; newer CLIs through an ACP "harness v2".
- Worktrees (`crates/worktree-manager`, `workspace-manager`, `git`): create/ensure-exists/batch cleanup/move, orphan cleanup (`cleanup_suspected_worktree`), per-repo setup, cleanup, archive and dev-server scripts (`parallel_setup_script` flag), squash-merge, rebase with conflict detection (`detect_conflict_op`, `get_conflicted_files`, `abort/continue_rebase`). Conflicts surface as UI state with abort/continue.
- Issues now: pinned old Claude CLI version rejected by models, throwaway `opencode serve` at startup, harnesses lacking session continuation. Lesson: hardcoded CLI versions rot; discover.

## 4. The remaining projects, per question

### Claude Squad (070eb93, AGPL)
- Drive: tmux session per agent; `HasUpdated()` hashes pane content each tick, `hasPrompt` by substring on known prompts; status `Running | Ready | Loading | Paused`; `AutoYes` daemon (`daemon/`) can press enter for prompts. `session/git/worktree*.go`: `git worktree add` from existing or new branch, force-remove stale, **Pause** removes the worktree but keeps the branch, **Resume** recreates it. Persistence is JSON.
- Complaints: prompt lost because text is sent before the CLI is ready (open issue "SendPrompt sends text before CLI is ready"); no worktree setup hook (deps, env files, port isolation: requested, open); Windows unsupported (creack/pty); focus mode; compact list. Takeaway: scraping needs readiness probes and is the reason people want hooks.

### Crystal / Nimbalyst (1e18e0b, MIT)
- Services per concern (`sessionManager`, `worktreeManager` with a mutex `withLock`, `panelManager` with Claude/Codex/log/terminal panels, `executionTracker`, `gitStatusManager`, `mcpPermissionServer`). Permission bridge = a local MCP server that Claude calls for `--permission-prompt-tool`, forwarded over IPC to the UI. `CRYSTAL_DIR` env isolates a dev instance. Issues: "Full Access mode switches back to Workspace, tasks hanging", project looks abandoned. Product moved to Nimbalyst (closed); lesson: open core with a commercial successor leaves forks stranded.

### Paradev / Superset (627d606, MIT; renamed from Superset)
- Terminals are the primary surface; `packages/pty-daemon` keeps PTYs alive across app restarts (session store, trust probe, process-tree utilities); `host-service` has `daemon-loss-sweep`, `terminal-agents` (attribution tokens tie a running process to an agent binding), `workspaces` (title jobs, tags, activity). `packages/agent-setup` generates per-agent wrapper scripts and managed hook files for ~20 CLIs (Claude, Codex, opencode plugin `superset-notify.js`, Cursor, Gemini, Droid...) that call a notify hook, plus a self-recursion guard test. `packages/port-scanner`: polls the process table every 2.5 s (30 s when idle), maps listening ports to terminal process trees, plus regex hints (`listening on`, `Local:`) to scan sooner; not port allocation. `packages/mcp` + `packages/cli`: agents can create workspaces, run terminals, manage tasks, organisation, automations, pages ("agents can drive the entire app and orchestrate other agents"). `packages/panes`: a pane layout engine. Cloud relay, mobile and web check-in.
- Takeaway: hooks plus process-tree attribution give reliable status for unmodified CLIs; MCP+CLI surface makes the app itself orchestratable.

### Sculptor (cdd6dc6, MIT)
- `agent_docs/terminal-agents/architecture.md`: third agent kind = terminal agent; Sculptor never parses its output; the program reports `busy | idle | waiting`, file-change and session-id to a local HTTP signal API (Claude hooks installed from a bundled TOML + hooks JSON that the user may edit/delete, sentinel prevents resurrection). Status feeds the same tab dots as native agents. Per-agent type chosen at "+".
- CI babysitter service: auto-spawns an agent to fix a failed pipeline or merge conflict, and the spec (SCU-1504) discusses preserving the user's agent type; only agents registered with `accepts_automated_prompts` may be fed prompts (a bare shell cannot).
- Named Layouts (`agent_docs/layouts/design.md`): per-workspace `WorkspaceLayoutState{placement, order, activePanel, expanded, splits, activeSubSection}` plus global sizes; design for saveable/switchable layouts with a Cmd+K dialog. Mobile docs: agent switcher, changes pill.

### opencode (678621a, MIT)
- Packages: `core`, `server` (HTTP + SSE), `protocol` (typed API groups: session, message, permission, pty, persistent-pty, worktree, vcs, job, usage, event...), `app` (Solid, shared by web and desktop), `desktop` (Electron, state machines for lifecycle), `tui`, `process-daemon`. `server/src/event-feed.ts`: SSE with per-subscriber `SubscriberCapacity = 4096` and an explicit `SubscriberOverflow` error (drop the slow consumer, force resync). Sessions have `parentID` (subagents are child sessions), `session/background.ts` tracks background executions and claims, `affinity.ts` groups a session tree. App has fork/handoff/revert as explicit state machines (`fork-machine.ts`, `handoff-machine.ts`, `revert-machine.ts`). Desktop AGENTS.md mandates one union + pure `transition` + driver for lifecycles and SQLite for desktop persistence. Usage and spinners: "dot-matrix Spinner only for AI activity, every other loading state uses Loader" (a good IA rule).

### Zed (2b248e6; GPL-3.0 agent crates)
- `acp_thread`: `AgentThreadEntry{UserMessage, AssistantMessage, SessionMessage, ToolCall, Elicitation, ContextCompaction}`; `ToolCallStatus{Pending, WaitingForConfirmation, InProgress, Paused, Completed, Failed, Rejected, Canceled}`; `subagents.rs` `SubagentAssociation{title, description, can_cancel, cwd, additional_directories}` (child inherits parent's cwd unless the parent reports otherwise; subagent state lives on the subagent's own thread); `message_queue.rs`, `elicitation.rs`, `compaction.rs`.
- `agent_ui`: `thread_metadata_store.rs` (ThreadId, optional ACP session id: a thread is a draft until the first message; `title_override` so user rename beats agent title updates; `interacted_at` distinct from `updated_at`), `thread_worktree_archive.rs` (archiving a thread persists the worktree's git state then deletes it from disk; restore recreates it), `threads_archive_view.rs`, `conversation_view/{subagent_card,message_queue,thread_search_bar}`. Transcript uses GPUI `ListState` (virtualised). Docs: threads sidebar grouped by project with status and agent icon, ctrl-tab to switch, archive via shift-backspace, running threads can't be archived, Terminal Threads (any CLI as a managed thread) in the same list, `create_worktree` task hook, new worktrees start detached HEAD.
- ACP v2 draft (docs, agentclientprotocol.com/rfds/v2/overview): `session/prompt` returns the inserted `messageId` immediately; activity reported via `state_update` (`running | idle | requires_action | unknown`) with stop reason on idle; prompts sent during work are inserted or held in order with optional `idempotencyKey`; tool calls upsert by `toolCallId` and can be `paused`/background with opt-in `tool_call/cancel`; `subagent_update` + `session_message(_chunk)` announce child sessions; permission requests require a title and structured subject; elicitation; `replay_complete`; `session/resume` replaces load; batches removed. This is the most Axiom-relevant spec: it makes the normalised model above a protocol rather than an invention.

### Closed products (docs/web only)
- **Claude Code agent teams** (docs, experimental, env flag): lead + teammates, each a full session with own context; shared task list (`pending/in_progress/completed`, `blocks/blockedBy`, tasks claimed under file lock, lowest id first, dependents unblock automatically); mailbox = JSON inbox file per agent (`~/.claude/teams/{team}/inboxes/{agent}.json`), polled and injected as synthetic user turns; message types incl. `task_assignment, broadcast, shutdown_request/response, plan_approval_request/response, idle_notification` (web reverse engineering, v2.1.47). Idle is normal and notifies the lead with the final answer. Hooks `TeammateIdle`, `TaskCreated`, `TaskCompleted` can block with exit code 2 as quality gates. Permissions: teammates start in the lead's mode (dontAsk becomes Manual), no per-teammate mode at spawn; teammate prompts bubble to the lead; messages between agents are labelled as not from the user, so a teammate cannot grant consent for another; auto-mode classifier reviews SendMessage. Limits: no nested teams, no resume of in-process teammates, task status can lag, same-file edits collide. UI: agent panel under the prompt (up/down, Enter to open transcript, Esc interrupts, `x` stops, Ctrl+T task list), idle rows hide after 30 s and collapse beyond three into "N idle agents".
- **Codex app** (web/docs): threads Local/Worktree/Cloud, built-in worktrees, review comments on diff, automations (schedule + skills) whose results land in a **review queue**, run in dedicated background worktrees. (Details of cleanup/limits not verified.)
- **Cursor 3 Agents Window** (changelog, forum): many agents across local/worktree/cloud/SSH, queue a follow-up while running (3.2), multi-root workspaces; at launch worktrees were weak in the Agents Window and users called its approach PR-oriented; cost multiplies with parallelism (anecdotal).
- **Antigravity Agent Manager** (Google blog, web): a manager surface separate from the editor; **inbox** of conversations and requests needing intervention (e.g. terminal-command approval), agents side by side with status, **artifacts** (task lists, plans, screenshots, browser recordings) as the progress deliverable, projects spanning folders with scoped permissions.
- **Conductor** (docs): workspace per task (worktree created immediately, branch named from prompt), setup/run/archive scripts from `conductor.json` or `.conductor/settings.toml`, `CONDUCTOR_PORT` injected so workspaces run side by side, scripts can be set non-concurrent for fixed ports/shared DB/Docker, diff viewer with comments, Cloud option.
- Newer significant ones seen via search (web, unverified depth): Orca (MIT, supervised tasks, workers, decision gates), Agent Orchestrator (Apache-2.0, PR/CI kanban, 27 agents), Helmor, Parallel Code, Berd (Goose backend), Intent by Augment, Agentastic.

## 5. Cross-cutting findings by requested topic

### Worktree lifecycle (what everyone converges on)
1. Create: `git worktree add -b <branch> <path> <base>`; branch name from prompt (Conductor) or generated (T3 prefix settings); submodules (T3).
2. Prepare: setup script after creation in a terminal/session plane, not inside the create verb (Emdash ADR 0003/0005, T3 setup marker, Conductor, VK `parallel_setup_script`); deps via copy of ignored files with preserve patterns (Emdash).
3. Run: dev servers per workspace. Port strategies: inject env var (`CONDUCTOR_PORT`), or detect after the fact by process-tree scan (Paradev), or nothing (T3, Claude Squad: open request). Nobody allocates and reserves a port block with conflict detection.
4. Review and merge: diff with inline comments sent back to the agent (VK, Conductor, T3); squash/rebase with conflict state and abort/continue (VK); PR-based merge (T3, Cursor, Paradev).
5. Pause/archive: remove worktree, keep branch or save git state, restore on demand (Claude Squad pause, Zed archive); teardown script non-fatal and time-boxed (Emdash).
6. Cleanup failure modes seen: orphaned worktrees for unsent drafts (T3), forks sharing a checkout (T3), stale dirs blocking `worktree add` (Claude Squad force-remove), reclaimable artifacts accumulating (Emdash).

### Delegation and inter-agent messaging
Three patterns: (a) MCP tool server injected into each agent (T3, Paradev, VK mcp crate, opencode); (b) protocol-level subagents (ACP v2 `subagent_update`, Zed); (c) file mailboxes (Claude teams). Pattern (a) is the only one that works across harnesses today and lets outside agents drive the app. Delivery semantics matter: T3's `pending|claimed|acknowledged|delivered|disposed` and "do not poll a running task, completion wakes you" avoid busy-wait loops that burn tokens. Task graphs: only Claude teams (blockedBy DAG) and T3 workflows (scripted stages) exist; VK has a human kanban, not an executable graph.

### Permission propagation summary
T3: child never broader than parent, step-down on unsupported, parent conversation hosts child approvals, admin cap. Claude teams: child starts at lead's mode, prompts bubble to lead, inter-agent messages cannot carry consent. Cursor/Codex: unverified. Nobody exposes a per-project "what can this agent touch" matrix with a revocable grant list across harnesses.

### Usage display
T3: dedicated page from on-disk transcripts, API-equivalent cost estimates, cache savings, 92-day retention. VK: `TokenUsageInfo{total, context_window}` per entry (context meter). Claude teams: cost scales per teammate (docs). opencode: `usage` and `usage-limits` modules. Gap: no live per-agent burn rate or budget stop across agents.

### Many concurrent streaming agents
Best-in-class is T3's stack (section 2.3). Others: VK JSON patch streams; opencode bounded SSE with overflow drop; Claude Squad ticks every N ms and hashes panes (O(agents) subprocess calls, issue "update git stats outside main goroutine"); Emdash/Paradev suffer watcher and poller cost. Common failure: global stores re-rendering the whole list on every token (T3 fights with per-entity keys, windows, and expanded-only subscriptions).

### Information architecture, what is primary
- Thread/workspace list as the spine (all of them). Selected agent's transcript centre. Diff/changes, terminal, browser/preview docked around it (Paradev, Sculptor panels, VK workspace). Kanban only in VK (and Paradev tasks) and it is what VK users say they lost ("which version has a workable kanban board?").
- State vocabulary is richer in T3 (pinned/active/working/snoozed/settled/archived) than anywhere else, and the review queue (Codex) and inbox (Antigravity) are the two attempts at "things that need me".

### What users complain about (aggregated)
1. Lost or garbled prompts to PTY agents (Claude Squad).
2. No setup/env/port story for worktrees (Claude Squad, Cursor forum).
3. Orphaned or shared worktrees and branch confusion (T3 issues).
4. Poller/watcher CPU, memory, bandwidth (Emdash 58 GB; Claude Squad ticks).
5. Hardcoded or stale agent CLI versions (VK).
6. Cost multiplier from parallel agents (Cursor, Claude teams docs).
7. Stuck permission modes and hanging tasks (Crystal "Full Access switches back").
8. Project abandonment or business failure (VK, Crystal). Local-first with no cloud dependency survives it (VK's local workspaces kept working).
9. Windows gaps (Claude Squad, Paradev).

## 6. Information architecture lessons from creative and professional tools

Mechanisms only. Mapping targets: A = agents, T = tasks, U = tool activity, P = permissions.

| Tool | Mechanism | What it solves | Maps to |
|---|---|---|---|
| FL Studio | **Channel rack**: one compact row per sound source with mute/solo, a mini-preview, a number routing it to a mixer insert; double-click opens the full editor (piano roll). Rows are uniform and dense; detail is a different window. | Scan 30 sources at once, act without opening | A: one row per agent with state glyph, mute (pause), solo (focus), a route number = its worktree/project |
| FL Studio | **Mixer inserts** with peak meter, routing, send levels | per-channel activity as a meter, not a transcript | U: a live activity meter per agent (events/s, tokens/s) |
| FL Studio | **Playlist** (arrangement) with pattern clips on tracks and a time ruler | what runs when | T: timeline of runs per agent (see below) |
| Ableton Live | **Session view**: grid of clips; a column = a track, a row = a **scene**; launching a scene fires all clips in the row; **launch quantization** delays a trigger to the next bar; per-clip play state with a progress ring | start many things in sync, safely, reversible, state visible per cell | T/A: grid of (agent x task) cells; a scene = a batch ("run all reviewers"); quantization = "start when the previous stage settles" or "when a slot frees" (concurrency cap) |
| Ableton Live | **Arrangement view** records what you did in session; **Follow Actions** chain clips (next, random, stop) automatically | automate sequencing; record then edit | T: pipelines (implement -> review -> fix) as follow actions on a run, editable afterwards |
| Ableton Live | **Detail view** docked at the bottom, content follows the selected clip/device (Clip / Device chain tabs) and is collapsible; **Info View** (hover text in a fixed corner, no tooltips) | one inspector instead of per-object windows; help without covering UI | A/T: one docked inspector that follows selection; U: hint bar (below) |
| Ableton Live | **Cue/solo** monitoring bus separate from master | listen to one without disturbing the mix | A: "peek" at an agent's stream without marking it read or stealing focus |
| DaVinci Resolve | **Pages** (Media, Cut, Edit, Fusion, Color, Fairlight, Deliver) along a bottom bar: same project, task-specific layouts, one primary job per page; **Deliver** page is a queue with job status | separate "gather", "work", "review", "ship" without separate apps | A: pages: Triage (inbox) / Work (one agent) / Review (diffs) / Deliver (merge queue) |
| DaVinci Resolve | **Node graph** (Color/Fusion): processing as a DAG where each node is inspectable and bypassable; **timeline thumbnails**/clip versions; **Render queue** with per-job progress | make a pipeline visible and per-stage controllable | T: task graph where each node is an agent step with its own status, bypass (skip), and retry; the merge/test gate is a node |
| Reaper | **Track control panel + mixer + TCP/MCP**: same track visible in arrange and mixer; **action list** (every command searchable, bindable, scriptable); **Custom actions** and **Screensets** (save/recall whole window layouts, keyed) | everything is a named command; layouts are first class | all: command registry is the spine; named layouts (Sculptor's SCU-1725 is the same idea) |
| Reaper | **Routing matrix** and **folders** (nesting tracks, collapse, group-solo/mute) | group control | A: agent groups per project/epic with group mute, group interrupt, group budget |
| Reaper | **Track meters in the track header at all zoom levels; track height presets** (small/medium/large) | density control | A/U: density levels (1 line per agent -> full transcript) switchable globally |
| Linear | **Inbox** with triage actions (accept/snooze/decline) and keyboard-first navigation; **Views** as saved filters; **issue statuses grouped into categories** (backlog/started/done) so workflows can differ but rollups stay uniform; **Cycles**; `C` to create, `Cmd+K` everywhere | things needing me as a queue with one-key resolution; custom states roll up | P/A: one **needs-you queue** (approvals, questions, failed, finished-to-review) with accept/deny/snooze, state categories for any harness's states |
| Raycast | **Command palette as the whole app**: fuzzy, frecency-ranked, aliases/hotkeys per command, **Action panel** (`Cmd+K` on the selected item shows context actions), **Quicklinks**, argument chips inline | everything reachable by typing; context actions follow selection | P: approve/deny/always-allow as actions on the selected request; U: actions on a tool-call row |
| Zed | **Threads sidebar** with status icon and agent icon, `ctrl-tab` switch, archive with a key, history view, terminal threads in the same list; **multi-buffer** (one editor over excerpts from many files) | many sessions in one list regardless of backend | A: unified list across structured and PTY agents; multi-buffer = one review surface over all agents' changes |
| Arc | **Spaces** (context groups) with pinned vs today tabs and auto-archive after N hours; **Split view**; **Peek** (preview a link in an overlay without opening a tab); **Little Arc** | auto-tidy of stale items, ephemeral inspection | A: "Today" agents auto-archive on settle; Peek for a transcript or diff overlay |
| DAWs generally | **Transport bar**: global play/stop/record/tempo/loop, always visible, one-key | the global state and global controls | A: global bar: N working / M waiting for you, **pause all**, **interrupt all**, spend so far, concurrency cap |
| DAWs/Resolve | **Hint bar / status line** that describes the hovered control; Ableton Info View, Resolve tooltips, Reaper action description in the actions list | learnability without tooltip clutter | U/P: fixed hint strip explaining the selected tool call or permission in plain words ("will delete 14 files in worktree X, reversible via checkpoint") |
| DAWs | **Undo history** as a first-class panel, **cue/arm** states (record-armed track) | explicit "armed" vs "active" | P: "armed" agents (allowed to run unattended) vs "monitor" (ask me) as a visible per-agent switch |
| Hardware sequencers | **Pattern chain/follow** and **mute groups** | | T/A |

### Which mechanisms to adopt, and why
1. **Channel-rack rows plus transport bar plus needs-you queue** answer "which of 12 agents needs me" without opening any. This is the primary surface; transcripts are secondary (Resolve pages: Triage / Work / Review / Deliver).
2. **Scenes and launch quantization** map onto the real constraint (rate limits, CPU, cost): a concurrency cap and "start when the previous stage settles" are better than N agents all starting. Show the queue explicitly; Cursor 3.2 and T3 queue messages but not agents.
3. **Docked inspector following selection** (Ableton detail view) replaces per-agent windows; Zed and T3 have the right panel but it is per feature, not one inspector.
4. **Node graph for tasks** only when work has real dependencies (Claude teams `blockedBy`, T3 workflows); otherwise a flat list. Bypass and retry per node, like Resolve.
5. **Command registry** (Reaper action list, Raycast) with context actions on the selected row; T3 already requires every feature be reachable by palette, keybinding and MCP tool, which is the same spine.
6. **Hint bar** for permissions: it fixes the weakest part of every product here (approval text is the raw tool call).
7. **Screensets**: named layouts per workspace (Sculptor design) rather than one fixed arrangement.
8. **Peek** and **cue**: look without changing read state or stealing focus.

## 7. Adopt

1. Server-owned domain state with events + projections + outbox in one transaction; clients render persisted state (T3 `orchestration-v2`). For Axiom this belongs in the engine host; define Thread / Run / RunAttempt / Node tree / TurnItem / RuntimeRequest as in 2.2.
2. Capabilities, not provider names: adapters declare what they support; degrade, never emulate permission modes (T3 doc 03). Match with ACP v2 capability objects.
3. Wire budgets as CI tests; live command output on a side channel subscribed only while expanded; coalesce streaming updates with a 50 ms window keyed by entity; per-subscriber byte/count budget that fails the subscriber into resync (T3, opencode SSE overflow).
4. `RuntimeRequest.responseCapability: live | message | not_resumable` for approvals so restarts are honest.
5. Child permission never broader than parent, step-down on unsupported, child approvals shown in the parent with the child's name; explicit admin cap (T3, Claude teams).
6. Exactly-once completion delivery state machine for delegated results; "do not poll, completion wakes you" in tool text.
7. Hooks-plus-process-attribution for PTY agents; local signal API with `busy|idle|waiting` (Sculptor, Paradev). Ship the hook installers as user-editable files with an uninstall sentinel (Sculptor).
8. Setup/teardown scripts outside the create verb, teardown non-fatal and time-boxed, artifacts reclaim preserving `.env*` (Emdash).
9. Thread states with server-owned settle/snooze/archive, 5 s Undo on every transition, "reverse states" rule (T3).
10. Zed's draft-until-first-message thread, `title_override`, `interacted_at` vs `updated_at`, archive that saves git state and removes the worktree.
11. Stateless materialiser + epoch on the conversation handle to kill stale async work (Emdash).
12. Replay fixtures recorded from real providers through the real orchestrator (T3 doc 03) as the adapter test strategy.
13. Fresh-instance data dir isolation env var for self-hosting dev (Crystal `CRYSTAL_DIR`, T3 "never touch live userdata").

## 8. Avoid

- PTY scraping as the primary status source (Claude Squad: lost prompts, hash polling cost). Use only as the fallback with a readiness probe.
- Hardcoded harness versions (VK). Discover and capability-probe; passive health checks only, no sessions that start MCP or hooks (T3 rule).
- A cloud dependency for core function (VK, Crystal fates). Keep local-first; any relay optional.
- Unbounded watchers and pollers (Emdash 1.6 TB, 2-minute background fetch). Budget every poller, back off when idle (Paradev 2.5 s -> 30 s idle is a good pattern).
- One shared checkout between forks/threads (T3 bug), orphan worktrees for unsent drafts. Worktree created lazily at first send; fork gets its own.
- A global event stream that carries tool output bodies.
- Letting the child inherit a broader mode, or consent claims relayed between agents (Claude docs rule is correct).
- Making kanban the primary metaphor: VK needed it for planning, but supervising running agents needs a queue and rows, not columns. Keep a board as an optional view over the same state.
- Copying GPL/AGPL code (Zed agent crates, Claude Squad).
- Copying visuals: out of scope by request.

## 9. Where Axiom can be materially better

1. **Local, native, Flutter**: none of the studied apps is native. Electron memory under many streams is the common pain (T3 issues, Emdash 58 GB). A Dart shell with virtualised lists and an isolate per heavy view, driven by a Rust daemon that owns coalescing, can hold dozens of agents with flat memory; make that a published benchmark like T3's `server-metrics` (plus a UI frame-time budget).
2. **Needs-you queue as the primary surface** (Linear/Antigravity inbox + Codex review queue), unified across approvals, questions, failures, and "finished, review it", with one-key accept/deny/snooze and "settle and advance". Everyone has pieces; nobody makes it the home screen.
3. **Permissions as a visible matrix** across harnesses: per agent, per project, per tool class, with inheritance shown (child <= parent), armed vs monitored per agent, a plain-language hint line, checkpoint-backed reversibility shown on destructive prompts, and a grant ledger. T3's modes are per-thread enums enforced by each provider; nobody shows effective permission across a tree.
4. **Port and resource broker**: allocate a port block per worktree with collision detection, inject env vars, tie to process trees, release on teardown; show it in the row. Conductor injects one var, Paradev detects, others ignore.
5. **Concurrency governor**: global cap, per-harness rate-limit awareness, queued-start state ("scenes with quantization"), spend/burn meter, and a hard budget stop. Cost multiplication is the top user concern and nobody offers a budget gate.
6. **Honest adapter ladder**: structured (ACP/app-server/SDK) -> hooks+PTY -> raw PTY, each showing which features are live for that agent (capability badges), so PTY agents are first-class in the same list (Zed Terminal Threads, Sculptor) instead of second class.
7. **Task graph with real gates**: executable DAG nodes (agent step, test gate, review gate, merge) with bypass/retry (Resolve nodes), on top of delegation tools, using ACP v2 subagent updates. Only Claude teams and T3 scripted workflows come close, and neither is visual or inspectable per node.
8. **Verification as a node type**: an objective gate (build, test, review by a different model) inside the task graph; an agent's "done" is a claim until the gate passes.
9. **Replay and audit**: the event log as an auditable timeline per agent with checkpoint restore that reconciles filesystem and provider conversation (T3 refuses rollback if the provider cannot; copy that rule).
10. **Remote-ready without Electron**: the daemon is already a separate process; a thin relay for mobile check-in can come later without redesign.

## 10. Reusable components (licence-compatible, for ideas or direct use)

- ACP: `agent-client-protocol` Rust crate and JSON schemas (Apache-2.0, cited via Zed's Cargo usage; verify licence file before depending).
- VK `crates/executors` (Apache-2.0): per-CLI log normalisers, `NormalizedEntry`/`ActionType`/`ToolStatus` shapes, approval service trait, Claude stream-json control wiring, ACP harness. Best direct reference for the daemon's adapter crate; read before writing, port with attribution, do not vendor wholesale (it tracks CLI versions that rot).
- VK `crates/git`, `worktree-manager`: rebase/conflict detection, orphan cleanup patterns (Apache-2.0).
- T3 (MIT): contracts shapes, `delegatedTaskModes.ts` rank logic, coalescer algorithm, budget algorithm, `ProjectSetupScriptRunner` sentinel-exit pattern, model manifest with remote update and `updatedAt` precedence, replay-fixture testkits, `server-metrics` methodology.
- Paradev (MIT): `port-scanner` (procfs/lsof process-tree to listening ports, hint patterns), `pty-daemon` design (PTYs outliving UI), `agent-setup` wrapper/hook generators and the self-recursion guard test.
- Sculptor (MIT): terminal-agent registration TOML + hooks JSON format, local HTTP signal API, layout state schema.
- Emdash (Apache-2.0): ADRs and glossary as a template for Axiom's `docs/decisions`, session lifecycle chassis concepts.
- Dart side: no mature ACP client found (not searched exhaustively).

## 11. Unverified

- Codex app: worktree cleanup, limits, snapshot behaviour; `developers.openai.com/codex/app/features` redirected to a page without these details. Only the search-result summary was used.
- Cursor 3 internals (how many agents, worktree lifecycle details), Antigravity docs (official docs page 404 on my guess; used the Google blog summary via search), Conductor checkpoints and its internals (closed).
- Claude teams mailbox schema comes from a third-party reverse engineering of v2.1.47, may have changed; docs confirm JSON inbox files, locking and message types in prose.
- Superset renamed to Paradev: repo clone was `superset-sh/superset`; README and licence say Paradev/MIT. Other aggregators disagree on licence (search result said "relicensing"); the LICENSE.md in the clone is MIT, Copyright Superset Inc.
- Zed: stable version and dates from web pages; ACP v2 details from the RFD overview page, which says it is a Preview and not frozen. The licence of the Rust `agent-client-protocol` crate was not checked.
- Star counts, user counts ("500,000 users") are self-reported in AGENTS.md or search snippets.
- I did not read: T3 `ProviderSessionManager`/`RunExecutionService` internals, VK `container.rs` flow end to end, Superset chat-v3, Sculptor task_service, opencode `session/` loop and `plugin/provider/claude-agent`. Statements about them are from directory names, doc files and headers only.
- Port allocation: I found no allocator in T3, VK, Emdash or Claude Squad by grep of names and docs; a deeper search could find one.
- Windows support per project not verified except from README/issues (Claude Squad open Windows issue; Paradev README "Windows: not yet available").
- Creative-tool section is from my knowledge of FL Studio, Ableton Live, DaVinci Resolve, Reaper, Linear, Raycast, Zed and Arc, not from fetched sources; check exact behaviours (e.g. Ableton launch quantization defaults, Resolve page list in the current version) before citing in a design doc.
- The T3 and Superset clones were deleted by something during the session; T3 was re-cloned (same SHA). Superset was not re-cloned, claims about it were captured before deletion.
