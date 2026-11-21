# Rows of stream rfp-B: attestation usability, Logos Core integration, the attestation side's monitor (RFP-021)

Branch `rfp-attest-integration` (from `f13faee2`). Proposals for the integrator: the new status of each row, the test or
run that shows each clause, and an EVIDENCE row text. Nothing here edits `docs/EVIDENCE.md`,
`docs/requirements-status.md` or `docs/traceability.md`. Every number below comes from a command run on the Modal build
box (`spike/modal-build/mrun`, profile of this stream) on 21 November 2026; "measured" unless marked. An independent
review of the branch (21 November) reran every cited test and run on a fresh box and reproduced them; its findings are
fixed on the branch after `d2d937a1` (the monitor's divergence rule, the relayer module's build, the U.4 statuses,
wording), and the rows below say what changed.

## Summary

| Row | Was | Proposed | Why, in one line |
|---|---|---|---|
| RFP-021 R.7 | in part | **shown** | `lez/harness/tests/stall.rs`: attestations verify during an 8.2-day stall and after it; the 106 outstanding real Sepolia updates (8 handoffs) applied in order by a late submitter, no repair, no gap |
| RFP-021 S.7 | in part | in part (the monitor gap closed; only the first-draft guide remains) | `monitor/attest`: the stall and half-weak-subjectivity alarms built and tested on the R.7 states |
| RFP-021 VS.4 | in part | in part (the monitor gap closed; only the first-draft `trust.md` remains) | `monitor/attest`: the divergence alarm and the consumer's freeze hook built and tested, including one provider colluding with the forger (the alarm stays critical, the freeze hook runs) |
| RFP-021 U.4 | waits on Logos | **waits on Logos** (shown instead: SPEL's v0.4 generator, at its open pull request, on our programs) | SPEL's LEZ v0.4 line is an open pull request (#301), not a release: the table files that under "waits on Logos". Shown instead: its own generator makes the attestation program's messages and types from `lez/wire`, 0 differences from ours; its decoder reads 905 of 905 recorded messages |
| RFP-022 U.4 | waits on Logos | **waits on Logos** (shown instead: the same generator with our tuple patch) | The same open pull request, which stops at a tuple in `BridgeMessage::Spend`; with our 23-line SPEL patch (drafted, not opened) it makes the bridge's messages and types, 0 differences from ours |
| RFP-021 U.1 | in part | **shown** (two conditions stated) | `ferrata_attest_module` in a headless Logos Core beside `lez_core` on a LEZ zone we ran: its LEZ reads through `lez_core` byte-equal to the plain path; an update's job and attestations sent through `lez_core`, applied or refused on the zone as the module's dry runs said. Conditions: `lez_core` runs with our patch (as merged it cannot send these transactions), and a sent transaction's outcome is not observable through Logos Core (`lez_core` reports inclusion; the run read verdicts with the CLI's follower) |
| RFP-021 F.9 | in part | in part (blocked on Logos for the last clause) | `verify_checkpoint`'s beacon checks through Logos Core's `beacon_api_module`, live in Logos Core (3 of 3 agree); the same check drafted as a method of Logos's `beacon_api_module` (applies, 32 of 32 tests) |
| RFP-021 U.2 | in part | **shown** | `ferrata attest` (cli/) on a LEZ v0.4.0-rc1 zone we ran: configuration read, updates submitted, attestations of slot, account and log tuples with predicates, dry runs whose codes (0, 130, 124, 23, 52) equal the zone's for every sent transaction (22 commands) |

(U.3, F.7: unchanged, below.)

## RFP-021 R.7: a stalled instance keeps safety and resumes

**Proposed: shown.** Clause by clause, as the RFP words it:

| Clause | Shown by |
|---|---|
| "An instance left unadvanced does not lose safety: attestations against already-verified headers continue to verify" | Three attestations under record R (slot 11,549,664, Sepolia block 12,105,936): a storage slot with a condition (279,157 execution gas), an ERC-20 `Transfer` log (97,794), an account's code hash (170,523). Asked again after 706,020 s (8.2 days) with no update, at the resume's block time: applied, identical statements and identifiers; the instance unchanged by them. Asked again after eight committee rotations: identical. |
| "a stalled instance resumes correctly when any party submits the outstanding updates" | The 106 updates Sepolia produced after R (recorded by the lab from Sepolia's beacon nodes, periods 1410 to 1417), each checked natively first as an updater does, all sent late at one block time: 106 jobs applied, 334 transactions, none refused; finalized slot 11,549,664 → 11,608,192; two records written on the way equal a second provider's canonical roots (asserted since `bce25e02`). |
| "with no manual repair" | No instruction other than the jobs `Open`, `Miller`, `Adopt`, `Finish`: no re-`Init`, no repair message (the program has none). |
| "and no gap in the verified handoff chain" | A committee adopted from the update of every period 1410 to 1417, in order, none missing (asserted). The newest period's update sent first is refused at `Open` with exit 23 `PeriodSkipped` and changes nothing. |

- Test: `lez/harness/tests/stall.rs` (`a_stalled_instance_keeps_attesting_and_resumes_without_repair`), commit `80a55d7d`;
  `cd lez && RISC0_HOME=$PWD/../../risc0-home-lez RISC0_BUILD_LOCKED=1 RISC0_DEV_MODE=1 cargo test -p harness --locked --test stall -- --nocapture`;
  1 passed, 297.9 s (Modal build box); run again after the review at `bce25e02` (the second-provider comparison now
  asserted): the same output, 1 passed, 315.8 s. Record: [`lez/results/r7-stall.md`](../../lez/results/r7-stall.md).
- Data: `sepolia-gloas-2026-11-10` and `sepolia-attest-2026-11-10` (the instance and R), `lez/lab/results/record/net/sepolia/updates/` (the outstanding updates), real Sepolia only.
- Limits: LEZ's state machine in process, not a sequencer; the stall stays inside R's weak-subjectivity period (it ends at slot 11,662,720; past it the instance is replaced, F.10).
- Also evidence for **R.3** (not this stream's row): an old record read after later committee rotations (eight), identical.

Proposed EVIDENCE row: | new (rfp-B 1) | **A stalled instance keeps attesting and resumes without repair (R.7, also R.3)**: Sepolia's instance left unadvanced for 8.2 days after record R (slot 11,549,664); R's three attestations (slot with condition, `Transfer` log, code hash) verify during the stall and after eight committee rotations, identical; the 106 updates Sepolia produced meanwhile (the lab's record, periods 1410–1417) submitted late in order: 106 jobs, 334 transactions applied, a committee adopted for every period, none skipped; the newest period's update sent first refused (23 `PeriodSkipped`), nothing changed | `lez/harness/tests/stall.rs` (commit `80a55d7d`): 1 passed, 297.9 s | Modal build box; LEZ v0.4.0-rc1's state machine in process, dev mode; real Sepolia data | [`lez/results/r7-stall.md`](../../lez/results/r7-stall.md) |

## RFP-021 S.7: the operator role, including how to detect a stall

**Proposed: in part; the "`monitor` is not built" part is closed.** What remains is the document: the operator guide is a
first draft (rfp-H's). What this stream adds:

- `attest-monitor` ([`monitor/attest/`](../../monitor/attest/README.md)): `instance-stalled` (the instance's finalized header behind the providers' median finalized header by more than `--stall-after`), `ws-half` (past half its weak-subjectivity period by the chain's clock), `ws-expired` (must be replaced), with alerts to a log, a command and standard error, a daemon with a status file and a clean stop.
- Tested on real data (`monitor/attest/tests/monitor.rs`, 12 of 12 pass after the review's fixes; 8 of 8 before): the R.7 test's stalled instance at the resume's time gives lag 702,336 s, `instance-stalled` and `ws-half` (half at slot 11,606,192: the real 8.2-day stall crossed it); the resumed instance gives no alarm and its root equals publicnode's recorded `0x23362d57…5466`; the daemon raises each alarm once, clears both with `recovered` when the instance resumes, stops on SIGTERM.
- Proposed text for the operator guide §3.5 (for rfp-H): "Run `attest-monitor watch --state lez:<sequencer> --program <program> --instance <id> --provider … --provider … --provider …` (three or more beacon providers of different clients). It warns `instance-stalled` when the instance's finalized header lags the providers' finalized header (their median: with three or more providers no single one can raise or hide it) by more than `--stall-after` (default 15,300 s, two missed two-hour sessions plus finality), and raises `ws-half` (critical) when the header is older than half its weak-subjectivity period: submit the outstanding updates then; anyone may, and the instance resumes with no repair (R.7). Past the period it raises `ws-expired`: the instance must be replaced (§ on replacement). Nobody is obliged to run an updater or this monitor."

Proposed EVIDENCE row: | new (rfp-B 2) | **The attestation side's `monitor`: stall, weak-subjectivity and divergence alarms** (S.7, VS.4): `attest-monitor` reads an instance (a LEZ sequencer's `getAccount`, or a file or command) and several beacon providers; on the R.7 test's real Sepolia instance states and providers serving Sepolia's recorded answers: healthy (3 agree), the real 8.2-day stall (`instance-stalled`, lag 702,336 s; `ws-half`), past the period (`ws-expired`), a forged finalized header (`instance-diverged`, the consumer's freeze hook run once with the instance id), the forged header with one provider colluding (still critical, the hook runs, the colluder named), the stated rule with `--diverge-at`, providers behind the instance's slot, unreachable or on another chain not counted, the daemon's alerts on change, an outage held (no false `recovered`), a failed freeze hook retried at the next poll, SIGTERM | `monitor/attest/tests/monitor.rs`: 12 of 12 pass | Modal build box | [`monitor/attest/README.md`](../../monitor/attest/README.md) |

## RFP-021 VS.4: committee-compromise failure modes and what a consumer can do

**Proposed: in part; the monitor gap is closed.** The documentation (`docs/trust.md` §2, §6) is a first draft (rfp-H's).
What this stream adds: the alarm the documentation tells consumers to wire to their freeze authority exists and is tested.

- `instance-diverged` (critical) when at least `--diverge-at` providers (default 1: any provider) report another root, or no block, at the instance's finalized slot, a slot they have finalized, whatever the others report: what a captured committee's forged handoff or finality leaves. A provider that reports the instance's root does not cancel it (it may collude with the forger; the alert names it under `agreeing`). The rule, with `n` providers answering: `f` lying providers raise it when `f` ≥ `--diverge-at`; `f` providers colluding with a forged instance hide it only when `f` > `n` − `--diverge-at` (by default, only when every provider that answers colludes). By default one provider can raise it (a false freeze costs a pause); with `--diverge-at 2` and three or more providers one provider can neither raise nor hide it.
- Tests: `a_forged_finalized_header_is_called_diverged_and_runs_the_consumers_freeze_hook` (the header altered as a captured committee would leave it; the alert names all three providers and the instance; the consumer's freeze tool ran exactly once); `one_colluding_provider_cannot_hide_a_divergence_from_the_freeze_hook` (two honest providers and one reporting the forged root, the freeze tool wired at level critical as the README says: exit 20, the tool ran once, the colluder named; two colluding of three: still critical; only colluding providers answering: confirmed, the stated limit); `the_divergence_rule_one_liar_raises_it_by_default_and_not_with_diverge_at_2`; `a_provider_behind_the_instance_is_not_counted` (providers that have not finalized the slot raise no false divergence); `the_daemon_holds_a_divergence_through_an_outage_and_retries_a_failed_freeze_hook`.
- Before the review's fix (`ac5a78d5`), one colluding provider turned the critical alarm into a warning, so a freeze tool wired at level critical did not run; the earlier text here and in the monitor's README claimed it could not be hidden.
- Proposed text for `trust.md` §2.2 (for rfp-H): "The attestation side's monitor (`monitor/attest`, `attest-monitor`) compares the instance's finalized header with several beacon providers' canonical roots and raises `instance-diverged` (critical) when any of them (by default; `--diverge-at N` for N) has another block at that slot, whatever the others say: a provider that agrees with a forged header may be colluding with the forger. A consumer runs it with three or more providers of different clients, operated by different parties, and wires `--alert-cmd` at level critical to its freeze authority; one dishonest provider can then raise a false alarm, which costs a pause, but cannot hide a real one, which only every provider together can. What can pass before the freeze is the consumer's caps over the time the alarm and the freeze take; size the caps to what it can afford to lose."
- Limits: the divergence is shown on an altered state against loopback providers serving recorded answers, not a real captured committee; the monitor has not run against public providers; a provider past the slot that no longer serves it (pruned history) counts as disagreeing.

## RFP-021 U.4 and RFP-022 U.4: an IDL through the SPEL framework

SPEL's LEZ v0.4 line is logos-co/spel pull request #301 (head `c90ca0b`, 16 Nov 2026, LEZ v0.4.0-rc3; open, not
released: `gh pr view 301 -R logos-co/spel` gave `OPEN`, head `c90ca0b`, on 21 Nov). The status table's header files an
upstream component that is not released under **waits on Logos**, so both rows stay there; what the pull request's own
code does with our programs is the "shown instead". It generates an IDL (version 2) from a program's core crate with no
annotations. Run against our crates, unchanged ([`idl/README.md`](../../idl/README.md), "SPEL v0.4 generates them";
record [`idl/generated/spel-v2.txt`](../../idl/generated/spel-v2.txt); `RECORDED=… bash idl/spel-v2/run.sh`, tree
`18fc505c`, the same results as the first run; reproduced by the review on a fresh box):

- **RFP-021 U.4, proposed waits on Logos; shown instead**: `generate_idl` on `lez/wire` gives 15 messages and 18 types;
  compared with `idl/ferrata_attestation.idl.json`, both ways, docs aside: **0 differences in 33 messages and types**.
  SPEL's own decoder on the generated IDL decodes 21 of 21 committed vectors and **905 of 905 messages recorded on LEZ
  zones** (the lab's 838, the live run's 67; `idl-recorded` leaves out 8 more of an older wire, which today's Rust wire
  does not decode either), each consuming every byte, and refuses each one a byte short, a byte long or with tag 255.
  The comparison covers messages and types: SPEL's version 2 format has no `errors`, `instructions` or `accounts`
  sections, so the hand-written IDL's 71 error codes stay in it alone. The generated file:
  [`idl/generated/ferrata_attestation.idl-v2.json`](../../idl/generated/ferrata_attestation.idl-v2.json). Moves to shown
  when SPEL releases its v0.4 line with this generator.
- **RFP-022 U.4, proposed waits on Logos; shown instead**: the same generator refuses `BridgeMessage::Spend`'s `fees: Vec<(Root, u128)>` ("only
  paths and arrays are supported"). With [`idl/spel-v2/patches/spel-c90ca0b-idl-gen-tuples.patch`](../../idl/spel-v2/patches/spel-c90ca0b-idl-gen-tuples.patch)
  (a tuple described as a struct with positional fields; Borsh encodes both alike) it gives 28 messages and 32 types: 0
  differences in 60 messages and types against `idl/ferrata_bridge.idl.json` (its 16 error codes not compared, as
  above), 50 of 50 vectors decoded. Remains: SPEL releasing its v0.4 line and taking tuple support (the patch is
  drafted for upstream, not opened), or the bridge's wire naming that tuple (which would move the bridge's image id).

Proposed EVIDENCE row: | new (rfp-B 3) | **SPEL's LEZ v0.4 generator, at its open pull request, makes both programs' messages and types from their core crates** (U.4, shown instead; the rows wait on SPEL's release): logos-co/spel PR #301 (`c90ca0b`, LEZ v0.4.0-rc3, open) `generate_idl` on `lez/wire`: the attestation program's messages and types equal to the hand-written IDL's (0 differences in 33; SPEL's version 2 format has no error, instruction or account sections, not compared); SPEL's decoder on it: 21 of 21 vectors and 905 of 905 recorded messages exactly, mutants refused; on `lez/bridge-wire` it stops at a tuple; with our 23-line SPEL patch (tuples as positional structs, not opened) the bridge's messages and types equal to ours (0 in 60), 50 of 50 vectors | `RECORDED=… bash idl/spel-v2/run.sh` (tree `18fc505c`) | Modal build box | [`idl/generated/spel-v2.txt`](../../idl/generated/spel-v2.txt) |

## RFP-021 F.7: the program on testnet, consumed inside the consuming transaction

**Proposed: waits on Logos (unchanged).** LEZ testnet 0.4 does not exist (as of 21 Nov 2026). What this stream adds
towards it, and what is shown instead:

- **The published interface any consuming program can call** is now generated by SPEL's own v0.4 tooling (its open
  pull request) from the program's core crate, its messages and types equal to ours (U.4 above): a consumer's client or
  program can be written from `idl/generated/ferrata_attestation.idl-v2.json` (the error codes from the hand-written IDL).
- **The calls a consuming program makes, against a running sequencer** (`ferrata attest`, U.2 below): an `Attest` to
  `header(id, slot)` dry-run and sent on a zone we ran, its code equal to the zone's.
- Shown instead (unchanged): same-transaction consumption in the harness and on rc1 and rc3 sequencers we ran (the
  bridge's `ImportRoot` calls `Attest` and acts on the reply in one transaction; EVIDENCE rows 3, 27, 91, 123).
- The day-one path on testnet 0.4: the day-one kit (`lez/testnet04/day1.sh`, another stream's) deploys the program,
  starts an instance and its updater and sends one `Attest`; the consuming step for day one is the bridge's
  `ImportRoot` under that instance once the owner deploys a vault for the testnet zone (an owner step on Sepolia), or,
  with no Sepolia step, the reference consumer of U.6 (`ConsumerMessage` claim of a public ERC-20 `Transfer`, which
  calls `Attest` and credits in the reply), deployed with the same loader as the program. Neither is added to the kit
  here (not this stream's file); both run today in the harness (`lez/harness/tests/attest_public.rs`,
  `vault_consumer.rs`).
- Blocker: LEZ testnet 0.4 itself (Logos).

## RFP-021 U.3: the update submitter as a Logos module with a headless daemon; the operator journey

**Proposed: in part (unchanged).** Not reached in this session beyond what other rows give it:
- The operator's stall detection exists now (`attest-monitor`, S.7 above), the "monitor" step of the journey
  (install, configure, run, monitor).
- Remains: the updater module on a sequencer with both its Ethereum sources through Logos Core's `beacon_api_module`
  (row 118 did it on the in-process chain; row 104 ran the module on a sequencer with direct HTTP sources), and the
  operator guide's final text (rfp-H). The run would be `cli/tests/attest-seq.sh` with a hook that installs
  `ferrata_updater_module` (sequencer backend) and `beacon_api_module` into a Logos Core beside the zone, with
  `beacon_api_module`'s providers on loopback serving the lab's recorded light-client data
  (`lez/lab/results/record/net/sepolia/`), and `attest-monitor watch` on the instance during the run.

## RFP-021 U.2: the CLI: attest, dry run with the on-chain codes, submit updates, read configuration

**Proposed: shown.** `ferrata attest` (`cli/src/attest.rs`, the `ferrata` tool's subcommand; reference
[`lez/ATTEST-CLI.md`](../../lez/ATTEST-CLI.md), "On a running sequencer"), run against a LEZ v0.4.0-rc1 sequencer we started
(`bash cli/tests/attest-seq.sh r4`, Modal build box, tree `4db5d1d2`; record
[`cli/results/attest-seq-r4/`](../../cli/results/attest-seq-r4/README.md)):

| Clause | Shown by (run r4) |
|---|---|
| "producing an attestation for a given (chain, contract, storage slot or log, predicate) tuple" | `attest --storage ADDRESS:SLOT:CMP:VALUE`, `--account ADDRESS:codehash`, `--log TX:LOG:emitter=ADDRESS` under the mainnet record of block 26,384,851 (chain 1), proofs over JSON-RPC: applied on the zone (WETH balance slot `eq` and `ge`, a USDC `Transfer` log with its emitter, WETH's code hash, a slot and a log together) |
| "verifying one off-chain as a dry run" | every command without `--send`: the program's function natively and the signed transaction settled on a copy of the zone's state; a dry run changes nothing (status before and after) |
| "submitting a consensus-state update" | `advance --send --yes`: two real Sepolia updates (8 transactions) and a mainnet finality update applied; `init` of an instance |
| "reading an instance's configuration" | `config`: chain id, committee, fork schedule, layout, bound; the instance id derived again from it and the checkpoint, equal (both instances) |
| "The dry run must report the same typed error codes the on-chain path returns" | refusals 130 `PredicateFailed` (twice), 124 `ReceiptFailed`, 23 `PeriodSkipped`, 52 `NotNewer`: the verdict, the dry run and the zone's code equal for each (the CLI exits 1 on any difference); every sent transaction equal to its dry run in exit code, execution gas, storage gas and number of events (receipts carry no event data) |

Limits: a zone we ran (LEZ's local configuration, one sequencer), not a public network; the zone's outcome read by
following its blocks with LEZ's block application (the sequencer's RPC has no receipts), as every sequencer row of the
table is read: the code is LEZ's own re-execution of the included transaction, anchored to the sequencer by its reverted
list and fee totals. Run r4's 24 lines are 22 commands and 2 monitor reads; its hook checked each command's exit status
(0 or 3), and the codes above are read from the outputs. After the review the hook expects each refusal's code itself
(`d3aff4e2`; run `rf1` below).

Proposed EVIDENCE row: | new (rfp-B 4) | **The attestation CLI against a running LEZ sequencer** (U.2): `ferrata attest` on a LEZ v0.4.0-rc1 zone with the final attestation program: `config` (ids derived again, equal), `advance` with real Sepolia updates (8 transactions, slot 11,562,112 → 11,567,104) and a mainnet finality update, `init` of a mainnet instance, `attest` of slot, account and log tuples with predicates over JSON-RPC proofs (applied: 94,795 to 349,917 gas), refusals 130, 124, 23 and 52 equal across verdict, dry run and zone; every sent transaction equal to its dry run (exit, execution and storage gas, number of events); 22 of 22 commands as expected | `bash cli/tests/attest-seq.sh r4` (tree `4db5d1d2`) | Modal build box; a zone we started; recorded Ethereum data | [`cli/results/attest-seq-r4/`](../../cli/results/attest-seq-r4/README.md) |

The same run read the zone's mainnet instance with `attest-monitor` through the sequencer's `getAccount` (S.7):
divergence confirmed, no alert at the record's time; `ws-half` at the run's time (the record is 11 days old).

## RFP-021 F.9: parameters fixed at Init; an API of the Logos Ethereum core module to verify a checkpoint

**Proposed: in part, blocked on Logos for its last clause** (the API must be Logos's module's; we drafted and tested the
change, not opened). The first clause (checkpoint, chain, chain id and finality parameters written at `Init`, never
changed) is shown as before (rows 18, 69, 70, 91; F.13). What this stream adds to the second:

- `ferrata_attest_module.verify_checkpoint` now reads **every Ethereum source through Logos Core's own Ethereum
  modules** when asked: the beacon checks through `beacon_api_module` (`"providers": {"via": "beacon_api_module",
  "chain_id": N}`: its providers for the chain, each asked pinned through `raw_get`, counted by endpoint, host and port: run r6's three
  are three ports of 127.0.0.1), the
  execution check through `eth_rpc_module` (row 117, `verified_proxy_module` in `required` mode). Test
  `the_beacon_checks_through_beacon_api_module_give_the_plain_paths_verdicts` (calls crate, 22 of 22 pass): with
  `beacon_api_module` answering from the live run's record of Lodestar, publicnode and Nimbus
  (`calls/tests/fixtures/verify-checkpoint-sepolia.record.json`), the three agree, with the plain path's slot and
  verdict each; pinned to two of them, two agree and the check fails; an id the module does not have is refused.
- **The upstream change, drafted and tested**: [`modules/ferrata_attest_module/upstream-drafts/beacon_api_module-aca138e-verify-checkpoint.patch`](../../modules/ferrata_attest_module/upstream-drafts/README.md)
  adds `verify_checkpoint(chain_id, root, expect_json)` to Logos's `beacon_api_module` itself (every enabled provider
  pinned: canonical, canonical at its slot, finalized there, genesis and deposit chain id; agreeing hosts counted); it
  applies to `aca138e` and the crate's tests pass with it (32 of 32, three new). With it merged, the API is the Logos
  module's.
- **Live in a headless Logos Core** (run r6, [`cli/results/attest-seq-r6/`](../../cli/results/attest-seq-r6/README.md)):
  `beacon_api_module` (aca138e, built from source with builder 5f74cdb's locks) configured with three providers on
  loopback answering what Lodestar, publicnode and Nimbus answered in the recorded live run of 13 Nov;
  `ferrata_attest_module.verify_checkpoint` with `providers: {via: beacon_api_module}`: verdict ok, the three agree
  through `beacon_api_module.raw_get` (slot 11,562,112). With row 117 (the execution check through `eth_rpc_module` and
  `verified_proxy_module`), every Ethereum read of the check goes through Logos Core's own Ethereum modules.
- Remains: the API being Logos's own (the drafted change above, to be opened and merged by Logos; its glue method is
  compiled only by the module builder, which no run built: drafted and unit-tested), and a live run against public
  providers (the brief of this stream allows recorded data only).

Proposed EVIDENCE row: | new (rfp-B 5) | **The checkpoint check's beacon reads through Logos Core's `beacon_api_module`, and the check drafted as a method of it** (F.9): `ferrata_attest_module.verify_checkpoint` in a headless Logos Core with `beacon_api_module` (aca138e, built from source), its three providers on loopback answering the recorded answers of Lodestar, publicnode and Nimbus: verdict ok, 3 of 3 agree through `raw_get`; replay test against the same record equal to the plain path; `beacon_api_module.verify_checkpoint` drafted for upstream (canonical, canonical at its slot, finalized, genesis and deposit chain id; hosts counted), applies to aca138e, 32 of 32 tests of its crate pass | run r6 of `cli/tests/attest-seq.sh` with `logos-run-hook.sh`; `cargo test` (calls crate, 22 of 22); `cargo test --no-default-features` (beacon_api_module with the patch) | Modal build box; recorded provider answers | [`cli/results/attest-seq-r6/`](../../cli/results/attest-seq-r6/README.md), [`modules/ferrata_attest_module/upstream-drafts/`](../../modules/ferrata_attest_module/upstream-drafts/README.md) |

## RFP-021 U.1: the attestation functionality in a Logos core module; LEZ through Logos Core's modules

**Proposed: shown.** The module already produced, verified and read configuration (rows 59, 60, 93, 108, 116); its
Ethereum reads already went through `eth_rpc_module`. What was missing, its LEZ reads and sends through Logos Core's
modules, is built and was run (run r6 below). Built in this stream:

- `ferrata_attest_module.lez` (opens a LEZ wallet inside Logos Core's `lez_core`), `lez_read` (an instance, a record or a
  job through `lez_core.get_account_public`), `lez_submit` (the steps of `advance`, or `build_attest_tx`'s transaction,
  through `lez_core.send_generic_public_transaction`, each waited for with `poll_transaction_status`); unit test of the
  calls' shapes (calls crate, 22 of 22 pass).
- The run: `modules/ferrata_attest_module/scripts/logos-build.sh` (the module's dev `.lgx` and `logosctl` through
  `modules/scripts/check.sh`; `lez_core` cc73ebf with our patch; `beacon_api_module` aca138e with builder 5f74cdb's
  locks), then `cli/tests/attest-seq.sh` with `ATTEST_SEQ_HOOK=modules/ferrata_attest_module/scripts/logos-run-hook.sh`:
  the zone of U.2, the module and `lez_core` in a headless Logos Core, the Sepolia instance read through `lez_core`, an
  update planned by the module on that state and its job sent through `lez_core`, a log attested through the module and
  sent through `lez_core`, a refused one the same way, each transaction's verdict read from the zone's receipts
  (`ferrata attest receipt`).

Run r6 ([`cli/results/attest-seq-r6/`](../../cli/results/attest-seq-r6/README.md); `ATTEST_SEQ_HOOK=modules/ferrata_attest_module/scripts/logos-run-hook.sh
bash cli/tests/attest-seq.sh r6`, Modal build box, tree `002ebb96`), clause by clause ("Build the attestation
functionality for consumers and operators in a Logos core module ...: producing an attestation, verifying one,
submitting a consensus-state update, and reading the checkpoint and chain configuration of an instance"):

| Clause | Through Logos Core (run r6) |
|---|---|
| producing an attestation | `fetch_log_proof` → `build_attest_tx` → `lez_submit` (`lez_core.send_generic_public_transaction`): a USDC `Transfer` of mainnet block 26,384,851 attested under the zone's record, applied (93,316 execution gas, 1 event) |
| verifying one | `dry_run_attest` on the record read through `lez_core`: code 0, then 110 for a tampered proof; the zone refused that one with 110 |
| submitting a consensus-state update | `advance` on the instance state read through `lez_core`, its job (`Open`, `Miller`, `Finish`) sent through `lez_core`: applied; finalized slot 11,567,104 → 11,567,392 |
| reading the checkpoint and chain configuration | `lez_read` through `lez_core.get_account_public`: the Sepolia instance (585 bytes), the mainnet instance (505) and record (365), each byte-equal to the plain path's; decoded as `read_instance` decodes them (configuration, store, id) |

Conditions of "shown", stated with it: (1) `lez_core` runs with our patch: built from its merged v0.4 state, a generic
send mentions no account authorizing, so as merged it cannot send these transactions (the patch is the bridge stream's,
`modules/patches/`); (2) a sent transaction's outcome is not observable through Logos Core: `lez_core` reports
inclusion only, so the run read every verdict with the CLI's follower (`ferrata attest receipt`), and a UI module
learns an `Attest`'s result only from the dry run beforehand. Limits: one zone we ran; the payer is a public account of
a wallet holding LEZ's published local test key. The packages' hashes in the run's record are what ran: rebuilt from
scratch by the review, the module package was bit-identical, while `lez_core`'s and `beacon_api_module`'s were not
(their builds are not bit-reproducible across boxes).

Proposed EVIDENCE row: | new (rfp-B 6) | **The attestation module's LEZ reads and sends through Logos Core's own `lez_core`** (U.1): `ferrata_attest_module` in a headless Logos Core (`logosctl` 66d098d) beside `lez_core` (cc73ebf, patched) and `beacon_api_module`, on a LEZ v0.4.0-rc1 zone with the final attestation program: three actor states read through `lez_core.get_account_public`, each byte-equal to the plain path's; an update's job (3 transactions) planned by the module on that state and sent through `lez_core.send_generic_public_transaction`, applied (slot 11,567,104 → 11,567,392); a log attested through the module and sent through `lez_core`, applied (93,316 gas); a tampered one refused on the zone with 110, as the module's dry run said | run r6 of `cli/tests/attest-seq.sh` with `logos-run-hook.sh` (tree `002ebb96`); packages from `logos-build.sh` | Modal build box; a zone we started; recorded Ethereum data | [`cli/results/attest-seq-r6/`](../../cli/results/attest-seq-r6/README.md) |
