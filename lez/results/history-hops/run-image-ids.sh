#!/usr/bin/env bash
# The default image ids' check of the history-hops branch, as run on the Modal build box (a box on which no
# target directory existed: fresh.txt records it): reproduce-image-ids.sh build in this tree, then compare against
# docs/image-ids.md and the live vault's immutables (a read over public JSON-RPC). From the repository root.
set -u
D=lez/results/history-hops/image-ids
mkdir -p $D
OUT="${TMPDIR:-$PWD/reproduce-out}/ids"
rm -rf $OUT
{ echo "before the build:"; for t in target lez/target checkpoint/target bridge/target; do if [ -e $t ]; then echo "$t EXISTS"; else echo "$t absent"; fi; done; ls -d lez/guests/*/target 2>/dev/null || echo "no guest target"; } > $D/fresh.txt
scripts/reproduce-image-ids.sh build --out $OUT > $D/build.txt 2>&1; echo "build exit $?" >> $D/build.txt
scripts/reproduce-image-ids.sh compare --out $OUT --vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c > $D/compare.txt 2>&1; echo "compare exit $?" >> $D/compare.txt
cp $OUT/ids.tsv $OUT/ids.txt $D/ 2>/dev/null; cp $OUT/logs/steps.txt $D/steps.txt 2>/dev/null
