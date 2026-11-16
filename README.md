# Axiom

An open-source, cross-platform, LLM-native development platform: a Tauri 2
desktop application (React + Monaco; the Qt shell is the legacy fallback, see
below) over a Rust service daemon that manages workspaces,
toolchains and builds, hosts AI agents with structured tools and
permissions, and can map elements of a running UI back to their source so
that AI-made changes are verified rather than assumed.

**Status: pre-alpha.** The architecture, contracts and first-milestone task
specifications are complete; application code is about to start. Nothing is
usable yet. See [`docs/roadmap/current.md`](docs/roadmap/current.md).

- Contributors and agents: start with [`AGENTS.md`](AGENTS.md).
- Architecture: [`docs/architecture/overview.md`](docs/architecture/overview.md).
- Decisions: [`docs/decisions/`](docs/decisions/README.md).
- Setup: [`docs/contributing/dev-setup.md`](docs/contributing/dev-setup.md).

```sh
scripts/check.sh               # default gate: Rust, editor and the Tauri shell
scripts/run.sh                 # launch the Tauri shell (the default since T-14)
scripts/run.sh --shell qt      # launch the legacy Qt shell (kept for one milestone)
scripts/check.sh desktop-qt    # test the legacy Qt shell (`desktop` is an alias)
scripts/tasks.py               # task board
```

On Linux the Tauri shell needs the WebKitGTK 4.1 / GTK 3 development packages
(`apps/desktop-tauri/scripts/sysroot.sh` extracts them locally without root).
The Qt shell stays buildable and tested until task T-15 retires it.

## Licence

Dual-licensed under MIT or Apache-2.0, at your option
([`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE)). Qt is
used under LGPL-3.0; see
[`docs/decisions/ADR-0011-licensing.md`](docs/decisions/ADR-0011-licensing.md).
