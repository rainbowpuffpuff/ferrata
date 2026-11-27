#!/usr/bin/env bash
# The bridge program on a zone of four LEZ sequencers, an attestor beside each, and the rule (B) checkpoint over the
# zone's real blocks into FerrataVault v5 on a LOCAL anvil (B1, RFP-022's return leg). Results: RESULTS.md §8.
#
#   checkpoint/attestor/zone-bridge.sh <stage> <run dir under e2e/runs/>      (from the worktree root, on the build box)
#
#   build     crown-driver (this tree), realblocks-crown, ferrata_attestor, ferrata-e2e, the vault
#   zone      the live part, one command (~1 h): a Logos Blockchain node and four sequencer_service processes
#             (upstream's four-sequencer devnet, attestor/zone.sh's set-up); crown-driver crown-a on the zone (the
#             attestation program and the bridge, the instance of the STAND-IN beacon chain); a local anvil (no fork):
#             the v5 vault in mode (B) over the zone's four keys (nMin 4, tMin 3), deposits j0 (public) and j1
#             (private) and their captures; one ferrata_attestor per sequencer (its key certified by that sequencer's
#             channel key) attesting the node's final tier every ZB_ROUND_S s; crown-driver crown-b (pairing,
#             registration, imports, Claim, ClaimPrivately and both burns, private ones proven by LEZ's wallet);
#             (with ZB_ROTATE=1, before session B, a rotation by the zone's committee: a fifth sequencer's key staked
#             on LEZ with `sequencer_service stake`, the committee's ChannelConfigOp applied by Bedrock, the fifth
#             sequencer and its attestor started, ZB_ROTATE_RUN_S (120) s); the final tier attested by every attestor
#             while the sequencers still run (each attestor asks its own sequencer's LEZ node, which must be up), all
#             on one tip at or above T1; the sequencers stopped; the node's answers saved from the first slot read to
#             its LIB
#   answers   re-packs the answers the zone stage saved (work/node) into zone/node-answers.tar.zst; it recovers
#             nothing after a failed zone stage (an attestor's round reads only from its signed tip's slot)
#   input     realblocks-crown over the node's final blocks T0+1..=T (and the rotation, if any) with all attestations
#             but one (3 of 4, n = 4, t = 3; after a rotation 4 of 5, t = 4); all n accepted, t - 1 refused (natively
#             and in r0vm); r0vm executor run
#   prove     the checkpoint's succinct proof on the CPU, identity_p254 (-> work/checkpoint/input.json for mwrap)
#   releases  crown-release (one release statement per burn), executor, CPU proofs, identity_p254 each
#   groth16 P the Groth16 receipt of P (checkpoint | release-<name>) from work/P/proof.json (mwrap on the laptop)
#   gpu-jobs  instead of prove and groth16 (with ZB_RELEASES_PROVE=0 for releases): one prover request per proof, for
#             deploy/testnet/tests/prover-kill-resume-modal.sh (the bridge's prover client on Modal's GPU, killed once)
#   gpu-seals the client's answers ($RUN/prover/done) as receipts/<proof>/seal.hex, journal.hex, prover.json
#   vault     the local anvil from its saved state: submitCandidate, W warped, finalizeAttested, the release delay
#             warped, the releases; the vault's state checked
#   fixture   vault/test/real/fixtures/${ZB_FIXTURE:-zone-bridge}.json for ZoneBridge.t.sol (the journal, the seals, the
#             run's values, the rotated key set)
#   verify    the input and journal re-derived from the recorded node answers and attestations; every receipt verified
#   records   only what every stage but build and verify ends with: the run's files scrubbed of this machine's paths
#             and checked (scripts/scrub_records.py); a file that still names a home directory or an agent's folder fails
#
# STAND-INs (every record says so): the beacon chain on LEZ (a committee the driver holds finalizes the anvil's
# blocks), the Ethereum side (a local anvil with Sepolia's chain id, which crown-driver pins), the vault's W and
# release delay (anvil time warps), leader proofs in dev mode (as every LEZ devnet here). Real: the four sequencers
# and their gossip, Bedrock's channel and its turn rotation, the programs (docs/image-ids.md), LEZ's private proofs,
# the attestors and their certificates, checkpoint_b's proof, the Groth16 wraps, RISC Zero's verifier in the vault.
#
# Environment: ZB_BUNDLE (the crown-live-3 LEZ bundle: binaries, the final LEZ programs, the fixture), ZB_PROGRAMS (R0BF
# checkpoint_b.bin, checkpoint.bin, release.bin, ids.jsonl of docs/image-ids.md), ZB_DRIVER, ZB_TMP (box scratch),
# ZB_ROUND_S (60), ZB_LEAVE_OUT (the attestor left out; default the last; `none`: all of them), ZB_ROTATE,
# ZB_ROTATE_RUN_S, ZB_FIXTURE, ZB_RELEASES_PROVE (0: stage releases writes the statements and runs them in the
# executor, not the prover), ZB_TOKEN (eth by default; test-token selects the deployment's TUSD); for stages after `zone`:
# ZB_CHECKPOINT_B (the rule (B) program's name in ZB_PROGRAMS,
# default checkpoint_b; checkpoint_b_0_3_5 for a zone on Logos Blockchain 0.3.5-rc.1, whose `zone` stage is
# lez/live/hooks/return-leg.sh), ZB_N_MIN and ZB_T_MIN (the vault's floors, default 4 and 3).
#
# LEZ v0.4.0-rc3 (RFP-022 F.7 on the current release candidate): ZB_LEZ names the LEZ tree whose devnet configuration
# and Bedrock settings the zone uses (default upstream/lez-v0.4.0-rc1; rc3's four-sequencer devnet is byte-identical,
# its Bedrock settings are Logos Blockchain 0.3.5-rc.1's, which the bundle must carry in lez-config/: a crown-rc3 bundle
# repacked with this tree's crown-driver built for rc3, lez/crown/build.sh repack). On a 0.3.5-rc.1 node (its
# GET /chain/fork answers) the fork digest becomes session A's bedrockGenesisId (crown-a --fork-digest), as
# lez/live/hooks/return-leg.sh does with one sequencer. ZB_CHECKPOINT_A names the rule (A) program the vault pins beside
# ZB_CHECKPOINT_B (default checkpoint; checkpoint_0_3_5 on rc3). ZB_DEPLOYER_LABEL: the label whose keccak names the anvil
# deployer (default ferrata/zone-bridge/anvil-deployer; a fresh label gives an address with nonce 0 on Sepolia too, so
# that vault/script/fork-rehearsal-31.sh can deploy the same vault at the same address on a fork of it).
# Every attestor round is timed (GNU time: wall, user and system CPU, peak RSS) and the bytes of the node's and the LEZ
# node's answers it read are counted (zone/attestor-<i>/resources.tsv: RFP-022 P.6's ferrata_attestor), and each
# attestor's earliest signed round is kept (zone/attestor-<i>/first-round.json: the stale tip of stage input's refusals).
set -euo pipefail
STAGE="${1:?stage}"; RUN_ARG="${2:?run dir (under e2e/runs/)}"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SPIKE="$(cd "$REPO/.." && pwd)"
LEZ="${ZB_LEZ:-$SPIKE/upstream/lez-v0.4.0-rc1}"; DEV="$LEZ/lez/configs/docker-all-in-one/devnet"
T="${ZB_TMP:-$SPIKE/tmp/zone-bridge}"
mkdir -p "$RUN_ARG"; RUN="$(cd "$RUN_ARG" && pwd)"; NAME="$(basename "$RUN")"
case "$RUN" in "$REPO"/e2e/runs/*) ;; *) echo "the run directory must be under e2e/runs/ (forge reads it there)"; exit 2 ;; esac
W="$T/work/$NAME"
BUNDLE="${ZB_BUNDLE:-$SPIKE/tmp/crown-fresh/lez/bundle.tar.zst}"
PROGRAMS="${ZB_PROGRAMS:-$SPIKE/tmp/final-build/repro-a/r0bf}"
DRV="${ZB_DRIVER:-$T/target-lez/release/crown-driver}"
ATT="$REPO/checkpoint/target/release/ferrata_attestor"
RC="$REPO/checkpoint/target/release/realblocks-crown"
E="$REPO/bridge/target/release/ferrata-e2e"
ROUND_S="${ZB_ROUND_S:-60}"
CKB="${ZB_CHECKPOINT_B:-checkpoint_b}"
CHANNEL=0101010101010101010101010101010101010101010101010101010101010101
FUND=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26   # the dev node's funded key (compose default)
PORT=18080; API="http://127.0.0.1:$PORT"; R=http://127.0.0.1:8545
INTERVAL=10; TICK_MIN=1; TICK_MAX=6; TICK_AFTER=3; MAX_STARTS=4          # crown session.sh's phase rule
CHAIN_ID=11155111                                                       # crown-driver's SEPOLIA_CHAIN_ID
SELECTED_TOKEN="${ZB_TOKEN:-eth}"
case "$SELECTED_TOKEN" in eth|test-token) ;; *) echo "ZB_TOKEN must be eth or test-token"; exit 2 ;; esac
# The default ETH fixture name is historical; ERC-20's default is the suite written with this change.
if [ "$SELECTED_TOKEN" = test-token ]; then FIXTURE_DEFAULT=zone-bridge-rc3-erc20; else FIXTURE_DEFAULT=zone-bridge; fi
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1 PATH="$HOME/.foundry/bin:$PATH"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$RUN/logs/zone-bridge.log"; }
die() { log "STOP: $*"; exit 1; }
# The records name no path of this machine and no agent's folder: the worktree's paths become relative to it, a
# scratch directory's become tmp/ (the same rules as records_final, which every stage ends with)
scrub() { sed -e "s#$W/#work/#g" -e "s#$T/#tmp/#g" -e "s#$REPO/##g" -e "s#$SPIKE/tmp/[^/][^/]*/#tmp/#g" -e "s#$SPIKE/#spike/#g" -e "s#$HOME/#~/#g"; }
id_of() { local v; v="$(jq -r --arg p "$1.bin" 'select(.program == $p or (.program | endswith("/" + $p))) | .image_id' "$PROGRAMS/ids.jsonl")"; [ -n "$v" ] || { echo "no $1 in $PROGRAMS/ids.jsonl" >&2; exit 1; }; echo "$v"; }
hexfile() { tr -d ' \n' < "$1" | sed 's/^0x//'; }
zero_address() { local a="${1,,}"; [ -z "$a" ] || [ "$a" = 0x0000000000000000000000000000000000000000 ]; }
asset_balance() { # token owner: an ERC-20 balance in base units, ETH in wei
  # cast prints a uint256 as "N [N in scientific notation]": the first word is the number
  if zero_address "$1"; then cast balance "$2" --rpc-url "$R"; else cast call "$1" 'balanceOf(address)(uint256)' "$2" --rpc-url "$R" | awk '{print $1}'; fi
}
mkdir -p "$RUN/logs"

