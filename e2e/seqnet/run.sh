#!/usr/bin/env bash
# The user's private round trip on a running LEZ sequencer (e2e/seqnet/README.md), on the build box (spike/modal-build's
# mrun; never on a laptop). Local only: an anvil chain (chain id 11155111, the stand-in light client's configuration is
# Sepolia's; nothing is sent to any public network) and a LEZ v0.4.0-rc1 devnet (sequencer + Logos Blockchain node).
#
#   run.sh build          the tools: ferrata, ferrata-relayer, seqnet-fund, seqnet-txs (cli/, --features seqnet),
#                         crown-driver (lez/crown, with crown-b --operator-only), realblocks-crown (checkpoint/),
#                         ferrata-e2e (bridge/), the vault (vault/lib installed by forge), the zone's bundle repacked
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

case "${1:-}" in
  build) build ;;
  cli|miniapp)
    NAME="${2:?run.sh $1 NAME}"
    export SEQNET_MODE="$1" SEQNET_NAME="$NAME" SEQNET_REPO="$REPO" SEQNET_T="$T"
    exec bash "$HERE/drive.sh"
    ;;
  *) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
