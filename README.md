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

**Start with [docs/EVIDENCE.md](docs/EVIDENCE.md)**: one page with every piece of evidence, its
numbers, where it ran, whether its proofs were real or in dev mode, what is stand-in data, and the
command that re-checks it.

## Components

Commands run from the repository root. Each directory's README gives the toolchains, the versions and
every other command.

| Component | What it is | Where it ran | Status | Reproduce |
|---|---|---|---|---|
| [Light-client core](crates/ferrata-core/README.md), `crates/` | `ferrata-core`: Ethereum sync-committee light client and execution-layer proofs (RLP, Merkle-Patricia, accounts, storage, receipts, logs, accumulator leaves), `no_std` Rust; `ferrata-spectests`: its vector runner and test suites | Laptop CPU, native tests; CI on every push | Built and tested: 52/52 official `sync` cases (Altair to Gloas), real mainnet and Sepolia data, Helios as a differential oracle, 107 planted mutants all killed and the 24 of an independent review; fuzzed, seven targets, 1.79 billion executions | `scripts/fetch-vectors.sh && cargo test --workspace --locked` |
| [Guest benchmarks](bench/README.md), `bench/` | The core inside a RISC Zero zkVM 3.0.5 guest, with real mainnet data | RISC Zero executor on a laptop CPU (cycle counts, no proving) | Measured: a finality update is 10.2M cycles, in steps of 2.3M, 4.2M and 3.6M | `cd bench && cargo build --release -p host && ./target/release/host blst-accel` |
| [LEZ program](lez/RESULTS.md), `lez/` | The light client as a LEZ v0.4.0-rc1 program (rebuilt against v0.4.0-rc2 unchanged: [`lez/RESULTS.md` §10](lez/RESULTS.md#10-lez-v040-rc2)), with a public job path and a proven path, its store upgrading itself at a fork of its schedule; attestation of accounts, storage slots, event logs and accumulator leaves under its records, Gloas records included (with the block's RLP header as witness); a reference consumer with a public claim and a private attestation | In-process harness over LEZ's own state machine and a copy of its fee settlement, on a cloud container; RISC Zero dev mode, apart from the real proofs. One recorded run of the program as it was before the attestation (image `cec0bc68…`) on LEZ's own v0.4.0-rc1 sequencer, receipts verified, with a Logos Blockchain node (one validator, leader proofs in dev mode), on a rented Linux container; one of the current image on LEZ's v0.4.0-rc2 sequencer, without a real proof; and, since 10 November, two instances (Sepolia and mainnet) of this source kept following their chains on a LEZ v0.4.0-rc1 zone on Modal, a session every two hours ([`lez/lab/`](lez/lab/README.md)). Real proofs: one private attestation (one NVIDIA L4; and on cloud CPUs held to 8 and 16 CPUs with 16 GiB, 904.3 s and 497.3 s: [`lez/results/p4-minimum`](lez/results/p4-minimum/README.md)), and the update path on earlier builds of the program (a laptop CPU, one NVIDIA L4, and the sequencer run's container CPU) | Prototype: `Init` and five real mainnet updates settled, every public transaction under the 5M target. On a mainnet block finalized on 10 November 2026: a storage slot attested in 274,842 cycles, an ERC-20 `Transfer` log in 96,584, a consumer's claim of that log in one transaction of 183,538, and the same log attested by a privately executed consumer in 380,577 user cycles with only the read of the record public, proven in 16.1 s on one NVIDIA L4 (a 227,170-byte receipt, settled with the receipt verified). On Sepolia (Gloas since October): from the bootstrap before the fork through it to period 1409 (166 transactions, the store upgrading itself), then under a Gloas-era record a storage slot in 278,253 cycles and an ERC-20 `Transfer` in 96,713, the same proofs refused as mainnet statements; the vault's deposit leaves proven in the guest at about 65,500 cycles each, on a synthetic state until the vault is deployed. On the sequencer, the 65 harness transactions of that earlier image had the same outcome, 64 at the same execution gas, and a real proof of a private finality update, made against the live instance on a CPU, was verified and applied. On v0.4.0-rc2: every harness table row the same as on rc1; on its sequencer the same outcomes and charges, deployment now paying execution gas and blocks final after 14–25 s | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads 1` (setup: `lez/RESULTS.md` §8; rc2: §10); the sequencer runs: `lez/live/README.md` |
| [Bridge program](lez/BRIDGE-RESULTS.md), `lez/` | The RFP-022 bridge program on LEZ v0.4.0-rc1 ([plan](lez/BRIDGE.md); on v0.4.0-rc2 the same figures, `lez/RESULTS.md` §10): a configuration paired with its vault, tokens registered by attesting the vault's registry, `ImportRoot` through the attestation program's direct path, plaintext and private claims with write-once nullifiers, fees by leaf index, token and global caps and freezes on attested Ethereum time (no caller time), roles and `Sunset`, wrapped balances in the bridge's own accounts (a `holder(account)` PDA per owner; no v0.4 token program is released), burns with write-once notes in the checkpoint's encoding | In-process harness over LEZ's own state machine and fee settlement, on a cloud container; a synthetic vault and records placed at genesis (the first deposits are the shared vectors'), and in one test blocks holding exactly the storage FerrataVault v5 wrote in a local EVM; private execution in RISC Zero dev mode in the tests; then one real proof each of a private claim and a private burn on one NVIDIA L4, of the program before its fees, caps and freezes, settled with the receipts verified on the build box; the merged program's private claim and burn proven for real on a LEZ devnet in the crown rehearsal ([EVIDENCE](docs/EVIDENCE.md) row 31) and on cloud CPUs held to 8 and 16 CPUs with 16 GiB (claim 905.4 s and 463.7 s, burn 756.3 s and 389.0 s; row 33) | Prototype: 78 transactions for the registry (two tokens in a configuration), fees by leaf index, caps, freezes and `Sunset`, besides the inbound, private and burn tests, every refusal with its code. On the merged program (time fix and security fixes, `lez/results/bridge-*.md`): import 231,365 execution gas, plaintext claim 258,885 (142,444 before fees, caps and freezes), public burn 164,153 (45,867 before); a private claim 551,502 (LEZ's fixed 409,764 + 141,738) with only `N`, the denomination, the root read and the fee schedule's range public; the checkpoint's own parser collects the public and the private burn byte for byte, the import, and the freeze authority's `Sunset` once. On the storage the vault itself wrote: five tokens registered, its root imported and six deposits claimed with its fees (23 transactions, 3 refusals). Under a Gloas-era record (a real Sepolia header): the pairing, a registration, the import and a claim. Proven, of the earlier program: the private claim in 15.3 s and the private burn in 12.4 s on one L4 (227,477- and 226,978-byte receipts); one flipped bit in either receipt is refused | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --test bridge_inbound --test bridge_private --test bridge_burn --test bridge_controls --test bridge_vault_storage`; the proofs: `lez/BRIDGE-RESULTS.md` §7 |
| [Inbound crossing, live](lez/live/inbound/README.md), `lez/live/inbound/` | Ethereum to LEZ on a running LEZ v0.4.0-rc1 sequencer and Logos Blockchain node: the attestation and bridge programs deployed through LEZ's loader, an instance following a beacon chain, the bridge paired with the vault, `ImportRoot`, a plaintext claim and a private claim with a real proof the sequencer verifies, and the refusals; the configuration's identity fixed in advance for the Sepolia vault, so the crossing deploys the programs frozen then (bridge image `66722fc7…`), not the tree's later bridge program | Modal build box (CPU), scripted and recorded; re-checked by replaying the recorded chain through LEZ's block application | Stand-in session done (its beacon chain and execution blocks made with keys we hold, not Ethereum): 57 blocks, 30 transactions as expected, the private claim proven in 254.8 s on CPU and included. On public Sepolia (11 Nov 2026): deposit 0 into the live vault `0xd0B5…8AF7` claimed on LEZ from capture 1 (instance caught up from period 1378 through Gloas to finalized slot 11551744; Init on the vault's `configId`; `ImportRoot` of root(1); the plaintext claim; refusals 215, 248, 249, 250): 186 blocks, 185 transactions, replay check passes. Deposit 1 (private, real proof) waits for capture 2 | `cd lez && live/inbound/run.sh stand-in`; `live/inbound/run.sh real CAPTURE_1 [CAPTURE_2]` on the build box |
| [GPU proving](gpu-prover/RESULTS.md), `gpu-prover/` | A small Modal app that proves the checkpoint fixtures, the LEZ update, one private attestation, the bridge program's private claim and burn, and the rule (B) checkpoint over real LEZ blocks with RISC Zero 3.0.5 on a GPU | One NVIDIA L4 in the cloud; receipts verified on the laptop (the bridge's on the build box) | Measured: 14–219 s per checkpoint proof (an hour of blocks with 50 private transactions in 219 s) and 49.5 s for the LEZ update, about 80 times faster than the laptop CPU; 16.1 s for the private attestation on the current images, 13.5 s on the previous ones; 15.3 s and 12.4 s for the bridge's private claim and burn, 39.5 s for the rule (B) checkpoint over 195 real LEZ blocks (no CPU figure) | `gpu-prover/README.md` |
| [Logos module](modules/RESULTS.md), `modules/` | `ferrata_attest_module`, a prototype Logos module in Rust over `ferrata-core`: `version`, `dry_run_attest` (a storage slot or log proven against a header record, with the program's verdict and code) and `verify_checkpoint` (instance id, the `Init` checks on the bootstrap, roots given in the call); no network, no transactions | Built with logos-module-builder and run under a headless Logos Core (`logosctl`) on a cloud container (Modal) | Prototype: built as a dev and a portable `.lgx`, loaded, 11 calls through Logos Core each equal to the native call; the instance id equals the program's for its mainnet instance | `modules/scripts/check.sh` (Linux x86-64 with Nix) |
| [Vault](vault/README.md), `vault/` | `FerrataVault` v5, the immutable Ethereum vault (Solidity), and its linked library: specification v2 with v3 D3–D5, D9 and the vault items of D11 and D13 (rule (B) beside rule (A) behind an immutable `mode`, watchers, the 14-day role change, keyless retirement, the cap floor, and D13's fees: mint, relay, burn and checkpoint fees under a 1% ceiling, changed seven days after announcement, published for LEZ); a deployment script that takes every gas limit from the node's estimate | Local EVM (Foundry): its own suite with a mock verifier behind RISC Zero's unmodified emergency-stop wrapper; the return leg with RISC Zero's real Groth16 verifier and real proofs; the deployment rehearsed on a local anvil fork of Sepolia, creation gas measured read-only on Sepolia | Prototype, with the security fixes of 11 November: 433 tests (43 with real proofs), 100% line and branch coverage of the vault and its library, 254 of 254 planted bugs caught, 14 invariants (balance equality, supply) on a rule (A) and on a rule (B) vault; real proofs of both rules verified (`test/real/`), among them a rule (B) checkpoint over 195 real LEZ sequencer blocks (`RealBlocks.t.sol`): a rule (B) checkpoint costs 542,969 gas against 974,346 for rule (A) (the security fixes add about 28.0k to each, `vault/GAS.md`); on today's Sepolia (EIP-8037) the vault's creation costs 39.5 M gas and fits | `cd vault && forge test` (install the pinned dependencies first: `vault/README.md`) |
| [Bridge core and release proof](bridge/README.md), `bridge/` | `ferrata-bridge-core` (commitments, trees, nullifiers, claim and release statements, recovery), the release and claim guests, and the release checked on Ethereum | Laptop CPU (RISC Zero 3.0.5 prover; Groth16 wrap in Docker on x86); local EVM with RISC Zero's real Groth16 verifier: the v2 vault with the checkpoint proof mocked, and the v3 vault with real checkpoint proofs (`e2e/`) | Prototype at specification v2, with v3's `NOut` v2 and `mac` v2 (review M-3, I-4): the release guest takes 77,736 cycles whatever the witness (executor; the earlier image, 80,501 cycles, was proven in 37–49 s on the laptop; the current one, the holder's proof, in 46.0 s at 8 CPUs and 26.4–27.0 s at 16, and wrapped, any relayer's step, in 55–93 s: [EVIDENCE](docs/EVIDENCE.md) row 33); an ETH release costs 367,988 gas on the v2 vault | `cd bridge && scripts/check.sh` (no proving; `bridge/scripts/prove-bench.sh` proves) |
| [Checkpoint program](checkpoint/README.md), `checkpoint/` | The LEZ to Ethereum checkpoint proof: re-hashes LEZ blocks, collects burns, follows key rotations, checks the support rule (A) or the final-tier attestations (B); `crates/attestor-core`, the attestor's certificate and message | RISC Zero executor on a laptop CPU and, for rule (B), on the Modal build box, on LEZ blocks and Logos Blockchain inscriptions built with their own crates; two real proofs of a one-block checkpoint, wrapped to Groth16; a candidate and a finalisation proof of the end-to-end fixture, which the v3 vault accepted through RISC Zero's Groth16 verifier (`e2e/`); one real rule (B) proof on one NVIDIA L4, wrapped to Groth16 and verified by a rule (B) vault in a local EVM; **real sequencer blocks**: the 202 blocks and inscriptions of the lab's LEZ v0.4.0-rc1 chain (a devnet with one validator and leader proofs in dev mode, n = 1) checked on the build box, blocks 8..=202 proven under rule (B) on one NVIDIA L4 and accepted by the v5 vault with RISC Zero's real verifier (local EVM); every one of these proofs is of an image from before the `ImportRoot` fix of the checkpoint's decoder (12d17e3), which changed the guests (the real-block proof's `checkpoint_b` is `0xd3597543…`) | Prototype: specification v2 §4.1–4.2 with v3 D3; rule (A), and rule (B) of v3 D7 and D13 in its own guest; cycles of both measured on the same blocks. On real blocks: the parser and block hash accept all 202 blocks and inscriptions as the sequencer made them; rule (B) over 195 of them (585 transactions, n = 1, t = 1) in 7.66M cycles, proven in 39.5 s, 347,832 + 195,149 gas ([RESULTS](checkpoint/RESULTS.md), "Real LEZ blocks") | `cd checkpoint && cargo +1.88 test -p checkpoint-core -p attestor-core` |
| [Return leg, end to end](e2e/README.md), `e2e/` | One fixture in which the checkpoint program, the release program and the v3 vault meet: a stand-in zone with burns, its candidate and finalisation checkpoints, the releases against the checkpoint's burn root, and the Sepolia scripts in `vault/script/` | Fixture, release proofs and the first wrap step on a 16-core cloud machine; checkpoint proofs on one NVIDIA L4; RISC Zero's Groth16 prover image in the cloud; local EVM (Foundry) with RISC Zero's real verifier; the Sepolia round trip rehearsed in the cloud on forks of Sepolia, every stage priced by Sepolia's node; on Sepolia, the first attempt's one transaction | Run: every proof accepted by the v3 vault with no mock on the Ethereum side, every negative case refused; the LEZ side is a stand-in (test keys, no node, no bridge program); proven with the checkpoint program before its `ImportRoot` fix, which refuses this fixture's import; a rule (B) checkpoint over real sequencer blocks, without burns, is in [checkpoint/RESULTS.md](checkpoint/RESULTS.md), "Real LEZ blocks". On Sepolia the first attempt ran out of gas at its first transaction (Foundry does not price EIP-8037's state gas; the round trip costs 5.6 times Foundry's figure there, `vault/GAS.md`); the re-planned run takes every limit from Sepolia's estimate and passed its rehearsals; the owner's run is under way | `cd vault && forge test --match-path 'test/real/*'` (`e2e/README.md` for the fixture) |
| [Command-line tool](cli/README.md), `cli/` | `ferrata`, the user's and relayer's tool over the bridge core, the bridge wire, LEZ v0.4.0-rc1's own wallet library and alloy: `keys` (LEZ accounts, `kAuth` and `kView` from the wallet's BIP-39 seed), `deposit` (the vault call with its commitment, printed, or sent from a keystore file only with `--send --yes`), `scan` and `recover` (the whole deposit list from any RPC, checked against `depositRoot`; burns from the zone's blocks; no per-item query), `claim` (private by default, plaintext only with its own confirmation), `burn`, `release` (the release statement natively, the guest in `r0vm`, the seal from a prover) and `status` (registry, fees and caps in the token's units, freezes, the configuration and its instance's newest record); every refusal named with the bridge program's exit code or the vault's error | Modal build box (CPU): a local anvil (chain id 1) with the v5 vault and the vault tests' mock verifier; the zone in the LEZ harness with the bridge guest in RISC Zero dev mode | Prototype: deposits, `scan` and `status` through the binary; the claim, burns, release and recovery through the library functions the binary calls, with the harness in place of LEZ's sequencer client; a wiped client with only the seed file finds 10 of 10 unclaimed deposits and 10 of 10 unreleased burns (16 deposits, 11 burns); refusals 250, 254 and `WrongDepositIndex` named before sending; the release paid 0.1 ETH once and was refused the second time. No real proof in this test; not run against a sequencer or a public network; the private claim is built but not tested | `cd cli && bash tests/e2e.sh` (Foundry and RISC Zero's toolchains: `cli/README.md`) |
| [Privacy suite](docs/privacy/RESULTS.md), `scripts/privacy/` | Milestone B2's privacy gate measured: 20 users' round trip (deposit, private claim, private burn, release) through the bridge program and the vault; every public byte collected on both chains; a differential gate, an adversary that tries to pair each claim with its deposit and each release with its burn, the field table against the proposal's §B6, and seed-only recovery | Cloud container (Modal, 2 vCPU): the LEZ harness in RISC Zero dev mode with synthetic records, Foundry with the mock verifier | Measured: the adversary at chance (5.0%) on both private paths and at 100% on the plaintext claim and the naive nullifiers; private claims, burns and releases identical once nullifiers, fresh randomness, own addresses and time are cut; 10 of 10 unclaimed deposits and 10 of 10 unreleased burns recovered from the seed; seven public fields §B6 does not list | `scripts/privacy/run.sh` (with `RISC0_HOME` as for `lez/`) |
| [Finality-rule analysis](docs/design/finality-rule/LEMMA.md), `docs/design/finality-rule/` | A model of LEZ v0.4 sequencer and Logos Blockchain behaviour cited to source lines, a written proof, and a discrete-event simulator | Simulator (Python, standard library), not a network | Broke the earlier rule (A); the repaired rule shows no violation in 500 adversarial runs within its conditions | `cd docs/design/finality-rule/sim && python3 experiments.py` (12,000 runs, 25 min on 14 processes; `python3 test_sim.py` checks the model) |

