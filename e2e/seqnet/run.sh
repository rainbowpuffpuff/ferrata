#!/usr/bin/env bash
# The user's private round trip on a running LEZ sequencer (e2e/seqnet/README.md), on the build box (spike/modal-build's
# mrun; never on a laptop). Local only: an anvil chain (chain id 11155111, the stand-in light client's configuration is
# Sepolia's; nothing is sent to any public network) and a LEZ v0.4.0-rc1 devnet (sequencer + Logos Blockchain node).
#
#   run.sh build          the tools: ferrata, ferrata-relayer, seqnet-fund, seqnet-txs (cli/, --features seqnet),
#                         crown-driver (lez/crown, with crown-b --operator-only), realblocks-crown (checkpoint/),
#                         ferrata-e2e (bridge/), the vault (vault/lib installed by forge), the zone's bundle repacked
#   run.sh build-miniapp  also: seqnet-chain (cli/), the qmltest runner (modules/ferrata_bridge_ui/qmltest-logos, Qt 6),
#                         ferrata_bridge_module's dev .lgx and logosctl (modules/scripts/check.sh: Nix, logos-module-builder
#                         and logos-logoscore-cli at their pins; the module's native tests and calls first); /nix cached
#   run.sh cli NAME       the round trip through the `ferrata` tool (hook-cli.sh); records in e2e/runs/seqnet-NAME/
#   run.sh miniapp NAME   the same through the mini-app driving ferrata_bridge_module in a headless Logos Core
#                         (hook-miniapp.sh)
#
# Inputs (beside the repository, spike/tmp/miniapp-sequencer/inputs/, put on the box with mrun --put): the crown-live-3
# LEZ bundle (bundle-crown-live-3.tar.zst, sha256 0013bab1...: sequencer_service, the node, r0vm, the final programs
# ferrata_lez.bin 0x816d1fef... and ferrata_bridge.bin 0xccde6171...) and the final R0BF programs (r0bf/checkpoint_b.bin
# 0xf8116722..., r0bf/release.bin 0xb64900bc...), docs/image-ids.md.
#
# Environment: SEQNET_PROOFS=dev (default: the sequencer accepts RISC Zero dev-mode receipts and LEZ's wallet makes
# fake ones) or real (the sequencer verifies, LEZ's wallet proves every private transaction on this machine's CPU).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
T="${SEQNET_TMP:-$BESIDE/tmp/miniapp-sequencer}"
IN="$T/inputs"
TGT="$T/target-lez"                                  # one target directory for cli/ and lez/crown/driver (LEZ's crates)
export CARGO_TERM_COLOR=never PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
say() { echo "[$(date -u +%H:%M:%S)] $*"; }
die() { say "STOP: $*"; exit 1; }

build() {
  local L="$T/logs/build"; mkdir -p "$L"
  say "vault: libraries and build"
  ( cd "$REPO/vault" && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 \
      OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0; } && forge build ) > "$L/vault.log" 2>&1 \
    || { tail -n 20 "$L/vault.log"; die "vault build"; }
  say "cli: ferrata, ferrata-relayer, seqnet-fund, seqnet-txs"
  ( cd "$REPO/cli" && CARGO_TARGET_DIR="$TGT" cargo build --release --features seqnet --bin ferrata --bin ferrata-relayer \
      --bin seqnet-fund --bin seqnet-txs ) > "$L/cli.log" 2>&1 || { grep -E -A7 '^error' "$L/cli.log" | head -60; die "cli build"; }
  say "crown-driver"
  ( cd "$REPO/lez/crown/driver" && { [ -f Cargo.lock ] || cp "$REPO/lez/live/driver/Cargo.lock" Cargo.lock; } \
      && CARGO_TARGET_DIR="$TGT" cargo build --release ) > "$L/driver.log" 2>&1 \
    || { grep -E -A7 '^error' "$L/driver.log" | head -60; die "crown-driver build"; }
  say "realblocks-crown, ferrata-e2e"
  ( cd "$REPO/checkpoint" && cargo build --release -p realblocks --bin realblocks-crown ) > "$L/realblocks.log" 2>&1 \
    || { grep -E -A7 '^error' "$L/realblocks.log" | head -40; die "realblocks-crown build"; }
  ( cd "$REPO/bridge" && cargo build --release -p ferrata-e2e ) > "$L/e2e.log" 2>&1 \
    || { grep -E -A7 '^error' "$L/e2e.log" | head -40; die "ferrata-e2e build"; }
  mkdir -p "$T/bin"
  cp "$TGT/release/ferrata" "$TGT/release/ferrata-relayer" "$TGT/release/seqnet-fund" "$TGT/release/seqnet-txs" \
     "$TGT/release/crown-driver" "$REPO/checkpoint/target/release/realblocks-crown" "$REPO/bridge/target/release/ferrata-e2e" "$T/bin/"
  say "the zone's bundle: crown-live-3's, with this tree's crown-driver and session.sh (lez/crown/build.sh repack)"
  mkdir -p "$T/crown-lez/bin"; cp "$T/bin/crown-driver" "$T/crown-lez/bin/crown-driver"
  CROWN_LEZ_TMP="$T/crown-lez" bash "$REPO/lez/crown/build.sh" repack "$IN/bundle-crown-live-3.tar.zst" > "$L/repack.log" 2>&1 \
    || { tail -n 20 "$L/repack.log"; die "repack"; }
  tail -n 8 "$L/repack.log"
  ( cd "$T/bin" && sha256sum * ) | tee "$T/bin/SHA256SUMS"
  "$T/bin/ferrata-e2e" 2>&1 | head -3 || true
}

