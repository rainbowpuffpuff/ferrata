#!/usr/bin/env bash
# The history-hops tests, from lez/: the hop chains through the deployed program, the second build (feature
# program-class) and the class crate's own tests. On the build box they ran with HOPS_TARGET=$TMPDIR/target-hops, a
# target directory apart from lez/target, so that the image-id check (run-image-ids.sh) built where nothing else had.
# RISC0_HOME: guest Rust 1.91.1 beside the tree (lez/RESULTS.md §6).
set -u
export RISC0_DEV_MODE=1 RISC0_HOME="${RISC0_HOME_LEZ:-$PWD/../../risc0-home-lez}"
if [ -n "${HOPS_TARGET:-}" ]; then export CARGO_TARGET_DIR="$HOPS_TARGET"; fi
D=results/history-hops
cargo test -p ferrata-lez-wire-class --locked > $D/wire-class-tests.txt 2>&1; echo "exit $?" >> $D/wire-class-tests.txt
cargo test -p harness --locked --features program-class --test history_hops -- --nocapture --test-threads 3 > $D/history-hops-tests.txt 2>&1
echo "exit $?" >> $D/history-hops-tests.txt
cargo test -p harness --locked --features program-class --test history_hops_review -- --nocapture --test-threads 2 > $D/history-hops-review-tests.txt 2>&1
echo "exit $?" >> $D/history-hops-review-tests.txt
