# ADR-0003: JSON-RPC 2.0 as newline-delimited JSON

Status: Accepted (2026-11-11)

## Decision
- Message model: JSON-RPC 2.0 requests, responses and notifications; no
  batches; bidirectional.
- Framing: one compact JSON document per `\n`-terminated line (NDJSON).
- Transport: daemon stdio in M1; additionally a Unix domain socket / Windows
  named pipe from M3. Same messages on both.
- Errors: JSON-RPC error object with a mandatory structured `data`
  (`kind`, `retryable`, `details`).
- The same envelope and framing are used for provider helpers and inspection
  adapters (over WebSocket text frames for web adapters).

Full specification: `docs/protocols/ipc.md`.

## Alternatives considered
- *LSP-style `Content-Length` framing.* Familiar, and tolerant of pretty-
  printed JSON. Rejected: more code on both sides (header parsing in C++ and
  Rust) for no benefit, since we control both serializers and compact JSON
  never contains a raw newline. NDJSON is also what MCP's stdio transport
  uses, which lets the MCP server mode share the codec. Trivially debuggable
  with `cat`/`jq`.
- *gRPC (tonic) / Cap'n Proto.* Strong schemas and streaming. Rejected for
  now: heavy C++ client dependencies in the shell, awkward over stdio, and
  no measured need. Reconsider only with profiling evidence.
- *Qt Remote Objects / D-Bus.* Qt- or Linux-specific; excludes other clients.
- *HTTP + WebSocket on localhost.* Opens a TCP port other local users and
  web pages can reach; needs auth from day one. Rejected as the default.

## Consequences
- Binary payloads (screenshots, M5) do not belong in JSON. They are passed
  as file references in the daemon's cache directory (or base64 only below
  64 KiB).
- A 64 MiB line limit bounds memory; file content is capped at 16 MiB.
- Research recommended byte-length framing as the conservative default; we
  deviate deliberately for the reasons above and record it here.
