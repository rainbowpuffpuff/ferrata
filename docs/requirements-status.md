# Requirement status, row by row

The status of every row of [traceability.md](traceability.md): 64 rows for RFP-021 and 73 for RFP-022. The
classification is the project's own, made on 18 November 2026 from the matrix as it stands at this commit, and
it is strict on purpose. A row is **shown** only if every clause of the requirement, as the RFP words it, is
demonstrated today by a named test or a recorded run on real code: LEZ v0.4 release candidates' own state
machine (the in-process harness) or one of their sequencers for the LEZ side, a local EVM or Sepolia for the
Ethereum side. Proofs in RISC Zero dev mode count only where the requirement is not about proving. A row is
**in part** when a clause is demonstrated and another is not, when the behaviour is narrower than the RFP
asks, when the only run is in the native core or in an in-process stand-in for a component the requirement
names, or when a required document exists only as a first draft. **not yet** means no clause is demonstrated,
or the row is not offered. **waits on Logos** means the requirement names LEZ testnet 0.4 or another upstream
component that is not released; the reason says what is shown instead. A trust assumption or a missing upstream
library that the matrix states as a condition is given in the reason and does not by itself lower a status.
Where a choice was open, the weaker status was taken. Every sequencer named here is one we ran; none is a
public LEZ network. Numbers in the Evidence column are rows of [EVIDENCE.md](EVIDENCE.md); nothing here is
audited.

## Counts

Hard requirements:

| RFP | shown | in part | not yet | waits on Logos | total |
|---|---:|---:|---:|---:|---:|
| RFP-021 | 17 | 35 | 4 | 3 | **59** |
| RFP-022 | 27 | 37 | 2 | 2 | **68** |

Soft requirements, counted apart (5 for each RFP): RFP-021 2 shown, 2 in part, 1 not yet; RFP-022 1 shown, 2 in part, 2 not yet (both not offered).

By section, hard requirements:

| RFP | Section | shown | in part | not yet | waits on Logos | total |
|---|---|---:|---:|---:|---:|---:|
| RFP-021 | Functionality | 8 | 6 | 0 | 1 | 15 |
| RFP-021 | Usability | 1 | 4 | 0 | 1 | 6 |
| RFP-021 | Reliability | 6 | 4 | 2 | 0 | 12 |
| RFP-021 | Performance | 1 | 8 | 0 | 0 | 9 |
| RFP-021 | Supportability | 0 | 8 | 2 | 1 | 11 |
| RFP-021 | Verification Security | 1 | 5 | 0 | 0 | 6 |
| RFP-022 | Functionality | 11 | 6 | 0 | 0 | 17 |
| RFP-022 | Usability | 2 | 7 | 0 | 1 | 10 |
| RFP-022 | Reliability | 5 | 9 | 0 | 0 | 14 |
| RFP-022 | Performance | 2 | 5 | 0 | 0 | 7 |
| RFP-022 | Supportability | 0 | 2 | 1 | 1 | 4 |
| RFP-022 | Security | 3 | 5 | 1 | 0 | 9 |
| RFP-022 | Privacy Preservation | 4 | 3 | 0 | 0 | 7 |

## RFP-021: Trustless Ethereum State Attestation

### Functionality

