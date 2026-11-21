#!/usr/bin/env bash
# The both-halves tests (cli/tests/both_halves.rs: RFP-022 Sec.6, F.8, F.13, Sec.8, R.4, Sec.9): builds what CI's round trip
# builds (e2e/ci/run.sh build: the vault, the release and rule (B) checkpoint guests, ci_zone), then runs them on a local
# anvil (chain id 1) and the LEZ harness. Nothing is sent to any network but the dependency fetches of cargo and forge.
#
#   e2e/both-halves/run.sh                      the three tests (seeds 1,2,3; 40 steps each)
#   FERRATA_BOTH_HALVES_SEEDS=4,5 FERRATA_BOTH_HALVES_STEPS=60 e2e/both-halves/run.sh randomised
#
# Writes e2e/both-halves/results/randomised.md, migration.md and fees.md.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
"$repo/e2e/ci/run.sh" build
export PATH="$HOME/.foundry/bin:$HOME/.risc0/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
lez_home="${RISC0_HOME_LEZ:-}"
if [ -z "$lez_home" ]; then
  for d in "$repo/../risc0-home-lez" "$HOME/risc0-home-lez"; do [ -d "$d" ] && { lez_home="$(cd "$d" && pwd)"; break; }; done
fi
[ -n "$lez_home" ] || { echo "no RISC Zero home with guest Rust 1.91.1 for LEZ (set RISC0_HOME_LEZ)" >&2; exit 1; }
release_elf="$(find "$repo/bridge/target/riscv-guest" -path '*riscv32im-risc0-zkvm-elf/release/*' -name 'release.bin' | head -1)"
cp_elf="$(find "$repo/checkpoint/target/riscv-guest" -path '*riscv32im-risc0-zkvm-elf/release/*' -name 'checkpoint_b.bin' | head -1)"
[ -n "$release_elf" ] && [ -n "$cp_elf" ] || { echo "the guests are not built" >&2; exit 1; }
export FERRATA_CI_RELEASE_ELF="$release_elf" FERRATA_CI_CHECKPOINT_B_ELF="$cp_elf"
export FERRATA_CI_ZONE="$repo/checkpoint/target/release/ci_zone"
export FERRATA_E2E_DIR="${FERRATA_E2E_DIR:-${RUNNER_TEMP:-$repo/../tmp}/both-halves}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}" CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
mkdir -p "$FERRATA_E2E_DIR"
cd "$repo/cli"
FERRATA_E2E=1 RISC0_DEV_MODE=1 RISC0_HOME="$lez_home" cargo test --locked --test both_halves -- --nocapture --test-threads 1 "$@"
