# Evidence index

Read this first. One row per piece of evidence: what it shows, its numbers, where it ran, whether
its proofs were real, how to re-check it, and the file with the details (every number below is
taken from that file, or from the suite run of this commit for the last section). Words used for
where it ran: **in-process harness** (LEZ v0.4.0-rc1's own state machine and a copy of its fee
settlement, no sequencer), **live rc1 sequencer** (LEZ's unmodified v0.4.0-rc1 sequencer with a
Logos Blockchain node), **local EVM** (Foundry), **fork of Sepolia** (anvil), **Sepolia** (read-only
calls), **one L4 GPU** (one NVIDIA L4 on Modal), **build box** (a Modal CPU container), **laptop
CPU**, **simulator**. **Stand-in** marks data we made ourselves where real data would go. Nothing
here is audited or deployed; no transaction was sent to Sepolia or any public network.

## Ethereum state on LEZ (RFP-021)

| # | What it shows | Numbers | Where it ran | Proofs | Re-check | Details |
|---|---|---|---|---|---|---|
| 1 | The light-client core passes the official sync-committee vectors and real chain data | 52 / 52 official `sync` cases, Altair to Gloas (185 steps); 33 of them step by step against Helios as a differential oracle; 107 planted mutants killed | laptop CPU, CI; re-run here on the build box (last section) | none (native) | `scripts/fetch-vectors.sh && cargo test --workspace --locked` | [`crates/ferrata-core/README.md`](../crates/ferrata-core/README.md) |
| 2 | What a finality update costs inside RISC Zero | 10.2M cycles, in steps of 2.3M, 4.2M and 3.6M | laptop CPU (RISC Zero executor) | none (cycle counts) | `cd bench && cargo build --release -p host && ./target/release/host blst-accel` | [`bench/RESULTS.md`](../bench/RESULTS.md) |
| 3 | The light client as a LEZ program on mainnet data: updates, then attestations other programs call | `Init` and five real mainnet updates settled; a storage slot attested in 274,842 cycles, an ERC-20 `Transfer` log in 96,584, a consumer's claim of that log in one transaction of 183,538 | in-process harness, build box | dev mode | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked` (setup: `lez/RESULTS.md` §8) | [`lez/RESULTS.md`](../lez/RESULTS.md) §2–§5 |
| 4 | The same through Sepolia's Gloas fork, the store upgrading itself, attestation under Gloas records | 166 transactions from before the fork to period 1409; under a Gloas record a storage slot in 278,253 cycles, a `Transfer` in 96,713; the vault's deposit leaves at about 65,500 cycles each (on a **stand-in** state: the vault is not deployed) | in-process harness, real Sepolia data | dev mode | as row 3 | [`lez/RESULTS.md`](../lez/RESULTS.md) §6 |
| 5 | A private attestation: a privately executed consumer reads a record, only the read is public | 380,577 user cycles; proven in 16.1 s, a 227,170-byte receipt, settled with the receipt verified | one L4 GPU; settlement in the in-process harness | **real** | `gpu-prover/README.md`, job `lez-attest` | [`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md), [`lez/RESULTS.md`](../lez/RESULTS.md) §5 |
| 6 | The program on a running sequencer | the 65 harness transactions of image `cec0bc68…` (before the attestation existed): same outcome, 64 at the same execution gas; a real proof of a private finality update, made on a CPU against the live instance, verified and applied | live rc1 sequencer, one validator (leader proofs in dev mode), rented Linux container | **real** (one update), the rest public | `lez/live/README.md` | [`lez/live/README.md`](../lez/live/README.md), [`lez/RESULTS.md`](../lez/RESULTS.md) §9 |
| 7 | The attestation as a Logos module | `ferrata_attest_module` built (dev and portable `.lgx`), loaded under a headless Logos Core: 11 calls, each equal to the native call | build box (Nix, Logos from source) | none | `modules/scripts/check.sh` (Linux x86-64 with Nix, about 47 min from a fresh container) | [`modules/RESULTS.md`](../modules/RESULTS.md) |

## The bridge (RFP-022)

| # | What it shows | Numbers | Where it ran | Proofs | Re-check | Details |
|---|---|---|---|---|---|---|
| 8 | The bridge program: pairing, registry, `ImportRoot`, claims, burns, fees by leaf index, caps, freezes, `Sunset`, every refusal with its code | `Init` 168,607 execution gas, `ImportRoot` (8 leaves) 231,558, `Register` 324,748; a plaintext claim 239,431 (1 ETH, relay fee) or 260,579 (10 ETH, mint and relay fees); a private claim 539,060 (LEZ's fixed 409,764 + 129,296) with only `N`, the denomination, the root read and the fee range public; burns 130,868 (public) and 513,593 (private); 72 transactions for the controls; under a Gloas record (a real Sepolia header with a **stand-in** state root) `Init` 204,559 and `Register` 360,279 | in-process harness, build box; **stand-in** vault storage and records | dev mode | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --test bridge_inbound --test bridge_private --test bridge_burn --test bridge_controls` | [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §1–§8 |
| 9 | The bridge program reads the storage the vault itself wrote: registry entries, fee schedules, root and count | 23 transactions, 20 applied: five tokens registered from FerrataVault v5's own storage (424,496–448,038 execution gas), its root imported twice, ETH's schedule 1 imported, six deposits claimed with the vault's fees; 3 refused (247) | local EVM (the vault), then in-process harness, build box; `Init` under a **stand-in** block | dev mode | `cd vault && forge test --match-contract RegistryStorageTest`, then `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --test bridge_vault_storage` | [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §8a |
| 10 | A private claim and a private burn proven | 15.3 s and 12.4 s; 227,477- and 226,978-byte receipts, settled with the receipts verified; one flipped bit in either refused | one L4 GPU; settlement on the build box | **real**, of the earlier program `25dfd9b2…` (before the registry, fees, caps and freezes) | `lez/BRIDGE-RESULTS.md` §7 (`prove_bridge`, job `lez-bridge`) | [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §7 |
| 11 | Ethereum to LEZ on a running sequencer: deployment by LEZ's loader, pairing, `ImportRoot`, claims | 57 blocks, 30 transactions as expected (26 applied, 4 refused); a private claim proven in 254.8 s (252,185 bytes) and included; re-checked by replaying the chain through LEZ's own block application. **Stand-in** beacon chain and execution blocks (keys we hold) and vault | live rc1 sequencer on the build box | **real** (the private claim, CPU) | `cd lez && live/inbound/run.sh stand-in` on the build box | [`lez/live/inbound/README.md`](../lez/live/inbound/README.md) |
| 12 | The vault's suite: deposits, checkpoints under both finality rules, releases, refunds, fees, roles, caps, freezes | 396 tests in 31 suites (14 invariants on a rule (A) and on a rule (B) vault; 21 tests with real proofs); 100% of the lines, statements, branches and functions of the vault (426 lines, 102 branches) and its library (227, 41); 223 of 223 planted bugs killed; runtime 24,242 bytes, 334 under EIP-170 | local EVM, build box | mock verifier behind RISC Zero's emergency stop; **real** Groth16 verifier and real seals in `test/real/` | `cd vault && forge test && forge coverage --report summary` (dependencies: `vault/README.md`) | [`vault/README.md`](../vault/README.md), [`evidence-suites.txt`](evidence-suites.txt) |
| 13 | What the vault costs on Ethereum | a checkpoint under rule (A) 974,346 gas, under rule (B) 542,969 (real verifier, real seals); real releases 373,657 and 403,523; `depositETH` 179,275 first, 159,285 later | local EVM (`forge --isolate`) | **real** seals | `vault/GAS.md` | [`vault/GAS.md`](../vault/GAS.md) |
| 14 | The deployment script, rehearsed | complete under both rules on a fork of Sepolia block 12,107,296 (rule (B) behind an accept-all test verifier); refunds after 86,401 s returned both escrows whole; creation gas of every contract estimated on Sepolia (the vault fits under EIP-8037) | fork of Sepolia (build box); Sepolia, read-only `eth_estimateGas` from the laptop | none submitted | `vault/script/fork-rehearsal.sh` (`vault/README.md`, "Deployment") | [`vault/README.md`](../vault/README.md) |
| 15 | The checkpoint program, rule (A): the vault's view of LEZ | executor: an idle hour 5,993,418 cycles, an hour with 50 private transactions 37.9M; a one-block checkpoint proven in 1,157.5 s and 1,758.9 s, wrapped to Groth16 in 195.1–215.2 s | laptop CPU | **real**, of the images before the `ImportRoot` fix of `integration` | `cd checkpoint && cargo +1.88 test -p checkpoint-core -p attestor-core` | [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md) |
| 16 | Rule (B): `t` sequencer attestations instead of a beacon wait | 1.69M cycles per attestor (5.1M at t = 3, 11.8M at t = 7); one proof of an idle hour (n = 4) in 31.87 s, receipt verified, then accepted by a rule (B) vault through RISC Zero's real verifier | build box (executor); one L4 GPU; local EVM | **real**, of `checkpoint_b` `0xd3597543…` (before the `ImportRoot` fix) | `checkpoint/RESULTS.md` §4 | [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md) §4, [`gpu-prover/results/ruleb-l4b/`](../gpu-prover/results/ruleb-l4b/) |
| 17 | Proving time on a GPU | 14.0–219.3 s per checkpoint proof (an hour with 50 private transactions in 219.3 s); 49.5 s for the LEZ update | one L4 GPU; receipts verified on the laptop | **real**, earlier images | `gpu-prover/README.md` | [`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md) |
| 18 | The return leg end to end: burn on a **stand-in** zone, checkpoint and release proofs, the v3 vault pays | every proof accepted with no mock on the Ethereum side, every negative case refused; the LEZ side is a stand-in (test keys, no node, no bridge program) | 16-core cloud machine and one L4 GPU (proofs); local EVM with the real verifier; fork of Sepolia | **real**, with the checkpoint before its `ImportRoot` fix (the fixed program refuses this fixture's import) | `cd vault && forge test --match-path 'test/real/*'` | [`e2e/README.md`](../e2e/README.md) |
| 19 | The release statement and the v2 vault | the release guest takes 80,501 cycles whatever the witness, proven in 37.1–48.6 s; an ETH release costs 367,988 gas (checkpoint mocked) | laptop CPU; local EVM with the real verifier | **real** | `cd bridge && scripts/check.sh` | [`bridge/RESULTS.md`](../bridge/RESULTS.md) |
| 20 | The finality rule (A) analysis | broke the earlier rule; the repaired rule shows no violation in 500 adversarial runs within its conditions (12,000 runs in all) | simulator (Python), not a network | none | `cd docs/design/finality-rule/sim && python3 experiments.py` | [`docs/design/finality-rule/RESULTS.md`](design/finality-rule/RESULTS.md) |

## What the evidence does not show yet

- **No crossing with Ethereum itself.** The vault is not deployed on Sepolia; the inbound crossing
  on a live sequencer (row 11) ran on a stand-in beacon chain and a stand-in vault. The Sepolia
  session waits for the owner's deposits into a new vault, and will deploy the programs frozen for it
  (bridge image `66722fc7…f4fa`), not the bridge program of this tree.
- **The bridge program of this tree** (registry, fees, caps, freezes, `Sunset`, the Gloas header;
  image `f11b1aba…58eb` at the integration worktree's path) has run only in the in-process harness,
  in dev mode: no real proof of it (row 10's are of the earlier `25dfd9b2…`) and no sequencer.
- **The return leg and every checkpoint proof are of images from before the `ImportRoot` fix**
  (rows 15–18): the fixed checkpoint refuses the return leg's import, and re-making that fixture and
  its proofs is not done. The images after the merge of rule (B) (`checkpoint/RESULTS.md`) have no
  proof or bench.
- **Shallow state.** The bridge's imports and registrations ran under synthetic state tries of about
  40 accounts (row 9: the vault's own storage, under a synthetic state trie); a mainnet-depth
  attestation was measured for the attestation program (row 3: 274,842 cycles for one slot).
- **On the light-client side**, the program's only sequencer run (row 6) is of an image from before
  the attestation existed.
- **Not measured**: proving on the declared user minimum (4 cores, macOS); the vault's
  registrations, deposits and releases under Sepolia's pricing.
- **Not built** (README, "Specified but not built"): among them `ferrata_attestor`, the bridge Logos
  module, the gas drop, watchers' software, a permissionless `Sunset` on LEZ. Nothing is audited.

## Suites at this commit

Re-run in the second integration pass (10 Nov 2026) on the build box, code as of commit d134a3a;
every count and command is in [`evidence-suites.txt`](evidence-suites.txt).

| Suite | Result |
|---|---|
| Root workspace, CI job as written (vectors fetched on the box; `stable` as 1.98.1) | 142 tests passed in 21 binaries; fmt, clippy `-D warnings` and the `no_std` RISC-V build clean |
| LEZ harness, every test (private paths in dev mode) | 35 passed in 20 binaries; clippy and fmt clean; `prove_bridge`'s dry run settles its claim and burn |
| Checkpoint: `checkpoint-core` and `attestor-core` (rules (A) and (B)); fidelity | 33 passed in 8 binaries, clippy and fmt clean, attestor vectors re-derived in Python; fidelity 17 passed in 6 |
| Vault: `forge test`, coverage, mutation | 396 passed in 31 suites; coverage 100% (lines, statements, branches, functions) of both files; 223/223 mutants killed |
| Link checker (`scripts/check-links.py`, laptop) | 13 files, 278 links, 0 broken (with this page staged) |

Not re-run here: `bridge/scripts/check.sh` (`bridge/` is unchanged since the first pass, where every
step but the last passed; the last fails off the original build path), `modules/scripts/check.sh`
(about 47 min, Nix from source), `bench/`, the finality simulator, and any proof.
