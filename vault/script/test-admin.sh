#!/usr/bin/env bash
# The tests of vault/script/admin.sh (the vault's admin, freeze-authority and watcher actions from a
# terminal, docs/cli-coverage.md G4) and of admin/tools/anvil-scenario.sh (the admin view's anvil
# scenario as one script, G14):
#
#   1. both scripts parse (bash -n);
#   2. admin.sh --help lists every action, each with its role and arguments;
#   3. a lexically malformed argument (including non-ETH amounts/lists) is refused with exit 2 before any
#      network call; mixed-case ETH aliases are accepted (the dead endpoint separates parsing from I/O);
#   4. a real imported keystore whose address differs from --from is refused before the dry run;
#   5. admin.sh's error table (the vault's error names and their selectors) matches cast sig;
#   6. node is new enough for admin/dump.mjs;
#   7. admin/tools/anvil-scenario.sh runs on one anvil: field-specific report masks plus altered-root/role
#      negative controls; every admin action sent and checked, including both release-freeze values and a
#      successful revocation of a finalised checkpoint; every fee word checked before/after activation;
#      status's real cast calls checked for one block pin; and an apply-roles race whose mined status-0
#      receipt must make --send fail even with CAST_ASYNC=1 in the environment.
#
# The scenario needs foundry's anvil, cast and forge, node 22 for admin/dump.mjs and jq; it binds
# 127.0.0.1 only and contacts nothing else. Its output goes to $FERRATA_ADMIN_TEST_OUT (or a fresh directory
# under $RUNNER_TEMP or the checkout's ignored .scratch/), which is kept for inspection. Test keystores also stay
# below that directory.
#
# Exit codes: 0 all passed; 1 a check failed; 2 usage.

set -euo pipefail

PROG=$(basename "$0")
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"

FAILURES=0
PASSES=0

ok() { echo "ok    $*"; PASSES=$((PASSES + 1)); }
bad() { echo "FAIL  $*"; FAILURES=$((FAILURES + 1)); }

# expect_exit WANT EXIT-CODE DESCRIPTION
expect_exit() {
  if [[ $1 == "$2" ]]; then ok "$3 (exit $2)"; else bad "$3: wanted exit $1, got $2"; fi
}

for tool in cast jq bash node; do
  command -v "$tool" > /dev/null 2>&1 || { echo "$PROG: $tool is not on PATH" >&2; exit 1; }
done

ADMIN=vault/script/admin.sh
SCENARIO=admin/tools/anvil-scenario.sh
# An endpoint nothing listens on: any network call fails with a connection error (cast exits 1), so an
# exit 2 proves the argument was refused before any request was made.
DEAD=http://127.0.0.1:1
VAULT=0x0000000000000000000000000000000000000001
SCRATCH_BASE=${RUNNER_TEMP:-$ROOT/.scratch}
mkdir -p "$SCRATCH_BASE"
OUT_DIR=${FERRATA_ADMIN_TEST_OUT:-$(mktemp -d "$SCRATCH_BASE/ferrata-admin-test.XXXXXX")}
mkdir -p "$OUT_DIR"

echo "== 1. the scripts parse (bash -n)"
for script in "$ADMIN" "$SCENARIO"; do
  if bash -n "$script"; then ok "bash -n $script"; else bad "bash -n $script"; fi
done

echo "== 2. admin.sh --help lists every action with its role"
HELP=$(bash "$ADMIN" --help) || bad "admin.sh --help exited non-zero"
while read -r action role; do
  line=$(grep -E "^  $action " <<< "$HELP" || true)
  if [[ -n $line && $line == *"($role"* ]]; then
    ok "--help lists $action ($role)"
  else
    bad "--help does not list $action ($role)"
  fi
done <<'ACTIONS'
status read-only
set-token-cap admin
set-global-cap admin
set-release-delay admin
raise-delta admin
set-deposits-enabled admin
announce-fees admin
apply-fees anyone
propose-roles admin
apply-roles anyone
transfer-admin admin
accept-admin the pending admin
set-deposits-frozen freeze authority
set-releases-frozen freeze authority
set-checkpoints-frozen freeze authority
set-token-deposits-frozen freeze authority
set-token-releases-frozen freeze authority
revoke-from freeze authority
retire freeze authority
cancel-candidate a watcher or the freeze authority
pause-checkpoints a watcher
ACTIONS