stop_pid() { # pid name [seconds]: SIGTERM, then SIGKILL
  local pid="$1" name="$2" s="${3:-60}" i
  [ -n "$pid" ] || return 0
  kill -0 "$pid" 2>/dev/null || { wait "$pid" 2>/dev/null || true; return 0; }
  kill -TERM "$pid" 2>/dev/null || true
  for i in $(seq 1 $(( s * 2 ))); do kill -0 "$pid" 2>/dev/null || break; sleep 0.5; done
  if kill -0 "$pid" 2>/dev/null; then kill -KILL "$pid" 2>/dev/null || true; log "$name did not stop in $s s: killed"; fi
  wait "$pid" 2>/dev/null || true
}

# ---- build -----------------------------------------------------------------------------------------------------
stage_build() {
  ( cd "$REPO/lez/crown/driver" && RISC0_HOME="$SPIKE/risc0-home-lez" CARGO_TARGET_DIR="$T/target-lez" cargo build --release --locked )
  ( cd "$REPO/checkpoint" && cargo build --release --locked -p realblocks --bin realblocks-crown && cargo build --release --locked -p ferrata-attestor )
  ( cd "$REPO/bridge" && cargo build --release --locked -p ferrata-e2e )
  ( cd "$REPO/vault" && forge build )
  sha256sum "$DRV" "$RC" "$ATT" "$E"
}

# ---- zone: the live part ----------------------------------------------------------------------------------------
SEQ_PID=(); NODE_PID=""; ANVIL_PID=""; LOOP_PID=()
zone_cleanup() {
  for p in "${LOOP_PID[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done
  for p in "${SEQ_PID[@]:-}"; do [ -n "$p" ] && kill -TERM "$p" 2>/dev/null || true; done
  [ -n "$ANVIL_PID" ] && kill -INT "$ANVIL_PID" 2>/dev/null || true
  sleep 5
  for p in "${SEQ_PID[@]:-}"; do [ -n "$p" ] && kill -KILL "$p" 2>/dev/null || true; done
  [ -n "$NODE_PID" ] && kill -TERM "$NODE_PID" 2>/dev/null || true
  sleep 3; [ -n "$NODE_PID" ] && kill -KILL "$NODE_PID" 2>/dev/null || true
  [ -n "$ANVIL_PID" ] && kill -KILL "$ANVIL_PID" 2>/dev/null || true
  mkdir -p "$RUN/zone/logs"
  for f in "$W"/logs/*.log; do
    [ -f "$f" ] || continue
    case "$(basename "$f")" in
      seq-*.log|bedrock.log) sed -e 's/\x1b\[[0-9;]*m//g' "$f" | scrub | gzip -c > "$RUN/zone/logs/$(basename "$f").gz" ;;
      *) scrub < "$f" > "$RUN/zone/logs/$(basename "$f")" ;;
    esac
  done
}

write_cfg() { # i: upstream's devnet sequencer configuration, loopback addresses, genesis plus local.json's funded accounts
  python3 -I - "$DEV/sequencer_config.json" "$LEZ/lez/sequencer/service/networks/local.json" "$W/cfg-$1.json" "$API" "$1" <<'EOF'
import json, sys
src, local, dst, api, i = sys.argv[1:6]; i = int(i)
c = json.load(open(src)); l = json.load(open(local))
have = {json.dumps(a, sort_keys=True) for a in c["genesis"]["actions"]}
extra = [a for a in l["genesis"]["actions"] if json.dumps(a, sort_keys=True) not in have]
assert all("supply_account" in a for a in extra), "only local.json's public test accounts are added"
c["genesis"]["actions"] += extra
c["bedrock_config"]["node_url"] = api
c["gossip"] = {"listen_addr": f"/ip4/127.0.0.1/udp/{19100 + i}/quic-v1",
               "bootstrap_peers": [f"/ip4/127.0.0.1/udp/{19100 + j}/quic-v1" for j in range(5) if j != i]}
c["metrics_listen_address"] = f"127.0.0.1:{19200 + i}"
json.dump(c, open(dst, "w"), indent=2)
EOF
}

start_attestor() { # i: a key, its certificate by sequencer i's channel key for this deployment, the loop
  local i="$1"
  mkdir -p "$W/att-$i/rounds" "$RUN/zone/attestor-$i"
  "$ATT" keygen --out "$W/att-$i/key" > "$W/att-$i/pub.txt"
  "$ATT" certify --channel-key "$W/seq-$i/channel_signing_key" --att-pub "$(cat "$W/att-$i/pub.txt")" \
    --deployment "$W/deployment.json" --out "$W/att-$i/cert.json" | tee -a "$RUN/logs/zone-bridge.log"
  cp "$W/att-$i/cert.json" "$RUN/zone/attestor-$i/cert.json"
  attest_loop "$i" > "$W/logs/attest-loop-$i.log" 2>&1 &
  LOOP_PID+=($!)
}

b58() { python3 -I -c '
import sys
b = bytes.fromhex(sys.argv[1].removeprefix("0x")); a = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
n = int.from_bytes(b, "big"); s = ""
while n: n, r = divmod(n, 58); s = a[r] + s
print("1" * (len(b) - len(b.lstrip(bytes(1)))) + s)' "$1"; }

rotate_committee() { # a fifth key staked on LEZ (upstream's `sequencer_service stake`); the committee proposes the channel's
                     # new key set (the turn's sequencer drafts the ChannelConfigOp, t(4) = 3 of the four sign it) and
                     # Bedrock applies it; then the fifth sequencer starts, with its attestor
  local k4 from ch n=0 k t0
  mkdir -p "$W/seq-4"; write_cfg 4
  sequencer_service setup "$W/seq-4" --config "$W/cfg-4.json" --funding-public-key "$FUND" > "$W/logs/setup-4.log" 2>&1 \
    || { tail -n 20 "$W/logs/setup-4.log"; die "setup 4"; }
  k4="$(grep -o 'channel signing public key *[0-9a-f]*' "$W/logs/setup-4.log" | awk '{print $NF}' || true)"
  [ -n "$k4" ] || { tail -n 20 "$W/logs/setup-4.log" | scrub; die "seq-4: no channel key in its setup log"; }
  ch="$(curl -fsS -m 5 "$API/channel/$CHANNEL")"; echo "$ch" > "$RUN/zone/channel-before-rotation.json"
  log "seq-4 set up: channel key $k4, not staked; the channel: $(echo "$ch" | jq -c '{keys: (.accredited_keys | length), configuration_threshold, config_tip_hash}')"
  from="$(b58 "$("$DRV" destinations | jq -r .public0)")"
  t0="$(date +%s)"
  env -u RISC0_DEV_MODE sequencer_service stake --wallet-home "$W/driver/wallet" --sequencer-key "$k4" --from "Public/$from" \
    > "$W/logs/stake.log" 2>&1 || { tail -n 20 "$W/logs/stake.log" | scrub; die "stake"; }
  tail -n 3 "$W/logs/stake.log" | scrub | tee -a "$RUN/logs/zone-bridge.log"
  for k in $(seq 1 120); do
    n="$(curl -fsS -m 5 "$API/channel/$CHANNEL" 2>/dev/null | jq -r '.accredited_keys | length' 2>/dev/null || echo 0)"
    [ "$n" = 5 ] && break; sleep 10
  done
  [ "$n" = 5 ] || die "the channel does not accredit a fifth key $(( $(date +%s) - t0 )) s after the stake"
  ch="$(curl -fsS -m 5 "$API/channel/$CHANNEL")"; echo "$ch" > "$RUN/zone/channel-after-rotation.json"
  log "the channel accredits five keys $(( $(date +%s) - t0 )) s after the stake was sent: $(echo "$ch" | jq -c '{keys: .accredited_keys, configuration_threshold, config_tip_hash}')"
  echo 5 > "$W/nseq"
  seq_aligned 4
  start_attestor 4
  log "seq-4 and its attestor started; ${ZB_ROTATE_RUN_S:-120} s before session B"
  sleep "${ZB_ROTATE_RUN_S:-120}"
  log "after the rotation: heights $(heights)"
}

seq_start() { # i marker: one launch of sequencer i
  local i="$1" marker="$2"
  echo "$marker" >> "$W/logs/seq-$i.log"
  ( export RUST_LOG="info,sequencer_core=debug,risc0_zkvm=warn,kameo=warn"; [ "$i" != 0 ] && export SEQUENCER_WAIT_FOR_CHANNEL=true
    exec env -u RISC0_DEV_MODE sequencer_service start "$W/seq-$i" --rpc-listen-address "127.0.0.1:$((19300 + i))" ) \
    >> "$W/logs/seq-$i.log" 2>&1 &
  SEQ_PID[$i]=$!
}

seq_aligned() { # i: started until its blocks fall TICK_MIN..TICK_MAX s after an epoch second (crown session.sh, lez/lab)
  local i="$1" starts=0 delay="" want launched sched after k marker
  while :; do
    starts=$((starts + 1))
    if [ -n "$delay" ]; then
      want=$(( ( (ORIGIN + TICK_AFTER - delay) % INTERVAL + INTERVAL ) % INTERVAL ))
      while [ $(( $(date +%s) % INTERVAL )) != "$want" ]; do sleep 0.1; done
    fi
    marker="zone-bridge start $i.$starts"; launched="$(date +%s)"; seq_start "$i" "$marker"; sched=""
    for k in $(seq 1 300); do
      sched="$(sed -n "/$marker/,\$p" "$W/logs/seq-$i.log" | grep -m1 'Block production scheduler started' | sed -n 's/^\[\([0-9T:-]*\)Z.*/\1/p' || true)"
      [ -n "$sched" ] && break
      kill -0 "${SEQ_PID[$i]}" 2>/dev/null || break
      sleep 1
    done
    [ -n "$sched" ] || { tail -n 20 "$W/logs/seq-$i.log" | scrub; die "seq-$i did not start its block scheduler"; }
    sched="$(date -u -d "${sched}Z" +%s)"; delay=$(( sched - launched ))
    after=$(( ( (sched + INTERVAL - ORIGIN) % INTERVAL + INTERVAL ) % INTERVAL ))
    log "seq-$i start $starts: scheduler $delay s after the launch; its blocks fall $after s after an epoch second"
    if [ "$after" -ge "$TICK_MIN" ] && [ "$after" -le "$TICK_MAX" ]; then break; fi
    if [ "$starts" -ge "$MAX_STARTS" ]; then log "seq-$i: this phase kept after $starts starts"; break; fi
    stop_pid "${SEQ_PID[$i]}" "seq-$i"
  done
  echo "$i $starts $after $delay" >> "$W/phases.txt"
}

heights() { local i out="" n; n="$(cat "$W/nseq" 2>/dev/null || echo 4)"; for i in $(seq 0 $(( n - 1 ))); do out="$out$( { curl -fsS -m 5 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' "http://127.0.0.1:$((19300 + i))" 2>/dev/null || true; } \
  | sed -n 's/.*"result":\([0-9]*\).*/\1/p')/"; done; echo "${out%/}"; }
