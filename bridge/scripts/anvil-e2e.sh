#!/usr/bin/env bash
# Real transactions on a local anvil node (127.0.0.1, chain id 31337; nothing leaves this machine).
# Deploys from anvil's account 0, at nonces 0, 1, 2, the RISC Zero Groth16 verifier (risc0-ethereum
# v3.0.0), the test token and FerrataVault (compiled from ../bridge-evm/src): the addresses the
# proofs bind. Registers ETH and TUSD, deposits, marks the burn root accepted with
# anvil_setStorageAt (TEST CHEAT standing in for the LEZ burn-root proof, which does not exist yet),
# then sends the two releases with the real Groth16 seals and one standalone `verify`, and reads
# gasUsed from each receipt. Log: artifacts/logs/<UTC time>-anvil-e2e.log
# This is the v1 run (artifacts/v1/). It compiled the vault from what was then a sibling repository
# (../../bridge-evm from evm-verify/); in this repository the v1 vault is commit a01fcd7 of vault/'s
# history, exported to artifacts/v1/vault-source/. The script is kept as it ran, not adapted.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export TMPDIR="$here/../../tmp" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
cd "$here/evm-verify"
log="$here/artifacts/logs/$(date -u +%Y%m%dT%H%M%SZ)-anvil-e2e.log"

# anvil's default development keys (public; mnemonic "test test test ... junk"), accounts 0..3.
K0=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
K1=0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d
K2=0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a
A0=$(cast wallet address --private-key $K0); A1=$(cast wallet address --private-key $K1)
A2=$(cast wallet address --private-key $K2); A3=0x90F79bf6EB2c4f870365E785982E1f101E93b906
MAX=115792089237316195423570985008687907853269984665640564039457584007913129639935
ETH=0x0000000000000000000000000000000000000000
PORT=18545; RPC=http://127.0.0.1:$PORT

fx() { python3 -I -c "import json,sys; print(json.load(open('test/fixtures/release-$1.json'))['$2'])"; }
hexdec() { python3 -I -c "print(int('$1', 16))"; }
send() { cast send --rpc-url $RPC --json "$@"; }
gas_of() { python3 -I -c "import json,sys; r=json.loads(sys.argv[1]); assert r['status'] in ('0x1', 1), r['status']; print(int(r['gasUsed'], 16))" "$1"; }

