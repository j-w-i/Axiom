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
  # The desktop shell is excluded here: `tauri::generate_context!` embeds the
  # built frontend, so it cannot compile before `pnpm run build`. It is checked
  # in the `tauri` step below, where the bundle exists.
  step "cargo clippy (deny warnings)"
  cargo clippy --workspace --all-targets --exclude axiom-desktop-tauri -- -D warnings
  # The bridge tests spawn the real daemon binary.
  step "cargo build -p axiomd (for the desktop-bridge tests)"
  cargo build -p axiomd
  step "cargo test";                   cargo test --workspace --exclude axiom-desktop-tauri
  step "schema files are current";     cargo run -q -p axiom-protocol --example export_schema -- --check
  step "task board is consistent";     python3 scripts/tasks.py check
  step "dependency matrix";            python3 scripts/check_deps.py
  step "CI workflow syntax"
  if command -v actionlint >/dev/null 2>&1; then
    actionlint -ignore 'unknown label' .github/workflows/*.yml
  else
    echo "SKIPPED: actionlint is not installed (go install github.com/rhysd/actionlint/cmd/actionlint@latest). CI rejects a malformed workflow before any job starts; M1.5 lost a round to that."
  fi
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
  # The shell compiles editor/src directly (ADR-0012 §4): the editor package's
  # dependencies must be present for tsc to resolve monaco-editor's types.
  (cd editor && pnpm install --frozen-lockfile)
  (cd apps/desktop-tauri && pnpm install --frozen-lockfile && pnpm run check && pnpm run build && pnpm run test)
  step "tauri shell: Rust host (needs the bundle it embeds), with and without the e2e feature"
  cargo clippy -p axiom-desktop-tauri --all-targets -- -D warnings
  cargo clippy -p axiom-desktop-tauri --all-targets --features e2e -- -D warnings
  cargo test -p axiom-desktop-tauri   # includes the release guard
fi

# End-to-end (real app, real webview, real daemon, real input). Needs a private
# X server, so it is opt-in: `scripts/check.sh e2e`. On Linux it runs twice:
# through WebKitWebDriver (trusted OS-level key events) and through the
# embedded driver that Windows and macOS CI use, so both paths stay honest.
if [[ "$what" == e2e ]]; then
  step "tauri shell: end-to-end (headless)"
  cargo build -p axiomd
  apps/desktop-tauri/scripts/e2e.sh --driver all
fi

# Packaging, as an installed copy behaves: unpack the package, launch it with no
# --daemon (the packaged sibling lookup must find the sidecar), and watch the
# protocol handshake on the wire. Opt-in: `scripts/check.sh package`.
if [[ "$what" == package ]]; then
  step "tauri shell: package launches, finds its sidecar and connects"
  cargo build -q --release -p axiom-desktop-tauri -p axiomd
  apps/desktop-tauri/scripts/bundle.sh --bundles deb
  apps/desktop-tauri/scripts/verify-package.sh
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
