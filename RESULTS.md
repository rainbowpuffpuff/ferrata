# Results

Every number below comes from a command run in this directory on 9 Nov 2026; the logs are in
`artifacts/logs/` (each proving log records the machine, `r0vm --version`, the Docker image digest,
the load average before and after, and GNU time's report). Derived figures (ratios, sums) show their
inputs.

**Machine.** AMD Ryzen 9 7940HS (a laptop part: 8 cores, 16 threads), 54 GiB RAM, no NVIDIA GPU,
Fedora 44, Linux 7.2.13, x86-64. **Load:** other agents were using the machine throughout. `top` at
22:58 and 23:01 showed another agent's `r0vm` proving job on 10–12 cores with 8.5 GB RSS. The 1-minute
load average was 13–35 during every proving measurement (16 hardware threads), so all proving wall
times are pessimistic for this machine; CPU time is reported next to them. The end-of-session repeat
("Timing re-run") ran under even heavier load. No idle-machine figure exists.

## Headline

| | result |
|---|---|
| core tests | 36 pass (19 property tests at 256 cases each); clippy `-D warnings`, `cargo fmt --check`, `riscv32im-unknown-none-elf` no_std build and clippy: clean |
| vectors | 6 files; 245 values recomputed independently in Python (stdlib only): all equal |
| deposit tree vs the Solidity vault | every value of the vault's own `tree.json` (8 deposits) reproduced bit for bit |
| journal vs Solidity | Rust journal = `abi.encode(...)` for 6 vectors, and = the vault's own `_releaseJournalDigest` for the 5 deployable ones |
| recovery, 100,000 deposits, 7 own | finds exactly the 7; median 30.3 ms (min 29.7, max 32.3, 7 runs, one thread, 1-min load 4.01) |
| release guest | 65,832 user cycles, 1 segment (po2 17); without RISC Zero's sha2 patch: 342,062 (po2 19) |
| succinct proof | 43.2 s, 109.4 s and (re-run) 95.8 s wall at load 20–34; 558–622 CPU-s; peak RSS 1.46 GB (host + r0vm); receipt 223,930 bytes |
| Groth16 wrap | 82.3 s, 91.2 s and (re-run) 162.9 s wall in total at load 13–35; identity_p254 alone 156.3 s (re-run); Docker prover ran 19–39 s, largest process peak RSS 7.32 GiB; seal 260 bytes |
| selector, verifier | `0x73c457ba`; risc0-ethereum v3.0.0 `RiscZeroGroth16Verifier` verifies the 3.0.5 receipts (claim confirmed) |
| gas, `verify` alone | 224,320 in the verifier's frame; 250,948 as its own transaction (anvil receipt) |
| gas, release ETH | 350,194 per transaction (`forge --isolate`), 345,394 when the release empties the escrow slot (anvil receipt) |
| gas, release ERC-20 | 378,055 per transaction (`forge --isolate`), 368,455 when two slots are emptied (anvil receipt) |
| Foundry | 19 tests pass, including every negative case |
| mutation pass on the core | 18 hand-written mutants (`scripts/mutants.py`): 17 killed, 1 survived, and that one is an equivalent mutant |

## Part A: `ferrata-bridge-core`

`cargo test -p ferrata-bridge-core`: 36 tests, all pass (`artifacts/logs/check.log`).

| file | tests | what |
|---|---|---|
| `tests/encoding.rs` | 4 + 4 property | hashed tags = SHA-256 of their strings = the vault's constants; prefix tags exact and prefix-free; every layout restated field by field; journal = eleven ABI words, decode rejects non-canonical padding; codec round trips |
| `tests/tree.rs` | 3 + 5 property | empty roots; index ≥ 2^32 rejected (2^32 would otherwise alias index 0); last index; incremental = full tree; every path verifies; tampered sibling, leaf or root fails; wrong index fails; old root rejects a newer leaf |
| `tests/inbound.rs` | 1 + 5 property | claim verifies; **every single-field change** of a claim fails (`root, token, amount, leafIndex, path, s, dest`) or changes `N` (`chainId, vault`); duplicate commitments get distinct nullifiers; the same deposit gets the same `N` under every later root; every derivation changes with each input; recovery finds exactly 7 of 100,000 and each recovered deposit claims with the recovered `N` |
| `tests/outbound.rs` | 2 + 5 property | release verifies and commits the journal; every single-field change fails (`burnRoot, leafIndex, path, s_out, chainId, vault, token, amount, recipient`), changes `N_out` (`lezConfigId`) or leaves verification alone but changes the journal (`feeRecipient, relayFee`); same `N_out` under every later root; derivations bind every input; memo round trip; duplicate `C_out` → distinct `N_out`; burn recovery with gap limit (finds counters 0,1,2,5 with limit 3 and also 9 with limit 4; a copied `r` with a forged memo is skipped but counts as used; notes are worthless to another chain or vault) |
| `tests/vectors.rs` | 6 | writes (`FERRATA_WRITE_VECTORS=1`) or checks `vectors/*.json` |
| `tests/vault_tree.rs` | 1 | `vectors/external/bridge-evm-tree.json` (the vault project's `tree.json`, 8 deposits made on `FerrataVault`): tag, 33 zero hashes, empty root, 8 leaf preimages, 8 leaves, 8 roots after insert, 8 paths, final root: all equal |

The "same nullifier on both claim paths" property is tested two ways: natively (public claim vs. the
wallet's recovery scan, `tests/inbound.rs`) and across execution environments: `host claim-check 16`
runs `verify_claim` inside the zkVM (the `claim` guest, riscv32im, accelerated sha2) for 16 deposits;
all 16 nullifiers equal the native ones (`artifacts/logs/claim-check.log`; 60,461–60,473 user cycles
each).

`python3 -I scripts/check_vectors.py vectors`: `vectors OK: 245 values recomputed independently`.

Recovery timing: `cargo run --release -p ferrata-bridge-core --example recover_bench` → "recover over
100000 deposits (7 own): 7 runs, ms each: 29.7, 30.4, 32.3, 31.6, 30.1, 30.0, 30.3; min 29.7 ms,
median 30.3 ms, max 32.3 ms" (1-minute load average 4.01 before and after). One HMAC and one SHA-256 per
deposit, plus the `dest_for` closure (one SHA-256 in the test).

## Part B: the release proof (RISC Zero 3.0.5)

Guest: `guests/src/bin/release.rs` reads the 1280-byte input, runs `verify_release`, commits the
352-byte journal; an invalid witness panics (`host execute`: "Guest panicked: release witness
rejected: RootMismatch" for a flipped `s_out` bit). The host checks every journal against the native
`release_journal` before saving anything. Image id `0x2ca5b06e8da21a4f51506ded03fa5bd89249080b30affe5350ee9da446fc4c41`.

### Execution (r0vm 3.0.5 executor, `artifacts/logs/execute.log`)

| guest build | user cycles | segments | padded |
|---|---|---|---|
| release, RISC Zero sha2 patch | 65,832 | 1 (po2 17) | 131,072 (paging 22,733, reserved 42,507) |
| release, crates.io sha2 (A/B, `sha2-unpatched-execute.log`) | 342,062 | 1 (po2 19) | 524,288 |
| claim (test vehicle), patched | 60,461–60,473 | 1 | |

The patch cuts user cycles 5.2× (342,062 / 65,832) and the padded segment 4×.

### Proving

| step | case | wall | load before → after | peak memory | output |
|---|---|---|---|---|---|
| succinct (`prove`) | eth | 109.36 s | 20.21 → 33.54 | 1,458,420 kB RSS (host + r0vm) | receipt 223,930 bytes (seal 222,668) |
| succinct (`prove`) | erc20 | 43.15 s | 27.55 → 23.13 | 1,457,600 kB | 223,930 bytes |
| Groth16 (`groth16`: identity_p254 in r0vm, then the Docker prover) | eth | 82.28 s | 19.65 → 15.12 | host + r0vm 1,443,948 kB; container cgroup `memory.peak` 9.23 GiB | receipt 1,201 bytes; seal 260 bytes |
| Groth16 | erc20 | 91.24 s | 13.19 → 16.88 | host + r0vm 1,443,900 kB; container 7.33 GiB | same |

The Docker container (`risczero/risc0-groth16-prover:v2025-04-03.1`) was seen by the monitor for
19.2 s (eth) and 23.5 s (erc20), so identity_p254 took roughly the remaining 63 s (82.28 − 19.2) and 68 s (91.24 − 23.5) of each wrap. The
cgroup `memory.peak` includes page cache (the prover reads its multi-GB proving key), which is why it
differs between two identical runs; the re-run below also records the largest per-process peak RSS
inside the container. CPU time: succinct 560–622 s user, wrap 888–897 s user (GNU time; host + r0vm,
excluding the container).

### Timing re-run (end of session, `FERRATA_ARTIFACTS=artifacts/rerun`)

| step | case | wall | load before → after | CPU (user) | peak memory |
|---|---|---|---|---|---|
| succinct | eth | 95.79 s | 22.25 → 32.27 | 557.95 s | 1,456,856 kB (host + r0vm) |
| identity_p254 alone | eth | 156.34 s | 30.37 → 35.15 | 887.38 s | 1,442,748 kB |
| Groth16 total (identity_p254 + Docker) | eth | 162.93 s | 35.15 → 31.21 | 898.88 s (host + r0vm) | host + r0vm 1,442,828 kB; Docker container seen 38.7 s, cgroup peak 7.34 GiB, largest process (`prover`) VmHWM 7.32 GiB |

The repeat was slower, not faster: the load was higher (22–35). Two conclusions hold regardless.
CPU time is stable across all runs (succinct 558–622 s; whole wrap 888–899 s in host + r0vm). identity_p254,
not the Groth16 prover, is most of the wrap: run alone it took 887 CPU-seconds, about all of the wrap's
host + r0vm CPU time, while the Docker container ran only 19–39 s. The container's prover process itself peaks at about 7.3 GiB RSS; the 9.23 GiB
of the first run was the cgroup figure, which includes page cache. The re-run log's own summary line
printed 0 for the cgroup peak (a bug: it took the poll made as the container exited); the corrected
figure is appended to that log and computed from the saved monitor data
(`20261109T220927Z-groth16-eth.monitor.txt`).

The Docker-based wrapper ran here without changes: Docker 29.9.2, no `--user` or SELinux option needed
(the daemon runs without SELinux labelling although the host enforces SELinux). The container writes
`proof.json` as root into the work directory (`RISC0_WORK_DIR=artifacts/work/<case>`, git-ignored);
the directory is ours, so it can still be deleted.

### Selector and verifier version: claim confirmed

Checked, not assumed:

1. risc0-zkvm 3.0.5 `Groth16ReceiptVerifierParameters::default()`: control_root
   `a54dc85a…998c1f56`, bn254_control_id `c07a6514…666e4404`, digest `73c457ba541936f0…d3c6eedc`, so the
   selector is `0x73c457ba` (`host verify`).
2. Both receipts made by r0vm 3.0.5 carry `verifier_parameters = 73c457ba…` and the seals start with
   `0x73c457ba`.
3. risc0-ethereum v3.0.0 `ControlID.sol`: `CONTROL_ROOT` equals the Rust control_root byte for byte;
   `BN254_CONTROL_ID = 04446e66…14657ac0` is the Rust value byte-reversed (the file says so). The
   verifier computes `SELECTOR` from these in its constructor: `0x73c457ba` in Foundry and on anvil,
   and it verifies both real seals. v3.0.1 has no changes under `contracts/src/groth16/`, so it
   matches too.
4. `contracts/deployment.toml` at v3.0.0 lists `RiscZeroGroth16Verifier` version 3.0.0, selector
   `0x73c457ba`, at `0x2a098988600d87650Fb061FfAff08B97149Fa84D` on Ethereum mainnet, Sepolia, Holesky,
   Hoodi and other chains. That is RISC Zero's record: I did not query any chain or compare deployed
   bytecode (no RPC access in this task).

## Part C: on-chain verification, measured

`evm-verify/`: 19 Foundry tests, all pass. `FerrataVault` is compiled unmodified from
`../../bridge-evm/src` with the vault project's own OpenZeppelin and compiler settings.

### Gas

| what | how | gas |
|---|---|---|
| `verify` alone | `vm.snapshotGasLastCall`: gas used inside the verifier's call frame | 224,320 |
| `verify` alone | `gasleft()` delta around the call in the test: frame + caller-side CALL (cold account, ABI-encoding the 260-byte seal) | 229,623 |
| `verify` alone | as its own transaction, anvil receipt `gasUsed` | **250,948** (= 21,000 + 5,628 calldata + 224,320) |
| release ETH (0.1 ETH) | `vault.release` frame, default `forge test` | 316,174 |
| release ETH | whole transaction, `forge test --isolate` | **350,194** (= 21,000 + 6,520 calldata + 322,674) |
| release ETH | whole transaction, anvil receipt; this release empties `escrowed[ETH]`, refund 4,800 | 345,394 |
| release ERC-20 (100 TUSD, relay fee 0.25) | `vault.release` frame, default mode | 339,519 |
| release ERC-20 | whole transaction, `--isolate` | **378,055** |
| release ERC-20 | whole transaction, anvil receipt; escrow and the vault's token balance go to 0, refunds 2 × 4,800 | 368,455 |
| release via `acceptBurnRoot` with the root-mocking wrapper | frame, default mode | ETH 319,727, ERC-20 343,072 (the wrapper's extra CALL: +3,553) |
| `depositETH`, first deposit | anvil receipt | 187,757 |
| `deposit` ERC-20, second deposit | anvil receipt | 187,338 |

The three ways of counting reconcile exactly: anvil = `--isolate` − refunds (350,194 − 4,800 =
345,394; 378,055 − 9,600 = 368,455), and `--isolate` = 21,000 + calldata + execution. Default-mode
Foundry frames are lower than the `--isolate` execution gas (ETH 322,674 vs 316,174; ERC-20 350,019 vs
339,519) because the test reads balances and vault state just before the release, in the same
transaction, which warms what the release then touches: ETH, the recipient account (2,500) and the
`escrowed` and `nullifierUsed` slots (2 × 2,000) = 6,500; ERC-20, the token account (2,500), the
recipient and fee-recipient balance slots, `escrowed` and `nullifierUsed` (4 × 2,000) = 10,500. The
`--isolate` and anvil figures are the ones to quote. Use 350k (ETH) and 378k (ERC-20) per release as the planning figure, of which
~224k is the Groth16 verification. The ERC-20 figure includes two fresh balance slots (recipient and
relayer); by EIP-2929/2200 pricing (not measured) a recipient that already holds the token would cost about 17,100 less (5,000 instead of 22,100 for its balance slot). Anvil ran the Prague hardfork;
Foundry tests run with `evm_version = cancun` (the vault project's setting). EIP-7623's calldata floor
does not bind: the ETH release calldata is 580 bytes (350 non-zero, 230 zero), so the floor is
21,000 + 10 × (230 + 4 × 350) = 37,300, far below the ~345k used.

The burn root was marked accepted by a direct storage write (`vm.store` / `anvil_setStorageAt`) in
the measured runs, because the LEZ burn-root proof does not exist yet. A mock stands in for that
proof only; the release proof, verifier, image id, journal and payouts are real.

### Negative cases (all revert)

Against the real verifier alone (`Groth16Verify.t.sol`): the eth seal with the erc20 journal and vice
versa (`VerificationFailed`); each of the 8 proof words with one bit flipped (each `VerificationFailed`
under a 2M gas cap: an invalid curve point makes the BN254 precompiles consume all the gas they are
given, so without a cap a tampered seal burns the caller's whole gas limit); the selector flipped
(`SelectorMismatch`); each of the 11 journal words flipped; a wrong image id; a truncated seal.

Through the vault (`VaultRelease.t.sol`): the other case's seal; a tampered seal; the right seal with
one request field changed: recipient, fee recipient, relay fee (to the other allowed value, 0),
nullifier, denomination index (the escrow covers the larger amount; the fee is standard for both
indices, so only the proof can object), token, burn root (another accepted root), chain id. Each
reverts with the verifier's `VerificationFailed` and leaves the nullifier unused; afterwards the
unmodified requests still release. Replaying a successful release reverts `NullifierAlreadyUsed`
(Foundry and anvil).

## Not done, weaker than specified, surprising

* **Proving times were all taken under heavy load** (another agent's r0vm job on 10–12 cores; load
  average 13–35). Three succinct runs of the same work took 43 s, 96 s and 109 s, which shows the
  contention; the end-of-session repeat was slower still. No idle-machine number exists. CPU time
  (558–622 s succinct, ~890 s identity_p254) is the steadier figure.
* **No device other than this laptop CPU was measured.** Phone figures below are extrapolations.
* **The LEZ burn-root proof is mocked**, and the burn tree is built by the host, not by a LEZ program.
* **The image id is not reproducible across machines as built.** The guest was built with the local
  rzup toolchain, not with RISC Zero's Docker guest builder (`cargo risczero build`), which this task's
  network allowance did not cover. An immutable vault pins this id, so a production build must be
  reproducible.
* **`recover_burns` takes `chainId` and `vault`**, which the brief's signature omits: they are needed to
  recompute `C_out`. `recover` takes structs instead of tuples.
* **Clippy on the host workspace runs with `RISC0_SKIP_BUILD=1`**: under clippy, its driver leaks into
  risc0-build's inner guest build, which then lacks the zkVM `std`. The guest crate is linted separately
  with the risc0 toolchain and the cfg flags risc0-build passes.
* **Docker memory comes from polling every 0.2 s**, not from GNU time inside the container. Per-process
  `VmHWM` is that process's own high-water mark, so it is exact unless the peak came in the last 0.2 s;
  it was recorded for the re-run only (7.32 GiB). The cgroup figure includes page cache.
* **The deployed verifier at `0x2a09…a84D` was not checked on chain**, only RISC Zero's
  `deployment.toml`.
* **Surprising:** identity_p254 (the STARK-to-BN254 recursion before Groth16) took 3–4 times as long
  as the Docker Groth16 prover itself (~63–68 s vs ~19–24 s in the first runs; 156 s alone vs 39 s in
  the re-run; all under load).
* **Surprising:** a tampered Groth16 seal costs whoever submits it nearly all the gas they attach (BN254
  precompile failure).
* **Surprising:** the 65,832-cycle guest still pays the fixed recursion cost: a 1-segment succinct proof
  used 558–622 CPU-seconds here.
* **Mutation testing is hand-written, not cargo-mutants** (not installed; I did not install it):
  18 mutants over the tree, encodings, statements, recovery scans and the PRF wrapper. 17 are killed
  (`artifacts/logs/mutants.log`). M04 survives and is equivalent: it writes a frontier branch at every
  0 bit instead of only the lowest. Every extra write is overwritten by the insertion that completes
  that subtree before any later index reads it, so roots are unchanged; in Solidity it would only cost
  gas. M02 was first skipped (its snippet predated rustfmt) and was re-run with the formatted snippet:
  killed.
* Not attempted: a LEZ-side claim program; deposit-side gas beyond the two incidental anvil deposits
  (the vault project measures those).

## Encodings: what I believe is wrong, risky or worth changing

1. **The claim binds `chainId` and `vault` only through the nullifier.** The deposit leaf (fixed by the
   vault) has no chain or vault in it, so `root` is the only link to the vault. If the LEZ program ever
   took `chainId`/`vault` from the claimant, each value would give a fresh nullifier for the same
   deposit: unbounded minting. They must be configuration constants, and in a private claim the proof's
   public `chainId`/`vault` must be checked against them. Tested as a property (changing either changes
   `N`).
2. **Inbound nullifiers do not bind the LEZ configuration.** `fid` contains chain and vault but not
   `lezConfigId`, and nullifier sets are per LEZ program. A second LEZ configuration that imports the
   same vault's roots could mint against the same deposits. This does not drain the vault (releases
   need burn roots proven for the vault's own immutable `lezConfigId`), but that configuration's wrapped
   token would be unbacked. Suggest: the LEZ program pins the vault's code hash, which commits to its
   immutables (`lezConfigId`, image ids, verifier), and/or put `lezConfigId` into `fid`.
3. **Burn-counter reuse leaks.** If a wallet reuses `i` (two devices on one seed, or a gap limit
   smaller than the real gap after a crash), `r` repeats: same `s_out`, and the two memos XOR to
   `recipient₁ ⊕ recipient₂`. For the same recipient, `C_out` repeats: two public burns linked as one
   owner. Suggest: the LEZ burn program rejects a repeated `r`, and the wallet scans before every burn.
4. **`r` and `memo` are not covered by the burn leaf.** The burn tree authenticates
   `(leafIndex, token, amount, C_out)` only. Recovery depends on reading `(r, memo)` from LEZ blocks.
   Whoever serves those can make recovery fail (liveness), though never steal. Including `r` in the
   leaf would let a wallet check its notes against an accepted root.
5. **`feeRecipient = 0` means "pay `msg.sender`"** (vault semantics). With a non-zero relay fee, anyone
   who sees the transaction (or the Groth16 wrapper) can front-run it and collect the fee. The principal
   is safe. Wallets should always bind a concrete fee recipient.
6. **Chain id width.** `s`, `fid` and `C_out` carry a `u64` chain id; the vault's journal carries
   `uint256 block.chainid`. Chains with ids ≥ 2^64 cannot be served (I know of none). The journal
   test includes chain id 2^64 − 1.
7. **Recovery assumes `dest` is derivable** from the seed and `(depositor, j)`. A wallet that picks a
   random fresh LEZ account per deposit cannot recover by trial recomputation.
8. **The vault's root image id is a placeholder in these tests**, and `releaseImageId` depends on a
   non-reproducible build (above).

No collision or domain-separation problem was found in the formulas as specified: all tagged
preimages are prefix-free and fixed-length per tag, leaves and nodes are separated from each other
and from tagged values, and HMAC uses are separated by their own tags.

## Can a phone or laptop make the succinct receipt, and leave only the Groth16 wrap to a third party?

**Laptop: yes, measured.** On this laptop CPU (Ryzen 9 7940HS), under heavy contention, the succinct
receipt took 43–110 s wall in three runs, 558–622 CPU-seconds, and 1.46 GB peak RSS (r0vm 3.0.5, CPU
only). The release statement is tiny
(65,832 cycles, one 2^17 segment), so most of the cost is RISC Zero's fixed per-proof work (segment
proof plus lift), not the bridge's logic. Any recent 4–8-core laptop with 4 GB free should finish in
minutes. That is an estimate: no other machine was measured.

**Phone: plausible, not measured.** About 1.5 GB of RAM and several CPU-minutes are within reach of
recent phones, but the prover was not built or run for Android or iOS here (I did not check RISC Zero's
support for that), and thermal throttling would stretch the time. Treat phone proving as a research item, with
a remote prover as the fallback (that prover sees the witness).

**The wrap: yes, and the wrapper learns nothing private.** The Groth16 step needs x86-64 and Docker
(risc0-groth16's docs: "requires an x86 architecture and Docker"), about 7.3 GiB for the prover process
(7.3–9.2 GiB cgroup peak including page cache), and r0vm for identity_p254. It took 1.4–2.7 min here under load.
Most of its CPU time is identity_p254 (~890 CPU-s here), which is also why a wrapper needs a real
machine, not just Docker. Its input is the succinct receipt (224 KB) and the journal. The journal is public anyway (it is the
release's calldata), and the receipt is a zero-knowledge STARK of the witness (RISC Zero's claim; not
re-examined here). The wrapper cannot change recipient, amount, fee or fee recipient, since all are in
the journal the proof commits to. It can:

* refuse or delay (liveness only: any other x86 machine can wrap);
* learn when the user is about to release and from which IP (network-level linkage; must be disclosed);
* submit the release itself, which changes nothing except that it collects the fee if the user bound
  `feeRecipient = 0`.

The wrap needs no secret, so it can be a permissionless market, or the relayer that submits the
transaction.
