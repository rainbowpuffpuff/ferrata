#!/usr/bin/env bash
# RFP-022 F.13 end to end: the protocol fees charged on LEZ in the wrapped token (docs/decisions/0048-*). The bridge
# round trip of scripts/ci/sequencer-roundtrip-hook.sh (its tools, stand-ins and records; read its header first), with
# two deposits under two fee schedules in between: schedule 0 (no protocol fee) and schedule 1 (a mint fee and a burn
# fee announced by the admin, in force after the vault's seven days, which anvil's clock passes). Run by
# lez/live/run.sh's phase `hook` while LEZ's standalone sequencer runs (FERRATA_CI_RT_HOOK=e2e/f13/hook.sh, through
# e2e/rc3-roundtrip/run.sh for LEZ v0.4.0-rc3).
# Steps (one line each in steps.tsv; the first failure stops the run):
#   fork, zone: programs and the stand-in instance, vault deployed on the fork      as the CI hook (the bridge program
#                   must be docs/image-ids.md's FERRATA_F13_BRIDGE_ROW, default ferrata_bridge_lezfee)
#   deposit under schedule 0
#                   `ferrata deposit` of one denomination to the user's public /0: the vault's escrow grows by the
#                   denomination and the relay fee, its LEZ fee total (lezFees) and the Ethereum fee recipient's credit
#                   (owed) by nothing; the vault captured at that block
#   fee change      the admin announces schedule 1 (a mint fee of 0.5% and a burn fee of 0.2% of each denomination, the
#                   same checkpoint fees), anvil's clock passes the seven days, applyFees puts it in force
#   deposit under schedule 1
#                   `ferrata deposit` again (the tool prices it under schedule 1): the escrow grows by the denomination,
#                   the relay fee and the mint fee, lezFees by exactly the mint fee (LezFeesAccrued), owed by nothing;
#                   the vault captured at that block
#   operator import crown-driver crown-b --operator-only over both captures: Init, Register ETH, ImportRoot n=1, then
#                   ImportRoot n=2 (schedule 1 is not imported: neither claims nor collections read it)
#   fees            seqnet-fund --public-only (as the CI hook)
#   claim of leaf 0 (schedule 0), claim of leaf 1 (schedule 1)
#                   `ferrata claim --plaintext --leaf N --relayer <the user's public /1>`: each claim mints exactly the
#                   denomination to /0 and the registered relay fee to /1, whatever the deposit's schedule; neither
#                   claim names a schedule or a fee (the message has no such field)
#   mint fees collected
#                   crown-driver collect-fees (anyone may send CollectFees; on this stand-in chain the operator's tool
#                   does, since no Ethereum endpoint serves a stand-in record's block to ferrata-relayer collect-fees):
#                   CollectFees attests the vault's lezFees under capture 2's record and mints it, in the wrapped token,
#                   to the configuration's fee recipient on LEZ: its holding grows by schedule 1's mint fee
#   nothing more to collect
#                   the same collection again: refused by the program (244), nothing minted
#   claim again refused, burn, checkpoint journal, checkpoint accepted, release journal
#                   as the CI hook (one denomination burnt)
#   release paid    the release under schedule 1 pays the fresh address the denomination less the relay, burn and
#                   checkpoint fees; the burn fee stays in escrow and lezFees grows by it; owed grows by the checkpoint
#                   fee only (the prover's)
#   record after the release
#                   the vault captured after the release; crown-driver crown-b --skip-stages 2 --updates-only yes: the
#                   stand-in committee finalizes that block (a record, no bridge call)
#   burn fee collected
#                   crown-driver collect-fees under capture 3's record: the fee recipient's holding grows by schedule
#                   1's burn fee
#   older record refused
#                   a collection under capture 2's record (a smaller total): refused by the program (247)
#   escrow backs LEZ
#                   every deposit claimed, the burn released, every fee collected: the vault's ETH escrow equals the
#                   wrapped ETH on LEZ (the user's /0 and /1 and the fee recipient)
#   release again refused
# Records in CI_RT_OUT as the CI hook's, and in facts.json the F.13 facts (fees.*).
set -uo pipefail
REPO="$CI_RT_REPO"; R="$CI_RT_OUT"; W="$CI_RT_WORK"; B="$CI_RT_TOOLS"; SEQ="$ZONE_RPC"; SEPOLIA_RPCS="$CI_RT_SEPOLIA_RPCS"
FORK_MAX_AGE=3600                                                    # s: the finalized block's age when forked
PORT=18547
RPC="http://127.0.0.1:$PORT"
CHAIN=11155111
DEPLOYER=0x000000000000000000000000000000005E9DE901                 # impersonated: nobody holds its key
RELAYER=0x70997970C51812dc3A010C7d01b50e0d17dc79C8                  # anvil's published dev account 1 (impersonated)
DEPOSITOR=0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC                 # anvil's published dev account 2 (its keystore)
ANVIL_MNEMONIC="test test test test test test test test test test test junk"
PHRASE="abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
SEAL=0x5ea15ea1                                                      # the mock verifier's seal
SEPOLIA_GENESIS_TIME=1655733600
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
unset RISC0_DEV_MODE

rm -rf "$W"; mkdir -p "$W" "$R/user" "$R/lez" "$R/eth" "$R/checkpoint"
STEPS="$R/steps.tsv"; : > "$STEPS"
CURRENT=start; STEP_T0=$(date +%s); ANVIL=""
say() { echo "[hook $(date -u +%T)] $*"; }
begin() { CURRENT="$1"; STEP_T0=$(date +%s); say "== $1"; }
ok() { printf '%s\tok\t%s\t%s\n' "$CURRENT" "$(( $(date +%s) - STEP_T0 ))" "${1:-}" >> "$STEPS"; say "ok: $CURRENT${1:+: $1}"; }
die() { printf '%s\tFAILED\t%s\t%s\n' "$CURRENT" "$(( $(date +%s) - STEP_T0 ))" "$*" >> "$STEPS"; say "STOP in \"$CURRENT\": $*" >&2; exit 1; }
cleanup() { if [ -n "$ANVIL" ]; then kill "$ANVIL" 2>/dev/null; wait "$ANVIL" 2>/dev/null; fi; }
trap cleanup EXIT
lower() { tr 'A-F' 'a-f'; }
# int TEXT: TEXT as a decimal integer if it is one (decimal or 0x hex), else nothing and status 1; never arithmetic on
# other text (bash evaluates subscripts such as a[$(cmd)] in $(( ))).
int() { [[ "$1" =~ ^(0x[0-9a-fA-F]+|0|[1-9][0-9]*)$ ]] && echo $(( $1 )); }
# u NAME CMD...: one command of the user; what it printed in user/NAME.txt
u() {
  local n="$1" rc; shift
  { echo "\$ ${*//$B\//}"; echo; "$@"; } > "$R/user/$n.txt" 2>&1; rc=$?
  sed -i -e "s#$W/#<work>/#g" -e "s#$B/##g" -e "s#RISC0_HOME=[^ ]*#RISC0_HOME=<risc0-home>#g" "$R/user/$n.txt"
  printf '%s\texit %s\n' "$n" "$rc" >> "$R/user/INDEX"
  return $rc
}
# The tool's --json answer (one line beginning {"command"; LEZ's wallet prints its sync on stdout too).
json_of() { grep -E '^\{"command"' "$1" | tail -n 1; }
eth_now() { printf '%d\n' "$(cast block latest --rpc-url "$RPC" --json | jq -r .timestamp)"; }
pass_time() { cast rpc evm_increaseTime "$1" --rpc-url "$RPC" > /dev/null && cast rpc evm_mine --rpc-url "$RPC" > /dev/null || die "evm_increaseTime"; say "the fork's clock moved on $1 s: $2"; }
call() { cast call "$VAULT" "$@" --rpc-url "$RPC"; }
docs_id() { python3 -I -c 'import re, sys; m = re.search(r"`" + re.escape(sys.argv[2]) + r"` \| `(0x[0-9a-f]{64})`", open(sys.argv[1]).read()); print(m.group(1) if m else "")' "$REPO/docs/image-ids.md" "$1"; }
image_id() { "$B/ferrata-e2e" image-id "$1" 2>/dev/null | grep -o -E '0x[0-9a-f]{64}' | head -n 1; }
relayer_round() {
  "$B/ferrata-relayer" --eth-rpc "$RPC" --vault "$VAULT" --chain-id "$CHAIN" --unlocked "$RELAYER" --from-block "$FROM" \
    --state "$W/relayer" run --inbox "$W/relayer-inbox" --poll-secs 1 --rounds 1 >> "$R/eth/relayer.jsonl" 2>> "$W/relayer.err" \
    || { tail -n 10 "$W/relayer.err"; die "a relayer round failed"; }
}
# wait_tx FILE: the LEZ transaction a command of the user sent ("sent: HASH") applied in a block of the zone
wait_tx() {
  local h line i
  h="$(grep -o -E 'sent: [0-9a-fA-Fx]+' "$1" | tail -n 1 | awk '{print $2}')"
  [ -n "$h" ] || die "no transaction hash in ${1#"$R"/}"
  for i in $(seq 1 60); do
    line="$("$B/seqnet-txs" --sequencer "$SEQ" --home "$W/txs-wallet" --bridge-account "$BRIDGE" --attest-account "$ATTEST" 2>/dev/null | grep -F "$h")"
    if [ -n "$line" ]; then
      echo "$line" | awk -F'\t' '{print $7}' | grep -q false || die "LEZ transaction $h reverted"
      echo "$h"
      return 0
    fi
    sleep 3
  done
  die "LEZ transaction $h not in a block after 3 min"
}
# holding KIND NAME [ACCOUNT]: the amount the user's account /ACCOUNT (default 0) of KIND holds, from `ferrata recover`
# (0 without a holding)
holding() {
  u "$2" "${F[@]}" --json recover || die "recover"
  json_of "$R/user/$2.txt" | jq -r --arg k "$1" --argjson a "${3:-0}" '[.result.holdings[] | select(.kind == $k and .account == $a) | .amount][0] // "0"'
}

