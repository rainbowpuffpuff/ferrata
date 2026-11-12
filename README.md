# Ferrata

Ferrata lets programs on the Logos Execution Zone (LEZ) act on Ethereum facts without a trusted relayer,
and moves ETH and ERC-20 tokens between Ethereum and LEZ without linking the two sides. It has two parts:

- **An attestation primitive** (RFP-021): a LEZ program that checks Ethereum sync-committee signatures
  itself and keeps a write-once record of every finalized header it accepts, so that any LEZ program can
  prove an account, a storage slot or an event log against such a record inside its own transaction.
- **A bridge** (RFP-022): an Ethereum vault that escrows ERC-20s and ETH in fixed denominations. LEZ mints
  the wrapped token against a deposit verified through the primitive, and the vault releases against a LEZ
  burn verified by a zero-knowledge proof on Ethereum. On the default path nothing public on either chain
  links a deposit to its mint or a burn to its release, beyond token, denomination, time and the residual
  correlations the design lists.

This repository is one codebase, from a finalized Ethereum header to a wrapped token and back, written as
the evidence for a proposal to [Logos RFP-022](https://github.com/logos-co/rfp) (a privacy-preserving wrapped
ERC-20 and ETH bridge for LEZ) that also covers RFP-021 (trustless Ethereum state attestation for LEZ).

What is here are prototypes and their measurements. **Not audited. Not for production.**

## Who it is for

- **LEZ program developers** who need an Ethereum fact (an account, a storage slot, an event log) inside
  their own transaction, by a public call or a private read: the
  [consumer guide](docs/guides/consumer-guide.md).
- **People moving ETH or a registered ERC-20** from Ethereum to LEZ and back without linking the two sides,
  and developers wiring those flows into a Logos module: the [user guide](docs/guides/user-guide.md).
- **Operators** of the services around it (the updater, the root importer, the checkpoint prover and
  relayer, watchers): the [operator guide](docs/guides/operator-guide.md).
- **Deployers** standing up their own, independently configured deployment, the programs on LEZ and the
  vault on an EVM chain: the [deployer guide](docs/guides/deployer-guide.md).

The guides are first drafts of 11 November 2026; every statement cites the file it rests on, and each says
what is not built.

## Quickstart

**In one command**, in a container with every toolchain pinned: the checks below and the rebuild of all 14 guest
programs against [`docs/image-ids.md`](docs/image-ids.md), ending in one line per check and a verdict (with Docker; a
10 GB image; 36 minutes on 16 CPUs). Without Docker, on a Modal account: `modal run scripts/reproduce/modal_app.py`.
What it pins and what it leaves out: [`scripts/reproduce/README.md`](scripts/reproduce/README.md).

```sh
scripts/reproduce/run.sh
```

Commands run from the repository root. The first two need only Rust (with curl and tar for the vectors)
and Python 3.

The light-client core on the official consensus vectors and real mainnet and Sepolia data (native tests,
a laptop is enough):

```sh
scripts/fetch-vectors.sh && cargo test --workspace --locked
```

(163 tests in 25 binaries; under two minutes on a 16-core machine with an empty cargo registry.)

The live Sepolia round trip, checked from its committed records, read-only (Python's standard library
only; it asks a public endpoint unless you give your own with `--rpc URL` or `SEPOLIA_RPC_URL`):

```sh
python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-1
```

The rest needs RISC Zero's toolchain (`rzup`, r0vm 3.0.5), Foundry (forge 1.3.6, solc 0.8.26) and, for
the Groth16 wraps, Docker on x86-64. The vault's suite (install the pinned dependencies first:
[`vault/README.md`](vault/README.md)):

```sh
cd vault && FOUNDRY_OFFLINE=false forge build && forge test
```

`vault/foundry.toml` keeps forge offline, so the first build must be allowed to fetch solc 0.8.26
(`FOUNDRY_OFFLINE=false`; without it forge stops at "can't install missing solc 0.8.26 in offline mode"); later
runs need no network. Expected: 451 passed and 1 skipped, the privacy suite's Ethereum half, which replays the
output of its LEZ half (`scripts/privacy/run.sh` runs both).

The attestation and bridge programs on LEZ v0.4.0-rc1's own state machine, in process, with RISC Zero in
dev mode. LEZ's guests are built with RISC Zero's guest Rust 1.91.1, kept in a RISC Zero home of their own
beside the repository (with `~/.risc0`'s 1.88.0 their build stops at "ruint@1.21.1 requires rustc 1.90");
`scripts/ci/install-risc0.sh` installs `rzup`, r0vm 3.0.5 and both guest toolchains:

```sh
scripts/fetch-upstream.sh
RISC0_HOME_LEZ=$PWD/../risc0-home-lez scripts/ci/install-risc0.sh
cd lez && RISC0_HOME=$PWD/../../risc0-home-lez RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads 1
```

On a 16-core machine: about 7 minutes of build, then 56 tests in 30 binaries in about 21 minutes (one test
thread; `vault_consumer` alone runs six minutes without printing). The tests rewrite the tables in
`lez/results/`, so git then shows them changed: the committed tables were written by an earlier build (they
name bridge image `43615dc4…`), and with this tree's programs ([`docs/image-ids.md`](docs/image-ids.md)) the
image ids and some execution-gas figures differ. The rest of the setup, the real proofs and the tables:
[`lez/RESULTS.md`](lez/RESULTS.md) §8.

The larger jobs have one command each (`just m0`, `just bench`, `just release`:
[docs/release-engineering.md](docs/release-engineering.md)); they need [`just`](https://github.com/casey/just), or
run the script each recipe wraps, named there (`just m0` is `python3 -I scripts/m0.py`, which only reads files).
Every component's re-check command is in [the table below](#components-and-their-status).

## Architecture in brief

```text
Ethereum                                                 LEZ
beacon chain: finality updates ---- anyone submits ----> attestation program: checks the sync
                                                         committee's signatures, writes a record
                                                         of each finalized header
                                                           | accounts, storage slots, event logs,
                                                           v proven against a record
FerrataVault: a deposit, in a ----- deposit root ------> bridge program: ImportRoot, then a claim
fixed denomination, with a                               mints the wrapped token (private by
commitment                                               default, a write-once nullifier)

FerrataVault: a candidate <-------- checkpoint proof --- a burn (a write-once note) in a LEZ block;
checkpoint, final after a           (Groth16)            the checkpoint program re-hashes the
wait; releases against the                               blocks, collects the burns, checks
checkpoint's burn root                                   finality by rule (B) or rule (A)
```

**Ethereum to LEZ** rests on Ethereum's sync committee and nothing else. The attestation program is the
light client of `crates/ferrata-core` running as a LEZ program: anyone can submit updates, and they stand or
fall on their signatures. Any LEZ program, the bridge among them, proves an Ethereum fact against one of its
records inside its own transaction ([design note](docs/design/attestation.md)). A deposit puts ETH or an
ERC-20 into the vault in a fixed denomination, with a commitment; the bridge program imports the vault's
deposit root through the attestation program (`ImportRoot`), and a claim against that root mints the
wrapped token, privately by default.

**LEZ to Ethereum** rests on the honesty of the zone's sequencers, because LEZ v0.4 offers an outside
verifier nothing stronger. The checkpoint program, a RISC Zero guest, re-hashes LEZ blocks, collects their
burns and checks finality by rule (B), final-tier attestations by t of n of the zone's sequencers (the
proposed rule), or by rule (A), inferred support with a beacon and a wait (the fallback). The vault verifies
its proof, wrapped to Groth16, holds the checkpoint as a candidate that watchers can cancel, and finalises it
after a wait; each release is proven against the checkpoint's burn root ([design note](docs/design/bridge.md);
who is trusted for what: [docs/trust.md](docs/trust.md)).

