#!/usr/bin/env bash
# Ferrata on a running LEZ sequencer (v0.4.0-rc1; LEZ_TAG=v0.4.0-rc2 or v0.4.0-rc3 for those): build, run, record.
#
#   run.sh build            LEZ's sequencer_service (as `just run-sequencer` builds it) and the driver
#   run.sh build-standalone LEZ's sequencer_service with the `standalone` feature (mock Bedrock)
#   run.sh build-bedrock    the Logos Blockchain node LEZ pins, resolved by LEZ's own resolver
#   run.sh run              start Bedrock and the sequencer, drive the whole scenario, stop, write results/
#   run.sh stop             stop whatever a run left behind
#
# Environment (all optional):
#   FERRATA_LIVE_BEDROCK   node (default if build-bedrock was run) | mock
#   FERRATA_LIVE_BLOCK_TIME  the sequencer's block interval; default 10s with a Bedrock node (LEZ's
#                          testnet.json value), LEZ's local.json value (15s) with the mock
#   FERRATA_LIVE_TICK_AFTER, FERRATA_LIVE_TICK_WINDOW  with a Bedrock node: the seconds after the
#                          seconds at which a Bedrock epoch can begin at which the sequencer is
#                          to make its blocks (default 3), and the range accepted (default "1 6");
#                          the zone is started again until it is met (at most
#                          FERRATA_LIVE_MAX_STARTS times, default 10; run-live.sh says why).
#                          FERRATA_LIVE_ALIGN=0 starts the zone once, whatever the phase
#   FERRATA_PROGRAM_BIN, FERRATA_SCRIPTED_BIN   the guest binaries to deploy (see README.md)
#   FERRATA_LIVE_PHASES    space-separated phases of the driver (default: all)
#   FERRATA_LIVE_SEQUENCER_DEV_MODE=1  the sequencer runs with RISC0_DEV_MODE=1, LEZ's Justfile's development mode
#                          (it takes fake receipts); phase attest-block then proves with fake receipts too
#   FERRATA_LIVE_PROOF_SECONDS  cap on the real proof of the private update (default 3600; 0 skips it)
#   FERRATA_LIVE_PROOF_THREADS  the prover's thread count (RAYON_NUM_THREADS), for a machine that
#                          reports fewer CPUs than it may use; FERRATA_LIVE_PROOF_NICE its niceness
#                          (default 10: below the node and the sequencer)
#   FERRATA_LIVE_OUT       where the records go (default results/ here)
#   FERRATA_LIVE_DRIVER_EXTRA  more options for every driver phase but private-real (for example
#                          --consumer-bin FILE, which phase attest-private needs)
#   FERRATA_LIVE_HOOK      the script phase `hook` runs while the zone runs, with the zone's RPC,
#                          node API, run directory, logs and binaries in ZONE_* variables
#                          (hooks/crown.sh: the bridge's private claim and burn by crown-driver)
#   For the observations in observations/: FERRATA_LIVE_PHASE_SECONDS (a cap on each phase) and
#   FERRATA_LIVE_BEDROCK_TRACE=1 (the node's block proposer logs at trace level)
#   FERRATA_LIVE_JOBS, FERRATA_LIVE_NICE   build parallelism (default 4 jobs under nice 19)
#   FERRATA_LIVE_CARGO_MESSAGES  a file: the builds append cargo's JSON messages to it
#                          (--message-format=json-render-diagnostics), for scripts/fetch_native.py check-build
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

# cargo_in DIR ARGS...: cargo in DIR with this script's target directory and niceness; with FERRATA_LIVE_CARGO_MESSAGES,
# cargo's JSON messages (its stdout) are appended to that file, the rendered diagnostics staying on stderr.
cargo_in() {
  local dir="$1"; shift
  if [ -n "${FERRATA_LIVE_CARGO_MESSAGES:-}" ]; then
    ( cd "$dir" && CARGO_TARGET_DIR="$TARGET" $NICE cargo "$@" --message-format=json-render-diagnostics ) >> "$FERRATA_LIVE_CARGO_MESSAGES"
  else
    ( cd "$dir" && CARGO_TARGET_DIR="$TARGET" $NICE cargo "$@" )
  fi
}

build_sequencer() { # $1: output name, rest: extra cargo arguments
  local out="$1"; shift
  check_lez
  echo "== building LEZ sequencer_service ($out) with $CARGO_BUILD_JOBS jobs"
  cargo_in "$LEZ_SRC" build --release --locked -p sequencer_service "$@"
  cp "$TARGET/release/sequencer_service" "$BIN/$out"
  ( cd "$LEZ_SRC" && rustc --version && cargo --version ) > "$BIN/$out.toolchain"
  sha256sum "$BIN/$out" | cut -d' ' -f1 > "$BIN/$out.sha256"
  echo "== $BIN/$out ($(cat "$BIN/$out.sha256"))"
}

build_driver() {
  check_lez
  check_driver_tag
  echo "== building the driver with $CARGO_BUILD_JOBS jobs"
  # The driver links LEZ's wallet: start its lockfile from LEZ's, so that the wallet is built
  # with the dependency versions LEZ pins (cargo keeps what applies and adds the rest).
  [ -f "$LIVE_DIR/driver/Cargo.lock" ] || cp "$LEZ_SRC/Cargo.lock" "$LIVE_DIR/driver/Cargo.lock"
  cargo_in "$LIVE_DIR/driver" build --release
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
  *) awk 'NR > 1 && /^set -euo/ { exit } NR > 1' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
