#!/usr/bin/env bash
# One command: build everything and run the desktop smoke tests locally.
# A thin wrapper over `scripts/check.sh desktop`, which already builds axiomd and the
# emit helper, configures and builds the Qt app (including the editor bundle copy),
# and runs ctest offscreen. The editor bundle is built first because the app and the
# editor smoke test need editor/dist.
#
# Usage: scripts/smoke.sh            (offscreen; never opens a window on your desktop)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}"

printf '==> editor bundle\n'
(cd editor && pnpm install --frozen-lockfile && pnpm run build)
exec scripts/check.sh desktop
