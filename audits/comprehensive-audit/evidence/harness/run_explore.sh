#!/usr/bin/env bash
set -uo pipefail
root="$(cd "$(dirname "$0")/../../../.." && pwd)"
app="$1"; target="${2:-audit_explore_test.dart}"
export PATH="$HOME/.cargo/bin:$PATH:$HOME/flutter/bin"
export AXIOMD_UNDER_TEST="$root/target/debug/axiomd"
export AUDIT_OUT="$root/audits/comprehensive-audit/evidence"
source "$root/scripts/env.sh"
sock="axiom-audit-$$"
kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --socket "$sock" --width 2560 --height 1440 --scale 1 >/dev/null 2>&1 &
kwin=$!
trap 'kill $kwin 2>/dev/null || true' EXIT
for _ in $(seq 80); do [[ -S "$XDG_RUNTIME_DIR/$sock" ]] && break; sleep 0.1; done
[[ -S "$XDG_RUNTIME_DIR/$sock" ]] || { echo "no kwin"; exit 1; }
unset DISPLAY
export WAYLAND_DISPLAY="$sock" GDK_BACKEND=wayland AXIOM_WINDOW_SIZE=1440x900
cd "$app"
flutter test "integration_test/$target" -d linux ${AUDIT_FLAGS:-}