**Since 11 November 2026** (EVIDENCE rows 28-30): the bridge program's freezes and caps run on time it reads from
its own attestation program's records, never on a caller's time ([`lez/BRIDGE.md`](lez/BRIDGE.md) §6); rule (B)
also checks the tip's producer inscription, so a forged history needs one channel key besides `t` attestation keys
([`checkpoint/RESULTS.md`](checkpoint/RESULTS.md) §4a); and the findings of a security review in a separate agent
context are fixed on both sides (LEZ: public balances at `holder(account)` PDAs, `ImportRoot` only as a visible
call, one `NOut` per burn, a MAC over the memo; vault: the keyless-retirement clock, the checkpoint fee to the
prover, stale candidates, no deposit after a final sunset, test chains only, a bound on `revokeWindow`), with tests
that fail on the code before the fixes ([`lez/BRIDGE-RESULTS.md`](lez/BRIDGE-RESULTS.md) §13,
[`vault/GAS.md`](vault/GAS.md)). These changed the bridge, `checkpoint_b` and release images: the real crossing of
row 27 and the real proofs before it used the earlier programs; the crown run, rehearsed on a fork of Sepolia with a
LEZ devnet and real proofs (row 31, [`e2e/crown/README.md`](e2e/crown/README.md)), uses these.

