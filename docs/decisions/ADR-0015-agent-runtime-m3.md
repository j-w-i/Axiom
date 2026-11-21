# ADR-0015: Agent runtime (M3): protocol 0.3, crates, credentials, checkpoints, clients, MCP

Status: **Accepted** (2026-11-21). Written by the M3 planning pass and accepted under the owner's
standing instruction to work M3–M5 autonomously ("Work all the way through to M5 autonomously",
2026-11-21). The owner may reopen any point; each is recorded so that a change is a new ADR, not
a quiet edit. Refines ADR-0007; does not supersede it.

## Context

M3 delivers a conversation with a local or remote model that reads the workspace, proposes and
applies patches under a permission policy, runs builds, can be resumed after a restart, and that
external agents can drive through MCP (`docs/roadmap/milestones.md` §M3). ADR-0007 fixed the
shape (in-house loop in the daemon, one gated tool registry, shadow-git checkpoints, JSONL
sessions, MCP both ways). `docs/protocols/agents.md` is a draft. This ADR records what the M3
task specs depend on. Windows support is deferred by the owner (2026-11-21): nothing here may
preclude it, but Windows-only work is not in M3.

## Decisions

### 1. Protocol 0.3: additive `agent/*` methods
IPC 0.1 and the 0.2 additions stay frozen. The daemon reports **0.3**; clients feature-detect
through `InitializeResult.methods`. Exact types are written by M3-ARCH-001 into
`crates/protocol/src/agent.rs`.

| Method / notification | Purpose |
|-----------------------|---------|
| `agent/models` | configured model providers, their models and capability profiles |
| `agent/configureProvider` | add/replace/remove a provider configuration (credential goes to the keychain, never the settings) |
| `agent/sessionCreate` | new session for a workspace (provider, model) |
| `agent/sessionList` | sessions of a workspace, newest first |
| `agent/sessionOpen` | attach this connection to a session; returns its events so far (replay for the UI) |
| `agent/send` | append a user message and run the loop until the turn ends |
| `agent/cancel` | stop the running turn (stream and tools) |
| `agent/approve` | answer an approval request: `once`, `session`, `deny` |
| `agent/checkpoints` | the session's checkpoints |
| `agent/restore` | restore a checkpoint; per-file conflicts are reported, never overwritten |
| `agent/tools` | the tool registry (name, description, schema, effect) |
| `agent/toolCall` | call one registry tool as an external principal (the MCP front end); same gate |
| `agent/policy` / `agent/setPolicy` | read / replace the user policy (settings `agent.policy`) |
| `agent/event` (notification) | one transcript event, streamed to attached connections |
| `agent/approvalRequested` (notification) | a pending approval, to connections that can approve |

New error kinds: `approval_required` (an `ask` with no connection able to approve: treated as
deny, with a hint), `checkpoint_conflict` (details per file), `model_error` (details
`{kind: auth|rate_limited|context_too_long|unavailable|invalid_request|transport}`).
`approval_denied` (M2) is reused for a denied tool call. `InitializeParams` gains an optional
`authToken` (decision 6) and `capabilities: {approvals: bool}`; both optional, so 0.1/0.2 clients
are unaffected.

### 2. Crates and module edges
- **`crates/agent`** (`axiom-agent`): message model, `ModelProvider` and the adapters, tool
  registry types, permission gate, checkpoints, session journal, compaction, the loop, the MCP
  codec (server and client halves). Depends on `protocol`, `store`. It does **not** depend on
  `workspace`, `exec`, `env` or `build`: tools are registered by `axiomd` as closures that call
  those services (`modules.md` "agent → none"). Tool *logic* that is not a service call
  (search/replace matching, result bounding, truncation cursors) lives in `agent`.
- **`axiomd`** gains a `tools` module: one thin adapter per tool (validate → one service call →
  typed result), the same rule as handlers. Edge `axiomd → agent`.
- **`apps/cli`** (`axiom` binary, crate `axiom-cli`): a protocol client. Edges `cli → protocol,
  rpc` (it reuses the NDJSON codec and the typed client in `axiom-rpc`). No business logic.