# ---- fork ----------------------------------------------------------------------------------------------------------
begin "fork"
FORK_RPC=""
for rpc in $SEPOLIA_RPCS; do
  for attempt in 1 2 3; do
    [ "$attempt" = 1 ] || sleep 30
    if fin="$(cast block finalized --json --rpc-url "$rpc" 2>>"$W/fork.err")" \
       && FORK_BLOCK="$(int "$(echo "$fin" | jq -r .number)")" && FORK_TIME="$(int "$(echo "$fin" | jq -r .timestamp)")" \
       && FORK_HASH="$(echo "$fin" | jq -r .hash)" && [[ "$FORK_HASH" =~ ^0x[0-9a-fA-F]{64}$ ]]; then
      FORK_AGE=$(( $(date +%s) - FORK_TIME ))
      if [ "$FORK_AGE" -le "$FORK_MAX_AGE" ] && [ "$FORK_AGE" -ge -300 ]; then FORK_RPC="$rpc"; break 2; fi
      say "$rpc: its finalized block $FORK_BLOCK is $FORK_AGE s old by this machine's clock (bound $FORK_MAX_AGE s; attempt $attempt of 3)"
    else
      say "$rpc: no usable finalized block (attempt $attempt of 3; cast's last error: $(tail -n 1 "$W/fork.err" | cut -c1-200))"
    fi
  done
done
[ -n "$FORK_RPC" ] || die "no public Sepolia RPC answered with a finalized block under $FORK_MAX_AGE s old (Sepolia not finalizing, or lagging endpoints: $SEPOLIA_RPCS)"
anvil --fork-url "$FORK_RPC" --fork-block-number "$FORK_BLOCK" --port "$PORT" --auto-impersonate --gas-limit 100000000 \
  --retries 10 --fork-retry-backoff 2000 --silent > "$W/anvil.log" 2>&1 & ANVIL=$!
for _ in $(seq 120); do cast chain-id --rpc-url "$RPC" > /dev/null 2>&1 && break; sleep 1; done
[ "$(cast chain-id --rpc-url "$RPC" 2>/dev/null)" = "$CHAIN" ] || { tail -n 20 "$W/anvil.log"; die "anvil did not fork Sepolia (chain id $CHAIN)"; }
got="$(cast block "$FORK_BLOCK" --json --rpc-url "$RPC" | jq -r .hash)"
[ "$(echo "$got" | lower)" = "$(echo "$FORK_HASH" | lower)" ] || die "the fork's block $FORK_BLOCK is $got, Sepolia's $FORK_HASH"
for a in "$DEPLOYER" "$RELAYER" "$DEPOSITOR"; do cast rpc anvil_setBalance "$a" 0x56bc75e2d63100000 --rpc-url "$RPC" > /dev/null || die "anvil_setBalance"; done
# The fresh release address, read once now: the public RPC keeps the fork block's state for a limited time only.
FRESH="$(cast wallet new --json | jq -r '.[0].address')"
[ "$(cast balance "$FRESH" --rpc-url "$RPC")" = 0 ] || die "the fresh address holds ETH on the fork"
ok "Sepolia block $FORK_BLOCK ($(date -u -d "@$FORK_TIME" +%FT%TZ), $FORK_AGE s old), hash $FORK_HASH, from $FORK_RPC; anvil forks it, chain id $CHAIN"

# ---- zone: the programs and the stand-in instance ------------------------------------------------------------------
begin "zone: programs and the stand-in instance"
now_slot=$(( ($(date +%s) - SEPOLIA_GENESIS_TIME) / 12 )); period=$(( now_slot / 8192 ))
if [ $(( now_slot - period * 8192 )) -ge 600 ]; then STAND_IN_SLOT=$(( period * 8192 + 64 )); else STAND_IN_SLOT=$(( (period - 1) * 8192 + 64 )); fi
export CROWN_STAND_IN_SLOT="$STAND_IN_SLOT"
# A standalone zone has no Logos Blockchain channel: session A's channel and genesis are tags (the bridge program
# checks neither; the checkpoint's key set is ci_zone's, below).
printf '{"accredited_keys": ["%s"], "posting_timeframe": 0, "config_tip_hash": "%s"}\n' "$(printf '01%.0s' $(seq 32))" "$(printf '00%.0s' $(seq 32))" > "$W/channel.json"
printf 'ferrata CI: a LEZ %s standalone sequencer (mock Bedrock), no Logos Blockchain genesis\n' "${LEZ_TAG:-v0.4.0-rc1}" > "$W/zone-genesis.txt"
D=(--program-bin "$B/ferrata_lez.bin" --bridge-bin "$B/ferrata_bridge.bin" --rpc "$SEQ" --run-dir "$W/crown" --wait-final no
   --sequencer-log "$ZONE_LOG/sequencer.log")
"$B/crown-driver" crown-a --chain stand-in "${D[@]}" --channel "$W/channel.json" --genesis "$W/zone-genesis.txt" \
  --identity-out "$W/session-a.json" > "$R/lez/crown-a.log" 2>&1 || { tail -n 25 "$R/lez/crown-a.log"; die "crown-driver crown-a"; }
SA="$W/session-a.json"; cp "$SA" "$R/lez/session-a.json"
ATTEST="$(jq -r .attestation.account "$SA")"; BRIDGE="$(jq -r .bridge.header "$SA")"
[ "$(jq -r .attestation.imageId "$SA")" = "$(docs_id ferrata_lez)" ] || die "the attestation program is not docs/image-ids.md's ferrata_lez"
BRIDGE_ROW="${FERRATA_F13_BRIDGE_ROW:-ferrata_bridge_lezfee}"
[ -n "$(docs_id "$BRIDGE_ROW")" ] || die "docs/image-ids.md has no row $BRIDGE_ROW"
[ "$(jq -r .bridge.imageId "$SA")" = "$(docs_id "$BRIDGE_ROW")" ] || die "the bridge program is not docs/image-ids.md's $BRIDGE_ROW"
ok "attestation program $(jq -r .attestation.imageId "$SA" | cut -c1-12)… and bridge program $(jq -r .bridge.imageId "$SA" | cut -c1-12)… deployed through LEZ's loader; stand-in instance $(jq -r .instance.id "$SA" | cut -c1-12)… (bootstrap slot $STAND_IN_SLOT); zone tip $(jq -r .tip.id "$SA")"

# ---- the vault on the fork -----------------------------------------------------------------------------------------
begin "vault deployed on the fork"
# DeployVault deploys RISC Zero's Groth16 verifier first, then the stop, the library and the vault: the fourth creation.
N="$(cast nonce "$DEPLOYER" --rpc-url "$RPC")"
VAULT="$(cast compute-address "$DEPLOYER" --nonce $(( N + 3 )) | awk '{print $NF}')"
"$B/crown-driver" config-id --identity "$SA" --vault "$VAULT" > "$W/config.json" 2>"$W/config.err" || { cat "$W/config.err"; die "crown-driver config-id"; }
CID="$(jq -r .configId "$W/config.json")"
jq -n --arg v "$VAULT" --argjson s "$(cat "$SA")" --argjson c "$CHAIN" '{evm_chain_id: $c, vault: $v,
  bedrock_genesis_id: $s.bedrockGenesisId, channel_id: $s.channel.id, bridge_header: $s.bridge.header,
  bridge_image_id: $s.bridge.imageId, attestation_root_id: $s.attestationRootId, salt: $s.salt}' > "$W/ci-identity.json"