**Added in the seventh pass** (11 November 2026, EVIDENCE rows 35–40): the vault through Slither, Aderyn and
Halmos and the Rust dependencies through cargo-audit and cargo-deny, every finding triaged: no defect in the vault,
7 Halmos properties proven and 4 not finished in their bound ([`notes/reviews/security/tools-2026-11-11.md`](notes/reviews/security/tools-2026-11-11.md));
the crown run's six programs rebuilt to their image ids at the published crown build path
([`docs/guides/verify.md`](docs/guides/verify.md)); the private attestation, claim and burn padded as LEZ's wallet
pads every private transaction and proven at 8 and 16 CPUs, 10.6 minutes at most at 16
([`lez/results/p4-minimum/README.md`](lez/results/p4-minimum/README.md)); a second consumer of the attestation,
`ferrata_holdings`, which accepts an ERC-20 balance at a finalized block (consumer guide §12); seven fuzz targets
over the core, 1.79 billion executions ([`fuzz/RESULTS.md`](fuzz/RESULTS.md)); and an independent review of the
core against the specifications, with 24 more mutants, all killed ([`docs/REVIEW-core.md`](docs/REVIEW-core.md)).

The guests need RISC Zero's toolchain (`rzup`, r0vm 3.0.5), the vault Foundry (forge 1.3.6, solc
0.8.26), and the Groth16 wraps Docker on x86-64. `lez/` and the checkpoint's fidelity tests read LEZ
v0.4.0-rc1 and Logos Blockchain from `upstream/` beside the repository; `lez/scripts/lez-select.sh v0.4.0-rc2`
points `lez/` at LEZ v0.4.0-rc2 instead (what changed: [docs/design/lez-rc2-changes.md](docs/design/lez-rc2-changes.md)).
`scripts/fetch-upstream.sh` fetches all three at the pinned commits. Scripts keep their scratch in
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

