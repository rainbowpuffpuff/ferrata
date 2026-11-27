#!/usr/bin/env bash
# The admin view's local anvil scenario (admin/README.md, "A local scenario"), as one script, and the
# vault's admin actions driven from a terminal through vault/script/admin.sh (docs/cli-coverage.md G4
# and G14).
#
#   admin/tools/anvil-scenario.sh OUT_DIR
#
# What it does, in order:
#   1. starts anvil on a free local port (127.0.0.1 only; stopped on exit, whatever happens);
#   2. runs the scenario of vault/script/AdminScenario.s.sol with --broadcast, using anvil's published
#      test accounts 0 and 1 as the admin and the freeze authority. Those keys are anvil's well-known
#      development keys, printed by anvil on every start and public in every Foundry document; they are
#      not secrets, and nothing here may reach a public network;
#   3. finds the vault's address in forge's broadcast record;
#   4. reads the vault with node admin/dump.mjs and writes the report to OUT_DIR/local-anvil-scenario.txt;
#   5. compares it with admin/results/local-anvil-scenario.txt using field-specific masks for deployed
#      addresses, run-dependent code/transaction/schedule/beacon hashes, times, blocks and the port; fixed
#      roots, ids, commitments and roles stay literal. Altered root and admin-role controls must fail;
#   6. drives every vault/script/admin.sh action against the same anvil. It checks caps, every freeze (both
#      global release values included), all fee words before/after activation, roles, the admin handover,
#      watcher actions, retirement, and a real checkpoint finalisation followed by successful revoke-from
#      and checked restored state/revocation epoch. A cross-account apply-roles race proves that a mined
#      status-0 receipt fails, and status's cast invocations prove that all vault/token calls share a block
#      pin. Each admin.sh step is logged with PASS/FAIL in OUT_DIR/admin-actions.log.
#
# Exit codes: 0 everything passed; 1 anything failed (a step, the comparison, or a tool); 2 usage.
# Needs foundry's anvil, cast and forge, node 22 for admin/dump.mjs, and jq. Offline: anvil binds
# 127.0.0.1 only and nothing else is contacted.

set -euo pipefail

PROG=$(basename "$0")

usage_error() {
  echo "$PROG: $*" >&2
  echo "$PROG: usage: $PROG OUT_DIR" >&2
  exit 2
}

die() {
  echo "$PROG: $*" >&2
  exit 1
}

(($# == 1)) || usage_error "exactly one argument, the output directory"
OUT_DIR=$1
mkdir -p "$OUT_DIR" || die "could not create $OUT_DIR"

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"

for tool in anvil cast forge node jq; do
  command -v "$tool" > /dev/null 2>&1 || die "$tool is not on PATH"
done

# Anvil's published accounts (their keys are printed on every anvil start; public test keys).
ADMIN=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266      # anvil account 0, the scenario's admin
FREEZE=0x70997970C51812dc3A010C7d01b50e0d17dc79C8    # anvil account 1, the freeze authority
ANYONE=0x90F79bf6EB2c4f870365E785982E1f101E93b906    # anvil account 3, a bystander (applyFees is permissionless)
W0=0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC        # anvil accounts 2, 4 and 5 as proposed watchers
W1=0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65
W2=0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc
NEXTFREEZE=0x976EA74026E726554dB657fA54763abd0C3a0aa9  # anvil account 6 as the proposed freeze authority

# A free port: bash can probe /dev/tcp without an outside tool.
free_port() {
  local p=$(( 20000 + RANDOM % 20000 ))
  while (exec 3<>"/dev/tcp/127.0.0.1/$p") 2> /dev/null; do
    exec 3>&- 3<&-
    p=$(( (p + 1) % 65000 ))
  done
  echo "$p"
}

PORT=$(free_port)
RPC=http://127.0.0.1:$PORT
echo "$PROG: anvil on $RPC (its output: $OUT_DIR/anvil.log)"

anvil --port "$PORT" --silent > "$OUT_DIR/anvil.log" 2>&1 & ANVIL_PID=$!
trap 'kill "$ANVIL_PID" 2> /dev/null; wait "$ANVIL_PID" 2> /dev/null' EXIT INT TERM

# Wait for the endpoint to answer (usually one round trip).
ready=0
for _ in $(seq 1 50); do
  if cast chain-id --rpc-url "$RPC" > /dev/null 2>&1; then ready=1; break; fi
  sleep 0.2
done
((ready)) || die "anvil did not answer on $RPC"

# The scenario itself (admin/README.md). The keys are anvil's published development keys.
echo "$PROG: forge script script/AdminScenario.s.sol --broadcast"
(cd vault &&
  ADMIN_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
  FREEZE_KEY=0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d \
  forge script script/AdminScenario.s.sol --rpc-url "$RPC" --broadcast) > "$OUT_DIR/forge-script.log" 2>&1 \
  || die "the scenario script failed (its output: $OUT_DIR/forge-script.log)"

BROADCAST=vault/broadcast/AdminScenario.s.sol/31337/run-latest.json
[[ -f $BROADCAST ]] || die "forge wrote no broadcast record at $BROADCAST"
VAULT=$(jq -r '[.transactions[] | select(.transactionType == "CREATE" and .contractName == "FerrataVault") | .contractAddress] | unique | .[0]' "$BROADCAST")
[[ -n $VAULT && $VAULT != null ]] || die "could not find the vault's address in $BROADCAST"
echo "$PROG: vault deployed at $VAULT"

# The admin view's dump of it. Exit code 3 means the read completed with some failed optional read
# (the mock verifier has no VERSION()/SELECTOR()); the committed record lists the same two.
echo "$PROG: node admin/dump.mjs $RPC <vault> 0"
DUMP_RC=0
node admin/dump.mjs "$RPC" "$VAULT" 0 > "$OUT_DIR/local-anvil-scenario.txt" 2> "$OUT_DIR/dump-progress.txt" || DUMP_RC=$?
case $DUMP_RC in
  0|3) ;;
  *) die "admin/dump.mjs failed with exit code $DUMP_RC (its stderr: $OUT_DIR/dump-progress.txt)" ;;