lib_slot() { { curl -fsS -m 5 "$API/cryptarchia/info" 2>/dev/null || true; } | sed -n 's/.*"lib_slot":\([0-9]*\).*/\1/p'; }

attest_round() { # i out: one attestation of the node's final tier by attestor i (fresh answers each round), timed
  local i="$1" out="$2" A="$W/att-$i" rc=0 tf
  rm -rf "$A/answers"; mkdir -p "$A/answers"; tf="$A/time.txt"; rm -f "$tf"
  /usr/bin/time -f '%e %U %S %M' -o "$tf" \
  "$ATT" attest --deployment "$W/deployment.json" --cert "$A/cert.json" --att-key "$A/key" --node "$API" \
    --lez "http://127.0.0.1:$((19300 + i))" --save "$A/answers" --slot-from "$FROM" --state "$A/state.json" \
    --out "$out" >> "$A/attest.log" 2>&1 || rc=$?
  # wall s, user s, sys s, peak RSS KiB (GNU time), bytes of the answers read (node and LEZ node), exit
  echo -e "$(date -u +%FT%TZ)\t$(tail -n 1 "$tf" 2>/dev/null | tr ' ' '\t')\t$(du -sb "$A/answers" | cut -f1)\t$rc" >> "$A/resources.tsv"
  return "$rc"
}

attest_loop() { # i: a round every ROUND_S s while the zone runs; each new tip kept; SIGTERM ends it after the round in hand
  local i="$1" A="$W/att-$i" n=0 tip last="" stop=""
  trap 'stop=1' TERM
  while [ -z "$stop" ]; do
    n=$((n + 1))
    if attest_round "$i" "$A/round.json"; then
      tip="$(jq -r .tip_id "$A/round.json")"
      echo -e "$(date -u +%FT%TZ)\t$n\tsigned\t$tip\t$(jq -r .tip_hash "$A/round.json")\t$(jq -r .node.lib_slot "$A/round.json")" >> "$A/rounds.tsv"
      [ "$tip" != "$last" ] && cp "$A/round.json" "$A/rounds/tip-$tip.json"; last="$tip"
    else
      echo -e "$(date -u +%FT%TZ)\t$n\trefused\t-\t$(tail -n 1 "$A/attest.log" | tr '\t' ' ' | cut -c1-200)" >> "$A/rounds.tsv"
    fi
    [ -n "$stop" ] && break
    sleep "$ROUND_S" & wait $! || true
  done
}

final_rounds() { # n t1: every attestor's round side by side (each against its own sequencer's LEZ node, the zone up),
                 # again until all n sign one tip at or above t1: realblocks-crown takes attestations of one T
  local n="$1" t1="$2" try i rc tips pids=()
  for try in $(seq 1 60); do
    for i in $(seq 0 $(( n - 1 ))); do
      rm -f "$W/att-$i/final.json"; attest_round "$i" "$W/att-$i/final.json" & pids[$i]=$!
    done
    rc=0; for i in $(seq 0 $(( n - 1 ))); do wait "${pids[$i]}" || rc=1; done
    for i in $(seq 0 $(( n - 1 ))); do
      if [ -s "$W/att-$i/final.json" ]; then
        echo -e "$(date -u +%FT%TZ)\tfinal-$try\tsigned\t$(jq -r .tip_id "$W/att-$i/final.json")\t$(jq -r .tip_hash "$W/att-$i/final.json")\t$(jq -r .node.lib_slot "$W/att-$i/final.json")" >> "$W/att-$i/rounds.tsv"
      else
        echo -e "$(date -u +%FT%TZ)\tfinal-$try\trefused\t-\t$(tail -n 1 "$W/att-$i/attest.log" | tr '\t' ' ' | cut -c1-200)" >> "$W/att-$i/rounds.tsv"
      fi
    done
    tips="$(for i in $(seq 0 $(( n - 1 ))); do jq -r '"\(.tip_id) \(.tip_hash)"' "$W/att-$i/final.json" 2>/dev/null || echo none; done | sort -u)"
    log "final round $try: tips signed $(echo "$tips" | awk '{print $1}' | tr '\n' ' ')(T1 = $t1, heights $(heights), lib $(lib_slot))"
    if [ "$rc" = 0 ] && [ "$(echo "$tips" | wc -l)" = 1 ] && [ "${tips%% *}" != none ] && [ "${tips%% *}" -ge "$t1" ]; then return 0; fi
    sleep 10
  done
  for i in $(seq 0 $(( n - 1 ))); do tail -n 2 "$W/att-$i/attest.log" | scrub; done
  die "the attestors did not sign one tip at or above T1 = $t1 in 60 rounds"
}

save_answers() { # the node's answers from FROM to its LIB, as zone.sh reads them, into $W/node: the checkpoint's input is
                 # built from these alone (an attestor's round reads the node only from the slot of the tip it signed last)
  local d="$W/node" lib from to
  rm -rf "$d"; mkdir -p "$d/blocks"
  curl -fsS -m 10 -o "$d/info.json" "$API/cryptarchia/info" || die "GET /cryptarchia/info"
  curl -fsS -m 10 -o "$d/channel.json" "$API/channel/$CHANNEL" || die "GET /channel"
  lib="$(jq -r '.cryptarchia_info.lib_slot // .lib_slot' "$d/info.json")"
  [[ "$lib" =~ ^[0-9]+$ ]] || die "no lib_slot in the node's /cryptarchia/info"
  from="$FROM"
  while [ "$from" -le "$lib" ]; do
    to=$(( from + 499 )); [ "$to" -gt "$lib" ] && to="$lib"
    curl -fsS -m 60 -o "$d/blocks/$from-$to.json" "$API/cryptarchia/blocks?slot_from=$from&slot_to=$to" || die "GET blocks $from..=$to"
    from=$(( to + 1 ))
  done
}

