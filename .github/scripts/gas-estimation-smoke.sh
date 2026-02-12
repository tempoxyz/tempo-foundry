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
  echo "PASS: eth_estimateGas succeeded (gas: $GAS)"
else
  ERROR_MSG=$(echo "$RESULT" | jq -r '.error.message // "unknown error"')
  ERROR_DATA=$(echo "$RESULT" | jq -r '.error.data // ""')
  echo "FAIL: eth_estimateGas failed: $ERROR_MSG"

  # Check if this is the validator token mismatch bug:
  # validatorTokens[address(0)] returns a non-default token (e.g. DONOTUSE),
  # causing the fee AMM swap to fail during RPC simulation.
  VALIDATOR_TOKEN_LOWER=$(echo "$VALIDATOR_TOKEN_ZERO" | tr '[:upper:]' '[:lower:]')
  DEFAULT_TOKEN_LOWER=$(echo "$DEFAULT_TOKEN" | tr '[:upper:]' '[:lower:]')
  if [[ "$VALIDATOR_TOKEN_LOWER" != "$DEFAULT_TOKEN_LOWER" && "$VALIDATOR_TOKEN_LOWER" != "0x0000000000000000000000000000000000000000" ]]; then
    echo ""
    echo "ROOT CAUSE: validatorTokens[address(0)] = $VALIDATOR_TOKEN_ZERO (expected $DEFAULT_TOKEN or 0x0)"
    echo "The RPC simulation beneficiary resolves to a non-default fee token, causing AMM swap failures."
    echo "Fix: use TIP_FEE_MANAGER_ADDRESS as beneficiary instead of address(0) in BuildPendingEnv."
  fi

  TEST_FAILED=1
fi

echo -e "\n=== Summary ==="
if [[ $TEST_FAILED -eq 0 ]]; then
  echo "ALL TESTS PASSED"
  exit 0
else
  echo "TESTS FAILED"
  exit 1
fi