esac
echo "$PROG: dump exit code $DUMP_RC (0 complete, 3 with the two optional reads the mock lacks)"

# Normalise only run-dependent fields, by report field: deployed vault/library/verifier addresses; runtime
# code and transaction hashes; time-dependent beacon and fee-schedule hashes; timestamps, port and block
# tags. Code byte counts differ from the record's older build. The historical record used a different,
# explicitly named freeze key, so each side substitutes only its own expected freeze-authority address.
# Roots, configuration ids, commitments, registry hashes, all other roles and every state value stay literal.
# The two optional-read errors are sorted because their asynchronous completion order races.
mask() { # mask REPORT EXPECTED_FREEZE_AUTHORITY
  local expected_freeze=$2
  sed -E \
    -e '/^(# vault:|Ferrata vault admin view:|  vault \.+)/s/0x[0-9a-fA-F]{40}/0xVAULT/g' \
    -e '/vault runtime code/s/keccak256 0x[0-9a-fA-F]{64}/keccak256 0xCODEHASH/' \
    -e '/linked library \(FerrataRegistry\)/s/0x[0-9a-fA-F]{40}/0xLIBRARY/' \
    -e '/^  (verifier|stop \(the vault.s verifier\)|wrapped verifier) /s/0x[0-9a-fA-F]{40}/0xVERIFIER/g' \
    -e "/${expected_freeze}/s//0xFREEZE_AUTHORITY/g" \
    -e '/^    [0-9]+ +[0-9]+ +0x[0-9a-fA-F]{40} /s/0x[0-9a-fA-F]{64}$/0xTXHASH/' \
    -e '/^    [0-9]+ +[0-9]{4}-[0-9]{2}-[0-9]{2}T/s/0x[0-9a-fA-F]{64}$/0xBEACONNONCE/' \
    -e '/^    [0-9]+ +[0-9]+ +.*0x[0-9a-fA-F]{64} +match +match$/s/0x[0-9a-fA-F]{64}/0xFEESCHEDULEHASH/' \
    -e '/FeesInForce /s/scheduleHash=0x[0-9a-fA-F]{64}/scheduleHash=0xFEESCHEDULEHASH/' \
    -e '/vault runtime code|linked library|stop \(the vault.s verifier\)|wrapped verifier/s/[0-9]+ bytes( of code)?/N bytes\1/g' \
    -e 's/[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?Z/TIME/g' \
    -e '/TIME|FROZEN until|until=|effectiveAt=|announcedAt=/s/\b1[789][0-9]{8}\b/EPOCH/g' \
    -e 's#http://127\.0\.0\.1:[0-9]+#http://127.0.0.1:PORT#g' \
    -e 's/blocks 0-[0-9]+/blocks 0-N/g' \
    -e 's/block [0-9]+/block N/g' \
    -e 's/in [0-9]+(\.[0-9]+)? s/in N s/g' \
    "$1" \
  | awk '
    function dump() {
      for (i = 2; i <= n; i++) { v = b[i]; for (j = i - 1; j >= 1 && b[j] > v; j--) b[j + 1] = b[j]; b[j + 1] = v }
      for (i = 1; i <= n; i++) print b[i]
      n = 0
    }
    /^  optional read/ { b[++n] = $0; next }
    { if (n) dump(); print }
    END { if (n) dump() }
  '
}

RECORD_FREEZE=0x34c12c034C40A6076Ee47C934aEc48D9a0f4e66a
compare_reports() { # compare_reports ACTUAL RECORD DIFF-FILE
  diff <(mask "$1" "$FREEZE") <(mask "$2" "$RECORD_FREEZE") > "$3"
}

