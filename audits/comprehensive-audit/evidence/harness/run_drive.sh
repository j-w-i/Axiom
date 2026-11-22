#!/usr/bin/env bash
# Profile build under a private virtual KWin at the owner's scale (as frames.sh).
set -uo pipefail
root="$(cd "$(dirname "$0")/../../../.." && pwd)"
app="$1"; target="$2"
export PATH="$HOME/.cargo/bin:$PATH:$HOME/flutter/bin"
export AXIOMD_UNDER_TEST="${AXIOMD:-$root/target/debug/axiomd}"
export AUDIT_OUT="$root/audits/comprehensive-audit/evidence"
source "$root/scripts/env.sh"
sock="axiom-audit-drive-$$"
kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --socket "$sock" --width 2560 --height 1440 --scale 1.25 >/dev/null 2>&1 &
kwin=$!
trap 'kill $kwin 2>/dev/null || true' EXIT
for _ in $(seq 80); do [[ -S "$XDG_RUNTIME_DIR/$sock" ]] && break; sleep 0.1; done
unset DISPLAY
export WAYLAND_DISPLAY="$sock" GDK_BACKEND=wayland AXIOM_WINDOW_SIZE="${AXIOM_WINDOW_SIZE:-2040x1120}"
cd "$app"
flutter drive --profile -d linux --driver=test_driver/integration_test.dart --target="integration_test/$target"
