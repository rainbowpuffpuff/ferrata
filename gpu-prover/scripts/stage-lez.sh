#!/usr/bin/env bash
# Copies what the LEZ light-client update proof needs into inputs-lez/ (git-ignored except
# MANIFEST.json): the prebuilt host binary ../lez/target/debug/prove_private (it embeds both guests
# and LEZ's privacy-preserving circuit; the guests are not rebuilt anywhere else because their ELFs
# hold absolute build paths) and the six mainnet-p1899 fixtures it reads. Read-only outside
# inputs-lez/. The copy freezes the binary that was hashed. (The proof in RESULTS.md was staged when
# lez/ and the core were two repositories, hence the two commits in the manifest; here both are
# this repository's.)
#
# usage: scripts/stage-lez.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/.." && pwd)"
lez="$repo/lez"
fx="$repo/crates/ferrata-spectests/fixtures/mainnet-p1899"
out="$here/inputs-lez"
rm -rf "$out/prove_private" "$out/mainnet-p1899"
mkdir -p "$out/mainnet-p1899"
cp -p "$lez/target/debug/prove_private" "$out/"
cp -p "$fx"/bootstrap.ssz "$fx"/update_{0,1,2,3,4}.ssz "$fx/SHA256SUMS" "$out/mainnet-p1899/"
(cd "$out/mainnet-p1899" && sha256sum -c SHA256SUMS)
python3 -I - "$out" "$(git -C "$repo" rev-parse HEAD)" "$(git -C "$repo" status --porcelain | wc -l)" \
  "$(git -C "$repo" rev-parse HEAD)" <<'EOF'
import hashlib, json, pathlib, sys
root, lez_commit, lez_dirty, ferrata_commit = pathlib.Path(sys.argv[1]), sys.argv[2], int(sys.argv[3]), sys.argv[4]
files = {str(p.relative_to(root)): {"bytes": p.stat().st_size, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
         for p in sorted(root.rglob("*")) if p.is_file() and p.name != "MANIFEST.json"}
manifest = {
    "binary_from": "lez/target/debug/prove_private", "ferrata_lez_commit": lez_commit,
    "ferrata_lez_uncommitted_changes": lez_dirty, "fixtures_from": "crates/ferrata-spectests/fixtures/mainnet-p1899",
    "ferrata_commit": ferrata_commit,
    "expected_image_id": "cec0bc687443be08c29c3216de8a3ed5f00a29d224fc266fe272f78cfe0f4d78",
    "files": files,
}
(root / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps(manifest, indent=2))
EOF
