#!/usr/bin/env bash
# Qt vs Tauri benchmarks on a private, invisible Xvfb (never the user's screen).
# Builds release binaries of axiomd and the Tauri app; uses the existing Qt
# build (build/desktop/axiom) — rebuild it in Release for a fair comparison
# when possible (performance.md §Fairness).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
# shellcheck disable=SC1091
source "$here/scripts/env.sh"
(cd "$root" && cargo build -q --release -p axiomd)
(cd "$here" && pnpm -s run build >/dev/null)
(cd "$root" && cargo build -q --release -p axiom-desktop-tauri)
# RANDR off: python-xlib cannot parse RANDR error events Xvfb may emit.
export AXIOM_XVFB_EXTRA="-extension RANDR"
# shellcheck disable=SC1091
source "$here/scripts/headless.sh"
trap headless_stop EXIT
mkdir -p "$here/bench/results"
out="$here/bench/results/$(hostname -s)-$(date +%Y%m%d-%H%M%S).json"
"$root/.toolchains/bench-venv/bin/python" "$here/bench/run.py" "$@" | tee "$out"
echo "results: $out" >&2
