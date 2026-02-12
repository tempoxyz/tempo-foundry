#!/bin/bash
set -euo pipefail

# Gas estimation smoke test — validates eth_estimateGas works correctly
# regardless of what validatorTokens[address(0)] contains.
#
# Regression test for: https://github.com/tempoxyz/tempo/pull/2588
# Run against any Tempo RPC (devnet, testnet, mainnet):
#   ETH_RPC_URL=https://rpc.mainnet.tempo.xyz ./gas-estimation-smoke.sh

if [ -z "${ETH_RPC_URL:-}" ]; then
  echo "ERROR: ETH_RPC_URL is not set"
  exit 1
fi

FEE_MANAGER="0xfeec000000000000000000000000000000000000"
DEFAULT_TOKEN="0x20c0000000000000000000000000000000000000"
TEST_FAILED=0

echo "=== Gas Estimation Smoke Test ==="
echo "RPC: $ETH_RPC_URL"

echo -e "\n--- Check validatorTokens[address(0)] ---"
VALIDATOR_TOKEN_ZERO=$(cast call --rpc-url "$ETH_RPC_URL" \
  "$FEE_MANAGER" "validatorTokens(address)(address)" \
  0x0000000000000000000000000000000000000000)
echo "validatorTokens[address(0)] = $VALIDATOR_TOKEN_ZERO"

# Use provided address or generate and fund a fresh one
echo -e "\n--- Setup test wallet ---"
if [[ -n "${TEST_ADDR:-}" ]]; then
  ADDR="$TEST_ADDR"
  echo "Using provided address: $ADDR"
else
  wallet_json="$(cast wallet new --json)"
  ADDR="$(jq -r '.[0].address' <<<"$wallet_json")"
  echo "Generated address: $ADDR"

  if cast rpc tempo_fundAddress "$ADDR" --rpc-url "$ETH_RPC_URL" >/dev/null 2>&1; then
    echo "Funding via tempo_fundAddress..."
    for i in {1..30}; do
      BAL=$(cast call --rpc-url "$ETH_RPC_URL" "$DEFAULT_TOKEN" \
        'balanceOf(address)(uint256)' "$ADDR" 2>/dev/null || echo "0")
      if [[ "$BAL" != "0" && -n "$BAL" ]]; then
        echo "Funded with $BAL tokens"
        break
      fi
      if [[ $i -eq 30 ]]; then
        echo "ERROR: Funding timed out"
        exit 1
      fi
      sleep 1
    done
  else
    echo "ERROR: Cannot fund address. Set TEST_ADDR to a pre-funded address."
    exit 1
  fi
fi

echo -e "\n--- Test 1: eth_estimateGas for ERC20 transfer ---"
ESTIMATE=$(cast estimate --rpc-url "$ETH_RPC_URL" \
  --from "$ADDR" \
  "$DEFAULT_TOKEN" "transfer(address,uint256)" "$ADDR" 1 2>&1) || true

if [[ "$ESTIMATE" =~ ^[0-9]+$ && "$ESTIMATE" -gt 0 ]]; then
  echo "PASS: estimate succeeded (gas: $ESTIMATE)"
else
  echo "FAIL: estimate failed: $ESTIMATE"
  TEST_FAILED=1
fi

echo -e "\n--- Test 2: eth_estimateGas via raw JSON-RPC ---"
RESULT=$(curl -s -X POST -H "Content-Type: application/json" -d '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "eth_estimateGas",
  "params": [{
    "from": "'"$ADDR"'",
    "to": "'"$DEFAULT_TOKEN"'",
    "value": "0x0"
  }]
}' "$ETH_RPC_URL")

if echo "$RESULT" | jq -e '.result' >/dev/null 2>&1; then
  GAS=$(echo "$RESULT" | jq -r '.result')
  echo "PASS: raw JSON-RPC estimate succeeded (gas: $GAS)"
else
  ERROR_MSG=$(echo "$RESULT" | jq -r '.error.message // "unknown error"')
  echo "FAIL: raw JSON-RPC estimate failed: $ERROR_MSG"
  TEST_FAILED=1
fi

echo -e "\n=== Summary ==="
if [[ $TEST_FAILED -eq 0 ]]; then
  echo "ALL TESTS PASSED"
  exit 0
else
  echo "TESTS FAILED: eth_estimateGas is broken (likely validator token mismatch)"
  exit 1
fi
