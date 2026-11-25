# Design spec (Fable), 2026-11-25

A review of the client as it renders today and what to change. For the session that implements it.

**Read from:** 25 images in `app/build/shots/` (written 2026-11-25 18:07–18:25): `shell`, `start`, `start-harness`, `instructions`, `add-agent`, `two-agents`, `asked`, `why-not`, `needs-you`, `plan-used-up`, `changes`, `diff`, `remark`, `copy`, `copy-throw-away`, `accounts`, `accounts-program`, `workbench-dark`, `workbench-light`, `workbench-narrow`, `workbench-usage`, `execution-open`, `context-open`, `transcript-dark`, `sheet-controls-dark`. Then `docs/design/system.md`, `app/lib/ui/tokens.dart`, `docs/product/`, `AGENTS.md`, and code only to check that each change can be made.

**Not run:** nothing was built, rendered or tested for this document. Sizes are read from code; colours from `tokens.dart`; what a screen looks like from the PNGs named.

**Kept:** charcoal and aqua, key radius 5 (plate 6, display 7, overlay 9), no strip of colour on any edge, the widgets layer only, 4.5:1 for words and 3:1 for marks, a keyboard path to every control. `system.md` stands. Where this document would go against a verdict in it, the change is in §6 and nothing in §5.A depends on it.

Paths are relative to `app/`. Sizes are logical pixels. `label`, `ui`, `ink3`, `bay` and the like are the names in `tokens.dart` (`AxType`, `AxColors`).

---

## 1. What is wrong, worst first

