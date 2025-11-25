#!/bin/bash
echo -e "\n=== TEMPO VERSION ==="
cast client --rpc-url $TEMPO_RPC_URL
tmp_dir=$(mktemp -d)
cd "$tmp_dir"
echo -e "\n=== INIT TEMPO PROJECT ==="
forge init -n tempo tempo-check
cd tempo-check
echo -e "\n=== FORGE TEST ==="
forge test
echo -e "\n=== FORGE SCRIPT ==="
forge script script/Mail.s.sol
echo -e "\n=== CREATE AND FUND ADDRESS ==="
read ADDR PK < <(cast wallet new --json | jq -r '.[0] | "\(.address) \(.private_key)"'); cast rpc tempo_fundAddress "$ADDR" --rpc-url "$TEMPO_RPC_URL"; printf "\naddress: %s\nprivate_key: %s\n" "$ADDR" "$PK"
echo -e "\n=== FORGE SCRIPT DEPLOY AND VERIFY ==="
forge script script/Mail.s.sol --private-key $PK --broadcast --verify
echo -e "\n=== FORGE CREATE DEPLOY AND VERIFY ==="
forge create src/Mail.sol:Mail --rpc-url $TEMPO_RPC_URL --private-key $PK --broadcast --verify --constructor-args 0x20c0000000000000000000000000000000000000