"$B/ci_zone" genesis --identity "$W/ci-identity.json" > "$W/ci-genesis.json" 2>"$W/ci-genesis.err" || { cat "$W/ci-genesis.err"; die "ci_zone genesis"; }
[ "$(jq -r .configId "$W/ci-genesis.json" | lower)" = "$(echo "$CID" | lower)" ] || die "ci_zone's configId $(jq -r .configId "$W/ci-genesis.json") is not the bridge configuration's $CID"
GS="$(jq -r .genesisState "$W/ci-genesis.json")"
CPB_ID="$(image_id "$B/checkpoint_b.bin")"; REL_ID="$(image_id "$B/release.bin")"
[ -n "$CPB_ID" ] && [ -n "$REL_ID" ] || die "ferrata-e2e image-id"
[ "$CPB_ID" = "$(docs_id checkpoint_b_31)" ] && [ "$REL_ID" = "$(docs_id release_31)" ] || die "the guests built are not docs/image-ids.md's checkpoint_b_31 and release_31 ($CPB_ID, $REL_ID)"
FROM="$(cast block-number --rpc-url "$RPC")"
jq -n --arg c "$CID" --arg g "$GS" --arg a "$(docs_id checkpoint_31)" --arg b "$CPB_ID" --arg r "$REL_ID" \
  --arg z 0x0000000000000000000000000000000000000000000000000000000000000000 \
  '{_source: "CI sequencer round trip (scripts/ci/sequencer-roundtrip.sh): rule (B); the checkpoint key set is ci_zone test committee (n = 4, nMin 3, tMin 2)",
    checkpointImageIdA: $a, checkpointImageIdB: $b, releaseImageId: $r, lezConfigId: $c, genesisState: $g, nMin: 3, tMin: 2,
    postingTimeframe: 0, ethLezDefinition: $z, tokenLezDefinition: $z}' > "$R/params.json"
jq -n --arg v "$VAULT" --arg c "$CID" --arg g "$GS" --argjson ch "$CHAIN" \
  '{label: "CI: anvil forking Sepolia; DeployVault deploys RISC Zero Groth16 verifier, whose code is then replaced on the fork by the vault tests MockRiscZeroVerifier",
    chainId: $ch, vault: $v, configId: $c, genesisState: $g, delta: 3600}' > "$R/identity.json"
jq -n --arg v "$VAULT" --argjson ch "$CHAIN" '{chainId: $ch, vault: $v, deposits: []}' > "$R/crown.json"
SEPOLIA_RPC_URL="$RPC" ETH_FROM="$DEPLOYER" FERRATA_UNLOCKED=1 FERRATA_NONCE_GAP=0 bash "$REPO/e2e/crown/sepolia.sh" "$R" deploy \
  > "$W/deploy.log" 2>&1 || { tail -n 30 "$W/deploy.log"; die "the vault's deployment (e2e/crown/sepolia.sh deploy)"; }
rm -f "$REPO/vault/deployments/crown-$(basename "$R").json"
[ "$(call 'mode()(uint8)')" = 1 ] || die "the vault does not run rule (B)"
G16="$(jq -r '.groth16Verifier // empty' "$R/deploy.json")"
[ -n "$G16" ] || die "deploy.json names no Groth16 verifier"
REAL_CODEHASH="$(cast keccak "$(cast code "$G16" --rpc-url "$RPC")")"
# The mock's runtime code: from forge's build artifact, else from a copy deployed by another impersonated address (the
# deployer's nonce is left alone).
MOCK_CODE="$(jq -r '.deployedBytecode.object // empty' "$REPO/vault/out/MockVerifiers.sol/MockRiscZeroVerifier.json" 2>/dev/null)"
if [ -z "$MOCK_CODE" ] || [ "$MOCK_CODE" = 0x ]; then
  AUX=0x000000000000000000000000000000005E9DE902
  cast rpc anvil_setBalance "$AUX" 0x56bc75e2d63100000 --rpc-url "$RPC" > /dev/null
  copy="$(cd "$REPO/vault" && forge create test/mocks/MockVerifiers.sol:MockRiscZeroVerifier --rpc-url "$RPC" --unlocked \
    --from "$AUX" --broadcast --json 2>"$W/mock.err" | jq -r .deployedTo)"
  [ -n "$copy" ] && [ "$copy" != null ] || { tail -n 10 "$W/mock.err"; die "no MockRiscZeroVerifier code (artifact or copy)"; }
  MOCK_CODE="$(cast code "$copy" --rpc-url "$RPC")"
fi
[ -n "$MOCK_CODE" ] && [ "$MOCK_CODE" != 0x ] || die "no MockRiscZeroVerifier code"
cast rpc anvil_setCode "$G16" "$MOCK_CODE" --rpc-url "$RPC" > /dev/null || die "anvil_setCode"
MOCK="$G16"
[ "$(cast call "$MOCK" 'accepted(bytes32,bytes32)(bool)' "$CPB_ID" "$CPB_ID" --rpc-url "$RPC")" = false ] || die "the mock's code is not at the verifier's address"
ok "vault $VAULT (rule B), configId $CID, genesisState $GS (ci_zone's test committee); RISC Zero's Groth16 verifier $G16 (code hash $REAL_CODEHASH) deployed by DeployVault, its code then replaced on the fork by MockRiscZeroVerifier's; checkpoint guest $CPB_ID, release guest $REL_ID"

# ---- the two deposits, under two fee schedules ----------------------------------------------------------------------
U="$W/user"; mkdir -p "$U"
printf '%s\n' "$PHRASE" > "$U/seed.txt"; printf 'ci-user\n' > "$U/lez.pass"; printf 'ci-depositor\n' > "$U/eth.pass"
chmod 600 "$U/seed.txt" "$U/lez.pass" "$U/eth.pass"
cast wallet import depositor --private-key "$(cast wallet private-key --mnemonic "$ANVIL_MNEMONIC" --mnemonic-index 2)" \
  --keystore-dir "$U/keystore" --unsafe-password ci-depositor > /dev/null || die "the depositor's keystore"
[ "$(cast wallet address --keystore "$U/keystore/depositor" --password ci-depositor)" = "$DEPOSITOR" ] || die "the depositor's keystore is not anvil's account 2"
FE=("$B/ferrata" --seed "$U/seed.txt" --eth-rpc "$RPC" --chain-id "$CHAIN" --vault "$VAULT" --from-block "$FROM")
ETH=0x0000000000000000000000000000000000000000
FEE_RECIPIENT="$(call 'feeRecipient()(address)')" || die "the vault's feeRecipient"
DENOM="$(python3 -I -c 'print(10**15)')"
RELAY0="$(call 'fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)' "$ETH" 0 0 | sed -n 2p | awk '{print $1}')"
[ -n "$(int "$RELAY0")" ] && [ "$RELAY0" -gt 0 ] || die "denomination 0's relay fee ($RELAY0) is not a positive number"
owed_fr() { call 'owed(address,address)(uint256)' "$FEE_RECIPIENT" "$ETH" | awk '{print $1}'; }
escrow() { call 'escrowed(address)(uint256)' "$ETH" | awk '{print $1}'; }
lez_fees() { call 'lezFees(address)(uint256)' "$ETH" | awk '{print $1}'; }
# log_sum TX EVENT-SIGNATURE: the sum of the data words of TX's logs of that event (one uint256 each, or the first)
log_sum() {
  cast receipt "$1" --rpc-url "$RPC" --json | python3 -I -c 'import json, sys; r = json.load(sys.stdin); t = sys.argv[1].lower(); print(sum(int(l["data"][2:66], 16) for l in r["logs"] if l["topics"] and l["topics"][0].lower() == t))' "$(cast keccak "$2")"
}
capture() {
  local slots s; slots="$("$B/crown-driver" capture-slots | jq -r '.slots[].slot')"; local sargs=()
  for s in $slots; do sargs+=(--slot "$s"); done
  python3 -I "$REPO/lez/crown/capture-anvil.py" --rpc "$RPC" --vault "$VAULT" --from-block "$FROM" "${sargs[@]}" "$W/$1" \
    > "$W/$1.log" 2>&1 || { tail -n 20 "$W/$1.log"; die "the capture of the vault ($1)"; }
}
# deposit_step N FILE-PREFIX: the user's `ferrata deposit` of denomination 0 to the public /0 (a dry run, then sent);
# prints the Ethereum transaction
deposit_step() {
  u "$2-dry-run" "${FE[@]}" deposit --denomination 0 --kind public --i-accept-public-account --from "$DEPOSITOR" || die "the deposit's dry run"
  u "$2" "${FE[@]}" deposit --denomination 0 --kind public --i-accept-public-account --keystore "$U/keystore/depositor" \
    --password-file "$U/eth.pass" --send --yes || die "the deposit"
  local tx; tx="$(grep -o -E 'sent: 0x[0-9a-fA-F]{64}' "$R/user/$2.txt" | tail -n 1 | awk '{print $2}')"
  [ -n "$tx" ] || die "the deposit printed no transaction (user/$2.txt)"
  [ "$(call 'nextLeafIndex()(uint64)')" = "$1" ] || die "the vault holds $(call 'nextLeafIndex()(uint64)') deposits, not $1"
  echo "$tx"
}

