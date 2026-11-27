#!/usr/bin/env bash
# Builds and packs the crown run's LEZ side on the build box (spike/modal-build's mrun; nothing heavy elsewhere).
#
#   build.sh seed     copy the crown build (sequencer_service, the node, the live target directory, the guests,
#                     vault/lib) from the crown worktree's box snapshot on the same Volume (a new box has none)
#   build.sh driver   crown-driver (driver/), in the live target directory the sequencer was built in
#   build.sh pack     bundle.tar.zst in $CROWN_LEZ_TMP (lez/lab's layout: bin/, node/, xdg-cache/, program/, plus
#                     lez-config/, fixture/, crown/ and SHA256SUMS): what session.sh runs from
#   build.sh all      driver and pack
#   build.sh repack FROM  FROM (a packed bundle.tar.zst) with this tree's crown-driver ($CROWN_LEZ_TMP/bin), fixture/
#                     and session.sh; everything else (sequencer, node, r0vm, xdg-cache, lez-config, the programs) kept
#                     byte for byte; SHA256SUMS rewritten; -> $CROWN_LEZ_TMP/bundle.tar.zst
#   build.sh rebase FROM  another LEZ tag under FROM's programs: FROM's ferrata_lez.bin, ferrata_bridge.bin and r0vm kept
#                     byte for byte; the sequencer, the node and xdg-cache/logos from $CROWN_LIVE (that tag's build, as
#                     lez/live/run.sh build-sequencer and build-bedrock make them), lez-config/ from $CROWN_LEZ_SRC (that
#                     tag's tree), this tree's crown-driver ($CROWN_LEZ_TMP/bin, built with that tag selected), fixture/
#                     and session.sh; lez-config/lez-tag.txt names the tag; -> $CROWN_LEZ_TMP/bundle.tar.zst
#
# This tree's crown-driver speaks only the bridge program of docs/image-ids.md's `ferrata_bridge_f13` (since ADR 0046)
# (accounts keyed by the configuration): pack, repack and rebase put it in place of a bundle's other program/
# ferrata_bridge.bin (the bundles of November 2026 carry the live runs' program): CROWN_BRIDGE_BIN if set (B1's
# ferrata_bridge.bin, which scripts/reproduce-image-ids.sh build --out DIR writes to DIR/programs/), otherwise B1's built
# from this tree as that script builds it (LEZ's methods, RISC0_HOME_LEZ); they stop if neither is possible
# (e2e/crown/README.md, "Since milestone B1").
# $CROWN_LIVE (default tmp/crown/live beside the repository) holds the sequencer, the node and the target directory;
# $CROWN_PROGRAMS (default tmp/crown/build/programs) the guests built at the crown path (their image ids are the run's).
# The fixture: the Sepolia checkpoint the instance starts from ($CROWN_FIXTURE, default the 12 Nov 2026 one; the
# period-1378 replay fixture left its weak-subjectivity period on 21 Oct 2026): what crown-driver's sepolia_base reads.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
CROWN_LEZ_TMP="${CROWN_LEZ_TMP:-$BESIDE/tmp/crown-lez}"
LIVE="${CROWN_LIVE:-$BESIDE/tmp/crown/live}"
PROGRAMS="${CROWN_PROGRAMS:-$BESIDE/tmp/crown/build/programs}"
LEZ="${CROWN_LEZ_SRC:-$BESIDE/upstream/lez-v0.4.0-rc1}"
FIXTURE="${CROWN_FIXTURE:-$REPO/crates/ferrata-spectests/fixtures/sepolia-2026-11-12}"
mkdir -p "$CROWN_LEZ_TMP/logs" "$CROWN_LEZ_TMP/bin"

