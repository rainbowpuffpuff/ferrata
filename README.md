# Ferrata

[![ci](https://github.com/rainbowpuffpuff/ferrata/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/rainbowpuffpuff/ferrata/actions/workflows/ci.yml)

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

Both parts live in this one repository and are tested against each other along the whole path: from a
finalized Ethereum header to a wrapped token on LEZ, and back to Ethereum. It was written as the evidence for a
proposal to [Logos RFP-022](https://github.com/logos-co/rfp) (a privacy-preserving wrapped ERC-20 and ETH bridge
for LEZ) that also covers RFP-021 (trustless Ethereum state attestation for LEZ).

What is here are prototypes and their measurements. **Not audited. Not for production.**

## Start here

| To | Read |
|---|---|
| see every result, with the command that re-checks it | [docs/EVIDENCE.md](docs/EVIDENCE.md) |
| check the live Sepolia round trip yourself | [docs/guides/verify.md](docs/guides/verify.md) |
| find where each RFP requirement is met | [docs/traceability.md](docs/traceability.md) |
| see the status of each requirement | [docs/requirements-status.md](docs/requirements-status.md): of the 59 and 68 hard requirements of RFP-021 and RFP-022, 17 and 27 shown, 35 and 37 in part, 4 and 2 not yet, 3 and 2 waiting on Logos (our own strict count, 18 November 2026) |
| see the light client on the public LEZ testnet's version (v0.3.1) | [lez-v03/](lez-v03/README.md): an experimental, unaudited port; a finality update in one transaction (6,647,546 gas, 66.5% of the cap), run on a v0.3.1 sequencer we started with the image `8d3b59ff…` ([EVIDENCE](docs/EVIDENCE.md) row 131), the reviewed image the prepared public-testnet run deploys; not yet on the public testnet. This branch's image `bac768a8…` adds a committee handoff in one transaction, sound only under the light client's honest-supermajority assumption (the next committee adopted by its summary; at most 6,737,794 gas on the mainnet replay and 6,682,541 on Sepolia's 32, in v0.3.1's block application in our harness, [EVIDENCE](docs/EVIDENCE.md) row 137); that image has not run on a sequencer |
| see the bridge's inbound leg and burns on LEZ v0.3.1 | [lez-v03-bridge/](lez-v03-bridge/README.md): an experimental, unaudited port; on a v0.3.1 sequencer we started, a private claim and a private burn proven for real; it already carries the fix of the two v0.4 known issues of [row 136](docs/EVIDENCE.md); reviewed twice ([EVIDENCE](docs/EVIDENCE.md) rows 132, 133) |
| see the leg back from the public LEZ testnet (a testnet stand-in) | [lez-v03-return/](lez-v03-return/README.md): **a stand-in, not the design**: the unchanged vault and real release proofs, the burn root attested by keys we hold (no checkpoint proof can exist for a v0.3.1 zone today); the attestor signs only after it has compared the burns with the bridge's state on the LEZ node at a Finalized block; rehearsed on a fork of Sepolia ([run](e2e/runs/testnet-return-fork-2/README.md)), not sent; and how far a real checkpoint gets on that network's own data ([EVIDENCE](docs/EVIDENCE.md) rows 134, 135) |
| read the design | [attestation](docs/design/attestation.md), [bridge](docs/design/bridge.md), [what LEZ v0.4 decides](docs/design/lez-v0.4-findings.md) |
| know who is trusted for what | [docs/trust.md](docs/trust.md) |
| see what is not built yet | [Specified but not built](#specified-but-not-built) |

Every document, one line each: [docs/README.md](docs/README.md).

## What has run

| What | Result | Where it ran | EVIDENCE rows |
|---|---|---|---|
| The light-client core | 52 of 52 official `sync` cases, Altair to Gloas; 33 of them step by step against Helios as a differential oracle | laptop CPU, CI | [1](docs/EVIDENCE.md) |
| The light client kept running ([the lab](lez/lab/README.md)) | two instances following Sepolia and mainnet on a LEZ zone, unattended, a session every two hours; the record replays offline, and each session's finalized roots equal a second beacon provider's | a LEZ v0.4.0-rc1 sequencer we run, one validator | [21](docs/EVIDENCE.md) |
| The round trip, live on Sepolia | Ethereum to LEZ and back on the final programs, one public and one private crossing each way, through FerrataVault v5; anyone can check it from the committed records | Sepolia; a LEZ devnet we ran | [91, 92](docs/EVIDENCE.md) |
| The return leg with several sequencers | the bridge program on a zone of four LEZ sequencers with an attestor beside each, its rule (B) checkpoint proven into the vault, and a rotation its committee proposed | a zone we ran; the vault on a local anvil, a stand-in beacon chain | [100, 101](docs/EVIDENCE.md) |
| The return leg on LEZ v0.4.0-rc3 | the checkpoint guests' second build reads Logos Blockchain 0.3.5-rc.1's fork digest (the deployed ids unchanged); its rule (B) checkpoint over an rc3 zone's real blocks proven for real, wrapped and accepted by the vault, both burns released | rc3's sequencer (one) and node (one validator); CPUs of a Modal box; the vault on a local anvil | [129, 130](docs/EVIDENCE.md) |
| M0 check 4: real proofs on the newest LEZ tag | the private update, attestation, claim and burn proven for real and applied by LEZ v0.4.0-rc3's own sequencer in final blocks (`just m0`: GO) | CPUs of a Modal box; rc3's sequencer and node | [123](docs/EVIDENCE.md) |
| M0 check 2: the gas drop | a claimant with no native LEZ: the relayer's sealed credit for its receipt's `N`, then its private claim on the final bridge sweeps the credit and pays its fee from it, real proofs, applied by LEZ v0.4.0-rc3's sequencer in final blocks; the same claim without the credit refused (`just m0`: GO) | CPUs of a Modal box; rc3's sequencer and node | [124](docs/EVIDENCE.md) |
| The privacy gate on real proofs | 20 users' round trip, every private claim, burn and release proven; an adversary pairing each side | one L4 GPU and CPUs on Modal | [89](docs/EVIDENCE.md) |
| The reproduction container | five checks and the rebuild of all 14 guest programs against [`docs/image-ids.md`](docs/image-ids.md): 5 of 5 | Modal; Docker on a fresh x86-64 machine ([record](scripts/reproduce/runs/2026-11-12-docker-aws/README.md)) | [90](docs/EVIDENCE.md) |

[docs/EVIDENCE.md](docs/EVIDENCE.md) is the index: one page with every piece of evidence, its numbers, where
it ran, whether its proofs were real or in dev mode, what is stand-in data, and the command that re-checks it.

## Check it yourself

Commands run from the repository root.

The live Sepolia round trip, checked from its committed records, read-only (Python's standard library only; it
asks a public endpoint unless you give your own with `--rpc URL` or `SEPOLIA_RPC_URL`):

```sh
python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-3
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
the vault reviewers should check. The LEZ side was a devnet we ran (one validator): no public LEZ network runs
any of it. The first live run, `crown-live-1` (11 November, EVIDENCE rows 41–42), made the same round trip on
earlier versions of these programs.

The runs in full, the three vaults deployed on Sepolia and what each check proves:
[docs/sepolia.md](docs/sepolia.md) and [docs/guides/verify.md](docs/guides/verify.md).

## Components and their status

| Component | What it is | Status | EVIDENCE rows |
|---|---|---|---|
| [Light-client core](crates/ferrata-core/README.md), `crates/` | Ethereum sync-committee light client and execution-layer proofs, `no_std` Rust | built and tested | [1, 40, 69](docs/EVIDENCE.md) |
| [Guest benchmarks](bench/README.md), `bench/` | The core inside a RISC Zero guest: cycle counts on real mainnet data | measured | [2](docs/EVIDENCE.md) |
| [LEZ program](lez/RESULTS.md), `lez/` | The light client as a LEZ program; accounts, storage slots and logs attested under its records | prototype; on LEZ sequencers we run | [3–6, 21, 22, 32, 38, 39, 70–73, 88](docs/EVIDENCE.md) |
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
| [Updater](lez/updater/README.md), `lez/updater/` | `ferrata-updater`, the light-client updater daemon | prototype; recorded data, the harness; 8 h on LEZ sequencers of two zones following Sepolia live, 74 updates applied, 56 on the chains kept ([record](lez/updater/results/live-sepolia/)) | [79–81, 99](docs/EVIDENCE.md) |
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

The guides are first drafts of 11 November 2026; every statement cites the file it rests on, and each says
what is not built. The design notes, the trust and performance documents, the reviews and the rest:
[docs/README.md](docs/README.md).

## Specified but not built

The design notes specify these; there is no code for them here. In short (in full, with what exists beside each:
[docs/not-built.md](docs/not-built.md)):

- **The bridge program on LEZ:** a transfer that credits a private recipient privately, the admin's own handover,
  the switch to a v0.4 token program when one is released; the nullifier and holder accounts keyed by the
  configuration, which close the two tenant-isolation issues of 18 November ([row 136](docs/EVIDENCE.md)).
- **The attestation program:** consumers bound to the root id, a chain of several EIP-2935 hops under a Gloas
  record, the F.8 test.
- **Rule (B):** the watchers' `monitor` reading attestations.
- **Daemons:** the relayer's root imports on a running LEZ sequencer, `monitor`'s client of the LEZ and Logos
  Blockchain nodes' RPCs, the attestation side's `monitor`, the A3 gate's three daemons on a running sequencer.
- **Logos modules:** Ethereum transactions through Logos Core's own modules beyond the two daemons on a devnet,
  LEZ through `lez_core` beyond the bridge module's wallet session, verified reads beyond the attestation
  module's, a published catalog.
- **The user's round trip on a LEZ sequencer through Logos Core's own modules:** inside Logos Basecamp the
  mini-app's round trip ran with LEZ through `lez_core`, its private claim and burn proven for real there (jobs the
  view polls, around Basecamp's 20 s cap on a view's call), but its Ethereum side still reads, signs and sends
  through a stand-in service.
- **Logos Blockchain 0.3.5-rc.1:** a vault for such a zone where it runs, a zone of several sequencers on it, and a Bedrock
  era change; the fork digest's reading is built as a second build of the checkpoint guests and paid out on a local anvil,
  over a zone of one sequencer and one local validator ([rows 129, 130](docs/EVIDENCE.md)).
- **From the v3 specification:** rule (A)'s Δ_g of 3 h when participation is partial, the gas drop's sealed credit in the relayer daemon
  (it ran through `ferrata-gasdrop` and crown-driver on LEZ v0.4.0-rc3, [row 124](docs/EVIDENCE.md)) and for burns.

## Not audited. Not for production.

No part of this repository has been audited. Three vaults are deployed on a public network, all on Sepolia, with
testnet parameters ([docs/sepolia.md](docs/sepolia.md)). No public LEZ network runs any of it: LEZ testnet 0.4
does not exist. The prototypes exist to measure and to test the design; the audit is planned for the code the
proposal would deliver. A security review by an agent in a separate context (11 November 2026) is not an audit,
and Slither, Aderyn, Halmos, cargo-audit, cargo-deny and fuzzing are tooling, not an audit. To report a
vulnerability: [SECURITY.md](SECURITY.md).

## Licence

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)), for
every directory, the imported ones included.

## How it was built

The code, the tests and the documents were drafted with coding agents (Claude Code) under the author's
design, decision log and acceptance tests; every commit was made with them.
[docs/how-it-was-built.md](docs/how-it-was-built.md) tells how, and what the commit trailers, the agents'
reviews and the author's own review do and do not cover.
