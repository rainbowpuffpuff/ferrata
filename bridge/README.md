# ferrata-bridge

Prototype evidence for the Ferrata proposal (Logos RFP-022, privacy-preserving wrapped ERC-20 and ETH
bridge for LEZ), at **specification v2** (`notes/design/SPEC-v2.md`, which applies the round-1 reviews):
the Rust side of the bridge's commitment scheme, a RISC Zero release proof and a claim measurement
guest, the LEZ checkpoint encodings, and measurements showing that the v2 Ethereum vault
(`spike/bridge-evm/src/FerrataVault.sol`) accepts the release proof on chain behind RISC Zero's emergency
stop. Unaudited prototype. Numbers and gaps: [RESULTS.md](RESULTS.md). The v1 code is at commit
`104d746`; its vectors, receipts and logs are kept under `vectors/v1/` and `artifacts/v1/`.

| path | what |
|---|---|
| `crates/ferrata-bridge-core/` | `no_std` (+ `std`) crate, no `unsafe`. Every byte layout in `src/encoding.rs`; keys (`kAuth`/`kView`) in `src/hash.rs`; trees in `src/tree.rs`; claim statement, nullifier registry and deposit recovery in `src/inbound.rs`; burn notes, release statement and journal, burn-tag registry and burn recovery in `src/outbound.rs`; checkpoint state `S`, `keySetHash`, `burnFrontierHash`, `t(n)` in `src/checkpoint.rs`. |
| `vectors/*-v2.json` | Known-answer vectors written by `tests/vectors.rs`; `scripts/check_vectors_v2.py` recomputes all 529 values with Python's standard library. |
| `guests/` | RISC Zero zkVM 3.0.5 guests (own workspace, RISC Zero's accelerated `sha2` patched in): `release` (§5.2) and `claim` (measurement only: the bridge's part of a private claim). |
| `methods/`, `host/` | risc0-build embedding; the host CLI that executes, checks witness-independence of the cycle count, proves, wraps to Groth16, verifies and writes fixtures. |
| `artifacts/` | v2 receipts, seals, journals, `scenario.json`, and every run log (machine, load, GNU time). |
| `evm-verify/` | Foundry: v2 journal equality, the real `RiscZeroGroth16Verifier` behind `RiscZeroVerifierEmergencyStop` (risc0-ethereum v3.0.0), and the v2 vault end to end. |
| `scripts/` | `check.sh` (every non-proving check), `prove-bench.sh` (measured proving steps), `mutants.py` (hand-written mutation pass), `check_vectors_v2.py`, `check_vectors_v1.py`. |

## Encodings (SPEC-v2)

Integers are big-endian at the stated width; `evmChainId` is a `uint256` (32 bytes) in every preimage;
addresses are 20 bytes (32-byte left-padded words only inside ABI-encoded journals); `‖` is plain
concatenation. Deposit leaves start with `0x00`, nodes with `0x01`, burn leaves with `0x02`; every other
preimage and HMAC message starts with an ASCII tag, and no tag is a prefix of another (tested).

