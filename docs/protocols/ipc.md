# IPC protocol 0.1 (shell/CLI/agents ↔ daemon)

**State: frozen for M1.** Source of truth for types: `crates/protocol`.
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

Unknown keys are stored and returned unchanged, so clients can add settings
without a protocol change; a key becomes "known" (typed, defaulted) by adding
it to this table and the daemon's registry.

## Versioning

- MINOR bump: new methods, notifications, optional fields, settings keys.
- MAJOR bump: anything that could break an existing client. Needs an ADR.
- The daemon must keep serving every method listed for the MAJOR it speaks.

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

- Shape: `crates/protocol/tests/contract.rs` (exists).
- Behaviour: `crates/axiomd/tests/` spawns the real binary and exercises every
  row of every table in this document (task M1-DAEMON-002). A behaviour stated
  here without a test there is a defect in the test suite.
