# Architecture decision records

Immutable once accepted (see `docs/architecture/governance.md`). Proposed
changes from implementation tasks go in `proposed/` as change requests.

| ADR | Decision | Status |
|-----|----------|--------|
| [0001](ADR-0001-technology-stack.md) | Qt 6 C++/QML shell, Rust daemon, TypeScript editor surface | Superseded by 0012 (shell) |
| [0002](ADR-0002-qt-rust-integration.md) | Separate processes; no Rust↔Qt FFI | Accepted (amended by 0012) |
| [0003](ADR-0003-ipc-protocol.md) | JSON-RPC 2.0, newline-delimited, over stdio then local sockets | Accepted |
| [0004](ADR-0004-editor-strategy.md) | Monaco first; LSP via the daemon; extension host as a later, measured stage | Superseded by 0020 (the editor; LSP via the daemon stands) |
| [0005](ADR-0005-contract-source-of-truth.md) | Rust protocol crate generates JSON Schema and TypeScript | Accepted |
| [0006](ADR-0006-provider-model.md) | Declarative provider manifests + optional out-of-process helper | Accepted |
| [0007](ADR-0007-agent-runtime.md) | In-house agent loop over a shared, gated tool registry; MCP exposure | Accepted |
| [0008](ADR-0008-visual-inspection.md) | Build-time source stamping + adapter protocol + objective verification | Accepted |
| [0009](ADR-0009-persistence.md) | SQLite for app state, TOML for project config, JSONL for transcripts | Accepted |
| [0010](ADR-0010-isolation.md) | Three named isolation tiers; never overstate | Accepted |
| [0011](ADR-0011-licensing.md) | MIT OR Apache-2.0; LGPL Qt modules only, dynamically linked | Accepted (amended by 0012, 0013) |
| [0012](ADR-0012-desktop-shell-tauri.md) | Desktop shell on Tauri 2 + React; Qt retired after a gated migration | Superseded by 0020 (shell); the process split stands |
| [0013](ADR-0013-visual-identity.md) | Visual identity "Precision Instrument": material grammar, generated themes, motion policy, M1.6 scope rule; OFL font assets | Accepted (amended by 0020) |
| [0014](ADR-0014-environments-and-builds.md) | Environments and builds (M2): protocol additions, dependencies, install approval | Accepted |
| [0015](ADR-0015-agent-runtime-m3.md) | Agent runtime (M3): protocol 0.3, crates, credentials, checkpoints, clients, MCP | Accepted |
| [0016](ADR-0016-external-agents-acp-m3.md) | External agents (Claude Code, Codex, Gemini CLI) through ACP | Accepted |
| [0017](ADR-0017-chat-first-class-surface.md) | Chat is a first-class surface, not only a dock | Accepted (amended by 0020) |
| [0018](ADR-0018-editor-depth-m4.md) | Editor depth (M4): protocol 0.4, language servers, terminal, Git, search | Accepted |
| [0020](ADR-0020-flutter-shell.md) | The desktop shell moves to Flutter; Tauri retired after a gated migration | Accepted, implemented (FL-QA-001) |
