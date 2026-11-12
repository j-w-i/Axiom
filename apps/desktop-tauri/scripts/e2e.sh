#!/usr/bin/env bash
# Builds (unless --no-build) and runs the end-to-end suites of the real app.
#
#   apps/desktop-tauri/scripts/e2e.sh [--no-build] [--driver external|embedded|all] [suite.mjs ...]
#
# The app is built with the `e2e` feature, which embeds a WebDriver server
# (127.0.0.1 only). That build is for testing: scripts/bundle.sh never enables
# it, it refuses to package a binary that contains it, and
# src-tauri/tests/release_guard.rs fails if a default build links it.
#
# --driver picks the WebDriver server (default: the platform's, see
# e2e/webdriver.mjs): `external` = tauri-driver + WebKitWebDriver, Linux only,
# trusted key events; `embedded` = the in-app server, every OS; `all` runs the
# suites through each server available here in turn.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
# shellcheck disable=SC1091
source "$here/scripts/env.sh"

build=1
driver=""
suites=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-build) build=0 ;;
    --driver) driver="${2:?--driver needs a value}"; shift ;;
    --driver=*) driver="${1#--driver=}" ;;
    *) suites+=("$1") ;;
  esac
  shift
done

if [[ $build == 1 ]]; then
  (cd "$root" && cargo build -q -p axiomd)
  (cd "$here" && pnpm -s run build)
  (cd "$root" && cargo build -q --release -p axiom-desktop-tauri --features e2e)
fi

run() {
  # Before the suites, the script-channel contract (e2e/eval-contract.mjs): if
  # the channel this platform uses misbehaves, that is reported as itself and
  # not as 30 unrelated-looking suite failures.
  if [[ ${#suites[@]} -eq 0 && "${AXIOM_WEBDRIVER:-}" != external ]] \
     && [[ "${AXIOM_WEBDRIVER:-}" == embedded || "$(uname -s)" != Linux ]]; then
    node "$here/e2e/runner.mjs" "$here/e2e/eval-contract.mjs" || return 1
  fi
  # `${suites[@]+...}`: an empty array is an "unbound variable" under `set -u`
  # in bash 3.2, which is what macOS ships.
  node "$here/e2e/runner.mjs" ${suites[@]+"${suites[@]}"}
}

case "$driver" in
  "") run ;;
  external|embedded) AXIOM_WEBDRIVER="$driver" run ;;
  all)
    status=0
    drivers=(embedded)
    [[ "$(uname -s)" == Linux ]] && drivers=(external embedded)
    for d in "${drivers[@]}"; do
      echo "[e2e] ===== WebDriver: $d"
      AXIOM_WEBDRIVER="$d" run || status=1
    done
    exit "$status"
    ;;
  *) echo "e2e.sh: --driver must be external, embedded or all" >&2; exit 2 ;;
esac
