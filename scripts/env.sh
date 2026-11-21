# shellcheck shell=bash
# Source this before building or testing the desktop shell on a machine where the GTK 3
# development packages and Xvfb cannot be installed system-wide (no sudo): scripts/sysroot.sh
# extracts them into .toolchains/gtk-sysroot (an older checkout's .toolchains/tauri-sysroot is
# used when present). With the packages installed normally this file only adds ~/.cargo/bin.
_axiom_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
_sysroot="$_axiom_root/.toolchains/gtk-sysroot"
[[ -d "$_sysroot" ]] || _sysroot="$_axiom_root/.toolchains/tauri-sysroot"
export PATH="$HOME/.cargo/bin:$PATH"
if [[ -d "$_sysroot/usr/lib64/pkgconfig" ]]; then
  export PKG_CONFIG_PATH="$_sysroot/usr/lib64/pkgconfig:$_sysroot/usr/share/pkgconfig:/usr/lib64/pkgconfig:/usr/share/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
  export LIBRARY_PATH="$_sysroot/usr/lib64${LIBRARY_PATH:+:$LIBRARY_PATH}"
  [[ -x "$_sysroot/usr/bin/Xvfb" ]] && export AXIOM_XVFB="$_sysroot/usr/bin/Xvfb"
fi
unset _axiom_root _sysroot
