#!/bin/bash
set -euo pipefail

# Get the directory where this script lives
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Non-verification tempo checks: local tests, fork tests, cast commands, DEX operations

# Hardfork version, defaults to T1 (latest features)
HARDFORK="${TEMPO_HARDFORK:-T1}"

# Fee token address, defaults to native fee token
FEE_TOKEN="${TEMPO_FEE_TOKEN:-0x20c0000000000000000000000000000000000000}"

# Build fee token args if not using native token (array for safe expansion)
FEE_TOKEN_ARG=()
if [[ "$FEE_TOKEN" != "0x20c0000000000000000000000000000000000000" ]]; then
  FEE_TOKEN_ARG=(--tempo.fee-token "$FEE_TOKEN")
fi

echo -e "\n=== USING HARDFORK: $HARDFORK ==="
echo -e "=== USING FEE TOKEN: $FEE_TOKEN ==="

echo -e "\n=== INIT TEMPO PROJECT ==="
tmp_dir=$(mktemp -d)
cd "$tmp_dir"
forge init -n tempo tempo-check
cd tempo-check

echo -e "\n=== FORGE TEST (LOCAL) ==="
TEMPO_FEE_TOKEN='' forge test

echo -e "\n=== FORGE SCRIPT (LOCAL) ==="
TEMPO_FEE_TOKEN='' forge script script/Mail.s.sol --sig "run(string)" "$(date +%s%N)"

echo -e "\n=== START TEMPO DEVNET TESTS ==="

# Export fee token for fork tests (templates use vm.envOr to read it)
export TEMPO_FEE_TOKEN="$FEE_TOKEN"

echo -e "\n=== TEMPO VERSION ==="
cast client --rpc-url "$ETH_RPC_URL"

echo -e "\n=== FORGE TEST (DEVNET) ==="
forge test --rpc-url "$ETH_RPC_URL"

echo -e "\n=== FORGE SCRIPT (DEVNET) ==="
forge script ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} script/Mail.s.sol --sig "run(string)" "$(date +%s%N)" --rpc-url "$ETH_RPC_URL"

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

