#!/usr/bin/env bash
# The ferrata CLI's end-to-end test: anvil (chain id 1, local only) for the vault, the LEZ harness for the
# zone (RISC0_DEV_MODE=1). Needs Foundry (anvil, forge) and the vault's libraries (installed if missing).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
# LEZ's guests (built for the harness) need RISC Zero's Rust 1.91.1, kept in its own RISC0_HOME beside
# the repository (lez/RESULTS.md §8); an RISC0_HOME already set wins.
if [ -z "${RISC0_HOME:-}" ] && [ -d "$here/../../risc0-home-lez" ]; then export RISC0_HOME="$(cd "$here/../../risc0-home-lez" && pwd)"; fi
cd "$here/../vault"
[ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0
python3 -I "$here/../scripts/vault_libs.py" check
forge build --quiet
cd "$here"
# The native archives this build links (rapidsnark, the Logos Blockchain circuits), fetched once and checked against
# scripts/native-deps.json; the build's messages checked: nothing native linked from outside the checked archives.
vars=$(python3 -I "$here/../scripts/fetch_native.py" env) && eval "$vars"
mkdir -p "$here/target"
FERRATA_E2E=1 RISC0_DEV_MODE=1 cargo test --locked --test e2e --no-run --message-format=json-render-diagnostics \
  > "$here/target/e2e-build.json"
python3 -I "$here/../scripts/fetch_native.py" check-build "$here/target/e2e-build.json" --target-dir "$here/target"
FERRATA_E2E=1 RISC0_DEV_MODE=1 cargo test --locked --test e2e -- --nocapture
