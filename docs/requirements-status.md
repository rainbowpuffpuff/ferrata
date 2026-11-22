# Requirement status, row by row

The status of every row of [traceability.md](traceability.md): 64 rows for RFP-021 and 73 for RFP-022. The
classification is the project's own, made on 18 November 2026 and updated on 22 November 2026 from the matrix as it stands at this commit, and
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
| RFP-021 | 37 | 18 | 1 | 3 | **59** |
| RFP-022 | 36 | 29 | 1 | 2 | **68** |

Soft requirements, counted apart (5 for each RFP): RFP-021 5 shown; RFP-022 2 shown, 3 in part.

By section, hard requirements:

| RFP | Section | shown | in part | not yet | waits on Logos | total |
|---|---|---:|---:|---:|---:|---:|
| RFP-021 | Functionality | 13 | 1 | 0 | 1 | 15 |
| RFP-021 | Usability | 2 | 3 | 0 | 1 | 6 |
| RFP-021 | Reliability | 8 | 4 | 0 | 0 | 12 |
| RFP-021 | Performance | 5 | 4 | 0 | 0 | 9 |
| RFP-021 | Supportability | 4 | 5 | 1 | 1 | 11 |
| RFP-021 | Verification Security | 5 | 1 | 0 | 0 | 6 |
| RFP-022 | Functionality | 11 | 6 | 0 | 0 | 17 |
| RFP-022 | Usability | 3 | 6 | 0 | 1 | 10 |
| RFP-022 | Reliability | 8 | 6 | 0 | 0 | 14 |
| RFP-022 | Performance | 5 | 2 | 0 | 0 | 7 |
| RFP-022 | Supportability | 0 | 3 | 0 | 1 | 4 |
| RFP-022 | Security | 3 | 5 | 1 | 0 | 9 |
| RFP-022 | Privacy Preservation | 6 | 1 | 0 | 0 | 7 |

## RFP-021: Trustless Ethereum State Attestation

### Functionality

| ID | Status | Why | Evidence |
|---|---|---|---|
| F.1 | shown | Init and updates settle in the harness and on rc1 and rc3 sequencers we ran; real proofs applied; an eight-day run. | 6, 21, 123; `lez/results/stage1-replay.md` |
| F.2 | shown | A header enters only by a verified finality update. Through the program, updates offering a header without verified finality are refused (50; 26, 1; 26; 25), and so is a proof against an attested, never-finalised header (213, 110); a header sent with a request or as an EIP-2935 hop is refused too (215, 120; 150, 151). | 39, 151; `lez/results/lc-security/f2-finalised-only.md`; `lez/results/stage5-attest-refusals.md`; `lez/results/stage4-attest-refusals.md` |
| F.3 | shown | Account, storage slot and logs attested through the program on mainnet and Sepolia blocks; 32 refusals; roots only from the record. | 3, 71, 72; `lez/results/stage4-attest.md`; `lez/results/stage4-attest-refusals.md` |
| F.4 | shown | All six comparisons (`=`, `≠`, `<`, `≤`, `>`, `≥`), against a constant and as relations between two attested facts, carried in the reference consumer's request through the program: 36 cases on a real mainnet record, each applied where it holds and refused (130) where it fails, at the boundary. | 151; `lez/results/lc-security/f4-comparisons.md` |
| F.5 | shown | One address on mainnet and Sepolia: each statement attested under its chain, the other chain's refused (140), in the harness. | 4; `lez/harness/tests/sepolia.rs`; `lez/results/stage5-attest-refusals.md` |
| F.6 | shown | Identifiers returned by the program equal a second derivation; the reference consumer keys its nullifier on them; a second build (not deployed) returns each one's nullifier-safety class beside it. | 145; `lez/harness/tests/attest_native.rs`; `lez/harness/tests/vault.rs`; `lez/harness/tests/history_hops.rs`; `vectors/acc-leaf.json` |
| F.7 | waits on Logos | Names a testnet. Shown instead: same-transaction consumption in the harness and on rc1 and rc3 sequencers we ran. | 3, 27, 91, 123; `lez/results/stage4-attest-consumer.md` |
| F.8 | shown | The full-diff test: two private claims (a deposit in the recorded block, one three hops back), byte-identical once only fresh randomness and the claim's nullifier are taken from one; events, charges and every public account's change identical; no secret, statement id, account, vault address, receipt proof's last node or hop header in either; the calling variant fails the same test. Private attestation proven for real and applied on rc3. | 5, 73, 123, 152; `lez/results/lc-security/f8-full-diff.md` |
| F.9 | in part | Parameters fixed at Init; `verify_checkpoint` ran against Sepolia providers from Ferrata's module, not as an API of the Logos Ethereum module. | 69, 70, 105, 117 |
| F.10 | shown | Replacement is a fresh instance: re-`Init` refused (203), an `Init` with another chain id refused (201), the instance unchanged; past the weak-subjectivity bound updates are refused, and a stale checkpoint was refused live. When to replace and how a consumer migrates: consumer guide §9.1, §9.3; the recipe's re-binding run (the same statement identifier under the replacement, 211 both ways, the old record still attests), its step 4 shown to be needed (the two pools' nullifiers differ) and not tooled. | 69, 70, 91, 105, 177, 178; `lez/updater/tests/replacement.rs`; `docs/guides/consumer-guide.md` §9 |
| F.11 | shown | No step checks its sender; three updaters with their own keys advanced the same instances in the harness. | 80; `lez/harness/tests/private_placement.rs`; `lez/updater/tests/recorded.rs` |
| F.12 | shown | `docs/trust.md` §2.4 lists the seven off-chain parties, what each can affect and why none, alone or as a class, can block; a test per party in which it lies, lags, stops or censors and an attestation is still produced (8 of 8; the table and the tests checked against each other); forged updates refused by the signature check (41, 40); the LEZ nodes clients read from argued, not tested. | 79, 80, 177; `lez/updater/tests/every_party.rs`; `docs/trust.md` §2.4 |
| F.13 | shown | Re-Init refused (203) in the harness and on sequencers; program deployed immutable through LEZ's loader; recovery by a fresh instance. | 6, 21, 91, 112; `lez/results/stage1-refusals.md` |
| F.14 | shown | One program followed mainnet and Sepolia through the Gloas fork, in the harness and side by side on the lab's zone. | 4, 21; `lez/results/stage5-sepolia-replay.md` |
| F.15 | shown | Two instances side by side on the lab's zone; another instance's job id and request are refused (205, 211). | 21, 70; `lez/results/stage1-refusals.md`; `lez/results/stage4-attest-refusals.md` |

