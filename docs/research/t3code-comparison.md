# T3 Code compared with Axiom

Read 2026-11-25 by an agent, source and documentation only: `pingdotgg/t3code` at `7dc7b29` (MIT, © T3 Tools Inc.; marks excluded). Nothing was run. Not read: its `Orchestrator.ts`, its Codex adapter, most of `ClaudeAdapterV2.ts`. Issues are their reporters' claims. Adds to `orchestration.md` §2; ideas are taken, no code. Paths are under `apps/server/src/` unless they begin otherwise; `o2/` is `orchestration-v2/`.

## What it does better, most worth doing first

| # | For Axiom | Where it is there | Beware | Size | Where it stands here |
|---|-----------|-------------------|--------|------|----------------------|
| 1 | What needs a person is more than questions: failed, at a plan's limit, finished and not seen, changes to bring in; a kept mark of what was seen; a badge and a notification with names only | `packages/client-runtime/src/state/threadInbox.ts`, `apps/web/src/components/ThreadNotificationCoordinator.logic.ts` | it announces "finished" between a child's end and its parent's waking (#31087): wait 2 s; what failed is never hidden by itself (#27052) | M | built, but for changes to bring in, the badge and the notification |
| 2 | What an agent was told to do is delivered by itself, once, from the journal | `mcp/toolkits/orchestrator/tools.ts`, `o2/QueuedRunOrder.ts` | hold it while the parent has an open question, is stopped or limited (#41049, #41583); two ends, both delivered (#39204); never let the parent answer a child's question (SI-2) | M | built; labels beyond the word of who says it (SI-17) are not |
| 3 | Comments on lines of a diff go into the agent's draft, with the digest | `apps/web/src/reviewCommentContext.ts` | one being written outlives a refresh (#42215) | S | built: added to the draft, of a line or a file, with the copy's digest; not of several lines yet |
| 4 | A denial can say why; an agent can ask something that is no approval | `permissionResultFromDecision` in `o2/Adapters/ClaudeAdapterV2.ts` | what is said meanwhile must not cancel the question (#26841) | S, M | why not: built; questions of an agent: to do |
| 5 | What was said and not heard is shown, and can be changed or taken back | `apps/web/src/components/chat/QueuedRunsControl.tsx` | the host holds one that is being edited (#15611); access and model as they were (#31290) | M | kept, not shown |
| 6 | Limits are a state with their reset; "continue at reset" by a person's press | `o2/UsageLimitRecoveryWorker.ts` | try a few times after the reset (#33708); one model's window is not another's (#32015). **Not** `provider/claudeResetCredits.ts`, which reads Claude's token (0007) | M | a notice at 90 % |
| 7 | What each run changed, and taking that back, in copies only | `o2/CheckpointRestoreSafety.ts` | nothing in the person's `.git` (#38102: 96 GB; #26227) | L | the last that was brought in |
| 8 | Continue after a crash: a key first, by itself later | `o2/RestartContinuation.ts` | never after Stop or with an open question; bounded (#41870) | M | waits until spoken to |
| 9 | A chosen part of what was said goes along where resuming is impossible, shown first | `o2/ContextHandoffService.ts` | labels stay (SI-17) | M | begins anew |
| 10 | A setup command for a copy, approved by its exact text, contained | `project/ProjectSetupScriptRunner.ts`, #42386 | no network for commands yet (K43) | M | to do |
| 11 | Copies are cleared by a rule that says what it kept and why, off by default | `docs/user/project-settings.md` | one removed with 42 uncommitted files (#42619) | S–M | K48 |
| 12 | Sessions can be renamed, set aside, snoozed, with undo; drafts outlive the window | `o2/ThreadSettlementService.ts` | | M | to do |
| 13 | Search over titles and what was said | `o2/ThreadSearch.ts` (SQLite FTS5) | SI-21 | M | to do |
| 14 | Long pastes, `@files`, images | `o2/AttachmentClaims.ts` | by content, outside the folder (#26591: 1.4 GB in a week) | L | to do |
| 15 | Effort can be chosen; the computer stays awake while something runs, if its person says so | `background/KeepAwake.ts` | | S | effort: built; awake: to do |

Later, with what it belongs to: ports beside terminals (M4); one prompt to several models, each in its copy; pull requests and schedules (the owner's decision). Its own rewind and fork break threads (#17423, #34753).

Most of its heavy complaints are about life cycles and disk: workers that outlive turns, races between steering and queueing, unbounded checkpoints, git asked again and again (#42557).

## What Axiom does better, and keeps
File-by-file bringing in, checked against what was shown; agents never write `.git`; asking first by default; a person answers, not the parent agent; whole text of a question and its digest; widening by delegation is asked about; no credential of another program is read; one spawn path, no relay, no telemetry; Claude Code with Axiom's tools only; prompts heard once, kept over a stop; provenance of figures; nothing of the content in windows' titles or badges.
