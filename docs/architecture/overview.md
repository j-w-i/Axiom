# Architecture

```
┌────────────────────────┐        local socket            ┌──────────────────────────────────────┐
│ axiom (Flutter)        │   peer identity + launch token  │ axiom-host (Rust)                    │
│ design system, stores, │ ◀────── JSON lines ───────────▶ │ journal · sessions · policy          │
│ transcript, deck       │                                 │ native engine · adapters · tools     │
└────────────────────────┘                                 └──┬──────────┬──────────┬─────────────┘
┌────────────────────────┐                                    │ spawn    │ spawn    │ HTTPS
│ axiom (CLI)            │ ◀──────────── same wire ──────────┤          │          │
└────────────────────────┘                              sandboxed    vendor      providers,
                                                        tool procs   harnesses   local servers
```

Two kinds of process. The **client** draws and takes input: it opens no network connection, touches no workspace file, and
starts nothing but the host (SI-14). The **host** owns everything else and outlives the window.
Why two processes, why Rust, why this wire: decisions 0002–0004, measured in `prototypes/`.

## Five things, never one

| Thing | Is | Example | Is not |
|-------|----|---------|--------|
| **Provider** | who serves inference, and its wire codec + quirk profile | Anthropic, OpenRouter, the LM Studio on this Mac | a login |
| **Model** | a capability set at a provider | `claude-opus-5-5` | a provider |
| **Connection** | one account or key, with a lane (key, plan sign-in, installed harness), identity and limits; any number per provider | "Claude Max, personal" | a provider setting |
| **Harness** | who runs the loop | Axiom native, Codex, Claude Code | a model |
| **Environment** | where effects happen: folder, worktree, sandbox, network policy | worktree `refresh-guard`, no network | a permission mode |

An **agent** is one choice of each, plus effort, tools, access and a parent. Illegal combinations are
computed from declared capabilities and explained in the menu, never silently repaired.

## Domain

`Workspace > Session > Agent > Run > Item`, plus `Request` and `Task`.

- **Session**: a piece of work, project-bound or global. **Agent**: a participant (above).
- **Run**: one activation of an agent, from prompt to idle; a retry is a new *attempt* in the same run.
- **Item**: prose, reasoning, tool call, output, diff, plan, notice. Addressed by id; updates are upserts,
  after ACP v2, so steering mid-run, background work and resumption need no special cases.
- **Request**: something a person must answer. Carries `live | message | not_resumable`, so a restart
  never leaves a dialog whose answer goes nowhere.
- **Task**: a node of the session's plan, with dependencies and a gate.

State changes are **events** in an append-only journal, committed in one transaction with the
projections they change and the effects they order (decision 0004). Anything shown can be rebuilt by replay.

## Crates (engine/)

| Crate | Owns | May depend on |
|-------|------|---------------|
| `axiom-proto` | wire types, framing, versions; fixtures for Dart | – |
| `axiom-journal` | SQLite event store, projections, recovery | proto |
| `axiom-policy` | capabilities, rules, grants, audit | proto |
| `axiom-spawn` | the only way to start a process; sandbox profiles, process trees | policy |
| `axiom-model` | codecs, transports, quirk profiles, usage, catalogue (M2) | proto |
| `axiom-tools` | native tools (M2) | spawn, policy |
| `axiom-engine` | native loop: context, compaction, scheduling (M2) | model, tools, journal |
| `axiom-harness` | adapter contract, ACP client, native adapters (M3) | spawn, proto |
| `axiom-orchestra` | delegation, tasks, worktrees (M5) | engine, harness |
| `axiom-host` | sockets, subscriptions, lifecycle | all |
| `axiom-cli` | command line | proto |

A crate is created when its first real code is written. Edges not in this table need a decision.

## The stream to the screen

Source → item delta in memory → coalesced (per item, ≤ 33 ms) → journal batch → subscribers.
Each subscriber has a byte and event budget; one that falls behind is told to resynchronise from a
snapshot rather than buffering without limit. Process output is a separate channel delivered only
while something displays it; the journal keeps a bounded head and tail, the full text goes to a file.
In the client one isolate reads and decodes, the UI isolate applies a batch per frame, and the
transcript lays out only visible blocks (decision 0010).

## Failure

| Event | Behaviour |
|-------|-----------|
| Client dies | Host continues; next client gets a snapshot then the tail |
| Host dies | Journal is the truth; running attempts become `interrupted`, their process trees are killed by job object / process group / parent-death watch; nothing re-runs by itself |
| Machine loses power | WAL recovery; the last ≤ 33 ms of deltas may be lost, a committed tool effect never is (intent is journalled first) |
| Harness dies or speaks nonsense | Attempt fails with the raw frames kept; the agent is resumable if the adapter said so |
| Provider interrupts a stream | Partial item kept and marked; retry policy per error class; tool calls cut short are never executed |

## Alternatives considered

A all-Dart (engine in isolates or a Dart process), B Rust in-process by FFI, C TypeScript host
reusing Pi/AI SDK code, D Rust host over a socket (chosen). See decision 0002 for the measurements.
