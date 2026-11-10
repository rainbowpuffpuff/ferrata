#!/usr/bin/env bash
# Stages what one real proof of the private attestation path needs, in the form of
# gpu-prover/inputs-lez/: the prebuilt host binary lez/target/debug/prove_attest (it embeds the
# consumer and program guests and LEZ's privacy-preserving circuit; the guests are not rebuilt
# anywhere else because their ELFs hold absolute build paths) and the fixture files it reads, with
# a MANIFEST.json of their sizes and SHA-256 sums. Nothing is proven here.
#
# usage: lez/scripts/stage-prove-attest.sh OUTDIR [BINARY]
#   OUTDIR   where to put prove_attest, mainnet-attest-2026-11-10/ and MANIFEST.json
#   BINARY   the prove_attest to stage (default: lez/target/debug/prove_attest, or under
#            CARGO_TARGET_DIR if that is set)
#
# The image ids are the ones the binary reports (`prove_attest --image-ids`). Set FERRATA_HEAD and
# FERRATA_DIRTY where the sources are a copy without `.git`.
set -euo pipefail
lez="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$lez/.." && pwd)"
out="${1:?usage: stage-prove-attest.sh OUTDIR [BINARY]}"
bin="${2:-${CARGO_TARGET_DIR:-$lez/target}/debug/prove_attest}"
fixture=mainnet-attest-2026-11-10
fx="$repo/crates/ferrata-spectests/fixtures/$fixture"
[ -x "$bin" ] || { echo "no binary at $bin: build it with cargo build -p harness --locked --bin prove_attest" >&2; exit 1; }
mkdir -p "$out/$fixture"
cp -p "$bin" "$out/prove_attest"
# What Fixture::load and the private attestation read (harness/src/attest.rs).
block=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["block"])' "$fx/fixture.json")
for f in SHA256SUMS fixture.json bootstrap.ssz finality_update.ssz "receipts_$block.json"; do
  cp -p "$fx/$f" "$out/$fixture/"
done
(cd "$out/$fixture" && sha256sum -c --ignore-missing --quiet SHA256SUMS)
ids=$("$out/prove_attest" --image-ids | head -1)
head="${FERRATA_HEAD:-$(git -C "$repo" rev-parse HEAD)}"
dirty="${FERRATA_DIRTY:-$(git -C "$repo" status --porcelain | wc -l)}"
python3 -I - "$out" "$head" "$dirty" "$ids" "$fixture" <<'PY'
import hashlib, json, pathlib, re, sys
root, head, dirty, ids, fixture = pathlib.Path(sys.argv[1]), sys.argv[2], int(sys.argv[3]), sys.argv[4], sys.argv[5]
consumer = re.search(r"consumer image ([0-9a-f]{64})", ids)
program = re.search(r"program image ([0-9a-f]{64})", ids)
if not consumer or not program:
    sys.exit(f"could not read the image ids from the binary's output: {ids!r}")
files = {str(p.relative_to(root)): {"bytes": p.stat().st_size, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
         for p in sorted(root.rglob("*")) if p.is_file() and p.name != "MANIFEST.json"}
manifest = {
    "binary_from": "lez/target/debug/prove_attest",
    "ferrata_commit": head,
    "ferrata_uncommitted_changes": dirty,
    "fixtures_from": f"crates/ferrata-spectests/fixtures/{fixture}",
    "expected_consumer_image_id": consumer.group(1),
    "program_image_id": program.group(1),
    "prove_command": "FERRATA_EXPECT_CONSUMER_IMAGE_ID=<expected_consumer_image_id> "
                     f"FERRATA_ATTEST_FIXTURE_DIR=<this directory>/{fixture} RISC0_DEV_MODE=0 "
                     "RISC0_SERVER_PATH=<r0vm 3.0.5 built with CUDA> /usr/bin/time -v ./prove_attest "
                     "--tx-out stage4-private-attest-tx.borsh > prove-attest.log 2>&1",
    "files": files,
}
(root / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps(manifest, indent=2))
PY
