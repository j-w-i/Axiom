#!/usr/bin/env bash
# Does the *packaged* app launch, find its sidecar daemon and connect to it?
#
# Gate criterion 6 is "packages for each OS install, launch and connect". CI
# builds the packages; this checks the part that can be checked without root:
# the packaged layout is unpacked to a temporary prefix and run exactly as an
# installed copy would be, on a private X display.
#
#   apps/desktop-tauri/scripts/verify-package.sh [path/to/package.deb|.rpm]
#
# Checks, each printed with its evidence:
#   1. the package contains the app and the daemon sidecar in the same bin dir
#   2. launched with no --daemon, the app starts that sidecar by itself
#      (the packaged sibling lookup, not a path passed on the command line)
#   3. the sidecar it started is the packaged one (not a copy from target/)
#   4. the app completes the protocol 0.1 handshake with it ("Connected")
#   5. the app's window is on the display
#
# Exit status: 0 only if every check passed.

set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
# shellcheck disable=SC1091
source "$here/scripts/env.sh"

pkg="${1:-$(ls -t "$root"/target/release/bundle/deb/*.deb 2>/dev/null | head -1)}"
[[ -n "$pkg" && -f "$pkg" ]] || { echo "verify-package: no package found (scripts/bundle.sh --bundles deb,rpm)" >&2; exit 2; }
# Absolute: the rpm branch extracts from inside the prefix directory.
pkg="$(readlink -f "$pkg")"
case "$pkg" in
  *.deb|*.rpm) ;;
  *) echo "verify-package: only .deb and .rpm can be verified here ($pkg)" >&2; exit 2 ;;
esac
(( $EUID != 0 )) || { echo "verify-package: run as a normal user, not root" >&2; exit 2; }

work="$(mktemp -d)"
app_pid=""
cleanup() {
  [[ -n "$app_pid" ]] && kill "$app_pid" 2>/dev/null || true
  [[ -n "${xvfb_pid:-}" ]] && kill "$xvfb_pid" 2>/dev/null || true
  pkill -f "$work/prefix" 2>/dev/null || true
  if [[ "${fail:-0}" == 0 ]]; then rm -rf "$work"; else echo "kept for inspection: $work"; fi
}
trap cleanup EXIT

fail=0
check() { # name, ok, detail
  if [[ "$2" == 1 ]]; then echo "PASS  $1${3:+ — $3}"; else echo "FAIL  $1${3:+ — $3}"; fail=1; fi
}

echo "package: $pkg ($(du -h "$pkg" | cut -f1))"

# ---- 1. layout: app + daemon sidecar in the same bin directory ---------------
case "$pkg" in
  *.deb) dpkg-deb -x "$pkg" "$work/prefix" ;;
  *.rpm) (mkdir -p "$work/prefix" && cd "$work/prefix" && rpm2cpio "$pkg" | cpio -idm --quiet) ;;
esac
mapfile -t bins < <(cd "$work/prefix" && find . -type f -path '*/bin/*' -printf '%P\n' | sort)
app_rel="$(printf '%s\n' "${bins[@]}" | grep -E 'bin/axiom-tauri$' || true)"
daemon_rel="$(printf '%s\n' "${bins[@]}" | grep -E 'bin/axiomd$' || true)"
check "the package ships the app and its daemon sidecar" \
  "$([[ -n "$app_rel" && -n "$daemon_rel" ]] && echo 1 || echo 0)" \
  "${app_rel:-no app} + ${daemon_rel:-no daemon}"
[[ -n "$app_rel" && -n "$daemon_rel" ]] || exit 1
[[ "$(dirname "$app_rel")" == "$(dirname "$daemon_rel")" ]] \
  && check "both are in the same directory (the sibling lookup can find it)" 1 "$(dirname "$app_rel")" \
  || { check "both are in the same directory" 0 "$(dirname "$app_rel") vs $(dirname "$daemon_rel")"; exit 1; }

app="$work/prefix/$app_rel"
daemon="$work/prefix/$daemon_rel"
chmod +x "$app" "$daemon"

# ---- a private display, so nothing appears on the developer's screen ---------
if [[ "${AXIOM_ALLOW_EXISTING_DISPLAY:-}" == "1" ]]; then
  echo "using the existing DISPLAY $DISPLAY (AXIOM_ALLOW_EXISTING_DISPLAY=1)"
else
  xvfb="${AXIOM_XVFB:-/usr/bin/Xvfb}"
  [[ -x "$xvfb" ]] || { echo "verify-package: no Xvfb (set AXIOM_XVFB)" >&2; exit 2; }
  n=$(( 60 + RANDOM % 30 ))
  "$xvfb" ":$n" -screen 0 1280x800x24 -nolisten tcp >/dev/null 2>&1 &
  xvfb_pid=$!
  for _ in $(seq 100); do [[ -e "/tmp/.X11-unix/X$n" ]] && break; sleep 0.05; done
  export DISPLAY=":$n" GDK_BACKEND=x11 XDG_SESSION_TYPE=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1
  unset WAYLAND_DISPLAY WAYLAND_SOCKET
