# Known issues and debts

| # | Issue | Owner / when |
|---|-------|--------------|
| K1 | Display-bound frame benchmarks (10 panes, 50 k blocks) not run; need CI or the owner's consent | M1 exit |
| K2 | Linux and Windows have compiled only in CI; nobody has looked at the app there | M1 exit |
| K3 | Anthropic's written position on orchestrators is not obtained (decision 0007) | owner, before M3 ships |
| K4 | Windows has no verified sandbox backend (decision 0008): commands are refused there; reading and changing files works | M2 spike |
| K5 | Provider long-tail facts (rate-limit headers, replay flags) are unverified (`research/providers.md` §8) | M2, per provider |
| K6 | `highlight.dart` is a one-line scanner, not a grammar | M4 |
| K7 | Windows uses a Unix socket too (Dart ≥ 3.11 and `uds_windows`); proved by CI only. The peer's identity is not checked there and directories get no ACL of their own: keep `AXIOM_DATA` inside the user's profile | M2 security tests |
| K8 | The macOS window (transparent title bar, traffic lights, dragging by the header) has been looked at, and dragged by the owner ("it works"); full screen is not known to have been tried | M1 exit |
| K9 | `scripts/package.sh` puts `axiom-host` and the CLI into the bundle. Built and its host run on macOS; not run on Linux or Windows | M7 |
| K50 | The system's window for choosing a folder (`file_selector`) has not been opened on any system | K8 |
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
| K22 | Schema 2, and 3 and 4 after it, are one-way; hosts from before it and this one exclude each other by `run/lock` | accepted before 1.0 |
| K23 | The client lists the last 1 000 sessions | M4 |
| K24 | Running lamps turn, so a frame is drawn on every refresh while an agent runs (6 paints of 12 × 10 px, nothing rebuilt); one scrolled just out of sight still counts. Reduced motion stops it | M2: measure power |
| K25 | A table being written costs up to 20 ms per frame in a debug build near its cut at 4 000 characters | M2 |
| K26 | Lamps do not grow with the text, only with the zoom; icons do, and in a key of fixed size only as far as it has room | M2 accessibility |
| K27 | 64 clients that follow and never read hold about 8 MiB each | M7 soak |
| K28 | If the journal's file is moved away the host stops, and the endings of what ran are lost | accepted |
| K29 | A write lock held by another program during a stop costs up to 10 s, and endings may then be unwritten | accepted |
| K30 | Engine, third review pass: a refused event leaves a replay's run open, a second lock soon after an exhausted outage fails at once, U+2060–U+206F count as emoji in the CLI's prose; four behaviours have no test (connection cap, growing script, lock inode, runtime shutdown limit) | M2 |
| K31 | Windows, Unix sockets. (1) When one end shuts a socket while the other begins to wait for it the wake-up can be lost and both wait for good: the host's threads look instead of waiting (stress: 31 rounds without a hang, before that 2 to 16), and the app leaves a host that is silent for 30 s. (2) A Unix socket that Dart itself listens on can bring the machine to a stop when it is closed: only tests did that, and they go through `examples/relay.rs` there. Dart's own client socket could still lose a wake-up; then (1)'s 30 s apply | accepted |
| K32 | The independent security review of M2 has not been done: the reviewing agent was stopped by its provider's safeguards before it began. Until it is, take the sandbox as tested by its author only | before M2 ends |
| K33 | Linux: a protected name that does not exist yet can be made by a command; it is told to the person afterwards | accepted |
| K51 | Linux: inside the sandbox no process can make a socket of this machine (pairs apart), as on macOS. What needs one there fails and says so: Python's `forkserver` (its default from 3.14), `Manager` of `multiprocessing`, servers that listen on a path. The kernel's Landlock can tell paths apart from version 9 on; until then it is all or none. The filter exists for x86-64 and ARM64; on another processor nothing is started | M7 |
| K34 | macOS: a process that closes every file it got, loses its parent and leaves its group between two looks outlives its command. It stays contained | accepted |
| K40 | Delegated agents (Claude Code, Codex, ACP) have been driven by scripted programs only. The owner said yes to one start of the real Claude Code on 2026-11-25; there was none on the computer to start (the Claude desktop app's own copy is not one a person installed). `host/tests/live.rs` does it, one start, tried out on the scripted program. Claude Code can only look or work, not ask first, because the switch for asking is not in its documentation | Claude Code installed and signed in |
| K52 | Dropping a folder: the windows' own code (Swift, GTK, Win32) has been built and tried by nobody: no drag has been made on any system. Under zoom the system's own buttons on macOS do not grow, the wheel moves twice as far at 200 %, and the smallest window is then tight. Fonts really installed were not tried | K8 |
| K53 | Bedrock is built on Amazon's documentation (read 2026-11-25) and has been tried by nobody. Not stated there, so assumed: that turns must alternate, the words of a prompt that is too long, that blank text is refused, how a call without arguments comes, whether a signature may come in pieces. Models are named by hand: the list is at another origin, where the key does not go (SI-4), so the app cannot offer effort for them. No SigV4, no VPC endpoint but by private DNS | owner's key; K32 for the list |
| K54 | A branch, as any agent, cannot be removed. Programs (Claude Code, Codex, ACP) cannot be branched: their own ways to fork a session are M3's | M3, M5 |
| K49 | What others came to begins at most 8 runs of an agent in a row without a person's word; how much they may cost is not limited yet (budgets, M6) | M6 |
| K48 | Copies for agents: nothing removes one by itself, and a session has no end that would; `.axiom-work` in the home folder grows until a person throws them away. Agents cannot commit in them. On Windows links are not copied, and nothing there has run but the unit tests that compile. A folder that is itself a git worktree or submodule is copied without its history | M5 |
| K47 | Signing in with ChatGPT is built on OpenAI's documentation (read 2026-11-25, a preview) and tried against a server of the tests only: nobody has signed in. How long what renews is, and so whether Windows' store takes it, is not documented; if it does not, the sign-in fails and is taken back. A crash between OpenAI's answer and the store loses the sign-in: one signs in again. How large a model's context is, OpenAI does not say there | the owner signs in once |
| K46 | Claude Code with Axiom's tools in place of its own (`--tools ""`, our server, nothing else allowed) rests on its documentation and has been tried with a scripted program only. If it says at its start that it has any other tool, it is ended. Whether a plan may be used so by a product is not certain: decision 0007's written confirmation is wanted before launch | the owner's yes; before launch |
| K45 | Programs are looked for in the usual folders only (Homebrew, `/usr/local/bin`, `~/.local/bin` and a few more); not where nvm, asdf or an own npm prefix put them | M3 |
| K44 | Several accounts of one program: Claude Code only, whose documentation says that logins are kept apart by folder. Codex documents `CODEX_HOME`, but not whether it parts what is in the system's store. Not tried with a real login | the owner's yes |
| K41 | Installed programs are given Axiom's `agent` tool where their own documentation says how: Claude Code that works, and ACP version 1. Not Codex (the form is documented nowhere), not ACP version 2 (a preview). Tried with a scripted program, not with a real one | the owner's yes |
| K42 | Windows: programs installed as `.cmd` are not found, the environment a harness gets is too small, and process tests do not run there | M3 |
| K43 | Fetching a page is one request for one address that a person is asked about; commands still have no network at all | accepted until the proxy, M4 |
| K37 | A provider that sends keep-alive comments for ever holds an answer open: only silence has a limit | M2 |
| K38 | macOS: a build that is not signed is another program to the keychain each time it is made, so its keys have to be added again. It never asks | signed builds, M7 |
| K39 | A folder below one that is protected by name (`.claude/worktrees/x`, say) cannot be a workspace an agent writes to | M5, Axiom's own worktrees |
