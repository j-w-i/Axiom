#!/usr/bin/env bash
# Type-checks the Tauri host for the two platforms this machine is not.
#
#   apps/desktop-tauri/scripts/cross-check.sh
#
# What this proves, and what it does not:
#   ✅ every Rust crate in the graph, and our own `#[cfg(target_os = ...)]` code
#      (the macOS application menu), compiles for x86_64-apple-darwin and
#      x86_64-pc-windows-msvc;
#   ⛔ nothing is linked, run, packaged or tested on those targets — that needs
#      the Apple SDK / the MSVC linker and a real machine or CI (tasks T-09/T-10).
#
# Two prerequisites are not part of the toolchain:
#   * Windows builds generate a resource file with `llvm-rc`. Without it,
#     tauri-winres aborts. Install LLVM (or extract the Fedora `llvm` package,
#     as scripts/sysroot.sh does for WebKitGTK) and put `llvm-rc` on PATH.
#   * The macOS graph compiles one Objective-C file in a build script; a Linux
#     clang does that fine, so CC/CXX are pointed at it.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
# shellcheck disable=SC1091
source "$here/scripts/env.sh"

triples=("x86_64-apple-darwin" "x86_64-pc-windows-msvc")
missing=()
for t in "${triples[@]}"; do
  rustup target list --installed | grep -qx "$t" || missing+=("$t")
done
if [[ ${#missing[@]} -gt 0 ]]; then
  echo "adding rust std for: ${missing[*]}"
  rustup target add "${missing[@]}"
fi

status=0
cd "$root"

echo "==> macOS (x86_64-apple-darwin): type-check"
if CC_x86_64_apple_darwin="${CC_x86_64_apple_darwin:-clang}" \
   CXX_x86_64_apple_darwin="${CXX_x86_64_apple_darwin:-clang++}" \
   cargo check -q -p axiom-desktop-tauri --all-targets --target x86_64-apple-darwin; then
  echo "    ok"
else
  echo "    FAILED"
  status=1
fi

echo "==> Windows (x86_64-pc-windows-msvc): type-check"
if ! command -v llvm-rc >/dev/null 2>&1; then
  echo "    SKIPPED: llvm-rc not on PATH (install LLVM, or extract the Fedora 'llvm' RPM)"
  status=1
elif cargo check -q -p axiom-desktop-tauri --all-targets --target x86_64-pc-windows-msvc; then
  echo "    ok"
else
  echo "    FAILED"
  status=1
fi

exit $status