stage_zone() {
  [ -e "$RUN/identity.json" ] && { echo "$RUN holds a run already (a new run directory, please)"; exit 2; }
  for f in "$DRV" "$ATT" "$E" "$BUNDLE" "$PROGRAMS/$CKB.bin"; do [ -e "$f" ] || { echo "missing $f (stage build, --put)"; exit 2; }; done
  rm -rf "$W"; mkdir -p "$W/bundle" "$W/bedrock" "$W/logs" "$W/inputs" "$RUN/zone" "$RUN/lez/session-a" "$RUN/lez/session-b" "$RUN/lez/inputs"
  trap zone_cleanup EXIT
  log "zone-bridge $NAME: bundle sha256 $(sha256sum "$BUNDLE" | cut -c1-64)"
  zstd -q -d -c "$BUNDLE" | tar -C "$W/bundle" -xf -
  ( cd "$W/bundle" && sha256sum -c --quiet SHA256SUMS ) || die "bundle SHA256SUMS"
  local B="$W/bundle"; local BIN="$B/bin" NODE="$B/node/logos-blockchain-node"
  chmod +x "$BIN"/* "$NODE"
  export PATH="$BIN:$PATH" RISC0_SERVER_PATH="$BIN/r0vm" XDG_CACHE_HOME="$B/xdg-cache"
  for p in ferrata_lez ferrata_bridge; do log "LEZ program $p.bin sha256 $(sha256sum "$B/program/$p.bin" | cut -c1-64)"; done
  log "crown-driver (this tree) sha256 $(sha256sum "$DRV" | cut -c1-64)"

  # -- the node: upstream's Bedrock configuration, as attestor/zone.sh (its deployment settings = the bundle's lez-config)
  cp "$LEZ/bedrock/node-config.yaml" "$LEZ/bedrock/deployment-settings.yaml" "$W/bedrock/"
  sed -i -e "s#listen_address: 0.0.0.0:18080#listen_address: 127.0.0.1:$PORT#" \
         -e 's#^    swarm:$#    swarm:\n      host: 127.0.0.1#' "$W/bedrock/node-config.yaml"
  cmp -s "$W/bedrock/deployment-settings.yaml" "$B/lez-config/deployment-settings.yaml" || die "deployment settings differ from the bundle's"
  ( cd "$W/bedrock" && POL_PROOF_DEV_MODE=true exec "$NODE" node-config.yaml --deployment deployment-settings.yaml ) > "$W/logs/bedrock.log" 2>&1 &
  NODE_PID=$!
  local i k
  for i in $(seq 1 600); do curl -fsS -m 3 -o /dev/null "$API/cryptarchia/info" 2>/dev/null && break; kill -0 "$NODE_PID"; sleep 0.2; done
  log "node up: $(curl -fsS -m 3 "$API/cryptarchia/info")"
  local last now_slot seen=0 v; ORIGIN=""; last="$(curl -fsS -m 3 "$API/cryptarchia/info" | sed -n 's/.*"slot":\([0-9]*\).*/\1/p')"
  for i in $(seq 1 900); do
    now_slot="$( { curl -fsS -m 3 "$API/cryptarchia/info" 2>/dev/null || true; } | sed -n 's/.*"slot":\([0-9]*\).*/\1/p')"
    if [ -n "$now_slot" ] && [ -n "$last" ] && [ "$now_slot" != "$last" ] && [ "$now_slot" -gt 0 ]; then
      v=$(( $(date +%s) - now_slot )); if [ -z "$ORIGIN" ] || [ "$v" -lt "$ORIGIN" ]; then ORIGIN="$v"; fi
      seen=$((seen + 1)); [ "$seen" -ge 3 ] && break
    fi
    last="$now_slot"; sleep 0.1
  done
  [ -n "$ORIGIN" ] || die "could not read the node's slot clock"
  log "node slot 0 is unix second $ORIGIN"

  # -- four sequencers: upstream's devnet configuration and keys; the genesis adds local.json's two funded test accounts
  #    (the devnet's own accounts hold pre-fee balances, 10000 and 20000, below one fee reservation)
  echo 4 > "$W/nseq"
  for i in 0 1 2 3; do
    mkdir -p "$W/seq-$i"; cp "$DEV/seq-$i/channel_signing_key" "$W/seq-$i/channel_signing_key"
    write_cfg "$i"
    local flag=""; [ "$i" = 0 ] && flag="--create-channel"
    sequencer_service setup "$W/seq-$i" --config "$W/cfg-$i.json" --funding-public-key "$FUND" $flag > "$W/logs/setup-$i.log" 2>&1 \
      || { tail -n 20 "$W/logs/setup-$i.log"; die "setup $i"; }
    log "seq-$i set up: channel key $(grep -o 'channel signing public key *[0-9a-f]*' "$W/logs/setup-$i.log" | awk '{print $NF}')"
  done
  jq '.genesis' "$W/cfg-0.json" > "$RUN/zone/genesis.json"
  seq_aligned 0
  for i in $(seq 1 150); do
    curl -fsS -m 5 "$API/channel/$CHANNEL" 2>/dev/null | grep -q '"accredited_keys":\["' && break; sleep 2
  done
  for i in 1 2 3; do seq_aligned "$i"; done
  for i in $(seq 1 120); do
    k="$(curl -fsS -m 5 "$API/channel/$CHANNEL" 2>/dev/null | jq -r '.accredited_keys | length' 2>/dev/null || echo 0)"
    [ "$k" = 4 ] && break; sleep 5
  done
  [ "$k" = 4 ] || die "the channel does not accredit four keys"
  FROM="$(curl -fsS -m 5 "$API/channel/$CHANNEL" | jq -r .tip_slot)"; FROM=$(( FROM > 600 ? FROM - 600 : 0 ))
  echo "$FROM" > "$W/slot-from.txt"
  for i in $(seq 1 60); do
    h="$(heights)"; [ "$(echo "$h" | tr '/' '\n' | sort -u | wc -l)" = 1 ] && [ "${h%%/*}" -ge 2 ] 2>/dev/null && break; sleep 5
  done
  log "four keys accredited; heights $(heights); lib slot $(lib_slot)"

  # -- the poller: the channel and the heights every 20 s (zone.sh's table)
  ( while :; do
      ch="$(curl -fsS -m 5 "$API/channel/$CHANNEL" 2>/dev/null || echo '{}')"
      echo "[$(date -u +%H:%M:%S)] keys $(echo "$ch" | jq -r '(.accredited_keys | length)' 2>/dev/null) tip_sequencer $(echo "$ch" | jq -r '.tip_sequencer // "-"' 2>/dev/null | cut -c1-8) tip_slot $(echo "$ch" | jq -r '.tip_slot // "-"' 2>/dev/null) | heights $(heights) | lib $(lib_slot)"
      sleep 20
    done ) > "$W/logs/poll.log" 2>&1 &
  LOOP_PID+=($!)

  # -- a 0.3.5-rc.1 node (LEZ v0.4.0-rc3) answers GET /chain/fork: its digest is the configuration's bedrockGenesisId
  local DIGEST="" FD=()
  if curl -fsS -m 10 -o "$W/chain-fork.json" "$API/chain/fork" 2>/dev/null; then
    DIGEST="$(jq -r .fork_digest "$W/chain-fork.json" | sed 's/^0x//' | tr 'A-F' 'a-f')"
    [[ "$DIGEST" =~ ^[0-9a-f]{64}$ ]] || die "/chain/fork: $(cat "$W/chain-fork.json")"
    cp "$W/chain-fork.json" "$RUN/zone/chain-fork.json"; FD=(--fork-digest "$DIGEST")
    log "node: Logos Blockchain with a fork digest (GET /chain/fork): $DIGEST"
  fi

  # -- the stand-in beacon chain's bootstrap slot: a zone that stamps its blocks with the wall clock (LEZ v0.4.0-rc3) takes
  #    the instance's Init only inside its checkpoint's weak-subjectivity period, and crown-driver's default slot (Sepolia
  #    period 1409) left it on 25 November 2026; as CI's sequencer round trip (scripts/ci/sequencer-roundtrip-hook.sh):
  #    64 slots into the current Sepolia period, or the previous one in the period's first 600 slots
  if [ -z "${CROWN_STAND_IN_SLOT:-}" ]; then
    local now_slot period; now_slot=$(( ($(date +%s) - 1655733600) / 12 )); period=$(( now_slot / 8192 ))
    if [ $(( now_slot - period * 8192 )) -ge 600 ]; then CROWN_STAND_IN_SLOT=$(( period * 8192 + 64 )); else CROWN_STAND_IN_SLOT=$(( (period - 1) * 8192 + 64 )); fi
    export CROWN_STAND_IN_SLOT
  fi
  log "the stand-in beacon chain's bootstrap slot: $CROWN_STAND_IN_SLOT (CROWN_STAND_IN_SLOT)"

  # -- the selected token's arguments, before session A: session A's identity records the configuration's salt, which
  # commits to its caps, and the caps are sized from the token. On a fresh local anvil the deployer's nonce is 0, so
  # the test token is its creation 4 (verifier, stop, registry, vault, token); the anvil section checks that.
  local DEPLOYER TOKEN_ADDR=0x0000000000000000000000000000000000000000 TOKEN_ARGS=(--token "$SELECTED_TOKEN")
  DEPLOYER="$(cast to-check-sum-address "0x$(cast keccak "${ZB_DEPLOYER_LABEL:-ferrata/zone-bridge/anvil-deployer}" | cut -c27-66)")"
  if [ "$SELECTED_TOKEN" = test-token ]; then
    TOKEN_ADDR="$(cast compute-address "$DEPLOYER" --nonce 4 | awk '{print $NF}')"
    TOKEN_ARGS+=(--token-address "$TOKEN_ADDR")
  fi

  # -- session A on the zone: crown-driver against seq-0 (transactions gossip to the turn's sequencer)
  local D=(--rpc "http://127.0.0.1:19300" --run-dir "$W/driver" --program-bin "$B/program/ferrata_lez.bin"
           --bridge-bin "$B/program/ferrata_bridge.bin" --chain stand-in --fixture "$B/fixture" --sequencer-log "$W/logs/seq-0.log"
           "${TOKEN_ARGS[@]}")
  curl -fsS -m 5 -o "$W/channel.json" "$API/channel/$CHANNEL"
  log "crown-a: $(jq -c '{keys: (.accredited_keys | length), posting_timeframe, configuration_threshold}' "$W/channel.json")"
  export FERRATA_DROP_AFTER_BLOCKS=40
  timeout 7200 env -u RISC0_DEV_MODE "$DRV" crown-a "${D[@]}" --channel "$W/channel.json" \
    --genesis "$W/bedrock/deployment-settings.yaml" "${FD[@]}" --identity-out "$W/session-a.json" > "$W/logs/driver-a.log" 2>&1 \
    || { tail -n 25 "$W/logs/driver-a.log" | scrub; die "crown-a"; }
  tail -n 4 "$W/logs/driver-a.log" | scrub | tee -a "$RUN/logs/zone-bridge.log"
  cp "$W/session-a.json" "$W/channel.json" "$RUN/lez/session-a/"; cp "$W/bedrock/deployment-settings.yaml" "$RUN/lez/session-a/"
  if [ -n "$DIGEST" ]; then
    [ "$(jq -r .bedrockGenesisId "$W/session-a.json" | sed 's/^0x//')" = "$DIGEST" ] || die "session A's bedrockGenesisId is not the fork digest"
  fi
  local T0 T0H KEYS PT CTH; T0="$(jq -r .tip.id "$W/session-a.json")"; T0H="$(jq -r .tip.hash "$W/session-a.json")"
  KEYS="$(jq -r '.channel.keys | join(",")' "$W/session-a.json")"; PT="$(jq -r .channel.postingTimeframe "$W/session-a.json")"
  CTH="$(jq -r .channel.configTipHash "$W/session-a.json")"
  log "T0 = $T0 $T0H; key set n = $(jq -r '.channel.keys | length' "$W/session-a.json"), posting timeframe $PT"

  # -- the Ethereum side: a local anvil (no fork), the vault over the zone's four keys, two deposits and their captures
  anvil --chain-id "$CHAIN_ID" --port 8545 --gas-limit 100000000 --auto-impersonate \
    --dump-state "$W/anvil-state.json" --state-interval 15 > "$W/logs/anvil.log" 2>&1 &
  ANVIL_PID=$!
  for i in $(seq 1 60); do cast chain-id --rpc-url "$R" > /dev/null 2>&1 && break; sleep 1; done
  local N VAULT CID GS Z=0x0000000000000000000000000000000000000000000000000000000000000000
  cast rpc anvil_setBalance "$DEPLOYER" 0x3635C9ADC5DEA00000 --rpc-url "$R" > /dev/null
  N="$(cast nonce "$DEPLOYER" --rpc-url "$R")"
  VAULT="$(cast compute-address "$DEPLOYER" --nonce $(( N + 3 )) | awk '{print $NF}')"   # verifier N, stop N+1, registry N+2
  if [ "$SELECTED_TOKEN" = test-token ]; then
    # token N+4; session A was given the address at nonce 0 + 4, and stage deploy checks it against its actual record
    [ "$(cast compute-address "$DEPLOYER" --nonce $(( N + 4 )) | awk '{print $NF}')" = "$TOKEN_ADDR" ] \
      || die "the deployer's nonce is $N, not 0: the test token would not be at $TOKEN_ADDR, session A's"
  fi
  CID="$("$DRV" config-id --identity "$W/session-a.json" --vault "$VAULT" "${TOKEN_ARGS[@]}" | jq -r .configId)"
  GS="$(python3 -I "$REPO/e2e/crown/identity.py" genesis-state --tip-id "$T0" --tip-hash "$T0H" --keys "$KEYS" \
        --posting-timeframe "$PT" --config-tip-hash "$CTH")"
  log "anvil (local, chain id $CHAIN_ID, no fork): deployer $DEPLOYER (impersonated) nonce $N; vault $VAULT; configId $CID; genesisState $GS"
  jq -n --arg v "$VAULT" --arg c "$CID" --arg g "$GS" --argjson ch "$CHAIN_ID" --arg d "$DEPLOYER" --arg f "$DIGEST" '{label:
    "STAND-IN Ethereum side: a local anvil (no fork, Sepolia chain id because crown-driver pins it), W and the release delay warped; the zone, its programs and proofs are real",
    chainId: $ch, vault: $v, configId: $c, genesisState: $g, delta: 3600, deployer: $d} + (if $f == "" then {} else {forkDigest: $f} end)' > "$RUN/identity.json"
  if [ "$SELECTED_TOKEN" = test-token ]; then
    jq --arg t "$TOKEN_ADDR" '. + {selectedToken: {name: "test-token", address: $t, denominations: [100000000,1000000000,10000000000],
      relayFees: [250000,2500000,25000000], checkpointFees: [100000,1000000,10000000], lezUnit: 1, unitWeight: 1, decimals: 6}}' \
      "$RUN/identity.json" > "$RUN/identity.json.tmp" && mv "$RUN/identity.json.tmp" "$RUN/identity.json"
  fi
  jq -n --arg c "$CID" --arg g "$GS" --arg a "$(id_of "${ZB_CHECKPOINT_A:-checkpoint}")" --arg b "$(id_of "$CKB")" --arg r "$(id_of release)" \
    --arg z "$Z" --argjson nm "${ZB_N_MIN:-4}" --argjson tm "${ZB_T_MIN:-3}" --arg lez "$(basename "$LEZ")" \
    '{_source: ("zone-bridge (checkpoint/attestor/zone-bridge.sh): the zone of four sequencers (" + $lez + "), nMin \($nm), tMin \($tm)"),
    checkpointImageIdA: $a, checkpointImageIdB: $b, releaseImageId: $r, lezConfigId: $c, genesisState: $g, nMin: $nm, tMin: $tm,
    postingTimeframe: 0, ethLezDefinition: $z, tokenLezDefinition: $z}' > "$RUN/params.json"
  if [ "$SELECTED_TOKEN" = test-token ]; then
    jq --arg t "$TOKEN_ADDR" '. + {selectedToken: {name: "test-token", address: $t, registryDefinition: "tokenLezDefinition"}}' \
      "$RUN/params.json" > "$RUN/params.json.tmp" && mv "$RUN/params.json.tmp" "$RUN/params.json"
  fi
  local d0 d1 C0 C1 base slots sargs=()
  d0="$("$DRV" destinations | jq -r .public0)"; d1="$("$DRV" destinations | jq -r .private0)"
  C0="$(python3 -I "$REPO/e2e/crown/identity.py" commitment --chain-id "$CHAIN_ID" --vault "$VAULT" --depositor "$DEPLOYER" --j 0 --kind public --dest "$d0")"
  C1="$(python3 -I "$REPO/e2e/crown/identity.py" commitment --chain-id "$CHAIN_ID" --vault "$VAULT" --depositor "$DEPLOYER" --j 1 --kind private --dest "$d1")"
  jq -n --arg v "$VAULT" --arg c0 "$C0" --arg c1 "$C1" --argjson ch "$CHAIN_ID" '{chainId: $ch, vault: $v,
    deposits: [{j: 0, denomIndex: 0, commitment: $c0}, {j: 1, denomIndex: 0, commitment: $c1}]}' > "$RUN/crown.json"
  if [ "$SELECTED_TOKEN" = test-token ]; then
    jq --arg t "$TOKEN_ADDR" '.deposits |= map(. + {token: $t})' "$RUN/crown.json" > "$RUN/crown.json.tmp" && mv "$RUN/crown.json.tmp" "$RUN/crown.json"
  fi
  base="$(cast block-number --rpc-url "$R")"
  slots="$("$DRV" capture-slots "${TOKEN_ARGS[@]}" | jq -r '.slots[].slot')"; for s in $slots; do sargs+=(--slot "$s"); done
  local S="$REPO/e2e/crown/sepolia.sh"
  export SEPOLIA_RPC_URL="$R" ETH_FROM="$DEPLOYER" FERRATA_UNLOCKED=1 FERRATA_NONCE_GAP=0
  "$S" "$RUN" deploy | tee -a "$RUN/logs/zone-bridge.log"
  if [ "$SELECTED_TOKEN" = test-token ]; then
    local RECORDED_TOKEN; RECORDED_TOKEN="$(jq -r .token "$RUN/deploy.json")"
    [ "${RECORDED_TOKEN,,}" = "${TOKEN_ADDR,,}" ] || die "deployed token $RECORDED_TOKEN is not CREATE deployer nonce N+4 ($TOKEN_ADDR)"
    TOKEN_ADDR="$RECORDED_TOKEN" # crown.json's deposits carry the deployment record's actual token address
  fi
  "$S" "$RUN" deposit 0 | tee -a "$RUN/logs/zone-bridge.log"
  python3 -I "$REPO/lez/crown/capture-anvil.py" --rpc "$R" --vault "$VAULT" --from-block "$base" "${sargs[@]}" "$W/inputs/capture-1" | tee -a "$RUN/logs/zone-bridge.log"
  "$S" "$RUN" deposit 1 | tee -a "$RUN/logs/zone-bridge.log"
  python3 -I "$REPO/lez/crown/capture-anvil.py" --rpc "$R" --vault "$VAULT" --from-block "$base" "${sargs[@]}" "$W/inputs/capture-2" | tee -a "$RUN/logs/zone-bridge.log"
  "$S" "$RUN" status | tee -a "$RUN/logs/zone-bridge.log"
  cp -r "$W/inputs/capture-1" "$W/inputs/capture-2" "$RUN/lez/inputs/"
  "$DRV" check "${TOKEN_ARGS[@]}" --chain stand-in --program-bin "$B/program/ferrata_lez.bin" --bridge-bin "$B/program/ferrata_bridge.bin" \
    --capture "$W/inputs/capture-1" --capture "$W/inputs/capture-2" --identity "$W/session-a.json" > "$W/logs/driver-check.log" 2>&1 \
    || { tail -n 20 "$W/logs/driver-check.log" | scrub; die "crown-driver check of the captures"; }
  tail -n 3 "$W/logs/driver-check.log" | scrub | tee -a "$RUN/logs/zone-bridge.log"

  # -- an attestor beside each sequencer: its key certified by that sequencer's channel key, for this deployment
  jq -n --arg c "$(printf '%064x' "$CHAIN_ID")" --arg v "$(echo "$VAULT" | tr 'A-F' 'a-f')" --arg l "${CID#0x}" --arg ch "$CHANNEL" \
    '{evm_chain_id: $c, vault: $v, lez_config_id: $l, channel_id: $ch}' > "$W/deployment.json"
  cp "$W/deployment.json" "$RUN/zone/deployment.json"
  for i in 0 1 2 3; do start_attestor "$i"; done
  log "four attestors started (a round every $ROUND_S s; attestation keys from the OS's randomness, never written to the repository)"

  # -- a rotation by the zone's committee (ZB_ROTATE=1), before session B: its blocks follow T0 like the bridge's
  if [ "${ZB_ROTATE:-0}" = 1 ]; then rotate_committee; fi

  # -- session B on the zone: the bridge's pairing, registration, imports, the claims and the burns
  timeout 7200 env -u RISC0_DEV_MODE RISC0_SERVER_PATH="$BIN/r0vm" nice -n 10 "$DRV" crown-b "${D[@]}" \
    --identity "$W/session-a.json" --capture "$W/inputs/capture-1" --capture "$W/inputs/capture-2" --private real \
    --session-out "$W/session-b.json" --notes-out "$W/burn-notes.json" > "$W/logs/driver-b.log" 2>&1 \
    || { tail -n 30 "$W/logs/driver-b.log" | scrub; die "crown-b"; }
  tail -n 12 "$W/logs/driver-b.log" | scrub | tee -a "$RUN/logs/zone-bridge.log"
  cp "$W/session-b.json" "$W/burn-notes.json" "$RUN/lez/session-b/"
  log "session B: T1 = $(jq -r .tipT1.id "$W/session-b.json"); steps $(jq -c '[.steps[] | [.step, .status, .block]]' "$W/session-b.json" | cut -c1-600)"
  "$DRV" export "${D[@]}" --out "$W/export" > "$W/logs/export.log" 2>&1 || log "export failed ($?)"
  local NS; NS="$(cat "$W/nseq")"

  # -- the final tier, signed while the sequencers still run: each attestor asks its own sequencer's LEZ node, and a
  #    stopped node answers nothing (nor finalizes more blocks). The loops end after the round in hand; then every
  #    attestor's round runs side by side, again until all sign one tip at or above T1; then the sequencers stop.
  local T1; T1="$(jq -r .tipT1.id "$W/session-b.json")"
  for p in "${LOOP_PID[@]}"; do kill -TERM "$p" 2>/dev/null || true; done
  for p in "${LOOP_PID[@]}"; do wait "$p" 2>/dev/null || true; done; LOOP_PID=()
  final_rounds "$NS" "$T1"
  local vargs=()
  for i in $(seq 0 $(( NS - 1 ))); do
    tail -n 1 "$W/att-$i/attest.log" | scrub | tee -a "$RUN/logs/zone-bridge.log"
    vargs+=(--att "$W/att-$i/final.json")
  done
  "$ATT" verify --deployment "$W/deployment.json" "${vargs[@]}" | scrub | tee -a "$RUN/logs/zone-bridge.log"
  for i in $(seq 0 $(( NS - 1 ))); do stop_pid "${SEQ_PID[$i]}" "seq-$i"; done; SEQ_PID=()
  log "sequencers stopped; waiting for the node's LIB to pass the channel's tip"
  local tip_slot lib=""
  tip_slot="$(curl -fsS -m 5 "$API/channel/$CHANNEL" | jq -r .tip_slot)"
  for i in $(seq 1 120); do lib="$(lib_slot)"; [ -n "$lib" ] && [ "$lib" -ge "$tip_slot" ] && break; sleep 2; done
  log "channel tip slot $tip_slot, lib $lib"
  for i in $(seq 0 $(( NS - 1 ))); do
    cp "$W/att-$i/final.json" "$RUN/zone/attestor-$i/final.json"; cp "$W/att-$i/rounds.tsv" "$RUN/zone/attestor-$i/rounds.tsv"
    jq '{tip_id, tip_hash, key_set_hash, node}' "$W/att-$i/final.json" > /dev/null
    scrub < "$W/att-$i/attest.log" | tail -n 400 > "$RUN/zone/attestor-$i/attest.log"
    ls "$W/att-$i/rounds" > "$RUN/zone/attestor-$i/tips-signed.txt"
    cp "$W/att-$i/resources.tsv" "$RUN/zone/attestor-$i/resources.tsv" 2>/dev/null || true
    local first; first="$(ls "$W/att-$i/rounds" | sed -n 's/^tip-\([0-9]*\)\.json$/\1/p' | sort -n | head -n 1)"
    [ -n "$first" ] && cp "$W/att-$i/rounds/tip-$first.json" "$RUN/zone/attestor-$i/first-round.json"
  done

  # -- the node's answers (the checkpoint's input is built from these alone): read from the first slot the attestors
  #    read to the node's LIB, after the sequencers stopped and the LIB passed the channel's tip
  save_answers
  pack_answers
  [ -d "$W/export" ] && tar -C "$W" -cf - export | zstd -q -T0 -19 -o "$RUN/lez/session-b/driver-export.tar.zst" -f
  cp "$W/phases.txt" "$RUN/zone/phases.txt"
  cp "$W/driver/manifest.tsv" "$RUN/lez/manifest.tsv" 2>/dev/null || true
  rm -f "$REPO/vault/deployments/crown-$NAME.json"   # DeployVault's record, kept as $RUN/deploy.json
  log "zone stage done"
}

