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
| **Environment** | where effects happen: folder, copy of it, sandbox, network policy | its own copy, no network | a permission mode |

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
projections they change (decision 0004): the session's row and what is open in it. Anything shown can be
rebuilt by replay. Before an effect its intent is on the drive: the tool call's event, and its entry in the
audit record (a chain: each entry holds the digest of the one before). The conversation as the model has it
is kept turn by turn beside the events, each result alone. A host that is killed goes on from that: a call
without a result is told to the model as unknown, and never done again. There is no outbox: nothing is
ordered that has not been begun.

## Crates (engine/)

| Crate | Owns | May depend on |
|-------|------|---------------|
| `axiom-proto` | wire types, framing, versions; fixtures for Dart | – |
| `axiom-journal` | SQLite event store, projections, recovery | proto |
| `axiom-policy` | capabilities, rules, grants, audit | proto |
| `axiom-spawn` | the only way to start a process; sandbox profiles, process trees | policy |
| `axiom-model` | codecs, transports, quirk profiles, usage | – |
| `axiom-tools` | native tools: planned, judged, then done exactly so | spawn, policy, proto |
| `axiom-engine` | native loop: context, compaction, asking a person | model, tools, policy, proto |
| `axiom-harness` | adapter contract, ACP client, native adapters (M3) | spawn, policy, proto |
| `axiom-orchestra` | delegation, tasks, worktrees (M5) | engine, harness |
| `axiom-host` | sockets, subscriptions, lifecycle | all |
| `axiom-cli` | command line | proto, host (its client and paths) |

A crate is created when its first real code is written. Edges not in this table need a decision.

## The stream to the screen

Source → item delta in memory → coalesced (per item, ≤ 33 ms) → journal batch → subscribers.
Each subscriber has a byte budget (8 MiB); one that falls behind is told to resynchronise: it follows again
from the last event it has, and reads the journal at its own pace. Nothing is buffered without limit.
There are no snapshots: a new client reads the history, in which ended items' deltas are folded into
their text. Of a command's output the journal keeps its beginning and its end, and a file in the agent's
scratch folder all of it up to 32 MiB; it is shown when the command ends.
In the client one isolate reads and decodes, the UI isolate applies a batch per frame, and the
transcript lays out only visible blocks (decision 0010).

## Failure

| Event | Behaviour |
|-------|-----------|
| Client dies | Host continues; next client reads the history, then the tail |
| Host dies | Journal is the truth; running attempts become `interrupted`, their process trees are killed by job object / process group / parent-death watch; nothing re-runs by itself |
| Machine loses power | WAL recovery. What a person did and what comes before an effect (session created or titled, prompt, tool call, run start, request and answer) is flushed to the drive before it is answered or acted on, and is never lost. Everything else (deltas, an item's or a run's ending, usage) is safe from a crash of the host, but what followed the last such flush may be lost; recovery then ends it as interrupted |
| The journal cannot be read or written | While another process locks it, writing waits: 10 s for all together, then every session that wants to write stops with a fault in words, in the client and in `host.status`. Other errors are tried a few times and cost only the session they meet. The next use opens a session again from the journal |
| The host's files are removed | It looks every 3 s and puts its socket, token, endpoint and locks back; if another host has them or the journal has gone, it stops in order |
| Harness dies or speaks nonsense | Attempt fails with the raw frames kept; the agent is resumable if the adapter said so |
| Provider interrupts a stream | Partial item kept and marked; retry policy per error class; tool calls cut short are never executed |

## Alternatives considered

A all-Dart (engine in isolates or a Dart process), B Rust in-process by FFI, C TypeScript host
reusing Pi/AI SDK code, D Rust host over a socket (chosen). See decision 0002 for the measurements.