### Usability

| ID | Status | Why | Evidence |
|---|---|---|---|
| U.1 | in part | Module loaded in a headless Logos Core, Ethereum reads through `eth_rpc_module`; LEZ reads and sends not through Logos Core's modules. | 59, 60, 93, 108, 116 |
| U.2 | in part | CLI attests, dry-runs with the program's codes, advances and reads configuration against the in-process chain only, not a sequencer. | 56; `lez/ATTEST-CLI.md` |
| U.3 | in part | Daemon and module followed Sepolia on sequencers we ran; the operator journey documented and recorded (operator guide §0, §3; `/health` ok, SIGTERM exit 0); Ethereum data partly outside Logos Core. | 79, 81, 97, 99, 104, 118, 179 |
| U.4 | waits on Logos | SPEL's release targets LEZ v0.2.4. Shown instead: a hand-written IDL in SPEL's format that SPEL's crate parses and decodes. | 55, 58; `idl/check.py` |
| U.5 | shown | Every listed failure mode refused through the program with its code (22 cases, a committee handoff refused with 27, 45 and 23 among them), each code's name and message from the IDL the CLI prints. A checkpoint stale by the sequencer's clock is refused by LEZ with its own text before the program runs, on the window the program declares; the 53 message applies when the update is signed after the end. | 39, 56, 69, 70, 151; `lez/results/lc-security/u5-failure-modes.md` |
| U.6 | shown | ERC-20 `Transfer` and native-ETH deposit consumers credit once in the harness, public and private claims, on real mainnet and Sepolia data. | 3, 73; `lez/harness/tests/attest_public.rs`; `lez/harness/tests/vault_consumer.rs` |

### Reliability

