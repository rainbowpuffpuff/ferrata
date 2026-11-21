#!/usr/bin/env bash
# Tests of scripts/ci/sequencer-records.sh (sequencer-attest.sh's record checks and path scrubbing), on the
# recorded run of 21 November 2026 and on copies of it changed here. bash, awk, python3; no network.
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

echo
echo "test-sequencer-records.sh: $npass passed, $nfail failed"
[ "$nfail" = 0 ]