echo "$PROG: comparing the dump with admin/results/local-anvil-scenario.txt (field-specific masks)"
if ! compare_reports "$OUT_DIR/local-anvil-scenario.txt" admin/results/local-anvil-scenario.txt \
    "$OUT_DIR/scenario-diff.txt"; then
  echo "$PROG: the scenario's dump differs from the committed record beyond what masking covers:" >&2
  cat "$OUT_DIR/scenario-diff.txt" >&2
  exit 1
fi
echo "$PROG: the dump matches the committed record"

# Negative controls: fixed state and configured identities must not disappear into a broad hash/address mask.
sed -E '/depositRoot /s/0x[0-9a-fA-F]{64}/0x0000000000000000000000000000000000000000000000000000000000000001/' \
  "$OUT_DIR/local-anvil-scenario.txt" > "$OUT_DIR/corrupt-root.txt"
if compare_reports "$OUT_DIR/corrupt-root.txt" admin/results/local-anvil-scenario.txt "$OUT_DIR/corrupt-root.diff"; then
  die "MASK TEST FAILED: an altered deposit root was accepted"
else
  echo "PASS  an altered deposit root fails the report comparison"
fi
sed -E '/^  admin \.+/s/0x[0-9a-fA-F]{40}/0x0000000000000000000000000000000000000002/' \
  "$OUT_DIR/local-anvil-scenario.txt" > "$OUT_DIR/corrupt-role.txt"
if compare_reports "$OUT_DIR/corrupt-role.txt" admin/results/local-anvil-scenario.txt "$OUT_DIR/corrupt-role.diff"; then
  die "MASK TEST FAILED: an altered admin role was accepted"
else
  echo "PASS  an altered admin role fails the report comparison"
fi

# ------------------------------------------------------------------------------------------------------------
# The admin actions, from a terminal, against the same anvil. Every action is one admin.sh call; the
# wrong-role ones are dry runs only (they must be refused, with the vault's error name, and exit 1), the
# state-changing ones are sent with --send (anvil's published accounts, unlocked), and each change is
# checked on the chain right after. Every step's output is kept in $OUT_DIR/admin-actions.log.
# ------------------------------------------------------------------------------------------------------------

A=(bash vault/script/admin.sh --rpc-url "$RPC" --vault "$VAULT")
LOG="$OUT_DIR/admin-actions.log"
: > "$LOG"
FAILURES=0

# act DESCRIPTION WANT-EXIT COMMAND... : runs one admin.sh call and counts it.
act() {
  local desc=$1 want=$2 rc=0
  shift 2
  printf '\n$ %s\n' "$*" >> "$LOG"
  if "$@" > "$OUT_DIR/step.out" 2>&1; then rc=0; else rc=$?; fi
  cat "$OUT_DIR/step.out" >> "$LOG"
  if [[ $rc == "$want" ]]; then
    echo "PASS  $desc"
    echo "PASS  $desc" >> "$LOG"
  else
    echo "FAIL  $desc (exit code $rc, wanted $want)"
    echo "FAIL  $desc (exit code $rc, wanted $want)" >> "$LOG"
    FAILURES=$((FAILURES + 1))
  fi
}

# assert DESCRIPTION TEST-COMMAND... : counts a failure when the check does not hold.
assert() {
  local desc=$1
  shift
  if "$@"; then
    echo "        ok: $desc"
    echo "        ok: $desc" >> "$LOG"
  else
    echo "        FAILED: $desc"
    echo "        FAILED: $desc" >> "$LOG"
    FAILURES=$((FAILURES + 1))
  fi
}

