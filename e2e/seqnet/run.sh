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
#   run.sh build-lezcore  also: lez_core (logos-execution-zone-module, LEZ v0.4.0-rc1's wallet as a Logos module) from
#                         its source at the merged v0.4 state, as merged and with modules/patches/lez_core-*.patch, and
#                         both packages' unit tests (Nix, its own pins: logos-module-builder 5f74cdb, LEZ 1806e87)
#   run.sh lezcore NAME   ferrata_bridge_module in a headless Logos Core beside lez_core: the module's LEZ reads, the
#                         private holding and the private claim and burn through lez_core, each beside the plain path
#                         (seqnet-chain) (hook-lezcore.sh)
#   run.sh deployer NAME  a second deployer's own configuration (docs/guides/deployer-guide.md §11): the vault by
#                         vault/script/deploy.sh with its FERRATA_* settings and RISC Zero's own Groth16 verifier, the LEZ
#                         configuration with its roles and caps (CROWN_ADMIN and CROWN_FREEZE_AUTHORITY, by default
#                         two fresh random keys; CROWN_TOKEN_CAP, CROWN_GLOBAL_CAP),
#                         its token, caps, fee schedule and admin handover, every value read back (hook-deployer.sh)
#   run.sh basecamp NAME  the round trip in the mini-app inside Logos Basecamp, ferrata_bridge_module and lez_core
#                         loaded in the same Basecamp (hook-basecamp.sh; first modules/ferrata_bridge_ui/scripts/
#                         basecamp-build.sh, BASECAMP_BUILD=its OUT)
#
# Inputs (beside the repository, spike/tmp/miniapp-sequencer/inputs/, put on the box with mrun --put; or built from
# source and named by SEQNET_BUNDLE and SEQNET_R0BF, as docs/guides/deployer-guide.md §11 step 0 does): the crown-live-3
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
# The zone's bundle and the R0BF programs: the live run's (in $IN), or the ones step 0 of the deployer guide's §11
# builds from source (SEQNET_BUNDLE: lez/crown/build.sh's bundle.tar.zst; SEQNET_R0BF: reproduce-image-ids.sh's r0bf/).
BUNDLE="${SEQNET_BUNDLE:-$IN/bundle-crown-live-3.tar.zst}"
TGT="$T/target-lez"                                  # one target directory for cli/ and lez/crown/driver (LEZ's crates)
mkdir -p "$T/logs"                                   # a fresh clone's first run has none (found by a second deployer)
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
  say "the zone's bundle: $(basename "$BUNDLE"), with this tree's crown-driver and session.sh (lez/crown/build.sh repack)"
  mkdir -p "$T/crown-lez/bin"; cp "$T/bin/crown-driver" "$T/crown-lez/bin/crown-driver"
  CROWN_LEZ_TMP="$T/crown-lez" bash "$REPO/lez/crown/build.sh" repack "$BUNDLE" > "$L/repack.log" 2>&1 \
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

# lez_core: main of logos-blockchain/logos-execution-zone-module after its v0.4 pull requests (#72 "Sequencer FFI
# adaptation" and #74 "sequencer query API") were merged on 13 Nov 2026 (13:18 and 13:20 UTC).
LEZ_CORE_REPO=logos-blockchain/logos-execution-zone-module
LEZ_CORE_REV=cc73ebfe0857705cfc1c3b29743fb6c8c6aebd82
LEZ_CORE_PATCH="$REPO/modules/patches/lez_core-cc73ebf-authorizes-public-pda.patch"

# Saves the box's /nix to the volume (the setup script restores it in the next container).
save_nix() {
  say "caching /nix for the next container (/vol/nix/lez-core-v04.tar.zst)"
  mkdir -p /vol/nix && tar -C / -cf - nix | zstd -q -T0 -3 -o /vol/nix/lez-core-v04.tar.zst.part -f \
    && mv /vol/nix/lez-core-v04.tar.zst.part /vol/nix/lez-core-v04.tar.zst && ls -la /vol/nix/lez-core-v04.tar.zst
}

