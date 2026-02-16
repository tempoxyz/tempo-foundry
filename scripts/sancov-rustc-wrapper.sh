#!/usr/bin/env bash
# RUSTC_WRAPPER that injects SanitizerCoverage flags only for tempo-precompiles.
#
# Usage: RUSTC_WRAPPER=./scripts/sancov-rustc-wrapper.sh cargo build ...
#
# Cargo invokes this as: wrapper <rustc> <args...>
# We inspect --crate-name to decide whether to add sancov flags.

RUSTC="$1"
shift

CRATE_NAME=""
PREV=""
for arg in "$@"; do
    if [ "$PREV" = "--crate-name" ]; then
        CRATE_NAME="$arg"
        break
    fi
    PREV="$arg"
done

SANCOV_CRATES="tempo_precompiles"

if [ -n "$CRATE_NAME" ] && echo "$SANCOV_CRATES" | grep -qw "$CRATE_NAME"; then
    EXTRA_FLAGS=(
        -Cpasses=sancov-module
        -Cllvm-args=-sanitizer-coverage-level=3
        -Cllvm-args=-sanitizer-coverage-trace-pc-guard
        -Cllvm-args=-sanitizer-coverage-trace-compares
    )
    exec "$RUSTC" "$@" "${EXTRA_FLAGS[@]}"
else
    exec "$RUSTC" "$@"
fi
