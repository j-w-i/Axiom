# Toolchain providers

**State: frozen for M2** (M2-ARCH-001, 2026-11-18; ADR-0014). Changes from here on are additive
and go through a task spec (`governance.md`). Types:
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

As built (M2-ENV-001, `crates/env`):
- Built-in manifests are compiled into `axiomd`; a test keeps them byte-identical to
  `providers/*/provider.toml`. A provider's `id` equals its directory name.
- An invalid manifest never overrides a valid one with the same id; it is listed with its
  `errors` (for one that did not parse, `id` is the directory name).
- Validation beyond serde: `schema`, id format, unique tool and task ids, bare `program` names,
  `version_regex` with exactly one capture group, only `${version}`/`${root}`/`${workspace}` in
  templates, 64-hex digests or a checksum URL on every archive artifact, `bin_dirs` inside the
  root, a delegate's `using` naming one of the tools, and no `process` (not supported yet).
- Trust is stored per workspace (`env/trust`); untrusted workspace manifests are parsed for
  listing but never applied.

### Detection (as built)
- **Project:** a provider applies when any `project.any_file` pattern (`*` and `?`, matched
  against names) matches an entry of the workspace root or of one of its non-hidden
  first-level directories.
- **Tools:** managed installs first, newest version first
  (`<data dir>/toolchains/<id>/<version>/<bin_dir>/<program>`, with the `bin_dirs` of the
  manifest's archive methods; directories starting with `.` are staging and ignored), then each
  absolute `PATH` entry (`program`, or `program.exe|.cmd|.bat` on Windows). The first candidate
  whose `version_args` output (stdout, else stderr) matches `version_regex` wins.
- The version command runs with its absolute path, no shell, stdin closed, a 5 s limit, and the
  system temp directory as working directory, so a project-local toolchain file (for example
  `rust-toolchain.toml`) cannot make a proxy install anything during detection.

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
framing as `ipc.md`, methods above. **Not enabled in M2** (ADR-0014 §5): no M2 provider needs
one, so M2 reports a manifest with `process` in `env/providers` with an error ("helper processes
are not supported yet") and does not load it. The helper protocol is specified when the first
provider needs it (expected: `apple-xtool`, because xtool's state cannot be probed with a
version regex alone — see `docs/research/xtool.md`).

## Core data (protocol 0.2, `crates/protocol/src/env.rs`)

```text
Toolchain   { providerId, toolId, version, location: Managed{root} | System{path}, verifiedMs }
Environment { toolchains: [Toolchain], pathPrepend: [dir], vars: {K: V} }
InstallPlan { planId, providerId, version, scope, steps: [Step], downloadBytes?, effects: [Effect],
              expiresMs }
Step        = Download{url, bytes?, sha256: Digest{value} | Url{url}} | Extract{format, stripComponents}
            | Run{argv, env} | SystemPackages{manager, packages, argv} | Verify{toolId}
Effect      = network | writeManaged | runPrograms | systemChange | elevation
```

Methods and their errors: `ipc.md` §Protocol 0.2.

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

### Installs (as built, M2-ENV-002)
- `env/versions` reads the first `archive` method's version source (`static`, `json` with a
  `*`-wildcard pointer, `github_releases` without drafts and pre-releases, tag prefix stripped),
  keeps names usable as directory names, sorts newest first and caches the list for an hour.
- `env/planInstall` picks the first archive method with an artifact for this OS/arch (or the
  given `method`), expands `${version}` in the artifact and checksum URLs, asks the server for
  the size (`HEAD`; absent if unknown), and lists `download → extract → verify` for each
  required tool. Effects: `network`, `writeManaged`, `runPrograms` (verification runs the new
  tool). Delegate and system-package methods come with M2-ENV-003/005 (`unsupported` until
  then). Installs use built-in and user providers; workspace providers are not installable
  (the 0.2 params name no workspace).
- A checksum file is either one bare digest or `sha256sum` lines; the entry whose base name
  equals the artifact URL's last segment is used (`*./rustup-init` matches `rustup-init`).

### Delegate installs (as built, M2-ENV-003)
Additive manifest fields (all optional; older manifests are unchanged):

| Field | On | Meaning |
|-------|----|---------|
| `format = "file"` | artifact | the download is the executable itself, placed at `${root}/<URL's last segment>` and made executable |
| `provides = [toolId]` | `archive` | this method installs that tool for another method (a bootstrap); it is not offered on its own |
| `setup = [argv]` | `archive` | run once after extraction inside the new root, with the method's `env` |
| `bin_dirs` | `delegate` | where the delegate's installed executables live, relative to the root |
| `format = "gz"` | artifact | M5.5: one gzip-compressed executable, decompressed to the root under the URL's last segment without `.gz` |
| `file_name` | `file`/`gz` artifact | M5.5: the executable's name in the root when the URL's is not it (`marksman-linux-x64` → `marksman`) |
| `versions` | `delegate` | the installable versions; absent = `list_versions` through the delegate on `PATH` |

- A delegate plan is: the bootstrap archive's steps when an `archive` method `provides` the
  `using` tool for this host (download, place, `setup`), else the delegate from `PATH`, else
  (M5.5) from the newest managed install of any provider (its `bin/` or root, which is put first
  on the delegate's `PATH` so npm finds its node), else `prerequisite_missing`; then `install` with argv[0] replaced by that delegate's
  absolute path; then `verify` for each required tool, run with the delegate's `env`.
- `install`, `uninstall` and `list_versions` must start with the `using` tool's program
  (validated): a delegate method can only ever run its delegate.
- Each managed version root is self-contained. For Rust: `${root}/rustup` (RUSTUP_HOME),
  `${root}/cargo` (CARGO_HOME, with rustup's proxies in `cargo/bin`) and
  `RUSTUP_TOOLCHAIN=${version}`. A rustup on the user's `PATH` is not used: its proxies would
  not exist in the root. `~/.rustup`, `~/.cargo` and shell profiles are never written
  (`rustup-init -y --no-modify-path --default-toolchain none`; the CI test runs with an empty
  temporary HOME and asserts it stays empty).
- Plans show `${root}` as the final root; execution runs in the staging directory with every
  occurrence of the final root rewritten, then renames it into place. Absolute symbolic links
  into the staging directory that an installer leaves behind (uv does) are re-pointed at the
  final root before the rename (M2-PROV-003).
- Built-in providers in M2: `rust`, `node`, `cmake`, `ninja`, `python` (`providers/*`).
  M5.5 adds a provider per tier-1 language server (`providers/lsp-*`, ADR-0021 §4).
- Uninstall runs the delegate's `uninstall` command with the delegate from inside the root
  (a failure is logged, not fatal), then removes the root.
- Detection runs a managed tool with its method's `env` expanded for that root (rustup's
  proxies need `RUSTUP_HOME` and `RUSTUP_TOOLCHAIN`).
- A failing command is `io` with details `{ "exitCode", "output" }` (the last 2,000
  characters of its output).
- Security rules and their tests: `docs/architecture/security.md` §Managed installs.

### Project files (as built, M2-ENV-005)
`.axiom/project.toml` (human-written, committed). Grammar, as parsed:

```toml
[environment]               # the only table; any other key or table is an error
<provider-id> = "<version>" # provider id: [a-z0-9][a-z0-9-]*
                            # version: an installed managed version ([A-Za-z0-9._+-], not
                            # starting with "."), or "system" for the host's own PATH
```

- Errors are `invalid_params` with details `{ "line": n, "reason": s }` (line is 1-based; for
  TOML syntax errors it is where the parser stopped).
- Resolution, per entry in key order:
  - `"system"`: each required tool must be found on the daemon's `PATH`. Nothing is
    prepended and no variables are set.
  - a version: `<data dir>/toolchains/<provider>/<version>` must exist, and each required tool
    must run from it with its method's `env`. `pathPrepend` gets every `bin_dirs` entry that
    exists, in manifest order. `vars` gets every method's `env` with `${root}` and
    `${version}` expanded; a later provider's variable wins.
  - Anything missing (an unknown provider, an uninstalled version, a tool that does not run)
    is collected and reported once as `prerequisite_missing` with
    `details.missing = [toolId or providerId]`.
- `.axiom/project.lock` is written by `env/resolve` through the workspace's contained, atomic
  write (temp file + rename), only when its text changes, and never for a failed resolution or
  a workspace without `project.toml`. It holds no host paths. Example:

  ```toml
  # Written by Axiom from .axiom/project.toml. Do not edit.
  version = 1

  [[toolchain]]
  provider = "rust"
  selected = "1.84.0"
  tool = "cargo"
  version = "1.84.0"
  location = "managed"

  [[toolchain.downloads]]
  url = "https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init"
  sha256 = "…"
  ```

  The digests come from the install receipt (`<root>/.axiom-install.toml`, written at publish).
- `env/detect` reports each provider's selected version from `project.toml`.
- A process runs inside the environment with `vars` plus `PATH = pathPrepend + inherited PATH`
  in its env overlay (`axiom_env::project::process_env`). The overlay's `PATH` also decides
  which program an argv names (`axiom-exec` test `env_overlay_path_selects_the_program`).
  M2-BUILD-001 starts provider tasks this way. `task/start` (0.1) is unchanged.

## Language servers (`[[lsp]]`, M4; ADR-0018 §2)
An optional table, additive to `axiom.provider/1`:

```toml
[[lsp]]
id = "rust-analyzer"          # unique across providers
languages = ["rust"]          # LSP language ids (the shell maps file extensions to them)
command = ["rust-analyzer"]   # argv, resolved in the workspace's environment
install_hint = "Install it with: rustup component add rust-analyzer"
# initialization_options = { ... }   # merged under settings lsp.<id>.initializationOptions
# requires = ["java"]                 # M5.5: tools that must resolve for the server to run
```

The daemon starts a server on first use, when a document of one of its languages opens.

M5.5 (ADR-0021 §3, §4, §6; additive):
- A server applies to **every** workspace, whether or not its provider detects the project
  (M4 required the provider to apply). A provider whose only content is a server (`[[tools]]`,
  install methods and `[[lsp]]`) has no `[project]` table.
- Its command resolves in the workspace's environment; when not found there, in the newest
  managed install of its provider (that install's `bin_dirs` and method `env`); else the server
  is `missing` with its provider id, so the shell can offer that provider's install.
- `requires = ["toolId", ...]`: tools (an SDK, a runtime) that must resolve in the environment;
  unresolved, the server is `missing` and its detail names the provider that supplies them.
- When two servers claim a language, the one from the more specific source serves it
  (workspace, then user, then built-in; then by id); the others are `standby`.
- Nothing is installed without the person's approval unless they turned on the setting
  `lsp.autoInstall` (default `false`).

## Template variables

Only these expand, and only in the fields documented as templates:
`${version}`, `${root}` (managed install root), `${workspace}`. No shell
evaluation, no nested expansion. An unknown `${…}` is a validation error.

## Errors

Provider operations return the IPC error model. Kinds added in protocol 0.2:
`checksum_mismatch`, `unsupported_platform`, `approval_denied`,
`prerequisite_missing` (details: `{ "missing": [toolId] }`); see `ipc.md` §Errors.

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