echo "== 3. a malformed argument is refused with exit 2, before any network call"
malformed() { # malformed DESCRIPTION COMMAND...
  local desc=$1 rc=0 out
  shift
  out=$(bash "$@" 2>&1) || rc=$?
  if [[ $rc == 2 && $out != *"Connection refused"* && $out != *"error sending request"* ]]; then
    ok "$desc (exit 2, no network: $(head -n 1 <<< "$out" | sed 's/^admin.sh: //'))"
  else
    bad "$desc: wanted exit 2 with no network call, got exit $rc: $(head -n 1 <<< "$out")"
  fi
}
malformed "status with a bad token address" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 status 0x123
malformed "set-token-cap with a bad direction" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-token-cap eth sideways 1 1
malformed "set-token-cap with a malformed amount" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-token-cap eth deposit 1.x5 1
malformed "set-token-cap with a malformed non-ETH capacity before decimals()" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-token-cap 0x0000000000000000000000000000000000000002 deposit 1.x5 1
malformed "set-token-cap with a malformed non-ETH refill before decimals()" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-token-cap 0x0000000000000000000000000000000000000002 deposit 1 2.x5
malformed "set-token-cap with too many decimals for the unit" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-global-cap deposit 0.1234567890123456789 1
malformed "set-global-cap with a malformed amount" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-global-cap deposit 1etherx 1
malformed "set-release-delay with a non-number" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-release-delay eth soon
malformed "raise-delta with a non-number" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 raise-delta 12x
malformed "set-deposits-enabled with a bad boolean" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-deposits-enabled eth maybe
malformed "announce-fees with an empty list entry" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 announce-fees eth 1,,2 3 4
malformed "announce-fees with a malformed amount in a list" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 announce-fees eth 1,2.5.5 3 4
malformed "announce-fees with an empty non-ETH list entry before decimals()" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 announce-fees 0x0000000000000000000000000000000000000002 1,,2 3 4
malformed "announce-fees with a leading empty non-ETH list entry before decimals()" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 announce-fees 0x0000000000000000000000000000000000000002 ,1 2 3
malformed "announce-fees with a trailing empty non-ETH list entry before decimals()" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 announce-fees 0x0000000000000000000000000000000000000002 1, 2 3
malformed "announce-fees with a malformed non-ETH burn list before decimals()" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 announce-fees 0x0000000000000000000000000000000000000002 1 2.x,3 4
malformed "apply-fees with a bad token address" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 apply-fees 0xzz
malformed "propose-roles with a bad watcher address" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 propose-roles 0x1 0x2 0x3 0x4
malformed "apply-roles with an extra argument" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 apply-roles now
malformed "transfer-admin with a bad address" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 transfer-admin nope
malformed "accept-admin with an extra argument" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 accept-admin now
malformed "set-deposits-frozen with a bad boolean" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-deposits-frozen perhaps
malformed "set-releases-frozen with a bad boolean" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-releases-frozen perhaps
malformed "set-checkpoints-frozen with a bad boolean" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-checkpoints-frozen perhaps
malformed "set-token-deposits-frozen with a bad token" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-token-deposits-frozen 0x123 true
malformed "set-token-releases-frozen with a bad boolean" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 set-token-releases-frozen eth perhaps
malformed "revoke-from with a non-number" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 revoke-from first
malformed "retire with an extra argument" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 retire now
malformed "cancel-candidate with a bad hash" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 cancel-candidate 0x123
malformed "pause-checkpoints with an extra argument" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 pause-checkpoints now
malformed "an unknown action" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 self-destruct
malformed "no --rpc-url (there is no default network)" \
  "$ADMIN" --vault "$VAULT" --from 0x0000000000000000000000000000000000000002 status
malformed "no --vault" \
  "$ADMIN" --rpc-url $DEAD --from 0x0000000000000000000000000000000000000002 status
malformed "no sender for the dry run" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" status
malformed "a raw private key on the command line" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --private-key 0x0000000000000000000000000000000000000000000000000000000000000001 --send retire
malformed "--unlocked without --from" \
  "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --unlocked --send retire

# Valid mixed-case aliases must reach the RPC rather than fail lexical address parsing.
eth_alias() {
  local alias=$1 rc=0 out
  out=$(bash "$ADMIN" --rpc-url $DEAD --vault "$VAULT" \
    --from 0x0000000000000000000000000000000000000002 status "$alias" 2>&1) || rc=$?
  if [[ $rc == 1 && $out != *"not an address"* &&
      ($out == *"Connection refused"* || $out == *"error sending request"*) ]]; then
    ok "ETH alias $alias is accepted (only the dead endpoint fails)"
  else
    bad "ETH alias $alias was not passed to the endpoint correctly (exit $rc): $(head -n 1 <<< "$out")"
  fi
}
eth_alias Eth
eth_alias eTH

echo "== 4. a signing wallet must be the displayed --from sender"
KEYSTORE_DIR="$OUT_DIR/keystore"
mkdir -p "$KEYSTORE_DIR"
ANVIL_ADMIN=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
ANVIL_FREEZE=0x70997970C51812dc3A010C7d01b50e0d17dc79C8
ANVIL_FREEZE_KEY=0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d
if cast wallet import freeze-signer --keystore-dir "$KEYSTORE_DIR" --unsafe-password review \
    --private-key "$ANVIL_FREEZE_KEY" > "$OUT_DIR/keystore-import.log" 2>&1; then
  ok "test keystore created below $OUT_DIR"
else
  bad "could not create the mismatched-signer test keystore"
