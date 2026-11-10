# Ferrata

Ferrata is one codebase, from a finalized Ethereum header to a wrapped token and back, written as the
evidence for a proposal to [Logos RFP-022](https://github.com/logos-co/rfp) (a privacy-preserving
wrapped ERC-20 and ETH bridge for LEZ) that also covers RFP-021 (trustless Ethereum state attestation
for LEZ). Its attestation primitive is a LEZ program that checks Ethereum sync-committee signatures
itself and keeps a write-once record of every finalized header it accepts, so that any LEZ program can
prove an account, a storage slot or an event log against such a record inside its own transaction. Its
bridge is an Ethereum vault that escrows ERC-20s and ETH in fixed denominations: LEZ mints the wrapped
token against a deposit verified through the primitive, and the vault releases against a LEZ burn
verified by a zero-knowledge proof on Ethereum. On the default path nothing public on either chain
links a deposit to its mint or a burn to its release, beyond token, denomination, time and the
residual correlations the design lists.

What is here are prototypes and their measurements. **Not audited. Not for production.**

## Components

Commands run from the repository root. Each directory's README gives the toolchains, the versions and
every other command.

| Component | What it is | Where it ran | Status | Reproduce |
|---|---|---|---|---|
| [Light-client core](crates/ferrata-core/README.md), `crates/` | `ferrata-core`: Ethereum sync-committee light client and execution-layer proofs (RLP, Merkle-Patricia, accounts, storage, receipts, logs, accumulator leaves), `no_std` Rust; `ferrata-spectests`: its vector runner and test suites | Laptop CPU, native tests; CI on every push | Built and tested: 52/52 official `sync` cases (Altair to Gloas), real mainnet and Sepolia data, Helios as a differential oracle, 107 planted mutants all killed | `scripts/fetch-vectors.sh && cargo test --workspace --locked` |
| [Guest benchmarks](bench/README.md), `bench/` | The core inside a RISC Zero zkVM 3.0.5 guest, with real mainnet data | RISC Zero executor on a laptop CPU (cycle counts, no proving) | Measured: a finality update is 10.2M cycles, in steps of 2.3M, 4.2M and 3.6M | `cd bench && cargo build --release -p host && ./target/release/host blst-accel` |
| [LEZ program](lez/RESULTS.md), `lez/` | The light client as a LEZ v0.4.0-rc1 program, with a public job path and a proven path, its store upgrading itself at a fork of its schedule; attestation of accounts, storage slots, event logs and accumulator leaves under its records, Gloas records included (with the block's RLP header as witness); a reference consumer with a public claim and a private attestation | In-process harness over LEZ's own state machine and a copy of its fee settlement, on a cloud container; RISC Zero dev mode, apart from the real proofs. One recorded run of the program as it was before the attestation (image `cec0bc68…`) on LEZ's own v0.4.0-rc1 sequencer, receipts verified, with a Logos Blockchain node (one validator, leader proofs in dev mode), on a rented Linux container. Real proofs: one private attestation (one NVIDIA L4), and the update path on earlier builds of the program (a laptop CPU, one NVIDIA L4, and the sequencer run's container CPU) | Prototype: `Init` and five real mainnet updates settled, every public transaction under the 5M target. On a mainnet block finalized on 10 November 2026: a storage slot attested in 274,842 cycles, an ERC-20 `Transfer` log in 96,584, a consumer's claim of that log in one transaction of 183,538, and the same log attested by a privately executed consumer in 380,577 user cycles with only the read of the record public, proven in 16.1 s on one NVIDIA L4 (a 227,170-byte receipt, settled with the receipt verified). On Sepolia (Gloas since October): from the bootstrap before the fork through it to period 1409 (166 transactions, the store upgrading itself), then under a Gloas-era record a storage slot in 278,253 cycles and an ERC-20 `Transfer` in 96,713, the same proofs refused as mainnet statements; the vault's deposit leaves proven in the guest at about 65,500 cycles each, on a synthetic state until the vault is deployed. On the sequencer, the 65 harness transactions of that earlier image had the same outcome, 64 at the same execution gas, and a real proof of a private finality update, made against the live instance on a CPU, was verified and applied | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads 1` (setup: `lez/RESULTS.md` §8); the sequencer run: `lez/live/README.md` |
| [Bridge program](lez/BRIDGE-RESULTS.md), `lez/` | The RFP-022 bridge program on LEZ v0.4.0-rc1 ([plan](lez/BRIDGE.md)): a configuration paired with its vault, `ImportRoot` through the attestation program's direct path, plaintext and private claims with write-once nullifiers, wrapped balances in the bridge's own accounts (no v0.4 token program is released), burns with write-once notes in the checkpoint's encoding | In-process harness over LEZ's own state machine and fee settlement, on a cloud container; a synthetic vault and records placed at genesis (the first deposits are the shared vectors'); private execution in RISC Zero dev mode in the tests; then one real proof each of a private claim and a private burn on one NVIDIA L4, settled with the receipts verified on the build box | Prototype: one token with three denominations; 15 transactions accepted and 30 refusals, each with its code (two private ones that LEZ's prover will not prove). Import 203,620 execution gas, plaintext claim 142,444, public burn 45,867; a private claim 439,815 (LEZ's fixed 409,764 + 30,051) with only `N`, the denomination and the root read public; the checkpoint's own parser collects the public and the private burn byte for byte, and the import since its decoder was fixed on this branch. Proven: the private claim in 15.3 s and the private burn in 12.4 s (227,477- and 226,978-byte receipts); one flipped bit in either receipt is refused | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --test bridge_inbound --test bridge_private --test bridge_burn`; the proofs: `lez/BRIDGE-RESULTS.md` §7 |
| [Inbound crossing, live](lez/live/inbound/README.md), `lez/live/inbound/` | Ethereum to LEZ on a running LEZ v0.4.0-rc1 sequencer and Logos Blockchain node: the attestation and bridge programs deployed through LEZ's loader, an instance following a beacon chain, the bridge paired with the vault, `ImportRoot`, a plaintext claim and a private claim with a real proof the sequencer verifies, and the refusals; the configuration's identity fixed in advance for the Sepolia vault | Modal build box (CPU), scripted and recorded; re-checked by replaying the recorded chain through LEZ's block application | Stand-in session done (its beacon chain and execution blocks made with keys we hold, not Ethereum): 57 blocks, 30 transactions as expected, the private claim proven in 254.8 s on CPU and included. The Sepolia session waits for the deposits | `cd lez && live/inbound/run.sh stand-in` on the build box |
| [GPU proving](gpu-prover/RESULTS.md), `gpu-prover/` | A small Modal app that proves the checkpoint fixtures, the LEZ update, one private attestation and the bridge program's private claim and burn with RISC Zero 3.0.5 on a GPU | One NVIDIA L4 in the cloud; receipts verified on the laptop (the bridge's on the build box) | Measured: 14–219 s per checkpoint proof (an hour of blocks with 50 private transactions in 219 s) and 49.5 s for the LEZ update, about 80 times faster than the laptop CPU; 16.1 s for the private attestation on the current images, 13.5 s on the previous ones; 15.3 s and 12.4 s for the bridge's private claim and burn (no CPU figure) | `gpu-prover/README.md` |
| [Vault](vault/README.md), `vault/` | `FerrataVault`, the immutable Ethereum vault (Solidity), at specification v2 with v3 D3–D5 and D9 | Local EVM (Foundry): its own suite with a mock verifier behind RISC Zero's unmodified emergency-stop wrapper; the return leg with RISC Zero's real Groth16 verifier and real proofs | Prototype: 286 tests with the mock verifier, 100% line and branch coverage, 127 of 127 planted bugs caught, 10 invariants; rule (A) only; 15 more tests run the return leg with real proofs | `cd vault && forge test` (install the pinned dependencies first: `vault/README.md`) |
| [Bridge core and release proof](bridge/README.md), `bridge/` | `ferrata-bridge-core` (commitments, trees, nullifiers, claim and release statements, recovery), the release and claim guests, and the release checked on Ethereum | Laptop CPU (RISC Zero 3.0.5 prover; Groth16 wrap in Docker on x86); local EVM with RISC Zero's real Groth16 verifier: the v2 vault with the checkpoint proof mocked, and the v3 vault with real checkpoint proofs (`e2e/`) | Prototype at specification v2: the release guest takes 80,501 cycles whatever the witness, proven in 37–49 s; an ETH release costs 367,988 gas on the v2 vault | `cd bridge && scripts/check.sh` (no proving; `bridge/scripts/prove-bench.sh` proves) |
| [Checkpoint program](checkpoint/README.md), `checkpoint/` | The LEZ to Ethereum checkpoint proof: re-hashes LEZ blocks, collects burns, follows key rotations, checks the support rule | RISC Zero executor on a laptop CPU, on LEZ blocks and Logos Blockchain inscriptions built with their own crates; two real proofs of a one-block checkpoint, wrapped to Groth16; a candidate and a finalisation proof of the end-to-end fixture, which the v3 vault accepted through RISC Zero's Groth16 verifier (`e2e/`); all of images from before this branch's `ImportRoot` fix, which changed both guests; no node ran | Prototype: specification v2 §4.1–4.2 with v3 D3, rule (A) only; cycles measured | `cd checkpoint && cargo +1.88 test -p checkpoint-core` |
| [Return leg, end to end](e2e/README.md), `e2e/` | One fixture in which the checkpoint program, the release program and the v3 vault meet: a stand-in zone with burns, its candidate and finalisation checkpoints, the releases against the checkpoint's burn root, and the Sepolia scripts in `vault/script/` | Fixture, release proofs and the first wrap step on a 16-core cloud machine; checkpoint proofs on one NVIDIA L4; RISC Zero's Groth16 prover image in the cloud; local EVM (Foundry) with RISC Zero's real verifier; the Sepolia stages on a local fork | Run: every proof accepted by the v3 vault with no mock on the Ethereum side, every negative case refused; the LEZ side is a stand-in (test keys, no node, no bridge program); proven with the checkpoint program before its `ImportRoot` fix, which refuses this fixture's import | `cd vault && forge test --match-path 'test/real/*'` (`e2e/README.md` for the fixture) |
| [Finality-rule analysis](docs/design/finality-rule/LEMMA.md), `docs/design/finality-rule/` | A model of LEZ v0.4 sequencer and Logos Blockchain behaviour cited to source lines, a written proof, and a discrete-event simulator | Simulator (Python, standard library), not a network | Broke the earlier rule (A); the repaired rule shows no violation in 500 adversarial runs within its conditions | `cd docs/design/finality-rule/sim && python3 experiments.py` (12,000 runs, 25 min on 14 processes; `python3 test_sim.py` checks the model) |

