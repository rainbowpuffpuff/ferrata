# e2e/seqnet: the robust hook's check of the first gas drop's late credit (RFP-022 R.06; the second review's M1),
# sourced by hook-robust.sh and by test-late-credit.sh, which runs it against a recorded run.
#
# late_credit_verdict DROP LATE CLAIM
#   DROP   the drops daemon's `job-done` result for the first drop (JSON: sends, tx)
#   LATE   holdproxy's "late forward answered" line for the credit it answered 503 and forwarded later (JSON; empty
#          when none was logged)
#   CLAIM  the claim's output (text; its dry run's "claims: N pending credit(s)" line)
# Prints why, and returns 0 only when the run shows what the check is about: two credits of the one drop reached the
# sequencer and were accepted (the 503'd send, forwarded late and answered with its own hash, and a later send, the
# hash the drop was booked with: three sends at least), and the claim claims exactly one pending credit, so of the two
# one applied. Anything less returns 1 (the third review's L-4: a late credit that applied before the daemon sent
# again was a PASS too, although only one credit of the drop then reached LEZ).
late_credit_verdict() {
  local d1=${1:-} late=${2:-} claim=${3:-} sends tx lh pend
  sends=$(jq -r '.sends // 0' <<< "${d1:-null}" 2>/dev/null || echo 0)
  tx=$(jq -r '.tx // ""' <<< "${d1:-null}" 2>/dev/null | tr 'A-F' 'a-f'); tx=${tx#0x}
  pend=$(grep -m1 -o -E 'claims: +[0-9]+ pending credit' <<< "$claim" | grep -o -E '[0-9]+' || true)
  if [ -z "$late" ]; then
    echo "no late forward logged: the credit answered 503 never reached the sequencer"
    return 1
  fi
  lh=$(jq -r '.answer | fromjson? | .result // empty | strings' <<< "$late" 2>/dev/null | tr 'A-F' 'a-f'); lh=${lh#0x}
  if ! [[ "$lh" =~ ^[0-9a-f]{64}$ ]]; then
    echo "the late credit reached the sequencer and was not accepted: $(jq -r '.answer // .' <<< "$late" 2>/dev/null | tr '\n' ' ' | cut -c1-200)"
    return 1
  fi
  if ! [[ "$sends" =~ ^[0-9]+$ ]] || [ "$sends" -lt 3 ]; then
    echo "the late credit ($lh) was accepted, but the daemon sent ${sends:-?} credits: it applied before a third was sent, so one credit of the drop reached LEZ, not the two this check is about"
    return 1
  fi
  if ! [[ "$tx" =~ ^[0-9a-f]{64}$ ]] || [ "$tx" = "$lh" ]; then
    echo "the drop was booked with ${tx:-no hash}, not another credit than the late one ($lh): one credit of the drop at the sequencer"
    return 1
  fi
  if [ "$pend" != 1 ]; then
    echo "two credits of the drop reached the sequencer ($tx, $lh), and the claim claims ${pend:-no} pending credit(s), not exactly 1"
    return 1
  fi
  echo "two credits of the first drop reached the sequencer and were accepted: a later send ($sends sends; the drop booked with $tx) and the send answered 503 to the daemon, forwarded late ($lh); the claim's dry run claims exactly 1 pending credit (a credit sent again is built only on the drop's recorded state: of the two, one applied)"
}
