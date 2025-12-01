#!/bin/bash

echo -e "\n=== INIT TEMPO PROJECT ==="
tmp_dir=$(mktemp -d)
cd "$tmp_dir"
forge init -n tempo tempo-check
cd tempo-check

echo -e "\n=== FORGE TEST (LOCAL) ==="
forge test

echo -e "\n=== FORGE SCRIPT (LOCAL) ==="
forge script script/Mail.s.sol

echo -e "\n=== START TEMPO FORK ==="
export TEMPO_RPC_URL="$_TEMPO_RPC_URL"

echo -e "\n=== TEMPO VERSION ==="
cast client --rpc-url $TEMPO_RPC_URL

echo -e "\n=== FORGE TEST (FORK) ==="
forge test

echo -e "\n=== FORGE SCRIPT (FORK) ==="
forge script script/Mail.s.sol

# echo -e "\n=== CREATE AND FUND ADDRESS ==="
read ADDR PK < <(cast wallet new --json | jq -r '.[0] | "\(.address) \(.private_key)"'); cast rpc tempo_fundAddress "$ADDR" --rpc-url "$TEMPO_RPC_URL"; printf "\naddress: %s\nprivate_key: %s\n" "$ADDR" "$PK"

# echo -e "\n=== WAIT FOR BLOCKS TO MINE ==="
sleep 5

# echo -e "\n=== FORGE SCRIPT DEPLOY ==="
forge script script/Mail.s.sol --private-key $PK --broadcast

# echo -e "\n=== FORGE CREATE DEPLOY ==="
forge create src/Mail.sol:Mail --rpc-url $TEMPO_RPC_URL --private-key $PK --broadcast --constructor-args 0x20c0000000000000000000000000000000000000