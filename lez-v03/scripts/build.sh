#!/usr/bin/env bash
# Build everything lez-v03 needs, from lez-v03/: LEZ v0.3.1 cloned beside the repository if absent (tag v0.3.1,
# commit checked), the program (guest, through methods-v03; image id independent of the build path through the
# repository's guest-rustc wrapper), the harness and the client; with `--lez`, also LEZ v0.3.1's own
# sequencer_service (standalone, its mock Bedrock) and wallet CLI from LEZ's tree and lockfile, into
# target/lez. Needs RISC Zero's guest Rust 1.91.1 (RISC0_HOME) and r0vm 3.0.5 on PATH.
set -euo pipefail
cd "$(dirname "$0")/.."
SPIKE="$(cd ../.. && pwd)"
U="$SPIKE/upstream/lez-v0.3.1"
if [ ! -f "$U/Cargo.toml" ]; then
  mkdir -p "$SPIKE/upstream"
  git clone -q --depth 1 --branch v0.3.1 https://github.com/logos-blockchain/logos-execution-zone "$U"
fi
test "$(git -C "$U" rev-parse HEAD)" = 1a003bfbfb89fcf987f62975cd08a401169e1b8c
test -z "$(git -C "$U" status --porcelain --untracked-files=no)"
export RISC0_HOME="${RISC0_HOME:-$SPIKE/risc0-home-lez}"
LOCKED="${LOCKED---locked}"
cargo build --release $LOCKED -p methods-v03 -p harness-v03 -p client-v03 --tests --bins
if [ "${1:-}" = --lez ]; then
  T="$(pwd)/target/lez"
  ( cd "$U" && CARGO_TARGET_DIR="$T" cargo build --release --locked -p sequencer_service --features sequencer_service/standalone -p wallet )
  ls -la "$T/release/sequencer_service" "$T/release/wallet"
fi
