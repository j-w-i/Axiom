# Known issues and debts

| # | Issue | Owner / when |
|---|-------|--------------|
| K1 | Display-bound frame benchmarks (10 panes, 50 k blocks) not run; need CI or the owner's consent | M1 exit |
| K2 | Linux and Windows have compiled only in CI; nobody has looked at the app there | M1 exit |
| K3 | Anthropic's written position on orchestrators is not obtained (decision 0007) | owner, before M3 ships |
| K4 | Windows has no verified sandbox backend (decision 0008) | M2 spike |
| K5 | Provider long-tail facts (rate-limit headers, replay flags) are unverified (`research/providers.md` §8) | M2, per provider |
| K6 | `highlight.dart` is a one-line scanner, not a grammar | M4 |
| K7 | Windows uses a Unix socket too (Dart ≥ 3.11 and `uds_windows`); proved by CI only. The peer's identity is not checked there and directories get no ACL of their own: keep `AXIOM_DATA` inside the user's profile | M2 security tests |
| K8 | The macOS window (transparent title bar, traffic lights, dragging by the header) is built but has not been looked at: it needs a real window and the owner's yes | M1 exit |
| K9 | The application bundle does not contain `axiom-host` yet; set `AXIOM_HOST` | M2 packaging |
| K10 | Keycaps use ⌘, which Archivo lacks: the system font supplies it on macOS, unverified elsewhere | M1 exit |
| K12 | Markdown: quotes do not nest blocks; HTML is shown as text; a link is styled but nothing opens it yet (it will ask first) | M2 |
| K13 | The transcript's ends are reached by moving its origin; there is no scroll bar, because a lazy list cannot know its length | M2: measured extents |
| K14 | Selecting text across rows that are not built copies only what is built; a selection in the block being written is lost when that block closes | M2 |
| K15 | A code block over 120 lines scrolls sideways in parts of 120 lines | M2 |
| K16 | Sessions are keyed by session id only; the workspace is a label until workspaces have ids (SI-21) | M2 policy |
| K17 | A journal restored from a backup whose numbers have caught up is not noticed: the journal has no identity on the wire | M2 |
| K18 | The host does not raise its open-file limit; its caps keep it near 150 descriptors | M2 spawn |
| K19 | A process of the same user that floods the socket makes some new connections fail; silent ones keep nobody out | accepted, same user only |
| K20 | Text dropped past 1 MiB is marked after a crash but not counted | accepted |
| K21 | A request that can be answered and is too long to show is refused by the host with a typed error; the engine must tell the agent so | M2 engine |
| K22 | Schema 2 is one-way; hosts from before it and this one exclude each other by `run/lock` | accepted before 1.0 |
| K23 | The client lists the last 1 000 sessions | M4 |
| K24 | Running lamps turn, so a frame is drawn on every refresh while an agent runs (6 paints of 12 × 10 px, nothing rebuilt); one scrolled just out of sight still counts. Reduced motion stops it | M2: measure power |
| K25 | A table being written costs up to 20 ms per frame in a debug build near its cut at 4 000 characters | M2 |
| K26 | Icons and lamps do not grow with the text | M2 accessibility |
| K27 | 64 clients that follow and never read hold about 8 MiB each | M7 soak |
| K28 | If the journal's file is moved away the host stops, and the endings of what ran are lost | accepted |
| K29 | A write lock held by another program during a stop costs up to 10 s, and endings may then be unwritten | accepted |
| K30 | Engine, third review pass: a refused event leaves a replay's run open, a second lock soon after an exhausted outage fails at once, U+2060–U+206F count as emoji in the CLI's prose; four behaviours have no test (connection cap, growing script, lock inode, runtime shutdown limit) | M2 |
| K31 | Windows, Unix sockets. (1) When one end shuts a socket while the other begins to wait for it the wake-up can be lost and both wait for good: the host's threads look instead of waiting (stress: 31 rounds without a hang, before that 2 to 16), and the app leaves a host that is silent for 30 s. (2) A Unix socket that Dart itself listens on can bring the machine to a stop when it is closed: only tests did that, and they go through `examples/relay.rs` there. Dart's own client socket could still lose a wake-up; then (1)'s 30 s apply | accepted |
