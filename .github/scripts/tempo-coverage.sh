#!/usr/bin/env bash
set -euo pipefail

# Tempo Precompiles Coverage Script
# Builds tempo-foundry with coverage instrumentation, runs tests, generates lcov/html reports

show_help() {
    cat << 'EOF'
Tempo Precompiles Coverage Script

Builds tempo-foundry with LLVM coverage instrumentation, runs Solidity tests
against tempo precompiles, and generates lcov/HTML coverage reports.

USAGE:
    tempo-coverage.sh <tempo-foundry-dir> [output-dir] [tempo-rev] [OPTIONS]

ARGUMENTS:
    <tempo-foundry-dir>    Path to local tempo-foundry repository (required)
    [output-dir]           Output directory for coverage reports (default: ./coverage-output)
    [tempo-rev]            Git revision of tempo to test against (optional)
                           If provided, updates Cargo.toml to use this revision

OPTIONS:
    --check                Use minimal invariant config (runs=1, depth=100) for CI
                           Without this flag, uses existing foundry.toml invariant settings
    -h, --help             Show this help message

EXAMPLES:
    # Run with existing config (full invariant runs)
    tempo-coverage.sh ~/work/tempo-foundry ~/work/coverage/report

    # Quick check with minimal invariant config
    tempo-coverage.sh ~/work/tempo-foundry ~/work/coverage/report --check

    # Test against specific tempo revision
    tempo-coverage.sh ~/work/tempo-foundry ~/work/coverage/report 670255b

    # Test specific revision with quick check
    tempo-coverage.sh ~/work/tempo-foundry ~/work/coverage/report 670255b --check

OUTPUT:
    The script generates the following files in the output directory:
    
    ├── unit.profdata        # Coverage data from unit tests
    ├── unit.lcov            # LCOV format coverage from unit tests
    ├── unit-html/           # HTML report for unit tests
    │   └── index.html
    ├── invariant.profdata   # Coverage data from invariant tests (if present)
    ├── invariant.lcov       # LCOV format coverage from invariant tests
    ├── invariant-html/      # HTML report for invariant tests
    │   └── index.html
    ├── combined.profdata    # Merged coverage data
    ├── combined.lcov        # Merged LCOV coverage
    └── combined-html/       # Combined HTML report
        └── index.html

REQUIREMENTS:
    - Rust toolchain with llvm-tools-preview: rustup component add llvm-tools-preview
    - genhtml (from lcov package): apt install lcov
    - tempo-foundry repository with tempo dependencies

EOF
}

# Check for help flag
for arg in "$@"; do
    if [[ "$arg" == "-h" || "$arg" == "--help" ]]; then
        show_help
        exit 0
    fi
done

# Parse --check flag
CHECK_MODE=false
for arg in "$@"; do
    if [[ "$arg" == "--check" ]]; then
        CHECK_MODE=true
    fi
done

# Remove --check from positional args
args=()
for arg in "$@"; do
    if [[ "$arg" != "--check" ]]; then
        args+=("$arg")
    fi
done
set -- "${args[@]}"

FOUNDRY_DIR="${1:?Usage: $0 <tempo-foundry-dir> [output-dir] [tempo-rev] [--check]}"
OUTPUT_DIR="${2:-$(pwd)/coverage-output}"
TEMPO_REV="${3:-}"

# Resolve to absolute path
FOUNDRY_DIR=$(cd "$FOUNDRY_DIR" && pwd)

echo "=== Tempo Precompiles Coverage ==="
echo "Foundry dir: $FOUNDRY_DIR"
echo "Output: $OUTPUT_DIR"
if [[ -n "$TEMPO_REV" ]]; then
    echo "Tempo rev: $TEMPO_REV"
fi
echo ""

# Find LLVM tools - try multiple locations
LLVM_BIN=""
for toolchain_dir in "$HOME/.rustup/toolchains"/stable-*; do
    candidate="$toolchain_dir/lib/rustlib/$(rustc -vV | grep host | cut -d' ' -f2)/bin"
    if [[ -x "$candidate/llvm-profdata" ]]; then
        LLVM_BIN="$candidate"
        break
    fi
done

# Fallback to explicit path
if [[ -z "$LLVM_BIN" ]]; then
    LLVM_BIN="$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin"
fi

