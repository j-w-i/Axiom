# ADR-0014: Environments and builds (M2): protocol additions, dependencies, install approval

Status: **Accepted** (2026-11-18). Proposed by the M2 planning pass; approved by the project owner
("Work through the M2 plan", 2026-11-18).

## Context

M2 makes Axiom recognise what a project needs, show what is installed, install toolchains into managed
storage with user-approved plans, and run provider-defined build/test tasks with navigable diagnostics
(`docs/roadmap/milestones.md` §M2). The provider manifest and the diagnostics types exist as drafts
(`crates/protocol/src/{provider,diagnostics}.rs`, `docs/protocols/providers.md`); IPC 0.1 is frozen.
This ADR records the decisions M2's task specs depend on, so implementation tasks need not make them.

## Decisions

### 1. Protocol: additive methods under the same protocol, feature-detected
IPC 0.1 stays frozen and valid. M2 adds methods and notifications only (governance: additive changes
through a task spec, `tests/contract.rs` unchanged, schemas regenerated). The daemon reports protocol
**0.2**; a 0.1 client keeps working because clients feature-detect with `InitializeResult.methods`
(`ipc.md` §Versioning rule 3). New methods (exact types in task M2-ARCH-001):

| Method / notification | Purpose |
|-----------------------|---------|
| `env/providers` | the loaded providers, their source (built-in / user / workspace) and validation errors |
| `env/detect` | for a workspace: which providers apply, each tool found or missing (version, location) |
| `env/versions` | installable versions for a provider (from the manifest's version source) |
| `env/planInstall` | an `InstallPlan` (every URL, digest source, command, system change); returns a `planId` |
| `env/install` | executes an approved `planId`; streams `env/installProgress`; ends with `env/installed` |
| `env/uninstall` | removes a managed install (its directory, nothing else) |
| `env/resolve` | the project's `Environment` (`.axiom/project.toml` + lock): PATH prepend and variables |
| `build/tasks` | the provider tasks a workspace offers (build/run/test…) |
| `build/run` | starts a provider task inside the project's environment; returns a `TaskId` (the existing `task/*` stream) |
| `build/diagnostics` (notification) | normalised `Diagnostic`s for a run, batched; final set on exit |

New error kinds (reserved by `providers.md`): `checksum_mismatch`, `unsupported_platform`,
`approval_denied`, `prerequisite_missing`.

### 2. Install approval is a protocol fact, not a UI convention
`env/install` accepts only a `planId` the daemon issued, unchanged, within 10 minutes. The plan is
immutable once issued; executing anything not in it is impossible by construction. The UI must show
the whole plan (every URL with size and checksum source, every command, every system change) before
calling `env/install`. `system_package` plans run the package manager through the platform's
elevation prompt (pkexec / sudo -A / UAC); Axiom never stores elevated credentials.

### 3. New dependencies (all MIT/Apache-2.0, `cargo deny` checked)
- `crates/env`: `reqwest` (rustls, no native TLS; streaming downloads) — the only HTTP client;
  `sha2` (already used); `tar`, `flate2`, `xz2`, `zip` for extraction; `toml` (manifest and project
  files); `regex` (version regexes); `semver`.
- `crates/build`: `regex`; nothing else beyond `exec`/`env`/`protocol`.
- No in-process plugin system and no scripting language (ADR-0006 unchanged).

### 4. Module edges (to add to `modules.md`)
`env → protocol, store`; `build → protocol, exec, env`; `axiomd → env, build`. The shell gains no
edge (it speaks the protocol through `desktop-bridge`).

### 5. Trust
Built-in and user providers load at start. Workspace providers (`.axiom/providers/`) are listed as
"available, untrusted" and load only after the user trusts the workspace (a per-workspace store flag
set from the UI). Provider `process` helpers are not enabled by M2 (no M2 provider needs one;
`apple-xtool` is later).

### 6. Project files
A project selects toolchains in `.axiom/project.toml` (`[environment] rust = "1.84.0"`, human-written,
committed); the resolved lock (exact versions and digests) is `.axiom/project.lock`, written by Axiom.
Axiom writes them only through `fs/*`-equivalent workspace-contained writes.

### 7. UI scope
M2 builds what its functionality backs (governance rule 9): the activity rail (D4) with Files and
Environments, the Environment Manager page (`workspace-spec.md` §5.1), the install-plan dialog, the
console's **Problems** view (the view strip appears now that there are two views), and provider tasks
in the palette and transport. No placeholder destinations.

### 8. Verification of the exit criterion
The "machine without Rust" scenario runs in CI in a clean container (no Rust on PATH) and once by hand;
the provider contract suite runs on Linux, macOS and Windows CI for every bundled provider.

## Consequences
- The daemon gains network access for downloads, confined to `crates/env`, to URLs that come from a
  validated manifest, with checksums verified before anything is extracted or run.
- IPC 0.2 is additive; the M1.5/M1.6 shells keep working against it.
- Workspace providers stay inert until trusted; supply-chain risk is limited to built-in manifests
  (reviewed in this repository) and the user's own.
