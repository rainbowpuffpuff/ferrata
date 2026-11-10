# Results (SPEC-v2)

Every number below comes from a command run in this directory on 10 Nov 2026; the logs are in
`artifacts/logs/` (each proving log records the machine, `r0vm --version`, the Docker image digest, the
load average before and after, and GNU time's report). Derived figures show their inputs. The older v1
results follow at the end, clearly labelled. In the committed logs, the absolute path of the directory
the commands ran in is written `<workspace>` (this directory was then `<workspace>/ferrata-bridge`).

**Specification.** "SPEC-v2 §x" is [`docs/design/spec/bridge-spec-v2.md`](../docs/design/spec/bridge-spec-v2.md)
and "v3 Dn" is [`docs/design/spec/bridge-spec-v3-changes.md`](../docs/design/spec/bridge-spec-v3-changes.md);
source comments cite them by their working names, `notes/design/SPEC-v2.md` and `notes/design/SPEC-v3-delta.md`.

**Machine.** AMD Ryzen 9 7940HS (a laptop part: 8 cores, 16 threads), 54 GiB RAM, no NVIDIA GPU, Fedora 44,
Linux 7.2.13, x86-64. Other agents shared the machine; the load average is printed next to every timing.

## Part 1: `ferrata-bridge-core` v2

`cargo test -p ferrata-bridge-core`: **54 tests pass** (28 of them property tests at proptest's default
256 cases), clippy `-D warnings`, `cargo fmt --check`, and the `riscv32im-unknown-none-elf` `no_std`
build and clippy are clean.

| file | tests | what |
|---|---|---|
| `tests/encoding.rs` | 5 + 6 property | hashed tags = SHA-256 of their v2 strings; 17 prefix tags exact and prefix-free and never starting with a leaf/node prefix; every layout restated field by field from the spec text (16 preimages, `keySetHash`, `burnFrontierHash`, `S`); both journals are their ABI words and decoding rejects non-canonical padding (including a `sunset` word other than 0/1); the guest-input codecs round-trip; an unknown kind byte is refused |
| `tests/tree.rs` | 5 + 6 property | direction bits must equal the leaf index (an index of 2^32 + 1 with the path of 1 is refused; a leaf claiming index 5 whose path to the root has directions 3 is refused); tampered sibling, leaf, root or direction bits fail; incremental = full tree; the canonical frontier resumes the tree exactly and is zero where the count has a 0 bit |
| `tests/inbound.rs` | 3 + 6 property | keys split from the seed; **every single-field change of a claim** fails (`root, depositor, token, amount, leafIndex, directions, sibling, s, kind, dest`) or changes `N` (`chainId, vault`); duplicate commitments get distinct `N`; derivations bind every input; the registry is keyed by (chain, vault, N); recovery over 100,000 deposits finds exactly the 7 own deposits inside a gap-5 window (both kinds), an 8th at account 7 only with a wider window, and nothing with another seed or with `kView` |
| `tests/outbound.rs` | 2 + 8 property | release verifies and commits the journal; **every single-field change** fails (`burnRoot, burnIndex, directions, sibling, sOut, r, chainId, vault, configId, token, amount, recipient`) or, for `relayer` and `relayFee`, leaves verification alone and changes the journal; same `NOut` under every later root; derivations bind every input (and `r`, pad, MAC are `kView` values, `sOut` a `kAuth` value); burn recovery follows the gap limit, ignores notes it did not author, drops a tampered memo |
| `tests/checkpoint.rs` | 3 + 2 property | `t(n)` table (n = 1…10, 100, 0, 2^32−1); `S` = SHA-256 of its preimage and changes with every field; key order and count matter; the frontier hash depends on the leaves only |
| `tests/vectors.rs` | 7 | writes (`FERRATA_WRITE_VECTORS=1`) or checks the eight `vectors/*-v2.json` files |
| `tests/vault_tree_v2.rs` | 1 | the v2 vault's own deposit-tree vectors, bit for bit (below) |

Negative tests for specific attacks (each a named test):

| attack | test | result |
|---|---|---|
| claim one deposit once per root | `attack_same_deposit_under_two_roots_gives_same_nullifier` | same `N` under roots n and n+1; the registry refuses the second |
| a burn of another configuration | `attack_burn_from_another_config_cannot_satisfy_a_release` | `RootMismatch` (verifies only for its own `configId`) |
| wrong token or amount | `attack_wrong_token_or_amount_fails` | `RootMismatch` |
| copied note on another token/amount | `attack_copied_note_with_other_token_or_amount_fails_the_mac` | MAC fails (also for another `COut` and another view key); the scan ignores it |
| repeated `r` | `attack_repeated_burn_tag_is_refused_by_the_registry` | `RepeatedTag` for a reused `i` and for a copied `r` |
| private kind opened as public | `attack_private_kind_commitment_cannot_be_opened_as_public` | `RootMismatch` both ways |

Vectors: `python3 -I scripts/check_vectors_v2.py vectors` → `v2 vectors OK: 529 values recomputed
independently` (the canonical frontier is recomputed there from complete subtrees, a different algorithm
from the Rust incremental tree).

Journal equality with Solidity (`evm-verify/test/v2/JournalEquality.t.sol`): the Rust release journal equals
`abi.encode` of its eleven fields for all 6 vectors, and the Rust checkpoint journal equals `abi.encode` of
its thirteen fields (with `uint32 nMin/tMin`, `uint64` counts, `bool sunset`) for all 3 vectors. The final v2
vault encodes both with exactly these types and order (`_checkpointDigest`, `_releaseJournalDigest`), and the
end-to-end test exercises both against the vault's own code: its releases verify only if the vault rebuilds
the proven release journal byte for byte, and the mocked checkpoint proof answers only the digest of the
test's Solidity restatement of the checkpoint journal (the restatement JournalEquality shows equal to the
Rust encoding), so a vault that encoded the checkpoint journal differently would have reverted.

