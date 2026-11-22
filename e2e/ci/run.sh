#!/usr/bin/env bash
# CI's round trip (e2e/ci/README.md): builds the tools it needs, then runs cli/tests/roundtrip_b.rs.
# The CI job (.github/workflows/ci.yml, job `roundtrip`) runs exactly this script; so can anyone with the
# toolchains below. Nothing is sent to any network but the dependency fetches of cargo and forge.
#
# Needs: Rust 1.93 (bridge/, checkpoint/'s guests' hosts) and 1.98.1 (LEZ's crates: cli/, checkpoint/fidelity),
# RISC Zero's rzup toolchains (cargo-risczero and r0vm 3.0.5; guest Rust 1.88.0 in ~/.risc0 or $RISC0_HOME for
# bridge/ and checkpoint/; guest Rust 1.91.1 in $RISC0_HOME_LEZ for LEZ's guests), Foundry (anvil, forge),
# the upstream sources beside the repository (scripts/fetch-upstream.sh).
#
#   e2e/ci/run.sh          build what is missing, run the test
#   e2e/ci/run.sh build    build only
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$HOME/.foundry/bin:$HOME/.risc0/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
lez_home="${RISC0_HOME_LEZ:-}"
if [ -z "$lez_home" ]; then
  for d in "$repo/../risc0-home-lez" "$HOME/risc0-home-lez"; do [ -d "$d" ] && { lez_home="$(cd "$d" && pwd)"; break; }; done
fi
[ -n "$lez_home" ] || { echo "no RISC Zero home with guest Rust 1.91.1 for LEZ (set RISC0_HOME_LEZ)" >&2; exit 1; }
export FERRATA_E2E_DIR="${FERRATA_E2E_DIR:-${RUNNER_TEMP:-$repo/../tmp}/ci-roundtrip}"
# No debug info and no incremental artefacts: cli/target is 13 GB with them (measured on the build box), more than
# GitHub's 10 GB of caches per repository; the caches then hold dependencies only.
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}" CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
mkdir -p "$FERRATA_E2E_DIR"
t0=$(date +%s)
stamp() { echo "[$(( $(date +%s) - t0 )) s] $*"; }

stamp "vault: libraries and build"
cd "$repo/vault"
[ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0
python3 -I "$repo/scripts/vault_libs.py" check
forge build --quiet

stamp "bridge: the release guest (risc0-build, guest Rust 1.88.0)"
cd "$repo/bridge"
cargo build --release --locked -p methods
release_elf="$(find target/riscv-guest -path '*riscv32im-risc0-zkvm-elf/release/*' -name 'release.bin' | head -1)"
[ -n "$release_elf" ] || { echo "no release.bin under bridge/target/riscv-guest" >&2; exit 1; }
export FERRATA_CI_RELEASE_ELF="$repo/bridge/$release_elf"

stamp "checkpoint: the rule (B) guest (risc0-build, guest Rust 1.88.0) and ci_zone (Rust 1.98.1)"
cd "$repo/checkpoint"
cargo build --release --locked -p methods
cp_elf="$(find target/riscv-guest -path '*riscv32im-risc0-zkvm-elf/release/*' -name 'checkpoint_b.bin' | head -1)"
[ -n "$cp_elf" ] || { echo "no checkpoint_b.bin under checkpoint/target/riscv-guest" >&2; exit 1; }
export FERRATA_CI_CHECKPOINT_B_ELF="$repo/checkpoint/$cp_elf"
stamp "native archives (rapidsnark, the Logos Blockchain circuits): fetched once, checked against scripts/native-deps.json"
vars=$(python3 -I "$repo/scripts/fetch_native.py" env) && eval "$vars"
cargo +1.98.1 build --release --locked -p fidelity --bin ci_zone --message-format=json-render-diagnostics \
  > "$FERRATA_E2E_DIR/ci_zone-build.json"
python3 -I "$repo/scripts/fetch_native.py" check-build "$FERRATA_E2E_DIR/ci_zone-build.json" --target-dir "$repo/checkpoint/target"
export FERRATA_CI_ZONE="$repo/checkpoint/target/release/ci_zone"

stamp "cli: the round-trip test (LEZ's guests: guest Rust 1.91.1 in $lez_home)"
cd "$repo/cli"
RISC0_HOME="$lez_home" cargo test --locked --test roundtrip_b --no-run --message-format=json-render-diagnostics \
  > "$FERRATA_E2E_DIR/cli-build.json"
python3 -I "$repo/scripts/fetch_native.py" check-build "$FERRATA_E2E_DIR/cli-build.json" --target-dir "$repo/cli/target"
stamp "built"
[ "${1:-}" = build ] && exit 0
for f in "$FERRATA_CI_RELEASE_ELF" "$FERRATA_CI_CHECKPOINT_B_ELF"; do echo "guest $(basename "$f") sha256 $(sha256sum "$f" | cut -c1-64)"; done
FERRATA_E2E=1 RISC0_DEV_MODE=1 RISC0_HOME="$lez_home" cargo test --locked --test roundtrip_b -- --nocapture
stamp "round trip passed"