| value | function | bytes | preimage / message |
|---|---|---|---|
| `kAuth`, `kView` | HMAC(bridgeSeed) | 15 | `"ferrata/auth/v1"`, `"ferrata/view/v1"` |
| `configId` | SHA-256 | 261 | `"ferrata/config/v1" ‖ bedrockGenesisId ‖ channelId ‖ bridgeHeader ‖ bridgeImageId ‖ evmChainId ‖ vault ‖ attestationRootId ‖ salt` |
| deposit leaf | SHA-256 | 145 | `0x00 ‖ sha256("ferrata/vault/leaf/v2") ‖ leafIndex:u64 ‖ depositor ‖ token ‖ amount:u256 ‖ C` |
| node | SHA-256 | 65 | `0x01 ‖ left ‖ right`; empty leaf 0; depth 32 |
| `s` | HMAC(kAuth) | 92 | `"ferrata/s/v1" ‖ evmChainId ‖ vault ‖ depositor ‖ j:u64` |
| `C` | SHA-256 | 77 | `"ferrata/C/v1" ‖ s ‖ kind:u8 ‖ dest` (kind 0x01 private, 0x02 public) |
| `fid` | SHA-256 | 148 | `"ferrata/fact/acc-leaf/v1" ‖ evmChainId ‖ vault ‖ slot:u256 (0) ‖ leafIndex:u64 ‖ leaf` |
| `N` | SHA-256 | 77 | `"ferrata/nf/v1" ‖ s ‖ fid` |
| `r_i` | HMAC(kView) | 59 | `"ferrata/burn-tag/v1" ‖ configId ‖ i:u64` |
| `sOut` | HMAC(kAuth) | 80 | `"ferrata/s-out/v1" ‖ configId ‖ r` |
| `COut` | SHA-256 | 119 | `"ferrata/Cout/v1" ‖ sOut ‖ evmChainId ‖ vault ‖ recipient` |
| memo | XOR | 20 | `recipient ⊕ HMAC(kView, "ferrata/memo/v1" ‖ r)[0..20]` |
| `mac` | HMAC(kView)[0..16] | 135 | `"ferrata/burn-mac/v1" ‖ r ‖ token ‖ amount:u256 ‖ COut` |
| burn leaf | SHA-256 | 189 | `0x02 ‖ sha256("ferrata/burn/leaf/v2") ‖ configId ‖ burnIndex:u64 ‖ token ‖ amount:u256 ‖ COut ‖ r` |
| `burnId` | SHA-256 | 87 | `"ferrata/burn/v1" ‖ configId ‖ burnIndex:u64 ‖ burnLeaf` |
| `NOut` | SHA-256 | 81 | `"ferrata/nf-out/v1" ‖ sOut ‖ burnId` |
| `keySetHash` | SHA-256 | 21 + 32n | `"ferrata/keyset/v1" ‖ n:u32 ‖ key_1 ‖ … ‖ key_n` |
| `burnFrontierHash` | SHA-256 | 1048 | `"ferrata/burn-frontier/v1" ‖ f_0 ‖ … ‖ f_31`, canonical frontier (`f_h` = 0 where bit h of the count is 0). **Not defined in SPEC-v2; this crate's choice.** |
| `S` | SHA-256 | 173 | `"ferrata/lez-state/v1" ‖ tipId:u64 ‖ tipHash ‖ keySetHash ‖ configTipHash ‖ burnCount:u64 ‖ burnFrontierHash ‖ importedLeafCount:u64 ‖ sunset:u8` |
| checkpoint journal | (vault hashes it) | 416 | `abi.encode(sha256("ferrata/vault/checkpoint/v2"), evmChainId, vault, lezConfigId, nMin, tMin, parentState, newState, burnRoot, burnCount, importedLeafCount, sunset, echoNonce)` |
| release journal | (vault hashes it) | 352 | `abi.encode(sha256("ferrata/vault/release/v2"), evmChainId, vault, lezConfigId, burnRoot, NOut, token, amount, recipient, relayer, relayFee)` |
| release guest input | private | 1340 | `burnRoot ‖ burnIndex:u64 ‖ directions:u32 ‖ siblings:32×32 ‖ sOut ‖ r ‖ evmChainId ‖ vault ‖ configId ‖ token ‖ amount ‖ recipient ‖ relayer ‖ relayFee` |

Statements:

* `verify_claim(root, {depositor, token, amount, leafIndex, path, s, kind, dest}, evmChainId, vault) -> N`:
  recompute `C` and the leaf, check the path with **direction bits = leafIndex** to `root`, return
  `N`. `evmChainId` and `vault` enter only `N`: the LEZ program must take them from its configuration.
  `register_claim` records `(evmChainId, vault, N)` in one write-once namespace.
* `verify_release(burnRoot, {burnIndex, path, sOut, r}, {evmChainId, vault, configId, token, amount,
  recipient, relayer, relayFee}) -> NOut`: recompute `COut` and the burn leaf, check the path (direction
  bits = burnIndex). The guest commits `release_journal(...)`. Its cycle count does not depend on the
  witness (asserted over 64 random witnesses; see RESULTS).