Deposit tree vs. the Solidity v2 vault (`tests/vault_tree_v2.rs`): every value of the vault project's own
`tree-v2.json` (8 deposits made on the vault, read back; vendored in `vectors/external/` with its sha256)
reproduced bit for bit: tag, 33 zero hashes, empty root, 8 leaf preimages binding the depositor, 8 leaves,
8 roots after insert, 8 paths with direction bits = index, final root.

Recovery v2 (`cargo run --release -p ferrata-bridge-core --example recover_bench 5 100000 1000000`, one
thread, g = 5, so 10 candidate commitments per deposit; `artifacts/logs/recover-bench-v2.log`, load
3.40 → 3.87):

| deposits | runs | min | median | max | public data read |
|---|---|---|---|---|---|
| 100,000 | 5 | 118.8 ms | 128.9 ms | 156.5 ms | 6,000,000 B to match (depositor 20 + j 8 + C 32 per deposit); 12,000,000 B with every leaf's fields for paths |
| 1,000,000 | 5 | 1,202.2 ms | 1,308.6 ms | 1,561.5 ms | 60,000,000 B to match; 120,000,000 B with every leaf |

Peak RSS of the whole run: 119,132 kB. These byte counts are the scan's inputs as packed fields; the
same data fetched as raw `eth_getLogs` entries is larger (topics and ABI words plus block and transaction
metadata), not measured here.

Mutation pass (`python3 -I scripts/mutants.py`, `artifacts/logs/mutants-v2.log`): **50 hand-written mutants
over every module, 49 killed, 1 survivor that is equivalent.**

* T07 survives: it writes a frontier branch at every 0 bit of the index instead of only the lowest. The extra
  entries sit at heights whose bit is still 0 in the new leaf count; the canonical frontier ignores them and
  the next insertion that sets that bit overwrites them before any read. Roots and frontier hashes are
  unchanged, so no test can kill it (in Solidity it would only cost gas).
* C02 first did not compile: my mutant wrote `x / 3 as u32`, which parses as `x / (3 as u32)`. Re-run with the
  mutation parenthesised: killed.
* What the mutants break: direction-bit check (removed, truncated), left/right order, wrong direction bit,
  root comparison, incremental append, frontier canonical form and resume, the kind byte in `C`, the slot in
  `fid`, the depositor in the leaf, the burn-leaf prefix, `r` and `configId` in the burn leaf, `configId` in
  `sOut`, the amount in the MAC, journal field order and `sunset`, `S`'s `sunset`, the key count in
  `keySetHash`, journal decoding, `j` in `s`, the claim nullifier keyed on the root, the claim ignoring kind or
  path, both registries, recovery kinds and window, unknown kind bytes, release `r`/vault/`NOut`, the MAC check
  (bypassed, one byte), burn-tag registry, MAC filter, memo authentication, gap limit, tags from `kAuth`,
  plaintext memo, `sOut` from `kView`, swapped keys, the PRF input, and `t(n)`.

