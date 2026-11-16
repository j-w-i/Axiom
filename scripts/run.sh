#!/usr/bin/env bash
# Launch the desktop app from a source checkout.
#
#   scripts/run.sh                 the Tauri shell (the default since T-14)
#   scripts/run.sh --shell qt      the legacy Qt shell (kept for one milestone)
#   scripts/run.sh --help
#
# Extra arguments after `--` are passed to the app. This opens a real window:
# tests and benchmarks must use apps/desktop-tauri/scripts/headless.sh instead.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

shell=tauri
while [[ $# -gt 0 ]]; do
  case "$1" in
    --shell) shell="${2:?--shell needs tauri or qt}"; shift 2 ;;
    --shell=*) shell="${1#--shell=}"; shift ;;
    -h|--help) sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --) shift; break ;;
    *) echo "unknown argument: $1 (see --help)" >&2; exit 2 ;;
  esac
done

case "$shell" in
  tauri)
    # shellcheck disable=SC1091
    source apps/desktop-tauri/scripts/env.sh
    (cd editor && pnpm install --frozen-lockfile)
    (cd apps/desktop-tauri && pnpm install --frozen-lockfile && pnpm run build)
    cargo build -p axiomd
    exec cargo run -p axiom-desktop-tauri -- "$@" ;;
  qt)
    # shellcheck disable=SC1091
    source scripts/dev-env.sh
    (cd editor && pnpm install --frozen-lockfile && pnpm run build)
    cargo build -p axiomd
    axiom_cmake -S apps/desktop -B build/desktop
    cmake --build build/desktop
    exec build/desktop/axiom "$@" ;;
  *) echo "unknown shell '$shell': use tauri or qt" >&2; exit 2 ;;
esac
