# shellcheck shell=bash
# Source this before building the Tauri prototype on a machine where the
# WebKitGTK/GTK development packages cannot be installed system-wide (no
# sudo). scripts/sysroot.sh extracts them into .toolchains/tauri-sysroot.
# With the -devel packages installed normally this file is unnecessary.
_axiom_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
_sysroot="$_axiom_root/.toolchains/tauri-sysroot"
export PATH="$HOME/.cargo/bin:$PATH"
if [[ -d "$_sysroot/usr/lib64/pkgconfig" ]]; then
  export PKG_CONFIG_PATH="$_sysroot/usr/lib64/pkgconfig:$_sysroot/usr/share/pkgconfig:/usr/lib64/pkgconfig:/usr/share/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
  export LIBRARY_PATH="$_sysroot/usr/lib64${LIBRARY_PATH:+:$LIBRARY_PATH}"
  export AXIOM_XVFB="$_sysroot/usr/bin/Xvfb"
fi
unset _axiom_root _sysroot
