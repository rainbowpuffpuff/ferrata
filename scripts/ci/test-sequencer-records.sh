#!/usr/bin/env bash
# Tests of scripts/ci/sequencer-records.sh (sequencer-attest.sh's record checks and path scrubbing), on the
# recorded run of 21 November 2026 and on copies of it changed here; and of the check stages of sequencer-attest.sh and
# sequencer-roundtrip.sh, on the box records of 24 November 2026 and on copies of them changed here. bash, awk, python3;
# no network.
#
#   bash scripts/ci/test-sequencer-records.sh [SCRATCH-DIR]   (default: a new directory under TMPDIR)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
S=${1:-$(mktemp -d "${TMPDIR:-/tmp}/test-sequencer-records.XXXXXX")}
mkdir -p "$S"
S=$(cd "$S" && pwd)
. "$here/sequencer-records.sh"
REC=$repo/e2e/runs/ci-sequencer-attest-2026-11-21/records
npass=0; nfail=0
check() { local what=$1; shift; if "$@"; then echo "ok   $what"; npass=$((npass + 1)); else echo "FAIL $what"; nfail=$((nfail + 1)); fi; }
not() { ! "$@"; }

# The recorded run stopped in `scenarios`: the consumer was never deployed, though programs.tsv lists it.
check "the recorded run's programs.tsv lists the consumer (what the old check accepted)" \
  awk -F'\t' '$1 == "consumer" { f = 1 } END { exit !f }' "$REC/programs.tsv"
check "the recorded run: consumer_deployed says no (no deploy consumer row in run.tsv)" not consumer_deployed "$REC"

# A copy with the consumer's deployment rows, as the attest-private phase writes them.
rm -rf "$S/rec" && mkdir -p "$S/rec" && cp "$REC/programs.tsv" "$REC/run.tsv" "$S/rec/"
n=$(awk -F'\t' '$1 == "consumer" { print $7 }' "$REC/programs.tsv")
row() { printf 'attest-private\t%s\t00\t99\t0\t%s\t-\t0\t0\t0\t0\t0\t0\t-\t1\t0\t0\t-\t-\t-\t-\n' "$1" "$2"; }
{ row "deploy consumer: CreateHeader (immutable) at X" applied
  i=1; while [ "$i" -le "$n" ]; do row "deploy consumer: WriteSegment $i of $n (98304 bytes)" applied; i=$((i + 1)); done
} >> "$S/rec/run.tsv"
check "header and all $n segments applied: deployed" consumer_deployed "$S/rec"
awk -F'\t' -v OFS='\t' '$2 ~ /^deploy consumer: WriteSegment 2 of / { $6 = "reverted" } 1' "$S/rec/run.tsv" > "$S/rec/run2.tsv"
mkdir -p "$S/rec2" && cp "$S/rec/programs.tsv" "$S/rec2/" && mv "$S/rec/run2.tsv" "$S/rec2/run.tsv"
check "one segment reverted: not deployed" not consumer_deployed "$S/rec2"
grep -v 'deploy consumer: CreateHeader' "$S/rec/run.tsv" > "$S/rec2/run.tsv"
check "no header: not deployed" not consumer_deployed "$S/rec2"

# Scrubbing: text files rewritten, binary files and other paths untouched.
B=$S/beside; H=$S/home
rm -rf "$S/out" && mkdir -p "$S/out/records/logs"
printf 'built %s/upstream/lez in %s/.cargo; kept /usr/bin/ld and %sx/not-beside\n' "$B" "$H" "$B" > "$S/out/sequencer-attest.log"
printf 'driver at %s/repo/lez/live\n' "$B" > "$S/out/records/logs/driver.log"
printf 'bin\0ary %s/x' "$B" > "$S/out/records/blocks.borsh"
scrub_paths "$S/out" "$B" "$H"
check "the log: the paths beside the repository and in HOME made relative" \
  grep -q -F 'built ../upstream/lez in ~/.cargo; kept /usr/bin/ld and' "$S/out/sequencer-attest.log"
