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
python3 -I scripts/check_vectors.py vectors

echo "== zkVM executor: release guest (cycles, journal == native, bad witness fails), claim guest vs native"
cargo build --release -p host
./target/release/host execute
./target/release/host claim-check 16

echo "== saved receipts verify against the image id built now (catches guest drift)"
./target/release/host verify eth
./target/release/host verify erc20

echo "== neighbour drift (warnings only): vendored tree.json and the vault source vs. spike/bridge-evm now"
src="$here/../bridge-evm/test/vectors/tree.json"
if [ -f "$src" ]; then
  cmp -s "$src" vectors/external/bridge-evm-tree.json && echo "tree.json: identical to the vendored copy" \
    || echo "WARNING: bridge-evm tree.json changed since it was vendored; re-copy and re-run"
fi
vault_sha="$(sha256sum "$here/../bridge-evm/src/FerrataVault.sol" | cut -d' ' -f1)"
grep -q "$vault_sha" vectors/external/PROVENANCE.txt && echo "FerrataVault.sol: unchanged (sha256 $vault_sha)" \
  || echo "WARNING: FerrataVault.sol changed since vectors/external/PROVENANCE.txt (now $vault_sha)"

echo "== Foundry: journal equality, real Groth16 verifier, vault end to end"
(cd evm-verify && forge test)
echo "ALL CHECKS PASSED"