| ID | Status | Why | Evidence |
|---|---|---|---|
| R.1 | shown | Step API equals `process_update` on 52 sync cases and the mainnet replay; vectors identical under two BLS libraries. | 1; `crates/ferrata-spectests/tests/steps.rs` |
| R.2 | shown | Each refusal leaves state unchanged; a refused attestation reverts the consumer's transaction and the same claim then succeeds. | `lez/results/stage1-refusals.md`; `lez/results/stage4-attest-refusals.md` |
| R.3 | shown | Two records attested after each of four committee rotations and ten years later, statements byte-identical and records unchanged (synthetic chain); five real mainnet records read after four real rotations and ten years later; a real Sepolia record attested before and after a real rotation. | 151; `lez/results/lc-security/r3-rotations.md` |
| R.4 | shown | Daemon killed inside a job resumes from chain state, no second Open, in the harness and on a sequencer; the lab restarted. | 79, 99, 104; `lez/lab/README.md` |
| R.5 | shown | A down source and dropped connections handled; restarted mid-job, the daemon finishes with no second Open, harness and sequencer. | 79, 99, 104 |
| R.6 | in part | BLS is audited `blst`; the light-client logic is Ferrata's own transcription, checked against the official vectors, Helios (the vectors, 220 recorded updates, 2,000 random mutations of real updates) and Lodestar (the same mutations), 0 disagreements; not audited. | 1, 140, 153; `crates/ferrata-spectests/HELIOS-DIFFERENTIAL.md` |
| R.7 | in part | Lab instances caught up and ran eight days unrepaired; no attestation tested while stalled; past its bound an instance is replaced. | 21, 69, 70; `lez/lab/README.md` |
| R.8 | shown | Duplicate Open (207), replayed Finish (205) and an applied update (52) refused in the harness and on sequencers we ran. | 6, 22, 112; `lez/results/stage1-refusals.md` |
| R.9 | in part | On a running sequencer, two honest daemons alone at windows 1, 8 and 30 and with an adversary: A3's waste ratio 0.0036 at window 30 (on 20 updates; the gate asks 100), above the gate at windows 1 and 8 and with the adversary; the abandoned-job policy is not built. | 80, 172; `lez/results/perf-gate-sequencer.md` |
| R.10 | in part | On a running sequencer, honest-1's fee share 0.493–0.609 and latency medians 105–306 s across windows, 8–23 completed jobs per run against A3's 100 updates, the window comparison confounded by one run's reveal step; the deferral strategy and its measured effect in the operator guide §3.4. | 80, 172; `docs/guides/operator-guide.md` §3.4 |
| R.11 | shown | Four private attestations proven against one snapshot, a finality update applied in between, then settled in one block with nothing re-proven; one account's second transaction on the same snapshot dropped. The block is assembled in process with rc1's block settlement; no sequencer ran it. | 152; `lez/results/lc-security/r11-one-block.md` |
| R.12 | shown | All seven CI jobs passed on GitHub at `main`; the five latest runs read on 18 November also passed. | GitHub runs 43730879603 and 44172503761; `.github/workflows/ci.yml` |

### Performance

| ID | Status | Why | Evidence |
|---|---|---|---|
| P.1 | shown | Each attestation type settles in one transaction, 96,584 to 1,618,872 cycles; 8.2M for a 306-log receipt without the Keccak accelerator. | 3; `lez/results/stage4-attest.md`; `lez/results/stage4-attest-keccak-soft.md` |
| P.2 | in part | Cycles, gas, proof time and size for every action that advances the tracked state (a finality update and a committee handoff each also proven in one private transaction and settled) and every statement type but the accumulator leaf: 39 guest executions proven for real; the leaf's only real proof is of an earlier image, and the public `VerifyAll`, `ApplyVerified` and `Read` are not proven. | 5, 32, 49, 123, 166; `lez/results/perf-proofs.md` |
| P.3 | shown | Tracking cost measured over eight days of the lab's sequencer (9,225,372 execution gas a finality update, 16,060,242 a handoff) and amortised per attestation apart from verification: 235.5M / N a day at the recommended cadence, 14.1M / N once per period (derived); the proven path proven for real, 485,298 + 224,063 gas an update, 451,895 + 224,063 a handoff. | 167; `lez/results/perf-tracking.md`; `docs/performance.md` RFP-021 §P.3 |
| P.4 | in part | This tree's padded private attestation proven at the declared minimum's 8 and 16 CPUs (1,254.8 s and 633.5 s), on an M1 and a GPU; the owner's laptop script is ready and rehearsed; not run on a laptop. | 32, 38, 50, 88, 168; `bench/perf/p4-laptop-owner.sh` |
| P.5 | shown | End-to-end latency from the live run's own records: median 18.1 min from the Ethereum event to a statement usable on LEZ (p90 20.7), split into finality 13.0, the next finalized header 3.1, availability 0.2, the update on LEZ 1.5 and the consuming transaction 0.3. | 99, 169; `lez/results/perf-latency-e2e.md` |
| P.6 | in part | The updater daemons on a running sequencer: 0.043–0.092 of a core and 57.3–57.8 MiB each; the update's proofs measured; the attestation CLI and module, the §8 `monitor` and the daemons as Logos modules not measured. | 21, 123, 172; `docs/performance.md` RFP-021 §P.6 |
| P.7 | shown | Account sizes in LEZ's serialized state with growth measured to 100,000 records (record 453 B, instance 593 B, job 1,110 B); one- and five-year growth derived at the design's cadence (4.12 MB and 20.58 MB); unfinished jobs at most (k − 1) × 10.09 MB a year with k racing operators, none with one (derived). | 170; `lez/results/perf-state.md`; `docs/performance.md` RFP-021 §P.7 |
| P.8 | shown | The BLS precompile delta modelled at time parity from measured native and in-program costs: 8,344,139 cycles of BLS per finality update in the program, 17,841 as a precompile; the update about 902,607 cycles instead of 9,228,905. | 127, 171; `docs/performance.md` RFP-021 §P.8 |
| P.9 | in part | Five statement types and every update step charged the same at every size to 100,000 records; the lab's 98 finality jobs flat; other statement types not measured against accumulated state. | 170; `lez/results/perf-state.md`; `lez/lab/README.md` |