- `workspace` gains `search` (text/regex over the ignore-filtered tree, `grep-searcher` +
  `grep-regex`, MIT/Unlicense) for `workspace.search`; that is a workspace-service operation.

### 3. Dependencies (all MIT/Apache-2.0 or compatible, `cargo deny` checked)
- HTTP: `reqwest` (already used by `env`, rustls) with streaming bodies; SSE is parsed in-house
  (a 60-line line parser; no SSE crate). No multi-provider SDK crate (ADR-0007 §2).
- Credentials: `keyring` (v3) with the platform-native stores: Secret Service over pure-Rust
  D-Bus on Linux, Keychain on macOS, Credential Manager on Windows. If no store is available, a
  provider may instead name an environment variable (`apiKeyEnv`); Axiom never writes a secret to
  settings, SQLite, transcripts, logs or fixtures.
- MCP: implemented in-house over the existing NDJSON codec (stdio transport only, spec
  2025-06-18 or newer as negotiated). The official `rmcp` crate was evaluated and not adopted:
  the subset M3 needs (initialize, tools/list, tools/call, ping, notifications/initialized and
  `notifications/tools/list_changed`) is small, and one JSON-RPC implementation is easier to keep
  correct. Streamable HTTP is not in M3.
- Search: `grep-searcher`, `grep-regex`, `grep-matcher` (BurntSushi, MIT/Unlicense) in
  `workspace`.
- Checkpoints use the **`git` command-line program** (decision 5), not a Git library.

### 4. Models
- Adapters: **Anthropic Messages** and **OpenAI Chat Completions** (the latter reaches Ollama,
  llama.cpp `llama-server`, LM Studio, vLLM). OpenAI Responses and Gemini are later.
- Capability profiles per provider kind and model: `tools`, `streamingToolArgs`,
  `parallelToolCalls`, `images`, `contextWindow`; values `yes|no|unknown`, overridable in the
  provider configuration. A model with `tools = no` can converse but the loop offers no tools.
- Tests never touch the network: a `ScriptedProvider` replays turns, and the adapters are tested
  against a loopback HTTP fixture server serving recorded SSE streams. The daemon accepts a
  scripted provider only when `AXIOM_TEST_SCRIPTED_MODEL=<file>` is set (test-only switch, like
  `AXIOM_TEST_INSECURE_LOOPBACK_HTTP`).
- Opaque provider blocks (Anthropic `thinking`/`redacted_thinking`) are stored verbatim and sent
  back only to the same provider kind.

### 5. Checkpoints: shadow git through the `git` CLI
- Shadow repository `<data dir>/checkpoints/<workspace hash>/` with its own `GIT_DIR`, its own
  index (`GIT_INDEX_FILE` inside it), `GIT_WORK_TREE` = the workspace root, `core.excludesFile`
  empty, hooks disabled, no global/system config (`GIT_CONFIG_NOSYSTEM=1`, `HOME` pointed at the
  shadow dir). The user's `.git`, index, refs and stash are never read or written.
- A checkpoint = `git add -A` into the private index + `write-tree` + `commit-tree`, referenced
  as `refs/axiom/<session>/<n>`. The workspace's `.gitignore` files apply, so build outputs are
  not captured. Files over 16 MiB are skipped and listed in the checkpoint.
- A checkpoint is taken **before the first tool with effect ≥ WriteWorkspace (including
  Execute) in each turn**; after each such tool a post-state snapshot records what the agent left.
- Restore to checkpoint C: for each file that differs between C and the workspace now, restore
  it only if its current content equals what the agent last left; otherwise report it as a
  conflict and leave it. Restore itself is a write and is checkpointed first, so it can be undone.
- If `git` is not on `PATH`, write and execute tools are refused with `prerequisite_missing`
  (`{missing: ["git"]}`): no unprotected write ever happens.

