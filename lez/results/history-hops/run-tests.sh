#!/usr/bin/env bash
# The history-hops tests as run on the Modal build box, from lez/: the hop chains through the deployed program, the
# second build (feature program-class) and the class crate's own tests. CARGO_TARGET_DIR apart from lez/target, so
# that the image-id check (run-image-ids.sh) builds in a target directory nothing else has used.
set -u
export RISC0_DEV_MODE=1 RISC0_HOME="$PWD/../../risc0-home-lez" CARGO_TARGET_DIR="${HOPS_TARGET:-$TMPDIR/target-hops}"
D=results/history-hops
cargo test -p ferrata-lez-wire-class --locked > $D/wire-class-tests.txt 2>&1; echo "exit $?" >> $D/wire-class-tests.txt
cargo test -p harness --locked --features program-class --test history_hops -- --nocapture --test-threads 3 > $D/history-hops-tests.txt 2>&1
echo "exit $?" >> $D/history-hops-tests.txt
