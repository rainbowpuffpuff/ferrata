#!/usr/bin/env bash
# rc3-return box job 1 (stage 1): checkpoint-core's tests and clippy in both builds; the reproduce script's build of the
# 14 guests and its compare (docs/image-ids.md, the crown run's table, the Sepolia vault's immutables over public
# JSON-RPC: a read, no transaction); then the second build's two guests (methods-0-3-5) and their ids.
# Run from the worktree root by mrun. Records into checkpoint/results/bedrock-0-3-5/ (synced back).
set -uo pipefail
W="$PWD"; SP="$(cd "$W/.." && pwd)"; S="$SP/tmp/rc3-return"; R="$W/checkpoint/results/bedrock-0-3-5"
mkdir -p "$S" "$R"
export TMPDIR="$S" XDG_CACHE_HOME="$S/xdg-cache" CARGO_TERM_COLOR=never
unset RISC0_DEV_MODE RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
: > "$R/steps.txt"
step() { local n="$1"; shift; local t0; t0=$(date +%s); ( "$@" ) > "$S/box1-$n.log" 2>&1; local rc=$?
  echo "$n exit $rc $(( $(date +%s) - t0 )) s $(date -u +%T)" | tee -a "$R/steps.txt"; return $rc; }
scrub() { sed -e "s#$SP/#spike/#g" -e "s#$HOME/#~/#g"; }

tests() {
  cd "$W/checkpoint" && export CARGO_TARGET_DIR="$W/checkpoint/target"
  cargo +1.88 test -p checkpoint-core -p attestor-core 2>&1 | grep -E '^(test |test result|running|     Running)' ; [ "${PIPESTATUS[0]}" = 0 ] || return 1
  echo "==== with --features bedrock-0-3-5"
  cargo +1.88 test -p checkpoint-core --features bedrock-0-3-5 2>&1 | grep -E '^(test |test result|running|     Running)' ; [ "${PIPESTATUS[0]}" = 0 ] || return 1
  echo "==== clippy, both builds"
  cargo +1.88 clippy -p checkpoint-core -p attestor-core --all-targets -- -D warnings 2>&1 | tail -n 3; [ "${PIPESTATUS[0]}" = 0 ] || return 1
  cargo +1.88 clippy -p checkpoint-core --all-targets --features bedrock-0-3-5 -- -D warnings 2>&1 | tail -n 3; [ "${PIPESTATUS[0]}" = 0 ] || return 1
}
repro() { cd "$W" && rm -rf "$S/repro" && CARGO_BUILD_JOBS=16 bash scripts/reproduce-image-ids.sh build --out "$S/repro"; }
compare() { cd "$W" && bash scripts/reproduce-image-ids.sh compare --out "$S/repro" --vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c; }
lock035() {
  cd "$W/checkpoint" && cargo metadata --format-version 1 --manifest-path guests-0-3-5/Cargo.toml > /dev/null || return 1
  echo "guests-0-3-5/Cargo.lock against guests/Cargo.lock (only the workspace members may differ):"
  diff guests/Cargo.lock guests-0-3-5/Cargo.lock; true
}
build035() {
  cd "$W/checkpoint" && RUSTC_WRAPPER="$W/scripts/guest-rustc" RISC0_BUILD_LOCKED=1 CARGO_BUILD_JOBS=16 \
    env -u RISC0_HOME cargo build --release -p methods-0-3-5 --locked || return 1
  python3 -I "$W/scripts/reproduce/guest_ids.py" "$W" "$S/ids-16.tsv" || return 1
  mkdir -p "$S/r0bf-035"
  local E="$W/bridge/target/release/ferrata-e2e" f bin
  for f in checkpoint_0_3_5 checkpoint_b_0_3_5; do
    bin="$(find "$W/checkpoint/target/riscv-guest" -path '*riscv32im-risc0-zkvm-elf/release/*' ! -path '*/deps/*' ! -path '*/build/*' -name "$f" -type f | head -n 1)"
    [ -n "$bin" ] || { echo "no user ELF $f"; return 1; }
    cp "$bin" "$S/r0bf-035/$f.elf"
    ( cd "$S/r0bf-035" && "$E" program-binary "$f.elf" "$f.bin" ) >> "$S/r0bf-035/ids.jsonl" || return 1
  done
  cat "$S/r0bf-035/ids.jsonl"
  sha256sum "$S"/r0bf-035/*
}

step tests tests
step repro repro
step compare compare
step lock035 lock035
step build035 build035
for n in tests repro compare lock035 build035; do scrub < "$S/box1-$n.log" > "$R/box1-$n.log"; done
[ -f "$S/repro/ids.tsv" ] && cp "$S/repro/ids.tsv" "$R/repro-ids.tsv"
[ -f "$S/repro/ids.txt" ] && cp "$S/repro/ids.txt" "$R/repro-ids-lez.txt"
[ -f "$S/repro/r0bf/ids.jsonl" ] && scrub < "$S/repro/r0bf/ids.jsonl" > "$R/repro-r0bf-ids.jsonl"
[ -f "$S/repro/logs/steps.txt" ] && scrub < "$S/repro/logs/steps.txt" > "$R/repro-steps.txt"
[ -f "$S/ids-16.tsv" ] && cp "$S/ids-16.tsv" "$R/ids-with-0-3-5.tsv"
[ -f "$S/r0bf-035/ids.jsonl" ] && scrub < "$S/r0bf-035/ids.jsonl" > "$R/r0bf-0-3-5-ids.jsonl"
echo "ALL DONE $(date -u +%T)" | tee -a "$R/steps.txt"
