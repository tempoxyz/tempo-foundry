#!/bin/bash
set -euo pipefail

# Non-verification tempo checks: local tests, fork tests, cast commands, DEX operations

# Hardfork version, defaults to T1 (latest features)
HARDFORK="${TEMPO_HARDFORK:-T1}"

# Fee token address, defaults to native fee token
FEE_TOKEN="${TEMPO_FEE_TOKEN:-0x20c0000000000000000000000000000000000000}"

# Build fee token args if not using native token (array for safe expansion)
FEE_TOKEN_ARG=()
if [[ "$FEE_TOKEN" != "0x20c0000000000000000000000000000000000000" ]]; then
  FEE_TOKEN_ARG=(--fee-token "$FEE_TOKEN")
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

echo -e "\n=== START TEMPO FORK TESTS ==="

# Export fee token for fork tests (templates use vm.envOr to read it)
export TEMPO_FEE_TOKEN="$FEE_TOKEN"

echo -e "\n=== TEMPO VERSION ==="
cast client --rpc-url "$ETH_RPC_URL"

echo -e "\n=== FORGE TEST (FORK) ==="
forge test --rpc-url "$ETH_RPC_URL"

echo -e "\n=== FORGE SCRIPT (FORK) ==="
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
  cast erc20 transfer --fee-token 0x20C0000000000000000000000000000000000002 0x20c0000000000000000000000000000000000002 0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 123456 --rpc-url "$ETH_RPC_URL" --private-key "$PK"
  cast erc20 transfer --fee-token 0x20C0000000000000000000000000000000000003 0x20c0000000000000000000000000000000000002 0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 123456 --rpc-url "$ETH_RPC_URL" --private-key "$PK"
else
  cast erc20 transfer ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} "${FEE_TOKEN}" 0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 123456 --rpc-url "$ETH_RPC_URL" --private-key "$PK"
fi

