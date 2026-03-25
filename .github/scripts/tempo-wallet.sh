#!/bin/bash
set -euo pipefail

# Tempo wallet keys.toml fallback tests
# Exercises --from / --sender resolving the signer from ~/.tempo/wallet/keys.toml
# without requiring --private-key or --tempo.access-key.
#
# Prerequisites:
#   - TEMPO_KEYS_TOML_B64 secret decoded into ~/.tempo/wallet/keys.toml
#   - The wallet address must be funded on the target network

# Fee token address, defaults to native fee token
FEE_TOKEN="${TEMPO_FEE_TOKEN:-0x20c0000000000000000000000000000000000000}"

FEE_TOKEN_ARG=()
if [[ "$FEE_TOKEN" != "0x20c0000000000000000000000000000000000000" ]]; then
  FEE_TOKEN_ARG=(--tempo.fee-token "$FEE_TOKEN")
fi

KEYS_FILE="${TEMPO_HOME:-$HOME/.tempo}/wallet/keys.toml"
if [[ ! -f "$KEYS_FILE" ]]; then
  echo "ERROR: keys.toml not found at $KEYS_FILE"
  exit 1
fi

WALLET_ADDR=$(grep -m1 'wallet_address' "$KEYS_FILE" | sed 's/.*= *"\(.*\)"/\1/')
if [[ -z "$WALLET_ADDR" ]]; then
  echo "ERROR: wallet_address not found in $KEYS_FILE"
  exit 1
fi

echo "=== Wallet: $WALLET_ADDR ==="
echo "=== RPC:    $ETH_RPC_URL ==="
echo "=== Fee:    $FEE_TOKEN ==="

# Fund the wallet address and wait for the fee token balance to be non-zero
echo -e "\n=== FUND WALLET ==="
for i in {1..100}; do
  OUT=$(cast rpc tempo_fundAddress "$WALLET_ADDR" --rpc-url "$ETH_RPC_URL" 2>&1 || true)
  if echo "$OUT" | jq -e 'arrays' >/dev/null 2>&1; then
    echo "$OUT" | jq
    break
  fi
  echo "[$i] $OUT"
  sleep 0.2
done
echo "Waiting for $WALLET_ADDR to be funded..."
for i in {1..30}; do
  BAL=$(cast call --rpc-url "$ETH_RPC_URL" "$FEE_TOKEN" 'balanceOf(address)(uint256)' "$WALLET_ADDR" 2>/dev/null || echo "0")
  if [[ "$BAL" != "0" && -n "$BAL" ]]; then
    echo "Funded with $BAL fee tokens"
    break
  fi
  if [[ $i -eq 30 ]]; then
    echo "ERROR: Funding timed out for $WALLET_ADDR"
    exit 1
  fi
  sleep 1
done

echo -e "\n=== CAST SEND WITH --from (keys.toml fallback) ==="
cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" \
  0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' \
  --from "$WALLET_ADDR"

echo -e "\n=== CAST ERC20 TRANSFER WITH --from (keys.toml fallback) ==="
cast erc20 transfer ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} \
  "$FEE_TOKEN" \
  0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 100 \
  --rpc-url "$ETH_RPC_URL" --from "$WALLET_ADDR"

echo -e "\n=== FORGE CREATE WITH --from (keys.toml fallback) ==="
tmp_dir=$(mktemp -d)
cd "$tmp_dir"
forge init -n tempo tempo-wallet-test --quiet
cd tempo-wallet-test

forge create ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} src/Counter.sol:Counter \
  --from "$WALLET_ADDR" --rpc-url "$ETH_RPC_URL" --broadcast

echo -e "\n=== FORGE SCRIPT WITH --sender (keys.toml fallback) ==="
forge script ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} script/Counter.s.sol \
  --sender "$WALLET_ADDR" --rpc-url "$ETH_RPC_URL" --broadcast

echo -e "\n=== TEMPO WALLET TESTS COMPLETE ==="