Around the programs: `ferrata`, the users' and relayers' command-line tool; the relayer `ferrata-relayer`;
the updater `ferrata-updater`; the watchers' `monitor`; the rule (B) attestor a sequencer runs; two Logos
modules and a Logos mini-app; a read-only admin view. Each is in the table of components below, with its
status.

## Live on Sepolia, and how to check it yourself

On 11 November 2026 (EVIDENCE rows 41–42) the whole round trip ran on earlier versions of these programs, Sepolia
to LEZ and back: the first live run, `crown-live-1`, ran `integration5`'s programs ([`e2e/crown/README.md`](e2e/crown/README.md),
"The programs of crown-live-1"), not the final ones of [`docs/image-ids.md`](docs/image-ids.md), which the second
live run, `crown-live-2`, deploys (its records are not in this tree yet). FerrataVault v5 deployed in mode (B) at
[`0xfE3199fa2634413CbAC317371076Cae3EB2d9E54`](https://sepolia.etherscan.io/address/0xfE3199fa2634413CbAC317371076Cae3EB2d9E54); a public and a private deposit, final on Sepolia, imported by the
bridge program on a LEZ v0.4.0-rc1 devnet from Sepolia's own finality updates and claimed, the private one with a
real proof; a public and a private burn; a rule (B) checkpoint over the devnet's blocks proven on a GPU, its Groth16
seal verified on Sepolia; both releases paid. 13 transactions, 74.9M gas, four starts (three stops on the RPC
endpoint's lagging reads, each resumed), verify 14 of 14 ([`e2e/runs/crown-live-1/`](e2e/runs/crown-live-1/README.md)).
The LEZ side was a devnet we ran (one validator): no public LEZ network runs any of it
("Not audited. Not for production.", below).

Anyone can re-check it from the committed records against a JSON-RPC endpoint of their choice:
`python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-1` (13 passed on two public endpoints). It checks
every recorded transaction, the deployment's CREATE addresses, the vault's immutables (image ids, configId,
genesisState, rule (B), verifier, linked registry), the deposits' commitments, the vault storage LEZ imported
(Merkle proofs against Sepolia's final headers), the checkpoint journal against the LEZ records and its seal
through the vault's own verifier, finalization and releases (EVIDENCE row 42).
[docs/guides/verify.md](docs/guides/verify.md) says what each check proves and what it still trusts, and how
to rebuild the run's programs to their image ids.

## Components and their status

Commands run from the repository root. Each directory's README gives the toolchains, the versions and
every other command.

| Component | What it is | Where it ran | Status | Reproduce |
|---|---|---|---|---|
| [Light-client core](crates/ferrata-core/README.md), `crates/` | `ferrata-core`: Ethereum sync-committee light client and execution-layer proofs (RLP, Merkle-Patricia, accounts, storage, receipts, logs, accumulator leaves), `no_std` Rust; `ferrata-spectests`: its vector runner and test suites | Laptop CPU, native tests; CI on every push | Built and tested: 52/52 official `sync` cases (Altair to Gloas), real mainnet and Sepolia data, Helios as a differential oracle, 107 planted mutants all killed and the 24 of an independent review; fuzzed, seven targets, 1.79 billion executions | `scripts/fetch-vectors.sh && cargo test --workspace --locked` |
| [Guest benchmarks](bench/README.md), `bench/` | The core inside a RISC Zero zkVM 3.0.5 guest, with real mainnet data | RISC Zero executor on a laptop CPU (cycle counts, no proving) | Measured: a finality update is 10.2M cycles, in steps of 2.3M, 4.2M and 3.6M | `cd bench && cargo build --release -p host && ./target/release/host blst-accel` |
| [LEZ program](lez/RESULTS.md), `lez/` | The light client as a LEZ v0.4.0-rc1 program (rebuilt against v0.4.0-rc2 unchanged: [`lez/RESULTS.md` §10](lez/RESULTS.md#10-lez-v040-rc2)), with a public job path and a proven path, its store upgrading itself at a fork of its schedule; attestation of accounts, storage slots, event logs and accumulator leaves under its records, Gloas records included (with the block's RLP header as witness); a reference consumer with a public claim and a private attestation | In-process harness over LEZ's own state machine and a copy of its fee settlement, on a cloud container; RISC Zero dev mode, apart from the real proofs. One recorded run of the program as it was before the attestation (image `cec0bc68…`) on LEZ's own v0.4.0-rc1 sequencer, receipts verified, with a Logos Blockchain node (one validator, leader proofs in dev mode), on a rented Linux container; one of the current image on LEZ's v0.4.0-rc2 sequencer, without a real proof; and, since 10 November, two instances (Sepolia and mainnet) of this source kept following their chains on a LEZ v0.4.0-rc1 zone on Modal, a session every two hours ([`lez/lab/`](lez/lab/README.md)). Real proofs: one private attestation (one NVIDIA L4; and on cloud CPUs held to 8 and 16 CPUs with 16 GiB, 904.3 s and 497.3 s: [`lez/results/p4-minimum`](lez/results/p4-minimum/README.md)), and the update path on earlier builds of the program (a laptop CPU, one NVIDIA L4, and the sequencer run's container CPU) | Prototype: `Init` and five real mainnet updates settled, every public transaction under the 5M target. On a mainnet block finalized on 10 November 2026: a storage slot attested in 274,842 cycles, an ERC-20 `Transfer` log in 96,584, a consumer's claim of that log in one transaction of 183,538, and the same log attested by a privately executed consumer in 2,355,666 user cycles (380,577 the consumer's) with only the read of the record public, proven in 16.1 s on one NVIDIA L4 (a 227,170-byte receipt, settled with the receipt verified). On Sepolia (Gloas since October): from the bootstrap before the fork through it to period 1409 (166 transactions, the store upgrading itself), then under a Gloas-era record a storage slot in 278,253 cycles and an ERC-20 `Transfer` in 96,713, the same proofs refused as mainnet statements; the vault's deposit leaves proven in the guest at about 65,500 cycles each, on a synthetic state, measured before the vault was deployed. On the sequencer, the 65 harness transactions of that earlier image had the same outcome, 64 at the same execution gas, and a real proof of a private finality update, made against the live instance on a CPU, was verified and applied. On v0.4.0-rc2: every harness table row the same as on rc1; on its sequencer the same outcomes and charges, deployment now paying execution gas and blocks final after 14–25 s | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads 1` (setup: `lez/RESULTS.md` §8; rc2: §10); the sequencer runs: `lez/live/README.md` |
| [Bridge program](lez/BRIDGE-RESULTS.md), `lez/` | The RFP-022 bridge program on LEZ v0.4.0-rc1 ([plan](lez/BRIDGE.md); on v0.4.0-rc2 the same figures, `lez/RESULTS.md` §10): a configuration paired with its vault, tokens registered by attesting the vault's registry, `ImportRoot` through the attestation program's direct path, plaintext and private claims with write-once nullifiers, fees by leaf index, token and global caps and freezes on attested Ethereum time (no caller time), roles and `Sunset`, wrapped balances in the bridge's own accounts (a `holder(account)` PDA per owner; no v0.4 token program is released), burns with write-once notes in the checkpoint's encoding | In-process harness over LEZ's own state machine and fee settlement, on a cloud container; a synthetic vault and records placed at genesis (the first deposits are the shared vectors'), and in one test blocks holding exactly the storage FerrataVault v5 wrote in a local EVM; private execution in RISC Zero dev mode in the tests; then one real proof each of a private claim and a private burn on one NVIDIA L4, of the program before its fees, caps and freezes, settled with the receipts verified on the build box; the merged program's private claim and burn proven for real on a LEZ devnet in the crown rehearsal ([EVIDENCE](docs/EVIDENCE.md) row 31) and on cloud CPUs held to 8 and 16 CPUs with 16 GiB (claim 905.4 s and 463.7 s, burn 756.3 s and 389.0 s; row 33) | Prototype: 78 transactions for the registry (two tokens in a configuration), fees by leaf index, caps, freezes and `Sunset`, besides the inbound, private and burn tests, every refusal with its code. On the merged program (time fix and security fixes, `lez/results/bridge-*.md`): import 231,365 execution gas, plaintext claim 258,885 (142,444 before fees, caps and freezes), public burn 164,153 (45,867 before); a private claim 551,502 (LEZ's fixed 409,764 + 141,738) with only `N`, the denomination, the root read and the fee schedule's range public; the checkpoint's own parser collects the public and the private burn byte for byte, the import, and the freeze authority's `Sunset` once. On the storage the vault itself wrote: five tokens registered, its root imported and six deposits claimed with its fees (23 transactions, 3 refusals). Under a Gloas-era record (a real Sepolia header): the pairing, a registration, the import and a claim. Proven, of the earlier program: the private claim in 15.3 s and the private burn in 12.4 s on one L4 (227,477- and 226,978-byte receipts); one flipped bit in either receipt is refused; fuzzed (`fuzz/programs/`, EVIDENCE row 78) | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --test bridge_inbound --test bridge_private --test bridge_burn --test bridge_controls --test bridge_vault_storage`; the proofs: `lez/BRIDGE-RESULTS.md` §7 |
| [Inbound crossing, live](lez/live/inbound/README.md), `lez/live/inbound/` | Ethereum to LEZ on a running LEZ v0.4.0-rc1 sequencer and Logos Blockchain node: the attestation and bridge programs deployed through LEZ's loader, an instance following a beacon chain, the bridge paired with the vault, `ImportRoot`, a plaintext claim and a private claim with a real proof the sequencer verifies, and the refusals; the configuration's identity fixed in advance for the Sepolia vault, so the crossing deploys the programs frozen then (bridge image `66722fc7…`), not the tree's later bridge program | Modal build box (CPU), scripted and recorded; re-checked by replaying the recorded chain through LEZ's block application | Stand-in session done (its beacon chain and execution blocks made with keys we hold, not Ethereum): 57 blocks, 30 transactions as expected, the private claim proven in 254.8 s on CPU and included. On public Sepolia (11 Nov 2026): deposit 0 into the live vault `0xd0B5…8AF7` claimed on LEZ from capture 1 (instance caught up from period 1378 through Gloas to finalized slot 11551744; Init on the vault's `configId`; `ImportRoot` of root(1); the plaintext claim; refusals 215, 248, 249, 250): 186 blocks, 185 transactions, replay check passes. That run's deposit 1 was never sent: the private claim of a Sepolia deposit, by this tree's programs, is the live round trip's (EVIDENCE row 41) | `cd lez && live/inbound/run.sh stand-in`; `live/inbound/run.sh real CAPTURE_1 [CAPTURE_2]` on the build box |
| [GPU proving](gpu-prover/RESULTS.md), `gpu-prover/` | A small Modal app that proves the checkpoint fixtures, the LEZ update, one private attestation, the bridge program's private claim and burn, and the rule (B) checkpoint over real LEZ blocks with RISC Zero 3.0.5 on a GPU | One NVIDIA L4 in the cloud; receipts verified on the laptop (the bridge's on the build box) | Measured: 14–219 s per checkpoint proof (an hour of blocks with 50 private transactions in 219 s) and 49.5 s for the LEZ update, about 80 times faster than the laptop CPU; 16.1 s for the private attestation on the current images, 13.5 s on the previous ones; 15.3 s and 12.4 s for the bridge's private claim and burn, 39.5 s for the rule (B) checkpoint over 195 real LEZ blocks (no CPU figure) | `gpu-prover/README.md` |
| [Logos modules](modules/RESULTS.md), `modules/` | `ferrata_attest_module` (Rust over `ferrata-core`): `version`, `dry_run_attest` (a storage slot or log against a header record, as read from the chain or by its fields, with the program's verdict, code and statement ids), `verify_checkpoint`, `fetch_log_proof` (a block's receipts over JSON-RPC, the receipts trie rebuilt and checked, one log's proof), `build_attest_tx` (the program's `Attest` message and its `header(id, slot)` account), `read_attestation` (the program's event against the dry run), `read_instance` (an instance's state as LEZ holds it: config, store, jobs; the id recomputed); `ferrata_bridge_module` (Rust over `ferrata-bridge-core`): `deposit` (commitment, vault calldata), `claim` (path, `verify_claim`, nullifier, the program's `Claim`), `burn` (note, the program's `Burn`), `release` (path, `verify_release`, nullifier, the vault's journal), `recover`, `status`; a catalog layout (`modules/catalog/`) | Built with logos-module-builder, dev and portable packages, loaded together by a headless Logos Core (dev and portable `logosctl`) on the build box; a log and a storage slot of mainnet block 26,384,851 attested end to end by the attestation program in the `lez/` harness from transactions the module built; the release action's steps run locally, and both modules installed from the catalog by a headless Logos Core (HTTPS on 127.0.0.1) | 24 calls equal to native in both Logos Cores; bridge values equal the specification vectors; end to end 36 of 36 checks (two logs, a slot, the instance read back, refusals 110 and 211, the bridge messages decoded by the program) | `modules/scripts/check.sh` (Linux x86-64, Nix); `modules/scripts/e2e.sh OUT` |
| [Vault](vault/README.md), `vault/` | `FerrataVault` v5, the immutable Ethereum vault (Solidity), and its linked library: specification v2 with v3 D3–D5, D9 and the vault items of D11 and D13 (rule (B) beside rule (A) behind an immutable `mode`, watchers, the 14-day role change, keyless retirement, the cap floor, and D13's fees: mint, relay, burn and checkpoint fees under a 1% ceiling, changed seven days after announcement, published for LEZ); a deployment script that takes every gas limit from the node's estimate | Local EVM (Foundry): its own suite with a mock verifier behind RISC Zero's unmodified emergency-stop wrapper; the return leg with RISC Zero's real Groth16 verifier and real proofs; the deployment rehearsed on a local anvil fork of Sepolia, creation gas measured read-only on Sepolia; deployed on Sepolia and used by the live round trip (EVIDENCE row 41) | Prototype, with the security fixes of 11 November: 433 tests (43 with real proofs), 100% line and branch coverage of the vault and its library, 254 of 254 planted bugs caught, 14 invariants (balance equality, supply) on a rule (A) and on a rule (B) vault; real proofs of both rules verified (`test/real/`), among them a rule (B) checkpoint over 195 real LEZ sequencer blocks (`RealBlocks.t.sol`): a rule (B) checkpoint costs 542,969 gas against 974,346 for rule (A) (the security fixes add about 28.0k to each, `vault/GAS.md`); on today's Sepolia (EIP-8037) the vault's creation costs 39.5 M gas and fits: the live round trip created it with 39,247,243, and its calls' receipts are in `vault/GAS.md` | `cd vault && forge test` (install the pinned dependencies first: `vault/README.md`) |
| [Bridge core and release proof](bridge/README.md), `bridge/` | `ferrata-bridge-core` (commitments, trees, nullifiers, claim and release statements, recovery), the release and claim guests, and the release checked on Ethereum | Laptop CPU (RISC Zero 3.0.5 prover; Groth16 wrap in Docker on x86); local EVM with RISC Zero's real Groth16 verifier: the v2 vault with the checkpoint proof mocked, and the v3 vault with real checkpoint proofs (`e2e/`) | Prototype at specification v2, with v3's `NOut` v2 and `mac` v2 (review M-3, I-4): the release guest takes 77,736 cycles whatever the witness (executor; the earlier image, 80,501 cycles, was proven in 37–49 s on the laptop; the current one, the holder's proof, in 46.0 s at 8 CPUs and 26.4–27.0 s at 16, and wrapped, any relayer's step, in 55–93 s: [EVIDENCE](docs/EVIDENCE.md) row 33); an ETH release costs 367,988 gas on the v2 vault; fuzzed (`fuzz/programs/`, EVIDENCE row 78) | `cd bridge && scripts/check.sh` (no proving; `bridge/scripts/prove-bench.sh` proves) |
| [Checkpoint program](checkpoint/README.md), `checkpoint/` | The LEZ to Ethereum checkpoint proof: re-hashes LEZ blocks, collects burns, follows key rotations, checks the support rule (A) or the final-tier attestations (B); `crates/attestor-core`, the attestor's certificate and message | RISC Zero executor on a laptop CPU and, for rule (B), on the Modal build box, on LEZ blocks and Logos Blockchain inscriptions built with their own crates; two real proofs of a one-block checkpoint, wrapped to Groth16; a candidate and a finalisation proof of the end-to-end fixture, which the v3 vault accepted through RISC Zero's Groth16 verifier (`e2e/`); one real rule (B) proof on one NVIDIA L4, wrapped to Groth16 and verified by a rule (B) vault in a local EVM; **real sequencer blocks**: the 202 blocks and inscriptions of the lab's LEZ v0.4.0-rc1 chain (a devnet with one validator and leader proofs in dev mode, n = 1) checked on the build box, blocks 8..=202 proven under rule (B) on one NVIDIA L4 and accepted by the v5 vault with RISC Zero's real verifier (local EVM); every one of these proofs is of an image from before the `ImportRoot` fix of the checkpoint's decoder (12d17e3), which changed the guests (the real-block proof's `checkpoint_b` is `0xd3597543…`) | Prototype: specification v2 §4.1–4.2 with v3 D3; rule (A), and rule (B) of v3 D7 and D13 in its own guest; cycles of both measured on the same blocks. On real blocks: the parser and block hash accept all 202 blocks and inscriptions as the sequencer made them; rule (B) over 195 of them (585 transactions, n = 1, t = 1) in 7.66M cycles, proven in 39.5 s, 347,832 + 195,149 gas ([RESULTS](checkpoint/RESULTS.md), "Real LEZ blocks"); fuzzed (`fuzz/programs/`, EVIDENCE row 78) | `cd checkpoint && cargo +1.88 test -p checkpoint-core -p attestor-core` |
| [Return leg, end to end](e2e/README.md), `e2e/` | One fixture in which the checkpoint program, the release program and the v3 vault meet: a stand-in zone with burns, its candidate and finalisation checkpoints, the releases against the checkpoint's burn root, and the Sepolia scripts in `vault/script/` | Fixture, release proofs and the first wrap step on a 16-core cloud machine; checkpoint proofs on one NVIDIA L4; RISC Zero's Groth16 prover image in the cloud; local EVM (Foundry) with RISC Zero's real verifier; the Sepolia round trip rehearsed in the cloud on forks of Sepolia, every stage priced by Sepolia's node; on Sepolia, the first attempt's one transaction | Run: every proof accepted by the v3 vault with no mock on the Ethereum side, every negative case refused; the LEZ side is a stand-in (test keys, no node, no bridge program); proven with the checkpoint program before its `ImportRoot` fix, which refuses this fixture's import; a rule (B) checkpoint over real sequencer blocks, without burns, is in [checkpoint/RESULTS.md](checkpoint/RESULTS.md), "Real LEZ blocks". On Sepolia the first attempt ran out of gas at its first transaction (Foundry does not price EIP-8037's state gas; the round trip costs 5.6 times Foundry's figure there, `vault/GAS.md`); the re-planned run takes every limit from Sepolia's estimate and passed its rehearsals; that run stopped after deposit 0; the crown run's round trip with the v5 vault then ran on Sepolia to DONE (EVIDENCE row 41) | `cd vault && forge test --match-path 'test/real/*'` (`e2e/README.md` for the fixture) |
| [Round trip in CI](e2e/ci/README.md), `e2e/ci/` | One deposit there and back on one machine, every step asserted, in GitHub Actions (job `roundtrip`): the v5 vault under rule (B) on anvil with the vault tests' mock verifier; LEZ v0.4.0-rc1's state transition in the harness with the attestation and bridge guests (dev mode); `Init`, `Register`, deposit, `ImportRoot`, `ImportFees`, claim, burn; a rule (B) checkpoint by the checkpoint guest over blocks that hold the transactions the harness executed (`checkpoint/fidelity` `ci_zone`); release by the release guest; a forged checkpoint and a redirected release refused; then the `ferrata` tool's own end-to-end test (`cli/tests/e2e.sh`). [How CI works](docs/guides/ci.md): the jobs (also `witness-independence` and the vault's suites, `vault`), their caches | GitHub's `ubuntu-latest` (no secret, nothing sent to a public network); proven on the Modal build box by the job's own script | Built and tested on the box by the job's scripts (EVIDENCE rows 76, 77); the workflow itself has not run on GitHub yet; the attestation reads a block holding the anvil vault's storage, not a beacon-attested anvil block; test sequencer and attestation keys, no Bedrock node or sequencer process; dev-mode receipts accepted by the mock verifier | `scripts/fetch-upstream.sh && scripts/ci/install-risc0.sh && e2e/ci/run.sh` |
| [Command-line tool](cli/README.md), `cli/` | `ferrata`, the user's tool over the bridge core, the bridge wire, LEZ v0.4.0-rc1's own wallet library and alloy: `keys`; `deposit` (private destination by default), `claim` (private, after the deposit's claim delay, by default), `burn` (from a private account by default), `release` (against the newest accepted root whose release delay has passed; through any relayer by default, `--self-send` as the option), `recover` (deposits, claims, burns, releases and holdings from the seed alone) and `status`; every sending command is a dry run until `--send --yes`, printing what it would send, what becomes public with the exposure of the choices (the mini-app's rules) and the cost; each public path needs its own acknowledgement; ERC-20 deposits with `--approve`; endpoints by option or variable, with `--chain-id`; `--json` for scripts; every refusal named with the program's exit code or the vault's error | Modal build box (CPU): a local anvil (chain id 1) with the v5 vault and the vault tests' mock verifier; the zone in the LEZ harness with the bridge guest in RISC Zero dev mode | Prototype: the private round trip on the defaults end to end (deposit through the binary; claim and burn proven privately in the harness with the wallet's own keys; release through a relayer's job, the relayer paid); the public path with its warnings and acknowledgements; 23 refusals and holds; recovery from the seed file alone (12 deposits, 2 claimed, one privately; 12 burns, 2 released; the holdings); a USDT-shaped ERC-20 approved and deposited through the binary (EVIDENCE rows 84, 85). No real proof in this test; the LEZ commands not run against a sequencer or a public network | `cd cli && bash tests/e2e.sh` (Foundry and RISC Zero's toolchains: `cli/README.md`) |
| [Relayer](docs/guides/operator-guide.md#6-the-relayer-and-the-wrapper), `cli/src/bin/relayer.rs`, `cli/src/relayer.rs` | `ferrata-relayer`: an inbox of jobs from users who opt in (releases: journal and seal, never a secret) and from the checkpoint prover (rule (B) checkpoints); wraps holders' succinct release receipts (`--wrap-command`); finalises any live candidate after its wait; retries jobs that fail for a temporary reason; withdraws its fees; watches LEZ's burns; the gas drop (a receipt binding `(N, oneTimeAddress, relayer)`, one credit per `N`, recorded before it is sent); JSON logs, `status`, clean stop on SIGTERM | Modal build box (CPU): an anvil fork of Sepolia with crown-live-1's real proofs and a new real release proof naming a relayer; the LEZ harness with a local vault | Prototype: checkpoint, finalisation, releases and fee collection on the fork with real proofs (a real proof naming the relayer pays it 2 × 10¹² wei); forged and tampered calls refused by the verifier; the gas drop with real receipts against the harness zone, its native credit a stand-in; not run against a live sequencer | `cd cli && bash tests/relayer.sh` |
| [Updater](lez/updater/README.md), `lez/updater/` | `ferrata-updater`, the light-client updater daemon: several beacon sources cross-checked, every candidate checked with the program's own steps before a transaction, any open job continued, deferral between updaters, retries with backoff, fee limits, `/health` and `/metrics`, one TOML file | Modal build box, against LEZ v0.4.0-rc1's state machine in process (`HarnessLedger`, LEZ's fee settlement), dev mode | Prototype: catch-up to the lab's final mainnet and Sepolia roots on recorded data, liars and laggards demoted, restarts mid-job; proposal A3's done gate on 58 recorded updates in eight runs (EVIDENCE rows 79–81). Its backend for a running LEZ sequencer is not built | `cd lez && RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked` |
| [Watchers' `monitor`](monitor/README.md), `monitor/` | The watchers' node-side program (D13): checks every candidate against its own nodes' views (under rule (B) the tip must already be final; under rule (A) final by submittedAt + Δ_g; under both the key set must be the live one and the checkpoint fields must match), cancels in time, pauses for a persistent candidate or a stalled final tier, alerts on (and with the freeze key revokes) a wrong finalised checkpoint, audits the attestors' certificates and final-tier messages; state survives restarts | Build box: a local anvil chain with a v5 vault, an anvil fork of Sepolia at the live v5 vault | Prototype: 46/46 checks on anvil, 8/8 on the fork, 15 unit tests including the live round trip's real attestation; no LEZ RPC client (the node view comes through an adapter), no Logos module, no public network | `cd monitor && cargo test --release && bash tests/anvil-e2e.sh` |
| [Rule (B) attestor](checkpoint/attestor/README.md), `checkpoint/attestor/` | `ferrata_attestor`: the signer a sequencer runs beside its Logos Blockchain node (reads the node's final tier and checks it as the program does, signs with a certified key, never signs two histories) and the collector of t of n attestations | Build box: a real zone of four LEZ v0.4.0-rc1 sequencers (upstream's devnet configuration); the lab's chain with stand-in keys for a rotation; local EVM with RISC Zero's real verifier | Prototype: 3 of 4 proven on CPU, wrapped to Groth16 and accepted by a v5 vault (nMin 4, tMin 3); 2 of 4 and 11 forged cases refused; a silent sequencer; not a Logos module | `cd checkpoint && cargo +1.88 test -p ferrata-attestor` |
| [IDL](idl/README.md), `idl/` | The attestation and bridge programs' interfaces in SPEL's IDL format (every instruction, actor state and exit code); `idl/check.py` decodes the borsh vectors the wire crates write with the IDL alone, rejects IDL mutants, and decodes recorded real messages; SPEL's own crate parses both files (`idl/spel-check`) | CI job `idl`; build box | Built; regenerated at this merge for the wire changes of `lc-a1` (successors, the root id, the weak-subjectivity bound), `attest-a2` (codes) and `bridge-controls` (eight messages, the roles) | `python3 -I idl/check.py` |
| [Attestation CLI](lez/ATTEST-CLI.md), `lez/harness/src/bin/ferrata-attest.rs` | `ferrata-attest`: follows Sepolia through the program in the harness, advances the instance with an update, reads its configuration, attests a storage slot, an account, a log or a whole receipt from recorded proofs; the dry run gives the program's verdict or exit code and LEZ's charge; `--send` settles and checks it equals the dry run | In-process harness (build box) | Prototype: recorded fixtures, not live endpoints | see `lez/ATTEST-CLI.md` |
| [Admin view](admin/README.md), `admin/` | One static page (inline JS, read-only JSON-RPC) showing a deployed vault's configuration, roles, watchers, emergency stop, freezes, tokens, caps, fees owed, deposits, checkpoints, candidates, releases and refunds; `admin/dump.mjs` runs it in node | Sepolia (read-only, the live v5 vault); a local anvil scenario; headless Chromium on the build box | Prototype: read-only; self-test of 232 checks in CI job `admin` | `node admin/dump.mjs --selftest` |
| [Logos mini-app](modules/ferrata_bridge_ui/RESULTS.md), `modules/ferrata_bridge_ui/` | `ferrata_bridge_ui`, the RFP-022 mini-app as a Logos `ui_qml` module: deposit, claim, burn and release, recovery from the seed, the token registry with fees, caps and utilisation, the admin's `SetCap` and `Freeze`, what is public and private at each step, the leak indicator over the user's choices with the private, delayed and relayed defaults, the Ethereum RPC and LEZ endpoints, the wallet's connect and approval prompts in LP-0021's shape; its devnet backend `cli/examples/miniapp_devnet.rs` serves the module's methods from anvil and the LEZ harness | Packaged with logos-module-builder; driven headless (Qt Quick Test, offscreen) and loaded in Logos Basecamp (standalone, headless) on a cloud container (Modal) | Prototype: the private round trip on the defaults settles on the devnet: private deposit, the claim and release delays let pass on the devnet's clock, private claim and private burn proven by the harness with the wallet's own keys, release by a relayer to the fresh address; 17/17 test functions, 40 screenshots, in dev mode and with the two private proofs real (EVIDENCE rows 86, 87); the public path keeps its warnings. Basecamp's own UI test 10/10 on the previous view (its sandbox keeps a ui module off the network, so inside Logos the calls go to the core module, which has not run with the view) | `modules/ferrata_bridge_ui/scripts/box-test.sh`; `modules/ferrata_bridge_ui/scripts/basecamp-ui-tests.sh` |
| [Release engineering](docs/release-engineering.md), `justfile`, `scripts/` | One command per job: `just bench` (the proposal's numbers with a real proof and the hardware), `just bench-user` (user-side proofs on Modal at the declared minimum), `just m0` (the four M0 checks with values and GO / NO-GO), `just release` (versioned programs with image ids, the vault's bytecode, the CLI, checksums), `just deploy-testnet` (LEZ testnet 0.4 and an EVM chain; plan unless --send), `just halmos-long` | Build box and Modal containers (16 CPUs) | Built: release 0.1.0-rc.1 (six guests equal to the crown run's ids); the deployment's EVM side tested on anvil, its LEZ side in plan mode only | `just m0`, `just bench` |
| [Privacy suite](docs/privacy/RESULTS.md), `scripts/privacy/` | Milestone B2's privacy gate measured: 20 users' round trip (deposit, private claim, private burn, release) through the bridge program and the vault; every public byte collected on both chains; a differential gate, an adversary that tries to pair each claim with its deposit and each release with its burn, the field table against the proposal's §B6, and seed-only recovery | Cloud container (Modal, 2 vCPU): the LEZ harness in RISC Zero dev mode with synthetic records, Foundry with the mock verifier; again with **real proofs** of the final programs on one L4 GPU ([`docs/privacy/real/`](docs/privacy/real/RESULTS.md): 20 + 20 pairs, releases checked by RISC Zero's Groth16 verifier in the vault) | Measured: the adversary at chance (5.0%) on both private paths and at 100% on the plaintext claim and the naive nullifiers; private claims, burns and releases identical once nullifiers, fresh randomness, own addresses and time are cut; 10 of 10 unclaimed deposits and 10 of 10 unreleased burns recovered from the seed; seven public fields §B6 as first drafted did not list (now listed) | `scripts/privacy/run.sh` (with `RISC0_HOME` as for `lez/`) |
| [Finality-rule analysis](docs/design/finality-rule/LEMMA.md), `docs/design/finality-rule/` | A model of LEZ v0.4 sequencer and Logos Blockchain behaviour cited to source lines, a written proof, and a discrete-event simulator | Simulator (Python, standard library), not a network | Broke the earlier rule (A); the repaired rule shows no violation in 500 adversarial runs within its conditions | `cd docs/design/finality-rule/sim && python3 experiments.py` (12,000 runs, 25 min on 14 processes; `python3 test_sim.py` checks the model) |

The guests need RISC Zero's toolchain (`rzup`, r0vm 3.0.5), the vault Foundry (forge 1.3.6, solc
0.8.26), and the Groth16 wraps Docker on x86-64. Host Rust: stable for the root workspace, 1.98.1 for `lez/` and
`cli/` (their `rust-toolchain.toml`), and 1.88 and 1.93.0 where a command names them (`cargo +1.88` in
`checkpoint/`): `rustup toolchain install 1.88 1.93.0 1.98.1`. `cli/` and CI's round trip also need
`libpcsclite-dev` (LEZ's wallet links PC/SC). `lez/` and the checkpoint's fidelity tests read LEZ
v0.4.0-rc1 and Logos Blockchain from `upstream/` beside the repository; `lez/scripts/lez-select.sh v0.4.0-rc2`
points `lez/` at LEZ v0.4.0-rc2 instead (what changed: [docs/design/lez-rc2-changes.md](docs/design/lez-rc2-changes.md)).
`scripts/fetch-upstream.sh` fetches all three at the pinned commits. Scripts keep their scratch in
`tmp/` beside the repository, not in `/tmp`.

`vault/`, `bridge/`, `checkpoint/` and `docs/design/finality-rule/` were separate repositories,
imported with their history: their earlier commits keep their original paths.

## Evidence

[docs/EVIDENCE.md](docs/EVIDENCE.md) is the index: one page with every piece of evidence, its numbers, where
it ran, whether its proofs were real or in dev mode, what is stand-in data, and the command that re-checks it.
The latest additions, of 11 and 12 November 2026:

**Merged on 12 November 2026** (EVIDENCE rows 84–88; the four branches of `integration8`): the `ferrata` tool
complete for a user, private by default (deposit to a private account, the claim held by its delay and proven
privately with the wallet's own keys, a private burn, the release through any relayer), every step a dry run that
prints what becomes public, with recovery from the seed alone and 23 refusals and holds named before signing
([`cli/README.md`](cli/README.md)); the mini-app's private round trip on its defaults settled on its devnet,
also with the private claim and burn proven for real, 242.4 s and 209.4 s on a CPU
([`modules/ferrata_bridge_ui/RESULTS.md`](modules/ferrata_bridge_ui/RESULTS.md)); the padded private attestation,
claim and burn proven on Apple Silicon under macOS, 14.3 minutes at most on an M1 Mac mini with 16 GB and
7.9 on an M4, on the CPU (r0vm 3.0.5 has no Metal prover)
([`lez/results/p4-macos/README.md`](lez/results/p4-macos/README.md)).

**Since 11 November 2026** (EVIDENCE rows 28-30): the bridge program's freezes and caps run on time it reads from
its own attestation program's records, never on a caller's time ([`lez/BRIDGE.md`](lez/BRIDGE.md) §6); rule (B)
also checks the tip's producer inscription, so a forged history needs one channel key besides `t` attestation keys
([`checkpoint/RESULTS.md`](checkpoint/RESULTS.md) §4a); and the findings of the security review of 11 November
([`notes/reviews/security/review-2026-11-11.md`](notes/reviews/security/review-2026-11-11.md)) are fixed on both sides (LEZ: public balances at `holder(account)` PDAs, `ImportRoot` only as a visible
call, one `NOut` per burn, a MAC over the memo; vault: the keyless-retirement clock, the checkpoint fee to the
prover, stale candidates, no deposit after a final sunset, test chains only, a bound on `revokeWindow`), with tests
that fail on the code before the fixes ([`lez/BRIDGE-RESULTS.md`](lez/BRIDGE-RESULTS.md) §13,
[`vault/GAS.md`](vault/GAS.md)). These changed the bridge, `checkpoint_b` and release images: the real crossing of
row 27 and the real proofs before it used the earlier programs; the crown run uses these, rehearsed on a fork of
Sepolia with a LEZ devnet and real proofs (row 31, [`e2e/crown/README.md`](e2e/crown/README.md)) and then run live on
Sepolia to DONE on 11 November (row 41).

**Merged on 11 November 2026** (EVIDENCE rows 43–75; the ten branches of `integration7`): besides the rows
above, the weak-subjectivity bound in the core and the program, and successors with the root id (`Succeed`/`Spawn`:
a fork not in the schedule fixed at `Init` crossed by a successor under the parent's root id;
[`lez/results/a1-successors-ws.md`](lez/results/a1-successors-ws.md)); the vault's deposit leaves on real Sepolia data
through the attestation program's leaf requests, and the reference consumer's rules 5 to 7 with public and private
claims of the vault's real deposits, proven for real on a CPU ([`lez/RESULTS.md`](lez/RESULTS.md) §6); the bridge
program's B2 controls: the keyless `Sunset`, the freeze authority's 14-day handover, an M-of-N freeze authority,
watchers pausing LEZ for 12 hours and transfers between holders, the private transfer proven for real
([`lez/BRIDGE-RESULTS.md`](lez/BRIDGE-RESULTS.md) §14); rule (B) with t of n attestors on a real four-sequencer zone
([`checkpoint/attestor/RESULTS.md`](checkpoint/attestor/RESULTS.md)). The guests these changed have new image ids
([`docs/image-ids.md`](docs/image-ids.md)); the rows of earlier runs keep the ids they ran.

**Added in the seventh pass** (11 November 2026, EVIDENCE rows 35–40): the vault through Slither, Aderyn and
Halmos and the Rust dependencies through cargo-audit and cargo-deny, every finding triaged: no defect in the vault,
7 Halmos properties proven and 4 not finished in their bound ([`notes/reviews/security/tools-2026-11-11.md`](notes/reviews/security/tools-2026-11-11.md));
the crown run's six programs rebuilt to their image ids at the published crown build path
([`docs/guides/verify.md`](docs/guides/verify.md); since `integration7` guest builds are independent of the path,
[`docs/image-ids.md`](docs/image-ids.md)); the private attestation, claim and burn padded as LEZ's wallet
pads every private transaction and proven at 8 and 16 CPUs, 10.6 minutes at most at 16
([`lez/results/p4-minimum/README.md`](lez/results/p4-minimum/README.md)); a second consumer of the attestation,
`ferrata_holdings`, which accepts an ERC-20 balance at a finalized block (consumer guide §12); seven fuzz targets
over the core, 1.79 billion executions, and seven more over the programs (the bridge program's decoding and
handler, the checkpoint's LEZ block parser differential against LEZ's own decoder, rule (B) on edits of the lab's
real input, the Mantle inscription parsers, the release and claim inputs and journals), 4.31 billion executions, no
fault found ([`fuzz/RESULTS.md`](fuzz/RESULTS.md), "The programs"); and an independent review of the
core against the specifications, with 24 more mutants, all killed ([`docs/REVIEW-core.md`](docs/REVIEW-core.md)).

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
- [docs/guides/ci.md](docs/guides/ci.md): how CI works: the jobs, their caches, and what the GitHub runner
  differs in from the box the jobs were proven on.
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
- in the bridge program on LEZ (`lez/BRIDGE-RESULTS.md` §9): a transfer that credits a private recipient
  privately, the admin's own handover, the switch to a v0.4 token program when one is released;
- rule (B): `ferrata_attestor` packaged as a Logos module (the program is built, `checkpoint/attestor/`);
  the four-sequencer zone with a bridge program, burns and a rotation its committee proposes (the zone ran
  without a bridge program; the rotation was proven on the lab's chain with stand-in keys,
  `checkpoint/attestor/RESULTS.md` §2, §7);
- of the relayer, its root imports and its Logos module (the daemon `ferrata-relayer`, its wrapping and the
  gas drop are built); the watchers' `monitor` as a Logos module and its client of the LEZ and Logos
  Blockchain nodes' RPCs (the program is built, `monitor/`); the attestation side's `monitor` (records against
  several providers; the weak-subjectivity alarm);
- in the Logos modules (`modules/README.md`, "Left out"): reads and transactions through Logos Core's
  own modules (`eth_rpc_module`, `verified_proxy_module`, `keystore_module`, `tx_sender_module`,
  `lez_core`; the modules read plain `http://` JSON-RPC and build messages without signing or sending
  them), `advance` in `ferrata_attest_module`, the private claim, `ImportRoot`,
  registry reads and release proving in `ferrata_bridge_module`, and a published catalog (laid out, dry-run
  and installed from on 127.0.0.1 in `modules/catalog/`); the updater as a module (`ferrata-updater` is built as a
  plain binary, `lez/updater/`; its backend for a running sequencer is not built);
- the mini-app's private round trip on a LEZ sequencer: on its devnet the private round trip on the defaults
  settles (private claim and burn proven by the harness with the wallet's own keys, also for real; EVIDENCE
  rows 86, 87), but not through a sequencer: the `ferrata` tool's sequencer path (LEZ's wallet library for
  private sends and sync, and its reading of a private burn's note, `lez::visible_calls`) has not run against
  one; the mini-app calling `ferrata_bridge_module` inside Logos (both are built, on branches merged here, and
  have not run together);
- in the attestation program: consumers bound to the root id (the bridge, `ferrata_holdings` and the
  reference consumer still pin an instance); a chain of several EIP-2935 hops under a Gloas record (one hop
  is attested); the F.8 test (`docs/design/attestation.md` §6.5, §15).

Changes in [the v3 specification](docs/design/spec/bridge-spec-v3-changes.md) that the prototypes do not
implement yet:
- rule (A) (D13): Δ_g of 3 h when participation is partial (a deployment parameter);
- roles (D13): the deployment script that renounces the emergency stop's owner is run in fork
  rehearsals only (the vault's side, the watchers, the 14-day role change, keyless `retire()` and the
  cap floor are built);
- inbound (D13): the gas drop's sealed credit to a private one-time account and its sweep inside the
  private claim (LEZ's wallet; the gas-drop receipt binding `(N, oneTimeAddress, relayer)` is built and the
  relayer checks it; refunds drawing on the release cap are in the vault; `ImportRoot` attesting slots 0 and 1, and claims refusing
  `leafIndex >= count`, are in the bridge program);
- outbound (D13): releases against the newest root whose delay has passed, a wallet rule, in
  `ferrata_bridge_module` (it is in the `ferrata` tool, `cli/src/flow.rs`, tested in `cli/tests/e2e.rs`; the
  write-once burn account keyed on the whole note is in the bridge program; `NOut` v2 is in the release guest
  and the core since the security fixes).

## Not audited. Not for production.

No part of this repository has been audited. Two vaults are deployed on a public network, both on Sepolia.
FerrataVault v5 of `vault/` (rule (B)) is at
[`0xfE3199fa2634413CbAC317371076Cae3EB2d9E54`](https://sepolia.etherscan.io/address/0xfE3199fa2634413CbAC317371076Cae3EB2d9E54)
(created by transaction `0xee91c261265ac3ada50b525edb35a1faa32bb5bb4ffa2b1d433f5a278d9808a3`, block 12,111,048), with
its emergency stop (owner renounced) and library: the live round trip of 11 November 2026 made two deposits, a
checkpoint and two releases through it ([`e2e/runs/crown-live-1/`](e2e/runs/crown-live-1/README.md), EVIDENCE row
41), with testnet parameters (a 1-day revocation window, a 120 s release delay, no caps, no watchers, a freeze
authority without a key holder). Before it, the v3 vault of the end-to-end fixture was deployed on Sepolia at
[`0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7`](https://sepolia.etherscan.io/address/0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7) (created by
transaction `0x0a5c3a64b6357c76e273f3ecd91ad58078f9c872f3ab90e68dba521de0589f12`, block 12,107,894), with one deposit, deposit 0
(transaction `0xf3235de7115886851c2a92b6471e1cdce9c8aa9fa3c39558d60cade17a5789a0`, block 12,107,901, 0.001 ETH).
Deposit 0 was claimed on LEZ by the plaintext path: on a LEZ v0.4.0-rc1 sequencer we run, from a capture
of Sepolia's finalized state, with the frozen bridge program, not this tree's
([`lez/live/inbound/README.md`](lez/live/inbound/README.md), "The real session"). No public LEZ network runs
any of it: LEZ testnet 0.4 does not exist, and the round trip's LEZ side was a devnet we ran (one validator). The prototypes exist to measure and to test the design; the audit
is planned for the code the proposal would deliver. A security review by an agent in a separate context
(11 November 2026) is not an audit; its fixes are in this tree (EVIDENCE row 30), and the frozen bridge program
of the real session predates them and the time fix. Slither, Aderyn, Halmos, cargo-audit, cargo-deny and fuzzing
(EVIDENCE rows 35, 36, 40 and 78) are tooling, not an audit.

## Licence

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)), for
every directory, the imported ones included.

## How it was built

The code, the tests and the documents were drafted with coding agents (Claude Code) under the author's
design, decision log and acceptance tests; every commit was made with them.
[docs/how-it-was-built.md](docs/how-it-was-built.md) tells how, and what the commit trailers, the agents'
reviews and the author's own review do and do not cover.
