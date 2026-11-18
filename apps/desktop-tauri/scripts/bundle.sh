#!/usr/bin/env bash
# Builds installable packages for the host OS (Tauri bundler) with the daemon
# shipped as a sidecar.
#
#   apps/desktop-tauri/scripts/bundle.sh [--debug] [extra `tauri build` args]
#   e.g. scripts/bundle.sh --bundles deb,rpm
#
# Works from any working directory and on Linux, macOS and Windows (Git Bash).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
# shellcheck disable=SC1091
source "$here/scripts/env.sh"

profile=release
args=()
for a in "$@"; do
  case "$a" in
    --debug) profile=debug ;;
    *) args+=("$a") ;;
  esac
done

# Tauri requires sidecars named <name>-<target-triple> under src-tauri/binaries.
triple="$(rustc --print host-tuple 2>/dev/null || true)"
[[ -n "$triple" ]] || triple="$(rustc -vV | sed -n 's/^host: //p')"
[[ -n "$triple" ]] || { echo "bundle.sh: cannot determine the host target triple" >&2; exit 1; }

ext=""
case "$triple" in *windows*) ext=".exe" ;; esac

build_flag=()
[[ "$profile" == release ]] && build_flag=(--release)

# Cargo's real target directory (honours CARGO_TARGET_DIR and .cargo/config).
# Parsed as JSON (not with sed): on Windows the path's backslashes are escaped.
target="$(cd "$root" && cargo metadata --no-deps --format-version 1 \
  | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>process.stdout.write(JSON.parse(s).target_directory))')"
[[ -n "$target" ]] || { echo "bundle.sh: cannot determine cargo's target directory" >&2; exit 1; }

(cd "$root" && cargo build "${build_flag[@]}" -p axiomd)
mkdir -p "$here/src-tauri/binaries"
cp "$target/$profile/axiomd$ext" "$here/src-tauri/binaries/axiomd-$triple$ext"
echo "sidecar: src-tauri/binaries/axiomd-$triple$ext"

# The daemon is a sidecar only in the *package*. Keeping `externalBin` in
# tauri.conf.json would make every `cargo build` (and so clippy and the tests)
# depend on a prebuilt daemon. It is merged in here from a file rather than an
# inline JSON argument: on Windows `pnpm` is a .cmd shim and cmd.exe re-parses
# quotes, which mangles inline JSON.
overlay="$here/src-tauri/bundle.sidecar.json"

[[ "$profile" == debug ]] && args+=(--debug)
cd "$here"
pnpm -s tauri build --config "$overlay" "${args[@]}"

# The packaged app must not contain the end-to-end WebDriver server (feature
# `e2e`). `tauri build` produces target/<profile>/axiom-tauri and packages that
# file, so it is the binary every package ships.
app="$target/$profile/axiom-tauri$ext"
if grep -q "tauri-plugin-webdriver is ready" "$app"; then
  echo "bundle.sh: $app contains the end-to-end WebDriver server - refusing to ship it" >&2
  exit 1
fi
echo "release guard: $app does not contain the end-to-end WebDriver server"
