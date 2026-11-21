# IPC protocol 0.2 (shell/CLI/agents ↔ daemon)

**State: 0.1 frozen for M1; the 0.2 additions (§Protocol 0.2) frozen for M2 by M2-ARCH-001.**
Source of truth for types: `crates/protocol`.
Machine-readable form: `schemas/ipc/protocol.schema.json` (generated). This
document specifies the behaviour the types cannot express.

## Transport and framing

- JSON-RPC 2.0. No batch requests.
- One message per line: UTF-8 JSON with no embedded raw newline, terminated by
  `\n`. A receiver must accept `\r\n`. Empty lines are ignored.
- M1 transport: the daemon's stdin (client→daemon) and stdout (daemon→client).
  stderr is unstructured log text and is not part of the protocol.
- Maximum line length: 64 MiB. A longer line is a fatal protocol error: the
  receiver closes the connection.
- Either side may send requests and notifications. In 0.1 the daemon sends no
  requests, only notifications.

Why NDJSON rather than LSP's `Content-Length` framing: see ADR-0003.

## Lifecycle

1. Client sends `initialize`. Until it succeeds, every other request fails
   with `not_initialized`; notifications are dropped.
2. Version negotiation: versions are `MAJOR.MINOR`. If the client's MAJOR
   differs from the daemon's, `initialize` fails with `unsupported` and
   details `{ "supported": "0.1" }`. Otherwise the daemon answers with
   `min(client, daemon)` and both sides speak that.
3. Clients feature-detect using `InitializeResult.methods`, not the version.
4. `shutdown`, or EOF on the transport, ends the session: the daemon cancels
   all tasks started on this connection's workspaces, flushes the store, and
   exits when no connection remains.

## Requests

- `id` is chosen by the sender, unique among its in-flight requests.
- Every request gets exactly one response, including cancelled ones.
- Unknown method → `method_not_found`. Params that fail to deserialize →
  `invalid_params` with details `{ "reason": "<serde message>" }`.
- Unknown *fields* in params are ignored (forward compatibility). Unknown
  enum values are an `invalid_params` error.
- Requests are handled concurrently. Responses may arrive in any order.
  **Ordering guarantee:** for one task, `task/output` notifications are sent
  in `seq` order and before `task/exited`. The response to `task/start` is
  sent before any notification for that task.

## Cancellation and timeouts

- `$/cancel { id }` asks the receiver to stop working on a request. The
  receiver still responds exactly once: with the normal response if that
  was already produced when the cancel was processed, otherwise with a
  `cancelled` error. Cancelling an unknown or finished id is ignored.
- The daemon imposes no timeouts on requests in 0.1; clients do. Recommended
  client timeouts: `initialize` 10 s; everything else 30 s, after which the
  client sends `$/cancel` and reports a timeout to the user. Tasks are not
  requests and have no timeout.
- All 0.1 methods except `task/start`'s spawned process are short. Handlers
  must check their cancellation token before starting blocking I/O.

## Errors

```json
{ "code": -32013, "message": "File changed on disk.",
  "data": { "kind": "conflict", "retryable": false, "details": { "currentVersion": "ab…" } } }
```
- Clients branch on `data.kind` only. `code` exists for JSON-RPC conformance
  and always equals the kind's code (table in `envelope.rs`).
- `message` is one sentence, user-presentable, without internal paths or
  stack traces. Anything else goes to `$/log`.
- `retryable: true` means the identical request may succeed later.
- A kind the receiver does not know deserializes as `unknown`.

