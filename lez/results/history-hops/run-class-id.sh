#!/usr/bin/env bash
# The second build's image id, built as scripts/reproduce-image-ids.sh builds the others (the guest-rustc wrapper,
# locked guest builds, guest Rust 1.91.1), after run-image-ids.sh in the same tree; then every lez guest's id with
# scripts/reproduce/guest_ids.py, and the id compiled into the harness the tests ran (HOPS_TARGET). From the root.
set -u
D=lez/results/history-hops/image-ids
export RUSTC_WRAPPER="$PWD/scripts/guest-rustc" CARGO_TERM_COLOR=never RISC0_BUILD_LOCKED=1
unset CARGO_TARGET_DIR RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
(cd lez && RISC0_HOME="$PWD/../../risc0-home-lez" cargo build -p methods-program-class --locked) > $D/class-build.txt 2>&1
echo "build exit $?" >> $D/class-build.txt
python3 -I scripts/reproduce/guest_ids.py . $D/ids-with-class.tsv > /dev/null; echo "guest_ids exit $?" >> $D/class-build.txt
grep -h "FERRATA_LEZ_CLASS_ID" "${HOPS_TARGET:-$TMPDIR/target-hops}"/debug/build/methods-program-class-*/out/methods.rs > $D/class-id-in-tests.txt 2>&1
grep -h "FERRATA_LEZ_CLASS_ID" lez/target/debug/build/methods-program-class-*/out/methods.rs > $D/class-id-reproducible.txt 2>&1
