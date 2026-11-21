#!/usr/bin/env bash
# Launch the desktop app (the Flutter shell, ADR-0020) from a source checkout: a release build of
# the shell against a debug axiomd.
#
#   scripts/run.sh
#   scripts/run.sh --help
#
# Extra arguments after `--` are passed to the app. This opens a real window: tests and
# benchmarks must use scripts/headless.sh or apps/desktop-flutter/scripts/e2e.sh instead.
# (The Tauri shell is at the tag tauri-shell-final.)
#
# On Linux and macOS it builds the platform's bundle. On Linux it also keeps a desktop entry and
# the app's icons in ~/.local/share (dev.axiom.Axiom.desktop, named after the window's app id),
# so the window shows Axiom's icon and name; AXIOM_NO_DESKTOP_ENTRY=1 skips that.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH:$HOME/flutter/bin"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --flutter) shift; continue ;; # the default since FL-QA-001; kept for old habits
    --) shift; break ;;
    *) echo "unknown argument: $1 (see --help; app arguments go after --)" >&2; exit 2 ;;
  esac
done

# The window's app id (linux/CMakeLists.txt); the desktop looks up the entry of that name for the
# window's icon. Files are only rewritten when they change.
install_desktop_entry() {
  local id=dev.axiom.Axiom exec="$1"
  local icons=apps/desktop-flutter/packaging/icons
  local share="${XDG_DATA_HOME:-$HOME/.local/share}"
  local changed=0 size src dest
  for size in 32 64 128 256 512; do
    src="$icons/${size}x${size}.png"
    dest="$share/icons/hicolor/${size}x${size}/apps/$id.png"
    if ! cmp -s "$src" "$dest"; then
      mkdir -p "$(dirname "$dest")" && cp "$src" "$dest" && changed=1
    fi
  done
  local entry="$share/applications/$id.desktop"
  local text
  text="[Desktop Entry]
Type=Application
Name=Axiom
Comment=Axiom desktop shell (development build)
Exec=$exec
Icon=$id
StartupWMClass=$id
Categories=Development;
Terminal=false"
  if [[ "$(cat "$entry" 2>/dev/null)" != "$text" ]]; then
    mkdir -p "$(dirname "$entry")" && printf '%s\n' "$text" > "$entry" && changed=1
  fi
  if [[ "$changed" == 1 ]]; then
    command -v update-desktop-database >/dev/null && update-desktop-database -q "$share/applications" || true
    command -v gtk-update-icon-cache >/dev/null && [[ -f "$share/icons/hicolor/index.theme" ]] \
      && gtk-update-icon-cache -q "$share/icons/hicolor" || true
    command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
  fi
}

# shellcheck disable=SC1091
source scripts/env.sh
command -v flutter >/dev/null || { echo "run.sh: flutter is not installed (docs/contributing/dev-setup.md §Flutter)" >&2; exit 1; }
cargo build -p axiomd
daemon="$PWD/target/debug/axiomd"
if [[ "$(uname -s)" == Darwin ]]; then
  (cd apps/desktop-flutter && flutter build macos --release)
  exec apps/desktop-flutter/build/macos/Build/Products/Release/Axiom.app/Contents/MacOS/Axiom \
    --daemon "$daemon" "$@"
fi
(cd apps/desktop-flutter && flutter build linux --release)
bundle="$PWD/apps/desktop-flutter/build/linux/x64/release/bundle/axiom"
if [[ "${AXIOM_NO_DESKTOP_ENTRY:-}" != 1 ]]; then
  install_desktop_entry "$bundle --daemon $daemon" \
    || echo "run.sh: could not install the desktop entry (the icon may be generic)" >&2
fi
exec "$bundle" --daemon "$daemon" "$@"