begin "deposit under schedule 0"
u 01-keys "${FE[@]}" keys --accounts 2 || die "keys"
PUB1="$(awk '/^LEZ public accounts/ {f = 1; next} /^LEZ private accounts/ {f = 0} f && $1 == "/1" {print $2}' "$R/user/01-keys.txt")"
[[ "$PUB1" =~ ^0x[0-9a-f]{64}$ ]] || die "keys printed no public /1 (user/01-keys.txt)"
u 02-status "${FE[@]}" status || die "status"
[ "$(call 'feeState(address)(uint64,uint64)' "$ETH" | head -n 1)" = 0 ] || die "ETH is not under fee schedule 0"
OWED_0="$(owed_fr)"; ESC_0="$(escrow)"; LF_0="$(lez_fees)"
[ "$LF_0" = 0 ] || die "the vault's lezFees is $LF_0 before any deposit"
DEPOSIT_TX="$(deposit_step 1 04-deposit)" || die "the deposit under schedule 0"
OWED_1="$(owed_fr)"; ESC_1="$(escrow)"; LF_1="$(lez_fees)"
[ "$(( ESC_1 - ESC_0 ))" = "$(( DENOM + RELAY0 ))" ] || die "the escrow grew by $(( ESC_1 - ESC_0 )), not the denomination and the relay fee ($(( DENOM + RELAY0 )))"
[ "$OWED_1" = "$OWED_0" ] || die "the fee recipient was credited $(( OWED_1 - OWED_0 )) on Ethereum at a deposit under schedule 0"
[ "$LF_1" = "$LF_0" ] || die "lezFees grew by $(( LF_1 - LF_0 )) at a deposit under schedule 0 (no mint fee)"
capture capture-1
u 05-scan "${FE[@]}" scan || die "scan"
ok "Ethereum tx $DEPOSIT_TX: 0.001 ETH to the user's public /0; escrow +$(( ESC_1 - ESC_0 )) wei (the denomination and the relay fee $RELAY0), lezFees +0, the Ethereum fee recipient's credit +0; the vault captured"

begin "fee change"
read -r -a CPF <<< "$(for i in 0 1 2; do call 'fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)' "$ETH" 0 "$i" | sed -n 4p | awk '{print $1}'; done | tr '\n' ' ')"
read -r -a BF <<< "$(for i in 0 1 2; do call 'fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)' "$ETH" 0 "$i" | sed -n 3p | awk '{print $1}'; done | tr '\n' ' ')"
MINT=(5000000000000 50000000000000 500000000000000)   # 0.5% of 0.001, 0.01 and 0.1 ETH: whole LEZ units, under the 1% ceiling
BURN=(2000000000000 20000000000000 200000000000000)   # 0.2%: whole LEZ units too (ADR 0048: burn fees are minted on LEZ)
cast send "$VAULT" 'announceFees(address,uint256[],uint256[],uint256[])' "$ETH" "[${MINT[0]},${MINT[1]},${MINT[2]}]" \
  "[${BURN[0]},${BURN[1]},${BURN[2]}]" "[${CPF[0]},${CPF[1]},${CPF[2]}]" --rpc-url "$RPC" --unlocked --from "$DEPLOYER" > "$W/announce.txt" 2>&1 \
  || { cat "$W/announce.txt"; die "announceFees"; }
PENDING_AT="$(call 'feeState(address)(uint64,uint64)' "$ETH" | sed -n 2p | awk '{print $1}')"
[ -n "$(int "$PENDING_AT")" ] && [ "$PENDING_AT" -gt "$(eth_now)" ] || die "no pending fee schedule after announceFees ($PENDING_AT)"
pass_time $(( PENDING_AT - $(eth_now) + 1 )) "the fee announcement's seven days"
cast send "$VAULT" 'applyFees(address)' "$ETH" --rpc-url "$RPC" --unlocked --from "$DEPLOYER" > "$W/apply.txt" 2>&1 || { cat "$W/apply.txt"; die "applyFees"; }
[ "$(call 'feeState(address)(uint64,uint64)' "$ETH" | head -n 1)" = 1 ] || die "fee schedule 1 is not in force after applyFees"
M1="$(call 'fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)' "$ETH" 1 0 | sed -n 1p | awk '{print $1}')"
R1="$(call 'fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)' "$ETH" 1 0 | sed -n 2p | awk '{print $1}')"
B1="$(call 'fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)' "$ETH" 1 0 | sed -n 3p | awk '{print $1}')"
CP1="$(call 'fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)' "$ETH" 1 0 | sed -n 4p | awk '{print $1}')"
[ "$M1" = "${MINT[0]}" ] && [ "$R1" = "$RELAY0" ] && [ "$B1" = "${BURN[0]}" ] || die "schedule 1 of denomination 0: mint fee $M1 (want ${MINT[0]}), relay fee $R1 (want $RELAY0), burn fee $B1 (want ${BURN[0]})"
ok "announceFees (mint fees ${MINT[*]} wei, burn fees ${BURN[*]} wei (schedule 0: ${BF[*]}), checkpoint ${CPF[*]} unchanged); the seven days passed on the fork's clock; schedule 1 in force: denomination 0 pays mint fee $M1, relay fee $R1, burn fee $B1 at release"

begin "deposit under schedule 1"
DEPOSIT2_TX="$(deposit_step 2 06-deposit-2)" || die "the deposit under schedule 1"
grep -q -E "mint fee|$M1" "$R/user/06-deposit-2.txt" || say "note: the deposit's output names no mint fee"
OWED_2="$(owed_fr)"; ESC_2="$(escrow)"; LF_2="$(lez_fees)"
[ "$(( ESC_2 - ESC_1 ))" = "$(( DENOM + RELAY0 + M1 ))" ] || die "the escrow grew by $(( ESC_2 - ESC_1 )) at the deposit under schedule 1, not the denomination, the relay fee and the mint fee ($(( DENOM + RELAY0 + M1 )))"
[ "$(( LF_2 - LF_1 ))" = "$M1" ] || die "lezFees grew by $(( LF_2 - LF_1 )) at the deposit under schedule 1, not its mint fee $M1"
[ "$OWED_2" = "$OWED_1" ] || die "the Ethereum fee recipient was credited $(( OWED_2 - OWED_1 )) at the deposit (ADR 0048: nothing on Ethereum)"
CREDIT_EV="$(log_sum "$DEPOSIT2_TX" 'OwedCredited(address,address,uint256)')"
ACCRUED_EV="$(log_sum "$DEPOSIT2_TX" 'LezFeesAccrued(address,uint256,uint256)')"
[ "$CREDIT_EV" = 0 ] || die "the deposit's OwedCredited events credit $CREDIT_EV (ADR 0048: none)"
[ "$ACCRUED_EV" = "$M1" ] || die "the deposit's LezFeesAccrued events accrue $ACCRUED_EV, not the mint fee $M1"
capture capture-2
ok "Ethereum tx $DEPOSIT2_TX: escrow +$(( ESC_2 - ESC_1 )) wei (the denomination, the relay fee and the mint fee $M1); lezFees +$(( LF_2 - LF_1 )) (LezFeesAccrued $ACCRUED_EV), the Ethereum fee recipient $FEE_RECIPIENT credited nothing; the vault captured"

