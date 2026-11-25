# Design system

Source of truth: `app/lib/ui/tokens.dart`. Gallery: `app/lib/gallery/`. Images: `flutter test` writes `app/build/shots/` (offscreen).

**The idea.** A professional instrument: neutral graphite structure, one recessed charcoal display, aqua where something is live, a service's hue where something is its own.

## Rules
1. **Containment is spent, not given.** A surface may hold another surface only if the person acts on it as a unit (a key, a request, a menu, the display, the open session). Lists, selectors and activity are text on the surface they belong to. (Round 3 failed on this; the owner named it: "something is off about most of the app", and the plain sidebar was the part that worked.)
2. **Three depths.** Chassis; display (below); session panel, keys, requests (above). Overlays float. Nothing else has depth.
3. **One light, from above.** Raised parts have a faint lit top edge and a 1 px shadow; recessed parts a 5 px shade. Gradients span 2–4 %. No gloss, glow, engraving, side bevels or blur.
4. **Aqua means live**: running, focus, the primary act. Never decoration. Hue 192–201 (tested), the owner's choice of three. A bright key carries dark words; levels are neutral. Amber asks, red failed, green done; each state has its own shape (tested).
4a. **Things take the hue of the service they belong to.** Its mark, level, check and hover tint. What is selected is graphite, whoever it belongs to. Kinds of work have hues of their own (read, change, search, run, web, think), and code and diffs are in colour. Marks identify a service really in use (`THIRD_PARTY.md`); unknown models get none.
4b. **No accent edges.** No coloured strip along any side of a tab, card, row, menu or message (owner: "every LLM does this weird lip thing"). What is selected is filled; what was asked stands on a tile; what asks has the amber lamp. Lamps: running is a ring with an arc that goes round, asking a diamond, waiting a ring, paused two bars, done a check, failed a cross. A test reads the source for one.
4c. **Amber is the lamp, the warning glyph and the one line that says who asks; never a ground.** What is asked about, and notes, are in neutral ink. Darkened it is brown (owner: "this colour is awful"). The answer that acts is aqua, like any other primary act.
5. **Grid of 4.** Text starts on a shared left edge per surface (display 24, panel 12). Icons are 12–14 in a fixed column; text uses even leading so glyphs and words share a centre.
6. **Two voices.** Archivo for words; Spline Sans Mono only for literals: code, paths, commands.
7. **Numbers say where they come from.** Solid meter = reported or measured; dashed = estimated; empty track and words = unknown.
8. **Nothing new can be pressed by a click meant for something else.** What has just come or changed under a pointer takes no click until the pointer moves or half a second passes (`Settle`): execution panel, requests. A question falls the moment anything it was about changes.
8a. **Controls appear where the hand is.** A row shows its keys when hovered; state is always shown.
9. **Things sit inside what owns them.** Agents and plan in their session; an agent's views beside its name, which has its own section of the bar; model, harness, account, effort and access behind one key where the agent is spoken to, next to how full its context is. Choosing a model or a harness says what that costs (the context is read again) and waits for a yes; the keyboard lands on the answer that changes nothing.
10. **Motion** is springs (`snap`, `glide`) that can be interrupted; nothing waits for them; reduced motion jumps. Only a running lamp moves by itself.

## Roles
| Role | Use | Treatment |
|------|-----|-----------|
| `chassis` | window | graphite, lit top line |
| `bay` | the display; text fields | charcoal, inset |
| `plate` | a request; the open session's panel | raised |
| `key` / `keyDown` / `primary` | buttons at rest, held or engaged, main act | raised / inset |
| `overlay` | menus, dialogs | floating |
| `block` | code on the display | one tone up, no border |

Radii: key 5, plate 6, display 7, overlay 9; what is inside is never rounder than what holds it. Type 10–19. Density compact/regular/roomy changes control height (24/28/32), never type size.
States for every control: rest, hover, held, engaged (sunken), focused (keyboard ring), disabled (flat words, no surface), loading, error.

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
| 11 | Fable's review, steps 1–12 (`spec-fable.md`): what agents do in words, failure seen as failure, whole rows, one line of answers, a narrow window | to be shown |