echo -e "\n=== CAST ERC20 APPROVE WITH FEE TOKEN ==="
if [[ ${#FEE_TOKEN_ARG[@]} -eq 0 ]]; then
  cast erc20 approve --fee-token 0x20C0000000000000000000000000000000000002 0x20c0000000000000000000000000000000000002 0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 123456 --rpc-url "$ETH_RPC_URL" --private-key "$PK"
  cast erc20 approve --fee-token 0x20C0000000000000000000000000000000000003 0x20c0000000000000000000000000000000000002 0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F 123456 --rpc-url "$ETH_RPC_URL" --private-key "$PK"
else
  echo "skipped (custom fee token set)"
fi

echo -e "\n=== CAST SEND WITH FEE TOKEN ==="
if [[ ${#FEE_TOKEN_ARG[@]} -eq 0 ]]; then
  cast send --fee-token 0x20C0000000000000000000000000000000000002 --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK"
  cast send --fee-token 0x20C0000000000000000000000000000000000003 --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK"
else
  cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK"
fi

echo -e "\n=== CAST MKTX WITH FEE TOKEN ==="
cast mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK"

# T1-only features: 2D nonces, expiring nonces, access keys
if [[ "$HARDFORK" == "T1" ]]; then
  echo -e "\n=== CAST MKTX WITH NONCE-KEY (2D Nonce) ==="
  # Each nonce-key has its own nonce sequence starting at 0
  cast mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --nonce 0 --nonce-key 1

  echo -e "\n=== CAST SEND WITH NONCE-KEY (2D Nonce) ==="
  # Use a different nonce-key (2) with nonce 0 since each key starts fresh
  cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --nonce 0 --nonce-key 2

  echo -e "\n=== CAST MKTX WITH EXPIRING NONCE (TIP-1009) ==="
  # Use 25s expiry to stay safely within the 30s max (avoids timing issues with gas estimation)
  VALID_BEFORE=$(($(date +%s) + 25))
  cast mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --expiring-nonce --valid-before "$VALID_BEFORE"

  echo -e "\n=== CAST SEND WITH EXPIRING NONCE (TIP-1009) ==="
  VALID_BEFORE=$(($(date +%s) + 25))
  cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --expiring-nonce --valid-before "$VALID_BEFORE"

  echo -e "\n=== CAST MKTX WITH EXPIRING NONCE + VALID-AFTER ==="
  VALID_AFTER=$(($(date +%s) + 5))
  VALID_BEFORE=$(($(date +%s) + 25))
  cast mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --expiring-nonce --valid-before "$VALID_BEFORE" --valid-after "$VALID_AFTER"

  echo -e "\n=== CAST SEND WITH EXPIRING NONCE + VALID-AFTER ==="
  sleep 6  # Wait for valid_after to pass
  VALID_BEFORE=$(($(date +%s) + 25))
  cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --expiring-nonce --valid-before "$VALID_BEFORE" --valid-after "$(($(date +%s) - 1))"

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
# Modify existing Counter.sol to add require(newNumber > 100) for batch revert testing
cat > src/Counter.sol << 'EOF'
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract Counter {
    uint256 public number;

    function setNumber(uint256 newNumber) public {
        require(newNumber > 100, "bad number");
        number = newNumber;
    }

    function increment() public {
        number++;
    }
}
EOF
forge build
REQUIRE_COUNTER_OUTPUT=$(forge create src/Counter.sol:Counter --rpc-url "$ETH_RPC_URL" --private-key "$PK" --broadcast --json)
echo "Deploy output: $REQUIRE_COUNTER_OUTPUT"
REQUIRE_COUNTER=$(echo "$REQUIRE_COUNTER_OUTPUT" | jq -r '.deployedTo')
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
cat > script/BatchTest.s.sol << SOLEOF
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";

interface ICounter {
    function increment() external;
    function setNumber(uint256 newNumber) external;
    function number() external view returns (uint256);
}

contract BatchTestScript is Script {
    function run() public {
        address counter = ${REQUIRE_COUNTER};
        vm.startBroadcast();
        
        // Multiple calls that will be batched into a single transaction
        ICounter(counter).setNumber(500);
        ICounter(counter).increment();
        ICounter(counter).increment();
        ICounter(counter).increment();
        
        vm.stopBroadcast();
    }
}
SOLEOF

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
cat > src/BatchCounter.sol << 'SOLEOF'
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract BatchCounter {
    uint256 public number;

    constructor(uint256 initialNumber) {
        number = initialNumber;
    }

    function setNumber(uint256 newNumber) public {
        number = newNumber;
    }

    function increment() public {
        number++;
    }
}
SOLEOF

cat > script/DeployAndCall.s.sol << 'SOLEOF'
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import {BatchCounter} from "../src/BatchCounter.sol";

contract DeployAndCallScript is Script {
    function run() public {
        vm.startBroadcast();
        
        // Deploy contract (CREATE as first call)
        BatchCounter counter = new BatchCounter(100);
        
        // Call the newly deployed contract in the same batch
        counter.setNumber(200);
        counter.increment();
        counter.increment();
        
        // Final number should be 202 (200 + 2 increments)
        console.log("Deployed BatchCounter at:", address(counter));
        
        vm.stopBroadcast();
    }
}
SOLEOF

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
cat > script/BatchRevertTest.s.sol << SOLEOF
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";

interface ICounter {
    function increment() external;
    function setNumber(uint256 newNumber) external;
    function number() external view returns (uint256);
}

contract BatchRevertTestScript is Script {
    function run() public {
        address counter = ${REQUIRE_COUNTER};
        vm.startBroadcast();
        
        // First call succeeds
        ICounter(counter).setNumber(600);
        // Second call fails (require > 100 fails with 50)
        ICounter(counter).setNumber(50);
        
        vm.stopBroadcast();
    }
}
SOLEOF

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
