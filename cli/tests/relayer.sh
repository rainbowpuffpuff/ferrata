#!/usr/bin/env bash
# The relayer's tests (cli/src/relayer.rs, cli/src/bin/relayer.rs):
#  - the library's unit tests (journals, the burn watch, temporary failures, imported roots);
#  - tests/gasdrop.rs: the gas-drop checks on a real receipt (bridge/artifacts/gasdrop/), no dev mode;
#  - tests/relayer_fork.rs: the ferrata-relayer daemon on an anvil fork of Sepolia at the block before
#    crown-live-1's submitCandidate, with that run's real checkpoint and release proofs; a misbehaving
#    relayer refused. It reads public Sepolia state (FERRATA_FORK_URL, or the endpoints in the test) and
#    sends nothing to Sepolia. Results: e2e/runs/relayer-fork/results.json.
# Needs Foundry's anvil, and LEZ's toolchain as for tests/e2e.sh (the crate links LEZ's wallet).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
if [ -z "${RISC0_HOME:-}" ] && [ -d "$here/../../risc0-home-lez" ]; then export RISC0_HOME="$(cd "$here/../../risc0-home-lez" && pwd)"; fi
cd "$here"
unset RISC0_DEV_MODE
cargo test --lib relayer
cargo test --test gasdrop -- --nocapture
mkdir -p "$here/../e2e/runs/relayer-fork"
FERRATA_RELAYER_FORK=1 FERRATA_RELAYER_RESULTS="${FERRATA_RELAYER_RESULTS:-$here/../e2e/runs/relayer-fork/results.json}" \
  cargo test --test relayer_fork -- --nocapture
# The devnet harness with the relayer (cli/tests/e2e.rs with FERRATA_GASDROP_ELF): a real gas-drop receipt
# for a deposit of the local vault, checked against the zone; the claim naming the relayer; a paid release.
cd "$here/../vault"
[ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0
forge build --quiet
# (find fails when bridge/target does not exist yet: a fresh checkout builds it below.)
elf="$(find "$here/../bridge/target" -path '*riscv32im-risc0-zkvm-elf/release/gasdrop.bin' 2>/dev/null | head -1 || true)"
if [ -z "$elf" ]; then
  (cd "$here/../bridge" && RISC0_HOME= cargo build --release -p host)
  elf="$(find "$here/../bridge/target" -path '*riscv32im-risc0-zkvm-elf/release/gasdrop.bin' | head -1)"
fi
echo "gas-drop guest: $elf"
cd "$here"
FERRATA_E2E=1 RISC0_DEV_MODE=1 FERRATA_GASDROP_ELF="$elf" cargo test --test e2e -- --nocapture
