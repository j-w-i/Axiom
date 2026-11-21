# Development setup

Verified on Fedora 44 (x86_64). macOS and Windows steps follow the same
shape and are exercised by CI; report corrections.

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | stable ≥ 1.85 | `rustup`; `rust-toolchain.toml` selects stable + rustfmt + clippy |
| Node.js | ≥ 22 | for the editor bundle |
| pnpm | ≥ 9 | `npm i -g pnpm` |
| Python 3 | ≥ 3.9 | `scripts/tasks.py`, `scripts/check_deps.py` |
| Linux only | WebKitGTK 4.1 + GTK 3 dev packages | for the Tauri shell; without root run `apps/desktop-tauri/scripts/sysroot.sh` |
| macOS / Windows | Xcode CLT / WebView2 runtime + MSVC build tools | see the CI `tauri` job for the exact steps |

## Build and check

```sh
scripts/check.sh              # everything: fmt, clippy, tests, schema freshness, editor, Tauri shell
scripts/check.sh rust         # Rust only
scripts/check.sh editor       # editor only: install --frozen-lockfile, check, build, test
scripts/check.sh tauri        # the Tauri shell only (Linux needs WebKitGTK 4.1 / GTK 3 dev packages)
scripts/run.sh                # launch the desktop app
```
Editor development in a plain browser (fake host, sample document):
`cd editor && pnpm install && pnpm dev`.
`check.sh` is the gate for every task and for CI.

## Running the app

```sh
scripts/run.sh                # builds the editor bundle, the UI, axiomd and the host, then launches
```

## The command line (`axiom`)

`cargo build -p axiom-cli -p axiomd` builds `target/debug/axiom` next to the daemon. With the
desktop app running it shares the app's daemon (the socket in the data dir); otherwise it
starts one for the command.

```sh
target/debug/axiom task run -- cargo test               # in the current folder; exits with the code
target/debug/axiom build build                          # a provider task
target/debug/axiom agent send --chat "Explain RAII"     # a conversation without a workspace
target/debug/axiom agent send "Fix the failing test"    # the agent in the current folder
target/debug/axiom agent sessions
```

## Headless GUI tests

End-to-end and benchmark runs go through `apps/desktop-tauri/scripts/headless.sh`
(a private Xvfb on Linux); never open a window on your own desktop from a test.
`scripts/check.sh e2e` runs the suites; `scripts/check.sh package` checks the deb
and rpm as installed copies.

## Data locations

Tests and development runs should set `AXIOM_DATA_DIR` and `AXIOM_CACHE_DIR`
to temporary directories so they never touch your real profile.
