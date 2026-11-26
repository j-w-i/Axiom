# Worth building

Features T3 Code has, Axiom neither has nor plans, and Axiom should. Written 2026-11-25.

**Compared:** T3 Code v0.1.12 (`ca81b7b`) with Axiom `m5/unheard` (`f5a6bb4`), and with `design/modern` (`ea51056`) where it adds a function. Source and documents were read; neither app was run.
**Status:** in the roadmap since 2026-11-25: the owner asked for these to be worked into the milestones as advised here. The decisions at the end are followed as recommended, each written down in `../decisions/` before its code. Ideas are taken, no code.
**Left out:** what M0–M7, the PR requirements, the known issues or rows 1–15 of `../research/t3code-comparison.md` already cover. Where those name only the broad parent, the parent is given.

## How to read it

- **Size** is an estimate: S is about a day, M is several days, L is weeks. Tests are included.
- **Where** is the milestone it fits best.
- **Here** is what Axiom already has to build on. Paths are on `m5/unheard` unless it says otherwise.
- **Care** is what must not go wrong. `SI-n` is in `../security/threat-model.md`, `K-n` in `../known-issues.md`.
- **D-n** means one of the decisions at the end comes first.

The order is: what is cheap and useful every day, then what makes Axiom a complete place to work, then what belongs inside a large milestone.

| Part | Items | In short |
|------|-------|----------|
| 1. Now | 1–17 | Small, no decision needed |
| 2. Next | 18–41 | What people will otherwise leave Axiom to do |
| 3. With their milestone | 42–59 | Inside M4, M5 or after |

**If only ten:** 1, 2, 5, 8, 9, 18, 19, 24, 27, 31.

---

## Part 1. Now

### Talking and reading

**1. Earlier prompts with ↑** · S · M2
In an empty composer, ↑ brings back the last prompt of this session, and ↑ or ↓ moves through the rest.
*Here:* prompts are items of the session model already (`ItemKind.prompt`), so this needs only the client.
*Care:* never replace words that have been written. Leave input methods and moving the caret in several lines alone.
*Done when:* ↑ on an empty box recalls, ↓ past the newest gives the empty box back, and a box with words is untouched.

**2. Find in the open conversation** · M · M2
⌘F searches the whole session, including what is not built or loaded. It shows the number of matches, and Enter and Shift+Enter step through them.
*Here:* the transcript is lazy (K13), so the search must run on the session's data and then move the view. Row 13 is search across sessions; build one index for both.
*Care:* K14. Off the UI isolate.
*Done when:* a word in the first message of a 50 000-block session is found and shown, within the frame budget.

**3. Copy a whole message** · S · M2
A key on each answer and prompt copies it as Markdown. Tables also copy as CSV.
*Here:* only code blocks and selections copy now, and a selection copies only what is built (K14).
*Care:* copy what was said, not the escaped form shown for hidden characters, and say so when there were any (T11).

**4. Quote an answer in the draft** · M · M2
Select words in an answer and press "Quote". They go into the draft as a quotation, with room for a remark. "Reply to this" does it for a whole answer.
*Here:* remarks on diff lines do the same thing (`app/lib/shell/changes.dart`).
*Care:* an agent's words quoted by a person must stay marked as the agent's (SI-17).

**5. Every command has a key** · M · M1 or M2
One table of commands, used by ⌘K, the keys and tooltips: next and previous session, session 1–9, stop, take back, find, change model, effort or access, next request, allow and deny.
*Here:* `design/modern` has ⌘K over sessions and actions, and five window keys (N K , D B). They are a `switch` in `shell.dart`; make that a table before it grows.
*Care:* a key must never allow a request that has not been seen to its end. Include a key that moves between areas (F6 in T3).
*Done when:* PR-41 holds for going anywhere, not only within a control. Letting a person change the keys is item 41.

