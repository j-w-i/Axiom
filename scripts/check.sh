#!/usr/bin/env bash
# The gate: what CI runs. Opens no window and uses no network beyond fetching dependencies.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
part="${1:-all}"

rust() {
  (cd engine
   cargo fmt --all --check
   cargo clippy -q --workspace --all-targets --locked -- -D warnings
   cargo test -q --workspace --locked)
  fresh="$(cd engine && cargo run -q -p axiom-proto --example fixtures --locked)"
  if [ "$fresh" != "$(cat docs/protocol/fixtures.jsonl)" ]; then
    echo "docs/protocol/fixtures.jsonl is stale: cd engine && cargo run -p axiom-proto --example fixtures > ../docs/protocol/fixtures.jsonl" >&2
    exit 1
  fi
  if command -v cargo-deny >/dev/null; then (cd engine && cargo deny --all-features check); else echo "cargo-deny is not installed: licences and advisories not checked here (CI does)"; fi
}

app() {
  (cd engine && cargo build -q -p axiom-host --locked --bins --examples)
  cd app
  flutter pub get --enforce-lockfile >/dev/null
  dart format --output=none --set-exit-if-changed lib test tool benchmark
  flutter analyze --fatal-infos
  dart run tool/check_licences.dart
  flutter test
  cd ..
}

case "$part" in
  rust) rust ;;
  app) app ;;
  all) rust; app ;;
  *) echo "usage: scripts/check.sh [rust|app]" >&2; exit 2 ;;
esac
echo "ok: $part"
