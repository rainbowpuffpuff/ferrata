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
programs against [`docs/image-ids.md`](docs/image-ids.md), ending in one line per check and a verdict (a 10 GB image;
half an hour on 16 CPUs). On a Modal account (`pip install modal`, then `modal setup`), the route we have run:

```sh
modal run --detach scripts/reproduce/modal_app.py
```

With Docker on x86-64, `scripts/reproduce/run.sh` builds the same Dockerfile and runs the same `check.sh` in it: run
once under Docker on a fresh x86-64 Linux machine, 5 of 5 in 22 minutes ([`scripts/reproduce/runs/2026-11-12-docker-aws/`](scripts/reproduce/runs/2026-11-12-docker-aws/README.md)). What it pins and what
it leaves out: [`scripts/reproduce/README.md`](scripts/reproduce/README.md).

Commands run from the repository root. The first two need only Rust (with curl and tar for the vectors)
and Python 3.

The light-client core on the official consensus vectors and real mainnet and Sepolia data (native tests,
a laptop is enough):

```sh
scripts/fetch-vectors.sh && cargo test --workspace --locked
```

(163 tests in 25 binaries; under two minutes on a 16-core machine with an empty cargo registry.)

The live Sepolia round trip on the final programs, checked from its committed records, read-only (Python's
standard library only; it asks a public endpoint unless you give your own with `--rpc URL` or `SEPOLIA_RPC_URL`;
`e2e/runs/crown-live-1` is the first live run, on earlier programs):

```sh
python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-3
```

The rest needs RISC Zero's toolchain (`rzup`, r0vm 3.0.5), Foundry (forge 1.3.6, solc 0.8.26) and, for
the Groth16 wraps, Docker on x86-64. The vault's suite (install the pinned dependencies first:
[`vault/README.md`](vault/README.md)):

```sh
cd vault && FOUNDRY_OFFLINE=false forge build && forge test
```

`vault/foundry.toml` keeps forge offline, so the first build must be allowed to fetch solc 0.8.26
(`FOUNDRY_OFFLINE=false`; without it forge stops at "can't install missing solc 0.8.26 in offline mode"); later
runs need no network. Expected: 464 passed and 1 skipped, the privacy suite's Ethereum half, which replays the
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
thread; `vault_consumer` alone runs six minutes without printing; about 13 minutes with `--test-threads 8`). The
tests rewrite the 21 files they write in `lez/results/`; the committed ones were recorded from this tree's programs
([`docs/image-ids.md`](docs/image-ids.md); bridge image `ccde6171…`) on 12 November 2026, so git shows them unchanged
(the Stage 1, 4 and 5 tables that the `tables` binary of `lez/RESULTS.md` §8 writes are of the earlier images that
file names). The rest of the setup, the real proofs and the tables: [`lez/RESULTS.md`](lez/RESULTS.md) §8.

