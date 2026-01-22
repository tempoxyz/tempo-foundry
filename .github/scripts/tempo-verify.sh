#!/bin/bash
set -euo pipefail

# Verification checks: forge script and forge create with --verify flag
# Requires VERIFIER_URL to be set

if [[ -z "${VERIFIER_URL:-}" ]]; then
  echo "VERIFIER_URL not set, skipping verification tests"
  exit 0
fi

# Fee token address, defaults to native fee token
FEE_TOKEN="${TEMPO_FEE_TOKEN:-0x20c0000000000000000000000000000000000000}"

# Build fee token args if not using native token (array for safe expansion)
FEE_TOKEN_ARG=()
if [[ "$FEE_TOKEN" != "0x20c0000000000000000000000000000000000000" ]]; then
  FEE_TOKEN_ARG=(--fee-token "$FEE_TOKEN")
fi

echo -e "\n=== USING FEE TOKEN: $FEE_TOKEN ==="
echo -e "\n=== USING VERIFIER: $VERIFIER_URL ==="

echo -e "\n=== INIT TEMPO PROJECT ==="
tmp_dir=$(mktemp -d)
cd "$tmp_dir"
forge init -n tempo tempo-verify
cd tempo-verify

echo -e "\n=== CREATE AND FUND ADDRESS ==="
wallet_json="$(cast wallet new --json)"
ADDR="$(jq -r '.[0].address' <<<"$wallet_json")"
PK="$(jq -r '.[0].private_key' <<<"$wallet_json")"

for i in {1..100}; do
  OUT=$(cast rpc tempo_fundAddress "$ADDR" --rpc-url "$ETH_RPC_URL" 2>&1 || true)

  if echo "$OUT" | jq -e 'arrays' >/dev/null 2>&1; then
    echo "$OUT" | jq
    break
  fi

  echo "[$i] $OUT"
  sleep 0.2
done

printf "\naddress: %s\nprivate_key: %s\n" "$ADDR" "$PK"

echo -e "\n=== WAIT FOR BLOCKS TO MINE ==="
sleep 5

VERIFY_ARG=(--verify --retries 10 --delay 10)

echo -e "\n=== FORGE SCRIPT DEPLOY WITH VERIFY ==="
forge script ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} script/Mail.s.sol --sig "run(string)" "$(date +%s%N)" --private-key "$PK" --rpc-url "$ETH_RPC_URL" --broadcast "${VERIFY_ARG[@]}"

echo -e "\n=== FORGE SCRIPT DEPLOY WITH FEE TOKEN AND VERIFY ==="
forge script ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} script/Mail.s.sol --sig "run(string)" "$(date +%s%N)" --private-key "$PK" --rpc-url "$ETH_RPC_URL" --broadcast "${VERIFY_ARG[@]}"

echo -e "\n=== FORGE CREATE DEPLOY WITH VERIFY ==="
forge create ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} src/Mail.sol:Mail --private-key "$PK" --rpc-url "$ETH_RPC_URL" --broadcast "${VERIFY_ARG[@]}" --constructor-args "$FEE_TOKEN"

echo -e "\n=== FORGE CREATE DEPLOY WITH FEE TOKEN AND VERIFY ==="
if [[ ${#FEE_TOKEN_ARG[@]} -eq 0 ]]; then
  forge create --fee-token 0x20C0000000000000000000000000000000000002 src/Mail.sol:Mail --private-key "$PK" --rpc-url "$ETH_RPC_URL" --broadcast "${VERIFY_ARG[@]}" --constructor-args "$FEE_TOKEN"
  forge create --fee-token 0x20C0000000000000000000000000000000000003 src/Mail.sol:Mail --private-key "$PK" --rpc-url "$ETH_RPC_URL" --broadcast "${VERIFY_ARG[@]}" --constructor-args "$FEE_TOKEN"
else
  forge create ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} src/Mail.sol:Mail --private-key "$PK" --rpc-url "$ETH_RPC_URL" --broadcast "${VERIFY_ARG[@]}" --constructor-args "$FEE_TOKEN"
fi

echo -e "\n=== VERIFICATION TESTS COMPLETE ==="
