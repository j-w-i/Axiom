# CR-M1.6-SYSTEM-001: the control outline cannot meet 3:1 with the approved palette

_Status: **accepted by the owner, option A** (2026-11-17). Raised by M1.6-SYSTEM-001.
Implemented in the same task: `--line-input` (`graphite-500`) is generated (dark `#6c6962`,
light `#86827c`), `design-system.md` §2.2/§2.3/§4 name it, and `tests/tokens.test.ts`
checks it at 3:1 against editor, panel, chrome and well. SYSTEM-003, SHELL-003 and SHELL-006
take the field border from §4._

## Original assumption

`docs/design/design-system.md` §2.3, last row:

> `--line` (control outline) against the surface it sits on — 3:1 for inputs
> (WCAG 1.4.11); hairlines are decorative

and M1.6-SYSTEM-001 requirement 6(a): "every pair in §2.3 meets its minimum in both
themes", enforced by `tests/tokens.test.ts`.

## Evidence against it

The approved values (§2.2, ADR-0013; prototype `experiments/e5-visual-concepts`,
variant r) put `--line` a few lightness steps from every surface, by design: it is
the seam between regions. WCAG contrast of `--line` against the surfaces a field
can sit on (computed with the guard test's own function):

| Theme | `--line` | on well | on panel | on chrome | on editor |
|-------|----------|--------:|---------:|----------:|----------:|
| dark | `#0a0a0b` | 1.06 | 1.20 | 1.08 | 1.11 |
| light | `#c3c0ba` | 1.82 | 1.55 | 1.43 | 1.76 |

No lightness of a single `--line` reaches 3:1 against these surfaces without
stopping being a seam: in dark it would have to be lighter than `--text-faint`.
The fill of a field (`--surface-well`) is also within 1.1–1.2:1 of the panel in
dark, so the field's boundary is not identifiable by fill either. The design
relies on the recessed material (inset shadow + lit lower edge) to show a field,
which a contrast ratio does not measure.

So either §2.3's row is wrong (it describes an outline the palette does not have),
or the palette lacks a token for input boundaries.

## Proposed change (choose one)

**A. Add an input-boundary token (recommended).** `--line-input`, generated per
theme at ≥ 3:1 against `--surface-panel`, `--surface-chrome` and
`--surface-editor` (for example dark `#6f6c66` = 3.15–3.58:1 and light `#827e78` = 3.18–4.03:1 against panel, chrome, editor and well), used only
as the border of fields, the command field, the console prompt and other
value-holding controls (SYSTEM-003, SHELL-003, SHELL-006). `--line` stays the
seam. §2.3's row is rewritten to name `--line-input`, and the guard test checks it.
Cost: fields look slightly more outlined than in the prototype.

**B. Accept the material as the boundary.** Rewrite §2.3's row: fields are
identified by the recessed material and their position; `--line` is decorative.
The guard test does not check it. Cost: WCAG 1.4.11 is argued rather than
measured, and low-vision users may not see field edges in dark.

## Affected modules and contracts

`docs/design/design-system.md` §2.2/§2.3; `apps/desktop-tauri/scripts/gen-themes.mjs`
and `tests/tokens.test.ts` (A only); the field recipes in SYSTEM-003, SHELL-003,
SHELL-006. No protocol or bridge contract.

## Migration

A: add the token to the generator (one calibration entry per theme), add the pair to
the test, use it in the field components as they are restyled. B: documentation only.

## Workaround in place

None needed: decided before the task finished.