The larger jobs have one command each (`just m0`, `just bench`, `just release`:
[docs/release-engineering.md](docs/release-engineering.md)); they need [`just`](https://github.com/casey/just), or
run the script each recipe wraps, named there (`just m0` is `python3 -I scripts/m0.py`, which only reads files).
Every component's re-check command is in [docs/components.md](docs/components.md).

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

On 12 November 2026 (EVIDENCE rows 91–92) the whole round trip ran on the final programs of
[`docs/image-ids.md`](docs/image-ids.md), Sepolia to LEZ and back, as `crown-live-3`, the second live round trip:
FerrataVault v5 deployed in mode (B) at [`0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`](https://sepolia.etherscan.io/address/0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c),
the vault reviewers should check; LEZ's light client started from a fresh Sepolia checkpoint, inside the programs'
weak-subjectivity bound; a public and a private deposit, final on Sepolia, imported by the bridge program on a LEZ
v0.4.0-rc1 devnet from Sepolia's own finality updates and claimed, the private one with a real proof; a public and a
private burn; a rule (B) checkpoint over the devnet's blocks proven on a GPU, its Groth16 seal verified on Sepolia;
both releases paid. 13 transactions, 74.9M gas, two starts (one stop on the RPC endpoint's lagging reads, before the
vault, resumed), verify 14 of 14 ([`e2e/runs/crown-live-3/`](e2e/runs/crown-live-3/README.md)). A start before it,
`crown-live-2`, stopped at its first stage with nothing sent: the bound refused its 38-day-old checkpoint
([`e2e/runs/crown-live-2/`](e2e/runs/crown-live-2/README.md)). The first, `crown-live-1` (11 November, EVIDENCE rows
41–42), made the same round trip on earlier versions of these programs (`integration5`'s, built before guest builds
were path-independent) through the vault
[`0xfE3199fa2634413CbAC317371076Cae3EB2d9E54`](https://sepolia.etherscan.io/address/0xfE3199fa2634413CbAC317371076Cae3EB2d9E54)
([`e2e/runs/crown-live-1/`](e2e/runs/crown-live-1/README.md)). The LEZ side was a devnet we ran (one validator): no
public LEZ network runs any of it ("Not audited. Not for production.", below).

Anyone can re-check it from the committed records against a JSON-RPC endpoint of their choice:
`python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-3` (13 passed on two public endpoints; `e2e/runs/crown-live-1`
the same). It checks every recorded transaction, the deployment's CREATE addresses, the vault's immutables (image ids,
configId, genesisState, rule (B), verifier, linked registry), the deposits' commitments, the vault storage LEZ imported
(Merkle proofs against Sepolia's final headers), the checkpoint journal against the LEZ records and its seal through
the vault's own verifier, finalization and releases (EVIDENCE row 92). The vault's programs rebuild from this tree:
`scripts/reproduce-image-ids.sh build`, then `compare --vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c` gives all
of its ids (6 of 6 crown programs, the vault's two immutables among them, and 14 of 14 guests).
[docs/guides/verify.md](docs/guides/verify.md) says what each check proves and what it still trusts, and how to
rebuild the programs to their image ids.

## Components and their status

| Component | What it is | Status | EVIDENCE rows |
|---|---|---|---|
| [Light-client core](crates/ferrata-core/README.md), `crates/` | Ethereum sync-committee light client and execution-layer proofs, `no_std` Rust | built and tested | [1, 40, 69](docs/EVIDENCE.md) |
| [Guest benchmarks](bench/README.md), `bench/` | The core inside a RISC Zero guest: cycle counts on real mainnet data | measured | [2](docs/EVIDENCE.md) |
| [LEZ program](lez/RESULTS.md), `lez/` | The light client as a LEZ program; accounts, storage slots and logs attested under its records | prototype; on LEZ sequencers we run | [3–6, 21, 22, 32, 38, 39, 70–73, 88](docs/EVIDENCE.md) |
| [Bridge program](lez/BRIDGE-RESULTS.md), `lez/` | The RFP-022 bridge on LEZ: `ImportRoot`, private claims and burns, fees, caps, freezes | prototype; real proofs, a LEZ devnet | [8–10, 28, 30, 31, 33, 38, 74, 75, 78, 88, 91](docs/EVIDENCE.md) |
| [Inbound crossing, live](lez/live/inbound/README.md), `lez/live/inbound/` | Ethereum to LEZ on a running LEZ sequencer and Logos Blockchain node | run: a Sepolia deposit claimed on LEZ | [11, 27, 41, 91](docs/EVIDENCE.md) |
| [GPU proving](gpu-prover/RESULTS.md), `gpu-prover/` | A Modal app proving checkpoints, the LEZ update and the private attestation, claim and burn on a GPU | measured | [5, 10, 16, 17, 24](docs/EVIDENCE.md) |
| [Logos modules](modules/RESULTS.md), `modules/` | `ferrata_attest_module` (with `advance`, the updater's job planning) and `ferrata_bridge_module` (with the private claim, the root import, registry reads, the release rule, the release guest and the mini-app's calls), on the `ferrata` tool's and the updater's own code (`cli/wallet`, `lez/advance`), the attestation module's Ethereum reads and the bridge module's `import_root` also through Logos Core's `eth_rpc_module`; the four daemons as modules (`ferrata_attestor_module`, `ferrata_monitor_module`, `ferrata_updater_module`, `ferrata_relayer_module`); a catalog layout | built; run in a headless Logos Core | [7, 59–61, 93–98, 103, 104, 106–110](docs/EVIDENCE.md) |
| [Vault](vault/README.md), `vault/` | `FerrataVault` v5, the immutable Ethereum vault (Solidity), and its linked library | prototype; deployed on Sepolia | [12–14, 25, 30, 35, 41, 54, 91](docs/EVIDENCE.md) |
| [Bridge core and release proof](bridge/README.md), `bridge/` | Commitments, trees, nullifiers; the release and claim guests | prototype; real proofs | [19, 30, 33, 77, 78, 88](docs/EVIDENCE.md) |
| [Checkpoint program](checkpoint/README.md), `checkpoint/` | The LEZ to Ethereum checkpoint proof: re-hashes LEZ blocks, collects burns, checks rule (A) or (B) | prototype; proven on real LEZ blocks | [15, 16, 24, 29, 78, 91](docs/EVIDENCE.md) |
| [Return leg, end to end](e2e/README.md), `e2e/` | The checkpoint program, the release program and the v3 vault in one fixture | run; the LEZ side a stand-in | [18, 25, 26, 41, 91](docs/EVIDENCE.md) |
| [Round trip in CI](e2e/ci/README.md), `e2e/ci/` | One deposit there and back on one machine, every step asserted, in GitHub Actions | tested on the build box; not yet on GitHub | [76, 77](docs/EVIDENCE.md) |
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

## Evidence

[docs/EVIDENCE.md](docs/EVIDENCE.md) is the index: one page with every piece of evidence, its numbers, where
it ran, whether its proofs were real or in dev mode, what is stand-in data, and the command that re-checks it.
The latest additions, of 11 to 13 November 2026, one line each (in full:
[EVIDENCE.md, "The latest additions"](docs/EVIDENCE.md#the-latest-additions-of-11-to-13-november-2026)):

- **Merged on 13 November 2026** (EVIDENCE rows 100–110; `zone-bridge`, `updater-module-seq`, `relayer-module`,
  `logos-core-io`, `miniapp-sequencer`): the bridge program on a zone of four LEZ sequencers, its rule (B) checkpoint
  proven into the vault and a rotation its committee proposed; the user's private round trip on a running LEZ
  sequencer through the `ferrata` tool and through the mini-app in a headless Logos Core, its private claim and burn
  proven for real; the updater module on a running LEZ sequencer; the relayer's root imports and its module;
  Ethereum reads through Logos Core's `eth_rpc_module`; all re-checked on the merged tree
  ([`modules/results/final-2026-11-13/`](modules/results/final-2026-11-13/README.md)).
- **Live on 12–13 November 2026, merged on 13 November** (EVIDENCE rows 99, 80, `updater-live`): the updater on
  running LEZ sequencers following Sepolia live for eight hours on two zones, 74 updates applied (56 on the chains
  kept) across a period boundary; merged into the daemon the updater module drives, every test and gate figure the
  same.
- **Merged on 13 November 2026** (EVIDENCE rows 93–98, `bridge-module-complete` and `modules-more`): the five Logos
  modules on one tree, the attestation and bridge modules completed on the tools' own code and the three daemons as
  modules; every job through them in a headless Logos Core and from the local catalog.
- **Live on 12 November 2026** (EVIDENCE rows 91–92, `crown-live-3`): the round trip on the final programs on
  Sepolia, verify 14 of 14; anyone's check from the records 13 of 13 on two public endpoints; the vault's image ids
  rebuilt from this tree.
- **Merged on 12 November 2026** (EVIDENCE rows 84–88, `integration8`): the `ferrata` tool, private by default; the
  mini-app's private round trip on its devnet, also with real proofs; the padded private proofs on Apple Silicon.
- **Since 11 November 2026** (EVIDENCE rows 28–30): the bridge program's freezes and caps on time from its own
  attestation program's records; rule (B) also checks the tip's producer inscription; the security review's
  findings fixed on both sides.
- **Merged on 11 November 2026** (EVIDENCE rows 43–75, `integration7`): the weak-subjectivity bound, successors with
  the root id, the vault's deposit leaves on real Sepolia data, the reference consumer's rules 5 to 7, the bridge
  program's B2 controls, rule (B) with t of n attestors on a real four-sequencer zone.
- **Added in the seventh pass** (EVIDENCE rows 35–40): Slither, Aderyn, Halmos, cargo-audit and cargo-deny, every
  finding triaged; the crown run's programs rebuilt to their image ids; the padded private proofs at 8 and 16 CPUs; a
  second consumer; fuzzing of the core and the programs; an independent review of the core.

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
- [docs/REVIEW-vault.md](docs/REVIEW-vault.md) and [docs/REVIEW-guests.md](docs/REVIEW-guests.md): the same for
  the vault and its library (52 rows) and for every guest program's entry points and checks (55 rows); drafted on
  12 November 2026 with an agent review, not yet signed.

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
- rule (B): the watchers' `monitor` reading attestations (the four-sequencer zone has run the bridge program, both
  burns, their releases through a vault on a local anvil and a rotation its committee proposed,
  `checkpoint/attestor/RESULTS.md` §8, §9; `ferrata_attestor` is built as a program and as a Logos module, which ran
  beside each sequencer of a second run of that zone, `modules/RESULTS.md`);
- of the relayer, its root imports on a running LEZ sequencer, and its Logos module's LEZ side through Logos Core's
  own modules (the daemon `ferrata-relayer`, its wrapping, the gas drop, its root imports and its Logos module are
  built; the imports and the module ran on the mini-app's devnet, whose zone the module reaches, and on a fork of
  Sepolia, `modules/results/relayer-2026-11-13/`); the watchers' `monitor`'s client of the LEZ and Logos Blockchain nodes' RPCs (the
  program and its Logos module are built, `monitor/`, `modules/ferrata_monitor_module`; node views come through
  an adapter); the attestation side's `monitor` (records against several providers; the weak-subjectivity alarm);
- in the Logos modules (`modules/README.md`, "Left out"; [`modules/LOGOS-CORE-MODULES.md`](modules/LOGOS-CORE-MODULES.md)):
  transactions through Logos Core's own modules (`keystore_module`, `tx_sender_module`, `lez_core`: the
  modules build messages without signing or sending them; with our builder pin `keystore_module` does not
  build, and `lez_core` pins LEZ v0.3.1), verified reads (`verified_proxy_module`: public networks only, no
  `eth_getProof`), beacon data through `beacon_api_module` (the attestation module's Ethereum reads and the
  bridge module's `import_root` go through `eth_rpc_module`, rows 108, 109); the private claim's private execution and proof are the LEZ wallet's: the module builds and checks
  `ClaimPrivately`; the release's Groth16 wrap is the relayer's), and a published catalog (laid out,
  dry-run and installed from on 127.0.0.1 in `modules/catalog/`); the A3 gate's three daemons on a running
  sequencer (`ferrata-updater` is built as a binary and as a Logos module that drives the same daemon with either
  backend, `lez/updater/`, `modules/ferrata_updater_module`: the in-process LEZ chain, or a running LEZ sequencer
  through `lez/updater/sequencer/`'s library, which the module ran inside a headless Logos Core on a crown zone
  following Sepolia, EVIDENCE row 104, and the binary for eight hours, row 99; the gate's three daemons run in
  process, row 80);
- the user's round trip on a LEZ sequencer through Logos Core's own modules: on a running LEZ sequencer and
  node (a devnet with the final programs, the vault on a local anvil) the private round trip has run through the
  `ferrata` tool's sequencer path and through the mini-app driving `ferrata_bridge_module` inside a headless
  Logos Core (`e2e/seqnet/RESULTS.md`, EVIDENCE rows 102, 103), but the mini-app's calls (the module's `wallet`
  method) read the chains, sign and send through a stand-in service (`seqnet-chain`: LEZ's wallet library, a
  keystore, the relayer) instead of `eth_rpc_module`, `keystore_module`/`tx_sender_module` and `lez_core` (the
  module's `import_root` reads the vault through `eth_rpc_module`, row 109; the `wallet` method does not use it
  yet), the view ran under Qt Quick Test rather than in Basecamp, and that devnet's Ethereum side is anvil
  finalized by a stand-in committee, with the vault tests' mock verifier (no Groth16 proof) and a stand-in for the
  gas drop;
- in the attestation program: consumers bound to the root id (the bridge, `ferrata_holdings` and the
  reference consumer still pin an instance); a chain of several EIP-2935 hops under a Gloas record (one hop
  is attested); the F.8 test (`docs/design/attestation.md` §6.5, §15).
- for Logos Blockchain 0.3.5-rc.1 (the node LEZ v0.4.0-rc3 pins, which Logos's devnet runs): the checkpoint
  guests' reading of a transaction's fork digest, so checkpoints over such a zone
  ([`lez/results/v0.4.0-rc3/README.md`](lez/results/v0.4.0-rc3/README.md); today checkpoint-core refuses that
  framing, `checkpoint/crates/checkpoint-core/tests/bedrock_0_3_5.rs`).

Changes in [the v3 specification](docs/design/spec/bridge-spec-v3-changes.md) that the prototypes do not
implement yet:
- rule (A) (D13): Δ_g of 3 h when participation is partial (a deployment parameter);
- roles (D13): the deployment script that renounces the emergency stop's owner is run in fork
  rehearsals only (the vault's side, the watchers, the 14-day role change, keyless `retire()` and the
  cap floor are built);
- inbound (D13): the gas drop's sealed credit to a private one-time account and its sweep inside the
  private claim (LEZ's wallet; the gas-drop receipt binding `(N, oneTimeAddress, relayer)` is built and the
  relayer checks it; refunds drawing on the release cap are in the vault; `ImportRoot` attesting slots 0 and 1, and claims refusing
  `leafIndex >= count`, are in the bridge program).

## Not audited. Not for production.

No part of this repository has been audited. Three vaults are deployed on a public network, all on Sepolia.
FerrataVault v5 of `vault/` (rule (B)) with the final programs is at
[`0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`](https://sepolia.etherscan.io/address/0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c)
(created by transaction `0xa67bfa471825f0dad32e8243444700635a08f39d85b328672f5959b122c12822`, block 12,119,409), with
its emergency stop (owner renounced) and library: the live round trip of 12 November 2026 made two deposits, a
checkpoint and two releases through it ([`e2e/runs/crown-live-3/`](e2e/runs/crown-live-3/README.md), EVIDENCE row
91). The same v5 vault with the first run's programs is at
[`0xfE3199fa2634413CbAC317371076Cae3EB2d9E54`](https://sepolia.etherscan.io/address/0xfE3199fa2634413CbAC317371076Cae3EB2d9E54)
(transaction `0xee91c261265ac3ada50b525edb35a1faa32bb5bb4ffa2b1d433f5a278d9808a3`, block 12,111,048): the live round
trip of 11 November 2026 ([`e2e/runs/crown-live-1/`](e2e/runs/crown-live-1/README.md), EVIDENCE row 41). Both have
testnet parameters (a 1-day revocation window, a 120 s release delay, no caps, no watchers, a freeze authority without
a key holder). Before them, the v3 vault of the end-to-end fixture was deployed on Sepolia at
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
