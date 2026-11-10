#!/usr/bin/env bash
# Every non-proving check, in one place. Needs: Rust 1.93 (rustup default) with the
# riscv32im-unknown-none-elf target, the RISC Zero toolchain (`+risc0`, Rust 1.88), r0vm 3.0.5,
# Foundry, python3. No network once dependencies are fetched. Usage: scripts/check.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export TMPDIR="$here/../tmp"            # spike/tmp: never /tmp on this machine
export CARGO_TARGET_DIR="$here/target"
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1
unset RISC0_DEV_MODE
mkdir -p "$TMPDIR"

echo "== rustfmt"
cargo fmt --all --check
~/.risc0/toolchains/v1.88.0-rust-x86_64-unknown-linux-gnu/bin/rustfmt --edition 2021 --check guests/src/bin/*.rs

echo "== clippy (workspace; RISC0_SKIP_BUILD because clippy's wrapper leaks into risc0-build's guest build)"
RISC0_SKIP_BUILD=1 cargo clippy --workspace --all-targets -- -D warnings

echo "== clippy (guest crate, risc0 toolchain, with the cfg flags risc0-build passes)"
(cd guests && CARGO_TARGET_DIR="$here/target/guest-clippy" \
  RUSTFLAGS='-C passes=lower-atomic --cfg getrandom_backend="custom"' \
  cargo +risc0 clippy --target riscv32im-risc0-zkvm-elf -- -D warnings)

echo "== no_std build and clippy for riscv32im-unknown-none-elf"
cargo build -p ferrata-bridge-core --no-default-features --target riscv32im-unknown-none-elf
cargo clippy -p ferrata-bridge-core --no-default-features --target riscv32im-unknown-none-elf -- -D warnings

echo "== core tests"
cargo test -p ferrata-bridge-core

echo "== vectors, recomputed in Python (standard library only)"
python3 -I -B scripts/check_vectors_v2.py vectors
python3 -I -B scripts/check_vectors_v1.py vectors/v1   # superseded v1 vectors, kept checkable

echo "== zkVM executor: cycles, journal == native, bad witnesses fail; cycle count independent of the witness"
cargo build --release -p host
./target/release/host execute
./target/release/host invariance 64
./target/release/host claim-check 16

echo "== saved receipts verify against the image id built now (catches guest drift)"
for c in eth erc20 ethpull; do ./target/release/host verify $c; done

echo "== neighbour drift (warnings only): vendored copies vs. spike/bridge-evm now"
src="$here/../bridge-evm/test/vectors/tree-v2.json"
if [ -f "$src" ]; then
  cmp -s "$src" vectors/external/bridge-evm-tree-v2.json && echo "tree-v2.json: identical to the vendored copy" \
    || echo "WARNING: bridge-evm tree-v2.json changed since it was vendored; re-copy and re-run"
fi
if git -C "$here/../bridge-evm" rev-parse -q --verify v2 > /dev/null 2>&1; then
  git -C "$here/../bridge-evm" show v2:src/FerrataVault.sol | cmp -s - evm-verify/vendor/vault-v2/FerrataVault.sol \
    && echo "vault: evm-verify/vendor/vault-v2 == bridge-evm branch v2 ($(git -C "$here/../bridge-evm" rev-parse --short v2))" \
    || echo "WARNING: bridge-evm branch v2 changed since evm-verify/vendor/vault-v2 was exported"
fi

echo "== Foundry: journal equality, real Groth16 verifier behind the emergency stop, vault v2 end to end"
(cd evm-verify && forge test)
echo "ALL CHECKS PASSED"
