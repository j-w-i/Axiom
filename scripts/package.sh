#!/bin/sh
# Builds the application with everything it needs beside it: the host it starts, and the CLI.
# Opens no window. The macOS build is signed for this computer only (K38).
set -eu
cd "$(dirname "$0")/.."
(cd engine && cargo build --release --locked -p axiom-host -p axiom-cli --bin axiom-host --bin axiom)
bin=engine/target/release
case "$(uname -s)" in
Darwin)
    (cd app && flutter build macos --release)
    out=app/build/macos/Build/Products/Release/axiom.app
    cp "$bin/axiom-host" "$out/Contents/MacOS/axiom-host"
    # Not beside the program: its name differs from that only in case.
    mkdir -p "$out/Contents/Resources/bin"
    cp "$bin/axiom" "$out/Contents/Resources/bin/axiom"
    for f in Contents/MacOS/axiom-host Contents/Resources/bin/axiom; do
        codesign --force --sign - --options runtime "$out/$f"
    done
    codesign --force --sign - --preserve-metadata=entitlements,flags "$out"
    codesign --verify --strict "$out"
    ;;
Linux)
    (cd app && flutter build linux --release)
    out=$(echo app/build/linux/*/release/bundle)
    cp "$bin/axiom-host" "$out/axiom-host"
    mkdir -p "$out/bin" && cp "$bin/axiom" "$out/bin/axiom"
    ;;
*)
    (cd app && flutter build windows --release)
    out=$(echo app/build/windows/*/runner/Release)
    cp "$bin/axiom-host.exe" "$out/axiom-host.exe"
    mkdir -p "$out/bin" && cp "$bin/axiom.exe" "$out/bin/axiom.exe"
    ;;
esac
echo "ok: $out"
