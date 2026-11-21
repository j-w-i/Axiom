# Axiom

An open-source, cross-platform, LLM-native development platform: a Flutter
desktop application over a Rust service daemon that manages workspaces,
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
scripts/check.sh               # the gate: Rust and the desktop shell
scripts/run.sh                 # launch the desktop app
scripts/tasks.py               # task board
```

The desktop shell is `apps/desktop-flutter` (Flutter, ADR-0020); it needs the Flutter SDK, and
on Linux the GTK 3 development packages (`scripts/sysroot.sh` extracts them locally without
root on Fedora). Linux and macOS for now; Windows is deferred.
The earlier shells are retired: Qt by task T-15 (tag `qt-shell-final`), Tauri by FL-QA-001
(tag `tauri-shell-final`).

## Licence

Dual-licensed under MIT or Apache-2.0, at your option
([`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE)). Dependency
licensing: [`ADR-0011`](docs/decisions/ADR-0011-licensing.md) and
[`ADR-0020`](docs/decisions/ADR-0020-flutter-shell.md) §10.

Bundled fonts: IBM Plex Sans and IBM Plex Mono, © 2017 IBM Corp. with Reserved Font
Name "Plex", under the SIL Open Font License 1.1
([`OFL.txt`](apps/desktop-flutter/assets/fonts/OFL.txt); IBM's unmodified files,
source and checksums in [`SOURCE.md`](apps/desktop-flutter/assets/fonts/SOURCE.md);
ADR-0013 §6).
