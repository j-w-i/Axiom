#!/usr/bin/env bash
# Fedora only, no root needed: downloads the -devel RPMs the Flutter shell needs to *build* on
# Linux (GTK 3 and its pkg-config closure) plus Xvfb for headless runs, and extracts them into
# .toolchains/gtk-sysroot. Runtime libraries come from the system.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
S="$root/.toolchains/gtk-sysroot"; mkdir -p "$S/rpms"; cd "$S"
export PKG_CONFIG_PATH="$S/usr/lib64/pkgconfig:$S/usr/share/pkgconfig:/usr/lib64/pkgconfig:/usr/share/pkgconfig"
dl() { (cd rpms && dnf download -q -y --arch x86_64 --arch noarch "$@" >/dev/null); for r in rpms/*.rpm; do rpm2cpio "$r" | cpio -idmu --quiet 2>/dev/null; done; }
dl gtk3-devel xorg-x11-server-Xvfb
for _ in $(seq 1 25); do
  missing=$(for p in gtk+-3.0 gdk-3.0; do
    pkg-config --cflags "$p" 2>&1 | sed -n "s/^Package \(.*\) was not found.*/\1/p"; done | sort -u)
  [[ -z "$missing" ]] && break
  # shellcheck disable=SC2046
  dl $(for m in $missing; do echo "pkgconfig($m)"; done)
done
for f in usr/lib64/pkgconfig/*.pc; do
  sed -i "s#^prefix=/usr\$#prefix=$S/usr#; s#^libdir=/usr/lib64#libdir=$S/usr/lib64#; s#^includedir=/usr/include#includedir=$S/usr/include#" "$f"
done
for l in usr/lib64/*.so; do
  if [[ -L "$l" && ! -e "$l" ]]; then t=$(readlink "$l"); [[ -e "/usr/lib64/$t" ]] && ln -sf "/usr/lib64/$t" "$l"; fi
done
echo "sysroot ready: $S"
