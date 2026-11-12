#!/usr/bin/env bash
# Rebuild every RISC Zero guest of this tree and print the image ids beside the recorded ones (docs/image-ids.md,
# e2e/crown/README.md's table) and, if a vault is named, the vault's immutables (read over public JSON-RPC).
#
#   scripts/reproduce-image-ids.sh build   --out DIR
#       builds the guests where the tree is (any directory, any $HOME), writes DIR/ids.tsv (all 14 guests),
#       DIR/ids.txt (the two LEZ programs), DIR/r0bf/ids.jsonl (the four R0BF programs of the vault side) and DIR/logs/.
#   scripts/reproduce-image-ids.sh compare --out DIR [--vault ADDR] [--rpc URL]
#       one line per program: rebuilt id, the recorded id, the vault's immutable if --vault is given; exit 1 on a mismatch.
#   scripts/reproduce-image-ids.sh docker  [--out DIR] [--vault ADDR] [--rpc URL]
#       the pinned toolchains of scripts/reproduce/Dockerfile, then `build` on a copy of the tree inside it and `compare`.
#       Needs Docker and ~30 GB of disk; ~15-30 min on 16 cores.
#
# Why the path does not matter: guests are built through scripts/guest-rustc (cargo's build.rustc-wrapper, set in
# .cargo/config.toml), which remaps the tree, the upstream sources beside it, the cargo home and the RISC Zero
# toolchains to fixed names and replaces cargo's path-dependent -C metadata, for the zkVM target only. The same commit
# gives byte-identical guest ELFs and image ids at any path and with any $HOME (docs/image-ids.md: two paths, two HOMEs,
# 14 of 14). What must match is the toolchains (Rust 1.93.0, 1.98.1, RISC Zero 3.0.5 with guest Rust 1.88.0 and, for
# lez/, guest Rust 1.91.1 in its own RISC0_HOME) and the upstream sources beside the tree
# (../upstream/lez-v0.4.0-rc1 at 1806e87, ../upstream/logos-blockchain-d480efc6).
set -uo pipefail  # no -e: a find that matches nothing must not end the script; failures are checked
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$(cd "$HERE/.." && pwd)"
BESIDE="$(cd "$SRC/.." && pwd)"
VAULT=""
RPC="https://ethereum-sepolia-rpc.publicnode.com"
OUT=""
mode="${1:-}"; shift || true
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --vault) VAULT="$2"; shift 2 ;;
    --rpc) RPC="$2"; shift 2 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done