| ID | Status | Why | Evidence |
|---|---|---|---|
| F.1 | shown | Init and updates settle in the harness and on rc1 and rc3 sequencers we ran; real proofs applied; a seven-day run. | 6, 21, 123; `lez/results/stage1-replay.md` |
| F.2 | in part | Records exist only for finalised headers (213 otherwise) through LEZ; a forged finality branch is refused in native step tests only. | 39; `crates/ferrata-spectests/tests/steps.rs`; `lez/results/holdings.md` |
| F.3 | shown | Account, storage slot and logs attested through the program on mainnet and Sepolia blocks; 32 refusals; roots only from the record. | 3, 71, 72; `lez/results/stage4-attest.md`; `lez/results/stage4-attest-refusals.md` |
| F.4 | in part | `=`, `≥`, `>`, `<` and a relation run through the program from the consumer's request; `≠` and `≤` not run through it. | `lez/harness/tests/attest_public.rs`; `lez/results/stage4-attest-refusals.md` |
| F.5 | shown | One address on mainnet and Sepolia: each statement attested under its chain, the other chain's refused (140), in the harness. | 4; `lez/harness/tests/sepolia.rs`; `lez/results/stage5-attest-refusals.md` |
| F.6 | shown | Identifiers returned by the program equal a second derivation; the reference consumer keys its nullifier on them. | `lez/harness/tests/attest_native.rs`; `lez/harness/tests/vault.rs`; `vectors/acc-leaf.json` |
| F.7 | waits on Logos | Names a testnet. Shown instead: same-transaction consumption in the harness and on rc1 and rc3 sequencers we ran. | 3, 27, 91, 123; `lez/results/stage4-attest-consumer.md` |
| F.8 | in part | Private attestation proven for real and applied on rc3; byte searches find no query or account; the full-diff test is not built. | 5, 73, 123; `lez/harness/tests/attest_private.rs` |
| F.9 | in part | Parameters fixed at Init; `verify_checkpoint` ran against Sepolia providers from Ferrata's module, not as an API of the Logos Ethereum module. | 69, 70, 105, 117 |
| F.10 | in part | Bound and successors run in the harness; a stale checkpoint was refused live; the migration text is a first draft. | 69, 70, 91; `docs/guides/consumer-guide.md` §9 |
| F.11 | shown | No step checks its sender; three updaters with their own keys advanced the same instances in the harness. | 80; `lez/harness/tests/private_placement.rs`; `lez/updater/tests/recorded.rs` |
| F.12 | in part | Parties listed in a first-draft trust document; lying sources and an adversarial updater did not block; no test over every party. | 79, 80; `docs/trust.md` §1, §7 |
| F.13 | shown | Re-Init refused (203) in the harness and on sequencers; program deployed immutable through LEZ's loader; recovery by a fresh instance. | 6, 21, 91, 112; `lez/results/stage1-refusals.md` |
| F.14 | shown | One program followed mainnet and Sepolia through the Gloas fork, in the harness and side by side on the lab's zone. | 4, 21; `lez/results/stage5-sepolia-replay.md` |
| F.15 | shown | Two instances side by side on the lab's zone; another instance's job id and request are refused (205, 211). | 21, 70; `lez/results/stage1-refusals.md`; `lez/results/stage4-attest-refusals.md` |

### Usability

| ID | Status | Why | Evidence |
|---|---|---|---|
| U.1 | in part | Module loaded in a headless Logos Core, Ethereum reads through `eth_rpc_module`; LEZ reads and sends not through Logos Core's modules. | 59, 60, 93, 108, 116 |
| U.2 | in part | CLI attests, dry-runs with the program's codes, advances and reads configuration against the in-process chain only, not a sequencer. | 56; `lez/ATTEST-CLI.md` |
| U.3 | in part | Daemon and module followed Sepolia on sequencers we ran; operator journey is a first draft; Ethereum data partly outside Logos Core. | 79, 81, 97, 99, 104, 118 |
| U.4 | waits on Logos | SPEL's release targets LEZ v0.2.4. Shown instead: a hand-written IDL in SPEL's format that SPEL's crate parses and decodes. | 55, 58; `idl/check.py` |
| U.5 | in part | Codes through the program for most listed failures; no recorded refusal of a committee handoff through LEZ; messages in the harness CLI. | 39, 56, 69, 70; `lez/results/stage4-attest-refusals.md` |
| U.6 | shown | ERC-20 `Transfer` and native-ETH deposit consumers credit once in the harness, public and private claims, on real mainnet and Sepolia data. | 3, 73; `lez/harness/tests/attest_public.rs`; `lez/harness/tests/vault_consumer.rs` |

### Reliability