fi
SIGNER=$(cast wallet address --keystore "$KEYSTORE_DIR/freeze-signer" --password review 2>&1) || SIGNER=""
if [[ $SIGNER == "$ANVIL_FREEZE" ]]; then
  ok "the keystore's independently derived signer is the freeze authority"
else
  bad "the keystore derives $SIGNER, expected $ANVIL_FREEZE"
fi
MISMATCH_RC=0
MISMATCH_OUT=$(bash "$ADMIN" --rpc-url $DEAD --vault "$VAULT" --from "$ANVIL_ADMIN" \
  --keystore "$KEYSTORE_DIR/freeze-signer" --password review \
  --send set-deposits-enabled eth false 2>&1) || MISMATCH_RC=$?
expect_exit 2 "$MISMATCH_RC" "a mismatched --from and keystore are refused before the dry run"
if [[ $MISMATCH_OUT == *"does not match the signing wallet's address"* && $MISMATCH_OUT != *"dry run:"* &&
    $MISMATCH_OUT != *"Connection refused"* && $MISMATCH_OUT != *"error sending request"* ]]; then
  ok "the mismatch names both addresses and makes no network request"
else
  bad "the mismatch was not rejected cleanly before any network request: $(head -n 1 <<< "$MISMATCH_OUT")"
fi

echo "== 5. admin.sh's error table matches cast sig"
TABLE=$(grep -oE '\[0x[0-9a-f]{8}\]=[A-Za-z]+' "$ADMIN" | sed 's/^\[//; s/\]=/ /')
ENTRIES=$(wc -l <<< "$TABLE")
[[ $ENTRIES -gt 80 ]] && ok "$ENTRIES error names in the table" || bad "only $ENTRIES error names in the table"
while read -r selector name; do
  if [[ $(cast sig "$name()") == "$selector" ]]; then
    PASSES=$((PASSES + 1))
  else
    bad "error $name: table says $selector, cast sig says $(cast sig "$name()")"
  fi
done <<< "$TABLE"
echo "      ... $ENTRIES selectors checked against cast sig"
# The three the scenario's wrong-role steps rely on, named:
for name in NotAdmin NotFreezeAuthority NotWatcher; do
  if grep -q "\[.*\]=$name" "$ADMIN"; then ok "$name is in the table"; else bad "$name is missing from the table"; fi
done

echo "== 6. node 22 (admin/dump.mjs needs it)"
NODE_MAJOR=$(node --version | sed 's/^v\([0-9]*\).*/\1/')
if ((NODE_MAJOR >= 22)); then ok "node $(node --version)"; else bad "node $(node --version): admin/dump.mjs needs node 22"; fi

echo "== 7. the anvil scenario and the admin actions against it"
echo "      output: $OUT_DIR"
SCENARIO_RC=0
bash "$SCENARIO" "$OUT_DIR" > "$OUT_DIR/scenario.log" 2>&1 || SCENARIO_RC=$?
sed 's/^/      /' "$OUT_DIR/scenario.log"
if [[ $SCENARIO_RC == 0 ]]; then
  ok "the scenario script passed"
else
  bad "the scenario script failed (exit $SCENARIO_RC; its log: $OUT_DIR/scenario.log)"
fi
# What the scenario itself checked, spelled out (it fails on any of these; this makes each visible).
for line in "an altered deposit root fails the report comparison" "an altered admin role fails the report comparison" \
    "a relative time label alone does not fail the report comparison"; do
  if grep -q "^PASS  $line$" "$OUT_DIR/scenario.log"; then
    ok "$line"
  else
    bad "the mask negative control did not pass: $line"
  fi
done
for line in \
  "refused (NotAdmin)" \
  "refused (NotFreezeAuthority)" \
  "refused (NotWatcher)" \
  "dry run only (no --send)" \
  "set-token-cap, --send" \
  "set-global-cap, --send" \
  "set-deposits-frozen true, --send" \
  "set-deposits-frozen false, --send" \
  "set-releases-frozen true, --send" \
  "set-releases-frozen false, --send" \
  "announce-fees, --send" \
  "apply-fees, --send" \
  "revoke-from, --send" \
  "propose-roles, --send" \
  "set-deposits-enabled false, --send" \
  "set-release-delay, --send" \
  "raise-delta, --send" \
  "set-token-releases-frozen true, --send" \
  "set-checkpoints-frozen true, --send" \
  "apply-roles, --send" \
  "mined apply-roles revert" \
  "pause-checkpoints, --send" \
  "cancel-candidate, --send" \
  "is refused (UnknownCheckpoint)" \
  "transfer-admin, --send" \
  "accept-admin, --send" \
  "retire, --send"; do
  if grep -q "^PASS.*$line" "$OUT_DIR/admin-actions.log"; then
    ok "the scenario ran: $line"
  else
    bad "the scenario did not pass: $line"
  fi
done

echo
echo "$PROG: $PASSES checks passed, $FAILURES failed"
((FAILURES == 0)) || exit 1
echo "$PROG: all checks passed"