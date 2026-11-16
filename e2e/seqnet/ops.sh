#!/usr/bin/env bash
# The steps around the user's, for e2e/seqnet's mini-app run: the qmltest (qmltest-logos/main.cpp's seqnet.ops) runs
# them between the user's actions while the zone runs (inside hook-miniapp.sh, whose environment it inherits).
#   ops.sh import              capture-1 of anvil after the user's deposit, crown-driver check, crown-b --operator-only
#                              (the stand-in committee's finality update, bridge Init, Register ETH, ImportRoot)
#   ops.sh fund                the gas drop's stand-in (seqnet-fund): native LEZ to the user's private account /0
#   ops.sh checkpoint          the checkpoint over the zone's blocks (realblocks-crown, checkpoint_b executed, the stand-in
#                              prover, the relayer's submitCandidate, Delta let pass on anvil's clock, finalizeAttested)
#   ops.sh pass-time S WHAT    anvil's clock moved on S seconds
#   ops.sh records             the zone's transactions (seqnet-txs)
#   ops.sh chain [ADDR]        the chains' own state as one JSON line, read here and not through the module or the
#                              view: on anvil (cast) the user's and the vault's balances, the vault's Deposit, Release
#                              and CheckpointFinalized logs, ADDR's balance; on the sequencer (seqnet-txs) its head
#                              and every applied transaction (block, hash, kind, reverted, the bridge's visible calls)
set -uo pipefail
. "$SEQNET_ENV"
. "$REPO/e2e/seqnet/common.sh"
case "${1:-}" in
  import)
    slots="$("$BIN/crown-driver" capture-slots | jq -r '.slots[].slot')"
    sargs=(); for s in $slots; do sargs+=(--slot "$s"); done
    python3 -I "$REPO/lez/crown/capture-anvil.py" --rpc "$RPC" --vault "$VAULT" --from-block "$FROM" "${sargs[@]}" "$VOL/inputs/capture-1" \
      > "$W/capture-1.log" 2>&1 || { tail -n 20 "$W/capture-1.log"; die "capture-1"; }
    say "capture-1: $(tail -n 1 "$W/capture-1.log")"
    "$BIN/crown-driver" check --chain stand-in --program-bin "$ZONE_BUNDLE/program/ferrata_lez.bin" --bridge-bin "$BRIDGE_BIN" \
      --capture "$VOL/inputs/capture-1" --identity "$SA" --operator-only yes > "$W/check.log" 2>&1 || { tail -n 20 "$W/check.log"; die "crown-driver check"; }
    operator_import
    ;;
  fund) fund_private ;;
  checkpoint) checkpoint ;;
  pass-time) shift; s="$1"; shift; pass_time "$s" "$*" ;;
  records) records ;;
  chain) shift; chain_state "${1:-}" ;;
  *) sed -n '2,14p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
