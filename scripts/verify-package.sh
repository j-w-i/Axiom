#!/usr/bin/env bash
# Does the *packaged* app install as an installed copy, find its daemon beside it and connect?
# (FL-QA-001; the part that can be checked without root.)
#
#   scripts/verify-package.sh target/bundle/axiom_*.deb|axiom-*.rpm
#   scripts/verify-package.sh --app target/bundle/Axiom.app        (macOS bundle, structure)
#
# Checks, each printed with its evidence:
#   1. the package holds the shell and axiomd in one directory, and /usr/bin/axiom
#   2. the desktop entry is named after the app id, with StartupWMClass and Icon = the app id
#   3. the app id's icons are in hicolor
#   4. run with no --daemon from the unpacked prefix (on a private X display), the shell starts
#      the packaged axiomd and completes the handshake (`--self-test`: "connected …")
# Exit status 0 only if every check passed.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
id=dev.axiom.Axiom
fail=0
check() { if [[ "$2" == 0 ]]; then echo "PASS  $1 — $3"; else echo "FAIL  $1 — $3"; fail=1; fi; }

if [[ "${1:-}" == "--app" ]]; then
  # Absolute: the self-test prints the daemon's absolute path, compared with this below.
  app="$(cd "${2:?--app needs a .app path}" && pwd)"
  plist="$app/Contents/Info.plist"
  exe="$(plutil -extract CFBundleExecutable raw "$plist" 2>/dev/null || /usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")"
  bid="$(plutil -extract CFBundleIdentifier raw "$plist" 2>/dev/null || /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")"
  [[ -x "$app/Contents/MacOS/$exe" && -x "$app/Contents/MacOS/axiomd" ]]; check "the shell and axiomd side by side" $? "Contents/MacOS/{$exe,axiomd}"
  [[ "$bid" == "$id" ]]; check "the bundle identifier is the app id" $? "$bid"
  [[ -n "$(find "$app/Contents/Resources" -name 'AppIcon*' -o -name '*.icns' | head -1)" ]]; check "the icon is in the bundle" $? "$(find "$app/Contents/Resources" -name 'AppIcon*' -o -name '*.icns' | head -1)"
  out="$("$app/Contents/MacOS/$exe" --self-test --data-dir "$(mktemp -d)" 2>/dev/null | tail -1 || true)"
  [[ "$out" == "connected $app/Contents/MacOS/axiomd "* ]]; check "it connects to the bundled daemon" $? "$out"
  exit $fail
fi

pkg="$(cd "$(dirname "${1:?a .deb or .rpm}")" && pwd)/$(basename "$1")"
prefix="$(mktemp -d)"
trap 'rm -rf "${prefix:?}"' EXIT
case "$pkg" in
  *.deb) dpkg-deb -x "$pkg" "$prefix" ;;
  *.rpm) (cd "$prefix" && rpm2cpio "$pkg" | cpio -idm --quiet) ;;
  *) echo "verify-package: not a .deb or .rpm: $pkg" >&2; exit 2 ;;
esac
lib="$prefix/usr/lib/axiom"
[[ -x "$lib/axiom" && -x "$lib/axiomd" && -L "$prefix/usr/bin/axiom" ]]
check "1 the shell and axiomd in one directory, and /usr/bin/axiom" $? "$(ls "$lib" | tr '\n' ' ')→ $(readlink "$prefix/usr/bin/axiom")"
desktop="$prefix/usr/share/applications/$id.desktop"
grep -qx "StartupWMClass=$id" "$desktop" 2>/dev/null && grep -qx "Icon=$id" "$desktop"
check "2 the desktop entry is the app id's" $? "$(grep -E '^(StartupWMClass|Icon|Exec)=' "$desktop" 2>/dev/null | tr '\n' ' ')"
n=$(find "$prefix/usr/share/icons/hicolor" -name "$id.png" 2>/dev/null | wc -l)
[[ "$n" -ge 5 ]]; check "3 the icons are in hicolor" $? "$n sizes"

# shellcheck disable=SC1091
source "$root/scripts/env.sh"
# shellcheck disable=SC1091
source "$root/scripts/headless.sh" || { echo "verify-package: no private display" >&2; exit 1; }
[[ -n "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]] || { echo "verify-package: refusing to run without the private display" >&2; exit 1; }
data="$(mktemp -d)"
out="$(PATH=/usr/bin:/bin timeout 60 "$lib/axiom" --self-test --data-dir "$data" 2>/dev/null | tail -1 || true)"
headless_stop
rm -rf "${data:?}"
[[ "$out" == "connected $lib/axiomd "* ]]
check "4 run as an installed copy, it starts the packaged axiomd and connects" $? "$out"
exit $fail
