# Design decision log

RFP-021 Supportability #11 and RFP-022 Supportability #4 ask for a design decision log in the repository, recording
each major decision: the context, the options considered, the option chosen, and the rationale and trade-offs, kept
current at each milestone. This directory is that log, one record per decision (an architecture decision record).

**How it was made, and how it is kept.** These records were reconstructed on 21 November 2026 from what the
repository already holds: the design notes ([`attestation.md`](../design/attestation.md),
[`bridge.md`](../design/bridge.md)), the bridge specification's changes D1–D13
([`bridge-spec-v3-changes.md`](../design/spec/bridge-spec-v3-changes.md)), the "Decisions" items of
[`lez/RESULTS.md`](../../lez/RESULTS.md) §5, the trust document, the guides and the evidence index. Each record cites
its sources; a date is the one its source states, or the day the source entered the repository. The author's own
working log is a private file and is not copied here ([`how-it-was-built.md`](../how-it-was-built.md), "The decision
log"). From now on a decision is recorded here when it is taken: a new record with the next number, and a record
that a later one replaces keeps its text and says "superseded by ADR n" in its status. Records 0034–0045 did not
follow that rule: they were written on 26 November 2026 from the commits and documents of decisions taken on 21–25
November and not recorded then; each names the commit that took it, and its date is the decision's. At each milestone delivery
the log is checked against the design notes and brought up to date in the same tagged commit.

**Options.** The chosen option is the last; the decision it records is in the record's sources. Every other option
says where the repository records it (*Recorded:*), or which RFP clause rules it out (*Ruled out:*), or that it is
*reconstructed for contrast*: stated to make the choice clear, not recorded as an option when the decision was
taken. An internal review on 21 November 2026 found options and statements that no source supported; they were
corrected the same day, and each correction is in the record's history.

**Statuses.** *accepted*, then what of it is built, deployed or run, in the record's own words (for example
*accepted; built*: decided and in the code, with evidence in [`EVIDENCE.md`](../EVIDENCE.md); *not deployed*: built
in a second build that no deployment uses yet). *design only*: decided, not built. *superseded by ADR n*: replaced by
a later record, its text kept. A record describes only work in this tree.

