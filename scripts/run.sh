#!/usr/bin/env bash
# Launch the desktop app from a source checkout.
#
#   scripts/run.sh                 the desktop app (Tauri 2)
#   scripts/run.sh --flutter       the Flutter shell (ADR-0020; in progress, release build)
#   scripts/run.sh --help
#
# Extra arguments after `--` are passed to the app. This opens a real window:
# tests and benchmarks must use apps/desktop-tauri/scripts/headless.sh instead.
#
# On Linux it also keeps a desktop entry and the app's icons in ~/.local/share
# (dev.axiom.desktop-tauri-prototype.desktop, or dev.axiom.Axiom.desktop for the
# Flutter shell: named after the window's app id), so the window shows Axiom's icon
# and name; AXIOM_NO_DESKTOP_ENTRY=1 skips that.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

flutter=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --flutter) flutter=1; shift; continue ;;
    --) shift; break ;;
    *) echo "unknown argument: $1 (see --help; app arguments go after --)" >&2; exit 2 ;;
  esac
done

# The window's app id is the identifier (src-tauri/src/lib.rs sets it); the desktop looks up the
# entry of that name for the window's icon. Files are only rewritten when they change.
install_desktop_entry() {
  local id="${1:-dev.axiom.desktop-tauri-prototype}" exec="${2:-$PWD/target/debug/axiom-tauri}"
  local icons=apps/desktop-tauri/src-tauri/icons
  local share="${XDG_DATA_HOME:-$HOME/.local/share}"
  local changed=0 size src dest
  for size in 32 64 128 256 512; do
    case "$size" in
      256) src="$icons/128x128@2x.png" ;;
      512) src="$icons/icon.png" ;;
      *) src="$icons/${size}x${size}.png" ;;
    esac
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
source apps/desktop-tauri/scripts/env.sh
if [[ "$flutter" == 1 ]]; then
  export PATH="$PATH:$HOME/flutter/bin"
  command -v flutter >/dev/null || { echo "run.sh: flutter is not installed (docs/contributing/dev-setup.md §Flutter)" >&2; exit 1; }
  cargo build -p axiomd
  (cd apps/desktop-flutter && flutter build linux --release)
  bundle="$PWD/apps/desktop-flutter/build/linux/x64/release/bundle/axiom"
  if [[ "$(uname -s)" == Linux && "${AXIOM_NO_DESKTOP_ENTRY:-}" != 1 ]]; then
    install_desktop_entry dev.axiom.Axiom "$bundle --daemon $PWD/target/debug/axiomd" \
      || echo "run.sh: could not install the desktop entry (the icon may be generic)" >&2
  fi
  exec "$bundle" --daemon "$PWD/target/debug/axiomd" "$@"
fi
(cd editor && pnpm install --frozen-lockfile)
(cd apps/desktop-tauri && pnpm install --frozen-lockfile && pnpm run build)
cargo build -p axiomd
cargo build -p axiom-desktop-tauri

if [[ "$(uname -s)" == Linux && "${AXIOM_NO_DESKTOP_ENTRY:-}" != 1 ]]; then
  install_desktop_entry || echo "run.sh: could not install the desktop entry (the icon may be generic)" >&2
fi

exec target/debug/axiom-tauri "$@"