{
echo "== anvil end to end, $(date -Is), loadavg $(cat /proc/loadavg)"
anvil --port $PORT --chain-id 31337 --silent > /dev/null 2>&1 & APID=$!
trap 'kill $APID 2>/dev/null || true' EXIT
for _ in $(seq 1 100); do cast chain-id --rpc-url $RPC > /dev/null 2>&1 && break; sleep 0.1; done
echo "anvil $(anvil --version | head -1 | cut -d' ' -f2-), chain id $(cast chain-id --rpc-url $RPC), hardfork $(cast rpc --rpc-url $RPC anvil_nodeInfo | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["hardFork"])')"

ROOT_CTRL=$(grep -o 'CONTROL_ROOT = hex"[0-9a-f]*"' lib/risc0-ethereum/contracts/src/groth16/ControlID.sol | cut -d'"' -f2)
BN254=$(grep -o 'BN254_CONTROL_ID = hex"[0-9a-f]*"' lib/risc0-ethereum/contracts/src/groth16/ControlID.sol | cut -d'"' -f2)
IMAGE=$(fx eth releaseImageId); LEZ=$(fx eth lezConfigId)
ROOT_IMAGE=$(cast keccak "ferrata-bridge evm-verify: burn-root program (mocked)")

create() { forge create --broadcast --rpc-url $RPC --private-key $K0 --json "$@" | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["deployedTo"])'; }
VERIFIER=$(create lib/risc0-ethereum/contracts/src/groth16/RiscZeroGroth16Verifier.sol:RiscZeroGroth16Verifier --constructor-args 0x$ROOT_CTRL 0x$BN254)
TOKEN=$(create test/utils/TestContracts.sol:TestToken)
VAULT=$(create ../../bridge-evm/src/FerrataVault.sol:FerrataVault --constructor-args $VERIFIER $IMAGE $ROOT_IMAGE $LEZ $A0 $A3 64 "($MAX,0)" "($MAX,0)")
echo "verifier $VERIFIER (fixture $(fx eth verifier))"; echo "token    $TOKEN (fixture $(fx eth testToken))"; echo "vault    $VAULT (fixture $(fx eth vault))"
[ "${VERIFIER,,}" = "$(fx eth verifier | tr A-F a-f)" ] && [ "${TOKEN,,}" = "$(fx eth testToken | tr A-F a-f)" ] && [ "${VAULT,,}" = "$(fx eth vault | tr A-F a-f)" ] \
  || { echo "addresses differ from the ones the proofs bind"; exit 1; }
echo "verifier SELECTOR $(cast call --rpc-url $RPC $VERIFIER 'SELECTOR()(bytes4)')"

REG='registerToken(address,uint256[],uint256,uint256,(uint256,uint256),(uint256,uint256))'
send $VAULT "$REG" $ETH "[100000000000000000,1000000000000000000,10000000000000000000]" 1000000000000 3000 "($MAX,0)" "($MAX,0)" --private-key $K0 > /dev/null
send $VAULT "$REG" $TOKEN "[100000000,1000000000,10000000000]" 1 1 "($MAX,0)" "($MAX,0)" --private-key $K0 > /dev/null
send $VAULT 'setStandardRelayFee(address,uint8,uint256)' $TOKEN 0 250000 --private-key $K0 > /dev/null

r=$(send $VAULT 'depositETH(uint8,bytes32,uint64)' 0 $(cast keccak "someone's commitment 1") 0 --value 100000000000000000 --private-key $K1)
echo "depositETH 0.1 ETH (first deposit, empty tree): gasUsed $(gas_of "$r")"
send $TOKEN 'mint(address,uint256)' $A1 100000000 --private-key $K1 > /dev/null
send $TOKEN 'approve(address,uint256)' $VAULT 100000000 --private-key $K1 > /dev/null
r=$(send $VAULT 'deposit(address,uint8,bytes32,uint64)' $TOKEN 0 $(cast keccak "someone's commitment 2") 1 --private-key $K1)
echo "deposit 100 TUSD (second deposit): gasUsed $(gas_of "$r")"

ROOT=$(fx eth burnRoot); SLOT=$(cast index bytes32 $ROOT 46)   # burnRootAcceptedAt is slot 46
cast rpc --rpc-url $RPC anvil_setStorageAt $VAULT $SLOT 0x$(printf '%064x' 1) > /dev/null
echo "burnRootAcceptedAt[$ROOT] = $(cast call --rpc-url $RPC $VAULT 'burnRootAcceptedAt(bytes32)(uint256)' $ROOT) (set by anvil_setStorageAt)"

REL='release((address,uint8,address,address,uint256,bytes32,bytes32),bytes)'
for c in eth erc20; do
  tok=$(fx $c token); fee=$(hexdec $(fx $c relayFee)); recipient=$(fx $c recipient)
  before=$( [ $c = eth ] && cast balance --rpc-url $RPC $recipient || cast call --rpc-url $RPC $TOKEN 'balanceOf(address)(uint256)' $recipient)
  req="($tok,0,$recipient,$(fx $c feeRecipient),$fee,$(fx $c burnRoot),$(fx $c nullifier))"
  r=$(send $VAULT "$REL" "$req" $(fx $c seal) --private-key $K2)
  after=$( [ $c = eth ] && cast balance --rpc-url $RPC $recipient || cast call --rpc-url $RPC $TOKEN 'balanceOf(address)(uint256)' $recipient)
  echo "release $c: gasUsed $(gas_of "$r") (transaction total: 21,000 + calldata + execution); recipient balance ${before%% *} -> ${after%% *}"
  if send $VAULT "$REL" "$req" $(fx $c seal) --private-key $K2 > /dev/null 2>&1; then echo "REPLAY ACCEPTED"; exit 1; fi
  echo "  replay of the same proof: rejected"
done

r=$(send $VERIFIER 'verify(bytes,bytes32,bytes32)' $(fx eth seal) $IMAGE $(fx eth journalDigest) --private-key $K2)
echo "verify alone, as its own transaction: gasUsed $(gas_of "$r")"
echo "loadavg after $(cat /proc/loadavg)"
} 2>&1 | tee "$log"
echo "log: $log"