| Kind | Typical cause |
|------|---------------|
| `parse_error`, `invalid_request`, `method_not_found`, `invalid_params`, `internal` | JSON-RPC standard meanings |
| `not_initialized` | request before `initialize` |
| `cancelled`, `timeout`, `unsupported` | protocol level |
| `not_found` | file, directory, task or parent directory does not exist |
| `already_exists` | reserved (create operations, M2+) |
| `permission_denied` | OS permission, or path escapes the workspace |
| `conflict` | `expectedVersion` mismatch, or creating an existing file |
| `io` | any other OS I/O failure (details: `{ "osError": string }`) |
| `unsupported_encoding` | file is not valid UTF-8 |
| `too_large` | file > 16 MiB, state value > 1 MiB |
| `workspace_not_open` | unknown `workspaceId` |
| `spawn_failed` | process could not be started |
| `checksum_mismatch` (0.2) | a download's SHA-256 differs from the plan's (details: `{ "url", "expected", "actual" }`) |
| `unsupported_platform` (0.2) | the provider has no install method or artifact for this OS/arch |
| `approval_denied` (0.2) | `env/install` with an unknown, expired or already-used `planId`; or the user declined the OS elevation prompt |
| `prerequisite_missing` (0.2) | a required tool or system package is absent (details: `{ "missing": [toolId] }`) |

## Methods

Signatures and field docs are in the Rust modules; this is the index.

| Method | Params → Result | Module |
|--------|-----------------|--------|
| `initialize` | `InitializeParams → InitializeResult` | `lifecycle` |
| `shutdown` | `{} → {}` | `lifecycle` |
| `workspace/open` | `OpenParams → OpenResult` | `workspace` |
| `workspace/close` | `WorkspaceRef → {}` | `workspace` |
| `workspace/recent` | `{} → RecentResult` | `workspace` |
| `workspace/getState` | `GetStateParams → StateResult` | `workspace` |
| `workspace/setState` | `SetStateParams → {}` | `workspace` |
| `fs/list` | `ListParams → ListResult` | `fs` |
| `fs/readFile` | `ReadFileParams → ReadFileResult` | `fs` |
| `fs/writeFile` | `WriteFileParams → WriteFileResult` | `fs` |
| `fs/watch` | `WatchParams → {}` | `fs` |
| `task/start` | `StartParams → StartResult` | `task` |
| `task/cancel` | `TaskRef → {}` | `task` |
| `task/list` | `ListTasksParams → ListTasksResult` | `task` |
| `settings/get` | `GetParams → GetResult` | `settings` |
| `settings/set` | `SetParams → {}` | `settings` |

| Notification | Direction | Module |
|--------------|-----------|--------|
| `$/cancel` | both | `lifecycle` |
| `$/log` | daemon → client | `lifecycle` |
| `fs/didChange` | daemon → client | `fs` |
| `task/output` | daemon → client | `task` |
| `task/exited` | daemon → client | `task` |
| `settings/didChange` | daemon → client | `settings` |

## Protocol 0.2 (M2, ADR-0014)

Additive only: every 0.1 method, notification and meaning is unchanged, and a 0.1 client keeps
working (it negotiates 0.1 and never sees the additions it does not call). The daemon reports
`0.2`; clients feature-detect each addition from `initialize.methods` (a method appears there only
once the daemon serves it). Types: `crates/protocol/src/{env,build}.rs`.

| Method | Params → Result | Errors (besides the generic ones) |
|--------|-----------------|-----------------------------------|
| `env/providers` | `ProvidersParams → ProvidersResult` | — |
| `env/detect` | `WorkspaceRef → DetectResult` | `workspace_not_open` |
| `env/versions` | `VersionsParams → VersionsResult` | `not_found` |
| `env/planInstall` | `PlanInstallParams → InstallPlan` | `not_found`, `unsupported_platform`, `prerequisite_missing` |
| `env/install` | `InstallParams → InstallResult` | `approval_denied`, `checksum_mismatch`, `io`, `spawn_failed`, `cancelled` |
| `env/uninstall` | `UninstallParams → {}` | `not_found` |
| `env/resolve` | `WorkspaceRef → Environment` | `workspace_not_open`, `prerequisite_missing`, `invalid_params` (malformed `.axiom/project.toml`) |
| `env/trust` | `TrustParams → {}` | `workspace_not_open` |
| `build/tasks` | `WorkspaceRef → TasksResult` | `workspace_not_open` |
| `build/run` | `RunParams → RunResult` | `workspace_not_open`, `not_found`, `prerequisite_missing`, `spawn_failed` |

| Notification | Direction | Module |
|--------------|-----------|--------|
| `env/installProgress` | daemon → client | `env` |
| `env/installed` | daemon → client (broadcast) | `env` |
| `build/diagnostics` | daemon → client | `build` |