records_final() { # every file of the run (logs/, net/ where the proxy logs, zone/ ...; not the ignored work/), after
  # every stage, failed or not: scrub's rules applied, then refused (exit 1) if any file still names a home directory,
  # this machine's spike/ or an agent's folder (scripts/scrub_records.py; review M1: net/proxy.log was not scrubbed)
  python3 -I "$REPO/scripts/scrub_records.py" --skip work "$RUN" --map "$W/=work/" --map "$T/=tmp/" --map "$REPO/=" \
    --scratch "$SPIKE/tmp=tmp/" --map "$SPIKE/=spike/" --map "$HOME/=~/"
}

pack_answers() { # $W/node (save_answers) -> $RUN/zone/node-answers.tar.zst (as node/: info.json, channel.json, blocks/)
  local lib
  [ -s "$W/node/info.json" ] && [ -s "$W/node/channel.json" ] && ls "$W"/node/blocks/*.json > /dev/null 2>&1 || die "no saved answers in $W/node"
  tar -C "$W" --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 -cf - node | zstd -q -T0 -19 -o "$RUN/zone/node-answers.tar.zst" -f
  lib="$(sed -n 's/.*"lib_slot":\([0-9]*\).*/\1/p' "$W/node/info.json")"
  log "node answers (read after the sequencers stopped): slots from $(cat "$W/slot-from.txt") to the LIB $lib, $(ls "$W/node/blocks" | wc -l) windows, $(cat "$W"/node/blocks/*.json | wc -c) bytes ($(stat -c %s "$RUN/zone/node-answers.tar.zst") compressed)"
  cp "$W/node/channel.json" "$RUN/zone/channel-end.json"; cp "$W/node/info.json" "$RUN/zone/info-end.json"
}