## Guides, trust and performance (first drafts, 11 November 2026)

The documents the two RFPs ask for, drafted from the results and design notes above; every statement
cites the file it rests on, and each says what is not built.

- [docs/guides/consumer-guide.md](docs/guides/consumer-guide.md): consuming attestations from a LEZ
  program (public call, private read), statement identifiers, the Recommended Consumer Pattern, error
  codes, chain binding, migration (RFP-021 S.3, S.4).
- [docs/guides/operator-guide.md](docs/guides/operator-guide.md): the updater, the lab, the root
  importer, the checkpoint prover and relayer, watchers, keys, costs, failure modes, retirement
  (RFP-021 S.5, S.7; RFP-022 R.13).
- [docs/guides/deployer-guide.md](docs/guides/deployer-guide.md): deploying the programs on LEZ and the
  vault on an EVM chain, gas limits under Glamsterdam's state gas, configuration and pairing
  (RFP-022 R.14).
- [docs/guides/user-guide.md](docs/guides/user-guide.md): deposit, claim, burn, release, recovery from
  the seed, what each step reveals, defaults, and the developer integration journey (RFP-022 U.5–U.8,
  R.12).
- [docs/guides/verify.md](docs/guides/verify.md): check it yourself: rebuild the crown run's programs to
  their image ids (`scripts/reproduce-image-ids.sh`) and check a Sepolia run from its records against any JSON-RPC
  endpoint (`scripts/verify-sepolia.py`), with what each check proves and what it still trusts.