| ID | Status | Why | Evidence |
|---|---|---|---|
| R.1 | shown | Step API equals `process_update` on 52 sync cases and the mainnet replay; vectors identical under two BLS libraries. | 1; `crates/ferrata-spectests/tests/steps.rs` |
| R.2 | shown | Each refusal leaves state unchanged; a refused attestation reverts the consumer's transaction and the same claim then succeeds. | `lez/results/stage1-refusals.md`; `lez/results/stage4-attest-refusals.md` |
| R.3 | not yet | Records are write-once, but no test reads an old record after later committee rotations. | none |
| R.4 | shown | Daemon killed inside a job resumes from chain state, no second Open, in the harness and on a sequencer; the lab restarted. | 79, 99, 104; `lez/lab/README.md` |
| R.5 | shown | A down source and dropped connections handled; restarted mid-job, the daemon finishes with no second Open, harness and sequencer. | 79, 99, 104 |
| R.6 | in part | BLS is audited `blst`; the light-client logic is Ferrata's own transcription, checked against vectors and Helios, not audited. | 1; `crates/ferrata-spectests/tests/helios_oracle.rs` |
| R.7 | in part | Lab instances caught up and ran seven days unrepaired; no attestation tested while stalled; past its bound an instance is replaced. | 21, 69, 70; `lez/lab/README.md` |
| R.8 | shown | Duplicate Open (207), replayed Finish (205) and an applied update (52) refused in the harness and on sequencers we ran. | 6, 22, 112; `lez/results/stage1-refusals.md` |
| R.9 | in part | Two honest daemons and an adversary: refused-step gas 5.7–8.7% of applied, in the in-process chain; not run on a sequencer. | 80; `lez/updater/tests/recorded.rs` |
| R.10 | in part | Fee share 0.39–0.47 over 58 updates in the in-process chain; the latency effect is derived in a first-draft guide. | 80; `docs/guides/operator-guide.md` §3 |
| R.11 | not yet | Mechanism read from the LEZ source; several private reads in one block have not been run. | none |
| R.12 | shown | All seven CI jobs passed on GitHub at `main`; the five latest runs read on 18 November also passed. | GitHub runs 43730879603 and 44172503761; `.github/workflows/ci.yml` |

### Performance

| ID | Status | Why | Evidence |
|---|---|---|---|
| P.1 | shown | Each attestation type settles in one transaction, 96,584 to 1,618,872 cycles; 8.2M for a 306-log receipt without the Keccak accelerator. | 3; `lez/results/stage4-attest.md`; `lez/results/stage4-attest-keccak-soft.md` |
| P.2 | in part | Cycles and gas per action and statement type; proof time and size for the update and one log attestation only. | 5, 32, 49, 123; `lez/results/stage1-finality.md` |
| P.3 | in part | Per-update costs measured; the amortised tables are derived, per period and day, in a first-draft report, without real proving. | `lez/results/stage1-replay.md`; `docs/performance.md` RFP-021 §P.3 |
| P.4 | in part | Private attestation proven on cloud CPUs at the declared minimum, an M1 and a GPU; not on a laptop. | 32, 38, 50, 88 |
| P.5 | in part | Parts measured on sequencers we ran and in the live runs; finality is an estimate; no end-to-end figure. | 99; `e2e/runs/crown-live-3/README.md` |
| P.6 | in part | The update's proof and the lab's zone measured; the updater daemon's CPU, memory and bandwidth are not. | 21, 123; `lez/lab/README.md` |
| P.7 | in part | Account sizes measured in the harness; the one- and five-year figures are derived in a first-draft report, overhead not counted. | `lez/results/stage1-replay.md`; `docs/performance.md` RFP-021 §P.7 |
| P.8 | in part | Pairing steps measured (7.0M of 9.2M); the precompile delta is a derived upper bound, not a modelled cost. | 127; `lez/results/stage1-finality.md`; `docs/performance.md` RFP-021 §P.8 |
| P.9 | in part | Update steps flat across five mainnet updates; attestation cost against accumulated records not measured. | `lez/results/stage1-replay.md`; `lez/lab/README.md` |

### Supportability

| ID | Status | Why | Evidence |
|---|---|---|---|
| S.1 | waits on Logos | LEZ testnet 0.4 does not exist. Shown instead: rc1, rc2 and rc3 sequencers we ran; the lab followed Sepolia seven days. | 6, 21, 22, 99, 112, 123 |
| S.2 | in part | Sequencer runs by hand and scheduled, against live Sepolia data; CI runs the state transition, not a sequencer. | 6, 21, 76, 99; `lez/live/verify-run.sh` |
| S.3 | in part | Consumer guide is a first draft; its worked example is tested in the harness with real handler proofs. | 39; `docs/guides/consumer-guide.md` |
| S.4 | in part | The pattern's points run in two reference consumers in the harness; the guide's section is a first draft. | 39, 73; `docs/guides/consumer-guide.md` §6 |
| S.5 | in part | First drafts of the CLI, operator and consumer guides and the updater README. | `lez/ATTEST-CLI.md`; `docs/guides/operator-guide.md`; `lez/updater/README.md` |
| S.6 | in part | `trust.md` covers the listed points as a first draft. | `docs/trust.md` |
| S.7 | in part | Refusal costs, cadence and concurrency measured; the operator guide is a first draft; the stall alarm's `monitor` is not built. | 21, 80, 81; `lez/results/stage1-refusals.md`; `docs/guides/operator-guide.md` §3 |
| S.8 | not yet | No audit; it depends on Logos contracting one. Review, fuzzing and dependency checks are not an audit. | none (tooling: 1, 36, 40) |
| S.9 | in part | Catalog laid out from the template and installed from 127.0.0.1; not published. | 61, 98 |
| S.10 | in part | The release action's steps ran locally for linux-amd64; nothing published, the action not run on a release. | 61; `modules/scripts/release-local.sh` |
| S.11 | not yet | No decision log in the repository; the author's log is private, decisions are spread over the design notes. | `docs/how-it-was-built.md` |

