#!/usr/bin/env bash
set -euo pipefail

GIT_URL='ssh://git@github.com/tempoxyz/tempo'
BRANCH='main'

echo "=== Updating Git dependencies from: $GIT_URL ==="

sed -i -E \
  "s|(git = \"$GIT_URL\"),[[:space:]]*rev = \"[^\"]+\"|\1, branch = \"$BRANCH\"|g" \
  Cargo.toml

echo "Cargo.toml updated."

CRATES=$(grep -A2 "$GIT_URL" Cargo.toml | grep '=' | awk -F= '{print $1}' | tr -d ' ' || true)

if [[ -z "${CRATES}" ]]; then
    echo "No crates found pointing to $GIT_URL."
else
    echo "Found crates:"
    echo "$CRATES"
    echo "Updating Cargo.lock..."
    for crate in $CRATES; do
        cargo update -p "$crate"
    done
fi

echo "=== All updates applied. ==="
