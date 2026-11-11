# Axiom

An open-source, cross-platform, LLM-native development platform: a Qt
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
scripts/check.sh      # build, lint and test everything that exists
scripts/tasks.py      # task board
```

## Licence

Dual-licensed under MIT or Apache-2.0, at your option
([`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE)). Qt is
used under LGPL-3.0; see
[`docs/decisions/ADR-0011-licensing.md`](docs/decisions/ADR-0011-licensing.md).