## Part 2: release guest v2

`guests/src/bin/release.rs` reads a 1340-byte `ReleaseInput`, runs `verify_release` (§5.2) and commits
the 352-byte journal; an invalid witness panics. Image id
`0xdda76672e6e05535117ed6880227a414420344cba9c47b6e112c7dabc5b027c5`.

**Cycles** (`artifacts/logs/execute-v2.log`): **80,501 user cycles, one segment (po2 17).** Bad `sOut`
(`RootMismatch`) and bad direction bits (`DirectionsMismatch`) are refused inside the guest.

**Witness independence** (`host invariance 64`, `artifacts/logs/invariance-v2.log`): 64 random valid
witnesses (leaf indices 0, 2^32−1, 0x55555555, 0xAAAAAAAA and 60 random; random siblings, secrets and public
fields): one (user cycles, po2) value, `(80501, [17])`. The three proven cases also report identical full
session stats including paging: 131,072 total = 80,501 user + 23,068 paging + 27,503 reserved.

**Found and fixed on the way:** my first version ordered left/right with a branch-free mask, and the compiler
turned it back into a branch (LLVM recognises the masked XOR as a select; rv32im has no conditional move, so
selects become branches). The guest's count was 64,136 + 4 × popcount(leaf index) exactly: it leaked the
Hamming weight of the burn index (`artifacts/logs/invariance-v2-before-fix.log`: 14 distinct counts over 64
witnesses). The mask now goes through `core::hint::black_box`; that costs 16k user cycles but stays within the
same 2^17 segment, so proving cost does not change. Without the empirical check this would have shipped.

**Proving**, all under heavy load from other agents (1-minute load average 20–40, logged per run):

| step | case | wall | load before → after | CPU (user) | peak memory |
|---|---|---|---|---|---|
| succinct | eth | 150.04 s | 30.72 → 39.59 | 651.42 s | 1,461,332 kB (host + r0vm) |
| succinct | erc20 | 119.74 s | 39.59 → 36.15 | 620.40 s | 1,458,416 kB |
| succinct | ethpull | 49.78 s | 36.15 → 28.61 | 569.41 s | 1,460,928 kB |
| Groth16 (identity_p254 + Docker) | eth | 117.45 s | 19.80 → 22.06 | 936.81 s | host + r0vm 1,444,332 kB; container ran 29.2 s, `prover` VmHWM 7.32 GiB, cgroup peak 8.23 GiB |
| Groth16 | erc20 | 127.08 s | 22.06 → 22.40 | 936.61 s | 1,443,584 kB; container 28.3 s, VmHWM 7.32 GiB, cgroup 8.61 GiB |
| Groth16 | ethpull | 175.74 s | 22.40 → 34.21 | 905.81 s | 1,444,300 kB; container 44.4 s, VmHWM 7.29 GiB, cgroup 7.31 GiB |

Receipt sizes: succinct 223,930 bytes (seal 222,668); Groth16 receipt 1,201 bytes; on-chain seal 260 bytes
(selector `0x73c457ba` + 256-byte proof). All six receipts verify locally (`artifacts/logs/verify-v2.log`).
The selector, `ControlID` and verifier-version findings of v1 hold unchanged (same r0vm, same risc0-ethereum
v3.0.0): `Groth16ReceiptVerifierParameters::default()` digest `73c457ba…`, every seal starts with it, and the
on-chain verifier built from v3.0.0 `ControlID` computes the same `SELECTOR` and accepts every seal.

### Timing on a quiet machine

Each step started only once the 1-minute load average was below 2 (`scripts/prove-bench.sh`, outputs in
`artifacts/rerun-quiet/`, not committed; the committed receipts are the ones above). That controls the start
only: other agents' jobs could begin while a step ran, and the CPU share column shows how much of the machine
each step actually got. "Load after" includes the step's own prover threads. CPU share is GNU time's "Percent
of CPU this job got" (user + system time over wall time; 1600 % would be all 16 hardware threads). For Groth16
it leaves out the Docker container's own CPU time, which GNU time cannot see, so it reads lower there. Logs in
`artifacts/logs/`:

| step | case | wall | load before → after | CPU (user) | CPU share | peak memory | log |
|---|---|---|---|---|---|---|---|
| succinct | eth | 48.59 s | 1.92 → 12.88 | 586.94 s | 1212 % | 1,457,296 kB (host + r0vm) | `20261110T062310Z-prove-eth.log` |
| succinct | erc20 | 43.79 s | 1.97 → 11.76 | 575.36 s | 1318 % | 1,460,428 kB (host + r0vm) | `20261110T062804Z-prove-erc20.log` |
| succinct | ethpull | 43.64 s | 1.87 → 9.85 | 575.41 s | 1323 % | 1,457,448 kB (host + r0vm) | `20261110T063113Z-prove-ethpull.log` |
| identity_p254 alone | eth | 71.57 s | 1.93 → 15.06 | 923.23 s | 1292 % | 1,443,184 kB (host + r0vm) | `20261110T063422Z-p254-eth.log` |
| identity_p254 alone | erc20 | 62.59 s | 1.95 → 12.14 | 887.27 s | 1420 % | 1,443,220 kB (host + r0vm) | `20261110T063814Z-p254-erc20.log` |
| identity_p254 alone | ethpull | 56.44 s | 1.91 → 11.91 | 816.72 s | 1449 % | 1,442,896 kB (host + r0vm) | `20261110T064126Z-p254-ethpull.log` |
| Groth16 (identity_p254 + Docker) | eth | 77.57 s | 1.99 → 12.81 | 823.04 s | 1062 % | 1,443,104 kB (host + r0vm); container ran 18.9 s, `prover` VmHWM 7.32 GiB, cgroup peak 7.33 GiB | `20261110T064423Z-groth16-eth.log` |
| Groth16 (identity_p254 + Docker) | erc20 | 78.88 s | 1.96 → 10.90 | 835.92 s | 1061 % | 1,443,912 kB (host + r0vm); container ran 20.0 s, `prover` VmHWM 7.31 GiB, cgroup peak 7.32 GiB | `20261110T064746Z-groth16-erc20.log` |
| Groth16 (identity_p254 + Docker) | ethpull | 77.52 s | 1.85 → 13.45 | 837.56 s | 1082 % | 1,443,944 kB (host + r0vm); container ran 18.6 s, `prover` VmHWM 7.32 GiB, cgroup peak 7.33 GiB | `20261110T065105Z-groth16-ethpull.log` |

An earlier automatic repeat (04:09–04:19 local time; log names are UTC, `artifacts/logs/20261110T03*`; outputs
in `artifacts/rerun-lowload/`, not committed) started at load 3.54 and ran its steps back to back, so later
steps began at loads of 9.8–18.8, mostly the previous step's own threads still in the 1-minute average. Its
succinct and identity_p254 steps got 85–91 % of the 16 threads (the quiet-start set above: 76–91 %):

| step | case | wall | load before → after | CPU (user) | CPU share | peak memory | log |
|---|---|---|---|---|---|---|---|
| succinct | eth | 37.13 s | 3.54 → 9.78 | 519.32 s | 1402 % | 1,460,116 kB (host + r0vm) | `20261110T030949Z-prove-eth.log` |
| succinct | erc20 | 40.39 s | 9.78 → 12.81 | 548.15 s | 1360 % | 1,457,952 kB (host + r0vm) | `20261110T031026Z-prove-erc20.log` |
| succinct | ethpull | 40.12 s | 12.81 → 15.44 | 555.35 s | 1387 % | 1,457,104 kB (host + r0vm) | `20261110T031106Z-prove-ethpull.log` |
| identity_p254 alone | eth | 60.57 s | 15.44 → 16.18 | 881.55 s | 1457 % | 1,443,460 kB (host + r0vm) | `20261110T031147Z-p254-eth.log` |
| identity_p254 alone | erc20 | 61.29 s | 16.18 → 17.84 | 890.01 s | 1454 % | 1,443,260 kB (host + r0vm) | `20261110T031247Z-p254-erc20.log` |
| identity_p254 alone | ethpull | 60.56 s | 17.84 → 18.76 | 876.62 s | 1449 % | 1,443,672 kB (host + r0vm) | `20261110T031349Z-p254-ethpull.log` |
| Groth16 (identity_p254 + Docker) | eth | 79.94 s | 18.76 → 17.00 | 871.01 s | 1091 % | 1,443,412 kB (host + r0vm); container ran 19.1 s, `prover` VmHWM 7.32 GiB, cgroup peak 7.64 GiB | `20261110T031449Z-groth16-eth.log` |
| Groth16 (identity_p254 + Docker) | erc20 | 79.59 s | 17.00 → 16.53 | 863.40 s | 1086 % | 1,444,064 kB (host + r0vm); container ran 18.9 s, `prover` VmHWM 7.32 GiB, cgroup peak 7.33 GiB | `20261110T031609Z-groth16-erc20.log` |
| Groth16 (identity_p254 + Docker) | ethpull | 79.72 s | 16.53 → 15.26 | 875.40 s | 1099 % | 1,443,148 kB (host + r0vm); container ran 18.2 s, `prover` VmHWM 7.32 GiB, cgroup peak 7.33 GiB | `20261110T031729Z-groth16-ethpull.log` |