# Check llvm tools exist
if [[ ! -x "$LLVM_BIN/llvm-profdata" ]]; then
    echo "Error: llvm-profdata not found at $LLVM_BIN"
    echo "Install with: rustup component add llvm-tools-preview"
    exit 1
fi

echo "Using LLVM tools from: $LLVM_BIN"

# Verify foundry dir
if [[ ! -f "$FOUNDRY_DIR/Cargo.toml" ]]; then
    echo "Error: $FOUNDRY_DIR/Cargo.toml not found"
    exit 1
fi

cd "$FOUNDRY_DIR"

# Update tempo revision if specified
if [[ -n "$TEMPO_REV" ]]; then
    echo "=== Updating tempo dependencies to rev $TEMPO_REV ==="
    
    # Update all tempo dependencies to use the specified revision
    sed -i "s|tempo-alloy = { git = \"https://github.com/tempoxyz/tempo\".*}|tempo-alloy = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-contracts = { git = \"https://github.com/tempoxyz/tempo\".*}|tempo-contracts = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-revm = { git = \"https://github.com/tempoxyz/tempo\".*}|tempo-revm = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-evm = { git = \"https://github.com/tempoxyz/tempo\".*}|tempo-evm = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-chainspec = { git = \"https://github.com/tempoxyz/tempo\".*}|tempo-chainspec = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-primitives = { git = \"https://github.com/tempoxyz/tempo\".*}|tempo-primitives = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-precompiles = { git = \"https://github.com/tempoxyz/tempo\".*}|tempo-precompiles = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    
    # Also handle path dependencies - convert them to git
    sed -i "s|tempo-alloy = { path = .*}|tempo-alloy = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-contracts = { path = .*}|tempo-contracts = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-revm = { path = .*}|tempo-revm = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-evm = { path = .*}|tempo-evm = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-chainspec = { path = .*}|tempo-chainspec = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-primitives = { path = .*}|tempo-primitives = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    sed -i "s|tempo-precompiles = { path = .*}|tempo-precompiles = { git = \"https://github.com/tempoxyz/tempo\", rev = \"$TEMPO_REV\" }|" Cargo.toml
    
    # Update cargo lock
    cargo update -p tempo-precompiles -p tempo-revm -p tempo-primitives -p tempo-alloy -p tempo-contracts -p tempo-evm -p tempo-chainspec 2>/dev/null || true
fi

# Build with coverage instrumentation
echo "=== Building forge with coverage instrumentation ==="
RUSTFLAGS="-C instrument-coverage" cargo build --release --bin forge

# Find tempo-precompiles source - check path dependency first, then git
TEMPO_CHECKOUT=""

# Check for path dependency in Cargo.toml
TEMPO_PATH=$(grep 'tempo-precompiles' Cargo.toml | grep 'path' | sed 's/.*path *= *"//' | sed 's/".*//' | head -1 || true)
if [[ -n "$TEMPO_PATH" && -d "$FOUNDRY_DIR/$TEMPO_PATH" ]]; then
    # Resolve relative path from foundry dir
    TEMPO_CHECKOUT=$(cd "$FOUNDRY_DIR/$TEMPO_PATH/../.." && pwd)
fi

# Fall back to git checkout
if [[ -z "$TEMPO_CHECKOUT" || ! -d "$TEMPO_CHECKOUT/crates/precompiles" ]]; then
    LOCK_REV=$(grep -A5 'name = "tempo-precompiles"' Cargo.lock | grep 'source.*tempo.*#' | sed 's/.*#//' | tr -d '"' || true)
    if [[ -n "$LOCK_REV" ]]; then
        TEMPO_CHECKOUT=$(find ~/.cargo/git/checkouts/tempo-* -maxdepth 1 -type d -name "${LOCK_REV:0:7}*" 2>/dev/null | head -1)
    fi
fi

if [[ -z "$TEMPO_CHECKOUT" || ! -d "$TEMPO_CHECKOUT/crates/precompiles" ]]; then
    echo "Error: Could not find tempo precompiles source"
    exit 1
fi

PRECOMPILES_SRC="$TEMPO_CHECKOUT/crates/precompiles"
echo "Found precompiles source: $PRECOMPILES_SRC"

# Prepare test directory
SPECS_DIR="$TEMPO_CHECKOUT/docs/specs"
if [[ ! -d "$SPECS_DIR" ]]; then
    echo "Error: Specs directory not found at $SPECS_DIR"
    exit 1
fi

# Configure invariant tests
cd "$SPECS_DIR"

if [[ "$CHECK_MODE" == "true" ]]; then
    echo "=== Configuring invariant tests (runs=1, depth=100) for --check mode ==="
    # Update invariant config in foundry.toml to use runs=1, depth=100
    if grep -q "^invariant" foundry.toml; then
        sed -i 's/^invariant = .*/invariant = { runs = 1, depth = 100, fail_on_revert = true, show_solidity = true }/' foundry.toml
    else
        # Add after [profile.default] section
        sed -i '/^\[profile.default\]/a invariant = { runs = 1, depth = 100, fail_on_revert = true, show_solidity = true }' foundry.toml
    fi
else
    echo "=== Using existing invariant config ==="
fi

if ! grep -q "fs_permissions" foundry.toml; then
    echo 'fs_permissions = [{ access = "read-write", path = "./"}]' >> foundry.toml
fi

echo "Invariant config:"
grep "^invariant" foundry.toml | head -1

# Run unit tests
echo "=== Running unit tests ==="
rm -f *.profraw
LLVM_PROFILE_FILE="$SPECS_DIR/unit-%p.profraw" "$FOUNDRY_DIR/target/release/forge" test --match-test "test_" --fuzz-runs 1 --show-progress || true

# Run invariant tests (if they exist)
echo "=== Running invariant tests ==="
if [[ -d "$SPECS_DIR/test/invariants" ]]; then
    LLVM_PROFILE_FILE="$SPECS_DIR/invariant-%p.profraw" "$FOUNDRY_DIR/target/release/forge" test --match-path "test/invariants/*" --show-progress || true
else
    echo "No invariant tests found, skipping"
fi

# Generate coverage reports
echo "=== Generating coverage reports ==="
mkdir -p "$OUTPUT_DIR"

# Merge unit test profraw
$LLVM_BIN/llvm-profdata merge -sparse "$SPECS_DIR"/unit-*.profraw -o "$OUTPUT_DIR/unit.profdata" 2>/dev/null || true

# Merge invariant test profraw
if ls "$SPECS_DIR"/invariant-*.profraw 1>/dev/null 2>&1; then
    $LLVM_BIN/llvm-profdata merge -sparse "$SPECS_DIR"/invariant-*.profraw -o "$OUTPUT_DIR/invariant.profdata" 2>/dev/null || true
fi

# Merge all profraw for combined report
$LLVM_BIN/llvm-profdata merge -sparse "$SPECS_DIR"/*.profraw -o "$OUTPUT_DIR/combined.profdata"

# Generate lcov files
for profile in unit invariant combined; do
    if [[ -f "$OUTPUT_DIR/$profile.profdata" ]]; then
        $LLVM_BIN/llvm-cov export \
            --format=lcov \
            --instr-profile="$OUTPUT_DIR/$profile.profdata" \
            --object="$FOUNDRY_DIR/target/release/forge" \
            --sources "$PRECOMPILES_SRC" \
            > "$OUTPUT_DIR/$profile.lcov" 2>/dev/null || true
    fi
done

# Generate HTML reports
for profile in unit invariant combined; do
    if [[ -f "$OUTPUT_DIR/$profile.lcov" ]]; then
        genhtml "$OUTPUT_DIR/$profile.lcov" --output-directory "$OUTPUT_DIR/$profile-html" 2>/dev/null || true
    fi
done

# Print summary
echo ""
echo "=== Coverage Summary ==="
for profile in unit invariant combined; do
    if [[ -f "$OUTPUT_DIR/$profile.profdata" ]]; then
        echo ""
        echo "--- $profile ---"
        $LLVM_BIN/llvm-cov report \
            --instr-profile="$OUTPUT_DIR/$profile.profdata" \
            --object="$FOUNDRY_DIR/target/release/forge" \
            --sources "$PRECOMPILES_SRC" 2>/dev/null | tail -3
    fi
done

echo ""
echo "=== Output Files ==="
echo "LCOV: $OUTPUT_DIR/*.lcov"
echo "HTML: $OUTPUT_DIR/*-html/index.html"
echo ""
echo "Open: xdg-open $OUTPUT_DIR/combined-html/index.html"
