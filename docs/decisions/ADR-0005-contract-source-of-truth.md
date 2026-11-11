# ADR-0005: The Rust protocol crate is the single source of truth

Status: Accepted (2026-11-11)

## Decision
All cross-process contracts are defined as Rust types in
`crates/protocol` (`axiom-protocol`) with `serde` + `schemars` derives.
- JSON Schema is **generated** into `schemas/` by
  `cargo run -p axiom-protocol --example export_schema` and committed. A test
  and CI fail if the committed files are stale.
- TypeScript types for the editor surface and web inspection adapters are
  generated from those schemas (`json-schema-to-typescript`, wired in by the
  task that first needs them; M1's editor bridge is small enough to be
  hand-declared from its own document).
- The C++ shell does not get generated types. It reads and writes
  `QJsonObject` in exactly one layer (`DaemonClient` and the models) and is
  covered by the smoke test against the real daemon.
- Prose specifications in `docs/protocols/` describe behaviour only and defer
  to the crate for shapes.

## Alternatives considered
- *Protocol Buffers / an IDL first.* Adds a toolchain and a second
  representation for the dominant (Rust) consumer; JSON-RPC payloads would
  need a JSON mapping anyway.
- *Hand-written JSON Schema first, generate Rust.* Rust codegen from schema
  is weaker than schema generation from Rust, and the Rust side is where
  correctness matters most.
- *TypeSpec / OpenRPC documents.* Reasonable, but another language for agents
  to keep consistent. The generated `protocol.schema.json` already serves as
  a machine-readable method catalogue.

## Consequences
- One place to change; contract tests pin the wire format.
- The protocol crate must stay dependency-light and I/O-free so every
  component (including future WASM targets) can use it.