* `recover(kAuth, …, deposits, g, dest_of)`: per deposit, one HMAC and up to `2g` SHA-256 (g seed-derived
  account indices × both kinds). `recover_burns(keys, configId, …, notes, gap)`: regenerate `r_i`,
  keep notes whose MAC verifies, decrypt the memo, authenticate it against `COut`. `view_burns(kView, …)`
  does the first half with the view key alone. `register_burn_tag` refuses a repeated `r`.

## Reproduce

Prerequisites, all local: Rust 1.93 with the `riscv32im-unknown-none-elf` target; RISC Zero's toolchain
via `rzup` (Rust 1.88.0, cpp 2024.1.5, r0vm 3.0.5); Docker (x86-64) for the Groth16 step; Foundry;
Python 3. Never write to `/tmp` on this machine: every script sets `TMPDIR` to `spike/tmp`.

```sh
cd spike/ferrata-bridge
(cd evm-verify && forge install --no-git foundry-rs/forge-std@v1.10.0 risc0/risc0-ethereum@v3.0.0)
                                               # lib/ is not committed; evm-verify also needs the vault's
                                               # lib/ (OpenZeppelin 5.4.0), installed as ../vault/README.md says
scripts/check.sh                               # fmt, clippy, no_std build, core tests, Python vectors,
                                               # executor checks, cycle invariance, receipts, Foundry
cargo run --release -p ferrata-bridge-core --example recover_bench   # recovery at 1e5 and 1e6 deposits
python3 -I scripts/mutants.py                  # mutation pass over the core
cargo build --release -p host
for c in eth erc20 ethpull; do scripts/prove-bench.sh prove $c; done     # succinct receipts, timed
for c in eth erc20 ethpull; do scripts/prove-bench.sh groth16 $c; done   # Groth16 wraps, timed
./target/release/host fixtures                 # artifacts -> evm-verify/test/fixtures/v2
(cd evm-verify && forge test -vv)              # add --isolate for whole-transaction gas
```

## What is real and what is mocked

Real: the v2 release guest (image id `0xdda76672…c5`) and its succinct and Groth16 receipts made on this
machine with r0vm 3.0.5 and RISC Zero's Docker prover; `RiscZeroGroth16Verifier` built from risc0-ethereum
v3.0.0 `ControlID`, behind the unmodified `RiscZeroVerifierEmergencyStop`; the v2 `FerrataVault` compiled
unmodified from a pinned read-only export of `spike/bridge-evm` branch `v2` (`evm-verify/vendor/vault-v2`,
commit `1d61b32`, sha256 `3f90645f…`); the vault's own `submitCandidate`, `recordBeacon`, `finalize`,
`release`, `claimOwed`; the payouts.

Mocked: the LEZ checkpoint proof (the checkpoint program is built elsewhere and does not exist yet). In
`evm-verify/test/v2/VaultV2.t.sol` it is answered with `vm.mockCall` on the emergency stop for exactly the
`verify(seal, checkpointImageId, digest)` call of each checkpoint journal; every release proof reaches the
real verifier. The burn tree is built by the host, not by a LEZ program; the LEZ states `S` are computed by
this crate from test values. The claim guest is a measurement, not LEZ's private-execution circuit.

Versions: risc0-zkvm / risc0-build / risc0-groth16 3.0.5, r0vm 3.0.5, guest Rust 1.88.0 (rzup), host Rust
1.93.0, `sha2-v0.10.8-risczero.0` (git 244dc3b), Docker image `risczero/risc0-groth16-prover:v2025-04-03.1`
(`sha256:a4f80ce2…`), risc0-ethereum v3.0.0 (32aa0b6f), forge-std v1.10.0, Foundry 1.3.6-nightly (a4d66e8),
solc 0.8.26 (200 runs, cancun). OpenZeppelin for the vault: 5.4.0 from `spike/bridge-evm/lib` (read-only).

Licence: MIT OR Apache-2.0.
