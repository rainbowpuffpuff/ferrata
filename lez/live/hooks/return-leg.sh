#!/usr/bin/env bash
# The return leg over a zone whose Bedrock runs Logos Blockchain 0.3.5-rc.1 (LEZ v0.4.0-rc3): the hook of ../run.sh's
# phase `hook` (FERRATA_LIVE_HOOK=lez/live/hooks/return-leg.sh), run while the zone runs (rc3's sequencer_service, one
# sequencer, and its 0.3.5-rc.1 node, one validator, leader proofs in dev mode), with the zone in ZONE_* variables.
# On the build box only; nothing is sent to any public network.
#
# hooks/crown.sh's session on this zone, with what the return leg needs on top:
#   - the node's fork digest (GET /chain/fork) becomes the configuration's bedrockGenesisId (crown-a --fork-digest:
#     checkpoint-core's bedrock_0_3_5 module docs, where the deployment pins it);
#   - the Ethereum side as checkpoint/attestor/zone-bridge.sh's: a local anvil (no fork, Sepolia's chain id, which
#     crown-driver pins; its state dumped for the vault stage), FerrataVault v5 in mode (B) behind RISC Zero's own
#     Groth16 verifier (e2e/crown/sepolia.sh deploy deploys it), pinning the image ids of RL_PROGRAMS/ids.jsonl:
#     checkpoint_0_3_5 (A), checkpoint_b_0_3_5 (B), release; nMin 1, tMin 1 (one sequencer); deposits j0 (public) and
#     j1 (private) and their captures;
#   - one ferrata_attestor beside the sequencer (its key certified by the sequencer's channel key for this deployment),
#     attesting the node's final tier every RL_ROUND_S s while crown-b runs, then once the final tier reaches T1; it
#     signs a tip only when the sequencer's own LEZ node (ZONE_RPC, its getBlock) holds it final, and the zone is up
#     for its last round; the node's answers the checkpoint is built from are read separately, from FROM to the LIB;
#   - crown-b (pairing, registration, imports, Claim, ClaimPrivately, a public and a private Burn, the private ones
#     proven for real by LEZ's wallet);
#   - records in RL_RUN (a run directory under the worktree's e2e/runs/) in zone-bridge.sh's layout, so that its
#     later stages run on it: ZB_CHECKPOINT_B=checkpoint_b_0_3_5 ZB_N_MIN=1 ZB_T_MIN=1 zone-bridge.sh input|prove|...
#
# Environment (from run-live.sh): ZONE_RPC, ZONE_BEDROCK, ZONE_RUN_DIR, ZONE_LOG, ZONE_BIN, ZONE_OUT, ZONE_WAIT_FINAL,
# ZONE_PROGRAM_BIN, ZONE_SEQUENCER_CONFIG, ZONE_DEPLOYMENT. Ours: RL_WORKTREE (the worktree: e2e/, vault/, the
# attestor; the zone may run from a copy of it with rc3 selected), RL_RUN (the run directory, under
# $RL_WORKTREE/e2e/runs/), ZB_TMP (the box scratch zone-bridge.sh uses: the anvil state goes to
# $ZB_TMP/work/<run>/anvil-state.json), RL_BRIDGE_BIN (the bridge program), RL_PROGRAMS (R0BF programs and ids.jsonl),
# RL_DRIVER (default $ZONE_BIN/crown-driver), RL_ATTESTOR (default the worktree's checkpoint/target/release
# ferrata_attestor), RL_FIXTURE, RL_PROOF_THREADS (32), RL_ROUND_S (60).
set -euo pipefail
: "${ZONE_RPC:?run from run-live.sh phase hook}" "${RL_WORKTREE:?}" "${RL_RUN:?}" "${ZB_TMP:?}" "${RL_BRIDGE_BIN:?}" "${RL_PROGRAMS:?}"
WT="$RL_WORKTREE"
BESIDE="$(cd "$WT/.." && pwd)"
DRV="${RL_DRIVER:-$ZONE_BIN/crown-driver}"
ATT="${RL_ATTESTOR:-$WT/checkpoint/target/release/ferrata_attestor}"
FIXTURE="${RL_FIXTURE:-$WT/crates/ferrata-spectests/fixtures/sepolia-2026-11-12}"
THREADS="${RL_PROOF_THREADS:-32}"; ROUND_S="${RL_ROUND_S:-60}"
RUN="$RL_RUN"; NAME="$(basename "$RUN")"
case "$RUN" in "$WT"/e2e/runs/*) ;; *) echo "RL_RUN must be under $WT/e2e/runs/"; exit 2 ;; esac
W="$ZB_TMP/work/$NAME"; H="$W/hook"; A="$W/att-0"
R=http://127.0.0.1:8545; CHAIN_ID=11155111
Z=0x0000000000000000000000000000000000000000000000000000000000000000
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
say() { echo "[return-leg $(date -u +%H:%M:%S)] $*" | tee -a "$RUN/logs/zone-bridge.log"; }
die() { say "STOP: $*"; exit 1; }
scrub() { sed -e "s#$W/#work/#g" -e "s#$ZB_TMP/#tmp/#g" -e "s#$BESIDE/#spike/#g" -e "s#$HOME/#~/#g"; }
id_of() { jq -r --arg p "$1.bin" 'select(.program == $p or (.program | endswith("/" + $p))) | .image_id' "$RL_PROGRAMS/ids.jsonl"; }
[ -e "$RUN/identity.json" ] && { echo "$RUN holds a run already"; exit 2; }
rm -rf "$W"; mkdir -p "$H" "$A/rounds" "$RUN/logs" "$RUN/zone/attestor-0" "$RUN/lez/session-a" "$RUN/lez/session-b" "$RUN/lez/inputs"
for f in "$DRV" "$ATT" "$RL_BRIDGE_BIN" "$ZONE_PROGRAM_BIN"; do [ -e "$f" ] || die "missing $f"; done
[ -d "$WT/vault/out" ] || die "the vault is not built ($WT/vault/out)"
CKA="$(id_of checkpoint_0_3_5)"; CKB="$(id_of checkpoint_b_0_3_5)"; REL="$(id_of release)"
[ -n "$CKA" ] && [ -n "$CKB" ] && [ -n "$REL" ] || die "RL_PROGRAMS/ids.jsonl lacks checkpoint_0_3_5, checkpoint_b_0_3_5 or release"
say "the zone: LEZ RPC $ZONE_RPC, Bedrock $ZONE_BEDROCK; image ids: checkpoint_0_3_5 $CKA, checkpoint_b_0_3_5 $CKB, release $REL"

# ---- the node: its fork digest, its channel ---------------------------------------------------------------------
CHANNEL="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["bedrock_config"]["channel_id"])' "$ZONE_SEQUENCER_CONFIG" 2>/dev/null \
  || echo 0101010101010101010101010101010101010101010101010101010101010101)"
curl -fsS -m 10 "$ZONE_BEDROCK/chain/fork" > "$H/chain-fork.json" || die "GET /chain/fork: no answer (not a 0.3.5-rc.1 node?)"
DIGEST="$(jq -r .fork_digest "$H/chain-fork.json" | sed 's/^0x//' | tr 'A-F' 'a-f')"
[[ "$DIGEST" =~ ^[0-9a-f]{64}$ ]] || die "/chain/fork: $(cat "$H/chain-fork.json")"
curl -fsS -m 10 "$ZONE_BEDROCK/chain/id" > "$H/chain-id.json" 2>/dev/null || true
cp "$H/chain-fork.json" "$RUN/zone/chain-fork.json"; [ -s "$H/chain-id.json" ] && cp "$H/chain-id.json" "$RUN/zone/chain-id.json"
say "node: fork digest $DIGEST (GET /chain/fork), channel $CHANNEL"
for _ in $(seq 1 120); do
  curl -fsS -m 5 -o "$H/channel.json" "$ZONE_BEDROCK/channel/$CHANNEL" 2>/dev/null && grep -q '"accredited_keys":\["' "$H/channel.json" && break
  sleep 5
done
grep -q '"accredited_keys":\["' "$H/channel.json" 2>/dev/null || die "the node does not serve the channel's keys"
FROM="$(jq -r .tip_slot "$H/channel.json")"; FROM=$(( FROM > 600 ? FROM - 600 : 0 ))
say "the channel: $(jq -c '{keys: (.accredited_keys | length), configuration_threshold, posting_timeframe, tip_slot}' "$H/channel.json"); the attestor reads node blocks from slot $FROM"

# ---- LEZ session A ---------------------------------------------------------------------------------------------
D=(--rpc "$ZONE_RPC" --run-dir "$H/driver" --program-bin "$ZONE_PROGRAM_BIN" --bridge-bin "$RL_BRIDGE_BIN" --chain stand-in
   --fixture "$FIXTURE" --sequencer-log "$ZONE_LOG/sequencer.log" --wait-final "$ZONE_WAIT_FINAL")
env -u RISC0_DEV_MODE "$DRV" crown-a "${D[@]}" --channel "$H/channel.json" --genesis "$ZONE_DEPLOYMENT" --fork-digest "$DIGEST" \
  --identity-out "$H/session-a.json" > "$H/crown-a.log" 2>&1 || { tail -n 30 "$H/crown-a.log"; die "crown-a"; }
SA="$H/session-a.json"
[ "$(jq -r .bedrockGenesisId "$SA" | sed 's/^0x//')" = "$DIGEST" ] || die "session A's bedrockGenesisId is not the fork digest"
cp "$SA" "$H/channel.json" "$RUN/lez/session-a/"; cp "$ZONE_DEPLOYMENT" "$RUN/lez/session-a/deployment-settings.yaml"
T0="$(jq -r .tip.id "$SA")"; T0H="$(jq -r .tip.hash "$SA")"
KEYS="$(jq -r '.channel.keys | join(",")' "$SA")"; PT="$(jq -r .channel.postingTimeframe "$SA")"; CTH="$(jq -r .channel.configTipHash "$SA")"
say "session A: T0 = $T0 $T0H; bedrockGenesisId = the fork digest; key set n = $(jq -r '.channel.keys | length' "$SA"), posting timeframe $PT"

# ---- Ethereum: a local anvil, RISC Zero's verifier, the vault with this build's ids, two deposits, their captures ----
anvil --chain-id "$CHAIN_ID" --port 8545 --gas-limit 100000000 --auto-impersonate \
  --dump-state "$W/anvil-state.json" --state-interval 15 > "$H/anvil.log" 2>&1 & ANVIL=$!
trap 'kill -INT $ANVIL 2>/dev/null || true' EXIT
for _ in $(seq 60); do cast chain-id --rpc-url $R > /dev/null 2>&1 && break; sleep 1; done
DEPLOYER="$(cast to-check-sum-address "0x$(cast keccak 'ferrata/rc3-return/anvil-deployer' | cut -c27-66)")"
cast rpc anvil_setBalance "$DEPLOYER" 0x3635C9ADC5DEA00000 --rpc-url $R > /dev/null
N="$(cast nonce "$DEPLOYER" --rpc-url $R)"
VAULT="$(cast compute-address "$DEPLOYER" --nonce $(( N + 3 )) | awk '{print $NF}')"   # verifier N, stop N+1, registry N+2
CID="$("$DRV" config-id --identity "$SA" --vault "$VAULT" | jq -r .configId)"
GS="$(python3 -I "$WT/e2e/crown/identity.py" genesis-state --tip-id "$T0" --tip-hash "$T0H" --keys "$KEYS" \
      --posting-timeframe "$PT" --config-tip-hash "$CTH")"
jq -n --arg v "$VAULT" --arg c "$CID" --arg g "$GS" --argjson ch "$CHAIN_ID" --arg d "$DEPLOYER" --arg f "$DIGEST" '{label:
  "STAND-IN Ethereum side: a local anvil (no fork, Sepolia chain id because crown-driver pins it), W and the release delay warped; the zone (LEZ v0.4.0-rc3, Logos Blockchain 0.3.5-rc.1), its programs and proofs are real",
  chainId: $ch, vault: $v, configId: $c, genesisState: $g, delta: 3600, deployer: $d, forkDigest: $f}' > "$RUN/identity.json"
jq -n --arg c "$CID" --arg g "$GS" --arg a "$CKA" --arg b "$CKB" --arg r "$REL" --argjson pt "$PT" --arg z "$Z" \
  '{_source: "rc3-return (lez/live/hooks/return-leg.sh): one LEZ v0.4.0-rc3 sequencer on a Logos Blockchain 0.3.5-rc.1 node; the checkpoint guests of guests-0-3-5",
    checkpointImageIdA: $a, checkpointImageIdB: $b, releaseImageId: $r, lezConfigId: $c, genesisState: $g, nMin: 1, tMin: 1,
    postingTimeframe: $pt, ethLezDefinition: $z, tokenLezDefinition: $z}' > "$RUN/params.json"
d0="$("$DRV" destinations | jq -r .public0)"; d1="$("$DRV" destinations | jq -r .private0)"
C0="$(python3 -I "$WT/e2e/crown/identity.py" commitment --chain-id "$CHAIN_ID" --vault "$VAULT" --depositor "$DEPLOYER" --j 0 --kind public --dest "$d0")"
C1="$(python3 -I "$WT/e2e/crown/identity.py" commitment --chain-id "$CHAIN_ID" --vault "$VAULT" --depositor "$DEPLOYER" --j 1 --kind private --dest "$d1")"
jq -n --arg v "$VAULT" --arg c0 "$C0" --arg c1 "$C1" --argjson ch "$CHAIN_ID" '{chainId: $ch, vault: $v,
  deposits: [{j: 0, denomIndex: 0, commitment: $c0}, {j: 1, denomIndex: 0, commitment: $c1}]}' > "$RUN/crown.json"
say "anvil (local, chain id $CHAIN_ID, no fork): deployer $DEPLOYER (impersonated) nonce $N; vault $VAULT; configId $CID; genesisState $GS"
BASE="$(cast block-number --rpc-url $R)"
slots="$("$DRV" capture-slots | jq -r '.slots[].slot')"; sargs=(); for s in $slots; do sargs+=(--slot "$s"); done
export SEPOLIA_RPC_URL=$R ETH_FROM="$DEPLOYER" FERRATA_UNLOCKED=1 FERRATA_NONCE_GAP=0
bash "$WT/e2e/crown/sepolia.sh" "$RUN" deploy > "$H/eth-deploy.log" 2>&1 || { tail -n 30 "$H/eth-deploy.log"; die "vault deployment"; }
rm -f "$WT/vault/deployments/crown-$NAME.json"   # the same record is $RUN/deploy.json
[ "$(cast call "$VAULT" 'checkpointImageId()(bytes32)' --rpc-url $R 2>/dev/null)" = "$CKB" ] \
  || say "note: checkpointImageId() reads $(cast call "$VAULT" 'checkpointImageId()(bytes32)' --rpc-url $R 2>&1 | head -n 1)"
for j in 0 1; do
  bash "$WT/e2e/crown/sepolia.sh" "$RUN" deposit "$j" > "$H/eth-deposit-$j.log" 2>&1 || { tail -n 30 "$H/eth-deposit-$j.log"; die "deposit $j"; }
  python3 -I "$WT/lez/crown/capture-anvil.py" --rpc $R --vault "$VAULT" --from-block "$BASE" "${sargs[@]}" "$H/capture-$((j + 1))" \
    > "$H/capture-$((j + 1)).log" 2>&1 || { tail -n 20 "$H/capture-$((j + 1)).log"; die "capture $((j + 1))"; }
  say "deposit $j sent; capture-$((j + 1)): $(tail -n 1 "$H/capture-$((j + 1)).log")"
done
bash "$WT/e2e/crown/sepolia.sh" "$RUN" status > "$H/eth-status.log" 2>&1 || true
sleep 20; kill -INT $ANVIL 2>/dev/null || true; wait $ANVIL 2>/dev/null || true; trap - EXIT
[ -s "$W/anvil-state.json" ] || die "anvil left no state at $W/anvil-state.json"
cp -r "$H/capture-1" "$H/capture-2" "$RUN/lez/inputs/"
"$DRV" check --chain stand-in --program-bin "$ZONE_PROGRAM_BIN" --bridge-bin "$RL_BRIDGE_BIN" --capture "$H/capture-1" \
  --capture "$H/capture-2" --identity "$SA" > "$H/check.log" 2>&1 || { tail -n 20 "$H/check.log"; die "crown-driver check"; }
say "crown-driver check: $(tail -n 1 "$H/check.log")"

# ---- the attestor beside the sequencer ----------------------------------------------------------------------------
KEYFILE="$ZONE_RUN_DIR/sequencer_home/channel_signing_key"
[ -s "$KEYFILE" ] || { ls -la "$ZONE_RUN_DIR/sequencer_home" >&2; die "no channel_signing_key in the sequencer's home"; }
jq -n --arg c "$(printf '%064x' "$CHAIN_ID")" --arg v "$(echo "$VAULT" | tr 'A-F' 'a-f' | sed 's/^0x//')" --arg l "${CID#0x}" --arg ch "$CHANNEL" \
  '{evm_chain_id: $c, vault: $v, lez_config_id: $l, channel_id: $ch}' > "$W/deployment.json"
cp "$W/deployment.json" "$RUN/zone/deployment.json"
"$ATT" keygen --out "$A/key" > "$A/pub.txt"
"$ATT" certify --channel-key "$KEYFILE" --att-pub "$(cat "$A/pub.txt")" --deployment "$W/deployment.json" --out "$A/cert.json" \
  | tee -a "$RUN/logs/zone-bridge.log"
cp "$A/cert.json" "$RUN/zone/attestor-0/cert.json"
attest_round() { # out: one attestation of the node's final tier (fresh answers each round), checked against the LEZ node
  rm -rf "$A/answers"; mkdir -p "$A/answers"
  "$ATT" attest --deployment "$W/deployment.json" --cert "$A/cert.json" --att-key "$A/key" --node "$ZONE_BEDROCK" \
    --lez "$ZONE_RPC" --save "$A/answers" --slot-from "$FROM" --state "$A/state.json" --out "$1" >> "$A/attest.log" 2>&1
}
( n=0; last=""; stop=""
  trap 'stop=1' TERM   # SIGTERM ends the loop after the round in hand
  while [ -z "$stop" ]; do
    n=$((n + 1))
    if attest_round "$A/round.json"; then
      tip="$(jq -r .tip_id "$A/round.json")"
      echo -e "$(date -u +%FT%TZ)\t$n\tsigned\t$tip\t$(jq -r .tip_hash "$A/round.json")\t$(jq -r .node.lib_slot "$A/round.json")" >> "$A/rounds.tsv"
      [ "$tip" != "$last" ] && cp "$A/round.json" "$A/rounds/tip-$tip.json"; last="$tip"
    else
      echo -e "$(date -u +%FT%TZ)\t$n\trefused\t-\t$(tail -n 1 "$A/attest.log" | tr '\t' ' ' | cut -c1-200)" >> "$A/rounds.tsv"
    fi
    [ -n "$stop" ] && break
    sleep "$ROUND_S" & wait $! || true
  done ) > "$W/attest-loop.log" 2>&1 &
LOOP=$!
trap 'kill $LOOP 2>/dev/null || true' EXIT
say "attestor started (key from the OS's randomness, not recorded; certified by the sequencer's channel key; a round every $ROUND_S s)"

# ---- LEZ session B: the bridge, the private claim and burn proven for real ----------------------------------------
say "crown-b: real private proofs, RAYON_NUM_THREADS=$THREADS"
crc=0
env -u RISC0_DEV_MODE RAYON_NUM_THREADS="$THREADS" RISC0_SERVER_PATH="$(command -v r0vm)" nice -n 10 "$DRV" crown-b "${D[@]}" \
  --identity "$SA" --capture "$H/capture-1" --capture "$H/capture-2" --private real \
  --session-out "$H/session-b.json" --notes-out "$H/burn-notes.json" > "$H/crown-b.log" 2>&1 || crc=$?
tail -n 15 "$H/crown-b.log" | scrub | tee -a "$RUN/logs/zone-bridge.log"
"$DRV" export "${D[@]}" --out "$H/export" > "$H/export.log" 2>&1 || say "export failed ($?)"
[ "$crc" = 0 ] || die "crown-b exit $crc"
cp "$H/session-b.json" "$H/burn-notes.json" "$RUN/lez/session-b/"
T1="$(jq -r .tipT1.id "$H/session-b.json")"
say "session B: T1 = $T1; steps $(jq -c '[.steps[] | [.step, .status, .block]]' "$H/session-b.json" | cut -c1-700)"

# ---- the final tier reaches T1: the attestor's final round (the zone still up: its LEZ node must answer) ------------
kill -TERM $LOOP 2>/dev/null || true; wait $LOOP 2>/dev/null || true; trap - EXIT
ok=""
for _ in $(seq 1 40); do
  if attest_round "$A/final.json" && [ "$(jq -r .tip_id "$A/final.json")" -ge "$T1" ]; then ok=1; break; fi
  sleep 30
done
[ -n "$ok" ] || die "the attestor's final tier did not reach T1 = $T1 ($(tail -n 1 "$A/attest.log"))"
tail -n 1 "$A/attest.log" | scrub | tee -a "$RUN/logs/zone-bridge.log"
"$ATT" verify --deployment "$W/deployment.json" --att "$A/final.json" | scrub | tee -a "$RUN/logs/zone-bridge.log"
cp "$A/final.json" "$A/rounds.tsv" "$RUN/zone/attestor-0/"
scrub < "$A/attest.log" | tail -n 400 > "$RUN/zone/attestor-0/attest.log"
ls "$A/rounds" > "$RUN/zone/attestor-0/tips-signed.txt"
[ "$(jq -r .node.fork_digest "$A/final.json")" = "$DIGEST" ] || die "the final tier's fork digest is not /chain/fork's"

# ---- the node's answers (the checkpoint's input is built from these alone): from FROM to the LIB, read after the final
#      round (a round reads the node only from the slot of the tip the attestor signed last)
rm -rf "$W/node"; mkdir -p "$W/node/blocks"; echo "$FROM" > "$W/slot-from.txt"
curl -fsS -m 10 -o "$W/node/info.json" "$ZONE_BEDROCK/cryptarchia/info" || die "GET /cryptarchia/info"
curl -fsS -m 10 -o "$W/node/channel.json" "$ZONE_BEDROCK/channel/$CHANNEL" || die "GET /channel"
LIBS="$(jq -r '.cryptarchia_info.lib_slot // .lib_slot' "$W/node/info.json")"; [[ "$LIBS" =~ ^[0-9]+$ ]] || die "no lib_slot"
s0="$FROM"
while [ "$s0" -le "$LIBS" ]; do
  s1=$(( s0 + 499 )); [ "$s1" -gt "$LIBS" ] && s1="$LIBS"
  curl -fsS -m 60 -o "$W/node/blocks/$s0-$s1.json" "$ZONE_BEDROCK/cryptarchia/blocks?slot_from=$s0&slot_to=$s1" || die "GET blocks $s0..=$s1"
  s0=$(( s1 + 1 ))
done
tar -C "$W" --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 -cf - node | zstd -q -T0 -19 -o "$RUN/zone/node-answers.tar.zst" -f
cp "$W/node/channel.json" "$RUN/zone/channel-end.json"; cp "$W/node/info.json" "$RUN/zone/info-end.json"
say "node answers (read after the attestor's final round): slots from $FROM to the LIB $(jq -r '.cryptarchia_info.lib_slot // .lib_slot' "$W/node/info.json"), $(ls "$W/node/blocks" | wc -l) windows, $(cat "$W"/node/blocks/*.json | wc -c) bytes"
[ -d "$H/export" ] && tar -C "$H" -cf - export | zstd -q -T0 -19 -o "$RUN/lez/session-b/driver-export.tar.zst" -f
for f in crown-a.log crown-b.log check.log export.log capture-1.log capture-2.log eth-deploy.log eth-deposit-0.log eth-deposit-1.log eth-status.log anvil.log; do
  [ -f "$H/$f" ] && scrub < "$H/$f" > "$RUN/logs/$f"
done
for f in "$H"/driver/out/crown-private-*-tx.borsh; do [ -f "$f" ] && cp "$f" "$RUN/lez/session-b/"; done
cp "$H/driver/manifest.tsv" "$RUN/lez/manifest.tsv" 2>/dev/null || true
find "$RUN/logs" -type f -name '*.log' -exec sed -i -e "s#$W/#work/#g" -e "s#$ZB_TMP/#tmp/#g" -e "s#$BESIDE/#spike/#g" -e "s#$HOME/#~/#g" {} +
say "hook done: T0 $T0, T1 $T1, attested tip $(jq -r .tip_id "$A/final.json")"