- [docs/trust.md](docs/trust.md): who is trusted for what, what each failure does and what bounds the
  loss, residual privacy correlations (RFP-021 S.6; RFP-022 Security).
- [docs/performance.md](docs/performance.md): RFP-021 P.1–P.9 and RFP-022 P.1–P.7 with where each figure
  was measured, and the gaps.

## Specified but not built

The design notes specify these; there is no code for them here:
- in the bridge program on LEZ (`lez/BRIDGE-RESULTS.md` §9): a permissionless `Sunset`, the freeze
  authority's 14-day handover, a multisig freeze authority and the watchers on LEZ, transfers between
  holders, the switch to a v0.4 token program when one is released;
- rule (B): `ferrata_attestor` as the Logos module with which each sequencer would sign its node's
  final tier (its core, the certificate and the final-tier message, is `checkpoint/crates/attestor-core`);
  the rule (B) paths of the checkpoint program (`checkpoint/RESULTS.md` §4) and of the vault (v5) are built;
- the gas drop and the relayer; `monitor`, the watchers' node-side program (the vault's watcher role is built);
- the Logos modules beyond a prototype: `ferrata_bridge_module`, and of `ferrata_attest_module` all but
  the three methods in `modules/` (no network, no transactions, no log attested end to end); the
  attestation side's CLI with dry runs (the bridge's `ferrata` CLI is a prototype in `cli/`, whose README
  says what it does not do), the updater and relayer daemons, the IDL, the mini-app and the admin view;