| ADR | Decision | Date | Status | Requirements |
|---|---|---|---|---|
| [0001](0001-light-client-in-the-program.md) | Follow Ethereum through the sync committee, inside the LEZ program, on finality updates only | 10 November 2026 (the design note) | accepted; built | RFP-021 F.1, F.2, F.11, VS.1, VS.6 |
| [0002](0002-one-verification-core.md) | One `no_std` verification core, shared by the program, private consumers, the tools and the modules; reuse audited cryptography, transcribe the light client | 10 November 2026 | accepted; built | RFP-021 R.1, R.6, U.2, U.5 |
| [0003](0003-two-update-paths.md) | Two update paths with one verification: a public job of several transactions, and a proven path of one | 10 November 2026 | accepted; built | RFP-021 F.1, F.11, F.12, P.2 |
| [0004](0004-immutable-instances.md) | An immutable program; instances keyed by their configuration and checkpoint; replacement, never refresh; successors for forks after `Init` | 10–11 November 2026 | accepted; built (successors, root ids and the bound: 11 November) | RFP-021 F.9, F.10, F.13, F.14, F.15, VS.3 |
| [0005](0005-chain-id-in-the-configuration.md) | The chain id is part of the configuration and of every record; the program cannot check it, so the checkpoint is verified off chain | 10 November 2026 (Stage 4) | accepted; built | RFP-021 F.5, F.9, F.14 |
| [0006](0006-attest-replies-with-statements.md) | `Attest` replies with statements, `Read` with the record; one request may hold several facts | 10 November 2026 (Stage 4) | accepted; built | RFP-021 F.3, F.4, F.7, Soft.4 |
| [0007](0007-private-consumers-read.md) | Private consumers read the record and verify inside their own proof; they never call `Attest` | 10 November 2026 | accepted; built | RFP-021 F.8, R.11 |
| [0008](0008-errors-reach-the-transaction.md) | Typed errors reach the transaction's outcome, not the consumer; the client maps each code | 10 November 2026 (Stage 4) | accepted; built | RFP-021 U.2, U.5, S.4 (rules 3 and 4) |
| [0009](0009-statement-identifiers.md) | Statement identifiers per kind of fact, independent of the instance; a class says which may be nullified | 10 November 2026; the class on 18 November 2026 (second build) | accepted; built (the class in a second build, not deployed) | RFP-021 F.6, S.4 |
| [0010](0010-consumer-pattern.md) | The Recommended Consumer Pattern has seven rules: the RFP's four and a secret-keyed nullifier | 10 November 2026 | accepted; built (rules 5–7 on the vault's real deposits: 11 November) | RFP-021 S.4, F.8; RFP-022 F.3 |
| [0011](0011-updater-deferral.md) | The updater re-checks the chain and defers each step by a hashed share of a window; it finishes the job it opened | 12 November 2026 | accepted; built | RFP-021 R.4, R.5, R.8, R.9, R.10, U.3 |
| [0012](0012-updater-client-diversity.md) | The updater can require byte-identical data from several consensus clients before it pays | 18 November 2026 | accepted; built, off by default | RFP-021 F.12, S.6, VS.4 |
| [0013](0013-deposit-secrets-from-the-seed.md) | Deposit and burn secrets derive from the LEZ wallet seed; recovery scans whole lists | 9 November 2026 (SPEC-v2) | accepted; built | RFP-022 F.2, F.5, F.9, R.5, Priv.4 |
| [0014](0014-fixed-denominations.md) | Fixed denominations, at most eight per token | 9 November 2026 (SPEC-v2) | accepted; built | RFP-022 F.8, Priv.1, Priv.2 |
| [0015](0015-return-leg-rules.md) | The return leg: a checkpoint program over LEZ blocks, finality by rule (B) attestations, rule (A) as the fallback | 10 November 2026 (SPEC-v3 D1, D3, D4, D7) | accepted; both built; (B) proposed to Logos | RFP-022 F.6, F.7, F.12, Sec.2, Sec.7 |
| [0016](0016-stop-only-failsafe.md) | No escrow-moving failsafe: a stop that only RISC Zero's proof of unsoundness trips, lapsing freezes, keyless retirement and refunds | 10–11 November 2026; corrected 21 November 2026 (after a stop, refunds stop too) and 22 November 2026 (what starts the keyless-retirement count) | accepted; built | RFP-022 Sec.5, Sec.8, Sec.9 |
| [0017](0017-release-delay-and-index-free-nullifier.md) | A release delay inside a revocation window, and a release nullifier that does not depend on the burn's index | 10 November 2026 (D13); the guest on 11 November 2026 (security review M-3) | accepted; built | RFP-022 R.3, Sec.4, Sec.6, Soft.3 |
| [0018](0018-attested-time-on-lez.md) | LEZ-side caps and freezes run on attested Ethereum time | 11 November 2026 | accepted; built | RFP-022 F.11, F.14, Sec.4, Sec.5 |
| [0019](0019-bound-the-loss.md) | Bound what a captured committee or a forged root can take, instead of pricing the attack | 18 November 2026 (measured); the caps from 10 November 2026 | accepted; caps and freezes built; bonded backing design only | RFP-021 VS.4; RFP-022 Sec.4 |
| [0020](0020-pairing.md) | Pairing by one configuration id that both halves check; the vault's address by plain CREATE | 9 November 2026 (SPEC-v2: the pairing); plain CREATE 10 November 2026 (SPEC-v3 D2) | accepted; built | RFP-022 F.15, F.16 |
| [0021](0021-fees.md) | Fees bound in proofs, schedules announced seven days ahead, each at most 1%; the checkpoint fee pays provers | 9 November 2026 (SPEC-v2: the 1% ceiling and the 7-day notice); fee collection and the checkpoint fee 10 November 2026 (D13) | accepted; built | RFP-022 F.13, Sec.3 |
| [0022](0022-gas-drop.md) | A gas drop funds a first private claim's fee without linking it | 10 November 2026 (D13); run on LEZ v0.4.0-rc3 on 16 November 2026 | accepted; built and run with LEZ's wallet; in the relayer daemon and the `ferrata` tool (LEZ v0.4.0-rc1's sequencer, dev-mode proofs); not for burns | RFP-022 U.5, Priv.6 |
| [0023](0023-deploy-one-transaction-per-run.md) | Deploy one transaction per run, with limits from the target node's estimate | 10 November 2026 | accepted; built | RFP-022 R.14, S.1 |
| [0024](0024-reproducible-guests-and-second-builds.md) | Guest builds independent of the build path; changes to a deployed program go into a second build | 11 November 2026; second builds from 18 November 2026 | accepted; built | RFP-021 VS.3; RFP-022 Sec.8 |
| [0025](0025-tenant-issues-second-build.md) | The two tenant issues of 18 November are fixed in a second build of the bridge program, not by redeploying now | 18 November 2026 | accepted; the fix built and tested, the bridge program itself since milestone B1, not deployed | RFP-022 F.16, Sec.3 |
| [0026](0026-leaf-index-bound.md) | Every leaf index stays below 2^32 | 10 November 2026 (SPEC-v3 D9) | accepted; built | RFP-021 F.6; RFP-022 F.3, R.3 |
| [0027](0027-recorded-statements-beside-the-program.md) | Recorded statements by a separate program that calls the attestation program's own `Attest` | 21 November 2026 | accepted; built, not deployed | RFP-021 Soft.1, F.7, VS.3 |
| [0028](0028-operator-rebate-pool.md) | An operator rebate paid by a pool program for an advance landed through it | 21 November 2026 | accepted; built, not deployed | RFP-021 Soft.5, F.11, VS.6 |
| [0029](0029-voluntary-disclosure-claimed-only.md) | Voluntary disclosure of deposits and claims: the deposit secret only once its nullifier is spent | 21 November 2026 | accepted; built, not deployed | RFP-022 Soft.2, Priv.4 |
| [0030](0030-web-depositor-takes-a-ticket.md) | A web depositor that takes a ticket from the user's Logos tool and never sees the seed | 21 November 2026 | accepted; built, not deployed | RFP-022 Soft.5, U.7 |
| [0031](0031-committed-amounts-path.md) | Committed amounts: a new vault on both legs, the next one built so that a later switch is LEZ-side only | 21 November 2026 | design only | RFP-022 Soft.1, Sec.8 |
| [0032](0032-burn-tag-used-once.md) | A burn's tag is used once: the burn record keyed on the tag, the wallet's counters journalled | 22 November 2026 | accepted; built, not deployed | RFP-022 R.2, R.4, R.5, R.7 |
| [0033](0033-broken-verifier-without-proof-of-exploit.md) | A broken verifier with no published proof of exploit: contain, do not recover | 26 November 2026 | accepted; built (the runbook rehearsed in vault tests; no vault code changed) | RFP-022 Sec.9, R.4, Sec.4, Sec.5, Sec.8 |
| [0034](0034-advisory-vault-caps-zero.md) | Answer the Groth16 advisory on the four November vaults by setting release and deposit caps to zero | 21 November 2026 | accepted; executed on Sepolia, the vaults now superseded | RFP-022 R.8, Sec.2, Sec.3, Sec.6, Sec.9 |
| [0035](0035-own-module-catalog-index-on-main.md) | Publish an own module catalog with immutable releases and its index committed to `main` | 21–24 November 2026 | accepted; published and installed from | RFP-021 S.9, S.10; RFP-022 U.10, S.3 |
| [0036](0036-updater-turns-per-instance-state.md) | Count updater deferral turns per state of the instance, not per job | 22–23 November 2026 | accepted; built | RFP-021 F.12, R.9, R.10 |
| [0037](0037-risc-zero-3-1-phase-1.md) | Move the Ethereum side to RISC Zero 3.1 first, and leave the LEZ side on LEZ's 3.0.5 until LEZ moves | 23 November 2026 | accepted; phase 1 built and deployed on Sepolia, phase 2 waits on Logos | RFP-022 R.8, Sec.2, Sec.3, Sec.6 |
| [0038](0038-new-vault-own-verifier-and-renounced-stop.md) | New vaults keep their own verifier behind their own renounced stop, backed by keyed freezes, watchers and caps | 23 November 2026 | accepted; deployed on Sepolia for the test network | RFP-022 Sec.5, Sec.8, Sec.9 |
| [0039](0039-return-leg-bedrock-0-3-5-guest.md) | For the rc3 return leg, pin the checkpoint guest built for Logos Blockchain 0.3.5-rc.1 | 23 November 2026 | accepted; deployed on Sepolia for the test network | RFP-022 F.6, F.7, R.10, Sec.2 |
| [0040](0040-attest-cli-send-journal-and-resume.md) | Journal every attestation CLI send before submission and resume the same intent byte for byte | 23 November 2026 | accepted; built | RFP-021 R.5, U.5 |
| [0041](0041-public-own-rc3-test-network.md) | Run a public LEZ v0.4.0-rc3 test network with its own Logos Blockchain node while testnet 0.4 is absent | 24 November 2026 | accepted; deployed on the test network | RFP-021 S.1; RFP-022 S.1, R.10 |
| [0042](0042-ci-standalone-lez-sequencers.md) | Exercise the attestation and public bridge round trip in CI against standalone LEZ sequencers | 24 November 2026 | accepted; run in CI | RFP-021 S.2; RFP-022 R.10 |
| [0043](0043-network-relayer-fee-payer-default.md) | Use the network's gas-drop relayer by default and fall back to the user's fee account only on 404 or 410 | 25 November 2026 | accepted; built and tested in dev mode; the test network's relayer not deployed yet | RFP-022 Priv.7, U.5 |
| [0044](0044-deposit-journal-before-signing.md) | Journal a deposit before signing and resolve an interrupted send from the chain before planning another | 24–25 November 2026 | accepted; built | RFP-022 R.7 |
| [0045](0045-prover-client-resume-recorded-call.md) | Resume a prover call from its recorded id, retry a failed prover and deliver the resulting job once | 24–25 November 2026 | accepted; built, exercised against the recorded call-model stand-in | RFP-022 R.6 |
| [0046](0046-updater-join-or-wait.md) | Updaters join or wait for a job open on the instance's store instead of opening another | 27 November 2026 | accepted; built | RFP-021 R.9, R.10, F.12 |
