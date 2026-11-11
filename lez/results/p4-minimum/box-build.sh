#!/usr/bin/env bash
# p4-minimum, on the build box (mrun), from spike/ferrata-wt-p4/lez: build prove_attest and
# prove_bridge (debug, as for the GPU runs), check them in dev mode, stage them with their inputs.
set -u
OUT=../../tmp/p4/box
rm -rf "$OUT"; mkdir -p "$OUT"
export RISC0_HOME=$PWD/../../risc0-home-lez RISC0_BUILD_LOCKED=1
{
  date -u +%FT%TZ; echo "nproc $(nproc)"; grep -m1 'model name' /proc/cpuinfo; grep MemTotal /proc/meminfo
  rustc --version; cargo --version; r0vm --version; ldd --version | head -1
  R0=$(readlink -f "$(command -v r0vm)"); echo "r0vm $R0"; sha256sum "$R0"
  echo "FERRATA_HEAD=${FERRATA_HEAD:-} FERRATA_DIRTY=${FERRATA_DIRTY:-}"
} > "$OUT/box-env.txt" 2>&1
cat "$OUT/box-env.txt"
/usr/bin/time -v cargo build -p harness --locked --bin prove_attest --bin prove_bridge > "$OUT/build.log" 2>&1
echo "build exit $?"; grep -E '^error|Finished|Elapsed \(wall' "$OUT/build.log" | tail -5
for b in prove_attest prove_bridge; do
  target/debug/$b --image-ids > "$OUT/$b-ids.txt" 2>&1; echo "$b ids exit $?: $(head -c 400 "$OUT/$b-ids.txt")"
done
RISC0_DEV_MODE=1 target/debug/prove_attest --allow-dev-mode --tx-out "$OUT/attest-dry.borsh" > "$OUT/attest-dry.log" 2>&1
echo "attest dry exit $?"; grep -E 'DRY RUN|settled' "$OUT/attest-dry.log" | cut -c1-200
RISC0_DEV_MODE=1 target/debug/prove_bridge --allow-dev-mode --claim-out "$OUT/claim-dry.borsh" --burn-out "$OUT/burn-dry.borsh" > "$OUT/bridge-dry.log" 2>&1
echo "bridge dry exit $?"; grep -E 'DRY RUN|private claim:|private burn:' "$OUT/bridge-dry.log" | cut -c1-200
scripts/stage-prove-attest.sh "$OUT/stage/attest" target/debug/prove_attest > "$OUT/stage-attest.out" 2>&1; echo "stage attest exit $?"
scripts/stage-prove-bridge.sh "$OUT/stage/bridge" target/debug/prove_bridge > "$OUT/stage-bridge.out" 2>&1; echo "stage bridge exit $?"
file target/debug/prove_attest | cut -c1-200; ldd target/debug/prove_attest | sed 's/ (0x.*//'
du -sh "$OUT/stage"/*