Over both sets: succinct 37.1–48.6 s, identity_p254 alone 56.4–71.6 s, Groth16 77.5–79.9 s (its Docker
container ran 18.2–20.0 s of that), with the succinct and identity_p254 steps getting 76–91 % of the 16
hardware threads. Wall time mostly follows that share; the CPU time of a step also varied (succinct
519.3–586.9 CPU-s, identity_p254 816.7–923.2 CPU-s). These are the laptop's figures when the prover has most
of it; the committed receipts above took up to 4.0× as long under load.

## Part 3: claim guest (measurement only)

`guests/src/bin/claim.rs` runs `verify_claim` (§3.3: `C` with the kind byte, the leaf binding the
depositor, the path with direction bits = `leafIndex`, `fid`, `N`) and commits `root ‖ chain ‖ vault ‖ N ‖
token ‖ amount`. It is the bridge's part of a user's privacy-preserving claim, without LEZ's own circuit
around it, so it is a lower bound for user-side claim proving and the receipt a relayer would check before a
one-drop-per-nullifier gas drop. Image id `0x141329559d61e2aa260c11b98e822d6fe0ce4bee7874580be9b6518afad5c429`.

* **74,379 user cycles, one segment (po2 17)**, the same for 64 random witnesses of both kinds
  (`artifacts/logs/invariance-v2.log`).
* `host claim-check 16`: for 16 deposits (both kinds) the guest's journal, and so `N`, equals the native
  `verify_claim`.
* Succinct proof of the claim guest on the quiet machine (load below 2 at its start): **38.90 s** wall (load
  1.95 → 9.68), 534.45 CPU-s (1377 % CPU), peak RSS 1,461,752 kB; receipt 223,562 bytes; verifying it takes
  12.4 ms, which is what a relayer runs before a gas drop (`20261110T065453Z-prove-claim-claim.log`); the
  04:09 repeat took 38.81 s at 1397 % CPU (`20261110T031849Z-prove-claim-claim.log`).

## Part 4: the v2 vault, measured

`evm-verify/`: **21 Foundry tests pass** (journal equality 2, verifier and stop 10, vault end to end 9). The
vault is compiled unmodified from `evm-verify/vendor/vault-v2/`, a read-only export of the vault's
branch `v2` at `d64845b` (now in the history of `../vault/`; `FerrataVault.sol` sha256 `3f90645f…`).