### 6. Multiple clients: a local socket with a token
- `axiomd` keeps stdio for the client that started it (the shell). It also listens on a Unix
  domain socket `<data dir>/axiomd.sock` (mode 0600, in a 0700 directory) unless another live
  daemon already holds it; the token is in `<data dir>/axiomd.token` (0600), regenerated at each
  start. A socket connection must send that token in `initialize.authToken` or it is closed.
  Windows (named pipe) is deferred with Windows.
- Workspaces, tasks and sessions are shared by all connections of one daemon. Each connection
  ending stops only what it owns (its watchers); daemon-wide shutdown hooks run when the stdio
  client ends. Session events go to every connection attached to that session.
- **`axiom` CLI**: `axiom task run -- <argv>`, `axiom build <task>`, `axiom agent send "<msg>"`,
  `axiom agent sessions`, `axiom mcp`. It connects to the socket if a daemon is listening,
  otherwise it spawns its own `axiomd` over stdio.

### 7. MCP
- **Server:** `axiom mcp` (and `axiomd --mcp`, which execs the same code path) speaks MCP on
  stdio and is a protocol client of a daemon (the shared one if listening, else its own): it maps
  `tools/list` → `agent/tools` and `tools/call` → `agent/toolCall`. So the same gate and audit log
  apply, and an `ask` decision surfaces in the GUI when one is attached; with no approving
  connection it is denied with a hint to allow the rule in the policy.
- **Client:** user-configured stdio MCP servers (settings `agent.mcpServers`: name, argv, env)
  are started per session on first use; their tools join the registry as `mcp.<server>.<tool>`
  with effect `Execute`+`Network` (annotations never lower the effect). Workspace-configured MCP
  servers are not supported in M3 (trust).

### 8. Permission policy
As `agents.md` §Permission policy: rules on effect, tool name, path glob or argv prefix; deny >
ask > allow among matching rules; first match by specificity; default `ask`; `read` allowed by
default. `.axiom/project.toml` `[agent.policy]` may only make the user policy stricter.
"Allow for session" adds an in-memory rule. `credentials` and `deploy` are never remembered.
Nothing a model reads can change policy. Every decision and outcome goes to an audit log
(`<data dir>/audit/<workspace hash>.jsonl`, append-only).

### 9. Sessions
`<data dir>/sessions/<workspace hash>/<session id>.jsonl`, append-only, versioned event records
(`v`, `seq`, `ts`, `type`, payload). Store migration 4 adds the session index (id, workspace,
title, provider, model, created, updated). Tool *intent* is written before execution and the
result after; a resumed session whose last tool has no result marks it `interrupted`
(never re-run automatically). Compaction: when a turn's input tokens exceed 75 % of the model's
context window (32 768 when unknown), the loop asks the same model for a summary of all but the
last two exchanges and appends a `compaction` event; older lines stay in the file.

### 10. UI scope
The AI dock on the right (`workspace-spec.md` §5.2, Ctrl+L): session tabs, the log (messages,
tool rows, diff blocks, checkpoint markers with Restore), the approval card, the composer, and a
**Models** pane in Settings (configure providers; the key field writes to the keychain through
`agent/configureProvider`). No placeholder destinations; the M4 diff editor link is not shown
until M4.

### 11. Verification of the exit criterion
In CI, with the scripted provider: "add a function and fix the build" completes through tools
only; every write was preceded by a checkpoint and restores; a denied operation does not
execute; the session resumes after a daemon restart; an MCP client lists and calls tools subject
to the same policy. Once by hand with a real local model (Ollama or llama.cpp): owed by the
owner unless a local model can be installed on the development machine.

## Consequences
- The daemon gains outbound HTTPS to model endpoints, confined to `crates/agent`, to URLs from
  the user's provider configuration (loopback HTTP allowed for local servers).
- `git` becomes a runtime prerequisite for agent writes; read-only conversations work without it.
- The socket makes the daemon reachable by any local process that can read the token file: the
  same trust boundary as the user's files.
- IPC 0.3 is additive; M1.6/M2 shells keep working.
