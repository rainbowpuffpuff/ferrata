#!/bin/bash
# p4-macos, after pipeline.sh's proofs: build the release guest natively (bridge/methods, risc0-build) and
# ferrata-e2e (release, as the crown runner); then the succinct proof of the crown's release program
# (~/w/in/release/release.bin, image 0x17257f91...f5cd, the program of p4-minimum's second pass) on the witness
# release-private-1, two runs, each verified with `ferrata-e2e verify`.
# usage: bash ~/w/release.sh > ~/w/out/release.log 2>&1   (marks in ~/w/out/progress.txt)
set -u
W=$HOME/w; O=$W/out; P=$O/progress.txt; R=$O/release; IN=$W/in/release
IMG=0x17257f91bca70abf7a5476b498d687e95b6fb4d59e75b6bca1d46877ac9bf5cd
mark() { echo "$(date -u +%FT%TZ) $*" >> "$P"; }
. "$HOME/.cargo/env"; export PATH=$HOME/.risc0/bin:$HOME/.cargo/bin:$PATH CARGO_TERM_COLOR=never RISC0_BUILD_LOCKED=1
R0VM=$(ls -d "$HOME"/.risc0/extensions/v3.0.5-cargo-risczero-*/r0vm | head -1)
mkdir -p "$R"
sampler() { # $1 csv: every 1 s r0vm's RSS and vm_stat's free / compressor pages
  local f=$1 t0 r v; t0=$(date +%s)
  echo "t_s,r0vm_rss_kib,r0vm_procs,pages_free,pages_compressor" > "$f"
  while :; do
    r=$(ps -axo rss=,comm= | awk '$2 ~ /(^|\/)r0vm$/ {s+=$1; n++} END {printf "%d,%d", s, n}')
    v=$(vm_stat | awk -F: '/^Pages free/ {f=$2} /occupied by compressor/ {c=$2}
        END {gsub(/[ .]/,"",f); gsub(/[ .]/,"",c); printf "%s,%s", f, c}')
    echo "$(( $(date +%s) - t0 )),$r,$v" >> "$f"
    sleep 1
  done
}
mark "BEGIN release-build"
(cd "$W/ferrata/bridge" && /usr/bin/time -l cargo build --release --locked -p methods -p ferrata-e2e) > "$R/build.log" 2>&1
rc=$?; mark "END release-build exit $rc"; [ "$rc" = 0 ] || exit "$rc"
E=$W/ferrata/bridge/target/release/ferrata-e2e
{
  echo "== the release guest as built here (path-dependent image id)"
  find "$W/ferrata/bridge/target" -path '*riscv-guest*' \( -name '*.bin' \) -type f | while read -r b; do
    echo "$b $(stat -f %z "$b") bytes; $("$E" image-id "$b" 2>&1 | tail -1)"
  done
  echo "== the crown's release.bin (p4-minimum's program)"
  shasum -a 256 "$IN/release.bin"; "$E" image-id "$IN/release.bin" 2>&1 | tail -1
} > "$R/ids.txt" 2>&1
cd "$IN" || exit 1
RISC0_DEV_MODE=0 RISC0_INFO=1 RUST_LOG=info RISC0_SERVER_PATH=$R0VM "$E" exec --program release.bin \
  --input witness/release-private-1.input --meta witness/release-private-1.json > "$R/exec.log" 2>&1
mark "release-exec exit $?"
for i in 1 2; do
  rd=$R/run-$i; mkdir -p "$rd"
  sampler "$rd/mem.csv" & sp=$!
  mark "BEGIN release-prove-$i"
  t0=$(date +%s)
  RISC0_DEV_MODE=0 RISC0_INFO=1 RUST_LOG=info RISC0_SERVER_PATH=$R0VM /usr/bin/time -l "$E" prove --program release.bin \
    --input witness/release-private-1.input --meta witness/release-private-1.json --out-dir "$rd/prove" > "$rd/prove.log" 2>&1
  rc=$?; t1=$(date +%s); kill "$sp" 2>/dev/null
  echo "prove exit $rc wall_s $((t1 - t0))" > "$rd/result.txt"
  mark "END release-prove-$i exit $rc"
  RISC0_DEV_MODE=0 "$E" verify --receipt "$rd/prove/receipt-succinct.bin" --image-id "$IMG" \
    --meta witness/release-private-1.json > "$rd/verify-succinct.log" 2>&1
  echo "verify exit $?" >> "$rd/result.txt"
done
mark "RELEASE DONE"