The guests need RISC Zero's toolchain (`rzup`, r0vm 3.0.5), the vault Foundry (forge 1.3.6, solc
0.8.26), and the Groth16 wraps Docker on x86-64. `lez/` and the checkpoint's fidelity tests read LEZ
v0.4.0-rc1 and Logos Blockchain from `upstream/` beside the repository. `scripts/fetch-upstream.sh` fetches both at the pinned commits. Scripts keep their scratch in
`tmp/` beside the repository, not in `/tmp`.

`vault/`, `bridge/`, `checkpoint/` and `docs/design/finality-rule/` were separate repositories,
imported with their history: their earlier commits keep their original paths.

## Design notes

- [docs/design/attestation.md](docs/design/attestation.md): the attestation primitive (RFP-021).
- [docs/design/bridge.md](docs/design/bridge.md): the bridge (RFP-022).
- [docs/design/lez-v0.4-findings.md](docs/design/lez-v0.4-findings.md): what the LEZ v0.4 and Logos
  Blockchain sources say about bridging to Ethereum, with line links.
- [docs/design/finality-rule/](docs/design/finality-rule/): [MODEL.md](docs/design/finality-rule/MODEL.md),
  [LEMMA.md](docs/design/finality-rule/LEMMA.md) and [RESULTS.md](docs/design/finality-rule/RESULTS.md),
  the analysis of finality rule (A).