### Supportability

| ID | Status | Why | Evidence |
|---|---|---|---|
| S.1 | waits on Logos | LEZ testnet 0.4 does not exist. Shown instead: rc1, rc2 and rc3 sequencers we ran; the lab followed Sepolia eight days. The day-one deployment is built and rehearsed: one command deploys the attestation program, starts a Sepolia instance and its updater and attests the live vault, run on our own rc3 zone as if it were the public network. | 6, 21, 22, 99, 112, 123, 148 |
| S.2 | in part | Sequencer runs by hand and scheduled, against live Sepolia data; CI runs the state transition. A CI script builds LEZ's standalone sequencer from source and deploys the programs through its loader (20 transactions applied), in a workflow started by hand; its production and consumption phases stopped on recorded data older than the `Init` validity window. | 6, 21, 76, 99, 164; `lez/live/verify-run.sh`; `e2e/runs/ci-sequencer-attest-2026-11-21/` |
| S.3 | shown | Consumer guide: the developer journey in eight steps with a clause map (§0), the interface, pattern, codes and migration (§1–§11), a whole consumer step by step (§12, re-run, its table byte for byte the committed record) and the core module method by method with `logosctl` commands (§13), each run end to end. | 39, 60, 105, 108, 179; `docs/guides/consumer-guide.md` |
| S.4 | shown | Consumer guide §6: the nullifier keyed on the statement identifier, the chain binding, every code of the IDL (71) with who meets it and what the client does (generated, checked), refusing when no attestation can be produced; §6.2 names the test for each point, run in two reference consumers. | 39, 73, 177, 180; `docs/guides/consumer-guide.md` §6 |
| S.5 | shown | CLI guide (`lez/ATTEST-CLI.md`, the core user journey with commands as run, recorded: the one refusal predicted and reproduced, every sent transaction equal to its dry run); the operator journey in the operator guide §0, §3 and the updater README (recorded: `/health` ok, SIGTERM exit 0). | 99, 104, 179; `lez/ATTEST-CLI.md`; `docs/guides/operator-guide.md`; `lez/updater/README.md` |
| S.6 | shown | `docs/trust.md` §0 maps each listed point: the honest supermajority and the cost of the attack it admits (§2.2), the checkpoint and how a verifier confirms it (§2.1), what an attestation proves and does not (§2.3), what each participant observes and can affect (§2.4, §4.2), when the guarantees degrade or fail (§2.5, §6). | 105, 177; `docs/trust.md`; `docs/forgery-cost/` |
| S.7 | in part | Refusal costs, cadence and concurrency measured and in the operator guide §3; the stall alarm's `monitor` is not built. | 21, 80, 81; `lez/results/stage1-refusals.md`; `docs/guides/operator-guide.md` §3 |
| S.8 | not yet | No audit; it depends on Logos contracting one. Review, fuzzing and dependency checks are not an audit. | none (tooling: 1, 36, 40) |
| S.9 | in part | Catalog laid out from the template and installed from 127.0.0.1; publishing is one owner command, every path tested against a stub GitHub with no network (83 of 83), its dry run reproducible; not published. | 61, 98, 163; `modules/catalog/RELEASE.md` |
| S.10 | in part | The release action's steps ran locally for linux-amd64; the catalog's workflows call the action pinned (v1.8); nothing published, the action not run on a release. | 61, 163; `modules/scripts/release-local.sh` |
| S.11 | in part | `docs/decisions/`: 31 dated records (context, options, decision, rationale and trade-offs, sources), their form and references checked (0 findings); 26 reconstructed on one day from the design notes, 0027–0031 written on the day of their decisions; in 0009, 0014, 0021, 0022, 0025, 0027 and 0030 the only alternatives are reconstructed for contrast. That the log is kept current at each milestone can be shown only over the coming milestones. | 181; `docs/decisions/README.md`; `scripts/check-decisions.py` |

### Verification Security

