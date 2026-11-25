#!/usr/bin/env bash
# Host and headless client benchmarks. Opens no window. Figures: docs/engineering/performance.md.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
(cd engine && cargo build -q --release -p axiom-host && cargo run -q --release -p axiom-host --example bench)
(cd app && dart compile exe benchmark/markdown.dart -o build/markdown_bench >/dev/null && ./build/markdown_bench)
