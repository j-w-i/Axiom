# Development setup

Verified on Fedora 44 (x86_64). macOS and Windows steps follow the same
shape and are exercised by CI; report corrections.

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | stable ≥ 1.85 | `rustup`; `rust-toolchain.toml` selects stable + rustfmt + clippy |
| Flutter | stable 3.50.1 | the desktop shell (ADR-0020); see §Flutter |
| Python 3 | ≥ 3.9 | `scripts/tasks.py`, `scripts/check_deps.py`, `scripts/check_dart_licences.py` |
| Linux only | clang, CMake, Ninja, pkg-config, GTK 3 dev packages | for the shell's Linux build; without root (Fedora) run `scripts/sysroot.sh`; `xvfb` (or `kwin_wayland`) for the end-to-end suites |
| macOS | Xcode | for the shell's macOS build; see the CI `flutter` job |
| Node.js | optional | only for the real TypeScript language server in `lsp_test.dart` and for agent harnesses the daemon installs with `npm` |

Windows: the Rust workspace builds and is tested in CI; there is no Windows desktop build of the
shell yet (deferred by the owner).

## Build and check

```sh
scripts/check.sh              # everything: the Rust workspace and the shell
scripts/check.sh rust         # Rust only (+ schema freshness, task board, dependency matrix, deny)
scripts/check.sh flutter      # the shell only: lockfile, wire types, format, analyze, tests, licences
scripts/check.sh e2e          # the shell's end-to-end suites, in a private compositor (also: flutter-e2e)
scripts/check.sh package      # build the deb + rpm (macOS: Axiom.app) and verify them as installed copies
scripts/run.sh                # launch the desktop app
```
`check.sh` is the gate for every task and for CI. `scripts/run.sh -- --gallery` shows the
design system's gallery (`lib/ui/gallery.dart`) instead of the app.

## Flutter

The desktop shell (`apps/desktop-flutter`, ADR-0020) needs the Flutter SDK. The scripts look
for it on `PATH`, then in `~/flutter`:

```sh
git clone --depth 1 -b stable https://github.com/flutter/flutter.git ~/flutter
~/flutter/bin/flutter config --no-analytics --enable-linux-desktop
~/flutter/bin/flutter precache --linux
```

The Linux desktop build also needs clang, CMake, Ninja, pkg-config and the GTK 3 development
files (Fedora: `clang cmake ninja-build gtk3-devel`; Ubuntu: `clang cmake ninja-build
libgtk-3-dev`). Without root on Fedora, `scripts/sysroot.sh` downloads the GTK 3 development
files and Xvfb into `.toolchains/gtk-sysroot`; `source scripts/env.sh` before
`flutter build linux` (the scripts do it themselves).

After a protocol change, regenerate the shell's wire types from `schemas/`:
`cd apps/desktop-flutter && dart run tool/gen_types.dart` (`--check` is what `check.sh` runs).

## Running the app

```sh
scripts/run.sh                # builds a debug axiomd and a release build of the shell, then launches
scripts/run.sh -- <app args>  # e.g. -- --data-dir /tmp/axiom-try
```
On Linux it also keeps a desktop entry and the icons in `~/.local/share` (`dev.axiom.Axiom`) so
the window shows Axiom's name and icon; `AXIOM_NO_DESKTOP_ENTRY=1` skips that. This opens a
real window; tests never use it.

Packages: `scripts/bundle.sh [out-dir]` (default `target/bundle`) builds a `.deb` and an `.rpm`
on Linux (`/usr/lib/axiom/` with `axiom` and `axiomd` side by side, `/usr/bin/axiom`, the
desktop entry `dev.axiom.Axiom.desktop`, hicolor icons) and a zipped `Axiom.app` on macOS
(`axiomd` in `Contents/MacOS`). `scripts/verify-package.sh <package>` unpacks a deb or rpm
into a prefix (`--app Axiom.app` checks the bundle in place) and runs it with `--self-test`,
which must find the sibling `axiomd` and print `connected …`.

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

Never open a window on your own desktop from a test. On Linux:
- `apps/desktop-flutter/scripts/e2e.sh [suite_test.dart …]` (= `scripts/check.sh e2e`) runs
  the end-to-end suites in a private `kwin_wayland --virtual` when it is installed, else in a
  private Xvfb (`scripts/headless.sh`); `AXIOM_E2E_DISPLAY=xvfb` forces Xvfb.
  `AXIOM_E2E_NETWORK=1` adds the M2 install scenario (`env_rust_test.dart`, needs the internet);
  `AXIOM_REQUIRE_LSP=1` makes a missing real language server fail instead of skip.
- `apps/desktop-flutter/scripts/frames.sh` measures frame timings in a profile build in a
  private virtual KWin at 2560×1440, 125 % (`AXIOM_FRAMES_TARGET=editor_frames_test.dart` or
  `chat_frames_test.dart` for the other benchmarks); it needs `kwin_wayland`.
- Anything else GUI: `source scripts/env.sh; source scripts/headless.sh` (a private Xvfb,
  Wayland removed, private XDG directories; it refuses to run without Xvfb), then
  `headless_stop`.
`scripts/check.sh package` checks the deb and rpm as installed copies.

## Data locations

Tests and development runs should set `AXIOM_DATA_DIR` and `AXIOM_CACHE_DIR`
to temporary directories so they never touch your real profile.
