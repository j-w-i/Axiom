#!/usr/bin/env bash
# Frame timings of the Flutter shell (ADR-0020 §5) in a profile build, in a private virtual KWin
# at the owner's display: 2560×1440 at 125 %, the window at 2040×1120 logical (maximised there).
#
#   apps/desktop-flutter/scripts/frames.sh        AXIOM_FRAMES_ASSERT=0 to report without failing
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH:$HOME/flutter/bin"
(cd "$root" && cargo build -q -p axiomd)
export AXIOMD_UNDER_TEST="$root/target/debug/axiomd"
# shellcheck disable=SC1091
source "$root/apps/desktop-tauri/scripts/env.sh"
command -v kwin_wayland >/dev/null || { echo "frames.sh: needs kwin_wayland (a private compositor)" >&2; exit 2; }
sock="axiom-flutter-frames-$$"
kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --socket "$sock" \
  --width "${AXIOM_FRAMES_SCREEN_W:-2560}" --height "${AXIOM_FRAMES_SCREEN_H:-1440}" --scale "${AXIOM_FRAMES_SCALE:-1.25}" >/dev/null 2>&1 &
kwin=$!
trap 'kill $kwin 2>/dev/null || true' EXIT
for _ in $(seq 80); do [[ -S "$XDG_RUNTIME_DIR/$sock" ]] && break; sleep 0.1; done
unset DISPLAY
export WAYLAND_DISPLAY="$sock" GDK_BACKEND=wayland AXIOM_WINDOW_SIZE="${AXIOM_WINDOW_SIZE:-2040x1120}"
cd "$here"
flutter drive --profile -d linux --driver=test_driver/integration_test.dart --target=integration_test/${AXIOM_FRAMES_TARGET:-frames_test.dart}
