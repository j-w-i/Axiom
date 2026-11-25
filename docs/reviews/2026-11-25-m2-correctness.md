# Review of M2's engine, host and client (correctness)

> Independent, adversarial; of 407f3f5. Filed once its fixes were in, otherwise as the reviewer wrote it; machine paths are left out, and so is the list of mutants. **It is not a security review**: that one has not been done (known issues K32).
>
> | Finding | Now |
> |---------|-----|
> | 1 a prompt lost | fixed: kept until it is heard (table `pending`), also over a stop and a restart; `host/tests/agents.rs` |
> | 2 a killed host's commands | fixed: a process of its own ends them (`spawn/reaper.rs`) |
> | 3 strangers taken into the tree | fixed, `spawn/tree.rs` `who_has_a_number_after_one_of_ours_lives`; on Linux /proc is read once for a look |
> | 4 the same request 200 times | fixed: told in words, three tries |
> | 5 one slow call holds up the rest; said twice | fixed: slow calls run by themselves, prompts have names |
> | 6 a conversation no provider takes | fixed: what is sent is a view that is always valid, reloaded between runs |
> | 7 a closed session's zombie run | fixed |
> | 8 a double click | fixed: what has just come is not pressed |
> | 9 a command without an end | fixed |
> | 10 a full window; endless tool text | fixed: compaction, 64 KiB |
> | 11 a person's words lost in the UI | fixed: drafts outlive the view |
> | 12, 13, 14, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26 | fixed, each with a test |
> | 15, 29 line ends and context lines | fixed; an ambiguous hunk still takes the first place |
> | 16 another agent on each retry | fixed |
> | 27, 28 folders and files left | fixed: folders go again, eight outputs are kept |
> | 30 | `null` and `1.0` are taken. A file that is not text, and a line past its cut, are the shell's |
> | 31 | fixed: planning is not waited for |
> | 32 | fixed at the smallest size a window can have, at twice the text |
> | read-only | all but these: text queued together has no limit of its own; AgentSet clears the whole session's grants |
> | mutants | those the reviewer marked have tests. Not killed: the order in which a moved file is written and removed (undo hides it), `file-write-setugid` (denied without it too) |

# Review of 407f3f5 (macOS 27 arm64; nothing run on Linux/Windows, LM Studio not used, no fixes made)

Reproductions are uncommitted in the worktree the reviewer's worktree. They print what happens rather than assert:

U* findings run against the test fake `Asked`, not the real host.

## Reproduced

### P0

**1. A prompt that was acknowledged is lost** — host/src/agents.rs:516, 454 (queue), 520–530; server.rs `prompt`.
"Queued" lives only in an mpsc channel. `work` throws the whole inbox away when the run was cancelled, when `next()` fails, or when `begin` fails. Overview says a prompt is on the drive before it is answered and "is never lost".
- H1: Cancel, then Prompt while the run is still ending → `Ok({"queued":true})`; prompts in the journal `["go"]`, messages containing it: 0, model asked once. The comment says "what they said before that", but this was said after Stop. The window is the whole stop (killing a command, a blocking `plan`).
- H2: Prompt while it works → `queued:true`; orderly `Shutdown{force}`, restart → `["go"]`.
- R17: the same at engine level.
- The client clears the composer on success, so no copy exists.
- `it_is_spoken_to_and_stopped_while_it_works` enshrines the loss.

Fix: journal the prompt (item + pending row) before answering Queued. Take it out of the pending row when it goes into the conversation. After a stop keep it pending, or begin a run with it.

