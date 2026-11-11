# ADR-0008: Visual inspection — stamp at build time, inspect through adapters, verify objectively

Status: Accepted (2026-11-11); protocol is draft until M5

## Context
The defining feature: click a rendered element, get its source, let an agent
change it, and *prove* the change did what was asked. Research:
`docs/research/visual-inspection.md`. Experiments E2 and E3.

Key findings:
- React 19 removed the fiber `_debugSource` field that older click-to-source
  tools relied on (E3 observed `undefined`); owner stacks give component
  context, not exact host-element locations. Runtime internals are not a
  stable basis.
- Stamping elements at build time with their source location is exact,
  framework-version independent and cheap (E3: exact file:line:column for a
  selected button; correct handling of a repeated component; hot reload in
  ~100 ms; geometry delta equal to the intended 24 px).
- QML records instantiation line/column for every object, reachable through
  private API (E2, on Qt 6.11.2 and 6.12.0); public API gives the file only.
- Some frameworks expose no usable source metadata (SwiftUI on device,
  release builds). No universal mechanism exists.

## Decision
1. **One normalised model and adapter protocol**
   (`crates/protocol/src/inspection.rs`, `docs/protocols/inspection.md`),
   capability-driven: adapters declare what they can do; consumers degrade.
2. **Source mapping prefers compile-time facts.** Order of trust:
   build instrumentation → framework runtime metadata → source maps →
   workspace heuristics → visual inference. Every candidate carries its
   method, relation and a confidence bounded by its method. Uncertain
   mappings are presented as uncertain and never auto-patched.
3. **Web first** (React + Vite), implemented as (a) a dev-only build plugin
   that stamps host elements, and (b) CDP-driven inspection of the page in
   an isolated WebEngine profile. **QML second**, as an in-process probe, to
   prove the protocol is not web-shaped.
4. **Verification is assertion-based.** Before patching, the agent states
   checks (geometry relations, property values, expected change region).
   After reload the daemon re-identifies the element, evaluates the checks,
   and compares screenshots for unintended change. Verdicts: `verified`,
   `failed`, `inconclusive`. Compilation success and "pixels changed" are
   never sufficient for `verified`.
5. The model is not flattened to the web: `properties` keep framework
   vocabulary under namespaced keys, surfaces can nest, and node kinds
   distinguish rendering elements from logical components.

## Alternatives considered
- *Rely on React DevTools internals / fiber fields.* Broke with React 19.
- *Source maps alone.* Map generated JS positions, but a DOM node has no
  generated position without instrumentation; useful for CSS rules and as a
  supplement only.
- *Screenshot + VLM only.* Works everywhere, proves nothing; kept as the
  lowest tier (Level 1) with capped confidence.
- *Accessibility trees as the universal Level 2.* Good fallback for opaque
  native apps (planned as a generic adapter), but carries no source data.
- *Reuse GammaRay for Qt.* GPL-2.0-or-later/commercial; a small purpose-built
  probe under our licence is simpler than depending on it.

## Consequences
- Level 3 on the web needs our plugin in the project's dev build. Axiom adds
  it at preview launch through a generated Vite config wrapper rather than
  editing the user's config (designed in M5); other bundlers (webpack, Next,
  SWC-only pipelines) need their own plugin variants and are separate tasks.
- The QML probe depends on private Qt headers: built per Qt minor version,
  with a public-API fallback at file precision.
- Remaining unverified link, scheduled as the first M5 task: driving Qt
  WebEngine's Chromium over CDP with the calls E3 used against Chrome.
