#!/usr/bin/env bash
# Builds and packs the crown run's LEZ side on the build box (spike/modal-build's mrun; nothing heavy elsewhere).
#
#   build.sh seed     copy the crown build (sequencer_service, the node, the live target directory, the guests,
#                     vault/lib) from the crown worktree's box snapshot on the same Volume (a new box has none)
#   build.sh driver   crown-driver (driver/), in the live target directory the sequencer was built in
#   build.sh pack     bundle.tar.zst in $CROWN_LEZ_TMP (lez/lab's layout: bin/, node/, xdg-cache/, program/, plus
#                     lez-config/, fixture/, crown/ and SHA256SUMS): what session.sh runs from
#   build.sh all      driver and pack
#
# $CROWN_LIVE (default tmp/crown/live beside the repository) holds the sequencer, the node and the target directory;
# $CROWN_PROGRAMS (default tmp/crown/build/programs) the guests built at the crown path (their image ids are the run's).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
CROWN_LEZ_TMP="${CROWN_LEZ_TMP:-$BESIDE/tmp/crown-lez}"
LIVE="${CROWN_LIVE:-$BESIDE/tmp/crown/live}"
PROGRAMS="${CROWN_PROGRAMS:-$BESIDE/tmp/crown/build/programs}"
LEZ="$BESIDE/upstream/lez-v0.4.0-rc1"
FIXTURE="$REPO/crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10"
mkdir -p "$CROWN_LEZ_TMP/logs" "$CROWN_LEZ_TMP/bin"

case "${1:-}" in
  seed)
    snap=/vol/snap/ferrata-wt-crown.tar.zst
    [ -f "$snap" ] || { echo "no $snap (run on the ws-4 build box)"; exit 1; }
    rel() { echo "${1#/}"; }
    t0=$(date +%s)
    zstd -q -d -T0 -c "$snap" | tar -x -C / \
      "$(rel "$BESIDE/tmp/crown/live")" "$(rel "$BESIDE/tmp/crown/build")" "$(rel "$BESIDE/ferrata-wt-crown/vault/lib")" \
      "$(rel "$HOME/.cargo/registry")" "$(rel "$HOME/.cargo/git")"
    mkdir -p "$REPO/vault/lib"
    cp -a "$BESIDE/ferrata-wt-crown/vault/lib/." "$REPO/vault/lib/"
    echo "seeded in $(( $(date +%s) - t0 )) s: $(ls "$LIVE/bin" | tr '\n' ' '); programs: $(ls "$PROGRAMS" | tr '\n' ' ')"
    ;;
  driver|all)
    [ -f "$HERE/driver/Cargo.lock" ] || cp "$REPO/lez/live/driver/Cargo.lock" "$HERE/driver/Cargo.lock"
    ( cd "$HERE/driver" && CARGO_TARGET_DIR="$LIVE/target" CARGO_BUILD_JOBS="${CROWN_JOBS:-16}" cargo build --release )
    cp "$LIVE/target/release/crown-driver" "$CROWN_LEZ_TMP/bin/crown-driver"
    sha256sum "$CROWN_LEZ_TMP/bin/crown-driver"
    if [ "$1" = all ]; then exec "$0" pack; fi
    ;;
  pack)
    b="$CROWN_LEZ_TMP/bundle"
    rm -rf "$b" && mkdir -p "$b/bin" "$b/node" "$b/program" "$b/lez-config" "$b/fixture" "$b/crown"
    cp "$LIVE/bin/sequencer_service" "$b/bin/"
    [ -f "$LIVE/bin/sequencer_service.toolchain" ] && cp "$LIVE/bin/sequencer_service.toolchain" "$b/bin/"
    cp "$CROWN_LEZ_TMP/bin/crown-driver" "$b/bin/"
    # LEZ executes and proves programs through RISC Zero's r0vm (3.0.5).
    cp "$(readlink -f "$(command -v r0vm)")" "$b/bin/r0vm"
    "$b/bin/r0vm" --version > "$b/bin/r0vm.version"
    cp "$LIVE/bedrock/.resolved-host/logos-blockchain-node" "$b/node/"
    [ -f "$LIVE/bedrock/.resolved-host/metadata.json" ] && cp "$LIVE/bedrock/.resolved-host/metadata.json" "$b/node/"
    cp -r "$LIVE/xdg-cache" "$b/xdg-cache"
    cp "$PROGRAMS/ferrata_lez.bin" "$PROGRAMS/ferrata_bridge.bin" "$b/program/"
    cp "$LEZ/lez/sequencer/service/networks/local.json" "$LEZ/bedrock/node-config.yaml" "$LEZ/bedrock/deployment-settings.yaml" "$b/lez-config/"
    cp "$FIXTURE/bootstrap.ssz" "$FIXTURE"/update_1[34]*.ssz "$b/fixture/"
    cp "$HERE/session.sh" "$b/crown/"
    ( cd "$b" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS )
    tar -C "$b" -cf - . | zstd -q -T0 -9 -o "$CROWN_LEZ_TMP/bundle.tar.zst" -f
    echo "bundle: $(stat -c %s "$CROWN_LEZ_TMP/bundle.tar.zst") bytes, $(wc -l < "$b/SHA256SUMS") files"
    grep -E ' \./(bin|program)/' "$b/SHA256SUMS"
    ;;
  *) sed -n '2,13p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
