# Architecture decision records

Immutable once accepted (see `docs/architecture/governance.md`). Proposed
changes from implementation tasks go in `proposed/` as change requests.

| ADR | Decision | Status |
|-----|----------|--------|
| [0001](ADR-0001-technology-stack.md) | Qt 6 C++/QML shell, Rust daemon, TypeScript editor surface | Accepted |
| [0002](ADR-0002-qt-rust-integration.md) | Separate processes; no Rust↔Qt FFI | Accepted |
| [0003](ADR-0003-ipc-protocol.md) | JSON-RPC 2.0, newline-delimited, over stdio then local sockets | Accepted |
| [0004](ADR-0004-editor-strategy.md) | Monaco first; LSP via the daemon; extension host as a later, measured stage | Accepted |
| [0005](ADR-0005-contract-source-of-truth.md) | Rust protocol crate generates JSON Schema and TypeScript | Accepted |
| [0006](ADR-0006-provider-model.md) | Declarative provider manifests + optional out-of-process helper | Accepted |
| [0007](ADR-0007-agent-runtime.md) | In-house agent loop over a shared, gated tool registry; MCP exposure | Accepted |
| [0008](ADR-0008-visual-inspection.md) | Build-time source stamping + adapter protocol + objective verification | Accepted |
| [0009](ADR-0009-persistence.md) | SQLite for app state, TOML for project config, JSONL for transcripts | Accepted |
| [0010](ADR-0010-isolation.md) | Three named isolation tiers; never overstate | Accepted |
| [0011](ADR-0011-licensing.md) | MIT OR Apache-2.0; LGPL Qt modules only, dynamically linked | Accepted |