# ---- input ------------------------------------------------------------------------------------------------------
node_dir() { # the node's answers, unpacked from the run's record
  local d="$T/node-$NAME"; rm -rf "$d"; mkdir -p "$d"; zstd -q -d -c "$RUN/zone/node-answers.tar.zst" | tar -C "$d" -xf -; echo "$d/node"
}
tip_of_attestors() { jq -r .tip_id "$RUN/zone/attestor-0/final.json"; }
rc_args() { # realblocks-crown's arguments over the run's records (attestation files in $1, comma-separated)
  printf '%s\0' --node "$2" --genesis "$RUN/lez/session-a/deployment-settings.yaml" --session-a "$RUN/lez/session-a/session-a.json" \
    --vault "$(jq -r .vault "$RUN/identity.json")" --config-id "$(jq -r .configId "$RUN/identity.json")" \
    --genesis-state "$(jq -r .genesisState "$RUN/identity.json")" --tip "$(tip_of_attestors)" --n-min "${ZB_N_MIN:-4}" --t-min "${ZB_T_MIN:-3}" --att "$1"
}
n_att() { find "$RUN/zone" -maxdepth 1 -name 'attestor-*' -type d | wc -l; }
t_of() { python3 -I -c 'import sys; n = int(sys.argv[1]); print(max(1, min(-(-2 * n // 3), n - 1)))' "$1"; }
# The attestor left out: the last by default; after a rotation (ZB_ROTATE=1) the first, because the last is the key the
# rotation accredited, whose earliest round predates its accreditation, and stage input's stale-tip refusal needs a real
# earlier-round attestation by a key of the span's parent set.
left_out() {
  if [ -n "${ZB_LEAVE_OUT:-}" ]; then echo "$ZB_LEAVE_OUT"
  elif [ -f "$RUN/zone/channel-after-rotation.json" ]; then echo 0
  else echo $(( $(n_att) - 1 )); fi
}
all_but_one() { local i out=""; for i in $(seq 0 $(( $(n_att) - 1 ))); do [ "$i" = "$(left_out)" ] && continue; out="$out,$RUN/zone/attestor-$i/final.json"; done; echo "${out#,}"; }
cname() { echo "zone-$(jq -r .tip.id "$RUN/lez/session-a/session-a.json")-$(tip_of_attestors)"; }

