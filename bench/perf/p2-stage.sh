#!/usr/bin/env bash
# RFP-021 P.2's proofs on parallel containers, step 1 on the build box (spike/modal-build's mrun): build perf_prove,
# dump every measured transition (each one's input, the executor's journal, the guests, index.tsv: perf_prove --dump),
# strip the binary and stage it with the dump and their sha256 in STAGE. Step 2 (from the laptop): bring STAGE back
# with mrun --get and run bench/perf/prove_modal.py. bench/perf/prove-actions.sh is the same proofs one after the
# other on one machine.
#
#   bash bench/perf/p2-stage.sh STAGE        # from the repository root
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
stage="$(realpath -m "${1:?usage: p2-stage.sh STAGE}")"
rm -rf "$stage"; mkdir -p "$stage/bin"
cd "$root/lez"
export RISC0_HOME="${RISC0_HOME:-$root/../risc0-home-lez}"
cargo build -p harness --locked --bin perf_prove > "$stage/build.log" 2>&1 || { tail -30 "$stage/build.log"; exit 1; }
target/debug/perf_prove --dump "$stage/dump" > "$stage/dump.log" 2>&1 || { tail -30 "$stage/dump.log"; exit 1; }
strip -o "$stage/bin/perf_prove" target/debug/perf_prove
r0vm="$(readlink -f "$(command -v r0vm)")"
{
  echo "date: $(date -u +%FT%TZ)"
  echo "head: $(git -C "$root" rev-parse HEAD)"
  echo "uncommitted changes: $(git -C "$root" status --porcelain --untracked-files=no | wc -l)"
  echo "r0vm: $(r0vm --version) $(sha256sum "$r0vm" | cut -c1-64)"
  echo "transitions: $(($(wc -l < "$stage/dump/index.tsv") - 1))"
} > "$stage/stage.txt"
( cd "$stage" && find bin dump -type f | sort | xargs sha256sum > SHA256SUMS )
cat "$stage/stage.txt"
du -sh "$stage/bin" "$stage/dump"
