# ferrata-bridge

Prototype evidence for the Ferrata proposal (Logos RFP-022, privacy-preserving wrapped ERC-20 and
ETH bridge for LEZ): the Rust side of the bridge's commitment scheme, a RISC Zero "release" proof,
and measurements showing that the Ethereum vault (`spike/bridge-evm/src/FerrataVault.sol`) accepts that
proof on chain. Unaudited prototype. The measured numbers and the list of gaps are in
[RESULTS.md](RESULTS.md).

| path | what |
|---|---|
| `crates/ferrata-bridge-core/` | `no_std` (+ `std`) crate, no `unsafe`: deposit and burn trees, `s`, `C`, `fid`, `N`, `r_i`, `s_out`, memo, `C_out`, `N_out`, the claim and release statements, the vault's release journal, the two wallet recovery scans. All byte layouts in `src/encoding.rs`. |
| `vectors/*.json` | Known-answer vectors written by `tests/vectors.rs`; `scripts/check_vectors.py` recomputes all 245 values with Python's standard library. `vectors/external/` is the vault's own `tree.json`. |
| `guests/` | RISC Zero zkVM 3.0.5 guests (own workspace, RISC Zero's accelerated `sha2` patched in): `release` (the release proof) and `claim` (test vehicle). |
| `methods/`, `host/` | risc0-build embedding; the host CLI that executes, proves, wraps to Groth16, verifies and writes fixtures. |
| `artifacts/` | The receipts (succinct and Groth16), seals, journals, `scenario.json` and every run log (machine, load, GNU time). |
| `evm-verify/` | Foundry: journal equality against the vault's own code, the real `RiscZeroGroth16Verifier` (risc0-ethereum v3.0.0), the vault end to end with the real proof. |
| `scripts/` | `check.sh` (every non-proving check), `prove-bench.sh` (measured proving steps), `anvil-e2e.sh` (real transactions on a local anvil), `check_vectors.py`, `mutants.py` (hand-written mutation pass over the core). |

## Encodings

Integers are big-endian (`u64` 8 bytes, `uint256` 32 bytes); addresses are 20 bytes (32-byte left-padded
words only inside the ABI-encoded journal); `‖` is concatenation without length prefixes. The two
leaf tags and the release domain are SHA-256 of the string; every other tag is prefixed as its ASCII
bytes. Leaves start with `0x00`, nodes with `0x01`, every other preimage with `f`; no tag is a prefix of
another and each kind of preimage has a fixed length, so preimages of different kinds never coincide
(tested in `tests/encoding.rs`).

| value | function | bytes | preimage / message |
|---|---|---|---|
| deposit leaf | SHA-256 | 125 | `0x00 ‖ LEAF_TAG ‖ leafIndex:u64 ‖ token:20 ‖ amount:u256 ‖ C:32`, `LEAF_TAG = sha256("ferrata/vault/leaf/v1")` (= `FerrataVault.leafHash`) |
| burn leaf | SHA-256 | 125 | `0x00 ‖ BURN_LEAF_TAG ‖ leafIndex:u64 ‖ token:20 ‖ amount:u256 ‖ C_out:32`, `BURN_LEAF_TAG = sha256("ferrata/burn/leaf/v1")` |
| node | SHA-256 | 65 | `0x01 ‖ left ‖ right`; empty leaf = 32 zero bytes; depth 32; indices ≥ 2^32 rejected |
| `s` | HMAC-SHA256(k) | 68 | `"ferrata/s/v1" ‖ chainId:u64 ‖ vault:20 ‖ depositor:20 ‖ j:u64` |
| `C` | SHA-256 | 76 | `"ferrata/C/v1" ‖ s ‖ dest:32` |
| `fid` | SHA-256 | 86 | `"ferrata/deposit/v1" ‖ chainId:u64 ‖ vault:20 ‖ leafIndex:u64 ‖ leaf:32` |
| `N` | SHA-256 | 77 | `"ferrata/nf/v1" ‖ s ‖ fid` |
| `r_i` | HMAC-SHA256(k) | 59 | `"ferrata/burn-tag/v1" ‖ lezConfigId:32 ‖ i:u64` |
| `s_out` | HMAC-SHA256(k) | 48 | `"ferrata/s-out/v1" ‖ r` |
| memo | XOR | 20 | `recipient ⊕ HMAC-SHA256(k, "ferrata/memo/v1" ‖ r)[0..20]` (message 47 bytes) |
| `C_out` | SHA-256 | 95 | `"ferrata/Cout/v1" ‖ s_out ‖ chainId:u64 ‖ vault:20 ‖ recipient:20` |
| burn id | SHA-256 | 87 | `"ferrata/burn/v1" ‖ lezConfigId:32 ‖ leafIndex:u64 ‖ leaf:32` |
| `N_out` | SHA-256 | 81 | `"ferrata/nf-out/v1" ‖ s_out ‖ burnId` |
| release journal | (vault hashes it) | 352 | `abi.encode(RELEASE_DOMAIN, chainId, vault, lezConfigId, burnRoot, N_out, token, amount, recipient, feeRecipient, relayFee)`: eleven 32-byte words, `RELEASE_DOMAIN = sha256("ferrata/vault/release/v1")` |
| release guest input | private | 1280 | `burnRoot ‖ leafIndex:u64 ‖ path:32×32 ‖ s_out ‖ chainId:u64 ‖ vault ‖ lezConfigId ‖ token ‖ amount ‖ recipient ‖ feeRecipient ‖ relayFee` |

Statements:

* `verify_claim(root, {token, amount, leafIndex, path, s, dest}, chainId, vault) -> N`: recompute `C`,
  the leaf and its path to `root`; return `N`. One function for the public claim (witness in the clear)
  and the private claim (witness inside a proof). `chainId` and `vault` enter only `N`, so the LEZ
  program must take them from its configuration, never from the claimant.
* `verify_release(burnRoot, {leafIndex, path, s_out}, {chainId, vault, lezConfigId, token, amount,
  recipient, feeRecipient, relayFee}) -> N_out`: recompute `C_out`, the burn leaf and its path. The
  release guest then commits `release_journal(burnRoot, N_out, public)`, the exact bytes
  `FerrataVault._releaseJournalDigest` hashes.
* `recover(k, chainId, vault, deposits, dest_for)`: one pass, trial recomputation of `C`.
* `recover_burns(k, lezConfigId, chainId, vault, notes, gapLimit)`: regenerates `r_0, r_1, …` until
  `gapLimit` consecutive unused values, decrypts each memo and authenticates it against `C_out`. It needs
  `chainId` and `vault` (not in the brief's signature) because `C_out` binds them.

## Reproduce

Prerequisites, all local: Rust 1.93 with the `riscv32im-unknown-none-elf` target; RISC Zero's
toolchain via `rzup` (Rust 1.88.0, cpp 2024.1.5, r0vm 3.0.5); Docker (x86-64) for the Groth16 step;
Foundry; Python 3. Never write to `/tmp` on this machine: every script sets `TMPDIR` to `spike/tmp`.

```sh
cd spike/ferrata-bridge
git submodule update --init --recursive          # forge-std v1.10.0, risc0-ethereum v3.0.0
scripts/check.sh                                 # fmt, clippy, no_std build, 36 core tests, Python vectors,
                                                 # zkVM executor checks, receipt re-verification, 19 Foundry tests
cargo run --release -p ferrata-bridge-core --example recover_bench   # 100k-deposit recovery time

cargo build --release -p host
scripts/prove-bench.sh prove eth                 # succinct receipt, GNU time, load average
scripts/prove-bench.sh groth16 eth               # identity_p254 + Docker Groth16, container memory
scripts/prove-bench.sh p254 eth                  # identity_p254 alone
./target/release/host fixtures                   # artifacts -> evm-verify/test/fixtures
(cd evm-verify && forge test -vv)                # gas logs; add --isolate for whole-transaction gas
scripts/anvil-e2e.sh                             # real transactions on a local anvil, receipts' gasUsed
```

Use `FERRATA_ARTIFACTS=$PWD/artifacts/rerun` for timing re-runs, so the committed receipts (from
which the fixtures were made) are not replaced: Groth16 and STARK proofs are randomised, so every run
produces different seal bytes.

## What is real and what is mocked

Real: the release guest and its image id `0x2ca5b06e…4c41`, the succinct and Groth16 receipts made on
this machine with r0vm 3.0.5 and RISC Zero's Docker prover, `RiscZeroGroth16Verifier` built from
risc0-ethereum v3.0.0 `ControlID`, `FerrataVault` compiled from `../bridge-evm/src` unmodified, the
payouts. Mocked: the LEZ burn-root proof (it does not exist yet). The gas tests mark the root accepted
by writing `burnRootAcceptedAt` directly (`vm.store` / `anvil_setStorageAt`); one Foundry test instead
calls the vault's own `acceptBurnRoot` through a test-only verifier that accepts only registered root
journals and forwards release proofs to the real verifier. The burn tree is built by the host, not by a
LEZ program.

Versions: risc0-zkvm / risc0-build / risc0-groth16 3.0.5 (crates.io also has 3.0.6, not used), r0vm
3.0.5, guest Rust 1.88.0, host Rust 1.93.0, `sha2-v0.10.8-risczero.0` (git 244dc3b), Docker image
`risczero/risc0-groth16-prover:v2025-04-03.1` (`sha256:a4f80ce2…`), risc0-ethereum v3.0.0 (32aa0b6f),
forge-std v1.10.0, Foundry 1.3.6-nightly (a4d66e8), solc 0.8.26 (200 runs, cancun, as the vault project).
Vault tested: `FerrataVault.sol` sha256 `50c3e693…`, mtime 2026-11-09 21:51 (the vault project is
not a git repository; see `vectors/external/PROVENANCE.txt`).

Licence: MIT OR Apache-2.0.
