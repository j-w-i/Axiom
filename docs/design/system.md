# Design system

Source of truth: `app/lib/ui/tokens.dart`. Gallery: `app/lib/gallery/`. Images: `flutter test` writes `app/build/shots/` (offscreen).

**The idea.** A professional instrument: navy-steel structure, one recessed deep-navy display, cobalt where something is live, a service's hue where something is its own.

## Rules
1. **Containment is spent, not given.** A surface may hold another surface only if the person acts on it as a unit (a key, a request, a menu, the display, the open session). Lists, selectors and activity are text on the surface they belong to. (Round 3 failed on this; the owner named it: "something is off about most of the app", and the plain sidebar was the part that worked.)
2. **Three depths.** Chassis; display (below); session panel, keys, requests (above). Overlays float. Nothing else has depth.
3. **One light, from above.** Raised parts have a faint lit top edge and a 1 px shadow; recessed parts a 5 px shade. Gradients span 2–4 %. No gloss, glow, engraving, side bevels or blur.
4. **Cobalt means live**: running, focus, the primary act, the selected tab. Never decoration, never sky; levels are neutral. Amber asks, red failed, green done; each state has its own shape (tested).
4a. **Things take the hue of the service they belong to.** Its mark, level, dropdown edge, check and hover tint; a selected agent's edge and its tab mark. Kinds of work have hues of their own (read, change, search, run, web, think), and code and diffs are in colour. Marks identify a service really in use (`THIRD_PARTY.md`); unknown models get none.
5. **Grid of 4.** Text starts on a shared left edge per surface (display 24, panel 12). Icons are 12–14 in a fixed column; text uses even leading so glyphs and words share a centre.
6. **Two voices.** Archivo for words; Spline Sans Mono only for literals: code, paths, commands.
7. **Numbers say where they come from.** Solid meter = reported or measured; dashed = estimated; empty track and words = unknown.
8. **Controls appear where the hand is.** A row shows its keys when hovered; state is always shown.
9. **Things sit inside what owns them.** Agents and plan in their session; model, harness, connection, effort and access beside their agent; the composer is only for writing.
10. **Motion** is springs (`snap`, `glide`) that can be interrupted; nothing waits for them; reduced motion jumps. Only a running lamp moves by itself.

## Roles
| Role | Use | Treatment |
|------|-----|-----------|
| `chassis` | window | graphite, lit top line |
| `bay` | the display; text fields | navy, inset |
| `plate` | the open session's panel | raised |
| `key` / `keyDown` / `primary` / `answer` | buttons at rest, held or engaged, main act, answering a request | raised / inset |
| `request` | something needs a person | warm graphite, raised |
| `overlay` | menus, dialogs | floating |
| `block` | code on the display | one tone up, no border |

Radii 3–7. Type 10–19. Density compact/regular/roomy changes control height (24/28/32), never type size.
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
| 6 | hues by service and by kind of work, marks, richer code and states (`img/round6-*.png`) | **accepted by the owner, 2026-11-25** |
