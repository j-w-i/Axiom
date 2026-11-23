#!/usr/bin/env bash
# Runs the Flutter shell's end-to-end suites (integration_test) against the real daemon, in a
# private compositor: never on the developer's desktop (AGENTS.md).
#
#   apps/desktop-flutter/scripts/e2e.sh [suite_test.dart ...]   (default: every suite)
#
# Linux: a private virtual KWin (Wayland, the GPU) when kwin_wayland exists, else a private Xvfb
# (scripts/headless.sh). macOS: the suites run in the session (CI).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH:$HOME/flutter/bin"
cd "$root"
cargo build -q -p axiomd -p fake-lsp -p fake-acp -p fake-mcp
# The fake language server (tests/helpers/fake-lsp) is found on PATH by the daemon.
export PATH="$root/target/debug:$PATH"
export AXIOMD_UNDER_TEST="${AXIOMD_UNDER_TEST:-$root/target/debug/axiomd}"
cd "$here"
suites=("$@")
if [[ ${#suites[@]} -eq 0 ]]; then
  # Every suite but the frame benchmarks (bash 3.2 on macOS: no mapfile).
  while IFS= read -r f; do suites+=("$f"); done < <(cd integration_test && ls -- *_test.dart | grep -v -e frames_test -e perf_probe_test)
fi

cleanup() { [[ -n "${kwin_pid:-}" ]] && kill "$kwin_pid" 2>/dev/null || true; }
trap cleanup EXIT
if [[ "$(uname -s)" == Linux ]]; then
  # shellcheck disable=SC1091
  source "$root/scripts/env.sh"
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
    source "$root/scripts/headless.sh"
    echo "[e2e] private Xvfb: $DISPLAY"
  fi
fi
fail=0
summary=()
for s in "${suites[@]}"; do
  echo "[e2e] ===== $s"
  log="$(mktemp)"
  # A failing suite must not end the run (set -e with pipefail would): its status is kept.
  set +e
  flutter test "integration_test/$s" -d "$( [[ "$(uname -s)" == Darwin ]] && echo macos || echo linux )" ${AXIOM_E2E_FLAGS:-} 2>&1 | tee "$log"
  status=${PIPESTATUS[0]}
  set -e
  [[ $status -eq 0 ]] || fail=1
  # Skipped cases are reported, not counted as passes (audit AUD-060). Two reporters: the
  # terminal's "+3 ~1: All tests passed!" and, under GitHub Actions, the github reporter's
  # summary, a line of its own: "🎉 4 tests passed, 2 skipped.", "::error::2 tests passed,
  # 1 failed." (shown as "##[error]…"), with ", N flaky" before the skips. Only that whole line
  # is read, never "N skipped" from a test's own output (review N17).
  # A summary line that is not found is no skip (grep must not end the run under set -e).
  last="$(grep -Eo '[+][0-9]+( ~[0-9]+)?( -[0-9]+)?: (All tests (passed|skipped)|Some tests failed)' "$log" | tail -1 || true)"
  skipped="$(sed -nE 's/.* ~([0-9]+).*/\1/p' <<<"$last")"
  if [[ -z "$skipped" ]]; then
    gh_summary='^(🎉 |::(error|warning)::|##\[(error|warning)\])?[0-9]+ tests? passed(, [0-9]+ failed)?(, [0-9]+ flaky)?(, [0-9]+ skipped)?\.$'
    skipped="$(tr -d '\r' <"$log" | grep -E "$gh_summary" | tail -1 | sed -nE 's/.*, ([0-9]+) skipped\.$/\1/p' || true)"
  fi
  [[ -n "$skipped" ]] && summary+=("$s: $skipped skipped")
  rm -f "$log"
done
if [[ ${#summary[@]} -gt 0 ]]; then
  echo "[e2e] skipped (not run here):"
  printf '[e2e]   %s\n' "${summary[@]}"
fi
exit $fail
