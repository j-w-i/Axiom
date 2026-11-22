#!/usr/bin/env bash
# Packages the desktop app (FL-QA-001): the Flutter shell with axiomd beside it, where the shell
# looks for its daemon (locateDaemon: the executable's directory).
#
#   scripts/bundle.sh [out-dir]          Linux: .deb and .rpm; macOS: Axiom.app (zipped)
#
# Linux layout (both packages):
#   /usr/lib/axiom/           the Flutter bundle (axiom, lib/, data/) and axiomd
#   /usr/bin/axiom            → ../lib/axiom/axiom
#   /usr/share/applications/dev.axiom.Axiom.desktop   (StartupWMClass = the app id)
#   /usr/share/icons/hicolor/<n>x<n>/apps/dev.axiom.Axiom.png
# Verify with scripts/verify-package.sh <package>.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck disable=SC1091
source "$root/scripts/env.sh"
export PATH="$PATH:$HOME/flutter/bin"
out="$(mkdir -p "${1:-$root/target/bundle}" && cd "${1:-$root/target/bundle}" && pwd)"
id=dev.axiom.Axiom
version="$(sed -n 's/^version: *\([0-9.]*\).*/\1/p' "$root/apps/desktop-flutter/pubspec.yaml")"
(cd "$root" && cargo build -q --release -p axiomd)
icons="$root/apps/desktop-flutter/packaging/icons"

if [[ "$(uname -s)" == Darwin ]]; then
  (cd "$root/apps/desktop-flutter" && flutter build macos --release)
  app="$root/apps/desktop-flutter/build/macos/Build/Products/Release/Axiom.app"
  exe="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")"
  cp "$root/target/release/axiomd" "$app/Contents/MacOS/axiomd"
  rm -rf "$out/Axiom.app"
  cp -R "$app" "$out/Axiom.app"
  (cd "$out" && ditto -c -k --keepParent Axiom.app "Axiom-$version-macos.zip")
  echo "bundle: $out/Axiom.app ($exe + axiomd), $out/Axiom-$version-macos.zip"
  exit 0
fi

(cd "$root/apps/desktop-flutter" && flutter build linux --release)
bundle="$root/apps/desktop-flutter/build/linux/x64/release/bundle"
stage="$(mktemp -d)"
trap 'rm -rf "${stage:?}"' EXIT
mkdir -p "$stage/usr/lib/axiom" "$stage/usr/bin" "$stage/usr/share/applications"
cp -a "$bundle/." "$stage/usr/lib/axiom/"
install -m 0755 "$root/target/release/axiomd" "$stage/usr/lib/axiom/axiomd"
ln -s ../lib/axiom/axiom "$stage/usr/bin/axiom"
for n in 32 64 128 256 512; do
  install -D -m 0644 "$icons/${n}x${n}.png" "$stage/usr/share/icons/hicolor/${n}x${n}/apps/$id.png"
done
cat > "$stage/usr/share/applications/$id.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Axiom
Comment=LLM-native development platform
Exec=/usr/bin/axiom %F
Icon=$id
StartupWMClass=$id
Categories=Development;IDE;
Terminal=false
DESKTOP

# .deb
deb="$out/axiom_${version}_amd64.deb"
mkdir -p "$stage/DEBIAN"
cat > "$stage/DEBIAN/control" <<CONTROL
Package: axiom
Version: $version
Architecture: amd64
Maintainer: Axiom contributors
Depends: libgtk-3-0
Section: devel
Priority: optional
Description: Axiom, an LLM-native development platform
 The desktop shell (Flutter) and its daemon, axiomd.
CONTROL
dpkg-deb --root-owner-group --build "$stage" "$deb" >/dev/null
rm -rf "$stage/DEBIAN"

# .rpm (the staged tree as the build root)
top="$(mktemp -d)"
mkdir -p "$top/SPECS"
cat > "$top/SPECS/axiom.spec" <<SPEC
Name: axiom
Version: $version
Release: 1
Summary: Axiom, an LLM-native development platform
License: MIT OR Apache-2.0
Requires: gtk3
AutoReqProv: no
%description
The desktop shell (Flutter) and its daemon, axiomd.
%install
cp -a "$stage/." %{buildroot}/
%files
/usr/lib/axiom
/usr/bin/axiom
/usr/share/applications/$id.desktop
/usr/share/icons/hicolor/*/apps/$id.png
SPEC
rpmbuild -bb --quiet --define "_topdir $top" --define "_rpmdir $out" --define '_build_id_links none' \
  --define '__strip /bin/true' "$top/SPECS/axiom.spec" >/dev/null
rm -rf "${top:?}"
echo "bundle: $deb"
find "$out" -name "axiom-$version-1*.rpm" -printf 'bundle: %p\n'
