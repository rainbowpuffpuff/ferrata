#!/usr/bin/env bash
# Ferrata on a running LEZ v0.4.0-rc1 sequencer: build, run, record.
#
#   run.sh build            LEZ's sequencer_service (as `just run-sequencer` builds it) and the driver
#   run.sh build-standalone LEZ's sequencer_service with the `standalone` feature (mock Bedrock)
#   run.sh build-bedrock    the Logos Blockchain node LEZ pins, resolved by LEZ's own resolver
#   run.sh run              start Bedrock and the sequencer, drive the whole scenario, stop, write results/
#   run.sh stop             stop whatever a run left behind
#
# Environment (all optional):
#   FERRATA_LIVE_BEDROCK   node (default if build-bedrock was run) | mock
#   FERRATA_LIVE_BLOCK_TIME  the sequencer's block interval; default LEZ's local.json value (15s)
#   FERRATA_PROGRAM_BIN, FERRATA_SCRIPTED_BIN   the guest binaries to deploy (see README.md)
#   FERRATA_LIVE_PHASES    space-separated phases of the driver (default: all)
#   FERRATA_LIVE_PROOF_SECONDS  cap on the real proof of the private update (default 3600; 0 skips it)
#   FERRATA_LIVE_JOBS, FERRATA_LIVE_NICE   build parallelism (default 4 jobs under nice 19)
#   FERRATA_LIVE_RPC_PORT, FERRATA_LIVE_BEDROCK_PORT  (defaults 3040, 18080: LEZ's)
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/env.sh"

RPC_PORT="${FERRATA_LIVE_RPC_PORT:-3040}"
BEDROCK_PORT="${FERRATA_LIVE_BEDROCK_PORT:-18080}"
# Funded key of LEZ's local Bedrock node: `funding_pk` in bedrock/node-config.yaml, the Justfile's
# BEDROCK_FUNDING_PUBLIC_KEY.
BEDROCK_FUNDING_PUBLIC_KEY=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26
NODE_DIR="$FERRATA_LIVE_TMP/bedrock"
NODE_BIN="$NODE_DIR/.resolved-host/logos-blockchain-node"

build_sequencer() { # $1: output name, rest: extra cargo arguments
  local out="$1"; shift
  check_lez
  echo "== building LEZ sequencer_service ($out) with $CARGO_BUILD_JOBS jobs"
  ( cd "$LEZ_SRC" && CARGO_TARGET_DIR="$TARGET" $NICE cargo build --release --locked -p sequencer_service "$@" )
  cp "$TARGET/release/sequencer_service" "$BIN/$out"
  ( cd "$LEZ_SRC" && rustc --version && cargo --version ) > "$BIN/$out.toolchain"
  sha256sum "$BIN/$out" | cut -d' ' -f1 > "$BIN/$out.sha256"
  echo "== $BIN/$out ($(cat "$BIN/$out.sha256"))"
}

build_driver() {
  check_lez
  echo "== building the driver with $CARGO_BUILD_JOBS jobs"
  ( cd "$LIVE_DIR/driver" && CARGO_TARGET_DIR="$TARGET" $NICE cargo build --release )
  cp "$TARGET/release/live-driver" "$BIN/live-driver"
}

build_bedrock() {
  check_lez
  mkdir -p "$NODE_DIR"
  echo "== resolving the Bedrock node with LEZ's resolver (host mode)"
  # LEZ's `just resolve-host-bedrock-node`, with the output and the cargo target directory moved
  # out of the LEZ tree, and the environment LEZ's own Docker and CI wrappers give the resolver
  # (bedrock/tools/resolve_bedrock_node_in_docker.sh). No release of the node matches the
  # revision LEZ pins, so the resolver builds it from cargo's checkout with `--features testing`.
  ( cd "$LEZ_SRC" && RUSTFLAGS="-A single-use-lifetimes" RISC0_DEV_MODE=1 \
      ${FERRATA_LIVE_NODE_TOOLCHAIN:+RUSTUP_TOOLCHAIN=$FERRATA_LIVE_NODE_TOOLCHAIN} \
      $NICE python3 -I bedrock/tools/resolve_bedrock_node.py --target-platform host \
        --output-directory "$NODE_DIR/.resolved-host" \
        --target-directory "$FERRATA_LIVE_TMP/target-bedrock-node" )
  cat "$NODE_DIR/.resolved-host/metadata.json"
}

case "${1:-}" in
  build)
    build_sequencer sequencer_service
    if [ -f "$LIVE_DIR/driver/Cargo.toml" ]; then build_driver; fi
    ;;
  build-sequencer) build_sequencer sequencer_service ;;
  build-standalone) build_sequencer sequencer_service_standalone --features standalone ;;
  build-driver) build_driver ;;
  build-bedrock) build_bedrock ;;
  run) . "$LIVE_DIR/run-live.sh" ;;
  stop) . "$LIVE_DIR/run-live.sh" ;;
  *) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
