#!/usr/bin/env bash
# Stages what the real proofs of the bridge program's private claim and private burn need, in the
# form of gpu-prover/inputs-lez/: the prebuilt host binary lez/target/debug/prove_bridge (it embeds
# the bridge guest, the vectors it reads and LEZ's privacy-preserving circuit; the guest is not
# rebuilt anywhere else because its ELF holds absolute build paths), with a MANIFEST.json of its
# size and SHA-256. Nothing is proven here.
#
# usage: lez/scripts/stage-prove-bridge.sh OUTDIR [BINARY]
#   OUTDIR   where to put prove_bridge and MANIFEST.json
#   BINARY   the prove_bridge to stage (default: lez/target/debug/prove_bridge, or under
#            CARGO_TARGET_DIR if that is set)
#
# The image id is the one the binary reports (`prove_bridge --image-ids`). Set FERRATA_HEAD and
# FERRATA_DIRTY where the sources are a copy without `.git`.
set -euo pipefail
lez="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$lez/.." && pwd)"
out="${1:?usage: stage-prove-bridge.sh OUTDIR [BINARY]}"
bin="${2:-${CARGO_TARGET_DIR:-$lez/target}/debug/prove_bridge}"
[ -x "$bin" ] || { echo "no binary at $bin: build it with cargo build -p harness --locked --bin prove_bridge" >&2; exit 1; }
mkdir -p "$out"
cp -p "$bin" "$out/prove_bridge"
ids=$("$out/prove_bridge" --image-ids | head -1)
head="${FERRATA_HEAD:-$(git -C "$repo" rev-parse HEAD)}"
dirty="${FERRATA_DIRTY:-$(git -C "$repo" status --porcelain | wc -l)}"
python3 -I - "$out" "$head" "$dirty" "$ids" <<'PY'
import hashlib, json, pathlib, re, sys
root, head, dirty, ids = pathlib.Path(sys.argv[1]), sys.argv[2], int(sys.argv[3]), sys.argv[4]
bridge = re.search(r"bridge image ([0-9a-f]{64})", ids)
if not bridge:
    sys.exit(f"could not read the image id from the binary's output: {ids!r}")
files = {str(p.relative_to(root)): {"bytes": p.stat().st_size, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
         for p in sorted(root.rglob("*")) if p.is_file() and p.name != "MANIFEST.json"}
manifest = {
    "binary_from": "lez/target/debug/prove_bridge",
    "ferrata_commit": head,
    "ferrata_uncommitted_changes": dirty,
    "expected_bridge_image_id": bridge.group(1),
    "prove_command": "FERRATA_EXPECT_BRIDGE_IMAGE_ID=<expected_bridge_image_id> RISC0_DEV_MODE=0 "
                     "RISC0_SERVER_PATH=<r0vm 3.0.5 built with CUDA> /usr/bin/time -v ./prove_bridge "
                     "--claim-out bridge-private-claim-tx.borsh --burn-out bridge-private-burn-tx.borsh "
                     "> prove-bridge.log 2>&1",
    "files": files,
}
(root / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps(manifest, indent=2))
PY
