# ADR-0013: Visual identity "Precision Instrument" and its material grammar

Status: Accepted (2026-11-17), proposed by the M1.6 design package
(`docs/design/`) and approved by the project owner.

Amends ADR-0011 (licensing: OFL-1.1 font assets, §6 below). Supersedes the goals
and palette of `docs/migration/qt-to-tauri/design-system.md` §1–§2 ("Linear-grade
polish"). ADR-0012 (Tauri shell), the IPC protocol 0.1 and editor bridge 1 are
unaffected.

## Context

The M1.5 Tauri shell works and meets its Linux budgets, but its visual layer was
deliberately generic: cool low-chroma greys, an indigo accent, one surface tone,
and fades only (audit: `docs/design/concepts.md` §1). M1.6 asked for a
distinctive, polished, tactile identity, inspired by FL Studio's craft but
original, without costing editor responsiveness.

Three concepts were prototyped as isolated React + Monaco apps with real
repository content (`experiments/e5-visual-concepts/`), evaluated on weighted
criteria (A 7.85, B 7.40, C 7.60) and measured (`concepts.md` §3–§4). None won
outright, so a synthesis was proposed.

## Decision

**D1. Direction: the recommended synthesis, "Precision Instrument"**
(`docs/design/concepts.md` §5; reference screenshots
`docs/design/concepts/r-*.png`). This is Concept A's graphite and brass palette,
type, density and conventional topology, plus Concept B's instrument signals (task
transport with LED, elapsed time and output-rate meter; the hint line with keycaps;
engaged toggles sit in), plus Concept C's structural restraint (depth from tone and
hairlines, not bevels).

The rules this direction rests on, now normative for every shell change:

1. **Material grammar.** Every visible box is exactly one of: *raised* = you can
   press it; *recessed* = it holds a value or accepts input (fields, displays, the
   editor as the deepest well, engaged toggles); *flat* = content and structure
   (panels, headers, lists). Bevels and lit edges appear only on raised controls.
   Overlays are flat surfaces lifted by the single popover shadow.
2. **Palette.** Themes are generated from a few inputs (`themes.json`) with
   contrast clamping; state hues (ok, run, warn, danger, attention) are fixed slots
   that re-tinting cannot remove; the brass accent marks focus, the active item,
   running, the primary action and attention, and never decorates.
3. **Motion policy.** Animate only `transform` and `opacity` (colour ≤ 90 ms for
   hover and selection). One layout commit per panel toggle, with motion applied to
   content. Nothing on the input path. Loops only while their state is true,
   stepped (≤ 8 updates per second), at most three visible, paused when the window
   is hidden or unfocused. Standard, Reduced and Off levels; the OS
   reduced-motion setting always wins. Measured basis: in the prototype, smooth loops
   cost 18% of a core and stepped loops 4.4% (`concepts.md` §4).
4. **Scope rule.** M1.6 builds only surfaces backed by existing M1 functionality.
   The AI workspace (M3), Environment Manager and activity rail (M2), and the
   Problems and Terminal views (M2/M4) are designed (`workspace-spec.md` §5) and
   built by their own milestones (governance rule 9).
5. **Budgets.** P1–P12 of `docs/design/performance.md` §3 are adopted unchanged.

**D2–D10.** The owner accepted every recommendation of `concepts.md` §6 as written:

| # | Decision | Answer |
|---|----------|--------|
| D1 | Visual direction | Recommended synthesis, "Precision Instrument" |
| D2 | Typefaces | Bundle IBM Plex Sans (400/500/600) and IBM Plex Mono (400/500), IBM's own Latin1 split WOFF2 files, unmodified, about 180 kB (to be measured), OFL-1.1 (§6) |
| D3 | Window chrome | macOS: overlay title bar with the traffic lights inset into the title row. Windows and Linux: native decorations, themed dark or light with the setting. A custom frame is deferred |
| D4 | Activity rail | None until a second real destination exists (M2) |
| D5 | Motion setting | `appearance.motion` = `standard` / `reduced` / `off`, as a shell-side settings key (no protocol change) |
| D6 | Stderr | No longer painted red: gutter glyph, muted colour and the hidden "stderr:" prefix; only failure status lines use the danger colour |
| D7 | Tab reordering | Included in M1.6 (M1.6-INTERACTION-004) |
| D8 | Tooltip | Adopt `@radix-ui/react-tooltip` (MIT) |
| D9 | Delivery | Restyle in place, task by task, on branches merged in dependency order; no dual-design feature flag; legacy token aliases removed by M1.6-QA-006 |
| D10 | Light theme | Shipped with equal care; light warning text darkened to ≥ 4.5:1 |

Owner's answers, recorded verbatim from the session of 2026-11-17:

- D1: "Alright I agree synthesis is the one".
- D2–D10: answered "Accept all (Recommended)" to the question asking whether the
  recommendations for D2–D10 were accepted as written.

## Consequences

- `docs/design/*.md` are normative for M1.6. When they and this ADR disagree, this ADR
  wins; when they and a task file disagree, the design documents win.
- No M1.6 task file needed amendment: each was written against these
  recommendations (D2 → SYSTEM-002, D3 → SHELL-003 and SURFACES-003, D4 → SHELL-002,
  D5 → INTERACTION-001 and SURFACES-002, D6 → SHELL-006, D7 → INTERACTION-004,
  D8 → SYSTEM-004, D9 → SYSTEM-001 and QA-006, D10 → SURFACES-003).
- D3 adds one host capability (`core:window:allow-set-theme`), recorded in
  `docs/architecture/security.md` by M1.6-SURFACES-003, and a `tauri.conf.json` change
  on macOS (M1.6-SHELL-003). No other host change.
- Two additive presentation fields in `TasksStore` (`startedAt`, `outputSeq`); no
  protocol change.
- Identity risk accepted: the topology is close to a conventional IDE, and Zed
  also bundles IBM Plex. Distinctiveness rests on the palette, the material grammar,
  the transport and the hint line, and is judged in M1.6-QA-006 against the brief's
  success criteria (`creative-brief.md` §7).
- Verification on macOS and Windows inherits the M1.5 hardware/CI blocker
  (M1.6-QA-005).

## 6. Licensing amendment to ADR-0011

Font files under the **SIL Open Font License 1.1** may be bundled as application
assets. They are data, not code, and are not linked. Conditions:

- each font family ships with its `OFL.txt` next to the files
  (`apps/desktop-tauri/src/assets/fonts/<family>/OFL.txt`), and the About section
  and package licence notices list them;
- the fonts are not sold on their own;
- for IBM Plex, whose licence reserves the font name "Plex", only the copyright
  holder's **unmodified** files are bundled, using IBM's own published split
  subsets (`fonts/split/woff2/`). Axiom does not subset, convert or modify them,
  because a modified version could not carry the reserved name. A family with a
  reserved name that needs modification must be renamed per OFL §3;
- code dependencies remain restricted to the ADR-0011/0012 licences (`cargo deny`
  and the npm licence review are unchanged).

Resolves `docs/decisions/proposed/CR-M1.6-DESIGN-001-ofl-fonts.md`.

## Alternatives considered

Concepts A, B and C as prototyped; system fonts; a custom window frame on every
OS; a `design=v2` feature flag. Reasons: `docs/design/concepts.md` §2–§6.
