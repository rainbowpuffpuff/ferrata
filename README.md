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
| [LEZ program](lez/RESULTS.md), `lez/` | The light client as a LEZ v0.4.0-rc1 program, with a public job path and a proven path, and a private consumer | In-process harness over LEZ's own state machine and a copy of its fee settlement, no sequencer; RISC Zero dev mode, and one real proof on a laptop CPU | Prototype: `Init` and five real mainnet updates settled, every public transaction under the 5M target; one real proof of the proven path, made on an earlier build of the program | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads 1` (setup: `lez/RESULTS.md` §6) |
| [Vault](vault/README.md), `vault/` | `FerrataVault`, the immutable Ethereum vault (Solidity), at specification v2 with v3 D3–D5 and D9 | Local EVM (Foundry), a mock verifier behind RISC Zero's unmodified emergency-stop wrapper | Prototype: 286 tests, 100% line and branch coverage, 127 of 127 planted bugs caught, 10 invariants; rule (A) only | `cd vault && forge test` (install the pinned dependencies first: `vault/README.md`) |
| [Bridge core and release proof](bridge/README.md), `bridge/` | `ferrata-bridge-core` (commitments, trees, nullifiers, claim and release statements, recovery), the release and claim guests, and the release checked on Ethereum | Laptop CPU (RISC Zero 3.0.5 prover; Groth16 wrap in Docker on x86); local EVM with RISC Zero's real Groth16 verifier and the v2 vault, the checkpoint proof mocked | Prototype at specification v2: the release guest takes 80,501 cycles whatever the witness, proven in 37–49 s; an ETH release costs 367,988 gas | `cd bridge && scripts/check.sh` (no proving; `bridge/scripts/prove-bench.sh` proves) |
| [Checkpoint program](checkpoint/README.md), `checkpoint/` | The LEZ to Ethereum checkpoint proof: re-hashes LEZ blocks, collects burns, follows key rotations, checks the support rule | RISC Zero executor on a laptop CPU, on LEZ blocks and Logos Blockchain inscriptions built with their own crates; two real proofs of a one-block checkpoint, wrapped to Groth16; no node ran and no contract saw the seal | Prototype: specification v2 §4.1–4.2 with v3 D3, rule (A) only; cycles measured | `cd checkpoint && cargo +1.88 test -p checkpoint-core` |
| [Finality-rule analysis](docs/design/finality-rule/LEMMA.md), `docs/design/finality-rule/` | A model of LEZ v0.4 sequencer and Logos Blockchain behaviour cited to source lines, a written proof, and a discrete-event simulator | Simulator (Python, standard library), not a network | Broke the earlier rule (A); the repaired rule shows no violation in 500 adversarial runs within its conditions | `cd docs/design/finality-rule/sim && python3 experiments.py` (12,000 runs, 25 min on 14 processes; `python3 test_sim.py` checks the model) |

The guests need RISC Zero's toolchain (`rzup`, r0vm 3.0.5), the vault Foundry (forge 1.3.6, solc
0.8.26), and the Groth16 wraps Docker on x86-64. `lez/` and the checkpoint's fidelity tests read LEZ
v0.4.0-rc1 and Logos Blockchain from `upstream/` beside the repository. Scripts keep their scratch in
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
- the bridge program on LEZ: configurations, the registry, `ImportRoot`, claims and nullifiers, burns
  and burn notes, fees, caps, freezes, `Sunset`;
- rule (B): `ferrata_attestor`, the Logos module with which each sequencer would sign its node's final
  tier, and the rule (B) path in the checkpoint program and the vault (its cost is estimated only);
- the gas drop and the relayer; `monitor` and the watcher role;
- the Logos modules (`ferrata_attest_module`, `ferrata_bridge_module`), the CLI, the updater and relayer
  daemons, the IDL, the mini-app and the admin view;
- in the attestation program: attestation itself (inclusion, predicates, identifiers, accumulator
  leaves, history hops) and successors; the weak-subjectivity bound, in the core too
  (`docs/design/attestation.md` §15).

Changes in [the v3 specification](docs/design/spec/bridge-spec-v3-changes.md) that the prototypes do not
implement yet:
- vault (D11): the one-second boundary between revocation and refunds or releases; `finalize` also
  comparing `t` and the tip with the candidate and checking `importedLeafCount <= nextLeafIndex`;
  further logic in a linked library, since the runtime is 926 bytes under the EIP-170 limit;
- bridge core (D12): `mac` v2, which also covers `memo`; the witness-independence check of the guests'
  cycle counts runs in `bridge/scripts/check.sh`, not in CI;
- rule (B) as the proposed rule (D13): both finalisation paths in the vault behind an immutable `mode`,
  the v2 final-tier message, attestation-key certificates, a one-hour wait, watchers checking a
  candidate when it is submitted;
- rule (A) (D13): watchers cancelling a candidate whose key set is not the live one; Δ_g of 3 h when
  participation is partial; `submitCandidate` refusing a state already pending;
- roles (D13): up to three watchers that can only cancel candidates and pause checkpoints for 12 h; a
  multisig freeze authority that the admin replaces after a 14-day timelock; the emergency stop's owner
  renounced at deployment; `retire()` and `Sunset` open to anyone after 30 days without a finalised
  checkpoint; an immutable floor under the caps;
- inbound (D13): `ImportRoot` attesting slots 0 and 1 and claims refusing `leafIndex >= count`; refunds
  drawing on the release cap; the gas-drop receipt binding `(N, oneTimeAddress, relayer)`;
- outbound (D13): `NOut` v2, independent of `burnIndex`; the write-once burn account keyed on the whole
  note; releases against the newest root whose delay has passed;
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