**6. Compact now** · S · M2
A person can have the conversation compacted, and sees in the transcript that it was and what it kept.
*Here:* `engine/src/compact.rs` does it by itself when the context fills. The wire has no call for it.
*Care:* only between runs. For a delegated program, use its own way, or do not offer it.

### Sessions

**7. Pin, reorder, mark unread** · S · row 12
Pinned sessions stay on top, in an order the person chooses. "Not seen" can be put back.
*Here:* `design/modern` groups sessions by folder. Row 12 covers rename, set aside, snooze and undo; build these with it.

### Programs

**8. A program of the person's own** · M · M3
Enter a name, a command and arguments for any agent that speaks ACP. This is how Kiro, fx, Devin, Junie and the rest become reachable without Axiom knowing them.
*Here:* `harness/src/acp.rs` already takes `Acp.args` "from the person's own configuration or KNOWN". What is missing is where a person writes it, and where it is kept.
*Care:* never from a workspace (SI-5). Same trust gate before each start (0006, M3). Without a known mode only "ask" is possible; say so.
*Done when:* a scripted ACP program not in `KNOWN` can be added, started, asked about and removed.

**9. Where a program is, its arguments, its environment** · M · M3
For each program and account: the path, extra arguments and variables. It settles K45 (nvm, asdf, own npm prefix), and lets Claude Code use Bedrock or Vertex, which its own documentation describes.
*Here:* `harness/src/find.rs` already refuses what others may write to; a chosen path goes through the same checks. Claude Code accounts already have a folder each.
*Care:* a secret value goes into the OS store and is never shown again. It must not reach `argv`, the log or the journal. Do not allow variables that load code (`LD_PRELOAD`, `NODE_OPTIONS` and the like). Changing the base address must not become a way round decision 0007.

### Work in progress

**10. The agent's own list of things to do** · M · M3
Show an agent's plan as a list that ticks off, in the panel.
*Here:* Codex's plan arrives (`Event::Plan`) and becomes a notice of text (`host/src/delegated.rs:820`). The panel on `design/modern` has the place for it, and `state.md` there says the host has nothing to put in it. Needs an event on the wire and a tool for Axiom's own loop.
*Care:* it is the agent's claim, not a gate (PR-25).

**11. Background jobs, seen and stopped** · S–M · M4
A list of what agents have started and left running, with its command, time, last lines and Stop.
*Here:* the `job` tool does start, read, stop and list for agents (`tools/src/jobs.rs`). The person has only transcript rows.
*Care:* stopping kills the tree (SI-24) and nothing else.

**12. Ways to look at a diff** · S · M4
Side by side when there is room, ignore white space, wrap long lines, collapse all.
*Here:* `DiffLines` is unified only.
*Care:* ignoring white space changes the view only. What is brought in is still checked by digest.

**13. Remarks on several lines** · S · row 3
Row 3 says "not of several lines yet". Finish it: a range, several remarks in one draft, each with the copy's digest.

### The application

**14. A settings screen** · M · M1 or M2
System, light or dark; text size that icons follow (K26); zoom; code font; density; which key sends; notifications; what the limits are.
*Here:* density is a token already, and nothing sets it. The theme follows the OS only. Items 5, 15, 32 and 35 all need somewhere to live.
*Care:* keep it short. No theme editor, no custom CSS.

