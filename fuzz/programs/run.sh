#!/usr/bin/env bash
# Run every target in parallel with libFuzzer's fork mode, then minimize each corpus into
# corpus/<target>. Usage (from fuzz/programs): ./run.sh <seconds> <workers per target> <out dir> [targets]
# Expects `cargo +1.98.1 fuzz build --fuzz-dir fuzz/programs -a -s none` (RUSTFLAGS=-Coverflow-checks=on)
# and seeds/ (examples/seeds.rs).
set -u
SECS=${1:-600}; JOBS=${2:-4}; OUT=${3:-runs/last}
TARGETS=${4:-"bridge_wire bridge_handle lez_block checkpoint_input mantle release"}
BIN=target/x86_64-unknown-linux-gnu/release
mkdir -p "$OUT/logs" "$OUT/artifacts" corpus-work
for t in $TARGETS; do
  mkdir -p "corpus-work/$t" "$OUT/artifacts/$t" "seeds/$t" "corpus/$t"
  "$BIN/$t" -fork="$JOBS" -ignore_crashes=1 -ignore_ooms=1 -ignore_timeouts=1 \
    -max_total_time="$SECS" -timeout=20 -rss_limit_mb=4096 -max_len=262144 -print_final_stats=1 \
    -artifact_prefix="$OUT/artifacts/$t/" "corpus-work/$t" "corpus/$t" "seeds/$t" > "$OUT/logs/$t.log" 2>&1 &
done
wait
for t in $TARGETS; do
  rm -rf "corpus-new/$t"; mkdir -p "corpus-new/$t"
  "$BIN/$t" -merge=1 -timeout=20 -rss_limit_mb=4096 -max_len=262144 "corpus-new/$t" "corpus/$t" "corpus-work/$t" "seeds/$t" > "$OUT/logs/$t.merge.log" 2>&1
  rm -rf "corpus/$t"; mv "corpus-new/$t" "corpus/$t"
done
for t in $TARGETS; do
  echo "== $t"
  grep -E '^#[0-9]+: cov' "$OUT/logs/$t.log" | tail -1
  grep -E 'stat::number_of_executed_units' "$OUT/logs/$t.log" | tail -1
  grep -E 'MERGE-OUTER: [0-9]+ new files' "$OUT/logs/$t.merge.log" | tail -1
  echo "corpus $(ls corpus/$t | wc -l) files $(du -sh corpus/$t | cut -f1); artifacts $(ls $OUT/artifacts/$t | wc -l)"
done
