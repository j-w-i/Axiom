# CR-M1.6-DESIGN-001: allow OFL-1.1 font assets

Status: **Resolved — accepted by ADR-0013 §6 (2026-11-17).**

## Original assumption
ADR-0011 (as amended by ADR-0012) lists the licences allowed for dependencies:
MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, plus LGPL Qt modules (now retired). It
says nothing about non-code assets such as fonts.

## Evidence
The approved direction (D2, `docs/design/concepts.md` §6) bundles IBM Plex Sans and
IBM Plex Mono so that type renders identically on every OS; today the shell names
Inter and JetBrains Mono without bundling them, and each machine renders a
different fallback (`concepts.md` §1.3, weakness 10). IBM Plex is published under
the SIL Open Font License 1.1, which permits bundling and redistribution with
software, requires the licence text to accompany the fonts, and forbids selling the
fonts alone.

## Proposed change
Allow OFL-1.1 font files as application assets, under the conditions written in
ADR-0013 §6.

## Affected modules and contracts
`apps/desktop-tauri` (assets, About section, package notices). No code
dependency, protocol or module edge changes.

## Migration
M1.6-SYSTEM-002 adds the files with their `OFL.txt` and the notices.

## Workaround tried
System font stacks (the current state): rejected because metrics, and therefore
alignment and density, differ per OS.
