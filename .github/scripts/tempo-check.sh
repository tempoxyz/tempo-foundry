#!/bin/bash
set -euo pipefail

# Non-verification tempo checks: local tests, fork tests, cast commands, DEX operations

# Fee token address, defaults to native fee token
FEE_TOKEN="${TEMPO_FEE_TOKEN:-0x20c0000000000000000000000000000000000000}"

# Build fee token args if not using native token (array for safe expansion)
FEE_TOKEN_ARG=()
if [[ "$FEE_TOKEN" != "0x20c0000000000000000000000000000000000000" ]]; then
  FEE_TOKEN_ARG=(--fee-token "$FEE_TOKEN")
fi

echo -e "\n=== USING FEE TOKEN: $FEE_TOKEN ==="

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

echo -e "\n=== CAST MKTX WITH SPONSOR ==="
# Build a transaction where the sponsor pays gas for the sender
cast mktx ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --sponsor "$SPONSOR_PK"

echo -e "\n=== CAST SEND WITH SPONSOR ==="
# Send a sponsored transaction and verify the receipt shows the correct fee_payer
RECEIPT=$(cast send ${FEE_TOKEN_ARG[@]+"${FEE_TOKEN_ARG[@]}"} --rpc-url "$ETH_RPC_URL" 0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D 'increment()' --private-key "$PK" --sponsor "$SPONSOR_PK" --json)

# Verify the fee_payer in the receipt matches the sponsor address
RECEIPT_FEE_PAYER=$(echo "$RECEIPT" | jq -r '.feePayer // .fee_payer // empty')
if [[ -n "$RECEIPT_FEE_PAYER" ]]; then
  # Normalize addresses for comparison (lowercase)
  RECEIPT_FEE_PAYER_LOWER=$(echo "$RECEIPT_FEE_PAYER" | tr '[:upper:]' '[:lower:]')
  SPONSOR_ADDR_LOWER=$(echo "$SPONSOR_ADDR" | tr '[:upper:]' '[:lower:]')
  if [[ "$RECEIPT_FEE_PAYER_LOWER" == "$SPONSOR_ADDR_LOWER" ]]; then
    echo "SUCCESS: Receipt feePayer ($RECEIPT_FEE_PAYER) matches sponsor address"
  else
    echo "ERROR: Receipt feePayer ($RECEIPT_FEE_PAYER) does not match sponsor ($SPONSOR_ADDR)"
    exit 1
  fi
else
  echo "WARNING: feePayer not found in receipt (may not be supported on this devnet)"
  echo "Receipt: $RECEIPT"
fi

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