fi

# ---- 2/3. launch with NO --daemon: the packaged sibling must be found -------
# Nothing is passed on the command line, so the only way the app can start a
# daemon is locate_daemon's sibling lookup (the packaged layout).
data_a="$work/data-sibling"
mkdir -p "$data_a"
"$app" --data-dir "$data_a" >"$work/app-sibling.log" 2>&1 &
app_a=$!
child=""
for _ in $(seq 200); do
  child="$(pgrep -P "$app_a" -x axiomd 2>/dev/null | head -1 || true)"
  [[ -n "$child" ]] && break
  kill -0 "$app_a" 2>/dev/null || break
  sleep 0.1
done
check "with no --daemon, the app finds and starts the packaged sidecar" \
  "$([[ -n "$child" ]] && echo 1 || echo 0)" \
  "${child:+pid $child}${child:+ }$( [[ -n "$child" ]] && readlink -f "/proc/$child/exe" 2>/dev/null || echo "no child")"
exe_a=""
[[ -n "$child" ]] && exe_a="$(readlink -f "/proc/$child/exe" 2>/dev/null || true)"
check "the sidecar it started is the packaged binary" \
  "$([[ "$exe_a" == "$(readlink -f "$daemon")" ]] && echo 1 || echo 0)" \
  "${exe_a:-unknown}"
kill "$app_a" 2>/dev/null || true
wait "$app_a" 2>/dev/null || true
app_a=""

# ---- 4. the protocol 0.1 handshake, observed on the wire --------------------
# Through the benchmark tap (which forwards bytes unchanged and records every
# protocol line), so "Connected" is seen on the wire, not inferred from a pixel.
export AXIOM_TAP_LOG="$work/tap.jsonl" AXIOM_TAP_REAL="$daemon"
tap="$here/bench/daemon_tap.py"
data="$work/data"
mkdir -p "$data"
"$app" --daemon "$tap" --data-dir "$data" >"$work/app.log" 2>&1 &
app_pid=$!

for _ in $(seq 300); do
  grep -q '"event": "response", .*"method": "initialize"' "$work/tap.jsonl" 2>/dev/null && break
  kill -0 "$app_pid" 2>/dev/null || break
  sleep 0.1
done

spawned=$(grep -c '"event": "spawn"' "$work/tap.jsonl" 2>/dev/null || true)
ready=$(grep -c '"event": "response", .*"method": "initialize"' "$work/tap.jsonl" 2>/dev/null || true)
daemon_pid=$(python3 - "$work/tap.jsonl" <<'PY'
import json, sys
pid = None
try:
    for line in open(sys.argv[1], encoding="utf-8"):
        e = json.loads(line)
        if e.get("event") == "daemon_pid":
            pid = e["pid"]
except FileNotFoundError:
    pass
print(pid or "")
PY
)
exe_target=""
if [[ -n "$daemon_pid" && -e "/proc/$daemon_pid/exe" ]]; then
  exe_target="$(readlink -f "/proc/$daemon_pid/exe" 2>/dev/null || true)"
fi
check "the daemon it talks to is the packaged one" \
  "$([[ "$exe_target" == "$(readlink -f "$daemon")" ]] && echo 1 || echo 0)" \
  "${exe_target:-unknown}"
check "the app completes the protocol 0.1 handshake (Connected)" \
  "$([[ "${ready:-0}" -ge 1 ]] && echo 1 || echo 0)" \
  "tap saw ${spawned:-0} spawn(s), ${ready:-0} initialize answer(s)"
if [[ "${ready:-0}" -lt 1 ]]; then
  echo "--- tap events:"; sed -n 1,10p "$work/tap.jsonl" 2>/dev/null || echo "(no tap log)"
  echo "--- app log:"; sed -n 1,10p "$work/app.log" 2>/dev/null || echo "(no app log)"
fi

# ---- 5. a window exists on the display --------------------------------------
# The bench virtualenv has python-xlib; the system Python does not.
xpy="$root/.toolchains/bench-venv/bin/python"
[[ -x "$xpy" ]] || xpy="$(command -v python3)"
"$xpy" - "$app_pid" <<'PY' && win_ok=1 || win_ok=0
import sys
from Xlib import display
d = display.Display()
root = d.screen().root
names = []
def walk(w):
    try:
        for c in w.query_tree().children:
            names.append(c.get_wm_name() or "")
            walk(c)
    except Exception:
        pass
walk(root)
sys.exit(0 if any(n and ("Axiom" in n or "axiom" in n.lower()) for n in names) else 1)
PY
check "the app's window is on the display" "$win_ok" "private display $DISPLAY"

echo
if [[ "$fail" == 0 ]]; then echo "package verification: all checks passed"; else echo "package verification: FAILED (see above)"; fi
exit "$fail"
