#!/usr/bin/env bash
# p4-minimum, padded pass, on the build box (mrun --profile ws-4), from spike/ferrata-wt-p4-pad/lez:
# build prove_attest, prove_bridge and prove_native_transfer (debug, as before), run the padding test
# (harness/tests/wallet_padding.rs, dev mode), the dev-mode cycle counts of the padded transactions,
# then strip the two binaries and stage them for minimum.py (jobs attest-pad, bridge-pad).
# FERRATA_HEAD and FERRATA_DIRTY name the commit built (the box has no .git).
set -u
S=../../tmp/p4-pad/box          # spike/tmp/p4-pad, beside the repository
rm -rf "$S"; mkdir -p "$S/stripped" "$S/dry" "$S/stage"
export RISC0_HOME=$PWD/../../risc0-home-lez RISC0_BUILD_LOCKED=1
{
  date -u +%FT%TZ; echo "nproc $(nproc)"; grep -m1 'model name' /proc/cpuinfo; grep MemTotal /proc/meminfo
  rustc --version; cargo --version; r0vm --version; ldd --version | head -1
  R0=$(readlink -f "$(command -v r0vm)"); echo "r0vm $R0"; sha256sum "$R0"
  echo "FERRATA_HEAD=${FERRATA_HEAD:-} FERRATA_DIRTY=${FERRATA_DIRTY:-}"
} > "$S/box-env.txt" 2>&1
cat "$S/box-env.txt"
/usr/bin/time -v cargo build -p harness --locked --bin prove_attest --bin prove_bridge --bin prove_native_transfer \
  > "$S/build.log" 2>&1
echo "build exit $?"; grep -E '^(error|warning)|Finished|Elapsed \(wall' "$S/build.log" | tail -6
grep -B2 -A14 '^error' "$S/build.log" | head -60
# the padding test: the three padded transactions against the wallet's own, each settled
RISC0_DEV_MODE=1 /usr/bin/time -v cargo test -p harness --locked --test wallet_padding -- --nocapture \
  > "$S/test-wallet-padding.log" 2>&1
echo "test exit $?"; grep -E 'test result|panicked|^the wallet|^private |Elapsed \(wall' "$S/test-wallet-padding.log" | cut -c1-600
grep -B2 -A14 '^error' "$S/test-wallet-padding.log" | head -60
for b in prove_attest prove_bridge prove_native_transfer; do
  target/debug/$b --image-ids > "$S/$b-ids.txt" 2>&1; echo "$b ids: $(head -c 300 "$S/$b-ids.txt")"
done
# dev mode: the executor runs every session (r0vm logs its cycles), the receipts are fake
RISC0_DEV_MODE=1 RISC0_INFO=1 RUST_LOG=info target/debug/prove_attest --allow-dev-mode --shape wallet \
  --tx-out "$S/dry/attest.borsh" > "$S/dry/attest.log" 2>&1
echo "attest dry exit $?: $(grep -E 'DRY RUN' "$S/dry/attest.log" | cut -c1-220)"
grep -E 'number of segments|user cycles|total cycles' "$S/dry/attest.log" | sed 's/.*session[^ ]* //' | tr '\n' ' '; echo
RISC0_DEV_MODE=1 RISC0_INFO=1 RUST_LOG=info target/debug/prove_bridge --allow-dev-mode --shape wallet \
  --claim-out "$S/dry/claim.borsh" --burn-out "$S/dry/burn.borsh" > "$S/dry/bridge.log" 2>&1
echo "bridge dry exit $?: $(grep -E 'DRY RUN' "$S/dry/bridge.log" | cut -c1-220 | tr '\n' ' ')"
grep -E 'number of segments|user cycles|total cycles' "$S/dry/bridge.log" | sed 's/.*session[^ ]* //' | tr '\n' ' '; echo
RISC0_DEV_MODE=1 target/debug/prove_attest --settle "$S/dry/attest.borsh" > "$S/dry/settle-attest.log" 2>&1
echo "attest dry settle exit $?: $(grep SETTLED "$S/dry/settle-attest.log" | cut -c1-200)"
RISC0_DEV_MODE=1 target/debug/prove_bridge --settle "$S/dry/claim.borsh" "$S/dry/burn.borsh" > "$S/dry/settle-bridge.log" 2>&1
echo "bridge dry settle exit $?: $(grep SETTLED "$S/dry/settle-bridge.log" | cut -c1-200 | tr '\n' ' ')"
# strip (the same code without debug info: the laptop's uplink cannot move 157 MB) and stage
for b in prove_attest prove_bridge; do
  strip -o "$S/stripped/$b" target/debug/$b
  echo "$b: $(stat -c %s target/debug/$b) -> $(stat -c %s "$S/stripped/$b") bytes; sha256 $(sha256sum target/debug/$b | cut -c1-64) -> $(sha256sum "$S/stripped/$b" | cut -c1-64)"
done
scripts/stage-prove-attest.sh "$S/stage/attest-pad" "$S/stripped/prove_attest" > "$S/stage-attest.out" 2>&1
echo "stage attest exit $?"
scripts/stage-prove-bridge.sh "$S/stage/bridge-pad" "$S/stripped/prove_bridge" > "$S/stage-bridge.out" 2>&1
echo "stage bridge exit $?"
"$S/stage/attest-pad/prove_attest" --image-ids | head -1
du -sh "$S/stage"/*
