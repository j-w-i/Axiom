# Toolchain providers

**State: draft.** Frozen by the first task of M2 (M2-ENV-001) after it has
implemented detection for the reference manifest. Types:
`crates/protocol/src/provider.rs`. Schema:
`schemas/provider/manifest.schema.json`. Reference:
`providers/rust/provider.toml` (parsed in `crates/protocol/tests/drafts.rs`).
Research behind this design: `docs/research/toolchains.md`.

## What a provider is

A provider is a **directory** containing `provider.toml` and, optionally, a
helper executable. It tells Axiom how to recognise a kind of project, which
tools that needs, how to find or install them, and which tasks the project
offers.

Providers are data first. The daemon (`crates/env`) interprets the manifest;
third-party code runs only as (a) commands the manifest declares, each
passing the permission gate, or (b) an explicitly enabled helper process.
There is no in-process plugin API and no scripting language (ADR-0006).

## Discovery and registration

Search order (later entries override earlier ones with the same `id`):
1. Built-in: `providers/*/provider.toml` shipped with Axiom.
2. User: `<data dir>/providers/*/provider.toml`.
3. Workspace: `<workspace>/.axiom/providers/*/provider.toml` — loaded only
   after the user trusts the workspace; until then listed as "available,
   untrusted".

A manifest that fails schema validation is reported in the Environment view
with the validation error and is otherwise ignored. Unknown fields are
errors (`deny_unknown_fields`), so typos never silently disable something.

## Operations

The conceptual operations from the brief map onto this split:

| Operation | Declarative (manifest) | Helper process method |
|-----------|------------------------|-----------------------|
| detect project | `project.any_file` | `provider/detectProject` |
| detect tools | `tools[]` (`program`, `version_args`, `version_regex`) | `provider/detectTools` |
| capabilities | `hosts`, `install[].kind`, `tasks[].kind` | `provider/capabilities` |
| list versions | `install[].versions` / `list_versions` | `provider/listVersions` |
| plan install | derived from `install[]` | `provider/planInstall` |
| install / uninstall | executed by the daemon from the plan | `provider/install` (streams progress) |
| verify | re-run tool detection inside the new environment | – |
| create environment | `bin_dirs`, `env` → an `Environment` record | `provider/environment` |
| tasks (build/run/test) | `tasks[]` | `provider/tasks` |
| debug | not in v1 of the manifest (M6, with DAP) | – |

A provider with no `process` is fully declarative. If `process` is present,
the helper's answer replaces the declarative one for the methods it lists in
its `provider/capabilities` response. Helper protocol = same JSON-RPC/NDJSON
framing as `ipc.md`, methods above; it is specified in detail by M2-ENV-004
when the first provider needs it (expected: `apple-xtool`, because xtool's
state cannot be probed with a version regex alone — see
`docs/research/xtool.md`).

## Core data (to be added to the protocol in M2)

```text
Toolchain   { providerId, toolId, version, location: Managed{root} | System{path}, verifiedMs }
Environment { id, toolchains: [Toolchain], pathPrepend: [dir], vars: {K: V} }
InstallPlan { providerId, version, scope, steps: [Step], downloadBytes?, requiresApproval: [Effect] }
Step        = Download{url, sha256} | Extract{format, stripComponents} | Run{argv, env}
            | SystemPackages{manager, packages} | Verify{toolId}
```

- An **InstallPlan is shown to the user before anything runs.** It lists
  every URL, every command and every system change. Approval is per plan.
- Managed installs go to `<data dir>/toolchains/<providerId>/<version>/`.
  Install = stage in a sibling temp directory → verify → atomic rename.
  Uninstall = remove that directory. Nothing else on the machine changes;
  that is the definition of `scope = "managed"`.
- `delegate` installs with `scope = "managed"` run the ecosystem installer
  with its home variables pointed into the managed root (see the Rust
  manifest: `RUSTUP_HOME`, `CARGO_HOME`).
- `system_package` plans always need explicit approval and run the package
  manager through the platform's elevation prompt (pkexec/sudo/UAC). Axiom
  never caches elevated credentials.
- A project selects its environment in `.axiom/project.toml`
  (`[environment] rust = "1.84.0"`), which is human-readable and committed.
  The resolved lock (exact versions + digests) is `.axiom/project.lock`.

## Template variables

Only these expand, and only in the fields documented as templates:
`${version}`, `${root}` (managed install root), `${workspace}`. No shell
evaluation, no nested expansion. An unknown `${…}` is a validation error.

## Errors

Provider operations return the IPC error model. Additional kinds reserved for
M2: `checksum_mismatch`, `unsupported_platform`, `approval_denied`,
`prerequisite_missing` (details: `{ "missing": [toolId] }`).

## Contract tests (M2)

`crates/env/tests/provider_contract.rs` runs against **every** manifest under
`providers/`:
1. parses and validates;
2. every `version_regex` compiles and has exactly one capture group;
3. every template uses only allowed variables;
4. every `archive` artifact has a checksum source;
5. detection runs without error on the CI host (found or not found, never a
   crash);
6. for providers marked `ci_install = true` in the test's allow-list: a
   managed install into a temp directory succeeds, verifies, and uninstalls
   leaving the directory absent.

A new provider is accepted when it passes this suite; that is the whole
integration surface a contributor needs to understand.