# word INDEX SIG ARGS... : the INDEXth (0-based) word of a vault read, as a decimal.
word() {
  local i=$1 sig=$2 hexout
  shift 2
  hexout=$(cast call "$VAULT" "$sig" "$@" --rpc-url "$RPC") || return 1
  hexout=${hexout#0x}
  cast to-dec "0x${hexout:$((i * 64)):64}"
}

# word0x INDEX SIG ARGS... : the INDEXth (0-based) word of a vault read, as lowercase hex without 0x.
word0x() {
  local i=$1 sig=$2 hexout
  shift 2
  hexout=$(cast call "$VAULT" "$sig" "$@" --rpc-url "$RPC") || return 1
  hexout=${hexout#0x}
  echo "${hexout:$((i * 64)):64}"
}

# no0x ADDRESS: an address as lowercase hex, left-padded to a word, to compare with word0x.
no0x() {
  local a
  a=$(cast to-check-sum-address "$1") || return 1
  a=${a#0x}
  printf '%64s' "${a,,}" | tr ' ' 0
}

ETH=0x0000000000000000000000000000000000000000
echo "$PROG: the admin actions (vault/script/admin.sh), each checked on the chain"

# What an admin checks before acting. Wrap cast to record the real commands and verify status's block pin.
mkdir -p "$OUT_DIR/status-bin"
STATUS_REAL_CAST=$(command -v cast)
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'printf "%s\n" "$*" >> "$STATUS_CAST_LOG"' \
  'exec "$STATUS_REAL_CAST" "$@"' > "$OUT_DIR/status-bin/cast"
chmod +x "$OUT_DIR/status-bin/cast"
STATUS_CAST_LOG="$OUT_DIR/status-cast.log"
: > "$STATUS_CAST_LOG"
act "admin.sh status reads the vault's roles, caps, freezes and fee state" 0 \
  env PATH="$OUT_DIR/status-bin:$PATH" STATUS_REAL_CAST="$STATUS_REAL_CAST" STATUS_CAST_LOG="$STATUS_CAST_LOG" \
  "${A[@]}" --from "$ADMIN" status eth
STATUS_READ_BLOCK=$(sed -n 's/^chain:.*block \([0-9][0-9]*\).*/\1/p' "$OUT_DIR/step.out")
assert "status reports the block number it selected" test -n "$STATUS_READ_BLOCK"
status_calls_pinned() {
  local cmd seen=0
  while IFS= read -r cmd; do
    [[ ${cmd,,} == call\ "${VAULT,,}"* ]] || continue
    seen=1
    [[ " $cmd " == *" --block $STATUS_READ_BLOCK "* ]] || return 1
  done < "$STATUS_CAST_LOG"
  ((seen))
}
assert "every status vault eth_call carries its reported --block" status_calls_pinned
assert "status reads the timestamp of that same block" \
  grep -q "^block $STATUS_READ_BLOCK --field timestamp" "$STATUS_CAST_LOG"
assert "status describes a pinned non-state-changing snapshot" \
  grep -q "non-state-changing snapshot at block $STATUS_READ_BLOCK" "$OUT_DIR/step.out"
assert "status names the freeze authority" grep -q "freezeAuthority:  $FREEZE" "$OUT_DIR/step.out"
assert "status shows the releases freeze in force" grep -q "releases:.*\[IN FORCE\]" "$OUT_DIR/step.out"

# Wrong roles, refused in the dry run with the vault's error name (exit 1, nothing sent).
act "a global cap change by the freeze authority is refused (NotAdmin)" 1 \
  "${A[@]}" --from "$FREEZE" set-global-cap deposit 3000ether 1ether
assert "the vault's error name is decoded" grep -q "NotAdmin()" "$OUT_DIR/step.out"
act "a deposit freeze by the admin is refused (NotFreezeAuthority)" 1 \
  "${A[@]}" --from "$ADMIN" set-deposits-frozen true
assert "the vault's error name is decoded" grep -q "NotFreezeAuthority()" "$OUT_DIR/step.out"
act "pause-checkpoints by the admin is refused (NotWatcher)" 1 \
  "${A[@]}" --from "$ADMIN" pause-checkpoints
assert "the vault's error name is decoded" grep -q "NotWatcher()" "$OUT_DIR/step.out"

# A dry run sends nothing: the sender's nonce and the vault's state are unchanged.
NONCE_BEFORE=$(cast nonce "$ADMIN" --rpc-url "$RPC")
act "a global cap change as a dry run only (no --send)" 0 \
  "${A[@]}" --from "$ADMIN" set-global-cap deposit 1234ether 0
NONCE_AFTER=$(cast nonce "$ADMIN" --rpc-url "$RPC")
assert "the admin's nonce is unchanged ($NONCE_BEFORE)" test "$NONCE_BEFORE" = "$NONCE_AFTER"
assert "the global deposit cap is unchanged (100 ETH)" \
  test "$(word 0 globalCap\(uint8\) 0)" = 100000000000000000000
assert "the dry run says it did not send" grep -q "not sent:   dry run only" "$OUT_DIR/step.out"

# A token cap change, sent.
act "the admin sets ETH's deposit token cap (set-token-cap, --send)" 0 \
  "${A[@]}" --from "$ADMIN" --unlocked --send set-token-cap eth deposit 500ether 1ether
assert "tokenCap(ETH, deposit) capacity is 500 ETH" \
  test "$(word 0 tokenCap\(address,uint8\) "$ETH" 0)" = 500000000000000000000

# A global cap change, sent.
act "the admin sets the global deposit cap (set-global-cap, --send)" 0 \
  "${A[@]}" --from "$ADMIN" --unlocked --send set-global-cap deposit 2000ether 5ether
assert "globalCap(deposit) capacity is 2000 ETH" test "$(word 0 globalCap\(uint8\) 0)" = 2000000000000000000000

# A deposit freeze by the freeze authority, then its lift.
act "the freeze authority freezes all deposits (set-deposits-frozen true, --send)" 0 \
  "${A[@]}" --from "$FREEZE" --unlocked --send set-deposits-frozen true
assert "depositsFrozenUntil is in the future" \
  test "$(word 0 depositsFrozenUntil\(\))" -gt "$(cast block latest --field timestamp --rpc-url "$RPC")"
act "the freeze authority lifts the deposit freeze (set-deposits-frozen false, --send)" 0 \
  "${A[@]}" --from "$FREEZE" --unlocked --send set-deposits-frozen false
assert "depositsFrozenUntil is 0 again" test "$(word 0 depositsFrozenUntil\(\))" = 0

# Exercise the release-freeze selector itself, not only the scenario's initial Solidity freeze.
act "the freeze authority freezes all releases (set-releases-frozen true, --send)" 0 \
  "${A[@]}" --from "$FREEZE" --unlocked --send set-releases-frozen true
assert "releasesFrozenUntil is in the future" \
  test "$(word 0 releasesFrozenUntil\(\))" -gt "$(cast block latest --field timestamp --rpc-url "$RPC")"
act "the freeze authority lifts all releases (set-releases-frozen false, --send)" 0 \
  "${A[@]}" --from "$FREEZE" --unlocked --send set-releases-frozen false
assert "releasesFrozenUntil is 0" test "$(word 0 releasesFrozenUntil\(\))" = 0

# Three distinct, nonzero lists. Check every four-word fees() answer while announced and once in force;
# the relay words are the deployment's unchanged relay-fee list.
MINT_FEES=(100000000000000 1000000000000000 10000000000000000)
RELAY_FEES=(100000000000000 1000000000000000 10000000000000000)
BURN_FEES=(200000000000000 2000000000000000 20000000000000000)
CHECKPOINT_FEES=(300000000000000 3000000000000000 30000000000000000)
assert_fee_words() {
  local stage=$1 i j
  local -a expected
  for i in 0 1 2; do
    expected=("${MINT_FEES[$i]}" "${RELAY_FEES[$i]}" "${BURN_FEES[$i]}" "${CHECKPOINT_FEES[$i]}")
    for j in 0 1 2 3; do
      assert "$stage fees(ETH, 1, $i) word $j is ${expected[$j]}" \
        test "$(word "$j" fees\(address,uint256,uint8\) "$ETH" 1 "$i")" = "${expected[$j]}"
    done
  done
}
act "the admin announces fee schedule 1 (announce-fees, --send)" 0 \
  "${A[@]}" --from "$ADMIN" --unlocked --send announce-fees eth \
  0.0001,0.001,0.01 0.0002,0.002,0.02 0.0003,0.003,0.03
assert "a fee schedule is announced (pendingAt is set)" test "$(word 1 feeState\(address\) "$ETH")" != 0
EXPECTED_CALLDATA=$(cast calldata "announceFees(address,uint256[],uint256[],uint256[])" "$ETH" \
  "[100000000000000,1000000000000000,10000000000000000]" \
  "[200000000000000,2000000000000000,20000000000000000]" \
  "[300000000000000,3000000000000000,30000000000000000]")
assert "announce-fees prints independently encoded calldata" \
  grep -q "^calldata:   $EXPECTED_CALLDATA$" "$OUT_DIR/step.out"
assert_fee_words "announced"
cast rpc --rpc-url "$RPC" evm_increaseTime 604801 > /dev/null # seven days and one second
cast rpc --rpc-url "$RPC" evm_mine > /dev/null
act "anyone applies the announced fees after the notice (apply-fees, --send)" 0 \
  "${A[@]}" --from "$ANYONE" --unlocked --send apply-fees eth
FEE=$(cast call "$VAULT" "feeState(address)" "$ETH" --rpc-url "$RPC")
assert "fee schedule 1 is in force" test "$(cast to-dec "0x${FEE:2:64}")" = 1
assert "no fee schedule is pending" test "$(cast to-dec "0x${FEE:66:64}")" = 0
assert_fee_words "applied"

# Finalise the scenario's candidate through rule (A), then revoke checkpoint 0 immediately through admin.sh.
# The mock accepts the echo journal just as AdminScenario.s.sol accepted the original candidate journal.
PARENT_STATE=$(cast call "$VAULT" "finalState()(bytes32)" --rpc-url "$RPC")
LEZ_CONFIG=$(cast call "$VAULT" "lezConfigId()(bytes32)" --rpc-url "$RPC")
CHECKPOINT_IMAGE=$(cast call "$VAULT" "checkpointImageId()(bytes32)" --rpc-url "$RPC")
STOP=$(cast call "$VAULT" "verifier()(address)" --rpc-url "$RPC")
MOCK=$(cast call "$STOP" "verifier()(address)" --rpc-url "$RPC")
CANDIDATE=$(cast keccak "admin scenario: state 1")
BURN_ROOT=$(cast keccak "admin scenario: burn root")
TIP_HASH=$(cast keccak "admin scenario: tip")
DOMAIN=0xbdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7
ZERO32=0x0000000000000000000000000000000000000000000000000000000000000000
cast send "$VAULT" "recordBeacon()" --rpc-url "$RPC" --unlocked --from "$ANYONE" --json > "$OUT_DIR/record-beacon.json"
mapfile -t BEACON_WORDS < <(cast call "$VAULT" "beacons(uint256)(uint256,bytes32)" 1 --rpc-url "$RPC")
ECHO_NONCE=${BEACON_WORDS[1]%% *}
PART1=$(cast abi-encode "journalPrefix(bytes32,uint256,address,bytes32,uint32,uint32)" \
  "$DOMAIN" 31337 "$VAULT" "$LEZ_CONFIG" 3 2)
PART2=$(cast abi-encode "journalState(bytes32,bytes32,bytes32,uint64,uint64,bool,bytes32)" \
  "$PARENT_STATE" "$CANDIDATE" "$BURN_ROOT" 1 2 false "$ECHO_NONCE")
PART3=$(cast abi-encode "journalTip(uint64,bytes32,uint32,uint32)" 42 "$TIP_HASH" 3 2)
PREIMAGE="$PART1${PART2#0x}${PART3#0x}"
ECHO_DIGEST=$(node -e 'const c=require("node:crypto"); console.log("0x"+c.createHash("sha256").update(Buffer.from(process.argv[1].slice(2),"hex")).digest("hex"))' "$PREIMAGE")
CHECKPOINT_SIG="(uint32,uint32,bytes32,bytes32,bytes32,uint64,uint64,bool,bytes32,uint64,bytes32,uint32,uint32)"
cast send "$MOCK" "accept(bytes32,bytes32)" "$CHECKPOINT_IMAGE" "$ECHO_DIGEST" \
  --rpc-url "$RPC" --unlocked --from "$ANYONE" --json > "$OUT_DIR/accept-echo.json"
cast send "$VAULT" "finalize(uint256,$CHECKPOINT_SIG,bytes)" 1 \
  "(3,2,$PARENT_STATE,$CANDIDATE,$BURN_ROOT,1,2,false,$ECHO_NONCE,42,$TIP_HASH,3,2)" 0x \
  --rpc-url "$RPC" --unlocked --from "$ANYONE" --json > "$OUT_DIR/finalize-checkpoint.json"
for receipt in record-beacon accept-echo finalize-checkpoint; do
  assert "$receipt was mined successfully" jq -e '.status == "0x1" or .status == 1' "$OUT_DIR/$receipt.json"
done
assert "one checkpoint is finalised" test "$(word 0 checkpointCount\(\))" = 1
assert "finalisation moves finalState to the candidate" \
  test "$(cast call "$VAULT" "finalState()(bytes32)" --rpc-url "$RPC")" = "$CANDIDATE"
assert "checkpoint 0 starts unrevoked" test "$(word 6 checkpoints\(uint256\) 0)" = 0
EPOCH_BEFORE=$(word 0 revocationEpoch\(\))
act "the freeze authority revokes finalised checkpoint 0 (revoke-from, --send)" 0 \
  "${A[@]}" --from "$FREEZE" --unlocked --send revoke-from 0
assert "revocation restores the parent finalState" \
  test "$(cast call "$VAULT" "finalState()(bytes32)" --rpc-url "$RPC")" = "$PARENT_STATE"
assert "revocation restores the parent imported-leaf count" test "$(word 0 finalImportedLeafCount\(\))" = 0
assert "checkpoint 0 is marked revoked" test "$(word 6 checkpoints\(uint256\) 0)" = 1
assert "revocationEpoch rises by one" test "$(word 0 revocationEpoch\(\))" = "$((EPOCH_BEFORE + 1))"
# Recreate the pending candidate after revocation so the existing watcher-cancellation action remains reachable.
cast send "$VAULT" "submitCandidate($CHECKPOINT_SIG,bytes)" \
  "(3,2,$PARENT_STATE,$CANDIDATE,$BURN_ROOT,1,2,false,$ZERO32,42,$TIP_HASH,3,2)" 0x \
  --rpc-url "$RPC" --unlocked --from "$ANYONE" --json > "$OUT_DIR/resubmit-candidate.json"
assert "the candidate is resubmitted after revocation" test "$(word 4 candidates\(bytes32\) "$CANDIDATE")" != 0

# A role proposal naming three watchers and a new freeze authority (a 14-day timelock, not applied here).
act "the admin proposes new watchers and a new freeze authority (propose-roles, --send)" 0 \
  "${A[@]}" --from "$ADMIN" --unlocked --send propose-roles "$W0" "$W1" "$W2" "$NEXTFREEZE"
PROPOSAL=$(cast call "$VAULT" "roleProposal()" --rpc-url "$RPC")
PROPOSED=0x${PROPOSAL:26:40}
assert "the proposal names the new freeze authority" test "${PROPOSED,,}" = "${NEXTFREEZE,,}"
assert "the proposal has a time it takes effect" test "$(cast to-dec "0x${PROPOSAL:66:64}")" -gt 0

# Delisting: deposits stop, releases are unaffected.
act "the admin delists ETH (set-deposits-enabled false, --send)" 0 \
  "${A[@]}" --from "$ADMIN" --unlocked --send set-deposits-enabled eth false
assert "ETH's deposits are disabled" test "$(word 1 tokenConfig\(address\) "$ETH")" = 0

# A release delay for roots finalised later, and a raised delta.
act "the admin sets ETH's release delay (set-release-delay, --send)" 0 \
  "${A[@]}" --from "$ADMIN" --unlocked --send set-release-delay eth 900
assert "the release delay for roots finalised now is 900 s" \
  test "$(word 0 releaseDelayFor\(address,uint256\) "$ETH" "$(word 0 checkpointCount\(\))")" = 900
act "the admin raises delta (raise-delta, --send)" 0 \
  "${A[@]}" --from "$ADMIN" --unlocked --send raise-delta 1200
assert "delta is 1200 s" test "$(word 0 delta\(\))" = 1200

# The other per-token freezes, and the checkpoint freeze.
act "the freeze authority freezes ETH releases (set-token-releases-frozen true, --send)" 0 \
  "${A[@]}" --from "$FREEZE" --unlocked --send set-token-releases-frozen eth true
assert "ETH's releases are frozen" test "$(word 0 tokenReleasesFrozenUntil\(address\) "$ETH")" -gt 0
act "the freeze authority lifts ETH's release freeze (set-token-releases-frozen false, --send)" 0 \
  "${A[@]}" --from "$FREEZE" --unlocked --send set-token-releases-frozen eth false
assert "ETH's releases are not frozen" test "$(word 0 tokenReleasesFrozenUntil\(address\) "$ETH")" = 0
act "the freeze authority lifts the scenario's ETH deposit freeze (set-token-deposits-frozen false, --send)" 0 \
  "${A[@]}" --from "$FREEZE" --unlocked --send set-token-deposits-frozen eth false
assert "ETH's deposits are not frozen" test "$(word 0 tokenDepositsFrozenUntil\(address\) "$ETH")" = 0
act "the freeze authority freezes checkpoints (set-checkpoints-frozen true, --send)" 0 \
  "${A[@]}" --from "$FREEZE" --unlocked --send set-checkpoints-frozen true
assert "checkpointsFrozenUntil is in the future" \
  test "$(word 0 checkpointsFrozenUntil\(\))" -gt "$(cast block latest --field timestamp --rpc-url "$RPC")"
act "the freeze authority lifts the checkpoint freeze (set-checkpoints-frozen false, --send)" 0 \
  "${A[@]}" --from "$FREEZE" --unlocked --send set-checkpoints-frozen false
assert "checkpointsFrozenUntil is not in the future" \
  test "$(word 0 checkpointsFrozenUntil\(\))" -le "$(cast block latest --field timestamp --rpc-url "$RPC")"

# The role proposal applied, after its 14-day timelock, by anyone.
cast rpc --rpc-url "$RPC" evm_increaseTime 1209601 > /dev/null # fourteen days and one second
cast rpc --rpc-url "$RPC" evm_mine > /dev/null
act "anyone applies the proposed roles after the timelock (apply-roles, --send)" 0 \
  "${A[@]}" --from "$ANYONE" --unlocked --send apply-roles
assert "the watchers are the proposed ones" \
  test "$(word0x 0 watchers\(uint256\) 0)$(word0x 0 watchers\(uint256\) 1)$(word0x 0 watchers\(uint256\) 2)" \
    = "$(no0x "$W0")$(no0x "$W1")$(no0x "$W2")"
assert "the freeze authority is the proposed one" test "$(word0x 0 freezeAuthority\(\))" = "$(no0x "$NEXTFREEZE")"

# H1 regression: a second, identical proposal lets another account win between the dry run and the send.
# The cast wrapper injects only when admin.sh reaches cast send; a fixed gas limit prevents an earlier
# estimation from hiding the mined-revert case. CAST_ASYNC=1 also verifies that admin.sh forces it off.
act "the admin proposes roles again for the receipt-race test (propose-roles, --send)" 0 \
  "${A[@]}" --from "$ADMIN" --unlocked --send propose-roles "$W0" "$W1" "$W2" "$NEXTFREEZE"
cast rpc --rpc-url "$RPC" evm_increaseTime 1209601 > /dev/null
cast rpc --rpc-url "$RPC" evm_mine > /dev/null
mkdir -p "$OUT_DIR/race-bin"
REAL_CAST=$(command -v cast)
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'if [[ ${1:-} == send && " $* " == *" applyRoles() "* ]]; then' \
  '  CAST_ASYNC=false "$REAL_CAST" send "$RACE_VAULT" "applyRoles()" --rpc-url "$RACE_RPC" \' \
  '    --unlocked --from "$RACE_OTHER" --json > "$RACE_COMPETING_JSON"' \
  'fi' \
  'exec "$REAL_CAST" "$@"' > "$OUT_DIR/race-bin/cast"
chmod +x "$OUT_DIR/race-bin/cast"
act "a mined apply-roles revert after a successful dry run fails (apply-roles, --send)" 1 \
  env PATH="$OUT_DIR/race-bin:$PATH" REAL_CAST="$REAL_CAST" RACE_VAULT="$VAULT" RACE_RPC="$RPC" \
  RACE_OTHER="$ADMIN" RACE_COMPETING_JSON="$OUT_DIR/race-competing.json" CAST_ASYNC=1 ETH_GAS_LIMIT=500000 \
  "${A[@]}" --from "$ANYONE" --unlocked --send apply-roles
assert "the competing account's apply-roles receipt succeeded" \
  jq -e '.status == "0x1" or .status == 1' "$OUT_DIR/race-competing.json"
assert "the raced transaction was mined with receipt status 0" \
  grep -Eq '"status"[[:space:]]*:[[:space:]]*"0x0"' "$OUT_DIR/step.out"
assert "the mined revert is reported as a command failure" \
  grep -q 'was mined but failed (receipt status 0x0)' "$OUT_DIR/step.out"

# A watcher's actions, now that the watchers are installed.
act "a watcher pauses checkpoints (pause-checkpoints, --send)" 0 \
  "${A[@]}" --from "$W0" --unlocked --send pause-checkpoints
assert "checkpointsFrozenUntil is in the future (a 12-hour pause)" \
  test "$(word 0 checkpointsFrozenUntil\(\))" -gt "$(cast block latest --field timestamp --rpc-url "$RPC")"
CANDIDATE=$(cast keccak "admin scenario: state 1") # resubmitted after checkpoint 0's revocation
act "a watcher cancels the pending candidate (cancel-candidate, --send)" 0 \
  "${A[@]}" --from "$W0" --unlocked --send cancel-candidate "$CANDIDATE"
assert "the candidate is gone (submittedAt 0)" \
  test "$(word 4 candidates\(bytes32\) "$CANDIDATE")" = 0

# Reject-only revocation also refuses an out-of-range checkpoint index.
act "revoke-from by the freeze authority with no checkpoint 1 finalised is refused (UnknownCheckpoint)" 1 \
  "${A[@]}" --from "$NEXTFREEZE" revoke-from 1
assert "the vault's error name is decoded" grep -q "UnknownCheckpoint()" "$OUT_DIR/step.out"

# The admin handover in two steps.
act "the admin starts the handover (transfer-admin, --send)" 0 \
  "${A[@]}" --from "$ADMIN" --unlocked --send transfer-admin "$W1"
assert "the new admin is pending" test "$(word0x 0 pendingAdmin\(\))" = "$(no0x "$W1")"
act "the new admin accepts (accept-admin, --send)" 0 \
  "${A[@]}" --from "$W1" --unlocked --send accept-admin
assert "the admin is the new one" test "$(word0x 0 admin\(\))" = "$(no0x "$W1")"

# Retirement, by the (new) freeze authority.
act "the freeze authority retires the vault (retire, --send)" 0 \
  "${A[@]}" --from "$NEXTFREEZE" --unlocked --send retire
assert "the vault is retired" test "$(word 0 retired\(\))" = 1

# What an admin checks after acting.
act "admin.sh status reflects the new state" 0 \
  "${A[@]}" --from "$W1" status eth
assert "status shows ETH delisted" grep -q "depositsEnabled: false" "$OUT_DIR/step.out"
assert "status shows no deposit freeze" grep -q "deposits:.*\[not in force\]" "$OUT_DIR/step.out"
assert "status shows fee schedule 1 in force" grep -q "fee schedule in force: 1$" "$OUT_DIR/step.out"
assert "status shows the retired vault" grep -q "retired:      true" "$OUT_DIR/step.out"
assert "status names the new admin" grep -q "admin:            $W1" "$OUT_DIR/step.out"
assert "status names the new freeze authority" grep -q "freezeAuthority:  $NEXTFREEZE" "$OUT_DIR/step.out"

echo "$PROG: the admin actions: $(( $(grep -c '^PASS' "$LOG") )) passed, $FAILURES failed (the log: $OUT_DIR/admin-actions.log)"
((FAILURES == 0)) || exit 1

echo "$PROG: the scenario, the comparison and the admin actions all passed"