Wiring, as SPEC-v2 §6 says: the deployer's nonces 0–3 create `RiscZeroGroth16Verifier` (v3.0.0
`ControlID`), `RiscZeroVerifierEmergencyStop` around it (unmodified, guardian = the vault's freeze authority),
the test token and the vault, whose immutable `verifier` is the stop. These are exactly the addresses the
proofs bind (asserted), and slot 2 holds `lezConfigId` (asserted).

**Checkpoint side: real path, mocked proof.** The checkpoint program does not exist yet. The test drives the
vault's own `submitCandidate` (journal with `echoNonce = 0`), waits `delta`, calls `recordBeacon`, and calls
`finalize` with the beacon's nonce echoed. **TEST ONLY:** each checkpoint proof is answered with `vm.mockCall`
on the stop, for exactly the `verify(seal, checkpointImageId, digest)` call of that journal. Everything else,
in particular every release proof, reaches the real stop and the real Groth16 verifier. A test shows the real
rule at work: a beacon recorded 1 s before `delta` has passed gives `BeaconTooEarly`.

### Gas

| what | how | gas |
|---|---|---|
| `verify` alone, through the stop | stop's call frame, cold (`vm.snapshotGasLastCall`, any of the 3 seals) | 230,107 |
| `verify` alone, through the stop | as its own transaction: 21,000 + calldata 5,628 + frame | **256,735** |
| `verify` alone, Groth16 verifier directly | frame (`gasleft()` delta incl. the CALL: 229,623) | 224,320 |
| release ETH (0.1 ETH, no relayer) | whole transaction, `forge test --isolate` | **367,988** |
| release ERC-20 (100 TUSD, relayer, 0.25 fee) | whole transaction, `--isolate` | **397,874** |
| release ETH to a recipient that rejects it (pull-payment fallback, relayer paid) | whole transaction, `--isolate` | **431,433** |
| the same three, `vault.release` frame in default mode | warmed by the test's own pre-reads (see v1 for the reconciliation) | 335,980 / 361,338 / 396,589 |

`artifacts/logs/20261110T023116Z-forge-gas-v2.log`. As in v1, `--isolate` runs each release as its own
transaction (cold state, 21,000 + calldata + execution); v1 reconciled that method exactly against anvil
receipts, and the anvil run was not repeated for v2. The stop adds 5,787 gas to a verification
(230,107 − 224,320: one more CALL and the `paused` read). Against v1, a release costs 17.8k more for ETH
(367,988 vs 350,194) and 19.8k more for ERC-20 (397,874 vs 378,055): the stop, the checkpoint lookups, the
release-delay schedule, expiring-freeze timestamps. A pull-payment fallback costs 63k more than a direct ETH
release: the failed transfer, then `owed` and `totalOwed` written from zero.

### Negative cases (all revert, nullifier stays unused)

Verifier and stop alone (`Groth16Verify.t.sol`): another journal's seal, each of the 8 proof words with one bit
flipped (under a 2M gas cap: a bad curve point makes the BN254 precompiles burn all the gas they get), a
flipped selector (`SelectorMismatch`), each of the 11 journal words flipped, a wrong image id. The guardian's
`estop()` blocks every proof afterwards; nothing unpauses (ownership transfer does not); a non-guardian cannot
stop it; the circuit breaker refuses a real seal for another claim and any non-zero claim digest.

Vault (`VaultV2.t.sol`): another case's seal; a tampered seal; the right seal with one request field changed:
recipient, relayer, relay fee (to the other allowed value, 0), nullifier, denomination (escrow covers it, the
fee is valid, the proof is not), token, another accepted burn root (a second finalised checkpoint), chain id.
Each reverts in the verifier (`VerificationFailed`). Also: an unaccepted root (`BurnRootNotAccepted`), a
replay (`NullifierAlreadyUsed`), and after the guardian stops the verifier, a valid release reverts with the
stop's `EnforcedPause`. The pull-payment release credits `owed[recipient][ETH]`; nobody else can claim it
(`NothingOwed`); the recipient, once it accepts ETH, collects it with `claimOwed`, once.

## Not done, weaker than specified, surprising (v2)

* **Quiet is approximate.** The committed receipts were made under load 20–40. The timing runs started at a
  load below 2, but the machine stayed shared and nothing stopped other jobs from starting mid-run: the prover
  got 76–91 % of the 16 threads. Part 2 lists every run's share.
* **The checkpoint proof is mocked** (`vm.mockCall`, only for the checkpoint journals' exact calls), the
  burn tree is built by the host, and the LEZ states `S` come from test values: no LEZ program was run.
* **`burnFrontierHash` has no encoding in SPEC-v2.** This crate defines one (tagged, canonical frontier);
  the checkpoint program's author must use the same, or `S` will not match.
* **The cycle count leaked the leaf index's Hamming weight** until the fix above. Branch-free code is not
  enough inside the zkVM; the check that caught it (`host invariance`) runs in `scripts/check.sh`.
* **Recovery uses a fixed window of account indices `0..g`** for each kind. A deposit to account `g` or higher
  is missed (the test plants one at account 7 and finds it only with a wider window). There is no sliding
  window; the wallet must keep destinations inside its gap.
* **The image id is not reproducible across machines as built** (local rzup toolchain, not RISC Zero's
  Docker guest builder, which the network allowance of this build did not cover). An immutable vault pins it.
* **The v1 vault source nearly went unrecorded:** the vault's repository (now `../vault/`) was rewritten
  before I copied it (my first copy turned out to be the in-progress v2 and was deleted). Its history still
  holds it (commit `c417dc2`, sha256 `50c3e693…`, the hash v1 recorded), now exported to
  `artifacts/v1/vault-source/`.
  v2 is pinned the same way (`evm-verify/vendor/vault-v2`).
* **No anvil run for v2.** Whole-transaction gas comes from `forge --isolate`, a method v1 reconciled exactly
  with anvil receipts.
* **Mutation testing is hand-written** (cargo-mutants is not installed): 50 mutants, 49 killed, 1 equivalent.
* **The claim guest is not LEZ's circuit**; its cycles are a lower bound for a private claim.
* **Surprising:** the v2 release costs 17.8k–19.8k gas more than v1, and a pull-payment fallback 63k more
  than a direct ETH release (two storage slots written from zero).

## SPEC-v2: what I believe is wrong, unsafe or underspecified

1. **Refunds can be paid inside the revocation window (unsafe).** §3.4 lets `refund` pay any leaf at or above
   `sunsetLeafCount` as soon as a sunset checkpoint is final; §4.4 lets the freeze authority revoke that
   checkpoint within `revokeWindow`, but refunds already paid cannot be undone. With `t` keys an attacker
   can finalise a fabricated sunset checkpoint with a low `importedLeafCount` after Δ; leaves that LEZ really
   imported (and that are claimable or claimed there) can then be refunded on Ethereum before the freeze
   authority revokes: a double spend bounded by the escrow. The v2 vault implements exactly this (no delay in
   `refund`). The same holds for releases when a token's release delay is shorter than `revokeWindow`; v2
   allows 0, which my own tests use. SPEC-v3-delta D5 (read after I found this) adds the fix: refunds wait
   until `sunsetFinalizedAt + revokeWindow`, and release delays must lie in `[minReleaseDelay, revokeWindow]`.
2. **`burnFrontierHash` is undefined** (encoding and canonical form). Two honest implementations can disagree
   on stale frontier entries; this crate zeroes them.
3. **The burn MAC does not cover the memo.** A front-runner who copies a victim's whole note (same `r`,
   token, amount, `COut`, MAC) but a different memo gets a `kView`-only viewer to report a wrong recipient. The
   full wallet drops the note (`COut` does not open), the victim's own burn failed on the repeated `r`, and the
   copier's burn is a gift, so nothing is stolen; still, `mac` over `memo` too costs nothing.
4. **`kView` discloses burns only.** Deposits (`s`, `C`) derive from `kAuth`, so a viewing key cannot show a
   user's deposits without also giving the claim secret (Soft Requirement 2 is met outbound only).
5. **`t(n)` tolerates no compromised key for `n` ≤ 2** (`t(1) = t(2) = 1`). The deployment floors
   `nMin`/`tMin` are the only protection; the documentation should say which values are safe.
6. **The constant-cycle requirement (§5.2) is fragile**: it held in the source and failed in the binary.
   Ask for a measured check in CI, not a code review.
7. **One nullifier namespace (§1) is keyed redundantly**: `N` already commits to the chain id and vault
   through `fid`. Harmless; the protection against a second configuration minting comes from the reciprocal
   pairing at `Init`, not from the namespace.
8. **Deployment by predicted CREATE address** (§1, v3 D2): one stray transaction from the deployer before the
   vault moves its address and invalidates `configId`. Deploy from a fresh, single-purpose key.

## Can a phone or laptop make the succinct receipt, and leave only the wrap to a third party?

Unchanged from v1 in substance (same r0vm, same recursion cost): on this laptop CPU a v2 succinct receipt
takes 519–651 CPU-seconds over the 9 runs logged and 1.46 GB peak RSS (one 2^17 segment plus RISC Zero's fixed
recursion work), 37.1–48.6 s wall when it got most of the 16 threads (Part 2). The wrap needs x86-64, Docker
and about 7.3 GiB for the prover process; the wrapper sees only the succinct receipt (224 KB) and the journal,
which is public anyway, and cannot change recipient, relayer or fee, which the journal binds. The v2 guest
also makes the cycle count independent of the witness, so the length of the computation reveals nothing even
outside the recursion step. Phones remain unmeasured.

---

# v1 results (older, superseded by SPEC-v2)

Everything in this section is about the **v1** encodings, guest and vault (code at commit `8f1eb0b`; vectors in `vectors/v1/`, receipts and logs in `artifacts/v1/`). The v1 vault source (sha256 `50c3e693…`) is exported to `artifacts/v1/vault-source/` from the vault's commit `c417dc2` (in the history of `../vault/`).

Every number below comes from a command run in this directory on 9 Nov 2026; the logs are in
`artifacts/v1/logs/` (each proving log records the machine, `r0vm --version`, the Docker image digest,
the load average before and after, and GNU time's report). Derived figures (ratios, sums) show their
inputs.

**Machine.** AMD Ryzen 9 7940HS (a laptop part: 8 cores, 16 threads), 54 GiB RAM, no NVIDIA GPU,
Fedora 44, Linux 7.2.13, x86-64. **Load:** other agents were using the machine throughout. `top` at
22:58 and 23:01 showed another agent's `r0vm` proving job on 10–12 cores with 8.5 GB RSS. The 1-minute
load average was 13–35 during every proving measurement (16 hardware threads), so all proving wall
times are pessimistic for this machine; CPU time is reported next to them. The end-of-session repeat
("Timing re-run") ran under even heavier load. No idle-machine figure exists.

### Headline

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

### Part A: `ferrata-bridge-core`

`cargo test -p ferrata-bridge-core`: 36 tests, all pass (`artifacts/v1/logs/check.log`).

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
all 16 nullifiers equal the native ones (`artifacts/v1/logs/claim-check.log`; 60,461–60,473 user cycles
each).

`python3 -I scripts/check_vectors.py vectors`: `vectors OK: 245 values recomputed independently`.

Recovery timing: `cargo run --release -p ferrata-bridge-core --example recover_bench` → "recover over
100000 deposits (7 own): 7 runs, ms each: 29.7, 30.4, 32.3, 31.6, 30.1, 30.0, 30.3; min 29.7 ms,
median 30.3 ms, max 32.3 ms" (1-minute load average 4.01 before and after). One HMAC and one SHA-256 per
deposit, plus the `dest_for` closure (one SHA-256 in the test).

### Part B: the release proof (RISC Zero 3.0.5)

Guest: `guests/src/bin/release.rs` reads the 1280-byte input, runs `verify_release`, commits the
352-byte journal; an invalid witness panics (`host execute`: "Guest panicked: release witness
rejected: RootMismatch" for a flipped `s_out` bit). The host checks every journal against the native
`release_journal` before saving anything. Image id `0x2ca5b06e8da21a4f51506ded03fa5bd89249080b30affe5350ee9da446fc4c41`.

### Execution (r0vm 3.0.5 executor, `artifacts/v1/logs/execute.log`)

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
   bytecode (no RPC access during this build).

### Part C: on-chain verification, measured

`evm-verify/`: 19 Foundry tests, all pass. `FerrataVault` is compiled unmodified from
the v1 vault's `src/` (commit `c417dc2` of `../vault/`) with the vault project's own OpenZeppelin and
compiler settings.

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

### Not done, weaker than specified, surprising

* **Proving times were all taken under heavy load** (another agent's r0vm job on 10–12 cores; load
  average 13–35). Three succinct runs of the same work took 43 s, 96 s and 109 s, which shows the
  contention; the end-of-session repeat was slower still. No idle-machine number exists. CPU time
  (558–622 s succinct, ~890 s identity_p254) is the steadier figure.
* **No device other than this laptop CPU was measured.** Phone figures below are extrapolations.
* **The LEZ burn-root proof is mocked**, and the burn tree is built by the host, not by a LEZ program.
* **The image id is not reproducible across machines as built.** The guest was built with the local
  rzup toolchain, not with RISC Zero's Docker guest builder (`cargo risczero build`), which the
  network allowance of this build did not cover. An immutable vault pins this id, so a production build must be
  reproducible.
* **`recover_burns` takes `chainId` and `vault`**, which the planned signature omits: they are needed to
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
  (`artifacts/v1/logs/mutants.log`). M04 survives and is equivalent: it writes a frontier branch at every
  0 bit instead of only the lowest. Every extra write is overwritten by the insertion that completes
  that subtree before any later index reads it, so roots are unchanged; in Solidity it would only cost
  gas. M02 was first skipped (its snippet predated rustfmt) and was re-run with the formatted snippet:
  killed.
* Not attempted: a LEZ-side claim program; deposit-side gas beyond the two incidental anvil deposits
  (the vault project measures those).

### Encodings: what I believe is wrong, risky or worth changing

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

### Can a phone or laptop make the succinct receipt, and leave only the Groth16 wrap to a third party?

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
