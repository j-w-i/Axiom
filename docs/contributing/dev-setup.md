# Development setup

Verified on Fedora 44 (x86_64). macOS and Windows steps follow the same
shape and are exercised by CI; report corrections.

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | stable ≥ 1.85 | `rustup`; `rust-toolchain.toml` selects stable + rustfmt + clippy |
| Node.js | ≥ 22 | for the editor bundle |
| pnpm | ≥ 9 | `npm i -g pnpm` |
| CMake | ≥ 3.21 | |
| Ninja | any | |
| C++ compiler | C++17 | GCC ≥ 11, Clang ≥ 14, **MSVC 2022 on Windows** (Qt WebEngine has no MinGW build) |
| Qt | 6.12.x (min 6.8) | modules: base, declarative, webengine, webchannel, websockets, positioning |
| Python 3 | ≥ 3.9 | `scripts/tasks.py`, aqtinstall |

## Qt without root

```sh
scripts/bootstrap-qt.sh              # installs Qt 6.12.0 into .toolchains/qt (≈2 GB)
scripts/bootstrap-qt.sh --gl-headers # Fedora only: if OpenGL/xkbcommon -devel packages are missing
source scripts/dev-env.sh            # sets AXIOM_QT, defines axiom_cmake
```
With a system or official-installer Qt instead: `export AXIOM_QT=/path/to/Qt/6.12.0/gcc_64`
before sourcing `dev-env.sh`.

Linux build dependencies if you can install packages:
Fedora `sudo dnf install mesa-libGL-devel mesa-libEGL-devel libxkbcommon-devel`;
Debian/Ubuntu `sudo apt install libgl1-mesa-dev libegl1-mesa-dev libxkbcommon-dev`.

## Build and check

```sh
scripts/check.sh              # default: fmt, clippy, tests, schema freshness, editor, Tauri shell
scripts/check.sh rust         # Rust only
scripts/check.sh editor       # editor only: install --frozen-lockfile, check, build, test
scripts/check.sh tauri        # the Tauri shell only (Linux needs WebKitGTK 4.1 / GTK 3 dev packages)
scripts/check.sh desktop-qt   # the legacy Qt shell (needs the Qt setup above; `desktop` is an alias)
scripts/check.sh everything   # the default plus the Qt shell
scripts/run.sh                # launch the Tauri shell; `--shell qt` launches the Qt shell
```
Editor development in a plain browser (fake host, sample document, no Qt):
`cd editor && pnpm install && pnpm dev`.
The Qt setup above is needed only for `desktop-qt` / `everything`. `check.sh` is the gate for every task and for CI.

## Running the app

```sh
cargo build -p axiomd
(cd editor && pnpm install && pnpm run build)
source scripts/dev-env.sh
axiom_cmake -S apps/desktop -B build/desktop && cmake --build build/desktop
build/desktop/axiom --daemon target/debug/axiomd
# (--editor-assets <dir> arrives with M1-SHELL-003; --data-dir <dir> is forwarded to axiomd)
```

## Headless GUI tests

`QT_QPA_PLATFORM=offscreen` runs the shell and WebEngine without a display
(verified in experiment E1). CI uses it on Linux. If WebEngine refuses to
start as root or in a container, set `QTWEBENGINE_DISABLE_SANDBOX=1` **for
tests only**.

## Data locations

Tests and development runs should set `AXIOM_DATA_DIR` and `AXIOM_CACHE_DIR`
to temporary directories so they never touch your real profile.