- **Install approval.** `env/planInstall` runs nothing and returns an immutable `InstallPlan`
  (every URL with size and checksum source, every command, every system change, and the
  `effects` summary). `env/install` takes only the `planId`, executes exactly that plan and
  accepts each `planId` once, within 10 minutes (`expiresMs`). A client must show the whole plan
  before calling it; a `Step` kind the client does not know fails deserialization, so it can
  never approve a step it cannot show.
- **Install progress and end.** `env/installProgress` at most 10 per second. A successful install
  sends `env/installed` (to every client, so other views refresh) and then the `env/install`
  response with the same `InstallResult`. `$/cancel` on the request stops the install, removes the
  staging directory and answers `cancelled`; nothing is published.
- **`env/trust`.** Workspace providers (`.axiom/providers/`) are listed with `trusted: false` and
  never loaded until the workspace is trusted (ADR-0014 §5); the flag is stored per workspace.
  Not in ADR-0014's table; it is the "store flag set from the UI" of §5.
- **Builds are tasks.** `build/run` returns a `TaskId`; output, exit and cancellation are the 0.1
  `task/*` methods and notifications. `build/diagnostics` carries only the diagnostics new since
  the previous notification for that task; the last one has `complete: true` and precedes
  `task/exited`.

### Builds as built (M2-BUILD-001)
- `build/tasks` lists the tasks of every trusted provider whose project detection matches the
  workspace, in registry order, then manifest order.
