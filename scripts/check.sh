#!/usr/bin/env bash
# The single quality gate. Every task must leave this passing.
# Usage: scripts/check.sh            (everything that exists in the repo)
#        scripts/check.sh rust       (only the Rust workspace + repository guards)
#        scripts/check.sh editor | tauri | desktop   (one part)
#        scripts/check.sh e2e        (Tauri end-to-end test, headless, ~2 min)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
what="${1:-all}"

step() { printf '\n==> %s\n' "$*"; }

if [[ "$what" == all || "$what" == rust ]]; then
  step "cargo fmt --check";            cargo fmt --all --check
  step "cargo clippy (deny warnings)"; cargo clippy --workspace --all-targets -- -D warnings
  # The bridge tests spawn the real daemon binary.
  step "cargo build -p axiomd (for the desktop-bridge tests)"
  cargo build -p axiomd
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

if [[ ( "$what" == all || "$what" == tauri ) && -f apps/desktop-tauri/package.json ]]; then
  step "tauri shell: typecheck, bundle, UI-state tests (real daemon), design-token guards"
  # Linux: WebKitGTK/GTK development headers. scripts/sysroot.sh provides them
  # under .toolchains/ when the -devel packages cannot be installed.
  # shellcheck disable=SC1091
  source apps/desktop-tauri/scripts/env.sh
  (cd apps/desktop-tauri && pnpm install --frozen-lockfile && pnpm run check && pnpm run build && pnpm run test)
fi

# End-to-end (real app, real webview, real daemon, real input). Needs a private
# X server + tauri-driver, so it is opt-in: `scripts/check.sh e2e`.
if [[ "$what" == e2e ]]; then
  step "tauri shell: end-to-end (headless)"
  cargo build -p axiomd
  apps/desktop-tauri/scripts/e2e.sh
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
