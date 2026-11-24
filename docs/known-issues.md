# Known issues and debts

| # | Issue | Owner / when |
|---|-------|--------------|
| K1 | Display-bound frame benchmarks (10 panes, 50 k blocks) not run; need CI or the owner's consent | M1 exit |
| K2 | Linux and Windows have compiled only in CI; nobody has looked at the app there | M1 exit |
| K3 | Anthropic's written position on orchestrators is not obtained (decision 0007) | owner, before M3 ships |
| K4 | Windows has no verified sandbox backend (decision 0008) | M2 spike |
| K5 | Provider long-tail facts (rate-limit headers, replay flags) are unverified (`research/providers.md` §8) | M2, per provider |
| K6 | `highlight.dart` is a one-line scanner, not a grammar | M4 |
| K7 | Windows uses a Unix socket too (Dart ≥ 3.11 and `uds_windows`); proved by CI only. The peer's identity is not checked there: the directory's ACL and the token are | M2 security tests |
| K8 | The macOS window (transparent title bar, traffic lights, dragging by the header) is built but has not been looked at: it needs a real window and the owner's yes | M1 exit |
| K9 | The application bundle does not contain `axiom-host` yet; set `AXIOM_HOST` | M2 packaging |
| K10 | Keycaps use ⌘, which Archivo lacks: the system font supplies it on macOS, unverified elsewhere | M1 exit |
| K12 | Markdown: quotes do not nest blocks; HTML is shown as text; a link is styled but nothing opens it yet (it will ask first) | M2 |
| K13 | The transcript's ends are reached by moving its origin; there is no scroll bar, because a lazy list cannot know its length | M2: measured extents |
| K14 | Selecting text across rows that are not built copies only what is built | M2 |
| K15 | A code block over 120 lines scrolls sideways in parts of 120 lines | M2 |
