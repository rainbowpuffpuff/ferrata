#!/usr/bin/env bash
# The robust hook's late-credit check (late-credit.sh) against a recorded run: PASS on the record, FAIL on each variant
# that does not show two credits of one drop accepted at the sequencer with one applied (the third review's L-4). The
# check as it was at bff8b821 (`old_check`) is printed beside it.
#   bash e2e/seqnet/test-late-credit.sh [RUN_DIR]   (default e2e/runs/seqnet-robust-3; jq and grep only, no network)
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/late-credit.sh"
R="${1:-$here/../runs/seqnet-robust-3}"
d1=$(jq -c '.drop1' "$R/robust.json")
late=$(jq -c '.drop1LateCredit // empty' "$R/robust.json")
claim=$(cat "$R/user/10-claim.txt")
lh=$(jq -r '.answer | fromjson | .result' <<< "$late")
# hook-robust.sh at bff8b821: a late forward logged and one pending credit was a PASS, in both of its branches.
old_check() { [ -n "$2" ] && grep -q 'claims: *1 pending credit' <<< "$3"; }
fails=0
expect() { # NAME WANT(pass|fail) DROP LATE CLAIM
  local v rc got old
  v=$(late_credit_verdict "$3" "$4" "$5"); rc=$?
  got=fail; [ "$rc" = 0 ] && got=pass
  old=fail; old_check "$3" "$4" "$5" && old=pass
  if [ "$got" = "$2" ]; then echo "ok   $1: $got (the old check: $old). $v"; else echo "BAD  $1: $got, wanted $2. $v"; fails=$((fails + 1)); fi
}
expect "the record, $(basename "$R")" pass "$d1" "$late" "$claim"
expect "variant: the late credit applied before a third send (2 sends)" fail "$(jq -c '.sends = 2' <<< "$d1")" "$late" "$claim"
expect "variant: the late credit not accepted (an error answered)" fail "$d1" \
  "$(jq -c '.answer = "{\"jsonrpc\":\"2.0\",\"id\":62,\"error\":{\"code\":-32603,\"message\":\"refused\"}}"' <<< "$late")" "$claim"
expect "variant: the drop booked with the late credit's own hash" fail "$(jq -c --arg h "$lh" '.tx = $h' <<< "$d1")" "$late" "$claim"
expect "variant: the claim claims 2 pending credits" fail "$d1" "$late" "$(sed -E 's/claims: +1 pending credit/claims:         2 pending credit/' <<< "$claim")"
expect "variant: no late forward logged" fail "$d1" "" "$claim"
echo "$fails unexpected"
exit "$fails"