- `build/run` resolves the environment from `.axiom/project.toml` when there is one (a failed
  resolution is its error: `prerequisite_missing` or `invalid_params`; the task does not
  start), expands `${workspace}` in the task's argv (any other variable is `invalid_params`),
  and starts it through the same executor as `task/start` with the workspace root as working
  directory and the environment as env overlay (`PATH` = prepend + the daemon's `PATH`).
  `task/list` and `task/cancel` see it like any task.
- Diagnostics: batches at most every 100 ms while output arrives, each with only the new ones;
  then one batch with `complete: true` (possibly empty) carrying the rest; then `task/exited`.
  The union of all batches is the parser's whole output for the run.
- Parsers (`crates/build/src/parse.rs`, pure): `cargo_json` (stdout, `compiler-message`; the
  primary span is the location, secondary spans and children with spans are `related`, ANSI
  stripped from `rendered`, "aborting due to …" skipped), `gcc_text` (`file:line[:col]:
  severity: message [-Wflag]`; a following `note:` with a location becomes `related`; excerpt
  lines join `rendered`), `regex` (named groups `file`, `line`, `column`, `severity`, `code`,
  `message`), `sarif` (2.1.0 `runs[].results[]`, file URIs decoded). Paths are made
  workspace-relative; one outside the workspace drops the location, never the diagnostic.
- The daemon keeps the last 20 runs per workspace in its store (provider, task, start,
  duration, outcome, error and warning counts); not exposed through the protocol yet.

## Protocol 0.3 (M3, ADR-0015)

Additive only, under the same rules as 0.2: every 0.1/0.2 method, notification and meaning is
unchanged; the daemon reports `0.3`; clients feature-detect from `initialize.methods`. Types:
`crates/protocol/src/agent.rs`. Behaviour: `docs/protocols/agents.md` (normative for the agent
runtime).

| Method | Params → Result | Errors (besides the generic ones) |
|--------|-----------------|-----------------------------------|
| `agent/models` | `ModelsParams → ModelsResult` | — (an unreachable provider is listed with `error`) |
| `agent/configureProvider` | `ConfigureProviderParams → {}` | `invalid_params`, `unsupported` (no credential store) |
| `agent/sessionCreate` | `SessionCreateParams → SessionInfo` | `workspace_not_open`, `not_found` |
| `agent/sessionList` | `SessionListParams → SessionListResult` | `workspace_not_open` |
| `agent/sessionOpen` | `SessionRef → SessionOpenResult` | `workspace_not_open`, `not_found` |
| `agent/sessionRename` | `SessionRenameParams → SessionInfo` | `not_found`, `invalid_params` |
| `agent/sessionDelete` | `SessionRef → {}` | `not_found` |
| `agent/image` | `ImageParams → ImageData` | `not_found` |
| `agent/send` | `SendParams → {}` | `not_found`, `conflict` (turn running), `prerequisite_missing` |
| `agent/cancel` | `SessionRef → {}` | `not_found` |
| `agent/approve` | `ApproveParams → {}` | `not_found` |
| `agent/checkpoints` | `SessionRef → CheckpointsResult` | `not_found` |
| `agent/restore` | `RestoreParams → RestoreResult` | `not_found`, `checkpoint_conflict`, `prerequisite_missing` |
| `agent/tools` | `WorkspaceRef → ToolsResult` | `workspace_not_open` |
| `agent/toolCall` | `ToolCallParams → ToolCallResult` | `approval_denied`, `approval_required`, `invalid_params` |
| `agent/policy` | `PolicyParams → PolicyResult` | — |
| `agent/setPolicy` | `Policy → {}` | `invalid_params` |

| Notification | Direction | Module |
|--------------|-----------|--------|
| `agent/event` | daemon → attached clients | `agent` |
| `agent/approvalRequested` | daemon → approving clients | `agent` |
| `agent/approvalResolved` | daemon → approving clients | `agent` |
| `agent/reveal` | daemon → clients of the workspace | `agent` |

- **`initialize`** gains two optional fields: `authToken` (required on the local socket,
  ignored on stdio) and `capabilities.approvals` (the client shows approvals to a person).
- **Attachment.** `agent/sessionOpen` and `agent/sessionCreate` attach the calling connection
  to the session; it then receives every `agent/event` of that session until it disconnects.
- **Turns.** `agent/send` responds once the turn is accepted; the turn's events follow and the
  last one is `turnEnd`. `textDelta` events are ephemeral (never persisted or replayed).
- **Approvals.** A tool call the policy answers with `ask` is announced with
  `agent/approvalRequested` to every connection that declared `capabilities.approvals` and waits
  for the first `agent/approve`; then `agent/approvalResolved` goes to the same connections. With
  no approving connection the call fails with `approval_required` and does not run.
- **Chat additions (M3-ARCH-002; ADR-0016, ADR-0017).**
  - `workspaceId` is optional in `SessionCreateParams`, `SessionRef`, `SendParams`,
    `RestoreParams` and `SessionListParams`. A session without one is a conversation with no
    workspace tools.
  - `SendParams` gains `images` (base64, ≤ 5 MiB each) and `fromTurn` (edit/regenerate: a
    `rewind` event, then the new turn replaces turn N on).
  - `user` events carry `images` as `ImageRef`s, fetched with `agent/image`.
  - `assistantText` may carry `kind: plan|thought` (external agents).
  - `ProviderKind` gains `acp`, with `ProviderConfig.command` and `env`.
  These changes are additive or required → optional: every message valid before is still
  valid.
- New error kinds: `approval_required` (-32023), `checkpoint_conflict` (-32024, details
  `{conflicts}`), `model_error` (-32025, details `{kind}`).
- Not in ADR-0015 §1's table, added by M3-ARCH-001: `agent/approvalResolved` (so every approving
  client can drop a prompt someone else answered) and `agent/reveal` (the `editor.open` tool's
  notification, ADR-0015 §2's tool list).

## Behavioural details

### `fs/writeFile` decision table

| File exists | `overwrite` | `expectedVersion` | Result |
|---|---|---|---|
| any | true | any | write |
| no | false | absent | write (create) |
| no | false | present | `conflict`, details `{ "currentVersion": null }` |
| yes | false | absent | `conflict`, details `{ "currentVersion": "<v>" }` |
| yes | false | equals current | write |
| yes | false | differs | `conflict`, details `{ "currentVersion": "<v>" }` |

Writes are atomic (temp file in the same directory, fsync, rename) and
preserve the existing file's permission bits. Text is written byte-for-byte;
the daemon never converts line endings.

### `fs/list` ignore rules
`ignored` is computed with the `ignore` crate using `.gitignore`, `.ignore` and
`.git/info/exclude` (the user's global gitignore is deliberately not
consulted, so results do not depend on the machine), evaluated from the workspace
root; `.git` itself is always `ignored: true`. Ignored entries are listed.

### `fs/didChange`
Events are debounced 100 ms per workspace and deduplicated per path (a
create followed by modify within the window is `created`; anything followed
by remove is `removed`). Changes under ignored directories (and under `.git`) are
dropped; a change to an individually ignored file in a non-ignored
directory is reported, because `fs/list` lists such files. If the OS reports an overflow, the daemon sends one change with
`path: ""`, `kind: "modified"`, meaning "re-list everything you show".

### Tasks
- `argv[0]` is resolved against the task's `PATH` (after `env` is applied).
- stdin is closed (`/dev/null`). There is no PTY in 0.1 (terminal: M4).
- Output is read in chunks of up to 8 KiB and forwarded as it arrives; a
  chunk that ends inside a UTF-8 sequence holds the partial bytes for the
  next chunk.
- Each task runs in its own process group / job so cancellation reaches
  grandchildren.
- `task/cancel`: Unix `SIGTERM` to the group, `SIGKILL` after 3 s. Windows:
  `TerminateJobObject` immediately (there is no portable graceful signal for
  non-console children).
- Finished tasks are kept for `task/list` (50 per workspace); their output is
  not retained by the daemon in 0.1.

### Settings (known keys)

| Key | Type | Default | Meaning |
|-----|------|---------|---------|
| `appearance.theme` | `"system" \| "light" \| "dark"` | `"system"` | UI and editor theme |
| `editor.fontSize` | integer 8–40 | `13` | editor font size, px |
| `editor.fontFamily` | string | `""` (platform monospace) | editor font |
| `editor.wordWrap` | boolean | `false` | soft wrap |
| `workspace.reopenLast` | boolean | `true` | reopen the last workspace at startup |
| `workspace.lastRoot` | string or null | `null` | canonical root of the most recently opened workspace; written by the daemon on `workspace/open` |
| `agent.providers` | array of `ProviderConfig` | `[]` | model provider configurations (M3); written by `agent/configureProvider`; never contains a key |

Unknown keys are stored and returned unchanged, so clients can add settings
without a protocol change; a key becomes "known" (typed, defaulted) by adding
it to this table and the daemon's registry.

## Versioning

- MINOR bump: new methods, notifications, optional fields, settings keys.
- MAJOR bump: anything that could break an existing client. Needs an ADR.
- The daemon must keep serving every method listed for the MAJOR it speaks.
- Negotiation: the session runs at `min(client minor, server minor)`; 0.2 and 0.3 added methods
  and optional fields only, so a 0.1 client talking to a 0.3 daemon gets `"0.1"` and unchanged
  behaviour.

## Example session

```
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"0.1","client":{"name":"axiom-desktop","version":"0.1.0"}}}
← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"0.1","server":{"name":"axiomd","version":"0.1.0"},"methods":["initialize","shutdown","workspace/open","…"],"host":{"os":"linux","arch":"x86_64","pathSeparator":"/"}}}
→ {"jsonrpc":"2.0","id":2,"method":"workspace/open","params":{"path":"/home/u/proj"}}
← {"jsonrpc":"2.0","id":2,"result":{"workspaceId":"w1","root":"/home/u/proj","name":"proj"}}
→ {"jsonrpc":"2.0","id":3,"method":"task/start","params":{"workspaceId":"w1","command":{"argv":["sh","-c","echo hi; exit 3"]},"env":{}}}
← {"jsonrpc":"2.0","id":3,"result":{"taskId":"t1"}}
← {"jsonrpc":"2.0","method":"task/output","params":{"taskId":"t1","seq":0,"stream":"stdout","data":"hi\n"}}
← {"jsonrpc":"2.0","method":"task/exited","params":{"taskId":"t1","outcome":"failed","exitCode":3,"durationMs":4}}
```

## Contract tests

- Shape: `crates/protocol/tests/contract.rs` (0.1) and `tests/m2_contract.rs` (0.2 additions).
- Behaviour: `crates/axiomd/tests/` spawns the real binary and exercises every
  row of every table in this document (task M1-DAEMON-002). A behaviour stated
  here without a test there is a defect in the test suite.
