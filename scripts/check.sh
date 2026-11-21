#!/usr/bin/env bash
# The single quality gate. Every task must leave this passing.
# Usage: scripts/check.sh            (everything: the Rust workspace and the desktop shell)
#        scripts/check.sh rust       (only the Rust workspace + repository guards)
#        scripts/check.sh flutter    (only the desktop shell, apps/desktop-flutter)
#        scripts/check.sh inspectors (only the web inspector, inspectors/web)
#        scripts/check.sh e2e        (the shell's end-to-end suites, private compositor)
#        scripts/check.sh package    (build the packages and verify them as installed copies)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
what="${1:-all}"

step() { printf '\n==> %s\n' "$*"; }

if [[ "$what" == all || "$what" == rust ]]; then
  step "cargo fmt --check";            cargo fmt --all --check
  step "cargo clippy (deny warnings)"
  cargo clippy --workspace --all-targets -- -D warnings
  # The bridge tests spawn the real daemon binary.
  step "cargo build -p axiomd (for the desktop-bridge tests)"
  cargo build -p axiomd
  step "cargo test";                   cargo test --workspace
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

# The web inspector (M5, ADR-0019): the Vite plugin and the in-page adapter.
if [[ "$what" == all || "$what" == rust || "$what" == inspectors ]]; then
  if command -v pnpm >/dev/null 2>&1; then
    step "web inspector: plugin and adapter tests"
    (cd inspectors/web && pnpm install --frozen-lockfile >/dev/null && pnpm test)
  else
    echo "SKIPPED: the web inspector's tests (pnpm is not installed). CI always runs them."
  fi
fi

# The Flutter shell (ADR-0020). Flutter is looked for on PATH, then in ~/flutter (dev-setup.md).
if [[ ( "$what" == all || "$what" == flutter ) && -f apps/desktop-flutter/pubspec.yaml ]]; then
  export PATH="$PATH:$HOME/flutter/bin"
  if ! command -v flutter >/dev/null 2>&1; then
    if [[ "$what" == flutter ]]; then
      echo "flutter is not installed (docs/contributing/dev-setup.md §Flutter)" >&2; exit 1
    fi
    echo "SKIPPED: the Flutter shell (flutter is not installed; see dev-setup.md). CI always runs it."
  else
    step "flutter shell: dependencies (lockfile enforced), wire types, format, analyze, tests, licences"
    # The client tests spawn the real daemon.
    cargo build -q -p axiomd
    (cd apps/desktop-flutter && flutter pub get --enforce-lockfile >/dev/null \
      && dart run tool/gen_types.dart --check \
      && dart format --output=none --set-exit-if-changed lib test tool \
      && flutter analyze --fatal-infos \
      && flutter test)
    python3 scripts/check_dart_licences.py apps/desktop-flutter >/dev/null
  fi
fi

# End-to-end (the real app against the real daemon, real input) in a private compositor, never on
# the developer's desktop: opt-in, `scripts/check.sh e2e` (`flutter-e2e` is the same).
if [[ "$what" == e2e || "$what" == flutter-e2e ]]; then
  step "desktop shell: end-to-end (private compositor)"
  apps/desktop-flutter/scripts/e2e.sh
fi

# Packaging, as an installed copy behaves: build the packages, unpack each to a prefix, run it
# with no --daemon (the packaged sibling lookup must find axiomd) and complete the handshake.
# Opt-in: `scripts/check.sh package`.
if [[ "$what" == package ]]; then
  step "desktop shell: packages install, find their daemon and connect"
  scripts/bundle.sh target/bundle
  if [[ "$(uname -s)" == Darwin ]]; then
    scripts/verify-package.sh --app target/bundle/Axiom.app
  else
    for p in target/bundle/*.deb $(find target/bundle -name '*.rpm'); do
      scripts/verify-package.sh "$p" || exit 1
    done
  fi
fi

printf '\nAll checks passed (%s).\n' "$what"