### Verification Security

| ID | Status | Why | Evidence |
|---|---|---|---|
| VS.1 | in part | Fabricated chain refused in the native core; forged signature refused through LEZ; the fabricated chain not sent through the program. | 1; `crates/ferrata-spectests/tests/adversarial.rs`; `lez/results/stage1-refusals.md` |
| VS.2 | in part | Roots, chain and predicate forgeries refused through the program; committee, participation and finality forgeries only in native tests. | `lez/results/stage4-attest-refusals.md`; `crates/ferrata-spectests/tests/steps.rs` |
| VS.3 | in part | Program deployed immutable through LEZ's loader, image ids reproducible; migration text is a first draft; no replacement attempt tested. | 6, 37, 82, 92; `lez/live/README.md` |
| VS.4 | in part | Documented in a first-draft `trust.md`; the attestation side's `monitor` is not built. | `docs/trust.md` §2, §6 |
| VS.5 | in part | Stated in first drafts of the trust document and the operator and consumer guides. | `docs/trust.md`; `docs/guides/operator-guide.md`; `docs/guides/consumer-guide.md` |
| VS.6 | shown | Forged signature refused from any sender; three keys applied updates; only spoofed internal messages refused by sender (202). | 80; `lez/results/stage1-refusals.md` |

### Soft requirements

| ID | Status | Why | Evidence |
|---|---|---|---|
| Soft.1 | in part | In the bridge's own form only: `ImportRoot` records a root once; no generic recorded-statement account. | 27, 41, 91; `lez/harness/tests/bridge_inbound.rs` |
| Soft.2 | shown | Four facts under one record in one call, 453,182 cycles, through the program. | 71; `lez/results/stage4-attest.md` |
| Soft.3 | in part | One EIP-2935 hop through the program on mainnet and Sepolia; longer chains only in the native core. | 72; `lez/results/stage4-attest.md` |
| Soft.4 | shown | Conditions on four facts and a relation between two of them in one request, accepted and refused through the program. | `lez/results/stage4-attest.md`; `lez/results/stage4-attest-refusals.md` |
| Soft.5 | not yet | A design note only; the protocol fee it would draw on starts at zero. | none |

## RFP-022: Privacy-Preserving Wrapped ERC-20 and Ether Bridge

On LEZ the wrapped balances are kept in the bridge program's own accounts, because no v0.4 token program is
released ([not-built.md](not-built.md)); the rows below are classified on the behaviour the RFP names, and
"mint" and "burn" mean a credit and a debit of those balances.

### Functionality