| ID | Status | Why | Evidence |
|---|---|---|---|
| VS.1 | shown | A self-consistent fabricated chain (a forger's 512-key committee, checkpoint, finality update and a block with real tries), accepted by the forger's own instance, sent through the program to the canonical mainnet instance: refused (40), at its records (213, 110), across instances (211) and by the reference consumer (213, 110); nothing written or credited. | 1, 151; `lez/results/lc-security/vs1-fabricated-chain.md` |
| VS.2 | shown | All six forgeries refused through the program: a committee never handed off (40, 27, 45, 23), 341 of 512 signers (51; 342 applies), a proof against a non-finalised header (213, 110), tampered roots (25, 110), another chain (140; another instance 211), a predicate the proof does not cover (110, 130, 131). | 151; `lez/results/lc-security/vs2-forgeries.md` |
| VS.3 | shown | Program deployed immutable through LEZ's loader, image ids reproducible; every replacement attempt through the loader refused and a mutable control swapped (2 of 2); the migration documented with what consumers must do (consumer guide §9: when to replace, the recipe, its re-binding run) and the rule stated for consumers (§2, "Pin the program, never a pointer"). The run moves a consumer to a replacement instance of the same program; a move to a new program version is the guide's text. | 6, 37, 82, 92, 160, 178; `lez/live/README.md`; `docs/guides/consumer-guide.md` §2, §9 |
| VS.4 | in part | Documented in `trust.md` §2.2 (what a captured committee can do) and §6 (failure classes, what bounds the loss); the attestation side's `monitor` is not built. | `docs/trust.md` §2, §6 |
| VS.5 | shown | The trustless model and the liveness-only role of every off-chain participant stated in `docs/trust.md` §0, the consumer guide §0, the CLI guide ("What you trust when you use it"), the operator guide ("Your role is liveness only") and the user guide §0; each participant's role exercised party by party (8 of 8). | 177; `docs/trust.md` §0; `docs/guides/consumer-guide.md` §0; `docs/guides/operator-guide.md` |
| VS.6 | shown | Forged signature refused from any sender; three keys applied updates; only spoofed internal messages refused by sender (202). | 80; `lez/results/stage1-refusals.md` |

### Soft requirements

| ID | Status | Why | Evidence |
|---|---|---|---|
| Soft.1 | shown | `ferrata_statements` records the attestation program's own `Attest` reply once at a public account; a reference reader uses one entry once per configuration after the guide's four consumer checks, each tested (an entry recorded for a weaker predicate is never read); two consumers share one entry, one `Use` 74,518 cycles against 432,128 for a direct attestation. Not deployed. | 155; `lez/results/statements.md`; `lez/results/statements-checks.md`; `docs/guides/recorded-statements.md` |
| Soft.2 | shown | Four facts under one record in one call, 453,182 cycles, through the program. | 71; `lez/results/stage4-attest.md` |
| Soft.3 | shown | Chains of EIP-2935 hops through the program on real data: up to 32 in one transaction under a Sepolia Gloas record (262,112 blocks back), four on mainnet; refusals with their codes. | 72, 145; `lez/results/history-hops-sepolia.md`; `lez/results/history-hops-mainnet.md` |
| Soft.4 | shown | Conditions on four facts and a relation between two of them in one request, accepted and refused through the program. | `lez/results/stage4-attest.md`; `lez/results/stage4-attest-refusals.md` |
| Soft.5 | shown | `ferrata_rebate`: a pool anyone funds, at an address committing to its terms, pays a fixed rebate to whoever lands an advance through `Land`; a direct `Finish` still applies and is paid nothing; one advance pays one rebate, split progress once per gap; on five real mainnet updates. Not deployed; funding it from wrapped fee tokens needs a conversion, not built. | 156; `lez/results/rebate.md`; `lez/results/rebate-terms.md`; `docs/guides/operator-rebate.md` |

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
| F.16 | in part | A vault-A deposit under configuration B's root is refused (249), but the configurations are not strictly separated: the malicious-tenant test shows that a second configuration of the same vault can freeze a public-kind deposit whose `s` it knows (the paired one's claim refused, 250) and fill a destination's shared holder account; the fix changes the image id (B1); switching documented in the deployer guide §7. | 8, 136; `lez/harness/tests/tenant.rs`; `docs/guides/deployer-guide.md` §7 |
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
| U.7 | shown | User guide §10 says what is public and what is private at each step on both chains, with the residual correlations and, per field, what was measured; the tool and the mini-app print the same table before each step, and the guide carries it word for word (generated from `cli/src/privacy.rs`, checked). | 62, 84, 86, 180; `cli/src/privacy.rs`; `docs/guides/user-guide.md` §10 |
| U.8 | in part | 21 refusals named without a deposit or burn in the tool; the freeze refusal is exercised in the mini-app only. | 62, 85; `cli/tests/e2e.rs` |
| U.9 | shown | Tool takes RPC and sequencer by option; the mini-app switches endpoints in the view, bad ones reported; tested. | 62, 85; `cli/README.md` |
| U.10 | in part | Catalog laid out from the template and installed from 127.0.0.1; publishing is one owner command, every path tested against a stub GitHub (83 of 83); not published. | 61, 98, 107, 163 |

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
| R.8 | in part | RISC Zero's unmodified Groth16 verifier accepts the receipts, on a local EVM and on Sepolia; no new ZK primitive. "Mature, audited" is not met by the versions pinned today: risc0-zkvm 3.0.5 is yanked and verifier 3.0.0 accepts forged claims (GHSA-g6qg-9j8p-7j87, GHSA-mm6c-35cm-g4x2); RISC Zero has stopped its own 3.0.0 deployments. Shown again after the move to 3.1: the Ethereum side after phase 1, the LEZ side after phase 2 (a LEZ tag on risc0 3.1, Logos). | 12, 19, 41, 91, 154; `bridge/evm-verify/test/v2/Groth16Verify.t.sol`; `docs/design/risc0-3.1-migration.md` |
| R.9 | shown | `docs/trust.md` §4.3: every third-party component (the updater, the root importer, the relayer both ways, the wrapper, the checkpoint prover, the watchers, the rule (B) attestors, RPC and LEZ nodes, the multisigs) with the effect of its outage and of its malfunction and the mitigation, each mitigation with the test or run that shows it; the operator guide §10. | 12, 43–48, 66, 68, 84, 85, 106, 124; `docs/trust.md` §4.3; `docs/guides/operator-guide.md` §10 |
| R.10 | in part | CI runs the round trip over the state transition and anvil; sequencer round trips, twice live on Sepolia, are by hand. | 41, 76, 91, 100, 102 |
| R.11 | in part | Every hard row of both RFPs mapped to a test, record or document, and CI's claims checked in the `links` job; 51 of 68 RFP-022 rows have a test CI runs, 17 do not, so the gate (`--require-ci RFP-022`) exits 1. | 162; `scripts/ci/row-tests.toml`; `scripts/ci/check-row-tests.py` |
| R.12 | shown | User guide §13: `ferrata_bridge_module`'s methods in journey order for both flows, recovery and status, with `logosctl` commands, then the messages and derivations below it; the flows run through the module natively and in a headless Logos Core (24 of 24, one release proven for real), and the mini-app's round trip through it on a running sequencer and inside Basecamp. | 84, 93, 103, 113, 114, 125, 126; `docs/guides/user-guide.md` §13 |
| R.13 | shown | `cli/README.md` (the user's journey command by command, every option and refusal; its end-to-end test in CI's round trip); the operator journey in the operator guide §0, §4, §6, §7, `monitor/README.md` and the updater README, each component run. | 43–46, 84, 85, 96, 102, 106, 107; `cli/README.md`; `docs/guides/operator-guide.md`; `monitor/README.md` |
| R.14 | shown | Deployer guide §2–§7 and §11: registering tokens, fees, the admin and freeze authorities and the finality depth, each read back by the walkthrough, run from a fresh clone with nothing beside it: every program rebuilt from source equal to `image-ids.md` (20 of 20), 29 of 29 settings PASS. Run by the team on its build box; the proposal's outside-deployer gate (B4) is still open, which the RFP clause does not ask for. | 26, 41, 53, 91, 122, 142, 182; `docs/guides/deployer-guide.md`; `docs/guides/runs/2026-11-21-second-deployer/` |