check "the log: a longer name that only starts like the path is kept" grep -q -F "${B}x/not-beside" "$S/out/sequencer-attest.log"
check "records/logs: scrubbed too" grep -q -F 'driver at ../repo/lez/live' "$S/out/records/logs/driver.log"
check "a binary file untouched" grep -q -a -F "$B/x" "$S/out/records/blocks.borsh"

# With the repository given: its own directory name leaves the records too, absolute or relative from beside it.
R=$B/some-checkout
rm -rf "$S/out2" && mkdir -p "$S/out2"
printf 'tables from %s/lez/results\nCompiling harness (../some-checkout/lez/harness)\nfrom ../other/lez and ../some-checkout-2/x\n' "$R" > "$S/out2/run.log"
scrub_paths "$S/out2" "$B" "$H" "$R"
check "with REPO: the repository's absolute path -> <repo>/" grep -q -F 'tables from <repo>/lez/results' "$S/out2/run.log"
check "with REPO: ../NAME/ written from beside it -> <repo>/" grep -q -F 'Compiling harness (<repo>/lez/harness)' "$S/out2/run.log"
check "with REPO: other paths beside it, and a longer name, kept" grep -q -F 'from ../other/lez and ../some-checkout-2/x' "$S/out2/run.log"
check "with REPO: the checkout's name nowhere left" not grep -q -F 'some-checkout/' "$S/out2/run.log"

# r0vm_of: rzup's layout (r0vm inside cargo-risczero's extension), else an r0vm on PATH of that version.
F=$S/r0vm-homes; rm -rf "$F"; mkdir -p "$F/lez/extensions/v3.0.5-cargo-risczero-x86_64-unknown-linux-gnu" "$F/bin"
fake() { printf '#!/bin/sh\necho "risc0-r0vm %s"\n' "$2" > "$1"; chmod +x "$1"; }
fake "$F/lez/extensions/v3.0.5-cargo-risczero-x86_64-unknown-linux-gnu/r0vm" 3.0.5
check "r0vm_of: the version rzup put in the home's extension" \
  test "$(r0vm_of "$F/lez" 3.0.5)" = "$F/lez/extensions/v3.0.5-cargo-risczero-x86_64-unknown-linux-gnu/r0vm"
check "r0vm_of: another version is not taken from the home" not env PATH=/usr/bin:/bin bash -c '. "$1"; r0vm_of "$2" 3.1.0' _ "$here/sequencer-records.sh" "$F/lez"
fake "$F/bin/r0vm" 3.1.0
check "r0vm_of: an r0vm on PATH that reports the version" \
  test "$(PATH="$F/bin:/usr/bin:/bin" bash -c '. "$1"; r0vm_of "$2" 3.1.0' _ "$here/sequencer-records.sh" "$F/lez")" = "$F/bin/r0vm"
check "r0vm_of: an r0vm on PATH of another version is refused" \
  not env PATH="$F/bin:/usr/bin:/bin" bash -c '. "$1"; r0vm_of "$2" 3.1.1' _ "$here/sequencer-records.sh" "$F/none"

# The round trip's fork: the hook's choice of a public Sepolia RPC (its lines, copied from the hook), with a fake cast
# whose answer depends on the RPC: an error, a block two hours old, a block number that is not a number (it must never
# reach shell arithmetic), a block 15 minutes old.
FB=$S/fork; mkdir -p "$FB/bin" "$FB/work"
cat > "$FB/bin/cast" <<'EOF'
#!/usr/bin/env bash
url="${@: -1}"; now=$(date +%s)
case "$url" in
  *bad*) echo "Error: error sending request for url ($url)" >&2; exit 1 ;;
  *stale*) printf '{"number":"0xba3c4f","timestamp":"%s","hash":"0x%064x"}\n' "$((now - 7200))" 1 ;;
  *evil*) printf '{"number":"a[$(touch PWNED)]","timestamp":"%s","hash":"0x%064x"}\n' "$now" 1 ;;
  *good*) printf '{"number":"0xba3c4f","timestamp":"0x%x","hash":"0x%064x"}\n' "$((now - 900))" 2 ;;
esac
EOF
chmod +x "$FB/bin/cast"
{ grep '^int() ' "$here/sequencer-roundtrip-hook.sh"
  sed -n '/^FORK_RPC=""$/,/^\[ -n "\$FORK_RPC" \] || die/p' "$here/sequencer-roundtrip-hook.sh"; } > "$FB/pick.sh"
