#!/usr/bin/env bash
# The single quality gate. Every task must leave this passing.
# Usage: scripts/check.sh            (everything that exists in the repo)
#        scripts/check.sh rust       (only the Rust workspace + repository guards)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
what="${1:-all}"

step() { printf '\n==> %s\n' "$*"; }

if [[ "$what" == all || "$what" == rust ]]; then
  step "cargo fmt --check";            cargo fmt --all --check
  step "cargo clippy (deny warnings)"; cargo clippy --workspace --all-targets -- -D warnings
  step "cargo test";                   cargo test --workspace
  step "schema files are current";     cargo run -q -p axiom-protocol --example export_schema -- --check
  step "task board is consistent";     python3 scripts/tasks.py check
  step "dependency matrix";            python3 scripts/check_deps.py
  step "cargo deny (licences, advisories, bans)"
  if cargo deny --version >/dev/null 2>&1; then
    cargo deny check
  else
    echo "SKIPPED: cargo-deny is not installed (cargo install cargo-deny --locked). CI always runs it."
  fi
fi

if [[ ( "$what" == all || "$what" == editor ) && -f editor/package.json ]]; then
  step "editor: install, typecheck, build, test"
  (cd editor && pnpm install --frozen-lockfile && pnpm run check && pnpm run build && pnpm run test)
fi

if [[ ( "$what" == all || "$what" == desktop ) && -f apps/desktop/CMakeLists.txt ]]; then
  step "desktop: QML literal guard";  python3 scripts/check_qml.py
  step "desktop: build axiomd and the emit test helper (debug) for the tests"
  cargo build -p axiomd -p emit
  step "desktop: configure, build, test"
  # shellcheck disable=SC1091
  source scripts/dev-env.sh
  axiom_cmake -S apps/desktop -B build/desktop
  cmake --build build/desktop
  QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}" ctest --test-dir build/desktop --output-on-failure
fi

printf '\nAll checks passed (%s).\n' "$what"
