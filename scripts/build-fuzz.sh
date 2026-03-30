#!/usr/bin/env bash
# Build forge with the `fuzz` profile and SanitizerCoverage instrumentation.
#
# Only the `tempo-precompiles` crate is compiled with sancov flags, via a
# RUSTC_WRAPPER that inspects --crate-name. This avoids instrumenting every
# crate in the binary (noise filtering) and allows LTO for all other code.
#
# Usage:
#   ./scripts/build-fuzz.sh
#
# Then run tests with:
#   /path/to/tempo-foundry/target/<target-triple>/fuzz/forge test --mt invariant
#
# In your project's foundry.toml, enable:
#   [invariant]
#   tempo_precompile_edges = true
#   tempo_precompile_trace_cmp = true
#   corpus_dir = "corpus/invariant"

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TARGET=$(rustc -vV | awk '/^host:/ { print $2 }')

RUSTC_WRAPPER="${SCRIPT_DIR}/sancov-rustc-wrapper.sh" \
cargo build \
  --profile fuzz \
  --bin forge \
  --target "$TARGET" \
  "$@"

echo ""
echo "Built: target/${TARGET}/fuzz/forge"