- [docs/design/spec/](docs/design/spec/): the bridge specification,
  [v2](docs/design/spec/bridge-spec-v2.md) and [the v3 changes](docs/design/spec/bridge-spec-v3-changes.md),
  which the prototypes cite as SPEC-v2 and SPEC-v3.
- [docs/traceability.md](docs/traceability.md): every requirement of RFP-021 and RFP-022 with its design,
  the evidence that exists today and the milestone that would deliver it.
- [docs/REVIEW-core.md](docs/REVIEW-core.md): each function of the core against its specification
  section, with deviations and the tests that pin it.

## Specified but not built

The design notes specify these; there is no code for them here:
- in the bridge program on LEZ (`lez/BRIDGE-RESULTS.md` §8): fees and relay fees, caps, freezes, `Sunset`,
  the registry beyond one token and its attestation from the vault, transfers between holders, the switch
  to a v0.4 token program when one is released;
- rule (B): `ferrata_attestor`, the Logos module with which each sequencer would sign its node's final
  tier, and the rule (B) path in the checkpoint program and the vault (its cost is estimated only);
- the gas drop and the relayer; `monitor` and the watcher role;
- the Logos modules (`ferrata_attest_module`, `ferrata_bridge_module`), the CLI, the updater and relayer
  daemons, the IDL, the mini-app and the admin view;