### Performance

| ID | Status | Why | Evidence |
|---|---|---|---|
| P.1 | shown | A plaintext claim settles in one public transaction, 261,671 execution gas broken down by handler, in the harness. | 8; `lez/results/bridge-inbound.md` |
| P.2 | shown | A root import's cost against the deposits it covers, measured (230,731–236,224 execution gas in the harness, 432,182 on Sepolia's real state), and per deposit at the hourly cadence with the instance's tracking: 10.25M / 1.02M / 102k at 1 / 10 / 100 deposits an hour, 934,029 / 93,403 / 9,340 on the proven path (derived). | 8, 27, 91, 173; `docs/performance.md` RFP-022 §P.2 |
| P.3 | shown | Deposit, candidate, finalisation and release gas measured with the real verifier on a local EVM and paid live on Sepolia. | 13, 25, 91; `vault/GAS.md` |
| P.4 | in part | Private claim and burn proven at the minimum's 8 and 16 CPUs (1,093.0 / 639.2 s and 932.8 / 534.5 s), on Mac minis and a GPU; the owner's laptop script is ready and rehearsed; only the release on a laptop. | 33, 38, 50, 88, 168 |
| P.5 | shown | Deposit to mint and burn to release from the live round trip's records: inbound 32.9 min, about 1.1–12.1 h with the client's default claim hold (derived); outbound 71.0 min from the burn's proving, of which W 60.1 min; testnet parameters. | 41, 91, 174; `lez/results/perf-latency-bridge.md` |
| P.6 | in part | Provers, `monitor` (0.0004 of a core, 14.0 MiB) and the relayer (0.0007 of a core, 20.3 MiB) measured with their bandwidth; the importer as a daemon, `ferrata_attestor`, `monitor`'s LEZ traffic and the daemons as Logos modules not measured. | 15, 17, 43, 91, 100, 175 |
| P.7 | shown | Growth of all bridge state measured (LEZ: a nullifier 165 B, a burn record 273 B, a root 205 B, 162 B per new holder; Ethereum: one slot per release and per new depositor), its storage cost named, and projected at 1M and 10M operations (nullifiers 165 MB / 1.65 GB, holders 162–324 MB at 1M claims; derived). | 170, 176; `lez/results/perf-projections.md`; `docs/performance.md` RFP-022 §P.7 |

