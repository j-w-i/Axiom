#!/usr/bin/env bash
# Launch the desktop app from a source checkout.
#
#   scripts/run.sh                 the desktop app (Tauri 2)
#   scripts/run.sh --help
#
# Extra arguments after `--` are passed to the app. This opens a real window:
# tests and benchmarks must use apps/desktop-tauri/scripts/headless.sh instead.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --) shift; break ;;
    *) echo "unknown argument: $1 (see --help; app arguments go after --)" >&2; exit 2 ;;
  esac
done

# shellcheck disable=SC1091
source apps/desktop-tauri/scripts/env.sh
(cd editor && pnpm install --frozen-lockfile)
(cd apps/desktop-tauri && pnpm install --frozen-lockfile && pnpm run build)
cargo build -p axiomd
exec cargo run -p axiom-desktop-tauri -- "$@"
