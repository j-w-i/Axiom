# ADR-0006: Toolchain providers are declarative manifests with an optional helper process

Status: Accepted (2026-11-11); manifest schema is draft until M2

## Context
Providers must be addable by third parties without touching core, must be
independently testable, and must not get unrestricted privileges by default.
Research (`docs/research/toolchains.md`, `prior-art.md`): mise, aqua and
proto show that most tools are describable as data (versions, per-platform
artifacts, checksums, env exports); VS Code and LSP/DAP/MCP show that
out-of-process JSON-RPC is the proven extension boundary; WASM component
plugins (Zed, Lapce) give sandboxing but toolchain installers need exactly
the ambient capabilities (network, filesystem, process spawning) a WASM
sandbox withholds.

## Decision
1. A provider is a directory with a `provider.toml` manifest (schema:
   `crates/protocol/src/provider.rs`). The daemon interprets it. Detection,
   version listing, managed archive installs, delegated installs (rustup,
   swiftly, …), system-package installs and task templates are all
   declarative.
2. When data is not enough, the manifest names a **helper process** that
   speaks `provider/*` JSON-RPC over stdio. It runs with the user's
   privileges, only after the user enabled that provider, and everything it
   asks the daemon to execute still passes the permission gate.
3. No in-process plugins, no embedded scripting language, no WASM host — for
   now. Revisit WASM for pure-compute extensions (parsers, formatters) by ADR
   if a need appears.
4. Axiom does not reimplement version managers. Prefer `delegate` to the
   ecosystem's own installer; use `archive` for tools that publish plain
   release archives with checksums.
5. Every provider must pass the shared provider contract test suite
   (`docs/protocols/providers.md` §Contract tests).

## Consequences
- Most providers are a single reviewable TOML file. `providers/rust` is the
  reference and already parses against the schema.
- Manifests cannot express arbitrary logic; complex ecosystems (Android SDK
  licences, xtool's Apple-ID setup) need a helper, which is more work for
  the provider author but keeps the trust boundary explicit.
- Checksums are mandatory for archive installs: some upstreams without
  published digests cannot be offered as managed installs until they are
  pinned in the manifest.