# The fixture's files crown-driver reads: the bootstrap, every period update, the second provider's headers (the
# checkpoint root), and SOURCE.md for the record.
fixture() {
  rm -rf "$1/fixture" && mkdir -p "$1/fixture"
  cp "$FIXTURE/bootstrap.ssz" "$FIXTURE"/update_*.ssz "$FIXTURE"/beacon_header_*_second.json "$FIXTURE/SOURCE.md" "$1/fixture/"
}
sums() { ( cd "$1" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS ); }
# B1's bridge program built from this tree as scripts/reproduce-image-ids.sh builds the LEZ guests (path-independent,
# so the same bytes anywhere): LEZ's methods with LEZ's RISC Zero home (RISC0_HOME_LEZ, default beside the repository).
bridge_b1_build() {
  local r0="${RISC0_HOME_LEZ:-$REPO/../risc0-home-lez}" bin
  [ -d "$r0" ] || {
    echo "the bundle's bridge program is not B1's and CROWN_BRIDGE_BIN is not set; building B1's needs LEZ's RISC Zero" \
      "home ($r0 is missing: set RISC0_HOME_LEZ), or set CROWN_BRIDGE_BIN to B1's ferrata_bridge.bin" \
      "(scripts/reproduce-image-ids.sh build --out DIR writes DIR/programs/ferrata_bridge.bin)" >&2
    exit 1
  }
  echo "the bundle's bridge program is not B1's: building B1's from this tree (LEZ's methods; CROWN_BRIDGE_BIN names" \
    "another)" >&2
  ( cd "$REPO/lez" && env -u CARGO_TARGET_DIR -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS -u FERRATA_GUEST_REMAP_EXTRA \
      RUSTC_WRAPPER="$REPO/scripts/guest-rustc" RISC0_BUILD_LOCKED=1 RISC0_HOME="$r0" cargo build -p methods --locked ) >&2 \
    || { echo "building LEZ's methods failed: set CROWN_BRIDGE_BIN to B1's ferrata_bridge.bin" >&2; exit 1; }
  bin="$(find "$REPO/lez/target/riscv-guest" -path '*release*' -name ferrata_bridge.bin | head -n 1)"
  [ -n "$bin" ] || { echo "no ferrata_bridge.bin under lez/target/riscv-guest after the build" >&2; exit 1; }
  echo "$bin"
}

# The bundle's bridge program: CROWN_BRIDGE_BIN if given (repack, rebase), and in any case the one this tree's
# crown-driver speaks (the sha256 docs/image-ids.md records for `ferrata_bridge_f13`). A bundle that carries another
# (crown-live-3's, the live runs' program) gets B1's built from this tree when CROWN_BRIDGE_BIN is not set.
bridge_b1() {
  if [ -n "${CROWN_BRIDGE_BIN:-}" ]; then cp "$CROWN_BRIDGE_BIN" "$1/program/ferrata_bridge.bin"; fi
  local want got
  want="$( { grep -F '`ferrata_bridge_f13` |' "$REPO/docs/image-ids.md" || true; } | awk -F'|' '{gsub(/[` ]/, "", $4); print $4}')"
  got="$( { sha256sum "$1/program/ferrata_bridge.bin" 2> /dev/null || true; } | cut -c1-64)"
  if [ -z "${CROWN_BRIDGE_BIN:-}" ] && [ -n "$want" ] && [ "$got" != "$want" ] && [ -d "$1/program" ]; then
    cp "$(bridge_b1_build)" "$1/program/ferrata_bridge.bin"
    got="$(sha256sum "$1/program/ferrata_bridge.bin" | cut -c1-64)"
  fi
  [ -n "$want" ] && [ "$got" = "$want" ] || {
    echo "program/ferrata_bridge.bin has sha256 $got, not the bridge program this tree's crown-driver speaks ($want," \
      "docs/image-ids.md \`ferrata_bridge_f13\`): set CROWN_BRIDGE_BIN to this tree's ferrata_bridge.bin" >&2
    exit 1
  }
}