**15. How one is told** · S · row 1
Row 1 plans a badge and a notification. Add: off, notification, sound or both; a different sound for "needs you" and for "finished"; silent when that session is being looked at; a click opens it.
*Care:* names only. Wait 2 s before saying "finished" (T3's #31087).

**16. CLI for scripts** · S · M2
`--json` everywhere; `say --wait` prints the answer; the exit code says done, failed or waits for a person.
*Here:* `cli/src/main.rs` has the calls. Today sessions print as JSON, and the codes are 0, 1 and 128 plus the signal.
*Care:* PR-13. A CLI wait must never answer a request by itself.

**17. Drop a folder, or open one from a terminal** · S · M2
A folder dropped on the window starts a session there. `axiom open .` does it in the running app.
*Here:* `design/modern` has the system's folder window.
*Care:* it fills in the form. It does not start an agent, and it grants nothing.

---

## Part 2. Next

### Files

**18. Read a file** · M · M4
A viewer with line numbers, real highlighting (K6), wrap, search and "go to line"; Markdown as it reads, images, CSV as a table. It can be pointed at from the transcript, a diff, a terminal or a search.
*Here:* nothing for the person beyond changed files. The client touches no workspace file, so the host reads by handle (SI-9).
*Care:* bounded size, hidden characters shown (T11), nothing fetched (T23). Read only (D-5).

**19. A path in an answer opens that file** · S after 18 · M4
`src/a.ts:42` becomes something to press, and opens at the line.
*Here:* K12 plans web links that ask first. This is different and needs no question: it reads inside the folder.
*Care:* outside the folder it asks, as any other read does.

**20. Find a file, find in files** · M · M4
Quick open by name, and text search with case, word and regular expression. Then a file tree, which can also put `@file` in the draft (row 14).
*Here:* agents have `list`, `glob` and `search`. Give the person the same code.

**21. Open in the person's editor, or show in Finder** · M · M4 · D-3
VS Code, Cursor, Zed, JetBrains and others, at the line.
*Here:* `outside.dart` opens two addresses. The client spawns nothing (SI-14), so this is the host's, on the one spawn path.
*Care:* absolute paths from trusted folders only (`find.rs`). The file's name is an argument, never part of a command line.

**22. Pictures an agent made** · M · M4
A screenshot or chart in the folder is shown in the transcript, can be enlarged, saved and copied.
*Here:* an image is a link now (`transcript/inline.dart:172`).
*Care:* local files only. Remote ones stay inert chips (T23). Decode off the UI isolate, with limits on size and on pixels. No SVG.

### Planning

**23. Plan first, then do** · M · M3
An agent may only read and must answer with a plan. The plan is shown as such; the person changes it, accepts it or says why not; then the agent works.
*Here:* access "read" exists, and Claude Code's plan mode is left out on purpose (`harness/src/claude.rs`). Native plan mode could be "read" plus a tool that hands in a plan.
*Care:* accepting a plan grants nothing beyond the access the person chooses next. No message is ever an approval (SI-2).

### Agents

**24. An agent may ask for another agent** · L · M5
Give work to a new agent, with harness, model, effort, access and copy. Now an agent can only talk to those a person added.
*Here:* `host/src/peers.rs` has `list, tell, wait, read`; PR-20, PR-21 and SI-1 describe the rest.
*Care:* child within parent (SI-1). A person is asked when it widens, starts a program or uses another account. Depth, number and cost are bounded, which K49 says they are not yet. Not before K49.

**25. Know and stop what was given away** · S–M · M5
A state for each piece of work (working, waits for a person, waits for children, done, failed), and cancel.
*Here:* delivery exactly once is built (row 2). "Stopping the parent stops the children" is SI-24.

**26. Answers that can be checked** · M · M5
Whoever gives work can ask for a result of a shape (JSON schema). What does not fit is sent back with the reason.
*Here:* `read` gives free text.
*Why:* a gate in a task graph (PR-25) needs something it can test.

### Models and usage

**27. What it cost and what it used, over time** · L · M6
By day, model, account, agent and session: tokens by kind, what the cache saved, estimated cost.
*Here:* M6 plans provenance and budgets. The harness reports `Cost` (`harness/src/event.rs`), and the wire's `Usage` has no place for it, so it is lost. Keep it now: history cannot be made up afterwards.
*Care:* a price table is dated, and gives "estimated". On a plan, the cost is "what the API would have charged", never "you spent". Unknown is unknown (PR-30).
*First step (S):* put cost and provenance on the wire and into the journal, before any view is drawn.

**28. Limits beside the conversation** · M · M6 and row 6
What is left in each window of this account, when it resets, and whether at this pace it lasts.
*Here:* `Limit` events carry used, reset and length. The host reports no usage per account, so `UsageKey` is only in the gallery. PR-4 wants it in the header.
*Care:* only where the program reports it (M3). No reading of another program's login to find out (0007).

**29. Choose a model without knowing its name** · M · M3
One searchable list over all accounts, with favourites, hiding and the last used. For Codex, what `model/list` says; for ACP, what it reports; a typed name always works.
*Here:* Codex's list is fetched after it has started and used only to check names (`harness/src/codex.rs`). Claude Code and Codex are typed names in the form.
*Care:* asking a program for its list starts it, so it needs the trust gate (0006, M3). Keep the last answer with its date.

**30. What a model can be told** · M · M3
Effort as the model names it, up to `xhigh` or `max`; speed where there is one; the size of the context. Shown only where offered.
*Here:* three levels of effort are fixed in `start.dart`.
*Care:* speed costs more. Say so, as the execution key does for a change of model.

### Sessions

**31. Bring in earlier conversations** · L · M6
Find what Claude Code and Codex kept for this folder, and continue it in Axiom.
*Here:* PR-51 brings in rules and skills, not conversations. The adapters can resume an id they are given.
*Why:* it is the difference between trying Axiom and moving to it.
*Care:* D-4. The person picks the folder and the sessions, and nothing is read before that. Everything brought in is labelled as from elsewhere (SI-17). No credentials. Read at most once, with limits. Bringing it in twice gives one session.

**32. Titles that say what it is about** · S–M · M6
A short title from a small model the person named, with "again" and "leave mine alone". The same choice later serves commit messages (43).
*Here:* the title is the first words of the first message.
*Care:* off until a model is chosen. It sends what the session was about to another model, so it needs a word about where that goes (PR-2).

**33. Set aside by rule** · S · row 12
After so many days without a word, or when all is brought in. Never what waits, runs, failed or holds changes that only a copy has.
*Care:* off by default, undo, says why. T3 hides failures (#27052).

**34. Working, and nothing to do for you** · S · row 12
Sessions that only wait for a job or a child fold into one line, and come back when they end or need someone. An agent can say what it waits for.

**35. Throw a session away** · M · M7 · D-2
With its copies and attachments, and the space comes back. It is the only way to remove something that should not have been pasted.
*Here:* none. Decision 0004's journal only grows.

### Folders

**36. Folders with an identity** · M · M4
A name, pin, remove, and settings of their own: default model and access, setup command (row 10), commands (37).
*Here:* K16, the workspace is a label. SI-21 wants an id anyway, and item 46 needs it.

**37. Commands of a folder** · M · M4, after terminals
"Dev", "Test", "Build": name, icon, key. Runs in a terminal, or as a job until there are any.
*Care:* kept by Axiom, not read from the repository. If a file in the repository may offer them, each is approved by its exact text, again when it changes (SI-5, SI-6, T14; row 10 does the same).

### The computer

**38. What is using the computer** · M · M4
Processes by agent: processor, memory, how long, and Stop. Orphans are shown as such.
*Here:* `axiom-spawn` knows every tree; nothing shows it. Background checks must be cheap and stop when nobody looks.
*Why:* with ten agents this is a daily question.

**39. Bounds on memory** · M · M7
A limit for what an agent runs: cgroups on Linux, job objects on Windows, and on macOS watching and stopping, which must be called that.
*Here:* only core dumps and the number of files are limited (`spawn/src/tree.rs`).

**40. First start** · M · M7
What was found on this computer, ready first; what is missing, with the command to copy; sign in; or start with a local model.
*Here:* finding programs, accounts and the sign-in page exist separately. Axiom installs nothing.

**41. Keys a person can change** · M · M7, after 5
Search, record, conflicts, reset.

---

## Part 3. With their milestone

### Git, for a person (M4, all D-1)

M4 says "Git and worktrees through the hardened wrapper" and names none of these. Agents still never write `.git` (SI-11, 0014). These are a person's acts, in the host.

**42. Commit what was brought in** · L
Choose files, see exactly what goes in, write the message, commit; on a new branch if wanted, and with a question on the default branch.
*Care:* hooks and filters are the repository's code. Either they run in the sandbox with the person told, or they do not run and the app says so.

**43. Suggested commit message** · S after 32 and 42
From the diff and the last few subjects. Always in a box to change.

**44. Push and pull** · L
Fast-forward only. Ahead and behind shown.
*Care:* Axiom reads no credential. It lets git use the person's own helper or agent, outside any sandbox an agent can reach, and shows the remote it will talk to. Agents never get it (T17).

**45. Compare with something else, read the log** · M
Against a branch, a commit or the merge base, and one commit at a time. Read only; needs no decision, and can come first.
*Here:* `host/src/changes.rs` compares with the last commit.

**46. A folder that holds several repositories** · M
Each with its own branch, changes and commit. Now it says "not a git repository".

### Terminals (M4 plans "persistent terminals")

**47. Output into the draft** · S
Select, "Add to message"; it goes with the command and is marked as output (SI-17).

**48. A terminal worth staying in** · M
Several, split, kept over a restart of the window, named by what runs, asking before one that is busy is closed; paths open files (19); sign-in for programs happens here instead of in a copied command.
*Care:* T42.

**49. Servers that agents started** · M
Found through the process tree, not by polling ports; shown by agent, with Open and Stop.
*Here:* the comparison's "later" line says ports beside terminals.

### Browser (M4 says "browser", all D-6)

**50. One page for both** · L
The agent's browser can be seen and taken over by the person. Whoever holds it is shown; the agent does not touch a page its person is using.
*Care:* in the host, in its own process and profile, never the person's. The client shows pictures (SI-14). What a page says is hostile (SI-17, SI-18).

**51. Point at it** · M after 50
Press an element: the draft gets its selector, text, styles, a picture, and the source line if it is a development build. Drawing on the page is the same thing with a region.
*Why:* the best thing T3 has for work on interfaces.

**52. Sizes, dark mode, recordings** · M after 50
Enough to check a layout and show what happened. Profiles and several tabs only if asked for.

### Orchestration (M5)

**53. Runs that can be resumed** · with the task graph
Each step has a key and happens once; it can be retried a bounded number of times, paused and continued; a person can be asked inside it.
*Here:* PR-25. T3 does it with scripts, which are Labs there. A graph that can be seen is better here; add scripts only if the graph proves too weak.

**54. Branch from an answer, and change an earlier prompt** · M · M2 "branching", row 7
Two presses on what the engine will have: "from here", and "say this again differently", with or without the files.
*Care:* T3's own break threads (#17423, #34753). If a program cannot go back, do not offer it. Files only in copies.

**55. Ask about a session without disturbing it** · M after 54
A side conversation that knows the session and writes nothing to it. Its answer can be quoted (4).

**56. One session in another** · M
`@session` in a prompt; the agent reads it when it needs to.
*Care:* across folders only with a grant (SI-21, T37). Needs 36.

**57. Axiom for the person's other agents** · M · D-7
A Claude Code in a terminal can list, start, tell and wait. Over standard input and output with a local grant, never HTTP.
*Here:* `host/src/mcp.rs` is such a server, for what Axiom started.
*Care:* a level per grant, shown, revocable. It answers no request, ever (SI-2).

**58. Compact while idle** · S–M after 6
Shortly before the provider's cache ends, if it would pay. Off by default, says what it saved.
*Here:* `research/harness-engines-a.md` suggests it.

### Upkeep (M7)

**59. Journal upkeep, start at login, a way out** · M
How much there is and where; compaction that keeps replay true; a check that can repair; the host as a login item by choice, which makes PR-14 true over a restart; and when a host will not start, what the person can do about it.

---

## Decisions that come first

| # | Question | Recommendation |
|---|----------|----------------|
| D-1 | May Axiom write `.git`, and talk to a remote, when a **person** says so? | Yes to commit, as its own decision: the host only, hooks stated, agents unchanged. Push after it, with git's own credentials and none of them read |
| D-2 | May a session be deleted although the journal only grows? | Yes. Remove the content, keep a record that something was removed. Privacy outweighs a complete history |
| D-3 | May the host start the person's editor? | Yes: a fixed list of programs, on the one spawn path, never for an agent |
| D-4 | May Axiom read what other programs kept of conversations? | Yes, after the person chooses, never by scanning first. They hold secrets people have pasted; treat them so |
| D-5 | May a person edit files in Axiom? | No. "Not an IDE" is right, and item 21 hands over well |
| D-6 | May an agent reach a server it started itself? | Yes, but only ports its own process tree listens on, and only through the proxy of M4. Without it, 49–52 are of little use |
| D-7 | May programs Axiom did not start use it? | Yes over a local grant; later than the rest |
| D-8 | Pull requests (already "the owner's decision") | Yes, narrowly, after D-1: GitHub only; open one, show its checks and comments, and let failed checks or comments wake the agent that made it. No page of all pull requests, no review tool, no stacks |
| D-9 | Schedules (already "the owner's decision") | Yes, only a time and a prompt in a chosen folder with chosen access, asking like any other run. No webhooks (SI-20) |
| D-10 | Another computer over SSH | Not now. It is the one way that does not need a listener, so nothing should be built that rules it out |

## Looked at and not worth it

| What | Why not |
|------|---------|
| Voice, screenshots of other apps | The system has both. Voice would mean models or a vendor, and captures need wide permissions |
| HTML replies, MCP apps | A web engine in a client that has none. Pictures (22) give most of it |
| Mermaid, maths | Nice, hostile input (T23), rarely needed. After K6 at the earliest |
| Theme editor, imported themes, custom CSS | One look done well (`../design/system.md`) |
| Translations | One person uses it, in English. Keep text out of layout so that it stays possible |
| Simulators and emulators | Useful for mobile work only; large; can come as an MCP server |
| ACP registry, installing and updating programs | Running what a catalogue says. Item 8 gives the reach; a note that a version is old is enough |
| PDF, Office, notebook, archive and database viewers | Not an IDE; item 21 |
| Issues and Linear | Another account and a lot of surface. An MCP server does it once M4 has the client |
| Cloning, publishing, new projects | A terminal does it, and it happens rarely |
| Prompt stash, prompt prediction, rich text, `@model`, zen mode, minimap, hover cards, bulk selection | Little use for their upkeep. Row 12's drafts, item 2 and item 5 cover most of it |
| `/goal` | Codex, Claude and OpenCode do it themselves; a bounded graph with gates (53) is the safe form |
| Agents that arrange sessions, themes or settings | Cuts against who decides what |
| Reset credits, pooled limits, more quota sources | Reads other programs' logins, or invites rotation (0007, PR-7) |
| Pi, Muse Code | No ACP. If they gain it, item 8 reaches them |
| Remote access, pairing, guests, web, phone | SI-20, and not a hosted service |
| Company policy files, 1Password, cookie import, port forwarding, default browser | For other people, or against an invariant |
| A card that gives an agent a secret | T29 and SI-3. A host-only form can be thought of when something needs it |

## Sources

Each item was found in T3's source and looked for in Axiom's documents and code by one reader, then checked by a second who tried to find the opposite. The checks were on words and code paths, not on a running program. Before building one, look again: `design/modern` was changing while this was written.
