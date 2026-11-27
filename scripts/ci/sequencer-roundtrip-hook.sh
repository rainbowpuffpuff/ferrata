#!/usr/bin/env bash
# The round trip of scripts/ci/sequencer-roundtrip.sh (RFP-022 Reliability #10), run by lez/live/run.sh's phase `hook`
# while LEZ v0.4.0-rc1's standalone sequencer runs on 127.0.0.1 (ZONE_RPC; its records in ZONE_OUT). Every step is a
# line of steps.tsv (name, ok or FAILED, seconds, what it showed); the first failure stops the run. Steps:
#   fork            anvil forks Sepolia at Sepolia's finalized block of the moment, read from the first public Sepolia
#                   RPC of CI_RT_SEPOLIA_RPCS that answers with a block under an hour old by this machine's clock
#                   (three attempts each, 30 s apart); the fork's copy of that block has Sepolia's hash
#   zone: programs and the stand-in instance
#                   crown-driver crown-a: the attestation program and the bridge program deployed through LEZ's
#                   program loader (their image ids those of docs/image-ids.md), the stand-in light-client instance
#                   initialised (a committee this run holds: anvil has no beacon chain)
#   vault deployed on the fork
#                   FerrataVault v5 under rule (B) by vault/script/deploy.sh (e2e/crown/sepolia.sh deploy: RISC Zero's
#                   Groth16 verifier 3.1.0, the emergency stop, the library, the vault) with the configuration's
#                   configId (crown-driver config-id) and the genesis checkpoint state of ci_zone's test committee
#                   (ci_zone genesis; the same configId); then, STAND-IN for the Groth16 proofs, the verifier's code
#                   replaced on the fork (anvil_setCode, as vault/script/fork-rehearsal-31.sh) by the vault tests'
#                   MockRiscZeroVerifier, which accepts exactly the (image id, journal digest) pairs it is told to
#   deposit         `ferrata deposit`: one denomination (0.001 ETH) to the user's public account /0
#   operator import the vault captured at the deposit's block; crown-driver crown-b --operator-only: the stand-in
#                   committee finalizes that block, the bridge's Init (pairing: configId in the vault's slot 2),
#                   Register ETH, ImportRoot n=1, each read from the vault's storage through the attestation program
#   fees            seqnet-fund --public-only: native LEZ for the user's fees (the gas drop's stand-in)
#   claim (plaintext)
#                   `ferrata claim --plaintext`: held by its delay, the delay let pass on the fork's clock, then sent;
#                   the user's public /0 holds the denomination
#   claim again refused
#                   the same claim again: refused (the nullifier is spent; the zone's state read back)
#   burn            `ferrata burn --from public`: the denomination to a fresh Ethereum address; the holding is 0
#   checkpoint journal
#                   ci_zone over every transaction the sequencer applied; the rule (B) guest executed (RISC Zero
#                   executor) on that input commits the journal ci_zone computed natively, with one burn
#   checkpoint accepted
#                   the mock verifier told to accept exactly that journal; ferrata-relayer submits the candidate, the
#                   fork's clock passes the wait (Delta), the relayer finalizes it; the vault's final state is the
#                   journal's, its burn root accepted
#   release journal `ferrata release --prove` executes the release guest in dev mode: its journal is the native one
#   release paid    the mock accepts exactly that journal; ferrata-relayer sends the release; the fresh address holds
#                   the Release event's amount, and amount + relay fee = the denomination
#   release again refused
#                   the same release call again: the vault refuses it (eth_call reverts with NullifierAlreadyUsed, its
#                   output kept in eth/replay.txt; nullifierUsed true)
# Records in CI_RT_OUT: steps.tsv, facts.json, identity.json, params.json, deploy.json, user/ (each command of the user
# and what it printed), lez/ (session A's identity, the operator's steps, every zone transaction), eth/ (the fork's
# transactions), checkpoint/ (ci_zone's journal and fields). Nothing is sent to Sepolia: anvil keeps every transaction.
# A failed step stops the run: `die` in a command substitution ends only that subshell, so every assignment from a
# function that can fail is followed by its own `|| die`.
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
# holding KIND: the amount the user's account /0 of KIND holds, from `ferrata recover` (0 without a holding)
holding() {
  u "$2" "${F[@]}" --json recover || die "recover"
  json_of "$R/user/$2.txt" | jq -r --arg k "$1" '[.result.holdings[] | select(.kind == $k and .account == 0) | .amount][0] // "0"'
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
# FERRATA_CI_BRIDGE_ROW: the docs/image-ids.md row the deployed bridge program must be (CI: this tree's; a rehearsal of
# the test network's program sets ferrata_bridge_b1 and puts that binary in place: e2e/rc3-roundtrip/run.sh).
BRIDGE_ROW="${FERRATA_CI_BRIDGE_ROW:-ferrata_bridge_f13}"
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

# ---- the user's deposit --------------------------------------------------------------------------------------------
begin "deposit"
U="$W/user"; mkdir -p "$U"
printf '%s\n' "$PHRASE" > "$U/seed.txt"; printf 'ci-user\n' > "$U/lez.pass"; printf 'ci-depositor\n' > "$U/eth.pass"
chmod 600 "$U/seed.txt" "$U/lez.pass" "$U/eth.pass"
cast wallet import depositor --private-key "$(cast wallet private-key --mnemonic "$ANVIL_MNEMONIC" --mnemonic-index 2)" \
  --keystore-dir "$U/keystore" --unsafe-password ci-depositor > /dev/null || die "the depositor's keystore"
[ "$(cast wallet address --keystore "$U/keystore/depositor" --password ci-depositor)" = "$DEPOSITOR" ] || die "the depositor's keystore is not anvil's account 2"
FE=("$B/ferrata" --seed "$U/seed.txt" --eth-rpc "$RPC" --chain-id "$CHAIN" --vault "$VAULT" --from-block "$FROM")
u 01-keys "${FE[@]}" keys --accounts 2 || die "keys"
u 02-status "${FE[@]}" status || die "status"
u 03-deposit-dry-run "${FE[@]}" deposit --denomination 0 --kind public --i-accept-public-account --from "$DEPOSITOR" || die "the deposit's dry run"
u 04-deposit "${FE[@]}" deposit --denomination 0 --kind public --i-accept-public-account --keystore "$U/keystore/depositor" \
  --password-file "$U/eth.pass" --send --yes || die "the deposit"
DEPOSIT_TX="$(grep -o -E 'sent: 0x[0-9a-fA-F]{64}' "$R/user/04-deposit.txt" | tail -n 1 | awk '{print $2}')"
[ -n "$DEPOSIT_TX" ] || die "the deposit printed no transaction (user/04-deposit.txt)"
[ "$(call 'nextLeafIndex()(uint64)')" = 1 ] || die "the vault holds $(call 'nextLeafIndex()(uint64)') deposits, not 1"
u 05-scan "${FE[@]}" scan || die "scan"
DENOM="$(python3 -I -c 'print(10**15)')"
ok "0.001 ETH to the user's public /0, Ethereum tx $DEPOSIT_TX; the vault's tree holds 1 leaf"

# ---- the operator's import -----------------------------------------------------------------------------------------
begin "operator import"
slots="$("$B/crown-driver" capture-slots | jq -r '.slots[].slot')"
sargs=(); for s in $slots; do sargs+=(--slot "$s"); done
python3 -I "$REPO/lez/crown/capture-anvil.py" --rpc "$RPC" --vault "$VAULT" --from-block "$FROM" "${sargs[@]}" "$W/capture-1" \
  > "$W/capture-1.log" 2>&1 || { tail -n 20 "$W/capture-1.log"; die "the capture of the vault"; }
"$B/crown-driver" check --chain stand-in --program-bin "$B/ferrata_lez.bin" --bridge-bin "$B/ferrata_bridge.bin" \
  --capture "$W/capture-1" --identity "$SA" --operator-only yes > "$W/check.log" 2>&1 || { tail -n 20 "$W/check.log"; die "crown-driver check"; }
"$B/crown-driver" crown-b --chain stand-in "${D[@]}" --identity "$SA" --capture "$W/capture-1" --private fake --operator-only yes \
  --session-out "$W/session-b.json" --notes-out "$W/notes.json" > "$R/lez/crown-b-operator.log" 2>&1 \
  || { tail -n 25 "$R/lez/crown-b-operator.log"; die "crown-driver crown-b --operator-only"; }
jq '{blocks, configId, vault, steps: [.steps[] | {step, status, block, tx_hash: (.tx_hash // .tx)}]}' "$W/session-b.json" > "$R/lez/operator-steps.json" 2>/dev/null \
  || cp "$W/session-b.json" "$R/lez/operator-steps.json"
n_applied="$(jq '[.steps[] | select(.status == "applied")] | length' "$W/session-b.json")"
n_steps="$(jq '.steps | length' "$W/session-b.json")"
[ "$n_steps" -ge 3 ] && [ "$n_applied" = "$n_steps" ] || die "the operator's steps: $n_applied of $n_steps applied"
ok "the stand-in committee finalized the fork's block $(jq -r .block "$W/capture-1/fixture.json"); $(jq -r '[.steps[].step] | join(", ")' "$W/session-b.json"): $n_applied of $n_steps applied"

# ---- fees, then the claim ------------------------------------------------------------------------------------------
begin "fees"
"$B/seqnet-fund" --sequencer "$SEQ" --seed "$U/seed.txt" --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" \
  --faucet-home "$W/faucet" --public-only > "$W/fund.out" 2> "$W/fund.err" || { tail -n 20 "$W/fund.err"; die "seqnet-fund"; }
grep '^{' "$W/fund.out" > "$R/lez/fund.jsonl"
PUB0="$(jq -r .to_public_0_hex "$R/lez/fund.jsonl" | head -n 1)"
[ -n "$PUB0" ] && [ "$PUB0" != null ] || die "seqnet-fund named no public account"
ok "native LEZ to the user's public /0 ($PUB0): $(jq -r .balance_after "$R/lez/fund.jsonl" | head -n 1)"

begin "claim (plaintext)"
F=("${FE[@]}" --sequencer "$SEQ" --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE")
C=(claim --plaintext --i-accept-plaintext-links-deposit --payer "$PUB0")
if u 06-claim-held "${F[@]}" "${C[@]}"; then
  say "the claim was not held by its delay"
else
  until="$(grep -o -E 'from Ethereum block [0-9]+' "$R/user/06-claim-held.txt" | head -n 1 | awk '{print $NF}')"
  [ -n "$until" ] || die "the held claim names no first block (user/06-claim-held.txt)"
  now="$(cast block-number --rpc-url "$RPC")"; n=$(( until - now + 1 )); [ "$n" -lt 1 ] && n=1
  cast rpc anvil_mine "$(printf '0x%x' "$n")" --rpc-url "$RPC" > /dev/null || die "anvil_mine"
  say "the claim's delay let pass on the fork's clock: $n blocks"
fi
u 07-claim-dry-run "${F[@]}" "${C[@]}" || die "the claim's dry run"
u 08-claim "${F[@]}" "${C[@]}" --send --yes || die "the claim"
CLAIM_TX="$(wait_tx "$R/user/08-claim.txt")" || die "the claim was not applied"
HOLD_CLAIM="$(holding public 09-recover)" || die "the holding after the claim could not be read"
[ "$HOLD_CLAIM" = "$DENOM" ] || die "the user's public /0 holds $HOLD_CLAIM after the claim, not $DENOM"
ok "LEZ tx $CLAIM_TX applied; the user's public /0 holds $HOLD_CLAIM (0.001 ETH)"

begin "claim again refused"
if u 10-claim-again "${F[@]}" "${C[@]}" --leaf 0 --send --yes; then die "a second claim of leaf 0 was not refused"; fi
grep -q -i -E 'claimed|nullifier|spent' "$R/user/10-claim-again.txt" || die "the second claim failed for another reason (user/10-claim-again.txt)"
HOLD_AGAIN="$(holding public 11-recover)" || die "the holding after the refused claim could not be read"
[ "$HOLD_AGAIN" = "$DENOM" ] || die "the holding changed after the refused claim: $HOLD_AGAIN"
ok "refused: $(grep -i -m1 -E 'claimed|nullifier|spent' "$R/user/10-claim-again.txt" | cut -c1-160); the holding unchanged"

# ---- the burn ------------------------------------------------------------------------------------------------------
begin "burn"
BR=(burn --from public --i-accept-public-account --account 0 --amount 0.001 --recipient "$FRESH" --payer "$PUB0")
u 12-burn-dry-run "${F[@]}" "${BR[@]}" || die "the burn's dry run"
u 13-burn "${F[@]}" "${BR[@]}" --send --yes || die "the burn"
BURN_TX="$(wait_tx "$R/user/13-burn.txt")" || die "the burn was not applied"
HOLD_BURN="$(holding public 14-recover)" || die "the holding after the burn could not be read"
[ "$HOLD_BURN" = 0 ] || die "the user's public /0 holds $HOLD_BURN after burning all of it"
ok "LEZ tx $BURN_TX applied: 0.001 ETH burnt to $FRESH; the holding is 0"

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
relayer_round
topic="$(cast keccak 'Release(bytes32,address,address,uint256,address,uint256,bytes32)')"
ev="$(cast logs --rpc-url "$RPC" --address "$VAULT" --from-block "$FROM" "$topic" --json)"
[ "$(echo "$ev" | jq length)" = 1 ] || die "the vault emitted $(echo "$ev" | jq length) Release events, not 1"
RELEASE_TX="$(echo "$ev" | jq -r '.[0].transactionHash')"
words="$(echo "$ev" | jq -r '.[0].data' | sed 's/^0x//')"
AMOUNT="$(python3 -I -c 'import sys; d = sys.argv[1]; print(int(d[0:64], 16))' "$words")"
RELAY_FEE="$(python3 -I -c 'import sys; d = sys.argv[1]; print(int(d[128:192], 16))' "$words")"
FRESH_BAL="$(cast balance "$FRESH" --rpc-url "$RPC")"
# The event's amount is the burn's whole denomination; the vault pays the recipient that less the relay fee and the
# fees it credits in the same transaction (the denomination's checkpoint fee, to the checkpoint's prover: OwedCredited).
[ "$AMOUNT" = "$DENOM" ] || die "the Release event's amount $AMOUNT is not the burnt $DENOM"
oc="$(cast keccak 'OwedCredited(address,address,uint256)')"
CREDITED="$(cast receipt "$RELEASE_TX" --rpc-url "$RPC" --json | python3 -I -c 'import json, sys; r = json.load(sys.stdin); t = sys.argv[1].lower(); print(sum(int(l["data"], 16) for l in r["logs"] if l["topics"] and l["topics"][0].lower() == t))' "$oc")"
[ "$(( FRESH_BAL + RELAY_FEE + CREDITED ))" = "$AMOUNT" ] || die "the fresh address holds $FRESH_BAL wei; with the relay fee $RELAY_FEE and the credited fees $CREDITED that is not the Release event's $AMOUNT"
[ "$FRESH_BAL" -gt 0 ] || die "the fresh address received nothing"
ok "Ethereum tx $RELEASE_TX: the Release event pays out the burnt $AMOUNT wei: $FRESH_BAL to the fresh address, $RELAY_FEE relay fee to the relayer, $CREDITED credited (OwedCredited: the checkpoint fee)"

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
  '{fork: {block: ($fb | tonumber), hash: $fh, time: $ft, ageS: ($fa | tonumber), maxAgeS: 3600, rpc: $frpc},
    standInSlot: ($ss | tonumber), vault: $v, configId: $c,
    deposit: {tx: $dtx, denominationWei: $den}, claim: {tx: $ctx, holdingAfterClaim: $hc},
    burn: {tx: $btx, recipient: $fr, holdingAfterBurn: $hb},
    checkpoint: {burnRoot: $br, burnCount: $bc, newState: $ns, tipId: $tip, vaultFinalState: $vf, burnRootAccepted: ($acc == "true")},
    release: {tx: $rtx, amountWei: $am, relayFeeWei: $rf, creditedFeesWei: $cr, recipientBalanceWei: $fb2, nullifier: $nul,
      replay: {callExit: ($rx | tonumber), revertSelector: $rs, nullifierUsed: ($nu == "true")},
      replayRefused: (($rx | tonumber) != 0 and $rs == "0xcad2ae02" and $nu == "true")}}' > "$R/facts.json"
say "the round trip: every step ok ($(wc -l < "$STEPS") steps)"
