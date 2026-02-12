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

# eth_estimateGas doesn't send a real transaction, so no funding needed.
ADDR="${TEST_ADDR:-$(cast wallet new --json | jq -r '.[0].address')}"
echo -e "\n--- Using address: $ADDR ---"

echo -e "\n--- Test 1: eth_estimateGas via raw JSON-RPC ---"
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
