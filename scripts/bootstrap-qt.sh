#!/usr/bin/env bash
# Installs the pinned Qt SDK into .toolchains/ without root, using aqtinstall.
# Usage: scripts/bootstrap-qt.sh [--gl-headers]
#   --gl-headers  (Fedora only) also extract OpenGL/EGL/xkbcommon headers from
#                 the distro's -devel RPMs into .toolchains/sysroot-dev, for
#                 machines where you cannot `sudo dnf install` them.
set -euo pipefail
cd "$(dirname "$0")/.."
QT_VERSION="${AXIOM_QT_VERSION:-6.12.0}"
case "$(uname -s)" in
  Linux)  host=linux;  arch=linux_gcc_64 ;;
  Darwin) host=mac;    arch=clang_64 ;;
  *) echo "On Windows use: aqt install-qt windows desktop $QT_VERSION win64_msvc2022_64 -m qtwebengine qtwebchannel qtwebsockets qtpositioning -O .toolchains/qt" >&2; exit 1 ;;
esac
mkdir -p .toolchains
[[ -x .toolchains/aqt-venv/bin/aqt ]] || { python3 -m venv .toolchains/aqt-venv; .toolchains/aqt-venv/bin/pip -q install aqtinstall; }
.toolchains/aqt-venv/bin/aqt install-qt "$host" desktop "$QT_VERSION" "$arch" \
  -m qtwebengine qtwebchannel qtwebsockets qtpositioning -O .toolchains/qt

if [[ "${1:-}" == "--gl-headers" ]]; then
  command -v dnf >/dev/null || { echo "--gl-headers supports Fedora (dnf) only" >&2; exit 1; }
  d=.toolchains/sysroot-dev; mkdir -p "$d"; cd "$d"
  dnf download -q --arch "$(uname -m)" --arch noarch libglvnd-devel mesa-libGL-devel mesa-libEGL-devel libxkbcommon-devel vulkan-headers
  for r in *.rpm; do rpm2cpio "$r" | cpio -idm --quiet; done
  for l in GL OpenGL EGL GLX GLESv2 xkbcommon; do
    t=$(ls /usr/lib64/lib$l.so.[0-9] 2>/dev/null | head -1 || true)
    [[ -n "$t" ]] && ln -sf "$t" "usr/lib64/lib$l.so"
  done
fi
echo "Qt $QT_VERSION installed. Now: source scripts/dev-env.sh"