| # | Shot | What | Cause | Fixed by |
|---|------|------|-------|----------|
| W1 | `asked`, `diff`, `needs-you` | The agent's row says `shell` (in amber) and `patch`: the tool's internal name. The specimen (`workbench-dark`) says "Asks to run a migration", "Editing refresh.ts". The person learns what is going on only by reading the display. | `shell/shell.dart:330` passes `last.name` | step 1 |
| W2 | `plan-used-up` | A run that failed looks finished: green check, "Wrote an answer". The same red sentence is on screen twice, 440 px apart. The only aqua key on the screen is "Manage usage", so the primary act of a failed conversation is leaving for a billing page. | `viewOf` reads the phase only; `shell/talk.dart:311` repeats `a.error`, which `_Ending` already says; `talk.dart:331` is `AxTone.primary` | step 2 |
| W3 | `asked`, `needs-you` | The four answers break over two rows and the one that was orphaned is "Allow once", the main answer. Above the plate floats a loose amber line, "1 more after this". | `patterns/transcript_parts.dart:516` caps the keys at half the plate and wraps them one by one; `talk.dart:284` | step 3 |
| W4 | `start`, `instructions`, `copy` | An empty check box cannot be seen: `keyDown` (#15171A–#191B1F) on `bay` (#17191D). In `copy` a box that may be ticked and one that may not look the same. | `ui/controls.dart:365` | step 4 |
| W5 | `needs-you` | The queue of what needs a person is the least legible thing on the screen: "Lead asks to run a com…" is cut, the age reads `20782d`, and the header, two lamps, the row under it and the plate are all amber within 280 px. | `patterns/session_panel.dart:472` one line beside the age; `shell.dart:359` has no upper unit | step 5 |
| W6 | `copy`, `copy-throw-away` | Amber no longer means "asks". In `copy` five of six notes are amber sentences and nothing is asking; `copy-throw-away` adds a sixth. With the lamp, the status, the count and the command in `asked`, a screen carries up to seven amber things. | `shell/changes.dart:298`, `:722`; `transcript_parts.dart:428` | §6 D4, step 13 |
| W7 | `asked`, `shell`, `changes` vs `plan-used-up` | The selected row and the selected tab are brown for Claude (#454146 and #2D2525: orange at 9 % and 11 % over graphite) and teal for OpenAI. Selection changes colour with the agent. The owner said of darkened amber: "this colour is awful"; this is that brown. | rule 4a; `session_panel.dart:201`, `controls.dart:247` | §6 D1, step 14 |
| W8 | `instructions`, `two-agents`, `accounts-program` | A main act that cannot be pressed yet (Start, Send, Add) is grey words with no box, beside quiet keys that are also words with no box ("Done", "Accounts"). They differ by ink only (`idle` against `ink2`). | `system.md` States: "disabled (flat words, no surface)" | §6 D3, step 15 |
| W9 | `workbench-dark` | In the two-column transcript the words column has holes: about 70 px of nothing under "I will read the refresh path…" because three calls stand beside one line. | round 7 verdict; `transcript_view.dart:582` | §6 D2, step 17 |
| W10 | `two-agents` | An agent nobody has spoken to yet is an empty black display. | `shell.dart:550` | step 6 |
| W11 | (code; `workbench-narrow` for the specimen) | The window may be 760 wide (`macos/Runner/MainFlutterWindow.swift:9`). The real shell keeps a 264 sidebar there and leaves the display 488. The specimen drops the sidebar under 860 and offers no way to it. | `shell.dart:148` | step 10 |
| W12 | `start`, `start-harness` | What matters most on the form is its smallest text: "Commands run contained: no network, writing only in the folder." and "It runs outside Axiom's sandbox." are 11 px `ink3`, squeezed beside Start, and wrap. "Accounts" is a bare word between selectors and reads as a caption. Gaps of 5 are off the grid of 4. | `shell/start.dart:78–83`, `:716`, `:769` | step 7 |
| W13 | `asked`, `changes` | Stop is quiet words in the far corner of the bar. | `shell.dart:628` | step 8 |
| W14 | `diff` | A patch row shows the beginning of the patch ("Patch …") where every other row shows what it touched. | `transcript_view.dart:1264` takes the first argument | step 9 |
| W15 | `changes`, `copy` | File names are 11 px `ink2`: smaller and dimmer than the notes under them. | `changes.dart:248` | step 11 |
| W16 | all real shots | The strip over the display is 44 px of nothing unless something failed. A globe icon with no word leads to accounts. | `shell.dart:175`, `:384` | §6 D5, step 16 |
| W17 | `asked` | The row says `claude-opus-5-5` where the model's name is known to the host. | `shell.dart:317` | step 12 |

`20782d` comes from the test's data (a time of 2 ms). The guard in step 5 is still right: an age has at most four characters.

---

## 2. What already works and must not change

Do not touch these while doing §5. A golden that changes for any reason other than the one a step names is a defect.

- Every colour value now in `tokens.dart`, both themes; `AxRadius`; `AxSpace`; `AxDensity`; the eleven text styles and their sizes (10–19).
- The lamps (`ui/status.dart`): ring with arc, diamond, ring, two bars, check, cross.
- The transcript's parts: the tile under what was asked (`_Voice`, `block.top`, radius 6), code blocks, inline code in `codeInline`, tables, quotes, `WorkLine`'s columns (22 icon, name, mono detail, result at the right), diff colours and the 34 + 22 px number and sign columns.
- The session panel: one plate holding agents and plan; times and owners ending on one edge; keys that appear on hover or focus and cover no words.
- `AgentBar`: the agent in its own section, its views beside it, the bar's title on the same line as the session's title (`shell_test.dart:113`).
- `RequestBar`'s behaviour: the subject whole and spelled out, allow disabled until it has been scrolled to its end, `Settle`, three answers and "Say why not", all of it by keyboard. Step 3 changes where its keys stand and nothing else.
- The composer: words on the display itself, the 1 px rule that turns `live` when focused, Enter and Shift+Enter, `ExecutionKey` and `ContextGauge` under the words.
- Usage popover and context popover: meters solid, dashed or empty by provenance.
- Left edges: 24 on the display, 12 in the panel.
- `_Place` (popovers open under their anchor, over it near the window's edge).
- The wording, except the strings named in §5.

---

## 3. The design, screen by screen

"as now" means the value is in the code today and stays. **new** marks what a step changes; the step is in brackets.

### 3.0 Frame

| Window width | Sidebar | Display margins (l, t, r, b) |
|---|---|---|
| ≥ 1080 | 264 | 0, 0, 8, 8 |
| 860–1079 | **224** [10] | 0, 0, 8, 8 |
| < 860 | **none; a Sessions key in the strip** [10] | **8**, 0, 8, 8 |

- Window: `Surface(chassis)`, radius 0.
- Strip over both columns: height 44.
  - Sidebar part (≥ 860): left `titleInset + 20`, right 10. `AxMark` 17, gap 9, "Axiom" `uiStrong`, then icon keys `AxTone.quiet`: Accounts and servers (`AxIcon.web`), New session (`AxIcon.plus`).
  - Display part: left 12, right 20. Empty unless something failed: `AxLamp(failed)`, gap 10, one line `label` `faultText` (host not found, host not answering, journal damaged, a key that did nothing: all as now). [16] adds to it.
- Display: `Surface(bay)`, radius `AxRadius.bay`.

**Under 860 [10].** The strip is one row: left `titleInset + 12`; `AxButton.icon(AxIcon.list, label: 'Sessions', tone: AxTone.quiet, engaged: open)`; gap 8; if `store.needs` is not empty: `AxLamp(asking)`, gap 7, `'${n} asking'` in `label` `attentionText`, as one `Pressable` (label `'$n asking, open the first'`) that does what pressing the first row of the queue does; `Spacer`; the failure line as above if there is one; icon keys Accounts and servers, New session; right 8. The Sessions key is the anchor of an `AxPopover` (width 264, height at most the window's less 60) whose panel is the sidebar's list, unchanged. Choosing a session, an agent or a waiting question closes it. Escape closes it and returns focus to the key (the popover does that already).

### 3.1 Sidebar list

`ListView`, padding 8, 0, 8, 12. In order:

**Needs you** (only if `store.needs` is not empty)
- Header **[5]**: padding 12, 0, 12, 4. Row: "Needs you" `label` `ink2`, expanded; the count `small` `ink3` at the right (only if more than one). No amber in the header.
- Row (`NeedsRow`) **[5]**: padding 12, 5, 12, 6; hover `hover`, radius `AxRadius.key`. Lamp `asking` at top 3, gap 9, then three lines:
  1. `'$agent $what'` `ui` `ink1`, **up to 2 lines**, nothing beside it.
  2. the subject, `codeSmall` `ink2`, 1 line, ellipsis.
  3. a row: the session `small` `ink3`, expanded, 1 line; gap 8; the age `small` `ink4`.
- **16** under the last row [5] (12 now).

**The open session** (`SessionPanel`): as now. `Surface(plate)`, radius `AxRadius.bay`; title row min height 36, left 11, `uiStrong`; add-agent icon key 22.

**An agent's row** (`AgentRow`): padding 11, 6, 11, 7; lamp in a 22 column.
- Line 1: name `ui`; three spaces; service mark 11; model `label` (`ink3` selected, `ink4` otherwise); elapsed `small` at the right.
- Line 2 **[1, 2]**: `label`, one line. What it says:

  | The agent | Line 2 | Ink |
  |---|---|---|
  | has a question that can be answered (a request of the open session with `opened.agent` this agent and `answerable == live`) | the request's `what`, first letter upper case: "Asks to run a command" | `attentionText` |
  | its last run failed (`ended == Outcome.failed`) | "Stopped with an error" | `faultText` |
  | last item is a tool | verb and object, table below | `ink3`; `faultText` if that call failed |
  | last item is reasoning / the prompt / an answer | "Thinking" / "Reading the request" / "Writing", "Wrote an answer" (as now) | `ink3` |
  | has no item | no second line (as now) | |

  | Tool name (lower case) | While it runs | After |
  |---|---|---|
  | read, notebookread | Reading *x* | Read *x* |
  | edit, write, multiedit, notebookedit, patch, apply_patch | Editing *x* | Edited *x* |
  | search, glob, list, grep, ls, find | Searching *x* | Searched *x* |
  | shell, test, job, bash, execute, exec_command | Running *x* | Ran *x* |
  | fetch, webfetch, websearch | Fetching *x* | Fetched *x* |
  | delegate, agent, task | Working with another agent | Worked with another agent |
  | anything else | the name, first letter upper case, then *x* | the same |

  *x* is what the tool's row shows as its detail (`ToolRow._detail`, made public as `ToolRow.detailOf`; for a patch, see 3.3). If it has no space and has a `/`, only what follows the last `/`. Then `oneLine(literal(x), 40)`. For a patch with a diff in its result: `'$n files'` (`'1 file'`), counted with `readDiffs`. If *x* is empty the verb stands alone.
- Lamp **[2]**: `failed` when `ended == Outcome.failed`, else by phase as now.
- Selected: gradient `rowSelectedTop` → `rowSelectedBottom` through `selected(…, hue)`; hover `tinted(hue)`. As now until D1.

**Other sessions**: header "Other sessions" (or "Sessions") `label` `ink3`, padding 12, 0, 12, 4; rows `SessionRow`, as now.

**Age** (`_Side.ageOf`) **[5]**: nothing for a time of 0 or less; `now`, `Nm`, `Nh`, `Nd` as now; from 365 days `'${days ~/ 365}y'`.

### 3.2 The bar over the display (`AgentBar`)

As now: height `max(40, control + 12)`; the agent's section on `plate.bottom`, left 24, right 18, mark 14, gap 9, name `uiStrong`, gap 10, lamp; `Seam`; gap 6; tabs; trailing at right 24.

- Lamp **[2]**: as the row's.
- Stop **[8]**: `AxButton(label: 'Stop', icon: AxIcon.stop, tone: AxTone.key)`. It is there only while the agent runs or asks and the host answers (as now).
- A replayed agent: model, harness and account in `label` `ink3`, then `ContextGauge` (as now). Host gone: "not live" `label` `attentionText` (as now).

### 3.3 Conversation

Transcript: left and right 24; reading width 640 (`_measure`), code and tool rows up to 780; two columns from 860 of display width at 54 : 46 with a gap of 44 (as now; D2). "Latest" key at right 20, bottom 14 when not at the end.

| State | Row, line 2 | Lamp (row and bar) | Display | Between transcript and composer | Composer |
|---|---|---|---|---|---|
| Nobody has spoken to it | none | ring | **one line at left 24, top 16: `'Nothing has been said to ${name} yet.'` `ui` `ink3`** [6] | nothing | placeholder `'Say something to ${name}'`; Send |
| Working | "Running cargo test" | turning arc | transcript; the open call's result reads "running" | nothing | `'Say more to ${name}. It is heard before the next step.'`; Queue; Stop in the bar |
| Asking | "Asks to run a command", amber | diamond | transcript | the request (3.4) | as working |
| Failed | "Stopped with an error", red | cross | the transcript ends with `_Ending`: `AxIcon.warn` 13 `faultText`, the provider's words `ui` `faultText`, 14 above | **nothing: the error is said once, by `_Ending`** [2] | Send. On a plan: "Using ChatGPT plan" `small` `ink3`, then **always** `AxButton(label: 'Manage usage', tone: AxTone.quiet, size: 24)`; never a primary key [2] |
| A send from here failed | by phase | by phase | transcript | lamp `failed` at top 3, gap 9, the words `ui` `faultText`, up to 4 lines; padding 24, 0, 24, 10 (as now; this line now shows `draft.problem` only) | the words are put back (as now) |
| Stopped / interrupted | by its last item | by phase | `_Ending` in `ink3`: "Stopped." / "Interrupted: the host stopped while this ran." | nothing | Send |
| Replayed, cannot act | by its last item | by phase | transcript | what was asked, as a list on `block.bottom` (as now) | none |
| Host gone | as last known | as last known | as last shown | as last shown | the key is disabled; writing goes on |

**A patch's row [9]** (`ToolRow`, names `patch`, `apply_patch`): the detail is the paths in its result's diff (`readDiffs(diff)`), joined with `', '`; empty while there is no result. Never the patch's text.

### 3.4 A request (`RequestBar`)

Outside: padding 12, 0, 12, 12. `Surface(plate)`, inside 12, 9, 12, 10.

- Title row: lamp `asking`, gap 9, `'$agent $what'` `uiStrong`, expanded; **if others wait: gap 12, `'$n more waiting'` `small` `ink3`** [3]. `n` is a parameter of `RequestBar` (`waiting`), not of `RequestView`: a changed count must not make it another request.
- 3 under it: the subject, `code`, `attentionText` (D4), whole.
- 3 under it: the scope, `label` `ink3`.
- If it does not fit: "There is more. Scroll to the end to allow this." `label` `ink2`, 5 above (as now).
- **Keys [3]**: two groups, each a `Row` with gaps of 6 that never breaks: *no* = [Say why not `quiet`] [Deny `quiet`]; *yes* = [Allow this for the session `key`, if offered] [Allow once `primary`]. The groups stand in a `Wrap` (spacing 8, run spacing 6, `WrapAlignment.end`), *no* first.
  - Inner width ≥ 720: `Row`[`Expanded`(words), 16, keys]. The keys take the width they need, at most the inner width less 240.
  - Under 720: words, 8, then the keys at the right.
  - So if a row breaks, it breaks between *no* and *yes*, and *yes* is the last row with Allow once at its right end.
- Saying why not (as now): a field under the scope, 8 above; keys [Back `quiet`] [Deny `key`].
- The line `'${n} more after this'` over the plate is removed [3].

### 3.5 Composer

As now. Rule 1 px `bayRule`, `live` when focused. Words: padding 24, 13, 24, 4; `prose`; placeholder `ink4`; height 25–220. Row under: padding 16, 2, 14, 10; `ExecutionKey`, `ContextGauge`, plan words with spacing 6; at the right the attach key if any and `AxButton(action, icon: AxIcon.send, tone: AxTone.primary)`, disabled while there is nothing to send (D3).

### 3.6 Changes, and an agent's copy

- Bar: padding 24, 10, 18, 8. Folder: branch glyph, branch `label` `ink2`, `'$n files'` `label` `ink3`; at the right "Look again" `quiet`. Copy: `'$n files in its own copy'` `label` `ink3`; at the right "Bring it up to date", "Throw it away", "Look again", all `quiet` (as now).
- List: padding 18, 8, 24, 24.
- File row **[11]**: padding **6, 8, 6, 8**. Check box (copy only; 3.9), chevron, path in **`code` `ink1`**, the word (`new`, `removed`) `small` `ink3` (`attentionText` for `conflict`); at the right `+n` `small` `doneText`, `−n` `small` `faultText`, or "not shown" `small` **`ink3`**; then the remark key.
- Note under a file **[11]**: left 50, **`label`** (`small` now). Ink as now (`attentionText` if it needs a look, else `ink3`) until D4.
- Diff under a file: `DiffLines`, left 24 (50 with a box), as now. "N more lines" `label` `ink3`.
- Remark (`remark`): as now.
- Foot of a copy: padding 24, 10, 18, 10; the sentence **`label`** `ink3`; `AxButton('Bring in N files', tone: primary)`.
- Throwing a copy away (`copy-throw-away`): padding 24, 10, 18, 4; the sentence `ui`, `attentionText` if something would be lost, else `ink2`; "Keep it" `quiet`, where the keyboard lands; "Throw the copy away" `key` (as now; D4 changes the ink).
- Nothing to show: one sentence at 24, 16 in `ui` `ink3`; a failure in `ui` `faultText` (as now).

### 3.7 New session, new agent (`StartPage`)

Page: padding 24, 28, 24, 24; column at most 460 wide, at the left; title `title`; 20 under it.

- A part (`FormPart`) **[7]**: label `label` `ink2`; **4**; the control; **4**; note **`label`** `ink3`; 16 under the part.
- Fields: `AxField`, as now (`bay`, radius 5, min height 28, focus border 1.5 `focus`, error border `fault` with a warn line under it).
- Selectors: `AxSelect`, as now.
- "Accounts" **[7]**: `AxButton(label: 'Accounts', icon: AxIcon.web, tone: AxTone.quiet)`: the same globe as the strip's key to the same page.
- Checks: 3.9.
- Foot **[7]**: the sentence that says how it runs ("Commands run contained: …", "It runs outside Axiom's sandbox. …", "Its model and your sign-in; …") on a line of its own in **`label` `ink2`**, full width; **8**; then the key row with Start (or Add) `primary` alone.
- A failure: lamp `failed`, words `ui` `faultText`, 12 above (as now).

| State | Start / Add |
|---|---|
| nothing to start with (no folder, or an agent without a name) | disabled (D3) |
| ready | `primary` |
| failed | `primary`; the failure line under it; what was written stays |

### 3.8 Accounts and servers

As now. It takes the form changes of 3.7 (gaps of 4, notes in `label`) and the box of 3.9.

### 3.9 Check box (`AxCheck`) [4]

16 × 16, radius 3, label `ui` at 8.

| | Fill | Border | Glyph |
|---|---|---|---|
| empty | `bay.bottom` | **1 px `idle`** (`ink2` on hover) | |
| ticked | `Surface(primary)` | its own | check 12 `onPrimary` |
| empty, cannot be ticked | none | **1 px `bayRule`** | |
| ticked, cannot be changed | `Surface(keyDown)` | its own | check 12 `idle` |

The border is on all four sides (`Border.all`). `idle` on `bay` is 5.9:1 dark, 4.8:1 light.

### 3.10 A display with nothing to show (`_Word`)

As now: centred, at most 380 wide; `title` `ink2`; detail `ui` `ink3` 8 under; a key 14 under.

---

## 4. Components and tokens

**Change**
- `AxCheck` (`ui/controls.dart`): 3.9.
- `RequestBar` (`patterns/transcript_parts.dart`): new `int waiting = 0`; keys in two groups; 720 in place of 620; no half-width cap.
- `NeedsRow` (`patterns/session_panel.dart`): 3.1.
- `FormPart` (`shell/start.dart`): gaps 4, note in `label`.
- `ToolRow` (`transcript/transcript_view.dart`): `_detail` becomes `static String detailOf(Object? args)`; a patch's detail from its diff.
- `viewOf` (`shell/shell.dart`): state and second line as in 3.1. It needs the open session's requests: `s.current?.requests`.
- `_Side.ageOf`: 3.1.
- `Shell.build`: widths of 3.0.
- `Talk` (`shell/talk.dart`): no `a.error` over the composer; no primary "Manage usage"; no count line.

**Add**
- `_Strip` in `shell/shell.dart`: the row of 3.0 under 860 (and of step 16).
- `AxColors.tabSelected`, only if D1 is answered B: dark `0x14FFFFFF`, light `0x1410182C`. Add its pair to the contrast test (`ink1`, `ink3` on it over `bay`).
- Shots: `shell-narrow`, `shell-narrow-open` (760 × 520, in `test/shell_test.dart`).

**Remove**
- Nothing that a step needs. Not used by anything but the gallery sheet and the token test, for the owner or a later clean-up: `AxColors.plateHover`, `plateSelected`, `voice`, `scrim`; `AxType.readout`.

No new dependency, no Material, no colour outside `tokens.dart`.

---

## 5. Steps

Each ships alone, on a branch, one concern per commit. For each: `scripts/check.sh app` before and after; a test that asserted the old arrangement is changed to assert the new one in the same commit, and none is removed; a golden is re-recorded only if the step names it, after looking at the difference. "Done when" is what to see in the PNG after `cd app && flutter test`.

### A. Need no answer

1. **The agent's row says what it is doing.** `shell.dart` `viewOf`: second line per the two tables in 3.1 (not yet the "failed" line: step 2).
   Done when: `asked.png` reads "Asks to run a command" in amber under "Lead", not "shell"; `diff.png` reads "Edited 2 files", not "patch"; `changes.png` still reads "Wrote an answer".

2. **A failed run looks failed and says so once.** `viewOf` and `_Display._bar`: lamp `failed` and "Stopped with an error" when `ended == Outcome.failed`. `talk.dart`: the line over the composer shows `draft.problem` only; the primary "Manage usage" goes; the quiet one in the row under the words is always there on a plan.
   Done when, in `plan-used-up.png`: the row and the bar show a cross; the row's second line is "Stopped with an error" in red; the sentence "billing (429): …" is there once; "Manage usage" is there once, as words after "Using ChatGPT plan"; nothing on the screen is aqua except the logo.

3. **The answers to a request stay together.** `RequestBar` and `Talk` per 3.4. Goldens: `controls-*`, `workbench-*`, `execution-dark` if the plate in them moves.
   Done when, in `asked.png`: the four keys are on one row, at the right, in the order Say why not, Deny, Allow this for the session, Allow once; the title line ends with "1 more waiting" in grey; there is no amber line over the plate. `why-not.png` is unchanged except that line.

4. **An empty check box can be seen.** `AxCheck` per 3.9. Goldens: `controls-*`.
   Done when: in `start.png` the box before "Work in a copy of the folder" has a grey outline; in `copy.png` the boxes of `Makefile` and `told.txt` have it and the box of `.envrc` has a fainter one; in `sheet-controls-dark.png` "Include docs" has it.

5. **The queue's rows are whole.** `NeedsRow`, the header, the gap under the list and `ageOf` per 3.1.
   Done when, in `needs-you.png`: "Lead asks to run a command" is not cut; the age is on the third line at the right and has at most four characters; "Needs you" is grey with "2" at the right; the only amber in the list is the two lamps.

6. **A conversation with nothing in it says so.** `_Display._work`: while the agent has no item, the line of 3.3 in place of the transcript; it listens to the agent, so the transcript takes over with the first item.
   Done when: `two-agents.png` shows "Nothing has been said to Reviewer yet." at the display's left edge under the bar.

7. **The form.** `FormPart` gaps and note style; the foot; the globe on "Accounts" (3.7).
   Done when, in `start.png`: the notes are as large as the labels "Folder" and "What should be done"; "Commands run contained: no network, writing only in the folder." is on its own line over Start and brighter than the notes; a globe stands before "Accounts". In `start-harness.png` the sandbox sentence no longer wraps beside the key.

8. **Stop is a key.** `shell.dart:628`: `AxTone.key`.
   Done when: in `asked.png` "Stop" stands on a raised key.

9. **A patch's row names its files.** `ToolRow` per 3.3.
   Done when: `diff.png` reads "Patch  a.txt, b.txt" with "+2 −1" at the right.

10. **A narrow window.** `Shell.build` per 3.0; two new shots at 760 × 520, the second after pressing the Sessions key.
    Done when: `shell-narrow.png` shows the display from 8 px of the left edge to 8 px of the right, a list icon at the top left and no sidebar; `shell-narrow-open.png` shows the session's panel in a floating panel under that icon. `shell.png` (1100 wide) is unchanged.

11. **File names in Changes.** `changes.dart` per 3.6.
    Done when: in `changes.png` and `copy.png` the paths are brighter and one size larger than the diff lines under them, and the notes are as large as "4 files" in the bar.

12. **A model is called by its name where the host has said it.** `viewOf`: if `s.modelsOf(connection)` is known and has the id, its `name`; else the id, as now. No list of names in the client. The test for `asked` lets its host list `claude-opus-5-5` with a name.
    Done when: `asked.png` shows that name in the row.

### B. After the owner's answer (§6)

13. **(D4 = B) Amber is the lamp, the warning glyph and one line.** `transcript_parts.dart:428`: the subject in `ink1`. `changes.dart:298`: a note that needs a look is `AxGlyph(AxIcon.warn, size: 12, color: attention)`, gap 6, the words in `label` `ink2`; other notes `label` `ink3` with no glyph. `changes.dart:722`: the same glyph, then the sentence in `ui` `ink1`.
    Done when: `copy.png` has no amber sentence and five amber triangles; in `asked.png` "cargo test" is white in the plate.

14. **(D1) What is selected.** B: `AgentRow` passes no hue to `selected(…)`; `AxTabs` fills the chosen tab with `tabSelected`; hover keeps `tinted(hue)`. C: both take `live` in place of the service's hue. Goldens: `workbench-*`, `controls-*`.
    Done when: in `asked.png` and `plan-used-up.png` the selected row and tab have the same fill.

15. **(D3 = B) A main act that cannot be pressed keeps its place.** `AxButton`: disabled `key` and `primary` draw a 1 px `bayRule` border, radius `AxRadius.key`, no fill, no shadow, words `idle`; disabled `quiet` stays words. `system.md` States is changed in the same commit.
    Done when: in `instructions.png` "Start" has an outline of the key's size; in `two-agents.png` "Send" has; "Done" in `accounts-program.png` has none.

16. **(D5 = B) The strip says where the work is.** From 860: in the display's part of the strip, when a session is open and nothing failed, the last part of its folder's path in `label` `ink2` (said whole to a screen reader: `'Folder $path'`); at the right `AxButton(label: 'Accounts', icon: AxIcon.web, tone: AxTone.quiet)`, right 8; the globe icon key leaves the sidebar's head.
    Done when: `asked.png` shows the folder's name over the display at its left and "Accounts" with a globe at the right, and the sidebar's head has only the plus.

17. **(D2 = B) Long runs fold.** `_RowView`, wide only: of the closed calls beside one passage, if more than four, the last three are shown and over them a row `'$n earlier'` (`label` `ink3`, chevron as on a tool row) that opens the rest; a failed call and the open one are never folded.
    Done when: in `workbench-dark.png` no gap in the words column is taller than four tool rows.

---

## 6. For the owner to decide

1. **The fill of what is selected** (`asked.png` against `plan-used-up.png`). Rule 4a gives it the service's hue; with Claude that is brown.
   A. As now. B. Graphite for every agent; the hue stays on marks, levels and hover (recommended). C. Aqua, as rule 4 says of the selected tab.

2. **Holes beside the work column** (`workbench-dark.png`). Round 7 made two columns the real transcript.
   A. As now. B. Two columns; runs of more than four calls fold to the last three (recommended). C. One column at every width, as in `workbench-narrow.png`.

3. **A main act that cannot be pressed yet** (`instructions.png`, `two-agents.png`). `system.md` says flat words, no surface.
   A. As now. B. It keeps a hairline outline and grey words; quiet keys stay flat (recommended).

4. **How much amber** (`copy.png`, `asked.png`). Rule 4c allows amber as a word.
   A. As now. B. Amber only for the lamp, the warning glyph and the one line that says who asks; the command and the notes in neutral ink (recommended).

5. **The strip over the display** (every real shot). PR-4 puts a dropdown per service in the header; the host reports no usage per account yet.
   A. Empty until usage exists. B. The folder's name and an Accounts key with its word (recommended). C. A key per service now, with account names and empty tracks marked "not reported".
