#!/usr/bin/env bash
# The LEZ side of a zone run, built from one LEZ tag's tree and packed as the bundle attestor/zone.sh (and
# zone-v040.sh) start from: bin/ (sequencer_service, its `standalone` build, LEZ's wallet, r0vm), node/logos-blockchain-node (the Logos Blockchain node the tag pins, by LEZ's own resolver), xdg-cache/ (the
# circuit artifacts the Logos crates download at build time and read at run time), lez-config/ (the tag's
# node-config.yaml, deployment-settings.yaml, local.json and lez-tag.txt) and SHA256SUMS. No Ferrata program: a run
# that deploys them puts them in program/ itself. Heavy (about 30 min on 16 cores): run on the build box.
#
#   checkpoint/attestor/zone-bundle.sh TAG OUT       # TAG v0.4.0 (or v0.4.0-rc3); OUT: a directory outside the repository
#   checkpoint/attestor/zone-bundle.sh add-programs BUNDLE PROGRAMS FIXTURE NEW
#       BUNDLE (a bundle of the first form) with program/ferrata_lez.bin and program/ferrata_bridge.bin from PROGRAMS
#       (built for the bundle's tag: lez/scripts/lez-select.sh) and fixture/ from FIXTURE (a Sepolia checkpoint fixture
#       of crates/ferrata-spectests/fixtures/: the bootstrap, the period updates, the second provider's headers,
#       SOURCE.md, as lez/crown/build.sh packs it), SHA256SUMS rewritten, as NEW: what zone-bridge.sh's ZB_BUNDLE is.
#       Nothing else of BUNDLE changes; PROGRAMS/ids.jsonl, when there, must name each program's sha256 (one entry per
#       program, by file name or path).
#
# LEZ's tree is upstream/lez-TAG beside the repository, at the tag's commit and unmodified (as lez/scripts/lez-select.sh
# checks); ZONE_BUNDLE_JOBS (default 16) cargo jobs. Nothing is written into the LEZ tree: the target directories, the
# node's resolution and the cache are under OUT.
set -euo pipefail
if [ "${1:-}" = add-programs ]; then
  BUNDLE="$(realpath "${2:?add-programs BUNDLE PROGRAMS FIXTURE NEW}")"; PROGS="$(realpath "${3:?PROGRAMS}")"
  FIX="$(realpath "${4:?FIXTURE}")"; NEW="$(realpath -m "${5:?NEW}")"
  b="$(dirname "$NEW")/.add-programs-$$"; rm -rf "$b"; mkdir -p "$b"; trap 'rm -rf "$b"' EXIT
  zstd -q -d -c "$BUNDLE" | tar -C "$b" -xf -
  ( cd "$b" && sha256sum -c --quiet SHA256SUMS ) || { echo "zone-bundle: $BUNDLE fails its SHA256SUMS" >&2; exit 1; }
  mkdir -p "$b/program" "$b/fixture"
  for p in ferrata_lez ferrata_bridge; do
    [ -f "$PROGS/$p.bin" ] || { echo "zone-bundle: no $PROGS/$p.bin" >&2; exit 1; }
    if [ -f "$PROGS/ids.jsonl" ]; then
      want="$(python3 -I -c '
import json, os, sys
shas = {json.loads(l)["sha256"] for l in open(sys.argv[1]) if l.strip()
        and os.path.basename(json.loads(l)["program"]).removesuffix(".bin") == sys.argv[2]}
print(shas.pop() if len(shas) == 1 else "")' "$PROGS/ids.jsonl" "$p")"
      [ -n "$want" ] || { echo "zone-bundle: ids.jsonl names no single sha256 for $p.bin" >&2; exit 1; }
      [ "$(sha256sum "$PROGS/$p.bin" | cut -c1-64)" = "$want" ] || { echo "zone-bundle: $p.bin is not the sha256 ids.jsonl names" >&2; exit 1; }
    fi
    cp "$PROGS/$p.bin" "$b/program/$p.bin"
  done
  cp "$FIX/bootstrap.ssz" "$FIX"/update_*.ssz "$FIX"/beacon_header_*_second.json "$FIX/SOURCE.md" "$b/fixture/"
  ( cd "$b" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS )
  tar -C "$b" --sort=name --owner=0 --group=0 --numeric-owner -cf - . | zstd -q -T0 -9 -o "$NEW" -f
  echo "bundle: $NEW, $(stat -c %s "$NEW") bytes, sha256 $(sha256sum "$NEW" | cut -c1-64)"; grep -E ' \./(program|fixture)/' "$b/SHA256SUMS"
  exit 0
