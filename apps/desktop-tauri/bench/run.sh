#!/usr/bin/env bash
# Benchmarks (Tauri shell; the Qt comparison runs only if a Qt build still exists) on a private, invisible Xvfb (never the user's screen).
# Builds release binaries of axiomd and the Tauri app; uses the existing Qt
# build (build/desktop/axiom) — rebuild it in Release for a fair comparison
# when possible (performance.md §Fairness).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
# shellcheck disable=SC1091
source "$here/scripts/env.sh"
# AXIOM_BENCH_NO_BUILD=1 skips the builds: a build leaves the load average high
# for minutes, and the baseline must be measured under load < 1.0.
if [[ "${AXIOM_BENCH_NO_BUILD:-0}" != 1 ]]; then
(cd "$root" && cargo build -q --release -p axiomd)
# The M1.6 scenarios drive an e2e-mode build (WebDriver server + the probe in
# src/e2e-probe.ts). It is kept apart as axiom-tauri-e2e; every other scenario
# measures the production build, built last so it is the one at target/release/axiom-tauri.
(cd "$here" && pnpm -s run build:e2e >/dev/null)
(cd "$root" && cargo build -q --release -p axiom-desktop-tauri --features e2e)
cp "$root/target/release/axiom-tauri" "$root/target/release/axiom-tauri-e2e"
(cd "$here" && pnpm -s run build >/dev/null)
(cd "$root" && cargo build -q --release -p axiom-desktop-tauri)
fi
# RANDR off: python-xlib cannot parse RANDR error events Xvfb may emit.
export AXIOM_XVFB_EXTRA="-extension RANDR"
# shellcheck disable=SC1091
source "$here/scripts/headless.sh"
trap headless_stop EXIT
mkdir -p "$here/bench/results"
out="$here/bench/results/$(hostname -s)-$(date +%Y%m%d-%H%M%S).json"
"$root/.toolchains/bench-venv/bin/python" "$here/bench/run.py" "$@" | tee "$out"
echo "results: $out" >&2