**2. kill -9 of the host does not end its commands on macOS** — spawn/src/tree.rs; nothing in spawn watches the parent on macOS (only bwrap's `--die-with-parent`).
- H15: agent runs `touch begun; sleep 4; touch late`; host killed after `begun` → `late=true`.
- overview "Host dies": process trees "are killed by … parent-death watch". False here. The next host journals "interrupted" and lets the agent go on while the old command still writes.

Fix: a supervisor, or a watcher on the mark pipe, that kills the group when the host's end closes. Also record the pgid and start time and kill at recovery.

### P1

**3. `Tree::look` takes strangers into the tree, and `kill` then SIGKILLs them** — tree.rs:91-103.
`look` lists the children of every known pid without checking that the pid is still the same process. After pid reuse, the stranger's children get valid start times and pass `others()`. Unit reproduction (since reverted): known = {own pid, wrong start time}, a sibling `sleep 30`:
```
would be ended before look: []
would be ended after look: [74493] (an unrelated process, 74493)
```
Nothing is pruned from `known` either. The scenario in production needs pid wrap during one command; I reproduced the mechanism, not a wrap.

Fix: `if sys::started(p) != Some(at) { forget it; continue }` before listing children.

**4. A call that cannot be run makes the run repeat the identical request up to 200 times** — run.rs:173-180, with codec `Unfinished` (model/src/codec/mod.rs), which drops the call and its result.
- R1 (arguments not JSON): `requests=200 first==last messages: true roles ["system","user"]`, 401 messages kept, "the run has taken 200 steps".
- R2 (finish_reason `length` inside a call): the same, 200 requests, no notice, no "Go on.". On a paid model that is 200 × max_output. ContextFull/Refusal/Other with a cut call take the same path by reading: `has_calls` is tested before `stop`.
- The sentence "Not done: the arguments are not…" never reaches the model. `what_cannot_be_run_is_said_back` only inspects `context`, not the request sent.
- R3 (text + broken call): the next request ends `["system","user","assistant"]`. That is a prefill, which current Anthropic models refuse with 400.

Fix: judge `stop` before `has_calls` unless `runs_calls()`. For unfinished calls remember a user text ("your call to X could not be read: …") instead of a result the codec drops. Give up after 2–3 such steps.

**5. One slow `model.list` blocks everything else on that connection; what the client gave up on is done later** — server.rs `requests`: one request at a time, `list_models` inline (first_byte 600 s, connect 15 s). StartPage asks for models by itself.
- H3: Models against a server that stays silent, then Cancel: `Stop answered within 8 s: false`. Request.answer and prompts wait too.
- H3b: server slow by 3 s, prompt sent twice: 0 model requests after 2 s, later `["append a line","append a line"]`. The Dart client gives up after 30 s, puts the words back, and the person sends again → done twice.

Fix: run Models (and anything that waits on a network or keychain) in its own task; answers carry ids already. Give the list its own short timeout. Add a client-made prompt id so a repeat is recognised.

**6. A result that cannot be kept, plus anything queued → a conversation no provider accepts, for good** — agents.rs:520-530 reuses the in-memory `Agent`; context.rs:76-87 repairs only the last assistant message.
- H6 (journal refuses the one result row; command ran): the follow-up run sends `["system","user","assistant+calls","user"]`. After a reload it still sends `[…,"assistant+calls","user","assistant","user"]`, forever.
- R5 gives the same at every `remember`/`say` point. R18: `load` on call + user text appends the result after the text: `["User:T","Assistant:C","User:T","User:R"]`.

Fix: in `work`, reload the context before another run. `load` must put UNKNOWN results directly behind the call they belong to, for every unanswered assistant message.

**7. The session fails while a question is open → a zombie run that swallows prompts** — run.rs:617-621 waits for person or cancel only; nothing watches `Session::closed`.
- H4: question open, one journal error from elsewhere in the session, session reopened: RequestClosed(expired), RunEnded(Interrupted). Then Prompt → `queued:true` and never heard; `status.runs` = 1; AgentSet → "the agent is at work".
- The UI has no run id, so no Stop. Answering the closed question returns `Ok({})` and does nothing.

Fix: `Stage::closed()` in the select in `ask` (and in the retry sleep and `perform`). Requeue what was queued.

**8. A double click on "Allow once" approves the next question unseen** — talk.dart `_answer` + RequestBar keyed by request.
- U10, two open questions, taps 96/160/256 ms apart: `[answer q1 once],[answer q2 once]`. The same geometry applies to the next question of one agent.

Fix: a new RequestBar ignores pointer and key activation for ~500 ms, or needs a fresh hover/focus.

**9. A command can outlive its limit without end** — tools/src/shell.rs:73-96.
After the process has ended, nothing bounds the pipe reads, and `kill()` is a no-op once `done`. S1: double-fork + setsid + close(3..255), holding stdout, `seconds: 3` → "(ended after 3 s…)" after 25.08 s. With `sleep 1e6` it never ends (Stop still works).

Fix: after `ended` or timeout, drain for at most ~1 s, then drop the pipes.

**10. The window's end bricks the agent, and one call gets there** — run.rs:195; walk.rs:251-323.
- R15: provider says too long → Failed; every later prompt fails the same way and adds another user message (`["system","user","user","user"]`). No compaction, no truncation, no way out but a new session.
- R6: `search` with `around: 20` returned 2,598,366 bytes into the context (200 × 41 × 300). `read` caps at 64 KiB, shell at 48 KiB; `search`, `glob` and `list` have no byte cap.

Fix: cap every tool's text (e.g. 64 KiB, said). Refuse before sending when the last usage says it cannot fit. Until compaction exists, offer a truncated continuation.

**11. The person's words are lost in the UI**
- U1: New session, description + folder written, "Add an account or a server", Done → text gone (StartPage State disposed). That is the first-run path. shell.dart:131-139.
- U3: composer draft gone after visiting Accounts. By reading also: another agent, session fault, reopen.
- U4: prompt fails while something else was typed → first text nowhere (composer.dart:73).
- U9: leaving StartPage while it starts, prompt fails → words: 0, error shown: 0; session and agent exist, unprompted.

Fix: keep drafts in HostStore by (session, agent) and 'start'. On failure put the words before what was typed since.

### P2

**12. Refused, yet kept** — run.rs:110-122. H11: events cannot be written → Prompt returns an error; retry → model is sent "delete the old backups" twice, transcript shows it once. Fix: `sync` before `remember`.

**13. A run that cannot begin stays open** — run.rs `begin` after RunStarted. H5: prompt refused; journal has RunStarted + Running and no end; "runs started 2 ended 1". The UI shows Running with a run that cannot be stopped. K30 again, for real agents. Fix: on error say RunEnded(Failed) + state.

**14. Waiting for a person counts as run time** — run.rs:163. R9: the write was done, then "the run has taken as long as it may". An answer after lunch fails the run. Fix: subtract waiting time; check before a step.

**15. Patch rewrites every line end of a mixed file** — edit.rs:180,218. E1: `"a\r\nb\nc\nd\n"`, one line changed → `"x\r\nb\r\nc\r\nd\r\n"`. E2: `replace` of two LF lines in such a file → NotFound. Fix: keep each untouched line with its own ending; match either.

**16. Retrying Start makes another agent each time** — start.dart:184. U2: `[create, add, open, prompt, add, open, prompt, add, open, prompt]`. The view shows the first, empty "Lead" while the last one works. Fix: remember the agent id beside `_session`.

**17. Anthropic accounts always show an error on StartPage.** H10: `model.list` → "the list of models cannot be set for this provider and model", displayed verbatim. The test fake lists Anthropic models, so tests cannot see it. Fix: answer an empty list (or a catalogue) for Messages.

**18. Nobody loads the models for an open session.** U6: 0 calls; only StartPage calls `models()`. After an app restart the model menu holds only the current model. Fix: Talk asks when the popover opens.

**19. Stop that fails says nothing** — shell.dart:414 `catchError((_) {})` (U5). The same swallow on `open`.

**20. AxMenu crashes when its items change while open** — overlay.dart `late final _nodes`. U11: `RangeError … Only valid value is 0: 1`. U12: 10,000 models (the host's cap) take 9.3 s to open (debug) — not lazy.

**21. Model name typed while the list is still coming** (U7): the field disappears under the caret, the select says "none", Start stays enabled and uses the invisible text. start.dart:207,270,294.

**22. Two results for one id** — fold.rs Broken + codec Unfinished un-dropping it. R4: two `role:tool` messages with `tool_call_id` c1 behind one call; strict servers refuse it. Fix: give the duplicate another id, or drop its result.

**23. agent.add: kept, not said.** H20: error to the client, 1 row in `agent`, 0 in the session; counts towards 64. Fix: say first, or both in one commit.

**24. Effort is accepted at setup, fails every run.** H10: agent added with effort; each prompt → "effort cannot be set for this provider and model". Check in `setup`.

### P3

25. H13: stopped at a question (also by host shutdown), the model is told "Stopped by a person while it was being done. It may have been done in part." Nothing was begun.
26. R16: cancel before any text, then a prompt → two `user` messages in a row (also several steerings, failed runs). Templates with strict alternation refuse it; merge in the codec.
27. S3: a failed patch says "nothing was changed"; folders n1/n2 remain.
28. S5: `output-*.txt` (up to 32 MiB each) and agents' scratch folders are never deleted.
29. E3: the trailing-space pass rewrites untouched context lines (`"hard break  "` → `"hard break"`), enshrined in a test. E4: an ambiguous hunk silently takes the first place.
30. S2: `null` or `1.0` for optional arguments is refused (local models send both). A non-UTF-8 file can be neither read nor deleted by any file tool. A line over 2000 characters cannot be read past its cut.
31. R7: a rewrite of an 8 MiB file holds Stop for 3.1 s (release); `plan` is not cancellable and diffs twice.
32. U8: overflow at 560×400 (16 px right) and at 700×480 with 2× text (9 px bottom).

## Read-only (not reproduced)

- **P2** AgentSet checks `busy()` then writes; Prompt reads the setup before taking the slot (server.rs:884, 1036). A run can begin with the old access after `set(read)` answered Ok. H7 hit it 0 of 40 times. Two concurrent sets lose one change. Fix: hold the slot's lock across set; read the setup after `speak`.
- **P2** Linux: `sys::children` scans all of /proc for each known pid, every 250 ms, on a runtime worker; `known` is never pruned. Hundreds of processes → 10⁵–10⁶ file opens per tick.
- **P2** `read` with `lines: 1` marks the whole file as seen, so `write` may replace the 9,999 lines never shown.
- **P3** `call`'s `say`/`remember` failures after an effect return without keeping the result although the store might still take it; the result becomes UNKNOWN.
- **P3** run.rs:367: an out-of-order event fails the run without ending its open items.
- **P3** Queued texts are joined without a limit (32 × 1 MiB into one message; the transcript cuts at 1 MiB, the model gets all).
- **P3** "(… a person has been told)" in shell.rs:131: nothing renders `made` (Linux).
- **P3** RequestView ignores `session`, always offers "for the session".
- **P3** `Running::drop` → `Tree::kill` loops up to 1000 × ≥1 ms synchronously on one of two runtime workers.
- **P3** `AgentSet` clears every agent's session grants, not only its own.
- docs/state.md still says M2 "nothing. There is no agent engine" (AGENTS.md rule).

Checked and fine:
- migration 1/2→3 and intent-before-effect order
- engine killed at each point without queued input
- two clients answering one question
- empty or whitespace answers (codecs drop them)
- cost per step (150 steps, 10 MB last request: 1.6 s release in all)