fork_pick() { # RPC-LIST: "RPC BLOCK" of the RPC the hook would fork, or "die: WHY"
  (cd "$FB" && PATH="$FB/bin:$PATH" SEPOLIA_RPCS="$1" bash -c '
    set -uo pipefail; W=work; FORK_MAX_AGE=3600; : > work/fork.err
    say() { :; }; die() { echo "die: $*"; exit 1; }; sleep() { :; }
    . ./pick.sh; echo "$FORK_RPC $FORK_BLOCK"')
}
check "fork: the selection's lines found in the hook" test "$(wc -l < "$FB/pick.sh")" -ge 10
check "fork: an RPC that answers with a recent block" test "$(fork_pick https://good.example)" = "https://good.example 12205135"
check "fork: an RPC that errs, then the next" \
  test "$(fork_pick 'https://bad.example https://good.example')" = "https://good.example 12205135"
check "fork: a block two hours old refused, then the next RPC" \
  test "$(fork_pick 'https://stale.example https://good.example')" = "https://good.example 12205135"
fork_fails() { [[ "$(fork_pick "$1")" == die:* ]]; }
check "fork: none usable: the step fails" fork_fails "https://bad.example https://stale.example"
fork_not_evaluated() { test "$(fork_pick "https://evil.example https://good.example")" = "https://good.example 12205135" && test ! -e "$FB/PWNED"; }
check "fork: a block number that is not one is refused, never evaluated" fork_not_evaluated

# The check stages of sequencer-attest.sh and sequencer-roundtrip.sh on the box records of 24 November 2026 (the
# scripts of review round 1): the record as it is passes; each change that makes a designed property false fails it
# (review round 1's mutations, among them the five its check had let pass). FERRATA_CI_NOW: the check's clock, an hour
# after the attest data's finalized block, half an hour after the round trip's fork block.
BOX=$repo/e2e/runs/ci-sequencer-2026-11-24
t_of() { python3 -I -c 'import datetime, json, sys; d = json.load(open(sys.argv[1]))
for k in sys.argv[2].split("."): d = d[k]
print(int(datetime.datetime.strptime(d, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=datetime.timezone.utc).timestamp()) + int(sys.argv[3]))' "$@"; }
A_NOW=$(t_of "$BOX/attest/fresh/check.json" finalizedTime 3600)
R_NOW=$(t_of "$BOX/roundtrip/run/facts.json" fork.time 1800)
PERIOD=$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["periods"][0]["period"])' "$BOX/attest/fresh/check.json")
# stage_check KIND NAME [VAR=VALUE...] -- CHANGE: a copy of the box record of KIND, CHANGE run inside it, then the check
# stage under the given environment; the stage's status is the function's.
stage_check() {
  local kind=$1 name=$2 d now; shift 2
  local envs=(); while [ "$1" != -- ]; do envs+=("$1"); shift; done; shift
  d=$S/checks/$kind-$name; rm -rf "$d"; mkdir -p "$S/checks"; cp -r "$BOX/$kind" "$d"
  (cd "$d" && eval "$1")
  if [ "$kind" = attest ]; then now=$A_NOW; else now=$R_NOW; fi
  env -u GITHUB_ACTIONS -u GITHUB_STEP_SUMMARY FERRATA_CI_NOW="$now" ${envs[@]+"${envs[@]}"} \
    bash "$here/sequencer-$kind.sh" "$d" check > "$d.log" 2>&1
}
set_json() { # FILE KEY.PATH JSON-VALUE
  python3 -I -c 'import json, sys; p, k, v = sys.argv[1:]; d = json.load(open(p)); o = d
ks = k.split(".")
for x in ks[:-1]: o = o[x]
o[ks[-1]] = json.loads(v); json.dump(d, open(p, "w"))' "$@"
}
check "attest: the box record passes its check" stage_check attest as-recorded -- ':'
check "attest: a period update's Finish refused fails it" not stage_check attest period-finish-refused -- \
  "awk -F'\t' -v OFS='\t' '\$2 == \"fresh A period $PERIOD update Finish (step 3)\" { \$6 = \"refused\"; \$7 = 40 } 1' records/run.tsv > x && mv x records/run.tsv"
check "attest: under GitHub Actions, the private phase left out (phases.txt and rows) fails it" \
  not stage_check attest private-left-out GITHUB_ACTIONS=true -- \
  "sed -i 's/ fresh-private//' phases.txt && awk -F'\t' '\$1 != \"fresh-private\"' records/run.tsv > x && mv x records/run.tsv"
check "attest: a local run without the private phase passes, its check saying incomplete" \
  stage_check attest private-left-out-locally -- \
  "sed -i 's/ fresh-private//' phases.txt && awk -F'\t' '\$1 != \"fresh-private\"' records/run.tsv > x && mv x records/run.tsv"
check "attest: ... and its summary says INCOMPLETE" \
  bash -c 'env -u GITHUB_ACTIONS -u GITHUB_STEP_SUMMARY bash "$1" "$2" summary > /dev/null 2>&1; grep -q "^\*\*INCOMPLETE" "$2/README.md"' \
  _ "$here/sequencer-attest.sh" "$S/checks/attest-private-left-out-locally"
check "attest: the data 14 days older fails it" not stage_check attest data-14-days-old -- \
  "python3 -I -c 'import datetime as D, json; p = \"fresh/check.json\"; c = json.load(open(p)); f = \"%Y-%m-%dT%H:%M:%SZ\"
for k in (\"finalizedTime\", \"bootstrapTime\"): c[k] = (D.datetime.strptime(c[k], f) - D.timedelta(days=14)).strftime(f)
json.dump(c, open(p, \"w\"))'"
check "attest: a finalized block over fresh-check's bound when fetched fails it" not stage_check attest fetched-stale -- \
  "$(declare -f set_json); set_json fresh/check.json finalizedAgeS 9000"
check "attest: one refusal more (CallAttest refused) fails it" not stage_check attest one-refusal-more -- \
  "awk -F'\t' -v OFS='\t' '\$2 == \"fresh consumer CallAttest: the Transfer log\" { \$6 = \"refused\"; \$7 = 215 } 1' records/run.tsv > x && mv x records/run.tsv"
check "roundtrip: the box record passes its check" stage_check roundtrip as-recorded -- ':'
check "roundtrip: a burn FAILED line beside its ok line fails it" not stage_check roundtrip burn-failed-and-ok -- \
  "awk -F'\t' -v OFS='\t' '\$1 == \"burn\" { print \"burn\", \"FAILED\", 180, \"not in a block after 3 min\" } 1' run/steps.tsv > x && mv x run/steps.tsv"
check "roundtrip: the fees step missing fails it" not stage_check roundtrip fees-missing -- \
  "awk -F'\t' '\$1 != \"fees\"' run/steps.tsv > x && mv x run/steps.tsv"
check "roundtrip: two steps swapped fails it" not stage_check roundtrip steps-swapped -- \
  "awk 'NR == 7 { held = \$0; next } { print } NR == 8 { print held }' run/steps.tsv > x && mv x run/steps.tsv"
check "roundtrip: a fork block two hours old when forked fails it" not stage_check roundtrip fork-stale -- \
  "$(declare -f set_json); set_json run/facts.json fork.ageS 7200"
check "roundtrip: the check four hours after the fork fails it" not stage_check roundtrip checked-late -- \
  "$(declare -f set_json); set_json run/facts.json fork.time '\"$(date -u -d "@$((R_NOW - 1800 - 14400))" +%Y-%m-%dT%H:%M:%SZ)\"'"
check "roundtrip: the replayed release accepted fails it" not stage_check roundtrip replay-accepted -- \
  "$(declare -f set_json); set_json run/facts.json release.replay.callExit 0 && : > run/eth/replay.txt"
check "roundtrip: the release short-paid fails it" not stage_check roundtrip short-paid -- \
  "$(declare -f set_json); set_json run/facts.json release.recipientBalanceWei '\"996000000000000\"'"

echo
echo "test-sequencer-records.sh: $npass passed, $nfail failed"
[ "$nfail" = 0 ]