echo -e "\n=== ADD AlphaUSD FEE TOKEN LIQUIDITY ==="
if [[ ${#FEE_TOKEN_ARG[@]} -eq 0 ]]; then
  cast send 0xfeec000000000000000000000000000000000000 'mint(address,address,uint256,address)' 0x20C0000000000000000000000000000000000001 0x20C0000000000000000000000000000000000000 1000000000 0x6c4143BEd3A13cf9E5E43d45C60aD816FC091d0c --private-key "$PK" --rpc-url "$ETH_RPC_URL"
else
  echo "skipped (custom fee token set)"
fi

echo -e "\n=== ADD BetaUSD FEE TOKEN LIQUIDITY ==="
if [[ ${#FEE_TOKEN_ARG[@]} -eq 0 ]]; then
  cast send 0xfeec000000000000000000000000000000000000 'mint(address,address,uint256,address)' 0x20C0000000000000000000000000000000000002 0x20C0000000000000000000000000000000000000 1000000000 0x6c4143BEd3A13cf9E5E43d45C60aD816FC091d0c --private-key "$PK" --rpc-url "$ETH_RPC_URL"
else
  echo "skipped (custom fee token set)"
fi

echo -e "\n=== ADD ThetaUSD FEE TOKEN LIQUIDITY ==="
if [[ ${#FEE_TOKEN_ARG[@]} -eq 0 ]]; then
  cast send 0xfeec000000000000000000000000000000000000 'mint(address,address,uint256,address)' 0x20C0000000000000000000000000000000000003 0x20C0000000000000000000000000000000000000 1000000000 0x6c4143BEd3A13cf9E5E43d45C60aD816FC091d0c --private-key "$PK" --rpc-url "$ETH_RPC_URL"
else
  echo "skipped (custom fee token set)"
fi

echo -e "\n=== CAST ERC20 TRANSFER WITH FEE TOKEN ==="
if [[ ${#FEE_TOKEN_ARG[@]} -eq 0 ]]; then
  cast erc20 transfer --tempo.fee-token 0x20C0000000000000000000000000000000000002 0x20c0000000000000000000000000000000000002 0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 123456 --rpc-url "$ETH_RPC_URL" --private-key "$PK"
  cast erc20 transfer --tempo.fee-token 0x20C0000000000000000000000000000000000003 0x20c0000000000000000000000000000000000002 0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 123456 --rpc-url "$ETH_RPC_URL" --private-key "$PK"
else
  cast erc20 transfer ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} "${FEE_TOKEN}" 0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 123456 --rpc-url "$ETH_RPC_URL" --private-key "$PK"
fi

echo -e "\n=== CAST ERC20 APPROVE WITH FEE TOKEN ==="
if [[ ${#FEE_TOKEN_ARG[@]} -eq 0 ]]; then
  cast erc20 approve --tempo.fee-token 0x20C0000000000000000000000000000000000002 0x20c0000000000000000000000000000000000002 0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 123456 --rpc-url "$ETH_RPC_URL" --private-key "$PK"
  cast erc20 approve --tempo.fee-token 0x20C0000000000000000000000000000000000003 0x20c0000000000000000000000000000000000002 0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 123456 --rpc-url "$ETH_RPC_URL" --private-key "$PK"
else
  echo "skipped (custom fee token set)"
fi

echo -e "\n=== CAST SEND WITH FEE TOKEN ==="
if [[ ${#FEE_TOKEN_ARG[@]} -eq 0 ]]; then
  cast send --tempo.fee-token 0x20C0000000000000000000000000000000000002 --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK"
  cast send --tempo.fee-token 0x20C0000000000000000000000000000000000003 --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK"
else
  cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK"
fi

echo -e "\n=== CAST MKTX WITH FEE TOKEN ==="
cast mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK"

# T1-only features: 2D nonces, expiring nonces, access keys
if [[ "$HARDFORK" == "T1" ]]; then
  echo -e "\n=== CAST MKTX WITH NONCE-KEY (2D Nonce) ==="
  # Each nonce-key has its own nonce sequence starting at 0
  cast mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --nonce 0 --tempo.nonce-key 1

  echo -e "\n=== CAST SEND WITH NONCE-KEY (2D Nonce) ==="
  # Use a different nonce-key (2) with nonce 0 since each key starts fresh
  cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --nonce 0 --tempo.nonce-key 2

  echo -e "\n=== CAST MKTX WITH EXPIRING NONCE (TIP-1009) ==="
  cast mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --tempo.expiring-nonce --tempo.valid-before "$(($(date +%s) + 25))"

  echo -e "\n=== CAST SEND WITH EXPIRING NONCE (TIP-1009) ==="
  cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --tempo.expiring-nonce --tempo.valid-before "$(($(date +%s) + 25))"

  echo -e "\n=== CAST MKTX WITH EXPIRING NONCE + VALID-AFTER ==="
  cast mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --tempo.expiring-nonce --tempo.valid-before "$(($(date +%s) + 25))" --tempo.valid-after "$(($(date +%s) + 5))"

  echo -e "\n=== CAST SEND WITH EXPIRING NONCE + VALID-AFTER ==="
  sleep 6  # Wait for valid_after to pass
  cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --tempo.expiring-nonce --tempo.valid-before "$(($(date +%s) + 25))" --tempo.valid-after "$(($(date +%s) - 1))"

  echo -e "\n=== SETUP ACCESS KEY ==="
  # Create an access key for testing
  access_wallet_json="$(cast wallet new --json)"
  ACCESS_KEY="$(jq -r '.[0].private_key' <<<"$access_wallet_json")"
  ACCESS_KEY_ADDR="$(jq -r '.[0].address' <<<"$access_wallet_json")"
  printf "Access key address: %s\n" "$ACCESS_KEY_ADDR"

  # Authorize the access key on-chain first (required for gas estimation)
  # Account Keychain precompile: 0xAAAAAAAA00000000000000000000000000000000
  # SignatureType: 0 = Secp256k1, Expiry: 1893456000 (year 2030), enforceLimits: false, limits: []
  cast send --rpc-url "$ETH_RPC_URL" 0xAAAAAAAA00000000000000000000000000000000 \
    'authorizeKey(address,uint8,uint64,bool,(address,uint256)[])' \
    "$ACCESS_KEY_ADDR" 0 1893456000 false "[]" \
    --private-key "$PK"

  # Fund the access key address (needed for gas)
  for i in {1..100}; do
    OUT=$(cast rpc tempo_fundAddress "$ACCESS_KEY_ADDR" --rpc-url "$ETH_RPC_URL" 2>&1 || true)
    if echo "$OUT" | jq -e 'arrays' >/dev/null 2>&1; then
      break
    fi
    sleep 0.2
  done
  sleep 3

  echo -e "\n=== CAST MKTX WITH ACCESS-KEY ==="
  # Use original address as root account (access key signs on behalf of root)
  cast mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --access-key "$ACCESS_KEY" --root-account "$ADDR"

  echo -e "\n=== CAST SEND WITH ACCESS-KEY ==="
  # Send transaction using the access key (Keychain signature wrapped in AA transaction)
  cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --access-key "$ACCESS_KEY" --root-account "$ADDR"
else
  echo -e "\n=== T1-ONLY FEATURES ==="
  echo "The following tests require T1 hardfork and are skipped on $HARDFORK:"
  echo "  - CAST MKTX WITH NONCE-KEY (2D Nonce)"
  echo "  - CAST SEND WITH NONCE-KEY (2D Nonce)"
  echo "  - CAST MKTX WITH EXPIRING NONCE (TIP-1009)"
  echo "  - CAST SEND WITH EXPIRING NONCE (TIP-1009)"
  echo "  - CAST MKTX WITH EXPIRING NONCE + VALID-AFTER"
  echo "  - CAST SEND WITH EXPIRING NONCE + VALID-AFTER"
  echo "  - SETUP ACCESS KEY"
  echo "  - CAST MKTX WITH ACCESS-KEY"
  echo "  - CAST SEND WITH ACCESS-KEY"
fi

echo -e "\n=== SETUP SPONSOR ==="
# Create a sponsor wallet for testing sponsored (gasless) transactions
sponsor_wallet_json="$(cast wallet new --json)"
SPONSOR_PK="$(jq -r '.[0].private_key' <<<"$sponsor_wallet_json")"
SPONSOR_ADDR="$(jq -r '.[0].address' <<<"$sponsor_wallet_json")"
printf "Sponsor address: %s\n" "$SPONSOR_ADDR"

# Fund the sponsor address (sponsor pays gas)
for i in {1..100}; do
  OUT=$(cast rpc tempo_fundAddress "$SPONSOR_ADDR" --rpc-url "$ETH_RPC_URL" 2>&1 || true)
  if echo "$OUT" | jq -e 'arrays' >/dev/null 2>&1; then
    break
  fi
  sleep 0.2
done
sleep 3

echo -e "\n=== CAST SEND WITH SPONSOR (--tempo.sponsor-signature) ==="
# Test sponsored transactions using pre-signed signature.
# Step 1: Get the fee_payer_signature_hash using --tempo.print-sponsor-hash
# Step 2: Sign it with the sponsor's private key
# Step 3: Send with --tempo.sponsor-signature

# Step 1: Get the hash that the sponsor needs to sign
FEE_PAYER_HASH=$(cast mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" \
  0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" \
  --tempo.print-sponsor-hash)
printf "Fee payer signature hash: %s\n" "$FEE_PAYER_HASH"

# Step 2: Sponsor signs the hash
SPONSOR_SIG=$(cast wallet sign --private-key "$SPONSOR_PK" "$FEE_PAYER_HASH" --no-hash)
printf "Sponsor signature: %s\n" "$SPONSOR_SIG"

# Step 3: Send the sponsored transaction with the signature
RECEIPT=$(cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" \
  0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" \
  --tempo.sponsor-signature "$SPONSOR_SIG" --json)

# Verify the fee_payer in the receipt matches the sponsor address
RECEIPT_FEE_PAYER=$(echo "$RECEIPT" | jq -r '.feePayer // .fee_payer // empty')
if [[ -z "$RECEIPT_FEE_PAYER" ]]; then
  echo "ERROR: feePayer not found in receipt"
  echo "Receipt: $RECEIPT"
  exit 1
fi

# Normalize addresses for comparison (lowercase)
RECEIPT_FEE_PAYER_LOWER=$(echo "$RECEIPT_FEE_PAYER" | tr '[:upper:]' '[:lower:]')
SPONSOR_ADDR_LOWER=$(echo "$SPONSOR_ADDR" | tr '[:upper:]' '[:lower:]')
if [[ "$RECEIPT_FEE_PAYER_LOWER" != "$SPONSOR_ADDR_LOWER" ]]; then
  echo "ERROR: Receipt feePayer ($RECEIPT_FEE_PAYER) does not match sponsor ($SPONSOR_ADDR)"
  exit 1
fi
echo "SUCCESS: Receipt feePayer ($RECEIPT_FEE_PAYER) matches sponsor address"

# Batch transaction tests (available on all hardforks)
echo -e "\n=== CAST BATCH-MKTX (NATIVE BATCHING) ==="
# Build a batch transaction with multiple calls as a single type 0x76 transaction
cast batch-mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" \
  --call "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D::increment()" \
  --call "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D::increment()" \
  --private-key "$PK"

echo -e "\n=== CAST BATCH-SEND (NATIVE BATCHING) ==="
# Send a batch transaction with multiple calls as a single type 0x76 transaction
cast batch-send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" \
  --call "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D::increment()" \
  --call "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D::increment()" \
  --private-key "$PK"

echo -e "\n=== CAST BATCH-SEND WITH VALUE SYNTAX (NATIVE BATCHING) ==="
# Test batch transaction with value syntax (currently using 0 value)
# TODO: Update to use non-zero value (e.g., 0.0001ether) once tempo#2294 is merged
# and the node supports per-call value transfers in batch transactions.
cast batch-send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" \
  --call "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D:0:increment()" \
  --call "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D::increment()" \
  --private-key "$PK"

echo -e "\n=== DEPLOY COUNTER WITH REQUIRE ==="
# Use CounterWithRequire.sol (has require(newNumber > 100)) for batch revert testing
cp "$SCRIPT_DIR/contracts/CounterWithRequire.sol" src/Counter.sol
forge build
REQUIRE_COUNTER_OUTPUT=$(forge create src/Counter.sol:Counter --rpc-url "$ETH_RPC_URL" --private-key "$PK" --broadcast 2>&1)
echo "Deploy output: $REQUIRE_COUNTER_OUTPUT"
# Extract address from human-readable output (avoids jq parse errors from stderr log pollution)
REQUIRE_COUNTER=$(echo "$REQUIRE_COUNTER_OUTPUT" | grep -oP 'Deployed to: \K0x[a-fA-F0-9]+')
if [[ "$REQUIRE_COUNTER" == "null" || -z "$REQUIRE_COUNTER" ]]; then
  echo "ERROR: Failed to deploy Counter with require"
  exit 1
fi
echo "Counter with require deployed at: $REQUIRE_COUNTER"

echo -e "\n=== CAST BATCH-SEND REVERT TEST ==="
# Test that batch reverts atomically when one call fails
# setNumber(1) fails because require(newNumber > 100)
if cast batch-send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" \
  --call "$REQUIRE_COUNTER::increment()" \
  --call "$REQUIRE_COUNTER::setNumber(uint256):1" \
  --private-key "$PK" 2>&1; then
  echo "ERROR: Batch should have reverted but succeeded"
  exit 1
fi
echo "OK: Batch correctly reverted (setNumber(1) failed require > 100)"

echo -e "\n=== CAST BATCH-SEND WITH ARGS AND ENCODED CALLDATA ==="
# Test batch with both function arguments and pre-encoded calldata
# First call: pre-encoded calldata for setNumber(200)
# Second call: function signature with args setNumber(101)
# Final number should be 101 (second call executes last)
ENCODED_CALLDATA=$(cast calldata "setNumber(uint256)" 200)
echo "Encoded calldata for setNumber(200): $ENCODED_CALLDATA"
cast batch-send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" \
  --call "$REQUIRE_COUNTER::$ENCODED_CALLDATA" \
  --call "$REQUIRE_COUNTER::setNumber(uint256):101" \
  --private-key "$PK"

NUMBER=$(cast call --rpc-url "$ETH_RPC_URL" "$REQUIRE_COUNTER" "number()(uint256)")
echo "Counter number after batch: $NUMBER (expected: 101)"

echo -e "\n=== FORGE SCRIPT --BATCH (NATIVE BATCHING) ==="
# Create a script that calls multiple contracts and batch them into a single tx
# Use template file and substitute REQUIRE_COUNTER address
sed "s/\${REQUIRE_COUNTER}/${REQUIRE_COUNTER}/" "$SCRIPT_DIR/contracts/BatchTest.s.sol.template" > script/BatchTest.s.sol

# Get number before batch
NUMBER_BEFORE=$(cast call --rpc-url "$ETH_RPC_URL" "$REQUIRE_COUNTER" "number()(uint256)")
echo "Counter number before forge script --batch: $NUMBER_BEFORE"

# Run forge script with --batch flag
forge script script/BatchTest.s.sol --broadcast --batch ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" --private-key "$PK"

# Verify all calls executed atomically
NUMBER_AFTER=$(cast call --rpc-url "$ETH_RPC_URL" "$REQUIRE_COUNTER" "number()(uint256)")
echo "Counter number after forge script --batch: $NUMBER_AFTER (expected: 503)"
if [[ "$NUMBER_AFTER" != "503" ]]; then
  echo "ERROR: Expected number to be 503 (500 + 3 increments), got $NUMBER_AFTER"
  exit 1
fi
echo "OK: forge script --batch executed all calls atomically"

echo -e "\n=== FORGE SCRIPT --BATCH WITH DEPLOY + CALLS ==="
# Test deploying a contract and calling it in the same batch transaction
# This tests the CREATE + CALL pattern (CREATE must be first)
cp "$SCRIPT_DIR/contracts/BatchCounter.sol" src/BatchCounter.sol
cp "$SCRIPT_DIR/contracts/DeployAndCall.s.sol" script/DeployAndCall.s.sol

forge build

# Build verification args if VERIFIER_URL is set (same pattern as tempo-deploy.sh)
VERIFY_ARG=()
if [[ -n "${VERIFIER_URL:-}" ]]; then
  VERIFY_ARG=(--verify --retries 10 --delay 10)
  echo "Will verify deployed contract via $VERIFIER_URL"
fi

# Run forge script with --batch flag - deploys and calls atomically
forge script script/DeployAndCall.s.sol --broadcast --batch ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} ${VERIFY_ARG[@]+"${VERIFY_ARG[@]}"} --rpc-url "$ETH_RPC_URL" --private-key "$PK"

echo "OK: forge script --batch with deploy + calls executed atomically"

echo -e "\n=== FORGE SCRIPT --BATCH REVERT TEST ==="
# Test that batch reverts atomically when one call in the script fails
# Use template file and substitute REQUIRE_COUNTER address
sed "s/\${REQUIRE_COUNTER}/${REQUIRE_COUNTER}/" "$SCRIPT_DIR/contracts/BatchRevertTest.s.sol.template" > script/BatchRevertTest.s.sol

NUMBER_BEFORE_REVERT=$(cast call --rpc-url "$ETH_RPC_URL" "$REQUIRE_COUNTER" "number()(uint256)")
echo "Counter number before batch revert test: $NUMBER_BEFORE_REVERT"

if forge script script/BatchRevertTest.s.sol --broadcast --batch ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" --private-key "$PK" 2>&1; then
  echo "ERROR: Batch script should have reverted but succeeded"
  exit 1
fi

# Verify number unchanged (atomic revert)
NUMBER_AFTER_REVERT=$(cast call --rpc-url "$ETH_RPC_URL" "$REQUIRE_COUNTER" "number()(uint256)")
echo "Counter number after batch revert: $NUMBER_AFTER_REVERT (expected: $NUMBER_BEFORE_REVERT - unchanged)"
if [[ "$NUMBER_AFTER_REVERT" != "$NUMBER_BEFORE_REVERT" ]]; then
  echo "ERROR: Expected number to remain $NUMBER_BEFORE_REVERT after atomic revert, got $NUMBER_AFTER_REVERT"
  exit 1
fi
echo "OK: forge script --batch correctly reverted atomically"

# Skip DEX/liquidity tests when using custom fee token (they assume multiple fee tokens)
if [[ ${#FEE_TOKEN_ARG[@]} -eq 0 ]]; then
  echo -e "\n=== CHANGE USER DEFAULT FEE TOKEN ==="
  cast send --rpc-url "$ETH_RPC_URL" 0xfeec000000000000000000000000000000000000 'setUserToken(address)' 0x20C0000000000000000000000000000000000002 --private-key "$PK"
  cast send --rpc-url "$ETH_RPC_URL" 0xfeec000000000000000000000000000000000000 'setUserToken(address)' 0x20C0000000000000000000000000000000000000 --private-key "$PK"

  echo -e "\n=== ADD LIQUIDITY: APPROVE DEX ==="
  cast erc20 approve 0x20c0000000000000000000000000000000000002 0xdec0000000000000000000000000000000000000 10000000000 --rpc-url "$ETH_RPC_URL" --private-key "$PK"
  cast erc20 approve 0x20c0000000000000000000000000000000000000 0xdec0000000000000000000000000000000000000 10000000000 --rpc-url "$ETH_RPC_URL" --private-key "$PK"

  echo -e "\n=== ADD LIQUIDITY: PLACE BID ==="
  cast send 0xdec0000000000000000000000000000000000000 "place(address,uint128,bool,int16)" 0x20c0000000000000000000000000000000000002 100000000 true 10 --private-key "$PK" -r "$ETH_RPC_URL"

  echo -e "\n=== ADD LIQUIDITY: PLACE ASK ==="
  cast send 0xdec0000000000000000000000000000000000000 "place(address,uint128,bool,int16)" 0x20c0000000000000000000000000000000000002 100000000 false 10 --private-key "$PK" -r "$ETH_RPC_URL"

  echo -e "\n=== ADD LIQUIDITY: PLACE FLIP ==="
  cast send 0xdec0000000000000000000000000000000000000 "placeFlip(address,uint128,bool,int16,int16)" 0x20c0000000000000000000000000000000000002 100000000 true -10 10 --private-key "$PK" -r "$ETH_RPC_URL"

  echo -e "\n=== ADD LIQUIDITY: SWAP EXACT AMOUNT IN ==="
  cast send 0xdec0000000000000000000000000000000000000 "swapExactAmountIn(address,address,uint128,uint128)" 0x20c0000000000000000000000000000000000000 0x20c0000000000000000000000000000000000002 100000000 9000000 --private-key "$PK" -r "$ETH_RPC_URL"

  echo -e "\n=== ADD LIQUIDITY: SWAP EXACT AMOUNT OUT ==="
  cast send 0xdec0000000000000000000000000000000000000 "swapExactAmountOut(address,address,uint128,uint128)" 0x20c0000000000000000000000000000000000002 0x20c0000000000000000000000000000000000000 9000000 100000000 --private-key "$PK" -r "$ETH_RPC_URL"
else
  echo -e "\n=== CHANGE USER DEFAULT FEE TOKEN ==="
  echo "skipped (custom fee token set)"

  echo -e "\n=== ADD LIQUIDITY: APPROVE DEX ==="
  echo "skipped (custom fee token set)"

  echo -e "\n=== ADD LIQUIDITY: PLACE BID ==="
  echo "skipped (custom fee token set)"

  echo -e "\n=== ADD LIQUIDITY: PLACE ASK ==="
  echo "skipped (custom fee token set)"

  echo -e "\n=== ADD LIQUIDITY: PLACE FLIP ==="
  echo "skipped (custom fee token set)"

  echo -e "\n=== ADD LIQUIDITY: SWAP EXACT AMOUNT IN ==="
  echo "skipped (custom fee token set)"

  echo -e "\n=== ADD LIQUIDITY: SWAP EXACT AMOUNT OUT ==="
  echo "skipped (custom fee token set)"
fi

echo -e "\n=== ANVIL FORK TESTS ==="
# Test anvil forking the Tempo network using the faucet-funded account
ANVIL_PORT=8546
echo "Starting anvil fork..."
# Pass hardfork to anvil (lowercase for CLI compatibility)
ANVIL_HARDFORK=$(echo "$HARDFORK" | tr '[:upper:]' '[:lower:]')
anvil --tempo --hardfork "$ANVIL_HARDFORK" --fork-url "$ETH_RPC_URL" --port $ANVIL_PORT --retries 10 --timeout 60000 &
ANVIL_PID=$!

# Ensure anvil is stopped on script exit
trap 'kill "$ANVIL_PID" 2>/dev/null || true' EXIT

# Wait for anvil to be ready (max 10 seconds)
for i in {1..10}; do
  if cast client --rpc-url "http://127.0.0.1:$ANVIL_PORT" 2>/dev/null; then
    echo "Anvil fork started successfully"
    break
  fi
  if [[ $i -eq 10 ]]; then
    echo "ERROR: Anvil fork failed to start"
    exit 1
  fi
  sleep 1
done

echo -e "\n=== ANVIL FORK: CHECK CLIENT VERSION ==="
cast client --rpc-url http://127.0.0.1:$ANVIL_PORT

echo -e "\n=== ANVIL FORK: CHECK CHAIN ID ==="
cast chain-id --rpc-url http://127.0.0.1:$ANVIL_PORT

echo -e "\n=== ANVIL FORK: CHECK BLOCK NUMBER ==="
cast block-number --rpc-url http://127.0.0.1:$ANVIL_PORT

echo -e "\n=== ANVIL FORK: FORGE TEST ==="
TEMPO_FEE_TOKEN="$FEE_TOKEN" forge test --rpc-url http://127.0.0.1:$ANVIL_PORT

echo -e "\n=== ANVIL FORK: FORGE SCRIPT ==="
TEMPO_FEE_TOKEN="$FEE_TOKEN" forge script ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} script/Mail.s.sol --sig "run(string)" "$(date +%s%N)" --rpc-url http://127.0.0.1:$ANVIL_PORT

echo -e "\n=== ANVIL FORK: CAST SEND ==="
# Use the faucet-funded account with explicit fee token (account state is forked from devnet)
cast send --tempo.fee-token "$FEE_TOKEN" --rpc-url http://127.0.0.1:$ANVIL_PORT 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK"

echo -e "\n=== ANVIL FORK: ERC20 TRANSFER ==="
cast erc20 transfer --tempo.fee-token "$FEE_TOKEN" 0x20c0000000000000000000000000000000000000 0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 123456 --rpc-url http://127.0.0.1:$ANVIL_PORT --private-key "$PK"

# T1-only features on anvil fork
if [[ "$HARDFORK" == "T1" ]]; then
  echo -e "\n=== ANVIL FORK: CAST SEND WITH NONCE-KEY (2D Nonce) ==="
  cast send --tempo.fee-token "$FEE_TOKEN" --rpc-url http://127.0.0.1:$ANVIL_PORT 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --nonce 0 --tempo.nonce-key 100

  echo -e "\n=== ANVIL FORK: CAST SEND WITH EXPIRING NONCE ==="
  cast send --tempo.fee-token "$FEE_TOKEN" --rpc-url http://127.0.0.1:$ANVIL_PORT 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --tempo.expiring-nonce --tempo.valid-before "$(($(date +%s) + 25))"
fi

echo -e "\n=== ANVIL FORK: BATCH SEND ==="
cast batch-send --tempo.fee-token "$FEE_TOKEN" --rpc-url http://127.0.0.1:$ANVIL_PORT \
  --call "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D::increment()" \
  --call "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D::increment()" \
  --private-key "$PK"

# Stop anvil
kill "$ANVIL_PID" 2>/dev/null || true
trap - EXIT

echo -e "\n=== ANVIL FORK TESTS COMPLETE ==="