- in the attestation program: successors and the root id (a fork not in the schedule fixed at
  `Init`); the weak-subjectivity bound, in the core too; the vault's deposit leaves on real data
  through the attestation program's leaf requests (they ran on a synthetic state, `lez/RESULTS.md` §6;
  the vault on Sepolia has been read only by the bridge program's `Init` and `ImportRoot`,
  `lez/live/inbound/README.md`); in the reference consumer, the nullifier keyed on a depositor's secret and with it a
  private claim (the bridge program has both), and the second reference consumer
  (`docs/design/attestation.md` §15, `lez/RESULTS.md` §5).

Changes in [the v3 specification](docs/design/spec/bridge-spec-v3-changes.md) that the prototypes do not
implement yet:
- bridge core: the witness-independence check of the guests' cycle counts runs in `bridge/scripts/check.sh`,
  not in CI;
- rule (B) (D13): watchers checking a candidate when it is submitted (`monitor`; the vault's rule (B)
  path, the final-tier message and the attestation-key certificates are built);
- rule (A) (D13): watchers cancelling a candidate whose key set is not the live one (`monitor`); Δ_g of
  3 h when participation is partial (a deployment parameter);
- roles (D13): LEZ's keyless `Sunset` (the vault's side, the watchers, the 14-day role change, keyless
  `retire()`, the cap floor and the deployment script that renounces the emergency stop's owner, is built;
  the bridge program's `Sunset` is the freeze authority's);
