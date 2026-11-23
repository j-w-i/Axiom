#!/usr/bin/env bash
# Performance measurements of the desktop app that the frame gate (frames.sh) does not cover
# (audit AUD-061; budgets in docs/design/performance.md): startup (cold and warm, to the first
# frame, the daemon ready and a folder listed), idle CPU and PSS of the release app, and the
# profile-mode probe (memory over repeated work, a 100 000-line file, search, console floods,
# typing under streaming output). Everything runs in a private compositor, never on the desktop.
#
#   scripts/perf.sh [--runs N] [--skip-probe]      results: target/perf/<host>-<time>.json
#
# Compare two builds by running it on each, interleaved, on the same machine; absolute numbers
# carry the machine's load (recorded in the result).
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
export PATH="$HOME/.cargo/bin:$PATH:$HOME/flutter/bin"
runs=5; probe=1
while [[ $# -gt 0 ]]; do
  case "$1" in
    --runs) runs="$2"; shift 2 ;;
    --skip-probe) probe=0; shift ;;
    -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[[ "$(uname -s)" == Linux ]] || { echo "perf.sh: Linux only for now" >&2; exit 2; }
command -v kwin_wayland >/dev/null || { echo "perf.sh: needs kwin_wayland (a private compositor)" >&2; exit 2; }
# shellcheck disable=SC1091
source scripts/env.sh
cargo build -q --release -p axiomd
(cd apps/desktop-flutter && flutter build linux --release >/dev/null)
bundle="$root/apps/desktop-flutter/build/linux/x64/release/bundle/axiom"
daemon="$root/target/release/axiomd"
out="$root/target/perf"; mkdir -p "$out"
result="$out/$(hostname -s)-$(date +%Y%m%d-%H%M%S).json"

sock="axiom-perf-$$"
kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --socket "$sock" \
  --width 2560 --height 1440 --scale "${AXIOM_PERF_SCALE:-1.25}" >/dev/null 2>&1 &
kwin=$!
work=$(mktemp -d)
trap 'kill $kwin 2>/dev/null || true; rm -rf "$work"' EXIT
for _ in $(seq 80); do [[ -S "$XDG_RUNTIME_DIR/$sock" ]] && break; sleep 0.1; done
[[ -S "$XDG_RUNTIME_DIR/$sock" ]] || { echo "perf.sh: the private KWin did not start" >&2; exit 1; }
unset DISPLAY
export WAYLAND_DISPLAY="$sock" GDK_BACKEND=wayland AXIOM_WINDOW_SIZE="${AXIOM_WINDOW_SIZE:-2040x1120}"

ws="$work/ws"; mkdir -p "$ws/src"
for i in $(seq 200); do echo "fn f$i() {}" > "$ws/src/f$i.rs"; done
printf 'fn main() {\n    println!("hi");\n}\n' > "$ws/src/main.rs"
warm_cache="$work/warm-cache"; warm_data="$work/warm-data"; mkdir -p "$warm_cache" "$warm_data"

# One launch: prints "main first_frame ready folder" in ms after exec.
launch() { # cache data hold_ms
  local t0 err
  err="$work/err"
  t0=$(date +%s%N)
  XDG_CACHE_HOME="$1" AXIOM_STARTUP_TRACE=1 AXIOM_STARTUP_OPEN="$ws" AXIOM_STARTUP_FILE=src/main.rs \
    AXIOM_STARTUP_HOLD_MS="$3" "$bundle" --daemon "$daemon" --data-dir "$2" 2>"$err" >/dev/null
  local base=$((t0 / 1000)) line
  for k in main first_frame ready folder; do
    line=$(grep -o "AXIOM_STARTUP $k=[0-9]*" "$err" | cut -d= -f2)
    printf '%s ' "$(( (line - base) / 1000 ))"
  done
  echo
}
cold=(); warm=()
for _ in $(seq "$runs"); do
  c="$work/c"; d="$work/d"; rm -rf "$c" "$d"; mkdir -p "$c" "$d"
  cold+=("$(launch "$c" "$d" 300)")
  warm+=("$(launch "$warm_cache" "$warm_data" 300)")
done

# Idle: folder and document open, 10 s sampled after 15 s.
err="$work/idle-err"
XDG_CACHE_HOME="$warm_cache" AXIOM_STARTUP_TRACE=1 AXIOM_STARTUP_OPEN="$ws" AXIOM_STARTUP_FILE=src/main.rs \
  AXIOM_STARTUP_HOLD_MS=30000 "$bundle" --daemon "$daemon" --data-dir "$warm_data" 2>"$err" >/dev/null &
app=$!
sleep 15
dpid=$(grep -o 'daemon=[0-9]*' "$err" | cut -d= -f2)
ticks() { awk '{print $14+$15}' "/proc/$1/stat"; }
pss() { awk '/^Pss:/{print $2}' "/proc/$1/smaps_rollup"; }
hz=$(getconf CLK_TCK)
a0=$(ticks $app); d0=$(ticks "$dpid"); sleep 10; a1=$(ticks $app); d1=$(ticks "$dpid")
idle_shell=$(awk -v t=$((a1 - a0)) -v hz="$hz" 'BEGIN{printf "%.2f", t / hz / 10 * 100}')
idle_daemon=$(awk -v t=$((d1 - d0)) -v hz="$hz" 'BEGIN{printf "%.2f", t / hz / 10 * 100}')
pss_shell=$(( $(pss $app) / 1024 )); pss_daemon=$(( $(pss "$dpid") / 1024 ))
wait $app || true

probe_json=null
if [[ "$probe" == 1 ]]; then
  (cd apps/desktop-flutter && AXIOMD_UNDER_TEST="$daemon" AXIOM_PERF_OUT="$work" \
    flutter drive --profile -d linux --driver=test_driver/integration_test.dart \
      --target=integration_test/perf_probe_test.dart >"$work/probe.log" 2>&1) \
    || { tail -30 "$work/probe.log" >&2; echo "perf.sh: the probe failed" >&2; exit 1; }
  probe_json=$(cat "$work"/probe-*.json)
fi

python3 - "$result" "$runs" "$idle_shell" "$idle_daemon" "$pss_shell" "$pss_daemon" "$probe_json" \
  "${cold[@]}" "${warm[@]}" <<'PY'
import json, os, statistics, sys
result, runs = sys.argv[1], int(sys.argv[2])
idle_shell, idle_daemon, pss_shell, pss_daemon, probe = sys.argv[3:8]
rows = [list(map(int, r.split())) for r in sys.argv[8:]]
cold, warm = rows[:runs], rows[runs:]
keys = ['main', 'first_frame', 'ready', 'folder']
def med(rs):
    return {k: statistics.median(r[i] for r in rs) for i, k in enumerate(keys)}
out = {
    'load_average': os.getloadavg(),
    'startup_ms': {'cold_median': med(cold), 'warm_median': med(warm), 'cold': cold, 'warm': warm},
    'idle': {'cpu_pct_shell': float(idle_shell), 'cpu_pct_daemon': float(idle_daemon),
             'pss_mib_shell': int(pss_shell), 'pss_mib_daemon': int(pss_daemon)},
    'probe': json.loads(probe),
}
open(result, 'w').write(json.dumps(out, indent=2))
print(json.dumps({k: out[k] for k in ('load_average', 'idle')}, indent=2))
print('startup cold', out['startup_ms']['cold_median'], 'warm', out['startup_ms']['warm_median'])
print('result:', result)
PY