stage_input() {
  local C="$RUN/checkpoint" nd n i; mkdir -p "$C"; nd="$(node_dir)"; n="$(cname)"
  for i in $(seq 1 $(( $(n_att) - 1 ))); do
    [ "$(jq -r '.tip_id, .tip_hash' "$RUN/zone/attestor-$i/final.json")" = "$(jq -r '.tip_id, .tip_hash' "$RUN/zone/attestor-0/final.json")" ] \
      || die "attestor $i signed another final tip than attestor 0"
  done
  local args; mapfile -d '' args < <(rc_args "$(all_but_one)" "$nd")
  "$RC" "${args[@]}" --out "$C" --name "$n" | tee "$C/realblocks-crown.txt"
  local t1; t1="$(jq -r .tipT1.id "$RUN/lez/session-b/session-b.json")"
  [ "$(jq -r .tip "$C/$n.json")" -ge "$t1" ] || die "the attested tip is below session B's T1 ($t1)"
  [ "$(jq -r .burn_count "$C/burns.json")" = 2 ] || die "burns.json does not hold the session's two burns"
  # all n, and t - 1 of n, over the same base (the collector replaces the attestations); their inputs stay in $T
  # (re-derived from the base and the records by the same commands), their metas are recorded
  local na tt all=() below=()
  na="$(n_att)"; tt="$(t_of "$na")"
  for i in $(seq 0 $(( na - 1 ))); do all+=(--att "$RUN/zone/attestor-$i/final.json"); done
  for i in $(seq 0 $(( tt - 2 ))); do below+=(--att "$RUN/zone/attestor-$i/final.json"); done
  "$ATT" collect --base "$C/$n.input" "${all[@]}" --out "$T/$n-all.input" --meta "$C/$n-all.json" --name "$n-all" | tee "$C/collect-all.txt"
  if "$ATT" collect --base "$C/$n.input" "${below[@]}" --out "$T/below.input" --meta "$T/below.json" --name "$n-below" \
      > "$C/collect-below.txt" 2>&1; then die "$(( tt - 1 )) of $na was accepted"; fi
  echo "$(( tt - 1 )) of $na: refused by the collector: $(tail -n 1 "$C/collect-below.txt")" | tee -a "$C/collect-below.txt"
  "$ATT" assemble --base "$C/$n.input" "${below[@]}" --out "$T/$n-below.input" --meta "$C/$n-below.json" --name "$n-below" \
    --expect InsufficientAttestations | tee -a "$C/collect-below.txt"
  "$E" exec --program "$PROGRAMS/$CKB.bin" --input "$C/$n.input" --meta "$C/$n.json" | tee "$C/exec.json"
  jq -e '.journal_equals_native' "$C/exec.json" > /dev/null
  "$E" exec --program "$PROGRAMS/$CKB.bin" --input "$T/$n-all.input" --meta "$C/$n-all.json" | tee "$C/exec-all.json"
  # (the meta of an input the program must refuse has no journal; the executor is given a placeholder one)
  jq -n '{kind: "checkpoint-b", journal: "00"}' > "$T/two-exec-meta.json"
  if "$E" exec --program "$PROGRAMS/$CKB.bin" --input "$T/$n-below.input" --meta "$T/two-exec-meta.json" > "$C/exec-below.txt" 2>&1; then
    die "the guest accepted t - 1 attestations"; fi
  grep -o 'checkpoint refused: [^"]*' "$C/exec-below.txt" | head -n 1 | tee -a "$C/exec-below.txt"
  # RFP-022 F.7's refusals over the run's real attestations (checkpoint/attestor's attestor-refusals --real): t - 1 of
  # them alone, with an attestation by a key outside the live set, with one of them twice, and with the earliest round
  # of the attestor left out (a real signature of an older tip); each refused natively (its meta) and by the guest
  local REF="$REPO/checkpoint/target/release/attestor-refusals" RD="$T/refusals-$NAME" stale=() f c want got
  [ -x "$REF" ] || die "no $REF (stage build)"
  [ -f "$RUN/zone/attestor-$(left_out)/first-round.json" ] && stale=(--stale "$RUN/zone/attestor-$(left_out)/first-round.json")
  rm -rf "$RD"; mkdir -p "$RD" "$C/refusals"
  "$REF" --base "$C/$n.input" --real "$(all_but_one)" "${stale[@]}" --out-dir "$RD" --prefix "$n-refuse" | tee "$C/refusals/native.txt"
  : > "$C/refusals/guest.txt"
  for f in "$RD"/*.input; do
    c="$(basename "$f" .input)"; cp "$RD/$c.json" "$C/refusals/$c.json"
    want="$(jq -r .expect_refusal "$RD/$c.json" | grep -oE 'InsufficientAttestations|NotAccredited|DuplicateAttestor|NotADescendant' | head -n 1)"
    [ -n "$want" ] || die "$c: no expected refusal in its meta"
    if "$E" exec --program "$PROGRAMS/$CKB.bin" --input "$f" --meta "$T/two-exec-meta.json" > "$RD/$c.exec.txt" 2>&1; then
      die "the guest accepted $c"; fi
    got="$(grep -o 'checkpoint refused: [^"]*' "$RD/$c.exec.txt" | head -n 1)"
    case "$got" in *"$want"*) ;; *) die "$c: the guest refused with '$got', the native run with $want" ;; esac
    echo "$c: native $(jq -r .native "$RD/$c.json"); guest $got" | tee -a "$C/refusals/guest.txt"
  done
}

# ---- prove ------------------------------------------------------------------------------------------------------
stage_prove() {
  local C="$RUN/checkpoint" n; n="$(cname)"
  mkdir -p "$C/receipts" "$W/work/checkpoint"
  "$E" prove --program "$PROGRAMS/$CKB.bin" --input "$C/$n.input" --meta "$C/$n.json" --out-dir "$C/receipts" | tee "$C/prove.json"
  "$E" verify --receipt "$C/receipts/receipt-succinct.bin" --image-id "$(id_of "$CKB")" --meta "$C/$n.json"
  "$E" p254 --receipt "$C/receipts/receipt-succinct.bin" --image-id "$(id_of "$CKB")" --out-dir "$W/work/checkpoint" | tee "$C/p254.txt"
  mkdir -p "$RUN/work/checkpoint"; cp "$W/work/checkpoint/input.json" "$RUN/work/checkpoint/input.json"
}

stage_releases() {
  local REL="$RUN/releases" f p
  mkdir -p "$REL"
  "$E" crown-release --identity "$RUN/identity.json" --burns "$RUN/checkpoint/burns.json" --notes "$RUN/lez/session-b/burn-notes.json" --out-dir "$REL"
  for f in "$REL"/release-*.input; do
    p="$(basename "$f" .input)"
    "$E" exec --program "$PROGRAMS/release.bin" --input "$f" --meta "$REL/$p.json" > "$REL/$p.exec.json"
    # ZB_RELEASES_PROVE=0: the statements and their executor runs only; the proofs are made elsewhere (a GPU through
    # the bridge's prover client: deploy/testnet/tests/prover-kill-resume-modal.sh, then receipts/<p>/seal.hex,
    # journal.hex and prover.json)
    [ "${ZB_RELEASES_PROVE:-1}" = 0 ] && continue
    "$E" prove --program "$PROGRAMS/release.bin" --input "$f" --meta "$REL/$p.json" --out-dir "$REL/receipts/$p" | tee "$REL/$p.prove.json"
    mkdir -p "$W/work/$p" "$RUN/work/$p"
    "$E" p254 --receipt "$REL/receipts/$p/receipt-succinct.bin" --image-id "$(id_of release)" --out-dir "$W/work/$p"
    cp "$W/work/$p/input.json" "$RUN/work/$p/input.json"
  done
}

stage_groth16() {
  local p="${3:?proof: checkpoint | release-<name>}" succ d meta img
  case "$p" in
    checkpoint) succ="$RUN/checkpoint/receipts/receipt-succinct.bin"; d="$RUN/checkpoint/receipts"; meta="$RUN/checkpoint/$(cname).json"; img="$(id_of "$CKB")" ;;
    release-*) succ="$RUN/releases/receipts/$p/receipt-succinct.bin"; d="$RUN/releases/receipts/$p"; meta="$RUN/releases/$p.json"; img="$(id_of release)" ;;
    *) die "groth16: checkpoint or release-<name>" ;;
  esac
  "$E" groth16 --receipt "$succ" --image-id "$img" --proof "$RUN/work/$p/proof.json" --out-dir "$d"
  "$E" verify --receipt "$d/receipt-groth16.bin" --image-id "$img" --meta "$meta"
}

# ---- gpu-jobs, gpu-seals: the proofs on a GPU through the bridge's prover client ----------------------------------
# gpu-jobs writes one prover request per proof (the checkpoint, then each release statement of stage releases) into
# $T/gpu-<run>/; deploy/testnet/tests/prover-kill-resume-modal.sh $RUN/prover <those requests> proves them on Modal's GPU
# (the client killed once mid-proof and resumed); gpu-seals turns its answers into receipts/<proof>/seal.hex, journal.hex
# and prover.json, each journal held to the statement's own (the native one), for stages vault, fixture and verify.
stage_gpu_jobs() {
  local C="$RUN/checkpoint" n G="$T/gpu-$NAME/requests" m p
  n="$(cname)"; rm -rf "$G"; mkdir -p "$G"
  jq -n --arg i "$C/$n.input" --arg m "$C/$n.json" '{kind: "checkpoint-b", input: $i, meta: $m}' > "$G/0-checkpoint.json"
  for m in "$RUN"/releases/release-*.json; do
    case "$m" in *.exec.json|*.prove.json) continue ;; esac
    p="$(basename "$m" .json)"
    jq -n --arg i "$RUN/releases/$p.input" --arg m "$m" '{kind: "release", input: $i, meta: $m}' > "$G/1-$p.json"
  done
  ls "$G"
}
stage_gpu_seals() {
  local C="$RUN/checkpoint" n D="$RUN/prover/done" d p want
  n="$(cname)"
  [ -f "$D/0-checkpoint.json" ] || die "no answer $D/0-checkpoint.json (deploy/testnet/tests/prover-kill-resume-modal.sh)"
  for d in "$D"/*.json; do
    [ "$(jq -r .ok "$d")" = true ] || die "$d: not ok"
    case "$(basename "$d" .json)" in
      0-checkpoint) p="$C/receipts"; want="$(jq -r .journal "$C/$n.json")" ;;
      1-release-*) p="$RUN/releases/receipts/$(basename "$d" .json | sed 's/^1-//')"
                   want="$(jq -r .journal "$RUN/releases/$(basename "$d" .json | sed 's/^1-//').json")" ;;
      *) die "$d: not a request of gpu-jobs" ;;
    esac
    mkdir -p "$p"
    [ "$(jq -r .journal "$d" | sed 's/^0x//')" = "$(echo "$want" | sed 's/^0x//')" ] || die "$d: the proof's journal is not the statement's"
    jq -r .seal "$d" | sed 's/^0x//' > "$p/seal.hex"; jq -r .journal "$d" | sed 's/^0x//' > "$p/journal.hex"
    jq '{name, kind, image_id, ok, seconds, call_id, attempts, resumed, prover}' "$d" > "$p/prover.json"
    echo "$(basename "$d" .json): seal $(head -c 10 "$p/seal.hex")..., $(( $(tr -d '\n' < "$p/seal.hex" | wc -c) / 2 )) bytes, journal = the statement's"
  done
}

# ---- vault: the local anvil from its saved state ------------------------------------------------------------------
# A rerun of stage vault starts again from the zone stage's saved chain ($W/anvil-state.json), on which none of the
# stage's own transactions exist; an earlier attempt's (its manifest stages, their journal rows and logs) are moved to
# vault-attempt-<n>/, kept, so that the stage sends them again instead of skipping them as done or waiting for them.
vault_attempt_aside() {
  local stages a=1 f
  stages="$(jq -r '.stages | keys[] | select(. == "submit" or . == "finalizeAttested" or startswith("release-"))' \
    "$RUN/manifest.json" 2>/dev/null)"
  [ -n "$stages" ] || return 0
  while [ -e "$RUN/vault-attempt-$a" ]; do a=$((a + 1)); done
  mkdir -p "$RUN/vault-attempt-$a"
  cp "$RUN/manifest.json" "$RUN/net/journal.jsonl" "$RUN/vault-attempt-$a/"
  for f in $stages; do [ -f "$RUN/logs/$f.log" ] && mv "$RUN/logs/$f.log" "$RUN/vault-attempt-$a/"; done
  python3 -I - "$RUN/manifest.json" "$RUN/net/journal.jsonl" $stages <<'PY'
import json, sys
manifest, journal, stages = sys.argv[1], sys.argv[2], set(sys.argv[3:])
m = json.load(open(manifest))
for s in stages:
    m["stages"].pop(s, None)
json.dump(m, open(manifest, "w"), indent=2)
rows = [json.loads(line) for line in open(journal) if line.strip()]
gone = {r["hash"] for r in rows if r.get("event") == "signed" and r.get("stage") in stages}
with open(journal, "w") as out:
    for r in rows:
        if r.get("hash") not in gone:
            out.write(json.dumps(r, sort_keys=True) + "\n")
PY
  log "an earlier vault attempt's transactions ($(echo $stages)) are not on the chain this stage starts from: set aside in vault-attempt-$a/"
}

stage_vault() {
  [ -s "$W/anvil-state.json" ] || die "no saved anvil state at $W/anvil-state.json"
  vault_attempt_aside
  local C="$RUN/checkpoint" n rel="[]" e p VAULT
  n="$(cname)"; VAULT="$(jq -r .vault "$RUN/identity.json")"
  [ "$(hexfile "$C/receipts/journal.hex")" = "$(jq -r .journal "$C/$n.json")" ] || die "checkpoint journal != native"
  if [ -f "$RUN/releases/releases.json" ]; then
    rel="$(jq -c '.[]' "$RUN/releases/releases.json" | while read -r e; do
      p="release-$(jq -r .name <<< "$e")"
      [ "$(hexfile "$RUN/releases/receipts/$p/journal.hex")" = "$(jq -r .journal "$RUN/releases/$p.json")" ] || { echo "$p: journal" >&2; exit 1; }
      jq -c --arg s "0x$(hexfile "$RUN/releases/receipts/$p/seal.hex")" '.seal = $s' <<< "$e"
    done | jq -s -c .)"
  fi
  jq --arg j "0x$(hexfile "$C/receipts/journal.hex")" --arg s "0x$(hexfile "$C/receipts/seal.hex")" --argjson r "$rel" \
    '.candidate = {journal: $j, seal: $s} | .releases = $r' "$RUN/crown.json" > "$RUN/crown.json.tmp" && mv "$RUN/crown.json.tmp" "$RUN/crown.json"
  anvil --chain-id "$CHAIN_ID" --port 8545 --gas-limit 100000000 --auto-impersonate --load-state "$W/anvil-state.json" \
    --dump-state "$W/anvil-state-vault.json" > "$W/logs/anvil-vault.log" 2>&1 &
  ANVIL_PID=$!
  trap '[ -n "$ANVIL_PID" ] && kill -INT "$ANVIL_PID" 2>/dev/null; sleep 2' EXIT
  local i; for i in $(seq 1 60); do cast chain-id --rpc-url "$R" > /dev/null 2>&1 && break; sleep 1; done
  [ "$(cast call "$VAULT" 'genesisState()(bytes32)' --rpc-url "$R")" = "$(jq -r .genesisState "$RUN/identity.json")" ] || die "the restored anvil has no vault"
  export SEPOLIA_RPC_URL="$R" ETH_FROM="$(jq -r .deployer "$RUN/identity.json")" FERRATA_UNLOCKED=1 FERRATA_NONCE_GAP=0
  local S="$REPO/e2e/crown/sepolia.sh"
  "$S" "$RUN" submit
  cast rpc evm_increaseTime 3601 --rpc-url "$R" > /dev/null; cast rpc evm_mine --rpc-url "$R" > /dev/null
  log "W warped: 3601 s (anvil evm_increaseTime)"
  "$S" "$RUN" finalize
  local final; final="$(cast call "$VAULT" 'finalState()(bytes32)' --rpc-url "$R")"
  [ "$final" = "$(jq -r .new_state "$C/$n.json")" ] || die "finalState $final is not the journal's new state"
  local root; root="$(jq -r .burn_root "$C/$n.json")"
  log "finalState $final = the journal's new state; burnRootStatus($root) = $(cast call "$VAULT" 'burnRootStatus(bytes32)(bool,uint256)' "$root" --rpc-url "$R" | tr '\n' ' ')"
  cast rpc evm_increaseTime 121 --rpc-url "$R" > /dev/null; cast rpc evm_mine --rpc-url "$R" > /dev/null
  log "release delay warped: 121 s"
  local k nrel paid="[]" status; nrel="$(jq '.releases | length' "$RUN/crown.json")"
  status="$(cast call "$VAULT" 'burnRootStatus(bytes32)(bool,uint256)' "$root" --rpc-url "$R" | head -n 1)"
  [ "$status" = true ] || die "the burn root is not accepted"
  for k in $(seq 0 $(( nrel - 1 ))); do
    local rcpt nul before after used release_token
    rcpt="$(jq -r ".releases[$k].recipient" "$RUN/crown.json")"; nul="$(jq -r ".releases[$k].nullifier" "$RUN/crown.json")"
    release_token="$(jq -r ".releases[$k].token" "$RUN/crown.json")"
    if zero_address "$release_token"; then
      before="$(cast balance "$rcpt" --rpc-url "$R")"
      "$S" "$RUN" release "$k"
      after="$(cast balance "$rcpt" --rpc-url "$R")"; used="$(cast call "$VAULT" 'nullifierUsed(bytes32)(bool)' "$nul" --rpc-url "$R")"
      [ "$used" = true ] || die "release $k: the nullifier is not used"
      log "release $k: recipient $rcpt +$(python3 -I -c "print($after - $before)") wei; nullifierUsed $used"
      paid="$(jq -c --arg r "$rcpt" --arg n "$nul" --arg w "$(python3 -I -c "print($after - $before)")" '. + [{recipient: $r, nullifier: $n, paidWei: $w}]' <<< "$paid")"
    else
      local relayer relay_fee relayer_before relayer_after recipient_delta relayer_delta expected_recipient expected_relayer fee_payee
      relayer="$(jq -r ".releases[$k].relayer" "$RUN/crown.json")"; relay_fee="$(jq -r ".releases[$k].relayFee" "$RUN/crown.json")"
      fee_payee="$relayer"; zero_address "$relayer" && fee_payee="$ETH_FROM" # FerrataVault: zero relayer means msg.sender
      [ "$(jq -r ".releases[$k].denomIndex" "$RUN/crown.json")" = 0 ] || die "release $k: test-token must burn denomination 0"
      relay_fee="$(python3 -I -c "print(int('$relay_fee', 0))")"
      before="$(asset_balance "$release_token" "$rcpt")"; relayer_before="$(asset_balance "$release_token" "$fee_payee")"
      "$S" "$RUN" release "$k"
      after="$(asset_balance "$release_token" "$rcpt")"; relayer_after="$(asset_balance "$release_token" "$fee_payee")"
      used="$(cast call "$VAULT" 'nullifierUsed(bytes32)(bool)' "$nul" --rpc-url "$R")"
      [ "$used" = true ] || die "release $k: the nullifier is not used"
      recipient_delta="$(python3 -I -c "print($after - $before)")"; relayer_delta="$(python3 -I -c "print($relayer_after - $relayer_before)")"
      # Schedule 0: denomination 100e6, burn fee 0, checkpoint fee 100000. If relayer == recipient the two observed
      # deltas are the same aggregate balance delta; account for the relay fee exactly once in that case.
      expected_recipient=$(( 100000000 - 100000 - relay_fee )); expected_relayer="$relay_fee"
      if [ "${fee_payee,,}" = "${rcpt,,}" ]; then
        [ "$recipient_delta" = "$(( expected_recipient + expected_relayer ))" ] || die "release $k: recipient/relayer got $recipient_delta, expected $(( expected_recipient + expected_relayer ))"
      else
        [ "$recipient_delta" = "$expected_recipient" ] || die "release $k: recipient got $recipient_delta, expected $expected_recipient"
        [ "$relayer_delta" = "$expected_relayer" ] || die "release $k: relayer got $relayer_delta, expected $expected_relayer"
      fi
      log "release $k: token $release_token recipient $rcpt +$recipient_delta base units; fee payee $fee_payee fee $relay_fee checked; nullifierUsed $used"
      paid="$(jq -c --arg r "$rcpt" --arg n "$nul" --arg w "$recipient_delta" --arg t "$release_token" --arg rel "$relayer" --arg fee "$relay_fee" \
        '. + [{recipient: $r, nullifier: $n, paidWei: $w, token: $t, relayer: $rel, relayFeeUnits: $fee}]' <<< "$paid")"
    fi
  done
  # replays (nothing is paid twice): each release's and the checkpoint's mined calldata called again from its sender
  # (e2e/crown/sepolia.sh would skip a stage its journal holds, and Crown.s.sol one already on chain, so neither can
  # send a replay); each must revert with the vault's error for it (a spent nullifier; a parent no longer final), so
  # that an RPC failure or another refusal cannot pass for the replay's
  local replays="[]" why stage h input out
  for stage in $(seq 0 $(( nrel - 1 )) | sed 's/^/release-/') submit; do
    h="$(jq -r --arg s "$stage" '.stages[$s][-1].hash' "$RUN/manifest.json")"
    input="$(cast tx "$h" input --rpc-url "$R")"
    if out="$(cast call "$VAULT" "$input" --from "$ETH_FROM" --rpc-url "$R" 2>&1)"; then die "$stage called again was not refused"; fi
    why=NullifierAlreadyUsed; [ "$stage" = submit ] && why=ParentNotFinal
    grep -qi "$(cast sig "$why()" | sed 's/^0x//')" <<< "$out" || {
      echo "$out" > "$W/logs/replay-$stage.log"; die "$stage called again: refused, but not with $why (logs/replay-$stage.log)"; }
    log "$stage (transaction $h) called again: refused ($why)"
    replays="$(jq -c --arg w "$stage" --arg h "$h" --arg r "$why" '. + [{what: $w, transaction: $h, refused: $r}]' <<< "$replays")"
  done
  local final_token; final_token="$(jq -r '.releases[0].token // 0' "$RUN/crown.json")"
  if zero_address "$final_token"; then
    jq -n --arg f "$final" --arg root "$root" --argjson paid "$paid" --arg bal "$(cast balance "$VAULT" --rpc-url "$R")" \
      --argjson rp "$replays" '{finalState: $f, burnRoot: $root, burnRootAccepted: true, releases: $paid, replaysRefused: $rp, vaultBalanceWeiAfter: $bal}' > "$RUN/vault-checks.json"
  else
    jq -n --arg f "$final" --arg root "$root" --argjson paid "$paid" --arg bal "$(asset_balance "$final_token" "$VAULT")" --arg t "$final_token" \
      --argjson rp "$replays" '{finalState: $f, burnRoot: $root, burnRootAccepted: true, releases: $paid, replaysRefused: $rp,
      token: $t, vaultTokenBalanceAfter: $bal}' > "$RUN/vault-checks.json"
  fi
  "$S" "$RUN" status | tee -a "$RUN/logs/zone-bridge.log"
}

# ---- fixture: what vault/test/real/ZoneBridge.t.sol reads ---------------------------------------------------------
stage_fixture() {
  local C="$RUN/checkpoint" n A="$RUN/lez/session-a/session-a.json" out="$REPO/vault/test/real/fixtures/${ZB_FIXTURE:-$FIXTURE_DEFAULT}.json"
  n="$(cname)"
  jq -n --slurpfile m "$C/$n.json" --slurpfile a "$A" --slurpfile cj "$RUN/crown.json" --slurpfile id "$RUN/identity.json" \
    --slurpfile vc "$RUN/vault-checks.json" --arg j "0x$(hexfile "$C/receipts/journal.hex")" --arg s "0x$(hexfile "$C/receipts/seal.hex")" \
    --arg img "$(id_of "$CKB")" --arg rimg "$(id_of release)" --arg src "e2e/runs/$NAME" '
    ($m[0]) as $m | ($a[0]) as $a | ($cj[0]) as $cj | ($vc[0]) as $vc |
    {_source: ("checkpoint/attestor/zone-bridge.sh fixture over " + $src), imageId: $img, releaseImageId: $rimg,
     journal: $j, journalSha256: ("0x" + $m.journal_sha256), seal: $s,
     keys: $a.channel.keys, postingTimeframe: $a.channel.postingTimeframe, configTipHash: $a.channel.configTipHash,
     parentTipId: $a.tip.id, parentTipHash: $a.tip.hash, tipId: $m.tip, tipHash: $m.tip_hash,
     depositor: $id[0].deployer, commitments: [$cj.deposits[].commitment],
     releaseCount: ($cj.releases | length),
     releases: [$cj.releases | to_entries[] | .value + {paidOnAnvil: ($vc.releases[.key].paidWei | tonumber)}]}
    + (if ($cj.deposits[0].token // null) != null then {token: $cj.deposits[0].token} else {} end)
    + (if (($m.config_changes // []) | length) > 0 then {rotatedKeys: [$m.keys_after[] | "0x" + ltrimstr("0x")]} else {} end)' > "$out"
  [ "$(jq -r .journal "$out" | sed 's/^0x//')" = "$(jq -r .journal "$C/$n.json")" ] || die "fixture journal"
  echo "wrote $out"
}

# ---- verify -----------------------------------------------------------------------------------------------------
stage_verify() {
  local C="$RUN/checkpoint" n v="$T/verify-$NAME" nd args p m
  n="$(cname)"; rm -rf "$v"; mkdir -p "$v"; nd="$(node_dir)"
  mapfile -d '' args < <(rc_args "$(all_but_one)" "$nd")
  "$RC" "${args[@]}" --out "$v" --name "$n" > "$v/out.txt"
  cmp -s "$v/$n.input" "$C/$n.input" || die "the re-derived input differs"
  [ "$(jq -r .journal "$v/$n.json")" = "$(jq -r .journal "$C/$n.json")" ] || die "the re-derived journal differs"
  cmp -s "$v/burns.json" "$C/burns.json" || die "the re-derived burns.json differs"
  echo "checkpoint input, journal and burns re-derived from the recorded node answers and attestations: equal"
  "$E" image-id "$PROGRAMS/$CKB.bin" "$PROGRAMS/release.bin"
  if [ -f "$C/receipts/receipt-succinct.bin" ]; then
    "$E" verify --receipt "$C/receipts/receipt-succinct.bin" --image-id "$(id_of "$CKB")" --meta "$C/$n.json"
    "$E" verify --receipt "$C/receipts/receipt-groth16.bin" --image-id "$(id_of "$CKB")" --meta "$C/$n.json"
  else
    # proven on a GPU through the bridge's prover client (zone-bridge.sh gpu-jobs, then gpu-seals): the record holds the
    # 260-byte Groth16 seal and the journal; RISC Zero's verifier checks the seal in the vault stage and the fixture's tests
    [ -f "$C/receipts/prover.json" ] || die "no receipt and no prover record for the checkpoint"
    [ "$(hexfile "$C/receipts/journal.hex")" = "$(jq -r .journal "$C/$n.json")" ] || die "the GPU proof's journal is not the native one"
    echo "checkpoint: proven on the GPU (receipts/prover.json); its journal is the native one; its seal is checked by RISC Zero's verifier (stage vault)"
  fi
  for m in "$RUN"/releases/release-*.json; do
    case "$m" in *.exec.json|*.prove.json) continue ;; esac
    p="$(basename "$m" .json)"
    if [ -f "$RUN/releases/receipts/$p/receipt-succinct.bin" ]; then
      "$E" verify --receipt "$RUN/releases/receipts/$p/receipt-succinct.bin" --image-id "$(id_of release)" --meta "$m"
      "$E" verify --receipt "$RUN/releases/receipts/$p/receipt-groth16.bin" --image-id "$(id_of release)" --meta "$m"
    else
      [ -f "$RUN/releases/receipts/$p/prover.json" ] || die "$p: no receipt and no prover record"
      [ "$(hexfile "$RUN/releases/receipts/$p/journal.hex")" = "$(jq -r .journal "$m")" ] || die "$p: the GPU proof's journal"
      echo "$p: proven on the GPU (prover.json); its journal is the statement's; its seal is checked by the vault stage"
    fi
  done
  [ "$(jq -r .candidate.seal "$RUN/crown.json")" = "0x$(hexfile "$C/receipts/seal.hex")" ] && echo "crown.json: candidate seal = recorded GPU seal (byte comparison only; inputs/journals/burns re-derived above; RISC Zero checks seals in stage vault and the forge tests)"
}

# A stage runs in a subshell, so that its EXIT traps (zone_cleanup writes zone/logs/) have run before its records
# are scrubbed and checked; build and verify write no record and check none (their re-checks over the runs recorded
# before this check, zone-bridge-1, -2 and -rc3, whose forge logs name the worktree's folder, are unchanged)
set +e
( set -e
  case "$STAGE" in
    build) stage_build ;;
    zone) stage_zone ;;
    answers) pack_answers ;;
    input) stage_input ;;
    prove) stage_prove ;;
    releases) stage_releases ;;
    gpu-jobs) stage_gpu_jobs ;;
    gpu-seals) stage_gpu_seals ;;
    groth16) stage_groth16 "$@" ;;
    vault) stage_vault ;;
    fixture) stage_fixture ;;
    verify) stage_verify ;;
    records) ;;
    *) sed -n '2,35p' "${BASH_SOURCE[0]}"; exit 2 ;;
  esac )
rc=$?
set -e
case "$STAGE" in
  zone|answers|input|prove|releases|gpu-seals|groth16|vault|fixture|records)
    records_final || { log "STOP: the records name this machine or an agent's folder (listed above)"; [ "$rc" -ne 0 ] || rc=1; } ;;
esac
exit "$rc"
