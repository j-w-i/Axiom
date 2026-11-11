#!/usr/bin/env bash
# Builds installable packages for the host OS (Tauri bundler) with the daemon
# as a sidecar. Usage: apps/desktop-tauri/scripts/bundle.sh [--debug] [extra tauri args]
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
# shellcheck disable=SC1091
source "$here/scripts/env.sh"
profile=release
args=()
for a in "$@"; do case "$a" in --debug) profile=debug ;; *) args+=("$a") ;; esac; done

# Tauri requires sidecars named <name>-<target-triple> under src-tauri/binaries.
triple="$(rustc -v print host-tuple 2>/dev/null || rustc -vV | sed -n 's/^host: //p')"
[[ -n "$triple" ]] || { echo "cannot determine the target triple" >&2; exit 1; }
mkdir -p "$here/src-tauri/binaries"
ext=""
[[ "$(uname -s)" == MINGW* || "$(uname -s)" == CYGWIN* || "$(uname -s)" == MSYS* ]] && ext=".exe"
(cd "$root" && cargo build --"$profile" -p axiomd)
cp "$root/target/$profile/axiomd$ext" "$here/src-tauri/binaries/axiomd-$triple$ext"
echo "sidecar: src-tauri/binaries/axiomd-$triple$ext"
cd "$here"
[[ $profile == debug ]] && args+=(--debug)
exec pnpm -s tauri build "${args[@]}"
