# Ferrata

[![ci](https://github.com/rainbowpuffpuff/ferrata/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/rainbowpuffpuff/ferrata/actions/workflows/ci.yml)

Ferrata lets programs on the Logos Execution Zone (LEZ) act on Ethereum facts without a trusted relayer
(RFP-021), and moves ETH and ERC-20 tokens between Ethereum and LEZ without linking the two sides (RFP-022). It is
the evidence for our proposal to Logos, [logos-co/rfp #257](https://github.com/logos-co/rfp/issues/257). Prototypes:
**not audited, not for production.**

## For reviewers: four links

1. **The status, with counts:** [docs/requirements-status.md](docs/requirements-status.md). Of the 59 hard
   requirements of RFP-021 and the 68 of RFP-022, **49 and 48 are shown** under our strict rule; 6 and 17 in part,
   1 and 1 not yet, 3 and 2 wait on Logos (27 November 2026). Each row gives its reason, tests and evidence.
2. **Verify it in ten minutes:** [docs/VERIFY.md](docs/VERIFY.md). Clone, then `python3 -I scripts/verify.py --all`
   runs the 124 quick offline checks in minutes, each naming the requirement rows it backs (Python 3.11; the few that
   also need Node or Foundry say so and skip without them); then `python3 -I scripts/verify.py --all --online` runs
   the 22 online checks, read-only.
3. **Try it on our test network:** [the testers' page](https://54-156-116-24.sslip.io/testers/): the faucet, the web
   wallet, a deposit on Sepolia and its claim on LEZ, step by step, with what each step should show; also a private
   claim from the command line, whose step says what still stops it. It is a LEZ v0.4.0-rc3 network we run for the
   bid, not Logos's LEZ testnet 0.4 (none is announced); its tokens have no value.
4. **The run records:** [docs/EVIDENCE.md](docs/EVIDENCE.md): every run and test, where it ran, whether its proofs
   were real, its record and the command that re-checks it; [What has run](#what-has-run) below picks the main ones.

## What it is

- **An attestation primitive** (RFP-021): a LEZ program that checks Ethereum sync-committee signatures
  itself and keeps a write-once record of every finalized header it accepts, so that any LEZ program can
  prove an account, a storage slot or an event log against such a record inside its own transaction.
- **A bridge** (RFP-022): an Ethereum vault that escrows ERC-20s and ETH in fixed denominations. LEZ mints
  the wrapped token against a deposit verified through the primitive, and the vault releases against a LEZ
  burn verified by a zero-knowledge proof on Ethereum. On the default path nothing public on either chain
  links a deposit to its mint or a burn to its release, beyond token, denomination, time and the residual
  correlations the design lists.

Both parts live in this one repository and are tested against each other along the whole path: from a
finalized Ethereum header to a wrapped token on LEZ, and back to Ethereum. It was written as the evidence for a
proposal to [Logos RFP-022](https://github.com/logos-co/rfp) (a privacy-preserving wrapped ERC-20 and ETH bridge
for LEZ) that also covers RFP-021 (trustless Ethereum state attestation for LEZ).

What is here are prototypes and their measurements. **Not audited. Not for production.**

**21 November 2026:** every proof here is made with RISC Zero 3.0.5, whose Groth16 verifier accepts forged claims (RISC Zero's advisories of 11 November 2026), and the owner closed the Sepolia vaults the same day (every release and deposit cap zero): [security note](docs/security/risc0-groth16-advisory-2026-11-21/README.md). On 23 November a new vault on RISC Zero's fixed verifier 3.1.0 took their place; it is the one our test network uses (the note's status of that day).

## Where to find the rest

| To | Read |
|---|---|
| see every result, with the command that re-checks it | [docs/EVIDENCE.md](docs/EVIDENCE.md) |
| re-check every claim from a terminal: each check with its requirement rows, offline in minutes, then online | [docs/VERIFY.md](docs/VERIFY.md): `python3 -I scripts/verify.py --all`; every component's command-line path: [docs/cli-coverage.md](docs/cli-coverage.md) |
| check the live Sepolia round trip yourself | [docs/guides/verify.md](docs/guides/verify.md) |
| find where each RFP requirement is met | [docs/traceability.md](docs/traceability.md) |
| see the status of each requirement | [docs/requirements-status.md](docs/requirements-status.md): one row per requirement, its status, reason, tests and evidence (the counts: [For reviewers](#for-reviewers-four-links)) |
| see the light client on the public LEZ testnet's version (v0.3.1) | [lez-v03/](lez-v03/README.md): an experimental, unaudited port; a finality update in one transaction (6,647,546 gas, 66.5% of the cap), run on a v0.3.1 sequencer we started with the image `8d3b59ff…` ([EVIDENCE](docs/EVIDENCE.md) row 131), the reviewed image deployed on the public testnet on 19 November (`Init`, two finality updates, an attestation of the vault; [EVIDENCE](docs/EVIDENCE.md) row 150). This branch's image `bac768a8…` adds a committee handoff in one transaction, sound only under the light client's honest-supermajority assumption (the next committee adopted by its summary; at most 6,737,794 gas on the mainnet replay and 6,682,541 on Sepolia's 32, in v0.3.1's block application in our harness, [EVIDENCE](docs/EVIDENCE.md) row 137); that image has not run on a sequencer. Its image `155b7317…` also freezes an instance on evidence that a sync committee it holds signed finality contradicting its records (one transaction, 6,656,148 gas at 510 of 512 signers; never produced by an honest network for an instance started from a finalized checkpoint, the assumption of [trust.md](docs/trust.md) §2.1; it must land while the instance is in the signing committee's period; experimental, unaudited, deployed nowhere, [`bench/MISBEHAVIOUR.md`](bench/MISBEHAVIOUR.md), [EVIDENCE](docs/EVIDENCE.md) row 144) |
| see the bridge's inbound leg and burns on LEZ v0.3.1 | [lez-v03-bridge/](lez-v03-bridge/README.md): an experimental, unaudited port; on a v0.3.1 sequencer we started, a private claim and a private burn proven for real; it already carries the fix of the two v0.4 known issues of [row 136](docs/EVIDENCE.md); reviewed twice ([EVIDENCE](docs/EVIDENCE.md) rows 132, 133) |
| see the leg back from the public LEZ testnet (a testnet stand-in) | [lez-v03-return/](lez-v03-return/README.md): **a stand-in, not the design**: the unchanged vault and real release proofs, the burn root attested by keys we hold (no checkpoint proof can exist for a v0.3.1 zone today); the attestor signs only after it has compared the burns with the bridge's state on the LEZ node at a Finalized block; rehearsed on a fork of Sepolia ([run](e2e/runs/testnet-return-fork-2/README.md)), not sent; and how far a real checkpoint gets on that network's own data ([EVIDENCE](docs/EVIDENCE.md) rows 134, 135) |
| be on LEZ testnet 0.4 the day it opens, once two accounts are funded (a deposit is credited about an hour after its block) and the kit is ported to LEZ v0.4.0 (released 27 November 2026; the kit is built for rc3, and the port has not started) | [lez/testnet04/](lez/testnet04/README.md): `day1.sh go URL` deploys the final attestation program byte for byte, starts a Sepolia instance and its updater and attests the live vault; the bridge program optionally; rehearsed on our own LEZ v0.4.0-rc3 zone with the sequencer restarted mid-run ([row 148](docs/EVIDENCE.md)); [`watch.sh`](lez/testnet04/watch.sh) watches for the endpoint |
| read the design | [attestation](docs/design/attestation.md), [bridge](docs/design/bridge.md), [what LEZ v0.4 decides](docs/design/lez-v0.4-findings.md) |
| know who is trusted for what | [docs/trust.md](docs/trust.md) |
| see what is not built yet | [Specified but not built](#specified-but-not-built) |

Every document, one line each: [docs/README.md](docs/README.md).

## What has run

| What | Result | Where it ran | EVIDENCE rows |
|---|---|---|---|
| The light-client core | 52 of 52 official `sync` cases, Altair to Gloas; 33 of them step by step against Helios as a differential oracle | laptop CPU, CI | [1](docs/EVIDENCE.md) |
| The light client kept running ([the lab](lez/lab/README.md)) | two instances following Sepolia and mainnet on a LEZ zone, unattended, a session every two hours; the record replays offline, and each session's finalized roots equal a second beacon provider's | a LEZ v0.4.0-rc1 sequencer we run, one validator | [21](docs/EVIDENCE.md) |
| The light client on the public LEZ testnet | its experimental v0.3.1 port (image `8d3b59ff…`) deployed, `Init` from a Sepolia checkpoint, two finality updates in one transaction each and an attestation of the vault's slots 0 and 1: 11 transactions in blocks 83019-83021; `verify` 29 of 29 and the explorer 11 of 11, the three blocks `Finalized` as the sequencer and the explorer reported them at the re-check (19 November, 11:21 UTC; the Logos Blockchain itself not checked by us) | the public LEZ testnet (LEZ v0.3.1); Sepolia read only | [150](docs/EVIDENCE.md) |
| The round trip, live on Sepolia | Ethereum to LEZ and back on the final programs, one public and one private crossing each way, through FerrataVault v5; anyone can check it from the committed records | Sepolia; a LEZ devnet we ran | [91, 92](docs/EVIDENCE.md) |
| Our seals on Ethereum mainnet, read-only (18 November; superseded as evidence) | every Groth16 seal the tree records with its journal (66, the live round trip's among them) accepted by RISC Zero's router and Groth16 verifier 3.0.0 on mainnet by `eth_call`; each seal, image id or journal with one byte changed refused. **Not evidence of soundness:** verifier 3.0.0 accepts forged claims and RISC Zero has stopped it ([security note, 21 November](docs/security/risc0-groth16-advisory-2026-11-21/README.md)); the row shows only that our seals are well-formed for the 3.0.0 circuits | Ethereum mainnet, read-only, two public endpoints; nothing deployed, nothing sent | [149](docs/EVIDENCE.md) |
| The return leg with several sequencers | the bridge program on a zone of four LEZ sequencers with an attestor beside each, its rule (B) checkpoint proven into the vault, and a rotation its committee proposed | a zone we ran; the vault on a local anvil, a stand-in beacon chain | [100, 101](docs/EVIDENCE.md) |
| The return leg on LEZ v0.4.0-rc3 | the checkpoint guests' second build reads Logos Blockchain 0.3.5-rc.1's fork digest (the deployed ids unchanged); its rule (B) checkpoint over an rc3 zone's real blocks proven for real, wrapped and accepted by the vault, both burns released | rc3's sequencer (one) and node (one validator); CPUs of a Modal box; the vault on a local anvil | [129, 130](docs/EVIDENCE.md) |
| The round trip on LEZ v0.4.0-rc3, live on Sepolia | Ethereum to LEZ and back with rc3's sequencer and its node 0.3.5-rc.1, the final LEZ programs and a new vault pinning the second build's `checkpoint_b`, real proofs; 13 transactions, verify 14 of 14 (rehearsed on a fork first) | Sepolia; an rc3 zone we ran, one sequencer, one validator | [141, 142](docs/EVIDENCE.md) |
| M0 check 4: real proofs on the newest LEZ tag of 16 November 2026 | the private update, attestation, claim and burn proven for real and applied by LEZ v0.4.0-rc3's own sequencer in final blocks (`just m0`: GO) | CPUs of a Modal box; rc3's sequencer and node | [123](docs/EVIDENCE.md) |
| M0 check 2: the gas drop | a claimant with no native LEZ: the relayer's sealed credit for its receipt's `N`, then its private claim on the final bridge sweeps the credit and pays its fee from it, real proofs, applied by LEZ v0.4.0-rc3's sequencer in final blocks; the same claim without the credit refused (`just m0`: GO) | CPUs of a Modal box; rc3's sequencer and node | [124](docs/EVIDENCE.md) |
| The privacy gate on real proofs | 20 users' round trip, every private claim, burn and release proven; an adversary pairing each side | one L4 GPU and CPUs on Modal | [89](docs/EVIDENCE.md) |
| The reproduction container | five checks and the rebuild of all 14 guest programs against [`docs/image-ids.md`](docs/image-ids.md): 5 of 5 | Modal; Docker on a fresh x86-64 machine ([record](scripts/reproduce/runs/2026-11-12-docker-aws/README.md)) | [90](docs/EVIDENCE.md) |

[docs/EVIDENCE.md](docs/EVIDENCE.md) is the index: one page with every piece of evidence, its numbers, where
it ran, whether its proofs were real or in dev mode, what is stand-in data, and the command that re-checks it.

## Check it yourself

Commands run from the repository root.

Every check below, and the rest, from one tool that names the requirement rows and EVIDENCE rows each one backs
([docs/VERIFY.md](docs/VERIFY.md); Python's standard library only):

```sh
python3 -I scripts/verify.py --list              # the checks
python3 -I scripts/verify.py --all               # every offline one, in minutes
python3 -I scripts/verify.py --all --online      # the read-only re-reads of Sepolia, our LEZ test network and the site
```

The live Sepolia round trip, checked from its committed records, read-only (Python's standard library only; it
asks a public endpoint unless you give your own with `--rpc URL` or `SEPOLIA_RPC_URL`):

```sh
python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-3
python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-4    # the same round trip with the LEZ side on LEZ v0.4.0-rc3
```

The light-client core on the official consensus vectors and real mainnet and Sepolia data (Rust, with curl and
tar for the vectors; 163 tests in 25 binaries, under two minutes on a 16-core machine):

```sh
scripts/fetch-vectors.sh && cargo test --workspace --locked
```

Five checks and the rebuild of all 14 guest programs, in a container with every toolchain pinned (Docker on
x86-64; a 10 GB image, half an hour on 16 CPUs):

```sh
scripts/reproduce/run.sh
```

The vault's suite, the programs on LEZ's own state machine and the larger jobs (`just m0`, `just bench`,
`just release`) need RISC Zero's toolchain and Foundry: [docs/guides/quickstart.md](docs/guides/quickstart.md).
Every component's re-check command: [docs/components.md](docs/components.md).

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

## Live on Sepolia

On 12 November 2026 the whole round trip ran on the final programs of [`docs/image-ids.md`](docs/image-ids.md),
Sepolia to LEZ and back, as `crown-live-3` (EVIDENCE rows 91–92): FerrataVault v5 deployed in mode (B) at
[`0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`](https://sepolia.etherscan.io/address/0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c),
the vault reviewers should check. The LEZ side was a devnet we ran (one validator). No LEZ network Logos runs carries
any of it; the attestation program and the bridge program at B1 also run on a LEZ v0.4.0-rc3 network we run that is
reachable publicly ([rows 237, 238](docs/EVIDENCE.md)). The first live run, `crown-live-1` (11 November, EVIDENCE rows 41–42),
made the same round trip on earlier versions of these programs.

On 18 November 2026 the same round trip ran live with the LEZ side on LEZ v0.4.0-rc3, then the newest tag: rc3's own
sequencer and its Logos Blockchain node 0.3.5-rc.1, one sequencer and one validator we ran, the same LEZ programs, and a
new FerrataVault v5 pinning the second build of the checkpoint guests (which reads that node's fork digest) at
[`0x80555Fe53CCcEf46951B23A3c75643a4C9283470`](https://sepolia.etherscan.io/address/0x80555Fe53CCcEf46951B23A3c75643a4C9283470),
as `crown-live-4` (EVIDENCE rows 141–142; [`e2e/runs/crown-live-4/`](e2e/runs/crown-live-4/README.md)): 13 transactions,
both private LEZ transactions and the checkpoint proven for real, both releases paid, verify 14 of 14.

The runs in full, the four vaults deployed on Sepolia and what each check proves:
[docs/sepolia.md](docs/sepolia.md) and [docs/guides/verify.md](docs/guides/verify.md).

**Security note, 21 November 2026.** The four Sepolia vaults verify through our own deployment of RISC Zero's Groth16
verifier 3.0.0, which accepts forged claims (RISC Zero's advisory GHSA-mm6c-35cm-g4x2 of 11 November 2026), with
no stop anyone can trip today and no freeze key. At Sepolia block 12,183,432 a forgery could take 0.001 Sepolia ETH
and nothing more. The same day the deployer's admin key closed all four: every release and deposit cap reads zero
(eight transactions, read back at block 12,184,587). What is affected, what is not, and the plan:
[docs/security/risc0-groth16-advisory-2026-11-21/](docs/security/risc0-groth16-advisory-2026-11-21/README.md).

## Components and their status

| Component | What it is | Status | EVIDENCE rows |
|---|---|---|---|
| [Light-client core](crates/ferrata-core/README.md), `crates/` | Ethereum sync-committee light client and execution-layer proofs, `no_std` Rust | built and tested | [1, 40, 69](docs/EVIDENCE.md) |
| [Guest benchmarks](bench/README.md), `bench/` | The core inside a RISC Zero guest: cycle counts on real mainnet data | measured | [2](docs/EVIDENCE.md) |
| [LEZ program](lez/RESULTS.md), `lez/` | The light client as a LEZ program; accounts, storage slots and logs attested under its records | prototype; on LEZ sequencers we run | [3–6, 21, 22, 32, 38, 39, 70–73, 88, 145](docs/EVIDENCE.md) (145: in-process harness) |
| [Bridge program](lez/BRIDGE-RESULTS.md), `lez/` | The RFP-022 bridge on LEZ: `ImportRoot`, private claims and burns, fees, caps, freezes | prototype; real proofs, a LEZ devnet | [8–10, 28, 30, 31, 33, 38, 74, 75, 78, 88, 91](docs/EVIDENCE.md) |
| [Inbound crossing, live](lez/live/inbound/README.md), `lez/live/inbound/` | Ethereum to LEZ on a running LEZ sequencer and Logos Blockchain node | run: a Sepolia deposit claimed on LEZ | [11, 27, 41, 91](docs/EVIDENCE.md) |
| [GPU proving](gpu-prover/RESULTS.md), `gpu-prover/` | A Modal app proving checkpoints, the LEZ update and the private attestation, claim and burn on a GPU | measured | [5, 10, 16, 17, 24](docs/EVIDENCE.md) |
| [Logos modules](modules/RESULTS.md), `modules/` | Six modules: `ferrata_attest_module`, `ferrata_bridge_module` and the four daemons (`ferrata_attestor_module`, `ferrata_monitor_module`, `ferrata_updater_module`, `ferrata_relayer_module`), on the tools' own code; a catalog layout | built; run in a headless Logos Core | [7, 59–61, 93–98, 103, 104, 106–110](docs/EVIDENCE.md) |
| [Vault](vault/README.md), `vault/` | `FerrataVault` v5, the immutable Ethereum vault (Solidity), and its linked library | prototype; deployed on Sepolia | [12–14, 25, 30, 35, 41, 54, 91](docs/EVIDENCE.md) |
| [Bridge core and release proof](bridge/README.md), `bridge/` | Commitments, trees, nullifiers; the release and claim guests | prototype; real proofs | [19, 30, 33, 77, 78, 88](docs/EVIDENCE.md) |
| [Checkpoint program](checkpoint/README.md), `checkpoint/` | The LEZ to Ethereum checkpoint proof: re-hashes LEZ blocks, collects burns, checks rule (A) or (B) | prototype; proven on real LEZ blocks | [15, 16, 24, 29, 78, 91](docs/EVIDENCE.md) |
| [Return leg, end to end](e2e/README.md), `e2e/` | The checkpoint program, the release program and the v3 vault in one fixture | run; the LEZ side a stand-in | [18, 25, 26, 41, 91](docs/EVIDENCE.md) |
| [Round trip in CI](e2e/ci/README.md), `e2e/ci/` | One deposit there and back on one machine, every step asserted, in GitHub Actions | runs in GitHub Actions (job `roundtrip`, green on `main`) | [76, 77](docs/EVIDENCE.md) |
| [Command-line tool](cli/README.md), `cli/` | `ferrata`, the user's tool: deposit, claim, burn, release, recover; private by default | prototype; anvil and the harness; a running LEZ sequencer (devnet) | [84, 85, 102](docs/EVIDENCE.md) |
| [Relayer](docs/guides/operator-guide.md#6-the-relayer-and-the-wrapper), `cli/` | `ferrata-relayer`: root imports on a fixed cadence, users' release jobs, rule (B) checkpoints, finalisation, the gas drop; also a Logos module (`ferrata_relayer_module`) | prototype; a fork of Sepolia; root imports on the mini-app's devnet | [46–48, 106–107](docs/EVIDENCE.md) |
| [Updater](lez/updater/README.md), `lez/updater/` | `ferrata-updater`, the light-client updater daemon; optionally sends only what sources of different consensus clients serve byte for byte | prototype; recorded data, the harness; 8 h on LEZ sequencers of two zones following Sepolia live, 74 updates applied, 56 on the chains kept ([record](lez/updater/results/live-sepolia/)); client diversity ten minutes live on mainnet against Nimbus, Lodestar and Lighthouse APIs ([record](lez/updater/results/client-diversity-2026-11-18/README.md)) | [79–81, 99, 147](docs/EVIDENCE.md) |
| [Watchers' `monitor`](monitor/README.md), `monitor/` | The watchers' node-side program: checks each candidate, cancels in time, pauses, alerts | prototype; anvil and a fork of Sepolia | [43–45](docs/EVIDENCE.md) |
| [Rule (B) attestor](checkpoint/attestor/README.md), `checkpoint/attestor/` | `ferrata_attestor`: the signer a sequencer runs, and the collector of t of n attestations | prototype; a four-sequencer zone, with the bridge program, its burns and their releases through a vault on a local anvil, and a rotation its committee proposed | [63–68, 100, 101](docs/EVIDENCE.md) |
| [IDL](idl/README.md), `idl/` | The attestation and bridge programs' interfaces in SPEL's IDL format | built | [55, 58](docs/EVIDENCE.md) |
| [Attestation CLI](lez/ATTEST-CLI.md), `lez/harness/` | `ferrata-attest`: advances an instance; attests a slot, an account, a log or a receipt | prototype; recorded fixtures | [56](docs/EVIDENCE.md) |
| [Admin view](admin/README.md), `admin/` | One static, read-only page showing a deployed vault's state | prototype; read-only, on Sepolia | [57](docs/EVIDENCE.md) |
| [Logos mini-app](modules/ferrata_bridge_ui/RESULTS.md), `modules/ferrata_bridge_ui/` | The RFP-022 mini-app as a Logos `ui_qml` module | prototype; on its devnet, and on a running LEZ sequencer (devnet) through `ferrata_bridge_module` in a headless Logos Core | [62, 86, 87, 103](docs/EVIDENCE.md) |
| [Release engineering](docs/release-engineering.md), `justfile`, `scripts/` | One command per job: `just bench`, `just m0`, `just release`, `just deploy-testnet` | built: release 0.1.0-rc.1 | [37, 49–54, 82](docs/EVIDENCE.md) |
| [Privacy suite](docs/privacy/RESULTS.md), `scripts/privacy/` | Milestone B2's privacy gate: 20 users' round trip, an adversary pairing each side | measured, also with real proofs | [23, 89](docs/EVIDENCE.md) |
| [Finality-rule analysis](docs/design/finality-rule/LEMMA.md), `docs/design/finality-rule/` | A model of LEZ v0.4 and Logos Blockchain, a written proof, a simulator | analysis; a simulator, not a network | [20](docs/EVIDENCE.md) |

Where each ran, the measurements, the command that re-checks each and the toolchains: [docs/components.md](docs/components.md).

## Documentation

- **LEZ program developers** who need an Ethereum fact inside their own transaction, by a public call or a
  private read: the [consumer guide](docs/guides/consumer-guide.md).
- **People moving ETH or a registered ERC-20** between Ethereum and LEZ, and developers wiring those flows into
  a Logos module: the [user guide](docs/guides/user-guide.md).
- **Operators** of the updater, the root importer, the checkpoint prover and relayer, and watchers: the
  [operator guide](docs/guides/operator-guide.md).
- **Deployers** standing up their own, independently configured deployment: the
  [deployer guide](docs/guides/deployer-guide.md).

The guides and the trust document were revised for delivery on 21 November 2026, and `docs/performance.md` reached its
second draft the same day; every statement cites the file it rests on, and each says what is not built. The
design notes, the trust and performance documents, the reviews and the rest: [docs/README.md](docs/README.md).

## Specified but not built

The design notes specify these; there is no code for them here. In short (in full, with what exists beside each:
[docs/not-built.md](docs/not-built.md)):

- **The bridge program on LEZ:** a transfer that credits a private recipient privately, the admin's own handover,
  the switch to a v0.4 token program when one is released; in the deployed program, the nullifier and holder
  accounts keyed by the configuration, which close the two tenant-isolation issues of 18 November
  ([row 136](docs/EVIDENCE.md)): they are built and tested in a second build of the program, which is not deployed
  ([row 143](docs/EVIDENCE.md)).
- **The attestation program:** consumers bound to the root id; a cost for forging a record (bonded backing a consumer
  can require, or anchoring to Ethereum's FFG finality: [docs/trust.md](docs/trust.md) §2.2, design only). (Chains of
  EIP-2935 hops are built: up to 32 in one transaction on real data, [row 145](docs/EVIDENCE.md); so is the F.8
  full-diff test, [row 152](docs/EVIDENCE.md).)
- **Rule (B):** the watchers' `monitor` reading attestations.
- **Daemons:** the relayer module's root imports on a running LEZ sequencer (the standalone daemon ran them:
  `e2e/runs/seqnet-robust-1/`), `monitor`'s client of the LEZ and Logos
  Blockchain nodes' RPCs, the A3 gate's three daemons on a running sequencer.
- **Logos modules:** Ethereum transactions through Logos Core's own modules beyond anvil and inside Basecamp (on anvil
  the two daemons' sends and the bridge module's user and admin sends go through Logos's send stack),
  LEZ through an unpatched `lez_core` (the bridge module's wallet session and, patched, the attestation module's reads
  and sends go through it), verified reads beyond the attestation
  module's.
- **The user's round trip on a LEZ sequencer through Logos Core's own modules:** inside Logos Basecamp the
  mini-app's round trip ran with LEZ through `lez_core`, its private claim and burn proven for real there (jobs the
  view polls, around Basecamp's 20 s cap on a view's call), but its Ethereum side read, signed and sent through a
  stand-in service there; through Logos Core's own modules it ran headless ([EVIDENCE](docs/EVIDENCE.md) row 199).
- **Logos Blockchain 0.3.5-rc.1:** the bridge and a t-of-n checkpoint over a zone of several sequencers on it (four
  rc3 sequencers ran a zone on it, [row 206](docs/EVIDENCE.md)), Logos's devnet itself, and a Bedrock era change;
  the fork digest's reading is built as a second build of the checkpoint guests and paid out on a local anvil, over a
  zone of one sequencer and one local validator ([rows 129, 130](docs/EVIDENCE.md)), and live on Sepolia through a vault
  pinning that build, with a zone of one sequencer and one validator we ran ([rows 141, 142](docs/EVIDENCE.md),
  [e2e/crown/README.md](e2e/crown/README.md)).
- **From the v3 specification:** rule (A)'s Δ_g of 3 h when participation is partial, the gas drop in the mini-app's
  claim and burn (the tool's claim and burn use the network's relayer by default on LEZ v0.4.0-rc1's sequencer,
  [row 258](docs/EVIDENCE.md); the relayer daemon's sealed credit, [row 205](docs/EVIDENCE.md); with real proofs through
  `ferrata-gasdrop` and crown-driver on rc3, [row 124](docs/EVIDENCE.md)).

## Not audited. Not for production.

No part of this repository has been audited. Four vaults are deployed on a public network, all on Sepolia, with
testnet parameters ([docs/sepolia.md](docs/sepolia.md)). On a LEZ network Logos runs only the light client's
experimental v0.3.1 port is deployed, on the public LEZ testnet ([row 150](docs/EVIDENCE.md)); none of the proposal's
v0.4 programs is: LEZ testnet 0.4 does not exist. LEZ v0.4.0 was released on 27 November 2026 (RISC Zero 3.1.0, so new program image ids, and new transaction and fee formats); everything here ran on its release candidates (rc1 to rc3, RISC Zero 3.0.5), and the port to v0.4.0 has not started. The attestation program and the bridge program at B1 run on a LEZ
v0.4.0-rc3 network we run that is reachable publicly ([rows 237, 238](docs/EVIDENCE.md)). The deployment for the day it opens
is one command, built for and rehearsed on our own LEZ v0.4.0-rc3 zone (not yet for v0.4.0) ([lez/testnet04/](lez/testnet04/README.md), [row
148](docs/EVIDENCE.md)). The prototypes exist to measure and to test the design; the audit is planned for the code the
proposal would deliver. A security review by an agent in a separate context (11 November 2026) is not an audit,
and Slither, Aderyn, Halmos, cargo-audit, cargo-deny and fuzzing are tooling, not an audit. The LEZ side's proofs are made
with RISC Zero 3.0.5, whose verifiers accept forged claims (RISC Zero's advisories of 11 November 2026); the Ethereum
side moved to 3.1.0 on 23 November 2026, and the LEZ side's move, the port to LEZ v0.4.0, has not started
([plan](docs/design/risc0-3.1-migration.md); [security note](docs/security/risc0-groth16-advisory-2026-11-21/README.md)). To report a
vulnerability: [SECURITY.md](SECURITY.md).

## Licence

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)), for
every directory, the imported ones included.

## How it was built

The code, the tests and the documents were drafted with coding agents (Claude Code) under the author's
design, decision log and acceptance tests; every commit was made with them.
[docs/how-it-was-built.md](docs/how-it-was-built.md) tells how, and what the commit trailers, the agents'
reviews and the author's own review do and do not cover.
