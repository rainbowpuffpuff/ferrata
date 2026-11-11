#!/usr/bin/env bash
# The witness-independence check of the bridge guests' cycle counts (SPEC-v2 §5.2; the zkVM step of
# bridge/scripts/check.sh): the release and claim guests are executed in r0vm 3.0.5 on n random valid witnesses
# each (default 64: the extreme leaf indices first, then random indices, siblings, secrets and public fields), and
# the run fails if the (user cycles, segment po2s) of one guest differ between witnesses. Executor only: no proof,
# and dev mode is refused. The CI job `witness-independence` runs exactly this script.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$HOME/.risc0/bin:$HOME/.cargo/bin:$PATH"
unset RISC0_DEV_MODE
cd "$repo/bridge"
t0=$(date +%s)
cargo build --release --locked -p host
t1=$(date +%s)
./target/release/host invariance "${1:-64}"
echo "build $((t1 - t0)) s, invariance $(($(date +%s) - t1)) s"
