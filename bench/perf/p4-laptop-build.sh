#!/usr/bin/env bash
# RFP-021 P.4 and RFP-022 P.4 on a container sized like a mid-range laptop (8 vCPU, 16 GiB), this tree's images:
# step 1, on the build box (mrun), from lez/: build prove_attest and prove_bridge, their dev-mode dry runs (cycles,
# the transactions settle), strip and stage them for lez/results/p4-minimum/minimum.py (jobs attest-pad, bridge-pad),
# with r0vm 3.0.5 beside them. As lez/results/p4-minimum/box-build-pad.sh, without the padding test and the
# yardstick. Output: spike/tmp/perf-p4 (bring it to the laptop with mrun --get); stage/ is the owner's bundle for
# bench/perf/p4-laptop-owner.sh (attest-pad/, bridge-pad/, r0vm/r0vm and its SHA256SUMS). Steps 2 and 3 (from the
# laptop): docs in lez/results/perf-p4-laptop.md.
set -u
S=../../tmp/perf-p4
rm -rf "$S"; mkdir -p "$S/stripped" "$S/dry" "$S/stage/r0vm"
export RISC0_HOME=$PWD/../../risc0-home-lez RISC0_BUILD_LOCKED=1
R0=$(readlink -f "$(command -v r0vm)")
{
  date -u +%FT%TZ; echo "nproc --all $(nproc --all)"; grep -m1 'model name' /proc/cpuinfo; grep MemTotal /proc/meminfo
  rustc --version; cargo --version; r0vm --version; ldd --version | head -1; echo "r0vm $R0"; sha256sum "$R0"
  echo "FERRATA_HEAD=${FERRATA_HEAD:-} FERRATA_DIRTY=${FERRATA_DIRTY:-}"
} > "$S/box-env.txt" 2>&1
cargo build -p harness --locked --bin prove_attest --bin prove_bridge > "$S/build.log" 2>&1
echo "build exit $?"; grep -E '^error|Finished' "$S/build.log" | tail -3
for b in prove_attest prove_bridge; do target/debug/$b --image-ids > "$S/$b-ids.txt" 2>&1; echo "$b: $(head -c 400 "$S/$b-ids.txt" | tr '\n' ' ')"; done
RISC0_DEV_MODE=1 RISC0_INFO=1 RUST_LOG=info target/debug/prove_attest --allow-dev-mode --shape wallet \
  --tx-out "$S/dry/attest.borsh" > "$S/dry/attest.log" 2>&1
echo "attest dry exit $?: $(grep -E 'DRY RUN' "$S/dry/attest.log" | cut -c1-200)"
RISC0_DEV_MODE=1 RISC0_INFO=1 RUST_LOG=info target/debug/prove_bridge --allow-dev-mode --shape wallet \
  --claim-out "$S/dry/claim.borsh" --burn-out "$S/dry/burn.borsh" > "$S/dry/bridge.log" 2>&1
echo "bridge dry exit $?: $(grep -E 'DRY RUN' "$S/dry/bridge.log" | cut -c1-200 | tr '\n' ' ')"
for b in prove_attest prove_bridge; do strip -o "$S/stripped/$b" target/debug/$b; done
scripts/stage-prove-attest.sh "$S/stage/attest-pad" "$S/stripped/prove_attest" > "$S/stage-attest.out" 2>&1; echo "stage attest exit $?"
scripts/stage-prove-bridge.sh "$S/stage/bridge-pad" "$S/stripped/prove_bridge" > "$S/stage-bridge.out" 2>&1; echo "stage bridge exit $?"
cp "$R0" "$S/stage/r0vm/r0vm"; (cd "$S/stage/r0vm" && sha256sum r0vm > SHA256SUMS)
# The staging scripts write the GPU jobs' command; this bundle is proven on a CPU, padded as the wallet pads.
python3 -I ../bench/perf/p4_manifest.py "$S/stage"
gzip -f "$S/dry/attest.log" "$S/dry/bridge.log"
du -sh "$S/stage"/*