- in the attestation program: successors and the root id (a fork not in the schedule fixed at
  `Init`); the weak-subjectivity bound, in the core too; the vault's deposit leaves on real data
  (the vault is not deployed yet: the capture command and the test are ready, `lez/RESULTS.md`
  §6); in the reference consumer, the nullifier keyed on a depositor's secret and with it a
  private claim (the bridge program has both), and the second reference consumer
  (`docs/design/attestation.md` §15, `lez/RESULTS.md` §5).

Changes in [the v3 specification](docs/design/spec/bridge-spec-v3-changes.md) that the prototypes do not
implement yet:
- vault (D11): the one-second boundary between revocation and refunds or releases; `finalize` also
  comparing `t` and the tip with the candidate and checking `importedLeafCount <= nextLeafIndex`;
  further logic in a linked library, since the runtime is 926 bytes under the EIP-170 limit;
- bridge core (D12): `mac` v2, which also covers `memo` (the bridge program's tests make their notes with
  it, from `lez/bridge-wire`); the witness-independence check of the guests' cycle counts runs in
  `bridge/scripts/check.sh`, not in CI;
- rule (B) as the proposed rule (D13): both finalisation paths in the vault behind an immutable `mode`,
  the v2 final-tier message, attestation-key certificates, a one-hour wait, watchers checking a
  candidate when it is submitted;
- rule (A) (D13): watchers cancelling a candidate whose key set is not the live one; Δ_g of 3 h when
  participation is partial; `submitCandidate` refusing a state already pending;
- roles (D13): up to three watchers that can only cancel candidates and pause checkpoints for 12 h; a
  multisig freeze authority that the admin replaces after a 14-day timelock; the emergency stop's owner
  renounced at deployment; `retire()` and `Sunset` open to anyone after 30 days without a finalised
  checkpoint; an immutable floor under the caps;
- inbound (D13): refunds drawing on the release cap; the gas-drop receipt binding
  `(N, oneTimeAddress, relayer)` (`ImportRoot` attesting slots 0 and 1, and claims refusing
  `leafIndex >= count`, are in the bridge program);
- outbound (D13): `NOut` v2 in the release guest (the bridge program's wire crate has it, wallet side
  only); releases against the newest root whose delay has passed (the write-once burn account keyed on
  the whole note is in the bridge program);
- checkpoint program: decoding the bridge program's `ImportRoot`, which carries its proofs after the
  four fields the checkpoint reads (`lez/BRIDGE-RESULTS.md` §5);
- fees (D13): a deposit escrows the denomination plus the mint and relay fees, and a burn releases the
  denomination less the burn, relay and checkpoint fees.

## Not audited. Not for production.

No part of this repository has been audited, and nothing is deployed. The prototypes exist to measure
and to test the design; the audit is planned for the code the proposal would deliver.

## How it was built

The code and the documents were drafted with coding agents (Claude Code) under the author's design,
review and acceptance tests, and every commit carries an `Assisted-by` trailer. The author owns the
design, the decisions and the acceptance tests: the specification and BLS vectors, the real-chain
replays, the differential oracles, the adversarial and mutation suites and the measurements each
directory documents. [docs/REVIEW-core.md](docs/REVIEW-core.md) maps each function of the core to its
specification section for the author's line-by-line review; its sign-off column is not filled in yet,
and the bridge prototypes have no such map. Agent reviews in a separate context were part of the work;
none of them is an audit.

## Licence

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)), for
every directory, the imported ones included.
