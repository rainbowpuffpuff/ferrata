#!/usr/bin/env bash
# Rebuild the six guests of the crown run and print their image ids beside the recorded ones
# (e2e/crown/README.md's table) and the deployed vault's immutables (read over public JSON-RPC).
#
#   scripts/reproduce-image-ids.sh docker  [--out DIR] [--vault ADDR] [--rpc URL]
#       builds scripts/reproduce/Dockerfile (pinned toolchains, the same paths as the build box) and runs
#       `build` and `compare` inside it. Needs Docker and ~30 GB of disk; ~15-30 min on 16 cores.
#   scripts/reproduce-image-ids.sh build   --out DIR       (inside a container or a throwaway box ONLY)
#       copies this tree to $FERRATA_BUILD_PATH (default: the crown build path below), builds the guests there
#       exactly as the crown build did, writes DIR/ids.txt, DIR/r0bf/ids.jsonl and DIR/logs/.
#   scripts/reproduce-image-ids.sh compare --out DIR [--vault ADDR] [--rpc URL]
#       prints one line per program: rebuilt id, the README's id, the vault's immutable; exit 1 on a mismatch.
#
# Why a fixed path: a RISC Zero guest built without Docker embeds the absolute paths of its sources (panic
# locations of path dependencies and of the cargo registry), so its image id depends on where it was built. The
# deployed programs were built on the build box (HOME=/home/q) at the crown build path
#   /home/q/Downloads/software_i_built/logos-rfp/spike/ferrata-wt-crown
# with LEZ v0.4.0-rc1 at spike/upstream/lez-v0.4.0-rc1 and RISC0_HOME=spike/risc0-home-lez for lez/.
set -uo pipefail  # no -e: a find that matches nothing must not end the script; failures are checked
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$(cd "$HERE/.." && pwd)"
SPIKE_FIXED=/home/q/Downloads/software_i_built/logos-rfp/spike
BUILD_PATH="${FERRATA_BUILD_PATH:-$SPIKE_FIXED/ferrata-wt-crown}"
SPIKE="$(dirname "$BUILD_PATH")"
VAULT="0xfE3199fa2634413CbAC317371076Cae3EB2d9E54"
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
    # Refuse to run anywhere but a container or a throwaway build box: this writes to a fixed absolute path.
    if [ "${FERRATA_REPRO_CONTAINER:-}" != 1 ] && [ -z "${MODAL_TASK_ID:-}" ]; then
      echo "build: run inside the Docker image (scripts/reproduce-image-ids.sh docker) or a throwaway box;" \
           "it writes to $BUILD_PATH" >&2; exit 2
    fi
    [ "$SRC" != "$BUILD_PATH" ] || { echo "build: the tree is already at $BUILD_PATH" >&2; exit 2; }
    if [ -e "$BUILD_PATH" ] && [ ! -f "$BUILD_PATH/.reproduce-image-ids" ]; then
      echo "build: $BUILD_PATH exists and is not a copy made by this script; refusing" >&2; exit 2
    fi
    mkdir -p "$OUT/logs" "$OUT/programs" "$OUT/r0bf"; OUT="$(cd "$OUT" && pwd)"
    log() { echo "$(date -u +%T) $*" | tee -a "$OUT/logs/steps.txt"; }
    log "source $SRC ($(git -C "$SRC" rev-parse HEAD 2>/dev/null || echo 'not a git tree')) -> $BUILD_PATH; nproc $(nproc)"
    for d in "$SPIKE/upstream/lez-v0.4.0-rc1" "$SPIKE/risc0-home-lez"; do
      [ -d "$d" ] || { log "missing $d (the Dockerfile and the build box provide it)"; exit 1; }
    done
    log "toolchains: $(cargo --version), $(r0vm --version 2>&1 | head -1), lez rust $(ls "$SPIKE/risc0-home-lez/toolchains" 2>/dev/null | tr '\n' ' ')"
    mkdir -p "$BUILD_PATH"
    # The crown build copied the tree without its build outputs (integ5's crown-build.sh); the same here.
    rsync -a --delete --exclude .git --exclude target/ --exclude /vault/lib/ --exclude /vault/out/ \
      --exclude /vault/cache/ "$SRC/" "$BUILD_PATH/"
    touch "$BUILD_PATH/.reproduce-image-ids"
    log "tree copied: $(du -sh "$BUILD_PATH" | cut -f1)"
    export CARGO_TERM_COLOR=never
    ( cd "$BUILD_PATH/lez" && RISC0_HOME="$SPIKE/risc0-home-lez" RISC0_BUILD_LOCKED=1 CARGO_BUILD_JOBS=8 \
        cargo build -p methods --locked ) > "$OUT/logs/lez-methods.log" 2>&1 & a=$!
    ( cd "$BUILD_PATH/checkpoint" && CARGO_BUILD_JOBS=8 cargo build --release -p methods --locked ) \
      > "$OUT/logs/checkpoint-methods.log" 2>&1 & b=$!
    ( cd "$BUILD_PATH/bridge" && CARGO_BUILD_JOBS=8 cargo build --release -p methods --locked \
        && cargo build --release -p ferrata-e2e --locked ) > "$OUT/logs/bridge-methods.log" 2>&1 & c=$!
    fail=0
    for p in a b c; do if eval "wait \$$p"; then log "build $p ok"; else log "build $p FAILED"; fail=1; fi; done
    [ $fail = 0 ] || { tail -n 20 "$OUT"/logs/*-methods.log; exit 1; }
    for f in ferrata_lez.bin ferrata_bridge.bin; do
      bin="$(find "$BUILD_PATH/lez/target/riscv-guest" -path '*release*' -name "$f" | head -n 1)"
      [ -n "$bin" ] && cp "$bin" "$OUT/programs/$f"
    done
    for f in checkpoint_b checkpoint segment release; do
      for r in "$BUILD_PATH/checkpoint/target/riscv-guest" "$BUILD_PATH/bridge/target/riscv-guest"; do
        bin="$(find "$r" -path '*release*' \( -name "$f.bin" -o -name "$f" \) -type f 2>/dev/null | grep -v '/deps/\|/build/' | head -n 1)"
        [ -n "$bin" ] && { cp "$bin" "$OUT/programs/$f.bin"; break; }
      done
    done
    # The LEZ programs are R0BF already (r0vm prints their id); the other four are user ELFs that
    # ferrata-e2e program-binary combines with the v1compat kernel as risc0-build 3.0.5 does.
    ( cd "$OUT/programs" && for f in ferrata_lez.bin ferrata_bridge.bin; do
        printf '%s %s %s\n' "$f" "$(sha256sum "$f" | cut -c1-64)" "$(r0vm --elf "$f" --id 2>&1 | tail -n 1)"; done ) > "$OUT/ids.txt"
    E="$BUILD_PATH/bridge/target/release/ferrata-e2e"
    ( cd "$OUT/r0bf" && for f in checkpoint_b checkpoint segment release; do "$E" program-binary "../programs/$f.bin" "$f.bin"; done ) \
      > "$OUT/r0bf/ids.jsonl" 2> "$OUT/logs/program-binary.log"
    # Cross-check: the ids risc0-build itself computed for the methods crates (methods.rs in OUT_DIR).
    find "$BUILD_PATH"/{lez,checkpoint,bridge}/target -path '*/build/methods-*/out/methods.rs' 2>/dev/null \
      | xargs -r grep -h -E 'pub const [A-Z0-9_]+_ID: \[u32; 8\]' > "$OUT/methods-ids.txt" || true
    log "user ELFs: $(ls "$OUT/programs" | tr '\n' ' ')"
    cat "$OUT/ids.txt" "$OUT/r0bf/ids.jsonl"
    ;;
  compare)
    [ -n "$OUT" ] || { echo "compare: --out DIR" >&2; exit 2; }
    exec python3 -I "$HERE/reproduce/compare_ids.py" --out "$OUT" --readme "$SRC/e2e/crown/README.md" --vault "$VAULT" --rpc "$RPC"
    ;;
  docker)
    OUT="${OUT:-$SRC/reproduce-out}"; mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
    docker build -t ferrata-reproduce -f "$HERE/reproduce/Dockerfile" "$HERE/reproduce"
    docker run --rm -v "$SRC:/src:ro" -v "$OUT:/out" ferrata-reproduce \
      bash /src/scripts/reproduce-image-ids.sh build --out /out
    "$0" compare --out "$OUT" --vault "$VAULT" --rpc "$RPC"
    ;;
  *) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
