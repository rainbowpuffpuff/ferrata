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
#   ops.sh approve NAME [reject]   (logoseth mode) the person at Logos's headless approver, evm_signer_cli: waits for the
#                              request tx_sender_module raised for the module's send, records what the approver shows
#                              (its status and prompt), and approves it with the account's password file (or rejects it).
#                              A request the approver's screen dropped is looked up by handle (`list`, then
#                              tx_sender_module's live sends) and shown again (`show`), and that recovery is recorded
#   ops.sh approve-lez NAME [reject]   (logoseth mode) the person at the LEZ role's wallet (its stand-in, seqnet-chain):
#                              waits for the role message the module asked for (module.role_pending), records it, and
#                              approves exactly that message (its SHA-256) with the role password, or rejects it
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
  approve)
    shift; name="${1:?approve NAME}"; decision="${2:-approve}"
    A="$RUN/approver"; mkdir -p "$A"
    lctl() { TMPDIR="$LOGOS_TMPDIR_SHORT" "$LOGOSCTL" --config-dir "$LOGOS_SESSION" "$@"; }
    unwrap() { jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else (.result // .) end' 2>/dev/null; }
    h=""; b=""
    for i in $(seq 240); do
      st=$(lctl call evm_signer_cli status --json < /dev/null 2>&1 | unwrap); h=$(jq -r '.rendered.handle // empty' <<< "$st" 2>/dev/null)
      [ -n "$h" ] && break
      # Nothing on screen after 5 s: the request is looked up as a person would (the manager's queue, then the
      # sender's live sends) and shown by handle (evm_signer_cli show). Recorded: a dropped request is not hidden.
      if [ $(( i % 10 )) = 0 ]; then
        q=$(lctl call evm_signer_cli list --json < /dev/null 2>&1 | unwrap)
        live=$(lctl call tx_sender_module live_sends --json < /dev/null 2>&1 | unwrap)
        want=$(jq -r '[.pending[]? | select(.requester == "tx_sender_module") | .handle] | first // empty' <<< "$q" 2>/dev/null)
        [ -n "$want" ] || want=$(jq -r '[.sends[]? | select(.status == "awaitingApproval") | .handle] | last // empty' <<< "$live" 2>/dev/null)
        if [ -n "$want" ]; then
          sh=$(lctl call evm_signer_cli show "str:$want" --json < /dev/null 2>&1 | unwrap)
          jq -c -n --arg name "$name" --argjson poll "$i" --arg handle "$want" --argjson status "${st:-null}" --argjson list "${q:-null}" \
            --argjson live "${live:-null}" --argjson show "${sh:-null}" '{name: $name, poll: $poll, handle: $handle, status: $status, list: $list, live_sends: $live, show: $show}' \
            >> "$A/recoveries.jsonl" 2>/dev/null || echo "{\"name\":\"$name\",\"poll\":$i,\"handle\":\"$want\"}" >> "$A/recoveries.jsonl"
          if [ "$(jq -r '.ok // false' <<< "$sh" 2>/dev/null)" = true ]; then h="$want"; b=$(jq -r '.bundle_id' <<< "$sh"); break; fi
        fi
      fi
      sleep 0.5
    done
    echo "$st" > "$A/$name.status.json"
    [ -n "$h" ] || die "approve $name: no request reached evm_signer_cli"
    [ -n "$b" ] || b=$(jq -r '.rendered.bundle_id' <<< "$st")
    if [ "$decision" = reject ]; then lctl call evm_signer_cli reject "str:$h" --json < /dev/null > "$A/$name.decision.json" 2>&1
    else lctl call evm_signer_cli approve "str:$h" "str:$b" "@$LOGOS_PASSWORD_FILE" --json < /dev/null > "$A/$name.decision.json" 2>&1; fi
    say "approver: $decision $name (handle $h, requested by $(jq -r '.rendered.requester // "tx_sender_module (shown by handle)"' <<< "$st"))"
    ;;
  approve-lez)
    shift; name="${1:?approve-lez NAME}"; decision="${2:-approve}"
    A="$RUN/approver"; mkdir -p "$A"
    chain() { curl -fsS -m 600 --data-binary @- "$SEQNET_CHAIN_URL" <<< "$1"; }   # the body on stdin: no password in argv
    p=""
    for _ in $(seq 240); do
      pend=$(chain '{"method":"module.role_pending","params":{}}'); p=$(jq -c '.result.pending | last // empty' <<< "$pend" 2>/dev/null)
      [ -n "$p" ] && break; sleep 0.5
    done
    [ -n "$p" ] || die "approve-lez $name: no role message is held at the role wallet"
    echo "$p" > "$A/lez-$name.held.json"
    id=$(jq -r .approval_id <<< "$p"); sha=$(jq -r .message_sha256 <<< "$p")
    if [ "$decision" = reject ]; then
      chain "$(jq -c -n --arg id "$id" '{method: "module.reject_role", params: {approval_id: $id}}')" > "$A/lez-$name.decision.json"
    else
      chain "$(jq -c -n --arg id "$id" --arg sha "$sha" --rawfile pw "$SEQNET_LEZ_ROLE_PASSWORD_FILE" \
        '{method: "module.approve_role", params: {approval_id: $id, message_sha256: $sha, password: $pw}}')" > "$A/lez-$name.decision.json"
    fi
    jq -e '.ok == true' "$A/lez-$name.decision.json" > /dev/null || die "approve-lez $name: $(head -c 400 "$A/lez-$name.decision.json")"
    say "role wallet: $decision $name ($id: $(jq -r .what <<< "$p"); message $(jq -r .message <<< "$p"), sha256 ${sha:0:18}...)"
    ;;
  *) sed -n '2,14p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