# ---- the operator's import -----------------------------------------------------------------------------------------
begin "operator import"
"$B/crown-driver" check --chain stand-in --program-bin "$B/ferrata_lez.bin" --bridge-bin "$B/ferrata_bridge.bin" \
  --capture "$W/capture-1" --capture "$W/capture-2" --identity "$SA" --operator-only yes > "$W/check.log" 2>&1 || { tail -n 20 "$W/check.log"; die "crown-driver check"; }
"$B/crown-driver" crown-b --chain stand-in "${D[@]}" --identity "$SA" --capture "$W/capture-1" --capture "$W/capture-2" \
  --private fake --operator-only yes --session-out "$W/session-b.json" --notes-out "$W/notes.json" > "$R/lez/crown-b-operator.log" 2>&1 \
  || { tail -n 25 "$R/lez/crown-b-operator.log"; die "crown-driver crown-b --operator-only"; }
jq '{blocks, configId, vault, steps: [.steps[] | {step, status, block, tx_hash: (.tx_hash // .tx)}]}' "$W/session-b.json" > "$R/lez/operator-steps.json" 2>/dev/null \
  || cp "$W/session-b.json" "$R/lez/operator-steps.json"
n_applied="$(jq '[.steps[] | select(.status == "applied")] | length' "$W/session-b.json")"
n_steps="$(jq '.steps | length' "$W/session-b.json")"
[ "$n_steps" -ge 4 ] && [ "$n_applied" = "$n_steps" ] || die "the operator's steps: $n_applied of $n_steps applied"
ok "the stand-in committee finalized the fork's blocks $(jq -r .block "$W/capture-1/fixture.json") and $(jq -r .block "$W/capture-2/fixture.json"); $(jq -r '[.steps[].step] | join(", ")' "$W/session-b.json"): $n_applied of $n_steps applied"

# ---- fees, then the claim ------------------------------------------------------------------------------------------
begin "fees"
"$B/seqnet-fund" --sequencer "$SEQ" --seed "$U/seed.txt" --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" \
  --faucet-home "$W/faucet" --public-only > "$W/fund.out" 2> "$W/fund.err" || { tail -n 20 "$W/fund.err"; die "seqnet-fund"; }
grep '^{' "$W/fund.out" > "$R/lez/fund.jsonl"
PUB0="$(jq -r .to_public_0_hex "$R/lez/fund.jsonl" | head -n 1)"
[ -n "$PUB0" ] && [ "$PUB0" != null ] || die "seqnet-fund named no public account"
ok "native LEZ to the user's public /0 ($PUB0): $(jq -r .balance_after "$R/lez/fund.jsonl" | head -n 1)"