# build-lezcore [STAGE]: core (lez_core as merged: LEZ's core FFI, the long Nix build), rest (the patched lez_core, both
# unit-test builds, the module and logosctl through check.sh), or both. A step whose output exists is not run again.
build_lezcore() {
  local stage="${1:-all}" L="$T/logs/build" P="$T/lez_core-patched" O="$T/modcheck" failed=""; mkdir -p "$L" "$T/logos" "$T/bin"
  # Nix's evaluation and local builds need a finite stack of 1 GiB on the box (modules/scripts/check.sh).
  if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
  local NB=(nix build --cores "${NIX_CORES:-30}" --max-jobs 2 -L)
  # Each step is tried (one failing does not stop the others); the failures are listed at the end.
  try() { local name="$1" t0=$(date +%s); shift; if "$@" > "$L/$name.log" 2>&1; then say "$name: ok ($(( $(date +%s) - t0 )) s)";
          else say "$name: FAILED ($(( $(date +%s) - t0 )) s)"; grep -a -E -A5 '^error|error:|FAILED' "$L/$name.log" | tail -n 25; failed="$failed $name"; fi; }
  [ -x "$T/bin/seqnet-chain" ] && [ "$T/bin/seqnet-chain" -nt "$REPO/cli/src/bin/seqnet_chain.rs" ] \
    || try seqnet-chain bash -c "cd '$REPO/cli' && CARGO_TARGET_DIR='$TGT' cargo build --release --features seqnet --bin seqnet-chain && cp '$TGT/release/seqnet-chain' '$T/bin/'"
  if [ "$stage" != rest ] && [ ! -e "$T/logos/lgx-lez_core-upstream" ]; then
    try lez_core-upstream "${NB[@]}" "github:$LEZ_CORE_REPO/$LEZ_CORE_REV#lgx" -o "$T/logos/lgx-lez_core-upstream"
    [ -e "$T/logos/lgx-lez_core-upstream" ] && save_nix
  fi
  if [ "$stage" != core ]; then
    [ -e "$T/logos/lgx-lez_core-patched" ] || {
      try lez_core-source bash -c "rm -rf '$P' && git clone -q 'https://github.com/$LEZ_CORE_REPO' '$P' && git -C '$P' checkout -q '$LEZ_CORE_REV' && git -C '$P' apply '$LEZ_CORE_PATCH' && git -C '$P' diff --stat"
      try lez_core-patched "${NB[@]}" "path:$P#lgx" -o "$T/logos/lgx-lez_core-patched"; }
    [ -e "$T/logos/lez_core-tests-upstream" ] || try lez_core-tests-upstream "${NB[@]}" "github:$LEZ_CORE_REPO/$LEZ_CORE_REV#unit-tests" -o "$T/logos/lez_core-tests-upstream"
    [ -e "$T/logos/lez_core-tests-patched" ] || try lez_core-tests-patched "${NB[@]}" "path:$P#unit-tests" -o "$T/logos/lez_core-tests-patched"
    # ferrata_bridge_module (native tests and calls, then the dev .lgx) and logosctl, through modules/scripts/check.sh.
    try check env MODULES=ferrata_bridge_module PORTABLE=0 E2E=0 LOGOS_TMPDIR="$(dirname "$T")/mseq" RISC0_DEV_MODE=1 \
      bash "$REPO/modules/scripts/check.sh" "$O"
    tail -n 12 "$O/report/summary.md" 2>/dev/null
    if [ -d "$O/lgx-ferrata_bridge_module" ]; then
      rm -rf "$T/logos/lgx-ferrata_bridge_module"; cp -rL "$O/lgx-ferrata_bridge_module" "$T/logos/lgx-ferrata_bridge_module"
      ln -sfn "$(readlink -f "$O/logos/bin/logosctl")" "$T/logos/logosctl"
    fi
    ( cd "$T/logos" && sha256sum lgx-*/*.lgx; ./logosctl --version 2>&1 | head -1 ) | tee "$T/logos/VERSIONS"
    save_nix
  fi
  [ -z "$failed" ] || die "build-lezcore: failed:$failed (logs in $L)"
}

case "${1:-}" in
  build) build ;;
  build-miniapp) build_miniapp ;;
  build-lezcore) build_lezcore "${2:-all}" ;;
  cli|miniapp|lezcore|basecamp|deployer)
    NAME="${2:?run.sh $1 NAME}"
    # The tools are rebuilt first (incremental), so that a run uses this tree's (a restored box may hold older ones).
    build > "$T/logs/build-before-$NAME.log" 2>&1 || { tail -n 20 "$T/logs/build-before-$NAME.log"; die "build"; }
    if [ "$1" != cli ] && [ "$1" != deployer ]; then
      ( cd "$REPO/cli" && CARGO_TARGET_DIR="$TGT" cargo build --release --features seqnet --bin seqnet-chain ) >> "$T/logs/build-before-$NAME.log" 2>&1 \
        && cp "$TGT/release/seqnet-chain" "$T/bin/" || die "seqnet-chain build"
    fi
    export SEQNET_MODE="$1" SEQNET_NAME="$NAME" SEQNET_REPO="$REPO" SEQNET_T="$T"
    exec bash "$HERE/drive.sh"
    ;;
  *) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