### Supportability

| ID | Status | Why | Evidence |
|---|---|---|---|
| S.1 | waits on Logos | LEZ testnet 0.4 does not exist. Shown instead: the vault on Sepolia and three round trips against zones we ran (the third with LEZ v0.4.0-rc3's sequencer and node 0.3.5-rc.1). The bridge program's day-one deployment is built and rehearsed with the attestation program's; the vault's Sepolia step for it is prepared, not sent. | 41, 91, 92, 122, 123, 142, 148 |
| S.2 | in part | User-side proofs built and proven on macOS; a macOS workflow for the user-side crates and five modules is written and its scripts are portable (the same exports under a FreeBSD userland and bash 3.2), never run on macOS (no push); the modules and the catalog run on Linux x86-64 only. | 88, 59–62, 94–98, 165; `.github/workflows/macos.yml` |
| S.3 | in part | The release action's steps ran locally; the catalog's workflows call it pinned (v1.8); nothing published, the action not run on a release. | 61, 163 |
| S.4 | in part | As RFP-021 S.11: `docs/decisions/` (0013–0026 the bridge's, 0019 and 0024 shared, 0029–0031 of 21 November), form and references checked; that the log is kept current at each milestone can be shown only over the coming milestones. | 181; `docs/decisions/README.md`; `scripts/check-decisions.py` |

### Security

| ID | Status | Why | Evidence |
|---|---|---|---|
| Sec.1 | not yet | No audit; it depends on Logos contracting one. Static analysis, symbolic checks and an internal review are not an audit. | none (tooling: 30, 35, 54) |
| Sec.2 | in part | Flipped proof and journal words, wrong inputs and replays refused by the real verifier; altered claims and receipts refused on LEZ. But verifier 3.0.0 accepts a forged claim (GHSA-mm6c-35cm-g4x2): on a fork, with the forgery stood in for, releases without a valid burn are paid (154); LEZ rc1 and rc3 verify private transactions with risc0 3.0.5 (Logos to confirm). The Ethereum side shown again after phase 1 of the move to 3.1, the LEZ side after phase 2. | 12, 42, 46, 75, 154; `lez/BRIDGE-RESULTS.md` §2 |
| Sec.3 | in part | The release journal binds recipient, relayer and fee: redirected, fee-raised and replayed releases refused by the real verifier; a redirected claim refused on LEZ (249, 252). But under verifier 3.0.0 a forged release journal can name any recipient and fee (on a fork, a stranger takes two deposits: 154), and the refusal on LEZ rests on LEZ's risc0 3.0.5. The Ethereum side shown again after phase 1 of the move to 3.1, the LEZ side after phase 2 (waits on Logos). | 46, 47, 76, 154; `lez/harness/tests/bridge_inbound.rs` |
| Sec.4 | in part | Buckets tested on both sides; the recommended default in the deployer guide §2; live vaults ran without caps. | 8, 28; `vault/test/Caps.t.sol`; `docs/guides/deployer-guide.md` §2 |
| Sec.5 | shown | Each half freezes by its own authority in tests that run without the other half. | 8, 28, 74; `vault/test/Freeze.t.sol`; `lez/harness/tests/bridge_controls.rs` |
| Sec.6 | in part | Vault invariants hold over 256 runs × 100 calls; invalid mints and releases refused; no randomised run over both real halves. Under verifier 3.0.0 a release without a valid burn succeeds (154's fork, with the forgery stood in for). | 12, 42, 154; `vault/test/invariant` |
| Sec.7 | shown | User guide §0 (its §12 states freezes, the revocation window and keyless retirement as the vault enforces them, held to the vault's source); `docs/trust.md` §0, §4.2, with the exceptions stated (the accredited sequencers for the return leg; rule (A)'s watchers outside its conditions); every role's mitigation tested (R.9). | 180; `docs/guides/user-guide.md` §0, §12; `docs/trust.md` §4.2 |
| Sec.8 | shown | Verifier and image ids are immutables, read back on Sepolia; the migration is documented (`vault/MIGRATION.md`: stop, retire, sunset, refunds, releases, the new configuration, users moving) and rehearsed on a local EVM with every in-flight position (8 of 8): an in-flight burn released to its own recipient only, an unimported deposit refunded to its depositor only after the sunset and its revocation window, an imported one not refundable. The four Sepolia vaults cannot change their verifier: the move to RISC Zero 3.1 is retirement and new vaults. | 74, 91, 92, 161; `vault/MIGRATION.md`; `vault/test/Migration.t.sol`; `docs/design/risc0-3.1-migration.md` §3 |
| Sec.9 | in part | Stop-only failsafe and per-case outcomes documented and rehearsed for a tripped stop and for a bug in Ferrata's own guests (`vault/MIGRATION.md` §4–5, `docs/trust.md` §6; 8 of 8): after the stop no role moves escrow, refunds included; after a guest bug the release and checkpoint freezes hold while renewed and a lapse reopens the forged path, per-token release freezes let unimported deposits be refunded, a revoked forged root pays nothing; the stop's owner renounced live on Sepolia. For a broken verifier with no published proof of exploit (GHSA-mm6c-35cm-g4x2) the stop does not engage: on the live vaults only the admin's caps contained a forgery (set to zero on 21 Nov 2026; RISC Zero paused its own 3.0.0 stops with the owner's `estop()` on 19 Nov 2026), and how escrow leaves such a vault is open (`docs/design/risc0-3.1-migration.md` §4). | 35, 41, 91, 154, 161; `vault/test/MigrationFailsafe.t.sol`; `vault/test/EmergencyStop.t.sol`; `vault/MIGRATION.md` §4–5; `docs/trust.md` §5, §6 |

### Privacy Preservation

| ID | Status | Why | Evidence |
|---|---|---|---|
| Priv.1 | shown | An adversary pairs 20 deposits with their claims at chance (5.0%), in dev mode and on real proofs of the final programs. | 23, 89; `docs/privacy/RESULTS.md` |
| Priv.2 | shown | An adversary pairs 20 burns with their releases at chance (5.0%), also on real proofs; the burn root's checkpoint mocked. | 23, 89; `docs/privacy/real/RESULTS.md` |
| Priv.3 | shown | Destinations and linked values searched in every public byte of 20 pairs each way: 0 of 320; plaintext control fails. | 23, 89; `vault/test/PrivacySuite.t.sol` |
| Priv.4 | shown | Every component that handles user data documented (`docs/trust.md` §8.1, user guide §10.1); no linking value in any submitted byte of either chain (0 of 320 hidden values over 20 round trips, dev mode and real proofs). | 23, 89; `docs/trust.md` §8.1; `docs/privacy/RESULTS.md` |
| Priv.5 | in part | Client-side refusals name no deposit or burn; on-chain indistinguishability of rejected, repeated and capped claims not measured. | 62, 85 |
| Priv.6 | shown | What each third party submitting for the user learns documented (`docs/trust.md` §8.2, user guide §10.2: the gas-drop relayer, the claim's relayer, the release relayer, the wrapper); the user switches per claim (`--relayer`) and per release (relayer address 0 for any, `--relayer`, `--self-send`), tested. | 46, 47, 48, 84, 85, 124; `docs/trust.md` §8.2 |
| Priv.7 | shown | Tool and mini-app default to private; a public destination, plaintext claim or public burn is refused without its own acknowledgement. | 62, 84, 85, 86 |

### Soft requirements

| ID | Status | Why | Evidence |
|---|---|---|---|
| Soft.1 | in part | Not implemented; the migration path is documented: the deployed vault cannot adopt committed amounts on either leg (three facts of the deployed code, each tested), so the path is a new vault with the old configuration retired, each safety claim citing its test. "Without redeploying the vault" is not met. | 159; `docs/design/committed-amounts.md` |
| Soft.2 | in part | A disclosure document from the user's own keys (burns by `kView`; deposits, and a deposit's secret once its nullifier is spent) that a third party verifies against public chain data alone, wallet-side, tested; a library with no command, not replayed on recorded chain data; "without weakening privacy for anyone else" cannot be met absolutely by disclosing a link. | 157; `bridge/crates/ferrata-disclose/tests/disclose.rs`; `docs/privacy/viewing-keys.md` |
| Soft.3 | shown | Per-token release delay tested on a local EVM within its bounds; the tool's early release is refused, then sent. | 85, 102; `vault/test/ReleaseDelay.t.sol` |
| Soft.4 | shown | The vault takes its verifier and image ids as constructor parameters: the same `FerrataVault` deployed with a second proof system runs deposit, checkpoint and release; a v1 vault asks only for its own image id; no function changes a deployed vault's verifier or image ids; adopting a new proof system is redeployment and migration, rehearsed (Sec.8). | 161; `vault/test/Migration.t.sol`; `vault/MIGRATION.md` §3, §7 |
| Soft.5 | in part | One static page deposits from any injected EIP-1193 wallet, served on 127.0.0.1, its security trade-offs documented; tested in headless Chromium against anvil with a stand-in wallet, not a wallet extension. | 158; `web/depositor/README.md`; `web/depositor/results/anvil-2026-11-21.txt` |