| ID | Status | Why | Evidence |
|---|---|---|---|
| F.1 | shown | ETH, 6- and 18-decimal and USDT-style deposits escrowed on a local EVM; ETH deposited live on Sepolia. | 12, 41, 91; `vault/test/Deposit.t.sol` |
| F.2 | shown | LEZ-side data never appears in calldata, events or storage; the destination found 0 of 320 times in public bytes. | 23, 89; `vault/test/Privacy.t.sol` |
| F.3 | shown | Mints only on a claim under a root imported through the attestation; repeat and second-root claims refused (250); live on Sepolia. | 8, 41, 76, 91; `lez/harness/tests/bridge_inbound.rs` |
| F.4 | shown | Mints to public and private accounts; the private claim proven for real and applied by sequencers we ran. | 8, 89, 91, 123; `lez/harness/tests/bridge_private.rs` |
| F.5 | shown | Burns write the note once with no Ethereum address; both live burns released on Sepolia. | 8, 91; `lez/harness/tests/bridge_burn.rs` |
| F.6 | shown | Burns from public and private holdings in the harness and on sequencers we ran; the private burn proven for real. | 8, 91, 100, 123; `lez/harness/tests/bridge_burn.rs` |
| F.7 | in part | Released on Sepolia against real proofs for a one-key devnet; 3 of 4 keys on anvil; validity rests on sequencer keys. | 24, 41, 65, 91, 100, 101 |
| F.8 | in part | Denominations enforced on both sides, pairing at chance among 20 users; anonymity-set size per denomination not measured, client view not built. | 23, 89; `vault/test/Deposit.t.sol`; `cli/README.md` |
| F.9 | shown | From the seed alone, 10 of 10 unclaimed deposits and unreleased burns found in the harness, also on real proofs. | 23, 84, 89, 102; `cli/tests/e2e.rs` |
| F.10 | in part | Write-once registry on both sides, registered live; delisting has no LEZ effect and the guides do not cover it. | 9, 41, 91; `vault/test/Admin.t.sol`; `lez/harness/tests/bridge_vault_storage.rs` |
| F.11 | shown | Per-token and global buckets, admin-set, on the vault and on LEZ, where they run on attested Ethereum time. | 8, 28; `vault/test/Caps.t.sol`; `lez/harness/tests/bridge_time.rs` |
| F.12 | in part | Outbound wait and release-delay changes tested on the vault; the inbound admin-set depth is not built. | `vault/test/Checkpoint.t.sol`; `vault/test/ReleaseDelay.t.sol` |
| F.13 | in part | Mint fee paid on LEZ, burn fee taken at release on the vault; rates capped at 1%; claims show their schedule's range. | 8, 9, 41, 91; `vault/test/Fees.t.sol` |
| F.14 | shown | All 32 freeze combinations on the vault; LEZ freezes per token or globally (242) in the harness; both in Ferrata's own code. | 8, 43, 74; `vault/test/Freeze.t.sol` |
| F.15 | shown | `Init` refuses another configuration (246) and foreign proofs (110); vault journals bind every field; the live vault holds the devnet's `configId`. | 8, 91, 92; `vault/test/JournalBinding.t.sol` |
| F.16 | in part | A vault-A deposit under configuration B's root is refused (249); no malicious-tenant test; the switching text is a first draft. | 8; `docs/guides/deployer-guide.md` §7 |
| F.17 | shown | USDT-style deposits and releases pass on a local EVM and through the tool; fee-on-transfer and false-returning tokens rejected. | 12, 85; `vault/test/Deposit.t.sol`; `vault/test/Release.t.sol` |

### Usability

| ID | Status | Why | Evidence |
|---|---|---|---|
| U.1 | in part | User functions in a module, LEZ through a patched `lez_core`; Ethereum sends through a stand-in; admin functions only in the mini-app. | 59, 93, 109, 113, 114, 125 |
| U.2 | in part | Mini-app round trip clicked through Basecamp on a sequencer we ran, real proofs; its Ethereum side is a stand-in service. | 57, 62, 86, 103, 125, 126 |
| U.3 | in part | Relayer and `monitor` run as daemons and modules on anvil and devnets; the relayer's root imports on a running sequencer remain. | 43, 46, 96, 106, 107, 120 |
| U.4 | waits on Logos | SPEL does not support LEZ v0.4. Shown instead: a hand-written IDL decodes the bridge program's 37 vectors; SPEL's crate parses it. | 55; `idl/check.py` |
| U.5 | in part | Defaults and exposure indicator tested in tool and mini-app; the gas drop ran on rc3, not yet in the relayer daemon. | 62, 84, 86, 124 |
| U.6 | shown | Tool and mini-app preselect the private path; each public choice needs its own acknowledgement; tested. | 62, 84, 85, 86 |
| U.7 | in part | Tool dry runs and the mini-app say what each step publishes; the user guide is a first draft. | 84; `cli/src/privacy.rs`; `docs/guides/user-guide.md` §10 |
| U.8 | in part | 21 refusals named without a deposit or burn in the tool; the freeze refusal is exercised in the mini-app only. | 62, 85; `cli/tests/e2e.rs` |
| U.9 | shown | Tool takes RPC and sequencer by option; the mini-app switches endpoints in the view, bad ones reported; tested. | 62, 85; `cli/README.md` |
| U.10 | in part | Catalog laid out from the template and installed from 127.0.0.1; not published. | 61, 98, 107 |

### Reliability