case "${1:-}" in
  seed)
    snap=/vol/snap/ferrata-wt-crown.tar.zst
    [ -f "$snap" ] || { echo "no $snap (run on the Modal build box)"; exit 1; }
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
    bridge_b1 "$b"
    fixture "$b"
    cp "$HERE/session.sh" "$b/crown/"
    sums "$b"
    tar -C "$b" -cf - . | zstd -q -T0 -9 -o "$CROWN_LEZ_TMP/bundle.tar.zst" -f
    echo "bundle: $(stat -c %s "$CROWN_LEZ_TMP/bundle.tar.zst") bytes, $(wc -l < "$b/SHA256SUMS") files"
    grep -E ' \./(bin|program)/' "$b/SHA256SUMS"
    ;;
  repack)
    from="${2:?build.sh repack FROM (a packed bundle.tar.zst)}"
    b="$CROWN_LEZ_TMP/bundle"
    rm -rf "$b" && mkdir -p "$b"
    zstd -q -d -c "$from" | tar -x -C "$b"
    ( cd "$b" && sha256sum -c --quiet SHA256SUMS ) || { echo "$from: its SHA256SUMS do not hold"; exit 1; }
    cp "$CROWN_LEZ_TMP/bin/crown-driver" "$b/bin/crown-driver"
    bridge_b1 "$b"
    fixture "$b"
    cp "$HERE/session.sh" "$b/crown/session.sh"
    sums "$b"
    tar -C "$b" --sort=name --owner=0 --group=0 --numeric-owner -cf - . | zstd -q -T0 -9 -o "$CROWN_LEZ_TMP/bundle.tar.zst" -f
    echo "bundle: $(stat -c %s "$CROWN_LEZ_TMP/bundle.tar.zst") bytes, $(wc -l < "$b/SHA256SUMS") files, sha256 $(sha256sum "$CROWN_LEZ_TMP/bundle.tar.zst" | cut -c1-64)"
    grep -E ' \./(bin|program|crown|fixture)/' "$b/SHA256SUMS"
    ;;
  rebase)
    from="${2:?build.sh rebase FROM (a packed bundle.tar.zst whose programs and r0vm are kept)}"
    b="$CROWN_LEZ_TMP/bundle"; f="$CROWN_LEZ_TMP/from"
    rm -rf "$b" "$f" && mkdir -p "$f" "$b/bin" "$b/node" "$b/program" "$b/lez-config" "$b/crown"
    zstd -q -d -c "$from" | tar -x -C "$f"
    ( cd "$f" && sha256sum -c --quiet SHA256SUMS ) || { echo "$from: its SHA256SUMS do not hold"; exit 1; }
    cp "$f/program/ferrata_lez.bin" "$f/program/ferrata_bridge.bin" "$b/program/"
    cp "$f/bin/r0vm" "$f/bin/r0vm.version" "$b/bin/"
    cp "$LIVE/bin/sequencer_service" "$b/bin/"
    [ -f "$LIVE/bin/sequencer_service.toolchain" ] && cp "$LIVE/bin/sequencer_service.toolchain" "$b/bin/"
    cp "$CROWN_LEZ_TMP/bin/crown-driver" "$b/bin/"
    cp "$LIVE/bedrock/.resolved-host/logos-blockchain-node" "$b/node/"
    [ -f "$LIVE/bedrock/.resolved-host/metadata.json" ] && cp "$LIVE/bedrock/.resolved-host/metadata.json" "$b/node/"
    mkdir -p "$b/xdg-cache" && cp -r "$LIVE/xdg-cache/logos" "$b/xdg-cache/logos"
    cp "$LEZ/lez/sequencer/service/networks/local.json" "$LEZ/bedrock/node-config.yaml" "$LEZ/bedrock/deployment-settings.yaml" "$b/lez-config/"
    echo "$(basename "$LEZ" | sed 's/^lez-//') $(git -C "$LEZ" rev-parse HEAD 2>/dev/null || echo unknown)" > "$b/lez-config/lez-tag.txt"
    bridge_b1 "$b"
    fixture "$b"
    cp "$HERE/session.sh" "$b/crown/session.sh"
    sums "$b"
    tar -C "$b" --sort=name --owner=0 --group=0 --numeric-owner -cf - . | zstd -q -T0 -9 -o "$CROWN_LEZ_TMP/bundle.tar.zst" -f
    rm -rf "$f"
    echo "bundle: $(stat -c %s "$CROWN_LEZ_TMP/bundle.tar.zst") bytes, $(wc -l < "$b/SHA256SUMS") files, sha256 $(sha256sum "$CROWN_LEZ_TMP/bundle.tar.zst" | cut -c1-64)"
    cat "$b/lez-config/lez-tag.txt"
    grep -E ' \./(bin|program|crown|fixture|node|lez-config)/' "$b/SHA256SUMS"
    ;;
  *) sed -n '2,22p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