- inbound (D13): the gas-drop receipt binding `(N, oneTimeAddress, relayer)` (refunds drawing on the
  release cap are in the vault; `ImportRoot` attesting slots 0 and 1, and claims refusing
  `leafIndex >= count`, are in the bridge program);
- outbound (D13): releases against the newest root whose delay has passed, a wallet rule (the write-once
  burn account keyed on the whole note is in the bridge program; `NOut` v2 is in the release guest and the
  core since the security fixes).

## Not audited. Not for production.

No part of this repository has been audited. One piece is deployed on a public network: the v3 vault of
the end-to-end fixture (not the v5 vault of `vault/`) is on Sepolia at
[`0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7`](https://sepolia.etherscan.io/address/0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7) (created by
transaction `0x0a5c3a64b6357c76e273f3ecd91ad58078f9c872f3ab90e68dba521de0589f12`, block 12,107,894), with one deposit, deposit 0
(transaction `0xf3235de7115886851c2a92b6471e1cdce9c8aa9fa3c39558d60cade17a5789a0`, block 12,107,901, 0.001 ETH).
Deposit 0 was claimed on LEZ by the plaintext path: on a LEZ v0.4.0-rc1 sequencer we run, from a capture
of Sepolia's finalized state, with the frozen bridge program, not this tree's
([`lez/live/inbound/README.md`](lez/live/inbound/README.md), "The real session"). No public LEZ network runs
any of it: LEZ testnet 0.4 does not exist. The prototypes exist to measure and to test the design; the audit
is planned for the code the proposal would deliver. A security review by an agent in a separate context
(11 November 2026) is not an audit; its fixes are in this tree (EVIDENCE row 30), and the frozen bridge program
of the real session predates them and the time fix. Slither, Aderyn, Halmos, cargo-audit, cargo-deny and fuzzing
(EVIDENCE rows 35, 36 and 40) are tooling, not an audit.

## How it was built

The code and the documents were drafted with coding agents (Claude Code) under the author's design,
review and acceptance tests; every commit was made this way. Not every commit says so: 185 of the 418
commits up to `0b135f7` carry an `Assisted-by` trailer (`git log --grep=Assisted-by`), every commit made
after 04:37 CET on 11 November among them; the others, most of those of 10 and 11 November, should be read
the same way. The author owns the
design, the decisions and the acceptance tests: the specification and BLS vectors, the real-chain
replays, the differential oracles, the adversarial and mutation suites and the measurements each
directory documents. [docs/REVIEW-core.md](docs/REVIEW-core.md) maps each function of the core to its
specification section for the author's line-by-line review; its sign-off column is not filled in yet,
and the bridge prototypes have no such map. Agent reviews in a separate context were part of the work;
none of them is an audit.

## Licence

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)), for
every directory, the imported ones included.