F=("${FE[@]}" --sequencer "$SEQ" --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE")
C=(claim --plaintext --i-accept-plaintext-links-deposit --payer "$PUB0" --relayer "$PUB1")
# claim_step LEAF PREFIX: the user's plaintext claim of LEAF, relay fee to the user's own public /1 (a relayer account the
# run can read back); held by its delay, the delay let pass on the fork's clock, a dry run, then sent; prints the LEZ tx
# alone on stdout (its notes go to stderr: the caller captures stdout)
claim_step() {
  if u "$2-held" "${F[@]}" "${C[@]}" --leaf "$1"; then
    say "the claim of leaf $1 was not held by its delay" >&2
  else
    local until now n
    until="$(grep -o -E 'from Ethereum block [0-9]+' "$R/user/$2-held.txt" | head -n 1 | awk '{print $NF}')"
    [ -n "$until" ] || die "the held claim names no first block (user/$2-held.txt)"
    now="$(cast block-number --rpc-url "$RPC")"; n=$(( until - now + 1 )); [ "$n" -lt 1 ] && n=1
    cast rpc anvil_mine "$(printf '0x%x' "$n")" --rpc-url "$RPC" > /dev/null || die "anvil_mine"
    say "the claim's delay let pass on the fork's clock: $n blocks" >&2
  fi
  u "$2-dry-run" "${F[@]}" "${C[@]}" --leaf "$1" || die "the claim's dry run"
  u "$2" "${F[@]}" "${C[@]}" --leaf "$1" --send --yes || die "the claim"
  wait_tx "$R/user/$2.txt"
}

begin "claim of leaf 0 (schedule 0)"
CLAIM_TX="$(claim_step 0 08-claim)" || die "the claim of leaf 0 was not applied"
HOLD_CLAIM="$(holding public 09-recover)" || die "the holding after the claim could not be read"
RELAYED_0="$(holding public 09b-recover 1)" || die "the relayer account's holding could not be read"
[ "$HOLD_CLAIM" = "$DENOM" ] || die "the user's public /0 holds $HOLD_CLAIM after the claim, not $DENOM"
[ "$RELAYED_0" = "$RELAY0" ] || die "the relayer account /1 holds $RELAYED_0 after the claim, not the relay fee $RELAY0"
ok "LEZ tx $CLAIM_TX applied: the user's public /0 holds $HOLD_CLAIM (the denomination), the relayer /1 $RELAYED_0 (the relay fee)"

begin "claim of leaf 1 (schedule 1)"
CLAIM2_TX="$(claim_step 1 08c-claim-2)" || die "the claim of leaf 1 was not applied"
HOLD_CLAIM2="$(holding public 09c-recover)" || die "the holding after the second claim could not be read"
RELAYED_1="$(holding public 09d-recover 1)" || die "the relayer account's holding could not be read"
[ "$(( HOLD_CLAIM2 - HOLD_CLAIM ))" = "$DENOM" ] || die "the claim under schedule 1 minted $(( HOLD_CLAIM2 - HOLD_CLAIM )) to /0, not the denomination $DENOM (as under schedule 0)"
[ "$(( RELAYED_1 - RELAYED_0 ))" = "$RELAY0" ] || die "the claim under schedule 1 paid the relayer $(( RELAYED_1 - RELAYED_0 )), not the relay fee $RELAY0 (as under schedule 0)"
for f in 08-claim 08c-claim-2; do
  if grep -q -i -E 'schedule|range' "$R/user/$f.txt"; then say "note: user/$f.txt mentions a schedule or range: $(grep -i -m1 -E 'schedule|range' "$R/user/$f.txt" | cut -c1-160)"; fi
done
ok "LEZ tx $CLAIM2_TX applied: /0 +$(( HOLD_CLAIM2 - HOLD_CLAIM )) (the denomination), the relayer /1 +$(( RELAYED_1 - RELAYED_0 )) (the relay fee): the same amounts as the claim under schedule 0"

# collect_fees NAME STAGE CAPTURES...: the bridge program's CollectFees of ETH under capture STAGE's record (crown-driver
# collect-fees: the vault's lezFees slot proven from that capture). Anyone may send it; on this stand-in chain the
# operator's tool sends it, since a record's block hash here is the stand-in's (anvil's fork mode writes a zero state
# root, which the capture replaces), which no Ethereum endpoint serves to `ferrata-relayer collect-fees`. Its JSON in
# lez/NAME.json; prints the event
collect_fees() {
  local name="$1" stage="$2"; shift 2; local caps=()
  for c in "$@"; do caps+=(--capture "$W/$c"); done
  "$B/crown-driver" collect-fees --chain stand-in "${D[@]}" --identity "$SA" "${caps[@]}" --stage "$stage" \
    --out "$R/lez/$name.json" > "$R/lez/$name.log" 2>&1 || { tail -n 20 "$R/lez/$name.log"; die "crown-driver collect-fees"; }
  jq -r .event "$R/lez/$name.json"
}
last() { jq -r --arg k "$2" '.[$k] // empty | tostring' "$R/lez/$1.json"; }

begin "mint fees collected"
[ "$(collect_fees 17-collect-mint 2 capture-1 capture-2)" = lez-fees-collected ] || die "the collection was not applied ($(cut -c1-300 "$R/lez/17-collect-mint.json"))"
[ "$(last 17-collect-mint vaultTotal)" = "$LF_2" ] || die "the collection read the vault's total $(last 17-collect-mint vaultTotal), not lezFees at capture 2 ($LF_2)"
[ "$(last 17-collect-mint amount)" = "$M1" ] && [ "$(last 17-collect-mint collectedBefore)" = 0 ] || die "the collection minted $(last 17-collect-mint amount) after $(last 17-collect-mint collectedBefore), not the mint fee $M1 after 0"
FR_LEZ="$(last 17-collect-mint feeRecipient)"; FR_1="$(last 17-collect-mint feeRecipientHoldingAfter)"
[ "$(last 17-collect-mint feeRecipientHoldingBefore)" = 0 ] && [ "$FR_1" = "$M1" ] || die "the fee recipient's LEZ holding went $(last 17-collect-mint feeRecipientHoldingBefore) -> $FR_1, not 0 -> $M1"
COLLECT1_TX="$(last 17-collect-mint tx)"
[ "$(last 17-collect-mint applied)" = true ] && [ "$(last 17-collect-mint collectedAfter)" = "$M1" ] || die "fees(cid, ETH) does not count $M1 after the collection"
ok "LEZ tx $COLLECT1_TX (CollectFees) applied under header(id, $(last 17-collect-mint recordSlot)) of block $(last 17-collect-mint block): the vault's lezFees $LF_2 attested, $M1 minted to the fee recipient $FR_LEZ on LEZ (holding 0 -> $FR_1; fees(cid, ETH) counts $M1)"

begin "nothing more to collect"
[ "$(collect_fees 18-collect-again 2 capture-1 capture-2)" = lez-fee-collection-refused ] || die "a second collection under the same record was applied ($(cut -c1-300 "$R/lez/18-collect-again.json"))"
[ "$(last 18-collect-again exit)" = 244 ] && [ "$(last 18-collect-again amount)" = 0 ] || die "the second collection was refused with $(last 18-collect-again exit), minting $(last 18-collect-again amount)"
ok "the same collection again: refused by the program with 244 (ALREADY_WRITTEN; LEZ tx $(last 18-collect-again tx)), nothing minted, fees(cid, ETH) still $(last 18-collect-again collectedAfter)"

begin "claim again refused"
if u 10-claim-again "${F[@]}" "${C[@]}" --leaf 0 --send --yes; then die "a second claim of leaf 0 was not refused"; fi
grep -q -i -E 'claimed|nullifier|spent' "$R/user/10-claim-again.txt" || die "the second claim failed for another reason (user/10-claim-again.txt)"
HOLD_AGAIN="$(holding public 11-recover)" || die "the holding after the refused claim could not be read"
[ "$HOLD_AGAIN" = "$HOLD_CLAIM2" ] || die "the holding changed after the refused claim: $HOLD_AGAIN (was $HOLD_CLAIM2)"
ok "refused: $(grep -i -m1 -E 'claimed|nullifier|spent' "$R/user/10-claim-again.txt" | cut -c1-160); the holding unchanged"

# ---- the burn ------------------------------------------------------------------------------------------------------
begin "burn"
BR=(burn --from public --i-accept-public-account --account 0 --amount 0.001 --recipient "$FRESH" --payer "$PUB0")
u 12-burn-dry-run "${F[@]}" "${BR[@]}" || die "the burn's dry run"
u 13-burn "${F[@]}" "${BR[@]}" --send --yes || die "the burn"
BURN_TX="$(wait_tx "$R/user/13-burn.txt")" || die "the burn was not applied"
HOLD_BURN="$(holding public 14-recover)" || die "the holding after the burn could not be read"
[ "$(( HOLD_CLAIM2 - HOLD_BURN ))" = "$DENOM" ] || die "the user's public /0 holds $HOLD_BURN after burning one denomination of $HOLD_CLAIM2"
ok "LEZ tx $BURN_TX applied: 0.001 ETH burnt to $FRESH; the holding is $HOLD_BURN (the other claim's denomination)"

# ---- the checkpoint ------------------------------------------------------------------------------------------------
begin "checkpoint journal"
"$B/seqnet-txs" --sequencer "$SEQ" --home "$W/txs-wallet-all" --bridge-account "$BRIDGE" --attest-account "$ATTEST" \
  --txs-json "$W/txs.json" > "$R/lez/transactions.tsv" 2> "$W/txs.err" || { tail -n 5 "$W/txs.err"; die "seqnet-txs"; }
mkdir -p "$W/checkpoint"
"$B/ci_zone" checkpoint --identity "$W/ci-identity.json" --txs "$W/txs.json" --out-dir "$W/checkpoint" > "$W/ci-zone.log" 2>&1 \
  || { tail -n 10 "$W/ci-zone.log"; die "ci_zone checkpoint"; }
cp "$W/checkpoint/cp-ci-b.json" "$R/checkpoint/"
# ferrata-e2e (bridge/) is on risc0-zkvm 3.1.0, whose executor is r0vm 3.1.0 (scripts/ci/install-risc0.sh puts it in
# ~/.risc0); LEZ's crates, the sequencer and the `ferrata` tool are on 3.0.5 (RISC0_HOME, the LEZ home).
R0VM31="${CI_RT_R0VM31:-}"
[ -n "$R0VM31" ] && [ -x "$R0VM31" ] || die "no r0vm 3.1.0 (CI_RT_R0VM31; scripts/ci/install-risc0.sh)"
env -u RISC0_HOME RISC0_SERVER_PATH="$R0VM31" "$B/ferrata-e2e" exec --program "$B/checkpoint_b.bin" \
  --input "$W/checkpoint/cp-ci-b.input" --meta "$W/checkpoint/cp-ci-b.json" \
  > "$R/checkpoint/exec.txt" 2>&1 || { tail -n 15 "$R/checkpoint/exec.txt"; die "the rule (B) guest's execution"; }
CP="$W/checkpoint/cp-ci-b.json"
[ "$(jq -r .burnCount "$CP")" = 1 ] || die "the checkpoint counts $(jq -r .burnCount "$CP") burns, not 1"
[ "$(jq -r .parentState "$CP" | lower)" = "$(echo "$GS" | lower)" ] || die "the checkpoint does not start at the vault's genesis state"
ok "$(jq '[.blocks[] | length] | add' "$W/txs.json") applied zone transactions in $(jq '.blocks | length' "$W/txs.json") blocks; the guest's journal equals the native one ($(tail -n 1 "$R/checkpoint/exec.txt" | cut -c1-120)); 1 burn, root $(jq -r .burnRoot "$CP")"

begin "checkpoint accepted"
journal="$(jq -r .journal "$CP")"; journal="0x${journal#0x}"
dig="0x$(python3 -I -c 'import hashlib, sys; print(hashlib.sha256(bytes.fromhex(sys.argv[1])).hexdigest())' "${journal#0x}")"
cast send "$MOCK" "accept(bytes32,bytes32)" "$CPB_ID" "$dig" --rpc-url "$RPC" --unlocked --from "$DEPLOYER" > "$W/accept-cp.txt" 2>&1 \
  || { cat "$W/accept-cp.txt"; die "the mock verifier's accept (checkpoint)"; }
mkdir -p "$W/relayer-inbox"
jq -n --arg j "$journal" --arg s "$SEAL" '{kind: "checkpoint", journal: $j, seal: $s}' > "$W/relayer-inbox/checkpoint.json"
relayer_round
[ "$(call 'finalState()(bytes32)' | lower)" = "$(echo "$GS" | lower)" ] || die "the candidate was finalized before its wait"
pass_time 3601 "the candidate's wait (Delta 3600 s)"
relayer_round
FINAL="$(call 'finalState()(bytes32)')"
[ "$(echo "$FINAL" | lower)" = "$(jq -r .newState "$CP" | lower)" ] || die "the vault's final state $FINAL is not the checkpoint's"
ACCEPTED="$(call 'burnRootStatus(bytes32)(bool,uint256)' "$(jq -r .burnRoot "$CP")" | head -n 1)"
[ "$ACCEPTED" = true ] || die "the vault has not accepted the checkpoint's burn root"
ok "submitCandidate, Delta let pass, finalizeAttested by ferrata-relayer; finalState $FINAL; burn root accepted"

# ---- the release ---------------------------------------------------------------------------------------------------
begin "release journal"
printf '%s\n' "$SEAL" > "$W/seal.hex"
if ! u 15-release-held "${F[@]}" release --seal "$W/seal.hex"; then say "the release was held (its delay after the checkpoint)"; fi
pass_time 121 "the release delay (120 s after finalizeAttested)"
u 16-release env RISC0_DEV_MODE=1 RISC0_HOME="$CI_RT_LEZ_HOME" "${F[@]}" --json release --program "$B/release.bin" --prove \
  --seal "$W/seal.hex" --job-out "$W/release-job.json" || die "the release (user/16-release.txt)"
grep -q 'journal equal to the native one' "$R/user/16-release.txt" || die "the release guest's journal was not checked against the native one"
CALLDATA="$(grep -o -E 'data 0x[0-9a-f]+' "$R/user/16-release.txt" | tail -n 1 | awk '{print $2}')"
ok "$(json_of "$R/user/16-release.txt" | jq -r '.report[] | select(test("journal equal to the native one"))' | head -n 1 | sed 's/^ *//' | cut -c1-160)"

begin "release paid"
rj="$(jq -r .journal "$W/release-job.json")"; rj="0x${rj#0x}"
rdig="0x$(python3 -I -c 'import hashlib, sys; print(hashlib.sha256(bytes.fromhex(sys.argv[1])).hexdigest())' "${rj#0x}")"
cast send "$MOCK" "accept(bytes32,bytes32)" "$REL_ID" "$rdig" --rpc-url "$RPC" --unlocked --from "$DEPLOYER" > "$W/accept-release.txt" 2>&1 \
  || { cat "$W/accept-release.txt"; die "the mock verifier's accept (release)"; }
cp "$W/release-job.json" "$W/relayer-inbox/release.json"
ESC_REL0="$(escrow)"
relayer_round
topic="$(cast keccak 'Release(bytes32,address,address,uint256,address,uint256,bytes32)')"
ev="$(cast logs --rpc-url "$RPC" --address "$VAULT" --from-block "$FROM" "$topic" --json)"
[ "$(echo "$ev" | jq length)" = 1 ] || die "the vault emitted $(echo "$ev" | jq length) Release events, not 1"
RELEASE_TX="$(echo "$ev" | jq -r '.[0].transactionHash')"
words="$(echo "$ev" | jq -r '.[0].data' | sed 's/^0x//')"
AMOUNT="$(python3 -I -c 'import sys; d = sys.argv[1]; print(int(d[0:64], 16))' "$words")"
RELAY_FEE="$(python3 -I -c 'import sys; d = sys.argv[1]; print(int(d[128:192], 16))' "$words")"
FRESH_BAL="$(cast balance "$FRESH" --rpc-url "$RPC")"
# The event's amount is the burn's whole denomination; the vault pays the recipient that less the relay fee, the
# checkpoint fee it credits to the checkpoint's prover (OwedCredited) and the burn fee it keeps in escrow for the fee
# recipient on LEZ (LezFeesAccrued; ADR 0048).
[ "$AMOUNT" = "$DENOM" ] || die "the Release event's amount $AMOUNT is not the burnt $DENOM"
CREDITED="$(log_sum "$RELEASE_TX" 'OwedCredited(address,address,uint256)')"
ACCRUED_R="$(log_sum "$RELEASE_TX" 'LezFeesAccrued(address,uint256,uint256)')"
LF_R="$(lez_fees)"; ESC_R="$(escrow)"; OWED_R="$(owed_fr)"
[ "$CREDITED" = "$CP1" ] || die "the release credited $CREDITED on Ethereum, not the checkpoint fee $CP1 alone"
[ "$ACCRUED_R" = "$B1" ] && [ "$(( LF_R - LF_2 ))" = "$B1" ] || die "the release accrued $ACCRUED_R (lezFees +$(( LF_R - LF_2 ))), not the burn fee $B1"
[ "$(( ESC_REL0 - ESC_R ))" = "$(( DENOM - B1 ))" ] || die "the escrow fell by $(( ESC_REL0 - ESC_R )) at the release, not the denomination less the burn fee ($(( DENOM - B1 )))"
[ "$OWED_R" = "$OWED_2" ] || die "the Ethereum fee recipient was credited $(( OWED_R - OWED_2 )) at the release (ADR 0048: nothing)"
[ "$(( FRESH_BAL + RELAY_FEE + CREDITED + B1 ))" = "$AMOUNT" ] || die "the fresh address holds $FRESH_BAL wei; with the relay fee $RELAY_FEE, the checkpoint fee $CREDITED and the burn fee $B1 that is not the Release event's $AMOUNT"
[ "$FRESH_BAL" -gt 0 ] || die "the fresh address received nothing"
ok "Ethereum tx $RELEASE_TX: the Release event pays out the burnt $AMOUNT wei: $FRESH_BAL to the fresh address, $RELAY_FEE relay fee to the relayer, $CREDITED credited (OwedCredited: the checkpoint fee), $B1 kept in escrow (LezFeesAccrued: the burn fee; lezFees $LF_2 -> $LF_R)"

begin "record after the release"
capture capture-3
"$B/crown-driver" crown-b --chain stand-in "${D[@]}" --identity "$SA" --capture "$W/capture-1" --capture "$W/capture-2" \
  --capture "$W/capture-3" --skip-stages 2 --updates-only yes --private fake --operator-only yes \
  --session-out "$W/session-b3.json" --notes-out "$W/notes-3.json" > "$R/lez/crown-b-record-3.log" 2>&1 \
  || { tail -n 25 "$R/lez/crown-b-record-3.log"; die "crown-driver crown-b --skip-stages 2 --updates-only yes"; }
grep -q 'recorded (--updates-only' "$R/lez/crown-b-record-3.log" || die "crown-b recorded no header for capture 3"
ok "the stand-in committee finalized the fork's block $(jq -r .block "$W/capture-3/fixture.json") (after the release): $(grep -o 'header(id, [0-9]*) of block [0-9]*' "$R/lez/crown-b-record-3.log" | tail -n 1)"

begin "burn fee collected"
[ "$(collect_fees 19-collect-burn 3 capture-1 capture-2 capture-3)" = lez-fees-collected ] || die "the collection after the release was not applied ($(cut -c1-300 "$R/lez/19-collect-burn.json"))"
[ "$(last 19-collect-burn vaultTotal)" = "$LF_R" ] || die "the collection read the vault's total $(last 19-collect-burn vaultTotal), not lezFees after the release ($LF_R)"
[ "$(last 19-collect-burn amount)" = "$B1" ] && [ "$(last 19-collect-burn collectedBefore)" = "$M1" ] || die "the collection minted $(last 19-collect-burn amount) after $(last 19-collect-burn collectedBefore), not the burn fee $B1 after $M1"
FR_2="$(last 19-collect-burn feeRecipientHoldingAfter)"
[ "$(last 19-collect-burn feeRecipientHoldingBefore)" = "$FR_1" ] && [ "$FR_2" = "$(( M1 + B1 ))" ] || die "the fee recipient's LEZ holding went $(last 19-collect-burn feeRecipientHoldingBefore) -> $FR_2, not $FR_1 -> $(( M1 + B1 ))"
COLLECT2_TX="$(last 19-collect-burn tx)"
[ "$(last 19-collect-burn collectedAfter)" = "$(( M1 + B1 ))" ] || die "fees(cid, ETH) does not count $(( M1 + B1 )) after the second collection"
ok "LEZ tx $COLLECT2_TX (CollectFees) applied under header(id, $(last 19-collect-burn recordSlot)) of block $(last 19-collect-burn block): lezFees $LF_R attested, $B1 (the release's burn fee) minted to the fee recipient (holding $FR_1 -> $FR_2; fees(cid, ETH) counts $(( M1 + B1 )))"

begin "older record refused"
[ "$(collect_fees 19b-collect-older 2 capture-1 capture-2 capture-3)" = lez-fee-collection-refused ] || die "a collection under the older record was applied ($(cut -c1-300 "$R/lez/19b-collect-older.json"))"
[ "$(last 19b-collect-older exit)" = 247 ] && [ "$(last 19b-collect-older amount)" = 0 ] || die "the older record's collection was refused with $(last 19b-collect-older exit), minting $(last 19b-collect-older amount)"
ok "a collection under capture 2's record (lezFees $LF_2, below the $(( M1 + B1 )) minted): refused by the program with 247 (IMPORT_MISMATCH; LEZ tx $(last 19b-collect-older tx)), nothing minted"

begin "escrow backs LEZ"
HOLD_END="$(holding public 20-recover)" || die "the user's holding could not be read"
RELAYED_END="$(holding public 20b-recover 1)" || die "the relayer account's holding could not be read"
ESC_END="$(escrow)"
LEZ_SUPPLY=$(( HOLD_END + RELAYED_END + FR_2 ))
[ "$ESC_END" = "$LEZ_SUPPLY" ] || die "the vault escrows $ESC_END wei of ETH, the wrapped ETH on LEZ is $LEZ_SUPPLY (/0 $HOLD_END, /1 $RELAYED_END, the fee recipient $FR_2)"
ok "every deposit claimed, the burn released, every fee collected: the vault's ETH escrow $ESC_END wei = the wrapped ETH on LEZ $LEZ_SUPPLY (the user's /0 $HOLD_END, /1 $RELAYED_END, the fee recipient $FR_2)"

begin "release again refused"
[ -n "$CALLDATA" ] || die "the release's calldata was not printed (user/16-release.txt)"
REPLAY_EXIT=0
cast call "$VAULT" "$CALLDATA" --from "$RELAYER" --rpc-url "$RPC" > "$R/eth/replay.txt" 2>&1 || REPLAY_EXIT=$?
[ "$REPLAY_EXIT" != 0 ] || die "the vault accepts the same release again"
want_sel="$(cast sig 'NullifierAlreadyUsed()')"
REPLAY_SEL="$(grep -o -i -- "$want_sel" "$R/eth/replay.txt" | head -n 1 | lower)"
[ -n "$REPLAY_SEL" ] || die "the same release reverted, but not with NullifierAlreadyUsed ($want_sel): $(head -c 300 "$R/eth/replay.txt")"
NULLIFIER="$(json_of "$R/user/16-release.txt" | jq -r .result.nullifier 2>/dev/null)"
if [ -z "$NULLIFIER" ] || [ "$NULLIFIER" = null ]; then NULLIFIER="$(grep -o -E 'nullifier[^0-9a-fx]*0x[0-9a-f]{64}' "$R/user/16-release.txt" | grep -o -E '0x[0-9a-f]{64}' | head -n 1)"; fi
USED="$([ -n "$NULLIFIER" ] && call 'nullifierUsed(bytes32)(bool)' "$NULLIFIER" || echo "?")"
[ "$USED" = true ] || die "the release's nullifier is not marked used ($NULLIFIER: $USED)"
ok "eth_call of the same release reverts with NullifierAlreadyUsed ($want_sel); nullifierUsed($NULLIFIER) = true"

# ---- the records ---------------------------------------------------------------------------------------------------
"$B/crown-driver" export --chain stand-in "${D[@]}" --out "$R/lez/export" > "$W/export.log" 2>&1 || say "crown-driver export failed: $(tail -n 3 "$W/export.log")"
rm -f "$R/lez/export/blocks.borsh" "$R/lez/export/segments.tsv"
python3 -I "$REPO/e2e/seqnet/ethtxs.py" --rpc "$RPC" --vault-out "$REPO/vault/out" \
  --names "deployer=$DEPLOYER,relayer=$RELAYER,user=$DEPOSITOR,vault=$VAULT,mock verifier=$MOCK" \
  > "$R/eth/transactions.tsv" 2> "$W/ethtxs.err" || say "ethtxs.py failed: $(tail -n 3 "$W/ethtxs.err")"
jq -n --arg fb "$FORK_BLOCK" --arg fh "$FORK_HASH" --arg ft "$(date -u -d "@$FORK_TIME" +%FT%TZ)" --arg v "$VAULT" --arg c "$CID" \
  --arg fa "$FORK_AGE" --arg frpc "$FORK_RPC" --arg rx "$REPLAY_EXIT" --arg rs "$REPLAY_SEL" --arg nu "$USED" \
  --arg dtx "$DEPOSIT_TX" --arg den "$DENOM" --arg ctx "$CLAIM_TX" --arg hc "$HOLD_CLAIM" --arg btx "$BURN_TX" --arg hb "$HOLD_BURN" \
  --arg fr "$FRESH" --arg br "$(jq -r .burnRoot "$CP")" --arg bc "$(jq -r .burnCount "$CP")" --arg ns "$(jq -r .newState "$CP")" \
  --arg tip "$(jq -r .tipId "$CP")" --arg vf "$FINAL" --arg acc "$ACCEPTED" --arg rtx "$RELEASE_TX" --arg am "$AMOUNT" \
  --arg rf "$RELAY_FEE" --arg cr "$CREDITED" --arg fb2 "$FRESH_BAL" --arg nul "$NULLIFIER" --arg ss "$STAND_IN_SLOT" \
  --arg d2 "$DEPOSIT2_TX" --arg c2 "$CLAIM2_TX" --arg hc2 "$HOLD_CLAIM2" --arg r0 "$RELAYED_0" --arg r1 "$RELAYED_1" \
  --arg rl "$RELAY0" --arg m1 "$M1" --arg o0 "$OWED_0" --arg o1 "$OWED_1" --arg o2 "$OWED_2" --arg e0 "$ESC_0" --arg e1 "$ESC_1" \
  --arg e2 "$ESC_2" --arg fr2 "$FEE_RECIPIENT" --arg ce "$CREDIT_EV" --arg br2 "$BRIDGE_ROW" --arg bid "$(jq -r .bridge.imageId "$SA")" \
  --arg lf0 "$LF_0" --arg lf1 "$LF_1" --arg lf2 "$LF_2" --arg lfr "$LF_R" --arg ae "$ACCRUED_EV" --arg ar "$ACCRUED_R" --arg b1 "$B1" \
  --arg cp1 "$CP1" --arg frl "$FR_LEZ" --arg fh1 "$FR_1" --arg fh2 "$FR_2" --arg k1 "$COLLECT1_TX" --arg k2 "$COLLECT2_TX" \
  --arg he "$HOLD_END" --arg re "$RELAYED_END" --arg ee "$ESC_END" --arg ls "$LEZ_SUPPLY" --arg er0 "$ESC_REL0" --arg err "$ESC_R" \
  '{fees: {adr: "0048", bridgeProgram: {row: $br2, imageId: $bid}, ethereumFeeRecipient: $fr2, lezFeeRecipient: $frl, relayFeeWei: $rl,
      schedule0: {depositTx: $dtx, mintFeeWei: "0", escrowDeltaWei: (($e1 | tonumber) - ($e0 | tonumber) | tostring),
        lezFeesDeltaWei: (($lf1 | tonumber) - ($lf0 | tonumber) | tostring),
        feeRecipientCreditDeltaWei: (($o1 | tonumber) - ($o0 | tonumber) | tostring),
        claim: {tx: $ctx, mintedToDestinationWei: $hc, paidToRelayerWei: $r0}},
      schedule1: {depositTx: $d2, mintFeeWei: $m1, burnFeeWei: $b1, checkpointFeeWei: $cp1,
        escrowDeltaWei: (($e2 | tonumber) - ($e1 | tonumber) | tostring),
        lezFeesDeltaWei: (($lf2 | tonumber) - ($lf1 | tonumber) | tostring), lezFeesAccruedEventsWei: $ae,
        feeRecipientCreditDeltaWei: (($o2 | tonumber) - ($o1 | tonumber) | tostring), owedCreditedEventsWei: $ce,
        claim: {tx: $c2, mintedToDestinationWei: (($hc2 | tonumber) - ($hc | tonumber) | tostring),
          paidToRelayerWei: (($r1 | tonumber) - ($r0 | tonumber) | tostring)}},
      collections: [{tx: $k1, vaultTotalWei: $lf2, mintedWei: $m1, feeRecipientHoldingAfterWei: $fh1},
                    {tx: $k2, vaultTotalWei: $lfr, mintedWei: $b1, feeRecipientHoldingAfterWei: $fh2}],
      release: {escrowBeforeWei: $er0, escrowAfterWei: $err, burnFeeAccruedEventsWei: $ar, lezFeesAfterWei: $lfr},
      conservation: {escrowWei: $ee, lezSupplyWei: $ls, user0Wei: $he, user1Wei: $re, feeRecipientWei: $fh2}},
    fork: {block: ($fb | tonumber), hash: $fh, time: $ft, ageS: ($fa | tonumber), maxAgeS: 3600, rpc: $frpc},
    standInSlot: ($ss | tonumber), vault: $v, configId: $c,
    deposit: {tx: $dtx, denominationWei: $den}, claim: {tx: $ctx, holdingAfterClaim: $hc},
    burn: {tx: $btx, recipient: $fr, holdingAfterBurn: $hb},
    checkpoint: {burnRoot: $br, burnCount: $bc, newState: $ns, tipId: $tip, vaultFinalState: $vf, burnRootAccepted: ($acc == "true")},
    release: {tx: $rtx, amountWei: $am, relayFeeWei: $rf, creditedFeesWei: $cr, recipientBalanceWei: $fb2, nullifier: $nul,
      replay: {callExit: ($rx | tonumber), revertSelector: $rs, nullifierUsed: ($nu == "true")},
      replayRefused: (($rx | tonumber) != 0 and $rs == "0xcad2ae02" and $nu == "true")}}' > "$R/facts.json"
say "the round trip: every step ok ($(wc -l < "$STEPS") steps)"
