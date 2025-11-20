#!/bin/bash
export TEMPO_RPC_URL=https://eng:zealous-mayer@rpc.testnet.tempo.xyz
export VERIFIER_URL=https://tempo:reverent-einstein-thirsty-edison@scout.tempo.xyz/api/
forge init -n tempo tempo-check
cd tempo-check
forge test
forge script script/Mail.s.sol
read ADDR PK < <(cast wallet new --json | jq -r '.[0] | "\(.address) \(.private_key)"'); cast rpc tempo_fundAddress "$ADDR" --rpc-url "$TEMPO_RPC_URL"; printf "\naddress: %s\nprivate_key: %s\n" "$ADDR" "$PK"
forge script script/Mail.s.sol --private-key $PK --broadcast --verify
forge create src/Mail.sol:Mail --rpc-url $TEMPO_RPC_URL --private-key $PK --broadcast --verify --constructor-args 0x20c0000000000000000000000000000000000000