build_miniapp() {
  local L="$T/logs/build"; mkdir -p "$L"
  say "cli: seqnet-chain"
  ( cd "$REPO/cli" && CARGO_TARGET_DIR="$TGT" cargo build --release --features seqnet --bin seqnet-chain ) > "$L/chain.log" 2>&1 \
    || { grep -E -A7 '^error' "$L/chain.log" | head -60; die "seqnet-chain build"; }
  cp "$TGT/release/seqnet-chain" "$T/bin/"
  say "the qmltest runner (Qt 6)"
  ( cmake -S "$REPO/modules/ferrata_bridge_ui/qmltest-logos" -B "$T/qmltest-build" -DCMAKE_BUILD_TYPE=Release \
      && cmake --build "$T/qmltest-build" -j 8 ) > "$L/runner.log" 2>&1 || { tail -n 30 "$L/runner.log"; die "the qmltest runner"; }
  cp "$T/qmltest-build/ferrata_miniapp_logos" "$T/bin/"
  say "ferrata_bridge_module (native tests and calls, then the dev .lgx) and logosctl, through modules/scripts/check.sh"
  local O="$T/modcheck"
  MODULES=ferrata_bridge_module PORTABLE=0 E2E=0 LOGOS_TMPDIR="$(dirname "$T")/mseq" RISC0_DEV_MODE=1 \
    bash "$REPO/modules/scripts/check.sh" "$O" > "$L/check.log" 2>&1 || { tail -n 40 "$L/check.log"; die "check.sh"; }
  tail -n 12 "$O/report/summary.md" 2>/dev/null
  mkdir -p "$T/logos"; rm -rf "$T/logos/lgx-ferrata_bridge_module"
  cp -rL "$O/lgx-ferrata_bridge_module" "$T/logos/lgx-ferrata_bridge_module"
  ln -sfn "$(readlink -f "$O/logos/bin/logosctl")" "$T/logos/logosctl"
  ( cd "$T/logos" && sha256sum lgx-ferrata_bridge_module/*.lgx; ./logosctl --version 2>&1 | head -1 ) | tee "$T/logos/VERSIONS"
  say "caching /nix for the next container ($T/nix-store.tar.zst; the setup script restores it)"
  tar -C / -cf - nix | zstd -q -T0 -3 -o "$T/nix-store.tar.zst.part" -f && mv "$T/nix-store.tar.zst.part" "$T/nix-store.tar.zst"
  ls -la "$T/nix-store.tar.zst"
}

case "${1:-}" in
  build) build ;;
  build-miniapp) build_miniapp ;;
  cli|miniapp)
    NAME="${2:?run.sh $1 NAME}"
    # The tools are rebuilt first (incremental), so that a run uses this tree's (a restored box may hold older ones).
    build > "$T/logs/build-before-$NAME.log" 2>&1 || { tail -n 20 "$T/logs/build-before-$NAME.log"; die "build"; }
    if [ "$1" = miniapp ]; then
      ( cd "$REPO/cli" && CARGO_TARGET_DIR="$TGT" cargo build --release --features seqnet --bin seqnet-chain ) >> "$T/logs/build-before-$NAME.log" 2>&1 \
        && cp "$TGT/release/seqnet-chain" "$T/bin/" || die "seqnet-chain build"
    fi
    export SEQNET_MODE="$1" SEQNET_NAME="$NAME" SEQNET_REPO="$REPO" SEQNET_T="$T"
    exec bash "$HERE/drive.sh"
    ;;
  *) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
