#!/usr/bin/env bash
# Builds (unless --no-build) and runs the end-to-end suite of the real app.
# The orchestration (private X server, tauri-driver, cleanup) is in
# e2e/runner.mjs so the same command works on Linux, macOS and Windows.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
# shellcheck disable=SC1091
source "$here/scripts/env.sh"
build=1; script=""
for a in "$@"; do case "$a" in --no-build) build=0 ;; *) script="$a" ;; esac; done

if [[ $build == 1 ]]; then
  (cd "$root" && cargo build -q -p axiomd)
  (cd "$here" && pnpm -s run build)
  (cd "$root" && cargo build -q --release -p axiom-desktop-tauri)
fi
# Never open a window on the developer's desktop: the runner starts its own
# Xvfb on Linux unless AXIOM_ALLOW_EXISTING_DISPLAY=1 explicitly asks for the
# existing display.
if [[ "${WAYLAND_DISPLAY:-}" != "" && "${AXIOM_ALLOW_EXISTING_DISPLAY:-}" != "1" ]]; then
  unset WAYLAND_DISPLAY
fi
if [[ -n "$script" ]]; then
  exec node "$here/e2e/runner.mjs" "$script"
fi
exec node "$here/e2e/runner.mjs"