case "$mode" in
  build)
    [ -n "$OUT" ] || { echo "build: --out DIR" >&2; exit 2; }
    mkdir -p "$OUT/logs" "$OUT/programs" "$OUT/r0bf"; OUT="$(cd "$OUT" && pwd)"
    log() { echo "$(date -u +%T) $*" | tee -a "$OUT/logs/steps.txt"; }
    R0LEZ="${RISC0_HOME_LEZ:-$BESIDE/risc0-home-lez}"
    for d in "$BESIDE/upstream/lez-v0.4.0-rc1" "$R0LEZ"; do
      [ -d "$d" ] || { log "missing $d (scripts/fetch-upstream.sh; RISC0_HOME_LEZ=$BESIDE/risc0-home-lez scripts/ci/install-risc0.sh; or set RISC0_HOME_LEZ)"; exit 1; }
    done
    # The wrapper is what makes the ids path-independent: never let an inherited one (sccache, ...) replace it.
    export RUSTC_WRAPPER="$SRC/scripts/guest-rustc" CARGO_TERM_COLOR=never RISC0_BUILD_LOCKED=1
    unset CARGO_TARGET_DIR RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
    log "source $SRC ($(git -C "$SRC" rev-parse HEAD 2>/dev/null || echo 'not a git tree')); HOME $HOME; nproc $(nproc)"
    log "toolchains: $(cargo --version), $(r0vm --version 2>&1 | head -1), lez rust $(ls "$R0LEZ/toolchains" 2>/dev/null | tr '\n' ' ')"
    j="${CARGO_BUILD_JOBS:-8}"
    ( cd "$SRC/lez" && RISC0_HOME="$R0LEZ" CARGO_BUILD_JOBS=$j cargo build -p methods --locked ) > "$OUT/logs/lez-methods.log" 2>&1 & a=$!
    ( cd "$SRC/checkpoint" && env -u RISC0_HOME CARGO_BUILD_JOBS=$j cargo build --release -p methods --locked ) \
      > "$OUT/logs/checkpoint-methods.log" 2>&1 & b=$!
    ( cd "$SRC/bridge" && env -u RISC0_HOME CARGO_BUILD_JOBS=$j cargo build --release -p methods --locked \
        && env -u RISC0_HOME cargo build --release -p ferrata-e2e --locked ) > "$OUT/logs/bridge-methods.log" 2>&1 & c=$!
    fail=0
    for p in a b c; do if eval "wait \$$p"; then log "build $p ok"; else log "build $p FAILED"; fail=1; fi; done
    [ $fail = 0 ] || { tail -n 20 "$OUT"/logs/*-methods.log; exit 1; }
    # Every guest: risc0-build's own id (methods.rs), the binary it embeds (R0BF), the user ELF, r0vm's id.
    python3 -I "$HERE/reproduce/guest_ids.py" "$SRC" "$OUT/ids.tsv" > "$OUT/logs/guest_ids.log" 2>&1 \
      || { cat "$OUT/logs/guest_ids.log"; exit 1; }
    for f in ferrata_lez ferrata_bridge; do
      bin="$(find "$SRC/lez/target/riscv-guest" -path '*release*' -name "$f.bin" | head -n 1)"
      [ -n "$bin" ] && cp "$bin" "$OUT/programs/$f.bin"
    done
    for f in checkpoint_b checkpoint segment release; do
      for r in "$SRC/checkpoint/target/riscv-guest" "$SRC/bridge/target/riscv-guest"; do
        bin="$(find "$r" -path '*riscv32im-risc0-zkvm-elf/release/*' ! -path '*/release/deps/*' ! -path '*/release/build/*' \
          -name "$f" -type f 2>/dev/null | head -n 1)"
        [ -n "$bin" ] && { cp "$bin" "$OUT/programs/$f.bin"; break; }  # the user ELF (named as before)
      done
    done
    # The LEZ programs are R0BF already (r0vm prints their id); the other four are user ELFs that
    # ferrata-e2e program-binary combines with the v1compat kernel as risc0-build 3.0.5 does.
    ( cd "$OUT/programs" && for f in ferrata_lez.bin ferrata_bridge.bin; do
        printf '%s %s %s\n' "$f" "$(sha256sum "$f" | cut -c1-64)" "$(r0vm --elf "$f" --id 2>&1 | tail -n 1)"; done ) > "$OUT/ids.txt"
    E="$SRC/bridge/target/release/ferrata-e2e"
    ( cd "$OUT/r0bf" && for f in checkpoint_b checkpoint segment release; do "$E" program-binary "../programs/$f.bin" "$f.bin"; done ) \
      > "$OUT/r0bf/ids.jsonl" 2> "$OUT/logs/program-binary.log"
    find "$SRC"/{lez,checkpoint,bridge}/target -path '*/build/methods*-*/out/methods.rs' 2>/dev/null \
      | xargs -r grep -h -E 'pub const [A-Z0-9_]+_ID: \[u32; 8\]' > "$OUT/methods-ids.txt" || true
    log "programs: $(ls "$OUT/programs" | tr '\n' ' ')"
    [ "$(ls "$OUT"/r0bf/*.bin 2>/dev/null | wc -l)" = 4 ] || { log "not all four R0BF programs made"; cat "$OUT/logs/program-binary.log"; exit 1; }
    cat "$OUT/ids.tsv" "$OUT/ids.txt" "$OUT/r0bf/ids.jsonl"
    ;;
  compare)
    [ -n "$OUT" ] || { echo "compare: --out DIR" >&2; exit 2; }
    exec python3 -I "$HERE/reproduce/compare_ids.py" --out "$OUT" --readme "$SRC/e2e/crown/README.md" \
      --table "$SRC/docs/image-ids.md" ${VAULT:+--vault "$VAULT"} --rpc "$RPC"
    ;;
  docker)
    OUT="${OUT:-$SRC/reproduce-out}"; mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
    docker build -t ferrata-reproduce -f "$HERE/reproduce/Dockerfile" "$HERE/reproduce"
    # The image holds the toolchains and the upstream sources at /work/upstream; the tree is copied beside them.
    docker run --rm -v "$SRC:/src:ro" -v "$OUT:/out" ferrata-reproduce bash -c \
      'rsync -a --exclude .git --exclude target/ --exclude /vault/lib/ --exclude /vault/out/ --exclude /vault/cache/ /src/ /work/ferrata/ \
       && bash /work/ferrata/scripts/reproduce-image-ids.sh build --out /out'
    "$0" compare --out "$OUT" ${VAULT:+--vault "$VAULT"} --rpc "$RPC"
    ;;
  *) sed -n '2,22p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
