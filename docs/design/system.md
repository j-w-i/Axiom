# Design system

Source of truth: `app/lib/ui/tokens.dart`. Gallery: `app/lib/gallery/`. Images: `flutter test` writes `app/build/shots/` (offscreen).

**The idea.** A professional desktop application: flat, near-neutral charcoal, what people know from other tools where they expect it, aqua only where something runs or has the keyboard, a service's hue where something is its own.

## Rules
1. **Containment is spent, not given.** A surface may hold another only if the person acts on it as a unit (a key, a request, the composer, a menu, a group of options). Lists, selectors and activity are text on the ground they belong to.
2. **Flat.** Grounds are the chassis (the sidebar) and the bay (everything else); a hairline parts them. A card is one tone up with a hairline round it. Only what floats casts a shadow (tested). No gradient, lit edge, inset, bevel, gloss or glow. (Rounds 1–11 had them; the owner: "it looks a bit like a toy and less of a professional desktop application".)
3. **Four inks that recede from each other** (tested): words, quieter words, notes, and, on the grounds only, marks and ages.
4. **Aqua is what runs and what has the keyboard**, and nothing else: not code, not icons, not keys. Keys have no hue; the main act is the lightest thing there is (dark theme) or the darkest (light). The owner's choice of three. Amber asks, red failed, green done; each state has its own shape (tested).
4a. **Things take the hue of the service they belong to.** Its mark, its level, its check, the hover of its menu item. What is selected has one neutral fill, whoever it belongs to. Icons of work have no hue; a failed one is red. Code, diffs and states are in colour. How full a context is, is neutral. Marks identify a service really in use (`THIRD_PARTY.md`); unknown models get none.
4b. **No accent edges.** No coloured strip along any side of a tab, card, row, menu, key or message. Lamps: running is a ring with an arc that goes round, asking a diamond, waiting a ring, paused two bars, done a check, failed a cross. A test reads the source for one.
4c. **Amber is the lamp, the warning glyph and the one line that says who asks; never a ground.**
5. **Grid of 4.** What is said, asked and written is one column, 720 at most, in the middle of its room, 24 from the edge at least. What was asked is at the right, as wide as its words, 600 at most. What came of a call stands beside it. Icons are 14–16 in a fixed column.
6. **Two voices.** Inter for words; JetBrains Mono, without ligatures, only for literals: code, paths, commands. Both are shipped, so that what is rendered in a test is what a person sees on every system.
7. **Numbers say where they come from.** Solid meter = reported or measured; dashed = estimated; empty track and words = unknown.
8. **Nothing new can be pressed by a click meant for something else** (`Settle`): execution panel, requests. A question falls the moment anything it was about changes.
8a. **A key looks like one.** It has a ground or an outline. Words alone are keys only among others, where they are the lesser of two.
8b. **A row shows its further keys when hovered; state is always shown.**
9. **Things sit inside what owns them.** Sessions under their folder, in the order they were made: opening one or working in it moves nothing. Agents under their session. An agent's views beside its name. What an agent asks is asked in its own conversation, and elsewhere only pointed to. Model, harness, account, effort and access behind one key where the agent is spoken to, next to how full its context is. Choosing a model or a harness says what that costs and waits for a yes. What the session plans and has changed is in a panel at the right, which can be put away and is not there when empty.
9a. **Every page has a way back**: Cancel or Done, and Escape. What is shown is what is selected in the sidebar, and nothing else is.
9b. **The words other tools use**: Refresh, Discard, Apply, Update, Cancel, Remove, Comment. Not "look again", "throw away", "bring in".
10. **Motion** is springs (`snap`, `glide`) that can be interrupted; nothing waits for them; reduced motion jumps. Only a running lamp moves by itself.

## Roles
| Role | Use | Treatment |
|------|-----|-----------|
| `chassis` | the sidebar | one tone above the bay |
| `bay` | everything else | the ground |
| `plate` | a request, the composer, text fields | one tone up, hairline |
| `block` | code, what was asked | one tone up, no outline |
| `key` / `keyDown` / `primary` | keys, held ones, the main act | fill, hairline / fill |
| `overlay` | menus, search | fill, hairline, shadow |
| `selected` / `hover` | rows, tabs, quiet keys | a veil |

Radii: check 4, control 6, code 8, card 10. Controls are 28 high, icon keys square. Type 10.5–19. Density compact/regular/roomy changes control height (24/28/32), never type size.
States for every control: rest, hover, held, engaged, focused (keyboard ring), disabled (a main act and a key keep a hairline outline with grey words; a quiet key is flat words), loading, error.
Keys of the window: new session N, search K, accounts comma, sidebar B, with ⌘ on macOS and Ctrl elsewhere.

**A request** is shown whole, spelled out, and cannot be allowed before it has been seen to its end.

## History
| Round | Concepts | Verdict |
|-------|----------|---------|
| 1 | A Console, B Score, C Folio (`img/a…c.png`) | A's materials, C's parallel work column, B's timeline carried forward |
| 2 | D synthesis (`img/d.png`) | owner: "mostly like it"; sky blue "is NOT it"; skeuomorphism "a tad too far" |
| 3 | E cobalt, shallower (`img/e.png`) | owner: "needs a bit more polish"; "something is off about most of the app" |
| 4 | real components, rule 1 (`img/round4-dark.png`) | owner: plan and models must belong to a session; not a fan of the layout at the bottom |
| 5 | agents and plan inside the open session's panel; execution beside the agent's name; no deck (`img/round5-*.png`) | shown 2026-11-24; owner: more colour, service marks, "the claude is orange cus claude" |
| 6 | hues by service and by kind of work, marks, richer code and states (`img/round6-*.png`) | accepted by the owner, then: "not a fan of this colour" of the navy conversation area |
| 7 | neutral charcoal: option B of three, "slightly darker"; the two-column transcript is the real one (`img/round7-*.png`) | the owner's choice, 2026-11-25; then: execution into the talking bar behind an expandable key, a warning before changing models, context shown clearly, views next to the agent, the agent in its own section |
| 8 | `ExecutionKey`, `ContextGauge`, `AgentBar`; the first had a lip on the agent's section and brown keys, rejected at once: all accent edges and amber grounds removed (`img/round8-*.png`) | 2026-11-25: conversation "better now"; not the cobalt of the buttons, and the violet running pill "just bad" |
| 9 | the owner's sampled blue for keys, links and live; lamps redrawn (`img/round9-*.png`) | 2026-11-25: "more of an aqua" |
| 10 | aqua, option B of three, with dark words on keys; every corner 2 rounder, then 1 back (asked for on buttons) (`img/round10-*.png`) | accepted by the owner, 2026-11-25 |
| 11 | Fable's review, steps 1–16 (`spec-fable.md`): what agents do in words, failure seen as failure, whole rows, one line of answers, a narrow window; graphite selection, outlines for main acts that cannot be pressed, less amber, the folder and Accounts in the strip | 2026-11-25, from pictures: B for D1, D3, D4, D5; D2 (long runs) as it was |
| 12 | flat surfaces, neutral keys, Inter, sessions by folder as a tree, one-column conversation, panel at the right, requests where their agent is, ordinary words | asked for by the owner 2026-11-25 ("a bit like a toy"; confusing; placement, colour and shapes off). From pictures: "definitely much better, and yes I like A" (neutral keys). Reverses: two columns (7), colours by kind of work (6), plan in the session's panel (5), key radius 5 and aqua keys (10), raised materials (1–11) |