fi
TAG="${1:?zone-bundle.sh TAG OUT}"; OUT="$(realpath -m "${2:?zone-bundle.sh TAG OUT}")"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"; BESIDE="$(cd "$REPO/.." && pwd)"
case "$OUT/" in "$REPO"/*) echo "zone-bundle: OUT must be outside the repository" >&2; exit 2 ;; esac
. "$HERE/zone-lib.sh"
COMMIT="$(zl_lez_commit "$TAG")" || { echo "zone-bundle: unknown LEZ tag $TAG" >&2; exit 2; }
R0VM_VERSION="$(zl_r0vm_version "$TAG")"
LEZ="${ZONE_BUNDLE_LEZ:-$BESIDE/upstream/lez-$TAG}"
[ -f "$LEZ/Cargo.toml" ] || { echo "zone-bundle: LEZ $TAG not found at $LEZ" >&2; exit 2; }
[ "$(git -C "$LEZ" rev-parse HEAD)" = "$COMMIT" ] || { echo "zone-bundle: $LEZ is not at $COMMIT ($TAG)" >&2; exit 2; }
[ -z "$(git -C "$LEZ" status --porcelain)" ] || { echo "zone-bundle: $LEZ is modified" >&2; exit 2; }
mkdir -p "$OUT/logs"; B="$OUT/bundle"; rm -rf "$B"; mkdir -p "$B/bin" "$B/node" "$B/lez-config"
export CARGO_TARGET_DIR="$OUT/target" XDG_CACHE_HOME="$OUT/xdg-cache" CARGO_BUILD_JOBS="${ZONE_BUNDLE_JOBS:-16}"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$OUT/logs/zone-bundle.log"; }

log "LEZ $TAG ($COMMIT): the Logos Blockchain node by LEZ's resolver (host)"
( cd "$LEZ" && RUSTFLAGS="-A single-use-lifetimes" RISC0_DEV_MODE=1 python3 -I bedrock/tools/resolve_bedrock_node.py \
    --target-platform host --output-directory "$OUT/node-resolved" --target-directory "$OUT/target-bedrock-node" ) \
  > "$OUT/logs/node.log" 2>&1 || { tail -n 30 "$OUT/logs/node.log"; exit 1; }
cp "$OUT/node-resolved/logos-blockchain-node" "$B/node/logos-blockchain-node"
cp "$OUT/node-resolved/metadata.json" "$B/node/metadata.json" 2>/dev/null || true

log "sequencer_service and wallet (release, --locked)"
( cd "$LEZ" && cargo build --release --locked -p sequencer_service -p wallet --bins ) > "$OUT/logs/build.log" 2>&1 \
  || { tail -n 40 "$OUT/logs/build.log"; exit 1; }
for b in sequencer_service wallet; do cp "$CARGO_TARGET_DIR/release/$b" "$B/bin/$b"; done
log "sequencer_service --features standalone (its mock Bedrock actor)"
( cd "$LEZ" && CARGO_TARGET_DIR="$OUT/target-standalone" cargo build --release --locked -p sequencer_service --features standalone ) \
  > "$OUT/logs/build-standalone.log" 2>&1 || { tail -n 40 "$OUT/logs/build-standalone.log"; exit 1; }
cp "$OUT/target-standalone/release/sequencer_service" "$B/bin/sequencer_service_standalone"

log "r0vm $R0VM_VERSION (the risc0-zkvm of LEZ $TAG's lockfile)"
r0="$(zl_find_r0vm "$R0VM_VERSION" "$HOME/.risc0" "$BESIDE/risc0-home-lez" || true)"
if [ -z "$r0" ]; then
  RISC0_HOME="$HOME/.risc0" rzup install r0vm "$R0VM_VERSION" > "$OUT/logs/rzup.log" 2>&1 || { tail -n 20 "$OUT/logs/rzup.log"; exit 1; }
  r0="$(zl_find_r0vm "$R0VM_VERSION" "$HOME/.risc0")"
fi
cp "$r0" "$B/bin/r0vm"; "$B/bin/r0vm" --version | tee -a "$OUT/logs/zone-bundle.log"

mkdir -p "$B/xdg-cache"; [ -d "$XDG_CACHE_HOME/logos" ] && cp -r "$XDG_CACHE_HOME/logos" "$B/xdg-cache/logos"
cp "$LEZ/lez/sequencer/service/networks/local.json" "$LEZ/bedrock/node-config.yaml" "$LEZ/bedrock/deployment-settings.yaml" "$B/lez-config/"
echo "$TAG $COMMIT" > "$B/lez-config/lez-tag.txt"
( cd "$LEZ" && rustc --version && cargo --version ) > "$B/lez-config/toolchain.txt"
( cd "$B" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS )
tar -C "$B" --sort=name --owner=0 --group=0 --numeric-owner -cf - . | zstd -q -T0 -9 -o "$OUT/bundle.tar.zst" -f
log "bundle: $OUT/bundle.tar.zst, $(stat -c %s "$OUT/bundle.tar.zst") bytes, sha256 $(sha256sum "$OUT/bundle.tar.zst" | cut -c1-64)"
grep -E ' \./(bin|node)/' "$B/SHA256SUMS" | tee -a "$OUT/logs/zone-bundle.log"
