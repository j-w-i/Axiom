# Known issues and debts

| # | Issue | Owner / when |
|---|-------|--------------|
| K1 | Display-bound frame benchmarks (10 panes, 50 k blocks) not run; need CI or the owner's consent | M1 exit |
| K2 | Linux and Windows have compiled only in CI; nobody has looked at the app there | M1 exit |
| K3 | Anthropic's written position on orchestrators is not obtained (decision 0007) | owner, before M3 ships |
| K4 | Windows has no verified sandbox backend (decision 0008) | M2 spike |
| K5 | Provider long-tail facts (rate-limit headers, replay flags) are unverified (`research/providers.md` §8) | M2, per provider |
| K6 | `highlight.dart` is a one-line scanner, not a grammar | M4 |
