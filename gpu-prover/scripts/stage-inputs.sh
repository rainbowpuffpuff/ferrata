#!/usr/bin/env bash
# Copies what Modal proves into inputs/ (git-ignored except MANIFEST.json): the guest program
# binaries of the local ferrata-checkpoint build (the .bin files its host embeds) and the fixtures.
# Writes inputs/MANIFEST.json with the source commit, the image ids the local host reports
# (`host ids`, no proving) and the sha256 of every file. Only reads ../checkpoint, which must be built
# (target/release/host) and have its fixtures generated (checkpoint/README.md).
#
# usage: scripts/stage-inputs.sh [fixture…]   (default: cp-empty-1 cp-empty-360 cp-pp5-360 cp-pp50-360)
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
fc="$(cd "$here/../checkpoint" && pwd)"
fixtures=("$@")
[ ${#fixtures[@]} -gt 0 ] || fixtures=(cp-empty-1 cp-empty-360 cp-pp5-360 cp-pp50-360)
guest="$fc/target/riscv-guest"

rm -rf "$here/inputs/programs" "$here/inputs/fixtures"
mkdir -p "$here/inputs/programs" "$here/inputs/fixtures"
cp -p "$guest/methods/ferrata-checkpoint-guest/riscv32im-risc0-zkvm-elf/release/checkpoint.bin" \
      "$guest/methods-segment/ferrata-segment-guest/riscv32im-risc0-zkvm-elf/release/segment.bin" \
      "$here/inputs/programs/"
for f in "${fixtures[@]}"; do
  cp -p "$fc/fixtures/$f.input" "$fc/fixtures/$f.json" "$here/inputs/fixtures/"
done

commit="$(git -C "$fc" rev-parse HEAD)"
dirty="$(git -C "$fc" status --porcelain -- . | wc -l)"
ids="$("$fc/target/release/host" ids)"
python3 -I - "$here/inputs" "$commit" "$dirty" "$ids" <<'EOF'
import hashlib, json, pathlib, sys
root, commit, dirty, ids = pathlib.Path(sys.argv[1]), sys.argv[2], int(sys.argv[3]), sys.argv[4]
local_ids = dict(line.split() for line in ids.splitlines() if line.strip())
files = {}
for p in sorted(root.glob("*/*")):
    if p.parent.name in ("programs", "fixtures"):
        b = p.read_bytes()
        files[str(p.relative_to(root))] = {"bytes": len(b), "sha256": hashlib.sha256(b).hexdigest()}
manifest = {
    "source": "checkpoint", "commit": commit, "uncommitted_changes": dirty,
    "programs_from": "target/riscv-guest/…/release/{checkpoint,segment}.bin (risc0-build 3.0.5, rzup Rust 1.88.0, no Docker)",
    "local_image_ids": {"checkpoint": local_ids["checkpoint"], "segment": local_ids["segment"]},
    "local_image_ids_from": "target/release/host ids (the ids compiled into the local host)",
    "files": files,
}
(root / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps(manifest, indent=2))
EOF
