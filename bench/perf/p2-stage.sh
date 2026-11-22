#!/usr/bin/env bash
# RFP-021 P.2's proofs on parallel containers, step 1 on the build box (spike/modal-build's mrun): build perf_prove,
# dump every measured transition (each one's input, the executor's journal, the guests, index.tsv: perf_prove --dump),
# strip the binary and stage it with the dump and their sha256 in STAGE. Step 2 (from the laptop): bring STAGE back
# with mrun --get and run bench/perf/prove_modal.py. bench/perf/prove-actions.sh is the same proofs one after the
# other on one machine.
#
#   bash bench/perf/p2-stage.sh STAGE        # from the repository root; STAGE new or empty
#
# P2_WITH_PRIVATE=1 also stages the proven update path: prove_private (one private transaction proving a whole
# finality update, VerifyAll, at a private account: lez/RESULTS.md §3) with its fixture (mainnet-p1899), and its
# --handoff mode's dry run (a committee handoff in one private transaction).
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
stage="$(realpath -m "${1:?usage: p2-stage.sh STAGE}")"
# Never delete what the caller names: STAGE must be new or empty (it used to be removed first).
[ -e "$stage" ] && [ -n "$(ls -A "$stage")" ] && { echo "p2-stage: STAGE must be new or empty: $stage" >&2; exit 1; }
mkdir -p "$stage/bin"
cd "$root/lez"
export RISC0_HOME="${RISC0_HOME:-$root/../risc0-home-lez}"
cargo build -p harness --locked --bin perf_prove > "$stage/build.log" 2>&1 || { tail -30 "$stage/build.log"; exit 1; }
target/debug/perf_prove --dump "$stage/dump" > "$stage/dump.log" 2>&1 || { tail -30 "$stage/dump.log"; exit 1; }
strip -o "$stage/bin/perf_prove" target/debug/perf_prove
if [ "${P2_WITH_PRIVATE:-0}" = 1 ]; then
  cargo build -p harness --locked --bin prove_private >> "$stage/build.log" 2>&1 || { tail -30 "$stage/build.log"; exit 1; }
  strip -o "$stage/bin/prove_private" target/debug/prove_private
  mkdir -p "$stage/fixture" && cp "$root"/crates/ferrata-spectests/fixtures/mainnet-p1899/*.ssz "$stage/fixture/"
  RISC0_DEV_MODE=1 "$stage/bin/prove_private" --allow-dev-mode --tx-out "$stage/private-dry.borsh" \
    > "$stage/private-dry.log" 2>&1 || { tail -20 "$stage/private-dry.log"; exit 1; }
  head -1 "$stage/private-dry.log"
  # The committee handoff in one private transaction (prove_private --handoff), dry: its cycles and that it settles.
  RISC0_DEV_MODE=1 "$stage/bin/prove_private" --handoff --allow-dev-mode --tx-out "$stage/private-handoff-dry.borsh" \
    > "$stage/private-handoff-dry.log" 2>&1 || { tail -20 "$stage/private-handoff-dry.log"; exit 1; }
  grep -E 'private transition|settled:' "$stage/private-handoff-dry.log"
fi
r0vm="$(readlink -f "$(command -v r0vm)")"
{
  echo "date: $(date -u +%FT%TZ)"
  # The build box's copy of the tree holds no .git: pass FERRATA_HEAD there.
  echo "head: ${FERRATA_HEAD:-$(git -C "$root" rev-parse HEAD 2>/dev/null || echo unknown)}"
  echo "r0vm: $(r0vm --version) $(sha256sum "$r0vm" | cut -c1-64)"
  echo "transitions: $(($(wc -l < "$stage/dump/index.tsv") - 1))"
} > "$stage/stage.txt"
( cd "$stage" && find bin dump $([ -d fixture ] && echo fixture) -type f | sort | xargs sha256sum > SHA256SUMS )
cat "$stage/stage.txt"
du -sh "$stage/bin" "$stage/dump"
