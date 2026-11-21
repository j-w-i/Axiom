#!/usr/bin/env bash
# Runs the Flutter shell's end-to-end suites (integration_test) against the real daemon, in a
# private compositor: never on the developer's desktop (AGENTS.md).
#
#   apps/desktop-flutter/scripts/e2e.sh [suite_test.dart ...]   (default: every suite)
#
# Linux: a private virtual KWin (Wayland, the GPU) when kwin_wayland exists, else a private Xvfb
# (apps/desktop-tauri/scripts/headless.sh). macOS: the suites run in the session (CI).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH:$HOME/flutter/bin"
cd "$root"
cargo build -q -p axiomd -p fake-lsp -p fake-acp
# The fake language server (tests/helpers/fake-lsp) is found on PATH by the daemon.
export PATH="$root/target/debug:$PATH"
export AXIOMD_UNDER_TEST="${AXIOMD_UNDER_TEST:-$root/target/debug/axiomd}"
cd "$here"
suites=("$@")
if [[ ${#suites[@]} -eq 0 ]]; then mapfile -t suites < <(cd integration_test && ls *_test.dart | grep -v frames_test); fi

cleanup() { [[ -n "${kwin_pid:-}" ]] && kill "$kwin_pid" 2>/dev/null || true; }
trap cleanup EXIT
if [[ "$(uname -s)" == Linux ]]; then
  # shellcheck disable=SC1091
  source "$root/apps/desktop-tauri/scripts/env.sh"
  if command -v kwin_wayland >/dev/null && [[ "${AXIOM_E2E_DISPLAY:-}" != xvfb ]]; then
    sock="axiom-flutter-e2e-$$"
    kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --socket "$sock" \
      --width "${AXIOM_E2E_WIDTH:-2560}" --height "${AXIOM_E2E_HEIGHT:-1440}" --scale "${AXIOM_E2E_SCALE:-1}" \
      >/dev/null 2>&1 &
    kwin_pid=$!
    for _ in $(seq 80); do [[ -S "$XDG_RUNTIME_DIR/$sock" ]] && break; sleep 0.1; done
    [[ -S "$XDG_RUNTIME_DIR/$sock" ]] || { echo "e2e: the private KWin did not start" >&2; exit 1; }
    unset DISPLAY
    export WAYLAND_DISPLAY="$sock" GDK_BACKEND=wayland
    echo "[e2e] private virtual KWin: $sock"
  else
    # shellcheck disable=SC1091
    source "$root/apps/desktop-tauri/scripts/headless.sh"
    echo "[e2e] private Xvfb: $DISPLAY"
  fi
fi
fail=0
for s in "${suites[@]}"; do
  echo "[e2e] ===== $s"
  flutter test "integration_test/$s" -d "$( [[ "$(uname -s)" == Darwin ]] && echo macos || echo linux )" ${AXIOM_E2E_FLAGS:-} || fail=1
done
exit $fail