| ID | Status | Why | Evidence |
|---|---|---|---|
| R.1 | shown | Refused claims exit with codes and spend nothing; a refused private claim cannot be proven; the deposit is then claimed. | 8; `lez/harness/tests/bridge_inbound.rs`; `lez/harness/tests/bridge_private.rs` |
| R.2 | shown | A refused burn burns nothing (asserted); a failed release leaves the nullifier unused; a held release was later paid. | 19, 85, 102; `lez/harness/tests/bridge_burn.rs`; `vault/test/PullPayment.t.sol` |
| R.3 | shown | Second claim (250), repeated burn note (244) and replayed release refused, also against RISC Zero's real verifier and in CI. | 8, 46, 76; `vault/test/invariant` |
| R.4 | in part | Caps and freezes only delay on both sides; a sunset, the emergency stop and a retirement are disclosed exceptions. | 8, 28; `vault/test/Freeze.t.sol`; `vault/test/Refund.t.sol`; `vault/test/EmergencyStop.t.sol` |
| R.5 | shown | A wiped client finds 10 of 10 unclaimed deposits and unreleased burns from the seed, also with real proofs. | 23, 84, 89, 102 |
| R.6 | in part | `monitor` killed and restarted makes exactly one cancellation; the relayer keeps its gas-drop record; no relayer kill mid-checkpoint. | 43, 46, 47, 96 |
| R.7 | in part | A view closed during a proof resumed its job once, in Basecamp; no test of a client killed between proof and sending. | 126; `e2e/runs/seqnet-basecamp-final-real/SUMMARY.md` |
| R.8 | shown | RISC Zero's unmodified Groth16 verifier accepts the receipts, on a local EVM and on Sepolia; no new ZK primitive. | 12, 19, 41, 91; `bridge/evm-verify/test/v2/Groth16Verify.t.sol` |
| R.9 | in part | Documented in first drafts of `trust.md` and the operator guide. | `docs/trust.md` §1, §4; `docs/guides/operator-guide.md` §10 |
| R.10 | in part | CI runs the round trip over the state transition and anvil; sequencer round trips, twice live on Sepolia, are by hand. | 41, 76, 91, 100, 102 |
| R.11 | in part | Most hard rows name a test or run; S.4 and Sec.1 name none, documentation rows name drafts; not all in CI. | this file |
| R.12 | in part | First drafts of the user guide, the consumer guide and the tool's README. | `docs/guides/user-guide.md`; `cli/README.md` |
| R.13 | in part | First drafts of the tool's README, the operator guide and `monitor`'s README. | `cli/README.md`; `docs/guides/operator-guide.md`; `monitor/README.md` |
| R.14 | in part | Deployer guide is a first draft; three vaults deployed on Sepolia by the script; no second deployer has followed it. | 26, 41, 53, 91, 122 |

### Performance

| ID | Status | Why | Evidence |
|---|---|---|---|
| P.1 | shown | A plaintext claim settles in one public transaction, 261,671 execution gas broken down by handler, in the harness. | 8; `lez/results/bridge-inbound.md` |
| P.2 | in part | Import costs measured in the harness and on Sepolia's state; the per-deposit figure is derived in a first-draft report. | 8, 27, 91; `docs/performance.md` RFP-022 §P.2 |
| P.3 | shown | Deposit, candidate, finalisation and release gas measured with the real verifier on a local EVM and paid live on Sepolia. | 13, 25, 91; `vault/GAS.md` |
| P.4 | in part | Private claim and burn proven on cloud CPUs at the minimum, Mac minis and a GPU; only the release on a laptop. | 33, 38, 50, 88 |
| P.5 | in part | Inbound and outbound parts measured live with testnet parameters; production figures are estimates and a simulation. | 41, 91; `e2e/runs/crown-live-3/README.md` |
| P.6 | in part | Provers and `monitor`'s memory measured; the daemons' CPU and bandwidth and the relayer's memory are not. | 15, 17, 43, 91, 100 |
| P.7 | in part | 1M and 10M figures derived from one measured and two estimated account sizes, in a first-draft report. | `docs/performance.md` RFP-022 §P.7 |

### Supportability

| ID | Status | Why | Evidence |
|---|---|---|---|
| S.1 | waits on Logos | LEZ testnet 0.4 does not exist. Shown instead: the vault on Sepolia and two round trips against devnets we ran. | 41, 91, 92, 122, 123 |
| S.2 | in part | User-side proofs built and proven on macOS; the modules and the catalog on Linux x86-64 only. | 88, 59–62, 94–98 |
| S.3 | in part | The release action's steps ran locally; nothing published, the action not run on a release. | 61 |
| S.4 | not yet | No decision log in the repository; the author's log is private, decisions are spread over the specification and notes. | `docs/how-it-was-built.md` |

