# Axiom

An open-source, cross-platform, LLM-native development platform: a Tauri 2
desktop application (React + Monaco) over a Rust service daemon that manages workspaces,
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
scripts/check.sh               # the gate: Rust, editor and the desktop shell
scripts/run.sh                 # launch the desktop app
scripts/tasks.py               # task board
```

On Linux the Tauri shell needs the WebKitGTK 4.1 / GTK 3 development packages
(`apps/desktop-tauri/scripts/sysroot.sh` extracts them locally without root).
The earlier Qt shell was retired by task T-15; it lives on at the tag `qt-shell-final`.

## Licence

Dual-licensed under MIT or Apache-2.0, at your option
([`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE)). Dependency
licensing: [`docs/decisions/ADR-0012-desktop-shell-tauri.md`](docs/decisions/ADR-0012-desktop-shell-tauri.md)
and [`ADR-0011`](docs/decisions/ADR-0011-licensing.md).

Bundled fonts: IBM Plex Sans and IBM Plex Mono, © 2017 IBM Corp. with Reserved Font
Name "Plex", under the SIL Open Font License 1.1
([`OFL.txt`](apps/desktop-tauri/src/assets/fonts/ibm-plex/OFL.txt); IBM's unmodified files,
source and checksums in [`SOURCE.md`](apps/desktop-tauri/src/assets/fonts/ibm-plex/SOURCE.md);
ADR-0013 §6).