### Security

| ID | Status | Why | Evidence |
|---|---|---|---|
| Sec.1 | not yet | No audit; it depends on Logos contracting one. Static analysis, symbolic checks and an internal review are not an audit. | none (tooling: 30, 35, 54) |
| Sec.2 | shown | Flipped proof and journal words, wrong inputs and replays refused by the real verifier; altered claims and receipts refused on LEZ. | 12, 42, 46, 75; `lez/BRIDGE-RESULTS.md` §2 |
| Sec.3 | shown | Redirected, fee-raised and replayed releases refused by the real verifier; a redirected claim refused on LEZ (249, 252). | 46, 47, 76; `lez/harness/tests/bridge_inbound.rs` |
| Sec.4 | in part | Buckets tested on both sides; the recommended default is in a first-draft guide; live vaults ran without caps. | 8, 28; `vault/test/Caps.t.sol`; `docs/guides/deployer-guide.md` §2 |
| Sec.5 | shown | Each half freezes by its own authority in tests that run without the other half. | 8, 28, 74; `vault/test/Freeze.t.sol`; `lez/harness/tests/bridge_controls.rs` |
| Sec.6 | in part | Vault invariants hold over 256 runs × 100 calls; invalid mints and releases refused; no randomised run over both real halves. | 12, 42; `vault/test/invariant` |
| Sec.7 | in part | Stated in first drafts of the user guide and `trust.md`. | `docs/guides/user-guide.md`; `docs/trust.md` |
| Sec.8 | in part | Image ids and verifier are immutables, read back on Sepolia; migration text is a first draft; no migration run. | 74, 91, 92; `docs/guides/deployer-guide.md` §9 |
| Sec.9 | in part | Stop-only failsafe tested, its owner renounced live on Sepolia; why no fund recovery exists is a first-draft document. | 35, 41, 91; `vault/test/EmergencyStop.t.sol`; `docs/trust.md` §6 |

### Privacy Preservation

| ID | Status | Why | Evidence |
|---|---|---|---|
| Priv.1 | shown | An adversary pairs 20 deposits with their claims at chance (5.0%), in dev mode and on real proofs of the final programs. | 23, 89; `docs/privacy/RESULTS.md` |
| Priv.2 | shown | An adversary pairs 20 burns with their releases at chance (5.0%), also on real proofs; the burn root's checkpoint mocked. | 23, 89; `docs/privacy/real/RESULTS.md` |
| Priv.3 | shown | Destinations and linked values searched in every public byte of 20 pairs each way: 0 of 320; plaintext control fails. | 23, 89; `vault/test/PrivacySuite.t.sol` |
| Priv.4 | in part | No linking value in submitted bytes (measured); the document of who handles what is a first draft. | 23; `docs/trust.md` §8; `docs/guides/user-guide.md` §10 |
| Priv.5 | in part | Client-side refusals name no deposit or burn; on-chain indistinguishability of rejected, repeated and capped claims not measured. | 62, 85 |
| Priv.6 | in part | A relayer chosen per claim and per release, tested; what it learns is measured but documented in first drafts. | 84, 85, 124; `docs/privacy/generated/fields.md` |
| Priv.7 | shown | Tool and mini-app default to private; a public destination, plaintext claim or public burn is refused without its own acknowledgement. | 62, 84, 85, 86 |

### Soft requirements

| ID | Status | Why | Evidence |
|---|---|---|---|
| Soft.1 | not yet | Not offered: committed amounts would need a new vault. | none |
| Soft.2 | in part | `kView` discloses a holder's burns, not deposits or claims, in native tests; disclosure to a third party not tested. | `bridge/crates/ferrata-bridge-core/tests/outbound.rs`; `docs/privacy/RESULTS.md` |
| Soft.3 | shown | Per-token release delay tested on a local EVM within its bounds; the tool's early release is refused, then sent. | 85, 102; `vault/test/ReleaseDelay.t.sol` |
| Soft.4 | in part | By redeployment only: a new proof system means a new vault and migration, which has not been run. | as Sec.8 |
| Soft.5 | not yet | Not offered: no web depositor for common wallets. | none |
