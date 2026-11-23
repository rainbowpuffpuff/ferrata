//! The attestation side's measured costs, pinned through the harness: RFP-021 performance
//! #3, #7, #8 and #9 have records and derived tables but no test, so this file re-measures
//! through the same scenarios and fails when a pinned number moves, which means the document
//! row that rests on it must be revisited. The four clauses, by test:
//!
//! - [`the_per_period_updates_cost_what_the_record_shows_and_do_not_grow`]: #3 "Document the
//!   amortised per-attestation cost of sync-committee tracking at the recommended operating
//!   cadence, separately from the per-attestation verification cost" (the per-period input: one
//!   adopting update's total, over its five transactions), #8 "Document the cycle-count delta
//!   between the in-program BLS12-381 path and a hypothetical native precompile" (the two
//!   pairing steps against the update's total), #7 "Document the growth rate and on-chain
//!   storage cost of any module state that accumulates with usage, in particular
//!   committee-tracking state" (the sizes of the record, the job account and the instance), and
//!   #9 "No action's cycle count or gas cost scales with accumulated on-chain storage" (every
//!   step of every update, while the records the chain holds grow from one to five);
//! - [`the_finality_update_costs_what_the_hourly_row_assumes`]: #3's per-hour input (a finality
//!   update, three transactions) and #8 again — `docs/performance.md` §P.8 states the comparison
//!   on this shape ("7.0M of an update's 9.2M public-path cycles");
//! - [`an_update_verified_privately_costs_what_it_is_charged`]: #3's proven-path input;
//! - [`an_attestation_costs_the_same_whatever_the_chain_holds`]: #3's other half — the
//!   per-attestation verification cost — and #9 again, measured before and after a thousand more
//!   records are placed on the chain, for the WETH balance slot and for every statement type of
//!   `lez/results/stage4-attest.md` (the absent account, the absent slot and the four facts among
//!   them); #7's record account at a scale the replay cannot reach;
//! - [`init_and_succeed_cost_the_same_whatever_the_chain_holds`],
//!   [`an_update_verified_in_one_call_costs_the_same_whatever_the_chain_holds`] and
//!   [`the_reference_consumer_costs_the_same_whatever_the_chain_holds`]: #9 for every other action
//!   (`harness::accumulated`): `Init` and its adoption, `Succeed` and `Spawn`, `VerifyAll` in a
//!   public and in a private transaction (the proven update path), `Read` and `ReadPadded`,
//!   `CallAttest` and the reference consumer's claim, each charged the same on the chain its
//!   scenario leaves and with a thousand more of each account that accumulates beside it. With the
//!   update steps above, every message the attestation program handles;
//! - [`the_leaf_statement_costs_the_same_whatever_the_chain_holds`]: #9 for the accumulator-leaf
//!   statement and the vault's slots under a record that binds a block hash (the synthetic vault);
//! - [`the_vault_pattern_costs_the_same_whatever_the_chain_holds`]: #9 for the reference consumer's
//!   vault pattern on FerrataVault v5's real deposits (`ClaimDeposit`, `ClaimDepositPrivately` with
//!   its public `Spend` and `CreditHolder`), and the one cost found to grow with use, the claimant's
//!   own private proof against the pools it holds (pinned at 0 and 100 pools).
//!
//! The private transactions also run with a thousand more commitments in LEZ's commitment set: the
//! fee payer's membership path, which LEZ's circuit hashes one hash per level, grows from 3 to 10
//! levels and LEZ's charge stays the same. The circuit's own cycles grow by 1,935 a level; LEZ's API
//! drops them, r0vm logs them (`RISC0_INFO=1 RUST_LOG=risc0_zkvm=info` with `--nocapture`), and
//! `lez/results/perf-state-actions.md` records them (`bench/perf/state-actions.sh`).
//!
//! Every pinned number cites a committed record:
//!
//! - `lez/results/v0.4.0-rc3/rc1-rerun/harness-tests-dev-mode.log`: this tree's programs in
//!   RISC Zero dev mode against LEZ v0.4.0-rc1 — the tag this harness builds against by default
//!   — worktree bd95305, 14 November 2026 (`environment.md` in that directory names the box).
//!   The rows pinned here are the printouts of `mainnet_replay_through_lez`,
//!   `a_finality_update_is_three_transactions`,
//!   `an_update_verified_privately_applies_to_the_public_instance` and
//!   `every_kind_of_fact_is_attested_under_the_real_record`.
//! - `lez/results/v0.4.0-rc3/stage1-replay.md` and `…/stage1-finality.md`: the same source on
//!   rc3 — every row equal to that run's (`…/compare-v0.4.0-rc1.md`: 73 of 73 and 21 of 21) —
//!   the only committed record that also holds each handler's post-state bytes.
//! - `lez/results/perf-state-actions.md` (`bash bench/perf/state-actions.sh`): the scenarios of
//!   `harness::accumulated` swept to 100,000 of each account; its K = 0 rows are what the last three
//!   tests pin, and where an earlier record holds the same transaction, its constant names it.
//! - `docs/performance.md` (§P.3, §P.7, §P.8) and `docs/design/attestation.md` (§3.1, §4.2):
//!   which document row each number carries, and where the document's figure is an earlier
//!   image's.
//!
//! The top-level `lez/results/stage1-replay.md` and `lez/results/stage1-finality.md` predate the
//! successors merge, so their record account is 333 bytes (no root id then) and their cycles are
//! the earlier image's; `lez/RESULTS.md` names the differences. Where `docs/performance.md`
//! still quotes one of those figures (§P.7's "333 B" record, its "1,249–1,409 B" job, its
//! "425–505 B" instance), this file pins what the current code produces — the record account is
//! 365 bytes — and it is that document row which has moved.

use fee_core::market;
use ferrata_core::{
    fork::LcFork,
    types::LightClientUpdate,
    validate::Context,
    witness::{uncompressed_keys, CompactUpdate},
};
use ferrata_lez_wire::{
    attest::{Cmp, Field, Request},
    header_actor, job_actor, Message,
};
use harness::trace::TransitionTrace;
use harness::{
    accumulated::{self as acc, Charged},
    as_finality_update,
    attest::{account_query, condition, u256, Stage4},
    ferrata_account, grouped,
    private::catalog,
    root,
    scenarios::{self, World},
    vault, Instance, TxReport, EXPECTED_FINALIZED, GENESIS_VALIDATORS_ROOT,
};
use lee::Actor;

// ---------------------------------------------------------------------------------------------
// The pinned figures
// ---------------------------------------------------------------------------------------------

// The mainnet replay (`scenarios::replay(2)`, five adopting updates). Record: the
// `mainnet_replay_through_lez` printout of the dev-mode log named in the module header, with the
// handler post-state bytes of `lez/results/v0.4.0-rc3/stage1-replay.md`.

/// Step 1 of update `i` (`Open`, then `JobOpen`). The one step whose cycles vary: with the
/// non-signer witness its update carries (the message bytes below, 2,391–3,153 across the
/// record), not with the state the chain holds — `docs/design/attestation.md` §4.2 quotes these
/// as "2.13–2.18M cycles".
const OPEN_CYCLES: [u64; 5] = [2_129_836, 2_186_674, 2_187_248, 2_131_882, 2_159_662];
const OPEN_MSG_BYTES: [u64; 5] = [2_391, 3_143, 3_153, 2_390, 2_759];
const OPEN_TX_GAS: [u64; 5] = [2_156_300, 2_213_138, 2_213_712, 2_158_346, 2_186_126];
const OPEN_TX_STOR: [u64; 5] = [2_853, 3_605, 3_615, 2_852, 3_221];

/// `JobOpen`, the record's flat number in every update.
const JOB_OPEN_CYCLES: u64 = 26_464;
/// Step 2, the Miller loop: flat in every update (`docs/performance.md` §P.8's "3.31M").
const MILLER_CYCLES: u64 = 3_313_372;
/// The adoption's two transactions of 256 keys each (`attestation.md` §4.3): flat.
const ADOPT1_CYCLES: u64 = 3_410_995;
const ADOPT2_CYCLES: u64 = 3_418_189;
/// Step 3, the final exponentiation: flat (`docs/performance.md` §P.8's "3.65M").
const FINISH_CYCLES: u64 = 3_653_838;
/// `Apply`: update_0 re-finalizes the checkpoint's own slot and sends no `Record`; every other
/// update finalizes a new header, so `Apply` carries a record write (`Record` after it).
const APPLY_NO_RECORD: u64 = 45_260;
const APPLY_WITH_RECORD: u64 = 65_473;
/// The `Record` handler, one execution per new finalized header.
const RECORD_CYCLES: u64 = 17_350;

/// The flat steps' transactions run one handler each, so their execution gas equals the
/// handler's cycles and their transaction size is constant across updates.
const MILLER_TX_STOR: u64 = 399;
const ADOPT_TX_STOR: u64 = 24_980;

/// Step 3's transaction (`Finish`, then `Apply`, then `Record` for updates 1–4).
const FINISH_TX_GAS: [u64; 5] = [3_699_098, 3_736_661, 3_736_661, 3_736_661, 3_736_661];
const FINISH_TX_STOR: [u64; 5] = [463, 527, 527, 527, 527];

/// #8: an adopting update's two pairing steps — the cycles a native precompile of the pairing
/// would remove — against its total, the share printed beside them.
const PAIRING_CYCLES: u64 = MILLER_CYCLES + FINISH_CYCLES;

/// #3: each update's total over its five transactions, the number the "once per period (a
/// handoff)" row of `docs/performance.md` §P.3 is derived from (its 16,076,120 is the earlier
/// image's; the current build charges 16,092,355 for update_1, about 5.17 billion a year at the
/// 321 periods the row assumes). Update_0 only supplies the next committee, so it finalizes no
/// new header and its `Finish` transaction carries no `Record`.
const UPDATE_TX_GAS: [u64; 5] = [15_997_954, 16_092_355, 16_092_929, 16_037_563, 16_065_343];
const UPDATE_TX_STOR: [u64; 5] = [53_675, 54_491, 54_501, 53_738, 54_107];

/// #7: the accounts that accumulate with usage. A record is 365 bytes of actor state — a
/// 289-byte `ferrata_core::witness::HeaderRecord` encoding plus the instance id, the chain id, a
/// length and (since the successors merge) the root id; `lez/RESULTS.md` names the change
/// ("365 bytes of state, not 333"), and `docs/performance.md` §P.7's "333 B" is the figure
/// before it. The instance holds its committees as fixed 80-byte summaries, so it does not grow
/// with usage: 505 bytes as `Init` leaves it, 585 once an update supplies the next committee's
/// summary (§P.7's "425–505 B" is the earlier image's; `attestation.md` §3.1). A job's account
/// exists only while the job runs — `Finish` clears it — so it does not accumulate either.
const RECORD_STATE_BYTES: usize = 365;
const RECORD_CONTENT_BYTES: usize = 289;
const INSTANCE_AT_INIT: usize = 505;
const INSTANCE_WITH_NEXT: usize = 585;
const JOB_STATE: [usize; 4] = [1_281, 1_377, 1_409, 1_441];

// The finality update of `scenarios::finality_path` (three transactions) and the job after it
// that only supplies the next committee. Record: the `a_finality_update_is_three_transactions`
// printout of the same log, with the post-state bytes of `lez/results/v0.4.0-rc3/
// stage1-finality.md`. `docs/design/attestation.md` §4.2's "9,216,121" for this row is the
// earlier image's; the current build charges 9,228,905, the input of `docs/performance.md`
// §P.3's "once per hour while importing" row (24 a day: 221,493,720 execution gas).

const FIN_OPEN_CYCLES: u64 = 2_171_611;
const FIN_JOB_OPEN_CYCLES: u64 = 23_291;
/// The pairing steps of a finality job: slightly cheaper than an adopting job's, because the
/// job account it loads is smaller (1,022–1,118 bytes, `attestation.md` §3.1).
const FIN_MILLER_CYCLES: u64 = 3_308_048;
const FIN_FINISH_CYCLES: u64 = 3_647_652;
const FIN_APPLY_CYCLES: u64 = 60_953;
const FIN_PAIRING_CYCLES: u64 = FIN_MILLER_CYCLES + FIN_FINISH_CYCLES;
const FIN_OPEN_TX_GAS: u64 = 2_194_902;
const FIN_OPEN_TX_STOR: u64 = 3_525;
const FIN_FINISH_TX_GAS: u64 = 3_725_955;
const FIN_FINISH_TX_STOR: u64 = 527;
const FIN_TOTAL_GAS: u64 = 9_228_905;
const FIN_TOTAL_STOR: u64 = 4_451;
const FIN_JOB_STATE: [usize; 2] = [1_022, 1_118];

/// The job that only supplies the next committee, in the same world (after the finality update
/// applied): adoption without a new header.
const NC_OPEN_CYCLES: u64 = 2_185_144;
const NC_OPEN_TX_GAS: u64 = 2_211_608;
const NC_OPEN_TX_STOR: u64 = 3_605;
const NC_TOTAL_GAS: u64 = 16_053_262;
const NC_TOTAL_STOR: u64 = 54_427;

// #3, the proven path (`VerifyAll` in one private call, settled through LEZ's private fee path;
// RISC Zero dev mode). Record: the `an_update_verified_privately_applies_to_the_public_instance`
// printout of the same log: "charged: execution gas 485,298 (PRIVATE_VERIFY_GAS 409,764 + public
// part 75,534 cycles)" and "handoff verified privately: execution gas 451,895 (public part
// 42,131 cycles)". `docs/performance.md` §P.3's proven rows (479,854; 448,530) are the earlier
// image's.

const FINALITY_PRIVATE_CYCLES: u64 = 75_534;
const FINALITY_PRIVATE_GAS: u64 = 485_298;
const HANDOFF_PRIVATE_CYCLES: u64 = 42_131;
const HANDOFF_PRIVATE_GAS: u64 = 451_895;
/// The flat parts of a private transaction's charge: LEZ's receipt gas and the private
/// transaction's storage gas (`fee_core::market`, §"Where things were measured" of
/// `docs/performance.md`).
const PRIVATE_VERIFY_GAS: u64 = 409_764;
const PRIVATE_GAS_STOR: u64 = 224_063;

// #3's per-attestation half and #9: the attestation of a WETH storage slot under the record of
// the finalized block (`Stage4`, the fixture of 10 November 2026). Record: the
// `every_kind_of_fact_is_attested_under_the_real_record` printout of the same log, the row
// "storage slot: a WETH balance, value >= 1" (275,865 cycles, 7,338 storage gas).

const ATTEST_CYCLES: u64 = 275_865;
const ATTEST_STOR: u64 = 7_338;
/// How many more records the second attestation of #9's measurement runs against — a tenth of
/// the 9,081 records a year of `docs/performance.md` §P.7's cadence projection.
const MORE_RECORDS: u64 = 1_000;

// #9 for every other action (`harness::accumulated`). Record: `lez/results/perf-state-actions.md`,
// the K = 0 rows (the sweep there goes to 100,000 of each account, here to `MORE_RECORDS`).

/// A transaction's pinned charge: each handler's cycles in order, then its execution and storage
/// gas.
struct Tx {
    handlers: &'static [(&'static str, u64)],
    exec: u64,
    stor: u64,
}

/// `Init` and its committee's adoption in two parts (`scenarios::World::new(2)`); also rows 1–3 of
/// `lez/results/v0.4.0-rc3/stage1-replay.md`.
const INIT_TXS: [Tx; 3] = [
    Tx { handlers: &[("Init", 191_616), ("JobOpen", 19_050)], exec: 210_666, stor: 1_896 },
    Tx { handlers: &[("Adopt", 3_399_141)], exec: 3_399_141, stor: 24_980 },
    Tx { handlers: &[("Adopt", 3_409_234), ("Apply", 58_991), ("Record", 17_350)], exec: 3_485_575, stor: 25_108 },
];
/// `Succeed` at the mainnet instance after update_0 (a successor with Gloas scheduled at epoch
/// 600,000), and the `Spawn` it sends.
const SUCCEED_TX: Tx = Tx { handlers: &[("Succeed", 45_512), ("Spawn", 25_098)], exec: 70_610, stor: 697 };
/// update_1 as a finality update in one public transaction, after update_0; also
/// `lez/results/v0.4.0-rc3/stage1-one-transaction.md`.
const VERIFY_ALL_TX: Tx = Tx {
    handlers: &[("VerifyAll", 9_049_362), ("ApplyVerified", 58_184), ("Record", 17_350)],
    exec: 9_124_896,
    stor: 4_105,
};
/// The same update in one private transaction: the private execution's cycles in RISC Zero's
/// executor (what the sender proves; LEZ charges it the flat `PRIVATE_VERIFY_GAS`; the same
/// handler, message and empty pre-state as the public transaction's `VerifyAll`), then the public
/// part's (`FINALITY_PRIVATE_CYCLES`, the record of `an_update_verified_privately_costs_what_it_is_charged`).
const PRIVATE_VERIFY_ALL_TX: Tx = Tx {
    handlers: &[("VerifyAll, private execution (executor)", 9_049_362), ("public part", FINALITY_PRIVATE_CYCLES)],
    exec: FINALITY_PRIVATE_GAS,
    stor: PRIVATE_GAS_STOR,
};
/// The eight statement types, each one `Attest` (cycles, which are its execution gas; storage
/// gas): rows 1–8 of `lez/results/stage4-attest.md`, the absent slot (3), the absent account (4)
/// and the four facts (8) among them.
const STATEMENTS: [(u64, u64); 8] = [
    (168_909, 4_424),
    (275_865, 7_338),
    (270_580, 7_291),
    (151_829, 4_150),
    (97_485, 2_623),
    (1_619_960, 58_812),
    (499_277, 13_578),
    (454_255, 12_309),
];
/// `CallAttest` of the WETH balance slot from an empty reader (row 1 of
/// `lez/results/stage4-attest-consumer.md`), then from the reader holding that reply (each call
/// replaces it, so a third call costs the second's).
const CALL_ATTEST_EMPTY: Tx = Tx {
    handlers: &[("consumer CallAttest", 87_410), ("Attest", 279_023), ("consumer (reply)", 19_748)],
    exec: 386_181,
    stor: 7_507,
};
const CALL_ATTEST_HOLDING: Tx = Tx {
    handlers: &[("consumer CallAttest", 93_003), ("Attest", 279_023), ("consumer (reply)", 22_977)],
    exec: 395_003,
    stor: 7_507,
};
/// The fee payer's membership path in LEZ's commitment tree (levels) as each private scenario leaves
/// the chain, and with a thousand more commitments (1,00x leaves: 2^10).
const PATH_LEVELS: (u64, u64) = (3, 10);
/// The accumulator-leaf statement and the vault's slots on the synthetic vault (cycles, which are
/// the execution gas; storage gas): rows 1–5 of `lez/results/stage5-vault-synthetic.md` — slots 0
/// and 1 alone, with one, two and eight deposit leaves under a record that binds the block hash, and
/// with one leaf under a record that holds the payload's fields.
const LEAVES: [(u64, u64); 5] =
    [(122_067, 2_319), (187_786, 3_391), (253_107, 4_463), (646_014, 10_895), (167_123, 2_700)];
/// The reference consumer's private read (`AttestPrivately`): its two private transitions in the
/// executor and the public part (`Read`), charged the flat receipt gas plus `Read`'s 12,268
/// (`docs/performance.md` §P.1's "422,032 (flat 409,764 + 12,268)").
const ATTEST_PRIVATELY_TX: Tx = Tx {
    handlers: &[
        ("AttestPrivately, private (executor)", 34_717),
        ("the record's reply, private (executor)", 346_763),
        ("public part", 12_268),
    ],
    exec: 422_032,
    stor: PRIVATE_GAS_STOR,
};
/// The vault pattern's public claim of FerrataVault v5's deposit 0 (row 1 of
/// `lez/results/stage5-consumer-public.md`), then the same to a holder credited before.
const CLAIM_DEPOSIT_FIRST: Tx = Tx {
    handlers: &[
        ("consumer ClaimDeposit", 114_234),
        ("Attest", 333_071),
        ("consumer (reply)", 36_995),
        ("consumer CreditHolder", 15_906),
    ],
    exec: 500_206,
    stor: 9_105,
};
const CLAIM_DEPOSIT_CREDITED: Tx = Tx {
    handlers: &[
        ("consumer ClaimDeposit", 114_234),
        ("Attest", 333_071),
        ("consumer (reply)", 36_995),
        ("consumer CreditHolder", 18_521),
    ],
    exec: 502_821,
    stor: 9_105,
};
/// The vault pattern's private claim of deposit 0: its two private transitions in the executor and
/// the public part (`Read`, `Spend`, `CreditHolder`); `lez/results/stage5-consumer-private.md`
/// gives the same transitions and execution gas.
const CLAIM_DEPOSIT_PRIVATELY_TX: Tx = Tx {
    handlers: &[
        ("ClaimDepositPrivately, private (executor)", 112_597),
        ("the record's reply, private (executor)", 1_392_412),
        ("public part", 47_083),
    ],
    exec: 456_847,
    stor: PRIVATE_GAS_STOR,
};
/// A private-kind claim's two private transitions with the claimant holding 0 and 100 other pools:
/// the claimant's own proof grows by about 2,100 cycles a pool.
const HOLDINGS_0: (u64, u64) = (114_317, 1_409_528);
const HOLDINGS_100: (u64, u64) = (213_887, 1_523_672);
/// A public reader's `ReadRecord` (`Read` at the record) with an empty reader (row 1 of
/// `lez/results/v0.4.0-rc3/shell-costs.md`, another record of the same size), then with the reader
/// holding the record it read, then `ReadPadded` with 1,024 bytes of padding.
const READS: [Tx; 3] = [
    Tx {
        handlers: &[("consumer ReadRecord", 12_061), ("Read", 12_268), ("consumer (reply)", 15_081)],
        exec: 39_410,
        stor: 604,
    },
    Tx {
        handlers: &[("consumer ReadRecord", 16_461), ("Read", 12_268), ("consumer (reply)", 17_559)],
        exec: 46_288,
        stor: 604,
    },
    Tx {
        handlers: &[("consumer ReadPadded", 27_561), ("ReadPadded", 18_377), ("consumer (reply)", 17_381)],
        exec: 63_319,
        stor: 1_628,
    },
];
/// The reference consumer's claim, crediting its sender for the first time (row 3 of
/// `stage4-attest-consumer.md`), and the same claim when the sender's balance holds earlier
/// credits.
const CLAIM_FIRST: Tx = Tx {
    handlers: &[
        ("consumer Claim", 37_667),
        ("Attest", 100_517),
        ("consumer (reply)", 31_502),
        ("consumer Credit", 15_703),
    ],
    exec: 185_389,
    stor: 2_768,
};
const CLAIM_CREDITED: Tx = Tx {
    handlers: &[
        ("consumer Claim", 37_667),
        ("Attest", 100_517),
        ("consumer (reply)", 31_502),
        ("consumer Credit", 18_035),
    ],
    exec: 187_721,
    stor: 2_768,
};

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// The pinned numbers of one test: `eq` asserts one measured number against its record and
/// keeps a row, `pin` keeps a row for a value asserted around the call site, and `print` writes
/// the table a CI log shows.
struct Pins {
    rows: Vec<String>,
}

impl Pins {
    fn new() -> Self {
        Self { rows: Vec::new() }
    }

    /// Assert `got` is the number the record gives, and record the row.
    fn eq(&mut self, what: &str, got: u64, want: u64) {
        assert_eq!(got, want, "{what}: the pinned number moved (record: {want})");
        self.pin(what, want);
    }

    /// Record a row for a value asserted or derived at the call site.
    fn pin(&mut self, what: &str, value: u64) {
        self.rows.push(format!("| {what} | {} |", grouped(value)));
    }

    /// Record a derived row (not a pinned number).
    fn note(&mut self, what: &str, value: &str) {
        self.rows.push(format!("| {what} | {value} |"));
    }

    fn print(&self, heading: &str) {
        println!("\n{heading}\n| pinned | value |\n|---|---:|");
        for row in &self.rows {
            println!("{row}");
        }
    }
}

/// Assert `got` is the transaction `want` pins, and record its rows.
fn pin_tx(pins: &mut Pins, got: &Charged, want: &Tx) {
    let handlers: Vec<(&str, u64)> = got.handlers.iter().map(|(h, c)| (h.as_str(), *c)).collect();
    assert_eq!(handlers, want.handlers, "{}: the pinned handlers' cycles moved", got.action);
    pins.note(&format!("{}: handlers (cycles each)", got.action), &got.handlers_text());
    pins.eq(&format!("{}: execution gas", got.action), got.gas_exec, want.exec);
    pins.eq(&format!("{}: storage gas", got.action), got.gas_stor, want.stor);
}

/// The only execution of `handler` in `report`.
fn one<'a>(report: &'a TxReport, handler: &str) -> &'a TransitionTrace {
    report
        .transitions
        .iter()
        .find(|t| t.handler == handler)
        .unwrap_or_else(|| panic!("{}: no {handler} handler ran", report.label))
}

/// #9: `handler`'s cycles, at the transaction `at` of every update's run (all but `Init`'s),
/// pinned to the record's `want`: every update costs what update 0's does, and no update's step
/// exceeds the one before it, while the records the chain holds grow with every job.
fn flat(pins: &mut Pins, world: &World, what: &str, at: usize, handler: &str, want: u64) {
    let seen: Vec<u64> = (1..world.runs.len()).map(|r| one(&world.runs[r].reports[at], handler).cycles).collect();
    assert!(seen.iter().all(|&c| c == want), "{what}: {seen:?}, the record pins {want}");
    assert!(seen.windows(2).all(|w| w[1] <= w[0]), "{what}: {seen:?} grew from one update to the next");
    pins.pin(what, want);
}

/// `VerifyAll` for `update` against the instance's current store, as `tests/private_placement.rs`
/// builds it (whose record this file pins).
fn verify_all(w: &World, update: &LightClientUpdate) -> Vec<u8> {
    let store = w.inst.store(&w.chain);
    let witness = Instance::signer_witness(w.native.core(), update, &w.spec);
    let adoption = if update.next_sync_committee.is_empty() {
        vec![]
    } else {
        uncompressed_keys(&update.next_sync_committee).unwrap()
    };
    borsh::to_vec(&Message::VerifyAll {
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        store: store.encode(),
        update: CompactUpdate::of(update).encode(LcFork::Electra),
        signers: witness.encode(),
        adoption,
    })
    .unwrap()
}

// ---------------------------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------------------------

#[test]
fn the_per_period_updates_cost_what_the_record_shows_and_do_not_grow() {
    let world = scenarios::replay(2);
    assert_eq!(world.runs.len(), 6, "Init, then the five updates");
    assert_eq!(world.chain.reports.len(), 3 + 5 * 5, "Init in 3 transactions, every update in 5");
    let mut pins = Pins::new();

    // #9: the steps the record shows flat, each pinned to one number and asserted on every
    // update — so no update's step can grow with the records accumulated before it. The chain
    // goes from one record (Init's) to five over the run.
    flat(&mut pins, &world, "JobOpen's cycles (in step 1's transaction)", 0, "JobOpen", JOB_OPEN_CYCLES);
    flat(&mut pins, &world, "Miller's cycles (step 2, the Miller loop)", 1, "Miller", MILLER_CYCLES);
    flat(&mut pins, &world, "adopt 1/2's cycles (256 keys)", 2, "Adopt", ADOPT1_CYCLES);
    flat(&mut pins, &world, "adopt 2/2's cycles (256 keys)", 3, "Adopt", ADOPT2_CYCLES);
    flat(&mut pins, &world, "Finish's cycles (step 3, the final exponentiation)", 4, "Finish", FINISH_CYCLES);

    // The gas of the flat steps' transactions follows their only handler's cycles, and their
    // storage gas — the transaction's serialized size — is constant too.
    for i in 0..5 {
        let run = &world.runs[1 + i];
        assert_eq!(run.reports[1].charge.gas_exec, MILLER_CYCLES, "update_{i}: step 2's execution gas");
        assert_eq!(run.reports[1].charge.gas_stor, MILLER_TX_STOR, "update_{i}: step 2's storage gas");
        assert_eq!(
            (run.reports[2].charge.gas_exec, run.reports[2].charge.gas_stor),
            (ADOPT1_CYCLES, ADOPT_TX_STOR),
            "update_{i}: adopt 1/2's transaction"
        );
        assert_eq!(
            (run.reports[3].charge.gas_exec, run.reports[3].charge.gas_stor),
            (ADOPT2_CYCLES, ADOPT_TX_STOR),
            "update_{i}: adopt 2/2's transaction"
        );
    }

    for i in 0..5 {
        let run = &world.runs[1 + i];
        assert_eq!(run.reports.len(), 5, "update_{i}: step 1, step 2, two adoptions, step 3");
        // Step 1 (`Open`, then `JobOpen`): pinned per update, because the cycles vary with the
        // witness the update carries — `Open` receives 2,391–3,153 bytes of message across the
        // record — not with the chain's state.
        let open = one(&run.reports[0], "Open");
        pins.eq(&format!("update_{i}: Open's cycles (step 1)"), open.cycles, OPEN_CYCLES[i]);
        pins.eq(&format!("update_{i}: Open's message bytes"), open.message_bytes as u64, OPEN_MSG_BYTES[i]);
        pins.eq(&format!("update_{i}: step 1's execution gas"), run.reports[0].charge.gas_exec, OPEN_TX_GAS[i]);
        pins.eq(&format!("update_{i}: step 1's storage gas"), run.reports[0].charge.gas_stor, OPEN_TX_STOR[i]);
        // Step 3's transaction: Finish, then Apply, then Record when a new header is finalized
        // (update_0 re-finalizes the checkpoint's slot, so no record and a lighter `Apply`).
        let finish = &run.reports[4];
        pins.eq(
            &format!("update_{i}: Apply's cycles"),
            one(finish, "Apply").cycles,
            if i == 0 { APPLY_NO_RECORD } else { APPLY_WITH_RECORD },
        );
        if i > 0 {
            pins.eq(&format!("update_{i}: Record's cycles"), one(finish, "Record").cycles, RECORD_CYCLES);
            assert_eq!(
                one(finish, "Record").post_state_bytes,
                Some(RECORD_STATE_BYTES),
                "update_{i}: the record account the handler writes"
            );
        }
        pins.eq(&format!("update_{i}: step 3's execution gas"), finish.charge.gas_exec, FINISH_TX_GAS[i]);
        pins.eq(&format!("update_{i}: step 3's storage gas"), finish.charge.gas_stor, FINISH_TX_STOR[i]);
        // #3: the per-period input — one update's total over its transactions.
        let gas: u64 = run.reports.iter().map(|r| r.charge.gas_exec).sum();
        let stor: u64 = run.reports.iter().map(|r| r.charge.gas_stor).sum();
        pins.eq(&format!("update_{i}: the update's total execution gas"), gas, UPDATE_TX_GAS[i]);
        pins.eq(&format!("update_{i}: the update's total storage gas"), stor, UPDATE_TX_STOR[i]);
    }

    // #9: step 1 is the one varying step, and the variation is its witness, not the state: the
    // two updates that run after four more records exist (update_3, update_4) each cost less of
    // it than the two that ran while fewer records were held (update_1, update_2).
    let opens: Vec<u64> = (0..5).map(|i| one(&world.runs[1 + i].reports[0], "Open").cycles).collect();
    assert!(opens[3] < opens[1] && opens[3] < opens[2], "a later update's Open costs more: {opens:?}");
    assert!(opens[4] < opens[1] && opens[4] < opens[2], "a later update's Open costs more: {opens:?}");

    // #8: the two pairing steps of an adopting update against its total — the cycle-count delta a
    // native precompile of the pairing would remove (`docs/performance.md` §P.8).
    let pairing = one(&world.runs[2].reports[1], "Miller").cycles + one(&world.runs[2].reports[4], "Finish").cycles;
    pins.eq("an adopting update's pairing steps (Miller + Finish)", pairing, PAIRING_CYCLES);
    pins.note(
        "the pairing share of its total execution gas",
        &format!("{:.1}% of {}", pairing as f64 / UPDATE_TX_GAS[1] as f64 * 100.0, grouped(UPDATE_TX_GAS[1])),
    );

    // #7: the record account — one per finalized header, written once; five after the replay,
    // none anywhere else. Update_0 re-finalizes the checkpoint's slot, so its `Record` is the
    // one `Init` already wrote.
    for (slot, _) in EXPECTED_FINALIZED {
        let record = world.inst.record(&world.chain, slot).unwrap_or_else(|| panic!("no record at slot {slot}"));
        assert_eq!(record.record.len(), RECORD_CONTENT_BYTES, "slot {slot}: the HeaderRecord encoding");
        assert_eq!(record.instance_id, world.inst.id, "slot {slot}");
        assert_eq!(record.root_id, world.inst.id, "slot {slot}");
        let actor = header_actor(world.inst.program, &world.inst.id, slot);
        pins.eq(
            &format!("the record of slot {slot}: actor state (bytes)"),
            world.chain.actor_state(actor).len() as u64,
            RECORD_STATE_BYTES as u64,
        );
    }
    assert!(world.inst.record(&world.chain, EXPECTED_FINALIZED[4].0 + 1).is_none(), "a record nowhere else");
    pins.pin("the record's HeaderRecord encoding (bytes, every record)", RECORD_CONTENT_BYTES as u64);

    // #7: the job account of an adopting update, at its four steps, and that `Finish` clears it:
    // the job account does not accumulate with updates (`attestation.md` §3.1, "cleared by
    // Finish"; §P.7's "1,249–1,409 B" is the earlier image's).
    for i in 0..5 {
        let run = &world.runs[1 + i];
        let job = [
            one(&run.reports[0], "JobOpen").post_state_bytes,
            one(&run.reports[1], "Miller").post_state_bytes,
            one(&run.reports[2], "Adopt").post_state_bytes,
            one(&run.reports[3], "Adopt").post_state_bytes,
        ];
        assert_eq!(job, [Some(JOB_STATE[0]), Some(JOB_STATE[1]), Some(JOB_STATE[2]), Some(JOB_STATE[3])], "update_{i}");
        let actor = job_actor(world.inst.program, &world.inst.id, &run.job_id);
        assert!(world.chain.actor_state(actor).is_empty(), "update_{i}: Finish clears the job account");
    }
    pins.pin("the job account after JobOpen (bytes)", JOB_STATE[0] as u64);
    pins.pin("the job account after Miller (bytes)", JOB_STATE[1] as u64);
    pins.pin("the job account after adopt 1/2 (bytes)", JOB_STATE[2] as u64);
    pins.pin("the job account after adopt 2/2, its largest (bytes)", JOB_STATE[3] as u64);

    // #7: the instance — committees as fixed 80-byte summaries, so its size does not grow with
    // usage: one size as `Init` leaves it, another once a next committee is adopted.
    pins.eq(
        "the instance's state as Init leaves it (bytes)",
        one(&world.runs[0].reports[2], "Apply").post_state_bytes.unwrap() as u64,
        INSTANCE_AT_INIT as u64,
    );
    for i in 0..5 {
        assert_eq!(
            one(&world.runs[1 + i].reports[4], "Apply").post_state_bytes,
            Some(INSTANCE_WITH_NEXT),
            "update_{i}: the instance's state after Apply"
        );
    }
    pins.eq(
        "the instance's state with a next committee (bytes, every update's Apply)",
        one(&world.runs[1].reports[4], "Apply").post_state_bytes.unwrap() as u64,
        INSTANCE_WITH_NEXT as u64,
    );
    pins.eq(
        "the instance's state as the replay leaves it (bytes)",
        world.chain.actor_state(world.inst.actor()).len() as u64,
        INSTANCE_WITH_NEXT as u64,
    );

    // #7's projection is derived, not pinned: `docs/performance.md` §P.7's cadence is one record
    // an hour while importing plus one per period.
    pins.note(
        "derived at §P.7's cadence (9,081 records a year)",
        &format!(
            "{} bytes of record state a year, {} over five years",
            grouped(9_081 * RECORD_STATE_BYTES as u64),
            grouped(5 * 9_081 * RECORD_STATE_BYTES as u64)
        ),
    );

    pins.print(
        "Pinned: the five mainnet updates through LEZ (scenarios::replay(2))\n\
         record: lez/results/v0.4.0-rc3/rc1-rerun/harness-tests-dev-mode.log, `mainnet_replay_through_lez`,\n\
         and stage1-replay.md's post-state column",
    );
}

#[test]
fn the_finality_update_costs_what_the_hourly_row_assumes() {
    let world = scenarios::finality_path();
    assert_eq!(world.runs.len(), 4, "Init, update_0, the finality update, the next-committee update");
    let mut pins = Pins::new();

    // #3: the per-hour input — a finality update, three transactions — which
    // `docs/performance.md` §P.3's "once per hour while importing" row is 24 of a day.
    let fin = &world.runs[2];
    assert_eq!(fin.reports.len(), 3, "a finality update is three transactions");
    pins.eq("finality update: Open's cycles (step 1)", one(&fin.reports[0], "Open").cycles, FIN_OPEN_CYCLES);
    pins.eq("finality update: JobOpen's cycles", one(&fin.reports[0], "JobOpen").cycles, FIN_JOB_OPEN_CYCLES);
    pins.eq("finality update: step 1's execution gas", fin.reports[0].charge.gas_exec, FIN_OPEN_TX_GAS);
    pins.eq("finality update: step 1's storage gas", fin.reports[0].charge.gas_stor, FIN_OPEN_TX_STOR);
    pins.eq(
        "finality update: Miller's cycles (step 2, a smaller job state than an adopting one)",
        one(&fin.reports[1], "Miller").cycles,
        FIN_MILLER_CYCLES,
    );
    assert_eq!(fin.reports[1].charge.gas_exec, FIN_MILLER_CYCLES, "step 2's execution gas");
    assert_eq!(fin.reports[1].charge.gas_stor, MILLER_TX_STOR, "step 2's storage gas");
    pins.eq("finality update: Finish's cycles (step 3)", one(&fin.reports[2], "Finish").cycles, FIN_FINISH_CYCLES);
    pins.eq("finality update: Apply's cycles", one(&fin.reports[2], "Apply").cycles, FIN_APPLY_CYCLES);
    pins.eq("finality update: Record's cycles", one(&fin.reports[2], "Record").cycles, RECORD_CYCLES);
    pins.eq("finality update: step 3's execution gas", fin.reports[2].charge.gas_exec, FIN_FINISH_TX_GAS);
    pins.eq("finality update: step 3's storage gas", fin.reports[2].charge.gas_stor, FIN_FINISH_TX_STOR);
    let gas: u64 = fin.reports.iter().map(|r| r.charge.gas_exec).sum();
    let stor: u64 = fin.reports.iter().map(|r| r.charge.gas_stor).sum();
    pins.eq("finality update: total execution gas", gas, FIN_TOTAL_GAS);
    pins.eq("finality update: total storage gas", stor, FIN_TOTAL_STOR);
    pins.note(
        "derived: §P.3's per-hour row, one an hour",
        &format!("{} execution gas a day", grouped(24 * FIN_TOTAL_GAS)),
    );

    // #8: the ratio `docs/performance.md` §P.8 states on this shape ("7.0M of an update's 9.2M
    // public-path cycles"): the cycles a native pairing precompile would remove.
    let pairing = one(&fin.reports[1], "Miller").cycles + one(&fin.reports[2], "Finish").cycles;
    pins.eq("finality update: the pairing steps (Miller + Finish)", pairing, FIN_PAIRING_CYCLES);
    pins.note(
        "the pairing share of its total execution gas",
        &format!(
            "{:.1}% of {} — the share a native precompile could remove at most",
            pairing as f64 / FIN_TOTAL_GAS as f64 * 100.0,
            grouped(FIN_TOTAL_GAS)
        ),
    );

    // #7: the job account of a finality job — smaller, since it holds no adoption state — and its
    // record is the same 365-byte account.
    assert_eq!(one(&fin.reports[0], "JobOpen").post_state_bytes, Some(FIN_JOB_STATE[0]));
    assert_eq!(one(&fin.reports[1], "Miller").post_state_bytes, Some(FIN_JOB_STATE[1]));
    pins.pin("a finality job's account after JobOpen (bytes)", FIN_JOB_STATE[0] as u64);
    pins.pin("a finality job's account after Miller (bytes)", FIN_JOB_STATE[1] as u64);
    assert_eq!(one(&fin.reports[2], "Record").post_state_bytes, Some(RECORD_STATE_BYTES));
    assert!(
        world.chain.actor_state(job_actor(world.inst.program, &world.inst.id, &fin.job_id)).is_empty(),
        "Finish clears the finality job's account"
    );

    // The other way a committee handover runs — adoption with no new header (five transactions):
    // the flat steps cost the adopting updates' numbers again, in another world.
    let nc = &world.runs[3];
    assert_eq!(nc.reports.len(), 5, "adoption runs in five transactions");
    pins.eq("next-committee update: Open's cycles (step 1)", one(&nc.reports[0], "Open").cycles, NC_OPEN_CYCLES);
    pins.eq("next-committee update: step 1's execution gas", nc.reports[0].charge.gas_exec, NC_OPEN_TX_GAS);
    pins.eq("next-committee update: step 1's storage gas", nc.reports[0].charge.gas_stor, NC_OPEN_TX_STOR);
    assert_eq!(one(&nc.reports[0], "JobOpen").cycles, JOB_OPEN_CYCLES, "JobOpen's cycles, flat here too");
    assert_eq!(one(&nc.reports[1], "Miller").cycles, MILLER_CYCLES, "Miller's cycles, flat here too");
    assert_eq!(one(&nc.reports[2], "Adopt").cycles, ADOPT1_CYCLES, "adopt 1/2's cycles, flat here too");
    assert_eq!(one(&nc.reports[3], "Adopt").cycles, ADOPT2_CYCLES, "adopt 2/2's cycles, flat here too");
    assert_eq!(one(&nc.reports[4], "Finish").cycles, FINISH_CYCLES, "Finish's cycles, flat here too");
    assert_eq!(one(&nc.reports[4], "Apply").cycles, APPLY_NO_RECORD, "no new header, so the lighter Apply");
    let gas: u64 = nc.reports.iter().map(|r| r.charge.gas_exec).sum();
    let stor: u64 = nc.reports.iter().map(|r| r.charge.gas_stor).sum();
    pins.eq("next-committee update: total execution gas", gas, NC_TOTAL_GAS);
    pins.eq("next-committee update: total storage gas", stor, NC_TOTAL_STOR);

    pins.print(
        "Pinned: the finality update and the next-committee handover (scenarios::finality_path())\n\
         record: lez/results/v0.4.0-rc3/rc1-rerun/harness-tests-dev-mode.log,\n\
         `a_finality_update_is_three_transactions`, and stage1-finality.md's post-state column",
    );
}

#[test]
fn an_update_verified_privately_costs_what_it_is_charged() {
    // The private path needs dev mode (the fake receipts LEZ's circuit verifies); CI runs with
    // RISC0_DEV_MODE=1, and the real proof is the `prove_private` binary's.
    assert_eq!(
        std::env::var("RISC0_DEV_MODE").as_deref(),
        Ok("1"),
        "run with RISC0_DEV_MODE=1 (fake receipts; the real proof is the prove_private binary's)"
    );
    let mut w = World::new(2);
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    let sender = w.chain.add_private_account(0x31, 1_000_000_000_000);
    let time = w.inst.config.slot_start_ms(w.updates[2].signature_slot) + 12_000;
    let spec = w.spec.clone();
    let ctx = Context { spec: &spec, genesis_validators_root: root(GENESIS_VALIDATORS_ROOT), bls: &scenarios::BLS };
    let mut pins = Pins::new();

    // #3, the proven path: a finality update verified in `VerifyAll` inside one private
    // transaction, applied to the public instance. The charge is LEZ's flat receipt gas plus the
    // public part's cycles.
    let update = as_finality_update(&w.updates[1]);
    let message = verify_all(&w, &update);
    let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
    let run = w
        .chain
        .prove_and_settle_private(
            &sender,
            Actor::new(sender.id(), ferrata_account()),
            message,
            vec![w.inst.actor(), header],
            &catalog(),
            100_000_000,
            time,
        )
        .expect("settles");
    assert!(run.charge.revert.is_none(), "{:?}", run.charge.revert);
    assert_eq!(run.charge.gas_stor, market::PRIVATE_GAS_STOR, "the private storage gas is fee_core's flat");
    assert_eq!(run.charge.gas_stor, PRIVATE_GAS_STOR);
    assert_eq!(
        run.charge.gas_exec,
        market::PRIVATE_VERIFY_GAS + run.charge.cycles,
        "the flat receipt gas plus the public part"
    );
    pins.eq("a finality update, proven: PRIVATE_VERIFY_GAS (flat)", market::PRIVATE_VERIFY_GAS, PRIVATE_VERIFY_GAS);
    pins.eq("a finality update, proven: the public part's cycles", run.charge.cycles, FINALITY_PRIVATE_CYCLES);
    pins.eq("a finality update, proven: execution gas", run.charge.gas_exec, FINALITY_PRIVATE_GAS);
    pins.pin("a private transaction's storage gas (flat)", PRIVATE_GAS_STOR);
    w.native.process_update(&update, update.signature_slot + 1, &ctx).unwrap();
    w.check_store();
    w.check_record(update.finalized_header.beacon.slot);

    // The handoff: the same update as served (it adopts the next committee, 512 keys in the same
    // private call). `docs/performance.md` §P.3's "once per period (handoff, no new header)" row
    // derives from this per-update cost (its 448,530 is the earlier image's).
    let sender2 = w.chain.add_private_account(0x41, 1_000_000_000_000);
    let update = w.updates[1].clone();
    let message = verify_all(&w, &update);
    let run = w
        .chain
        .prove_and_settle_private(
            &sender2,
            Actor::new(sender2.id(), ferrata_account()),
            message,
            vec![w.inst.actor()],
            &catalog(),
            100_000_000,
            time,
        )
        .expect("settles");
    assert!(run.charge.revert.is_none(), "{:?}", run.charge.revert);
    assert_eq!(run.charge.gas_exec, market::PRIVATE_VERIFY_GAS + run.charge.cycles);
    pins.eq("a handoff, proven: the public part's cycles", run.charge.cycles, HANDOFF_PRIVATE_CYCLES);
    pins.eq("a handoff, proven: execution gas", run.charge.gas_exec, HANDOFF_PRIVATE_GAS);
    w.native.process_update(&update, update.signature_slot + 1, &ctx).unwrap();
    w.check_store();

    pins.print(
        "Pinned: the proven path's per-update charge (dev mode)\n\
         record: lez/results/v0.4.0-rc3/rc1-rerun/harness-tests-dev-mode.log,\n\
         `an_update_verified_privately_applies_to_the_public_instance`",
    );
}

#[test]
fn an_attestation_costs_the_same_whatever_the_chain_holds() {
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let weth = s.fx.weth_proof();
    let request = Request {
        accounts: vec![account_query(
            &weth,
            vec![],
            vec![(s.fx.weth_slot, vec![condition(Field::Value, Cmp::Ge, u256(1))])],
        )],
        ..s.request()
    };

    // #3's other half — the per-attestation verification cost, reported apart from the tracking
    // cost — and #9's baseline: an attestation while the chain holds two records (the
    // checkpoint's and the finalized block's).
    let (few, _) = s.accept("a WETH balance slot, attested while the chain holds two records", slot, &request);
    assert_eq!(few.transitions.len(), 1, "an attestation is one handler execution");
    let mut pins = Pins::new();
    pins.eq("an attestation of a WETH balance slot: cycles", few.charge.cycles, ATTEST_CYCLES);
    pins.eq("an attestation of a WETH balance slot: execution gas", few.charge.gas_exec, ATTEST_CYCLES);
    pins.eq("an attestation of a WETH balance slot: storage gas", few.charge.gas_stor, ATTEST_STOR);
    pins.eq(
        "the record it attests under: actor state (bytes)",
        few.transitions[0].pre_state_bytes as u64,
        RECORD_STATE_BYTES as u64,
    );
    // Every statement type of `stage4-attest.md` the same way, the absent account, the absent slot
    // and the four facts among them.
    let requests = acc::statement_requests(&s);
    let few_all = acc::attest_all(&mut s, &requests);
    for (c, (cycles, stor)) in few_all.iter().zip(STATEMENTS) {
        pins.eq(&format!("{}: cycles", c.action), c.cycles("Attest"), cycles);
        assert_eq!((c.gas_exec, c.gas_stor), (cycles, stor), "{}: execution and storage gas", c.action);
    }

    // #9 and #7: a thousand more records — placed as the program writes them
    // (`harness::vault::place_record`, the LEZ test utility the vault tests place a record with)
    // at slots after the real ones — and the same attestation again. A thousand is a tenth of
    // the 9,081 records a year of `docs/performance.md` §P.7's cadence projection.
    let record = s.record(slot).header().expect("the record of the finalized header");
    assert_eq!(record.beacon.slot, slot);
    for i in 0..MORE_RECORDS {
        let mut at = record.clone();
        at.beacon.slot = slot + 1 + i;
        vault::place_record(&mut s.w.chain, s.w.inst.id, s.fx.chain_id, &at);
    }
    let (many, _) = s.accept("the same attestation among a thousand more records", slot, &request);
    assert_eq!(
        (many.charge.cycles, many.charge.gas_exec, many.charge.gas_stor),
        (few.charge.cycles, few.charge.gas_exec, few.charge.gas_stor),
        "#9: the attestation's cost does not scale with the records held"
    );
    pins.eq("the same attestation, a thousand more records held: cycles", many.charge.cycles, ATTEST_CYCLES);
    let many_all = acc::attest_all(&mut s, &requests);
    acc::assert_flat(&few_all, &many_all, MORE_RECORDS as usize);
    pins.note("every statement type, a thousand more records held", "the same cycles, execution and storage gas");
    assert_eq!(many.transitions[0].pre_state_bytes, RECORD_STATE_BYTES, "still attesting under one record");
    let held = MORE_RECORDS + 2;
    pins.note(
        "records the chain holds for the second attestation",
        &format!("{} accounts, {} bytes of record state", grouped(held), grouped(held * RECORD_STATE_BYTES as u64)),
    );

    pins.print(
        "Pinned: an attestation beside accumulated records\n\
         record: lez/results/v0.4.0-rc3/rc1-rerun/harness-tests-dev-mode.log,\n\
         `every_kind_of_fact_is_attested_under_the_real_record`, the row\n\
         `storage slot: a WETH balance, value >= 1`; the eight statement types: lez/results/stage4-attest.md",
    );
}

#[test]
fn init_and_succeed_cost_the_same_whatever_the_chain_holds() {
    let uw = acc::UpdateWorld::new();
    let more = MORE_RECORDS as usize;
    let mut pins = Pins::new();

    // `Init` and its adoption on a new chain, and on one that already holds a thousand records of
    // another instance and a thousand other instances.
    let few = acc::init_on(&mut uw.chain_for_init(0));
    let many = acc::init_on(&mut uw.chain_for_init(more));
    acc::assert_flat(&few, &many, more);
    for (c, want) in few.iter().zip(&INIT_TXS) {
        pin_tx(&mut pins, c, want);
    }

    // `Succeed` and the `Spawn` it sends, at the instance as update_0 leaves it, and with a
    // thousand more of its records and a thousand other instances on the chain.
    let few = acc::succeed(&mut uw.grown(0));
    let many = acc::succeed(&mut uw.grown(more));
    acc::assert_flat(std::slice::from_ref(&few), &[many], more);
    pin_tx(&mut pins, &few, &SUCCEED_TX);
    pins.note("added for the second run of each", "a thousand records and a thousand instances");

    pins.print(
        "Pinned: Init and Succeed beside accumulated accounts\n\
         record: lez/results/perf-state-actions.md; Init also lez/results/v0.4.0-rc3/stage1-replay.md rows 1-3",
    );
}

#[test]
fn an_update_verified_in_one_call_costs_the_same_whatever_the_chain_holds() {
    // The private path needs dev mode, as `an_update_verified_privately_costs_what_it_is_charged`.
    assert_eq!(
        std::env::var("RISC0_DEV_MODE").as_deref(),
        Ok("1"),
        "run with RISC0_DEV_MODE=1 (fake receipts; the real proof is the prove_private binary's)"
    );
    let uw = acc::UpdateWorld::new();
    let more = MORE_RECORDS as usize;
    let mut pins = Pins::new();

    // `VerifyAll` in one public transaction, then the proven path, each on the chain update_0
    // leaves and with a thousand more records of the instance and a thousand other instances.
    let few = acc::verify_all_public(&mut uw.grown(0));
    let many = acc::verify_all_public(&mut uw.grown(more));
    acc::assert_flat(std::slice::from_ref(&few), &[many], more);
    pin_tx(&mut pins, &few, &VERIFY_ALL_TX);

    // The proven path also with a thousand more commitments in LEZ's commitment set: the fee
    // payer's membership path is longer, the charge the same.
    let few = acc::verify_all_private(&mut uw.grown(0));
    let mut w = uw.grown(more);
    acc::add_commitments(&mut w.chain, more);
    let many = acc::verify_all_private(&mut w);
    acc::assert_flat(std::slice::from_ref(&few.charged), &[many.charged], more);
    pins.eq("the proven path: the fee payer's membership path (levels)", few.path_levels as u64, PATH_LEVELS.0);
    pins.eq("the same, a thousand more commitments", many.path_levels as u64, PATH_LEVELS.1);
    let few = few.charged;
    pin_tx(&mut pins, &few, &PRIVATE_VERIFY_ALL_TX);
    assert_eq!(
        few.gas_exec,
        market::PRIVATE_VERIFY_GAS + few.cycles("public part"),
        "the flat receipt gas plus the public part"
    );
    assert_eq!(few.gas_stor, market::PRIVATE_GAS_STOR, "the private storage gas is fee_core's flat");
    pins.note(
        "added for the second run of each",
        "a thousand records and a thousand instances; for the private one, a thousand commitments too",
    );

    pins.print(
        "Pinned: an update verified in one call beside accumulated accounts (dev mode)\n\
         record: lez/results/perf-state-actions.md; the public one also lez/results/v0.4.0-rc3/stage1-one-transaction.md",
    );
}

#[test]
fn the_reference_consumer_costs_the_same_whatever_the_chain_holds() {
    // The private read needs dev mode, as `an_update_verified_privately_costs_what_it_is_charged`.
    assert_eq!(
        std::env::var("RISC0_DEV_MODE").as_deref(),
        Ok("1"),
        "run with RISC0_DEV_MODE=1 (fake receipts; the real proofs are the prove_* binaries')"
    );
    let cw = acc::ConsumerWorld::new();
    let more = MORE_RECORDS as usize;
    let mut pins = Pins::new();

    // The module's `Read` and `ReadPadded`, from a public reader: empty, then holding the record.
    let few = acc::read_record(&mut cw.grown(0));
    let many = acc::read_record(&mut cw.grown(more));
    acc::assert_flat(&few, &many, more);
    for (c, want) in few.iter().zip(&READS) {
        pin_tx(&mut pins, c, want);
    }

    // `CallAttest` three times from one reader: empty, then holding the previous reply, twice.
    let few = acc::call_attest(&mut cw.grown(0));
    let many = acc::call_attest(&mut cw.grown(more));
    acc::assert_flat(&few, &many, more);
    pin_tx(&mut pins, &few[0], &CALL_ATTEST_EMPTY);
    pin_tx(&mut pins, &few[1], &CALL_ATTEST_HOLDING);
    assert_eq!(
        (&few[2].handlers, few[2].gas_exec, few[2].gas_stor),
        (&few[1].handlers, few[1].gas_exec, few[1].gas_stor),
        "the reader holds one reply, whatever the number of calls"
    );

    // The claim crediting its sender for the first time, then crediting a sender whose balance
    // holds one credit, or a thousand and one.
    let few = acc::claim(&mut cw.grown(0), "Claim, the sender's first credit", 0);
    let many = acc::claim(&mut cw.grown(more), "Claim, the sender's first credit", 0);
    acc::assert_flat(std::slice::from_ref(&few), &[many], more);
    pin_tx(&mut pins, &few, &CLAIM_FIRST);
    let mut s = cw.grown(0);
    acc::place_balance(&mut s, 1);
    let once = acc::claim(&mut s, "Claim, the sender credited before", 1);
    let mut s = cw.grown(more);
    acc::place_balance(&mut s, MORE_RECORDS + 1);
    let many = acc::claim(&mut s, "Claim, the sender credited before", MORE_RECORDS + 1);
    acc::assert_flat(std::slice::from_ref(&once), &[many], more);
    pin_tx(&mut pins, &once, &CLAIM_CREDITED);
    // The private read, by a new private account, with a thousand more commitments too.
    let few = acc::attest_privately(&mut cw.grown(0));
    let mut s = cw.grown(more);
    acc::add_commitments(&mut s.w.chain, more);
    let many = acc::attest_privately(&mut s);
    acc::assert_flat(std::slice::from_ref(&few.charged), &[many.charged], more);
    assert_eq!((few.path_levels as u64, many.path_levels as u64), PATH_LEVELS, "the fee payer's membership path");
    pin_tx(&mut pins, &few.charged, &ATTEST_PRIVATELY_TX);
    pins.note(
        "added for the second run of each",
        "a thousand records, spent claims, balances and readers (and commitments, for the private read); \
         the sender credited 1,001 times",
    );

    pins.print(
        "Pinned: the reference consumer beside accumulated accounts\n\
         record: lez/results/perf-state-actions.md; the first read: lez/results/v0.4.0-rc3/shell-costs.md row 1;\n\
         the first call and the first claim also\n\
         lez/results/stage4-attest-consumer.md rows 1 and 3",
    );
}

#[test]
fn the_leaf_statement_costs_the_same_whatever_the_chain_holds() {
    let lw = acc::LeafWorld::new();
    let more = MORE_RECORDS as usize;
    let mut pins = Pins::new();
    let few = acc::attest_leaves(&lw, &mut lw.grown(0));
    let many = acc::attest_leaves(&lw, &mut lw.grown(more));
    acc::assert_flat(&few, &many, more);
    for (c, (cycles, stor)) in few.iter().zip(LEAVES) {
        pins.eq(&format!("{}: cycles", c.action), c.cycles("Attest"), cycles);
        assert_eq!((c.gas_exec, c.gas_stor), (cycles, stor), "{}: execution and storage gas", c.action);
    }
    pins.note("added for the second run", "a thousand records of the synthetic instance");
    pins.print(
        "Pinned: the accumulator-leaf statement beside accumulated records\n\
         record: lez/results/perf-state-actions.md; lez/results/stage5-vault-synthetic.md rows 1-5",
    );
}

#[test]
fn the_vault_pattern_costs_the_same_whatever_the_chain_holds() {
    assert_eq!(
        std::env::var("RISC0_DEV_MODE").as_deref(),
        Ok("1"),
        "run with RISC0_DEV_MODE=1 (fake receipts; the real proofs are the prove_* binaries')"
    );
    // The record placed rather than replayed (`RealVault::placed`: the replay of 32 Sepolia periods
    // is a minute); the pins are the replayed world's figures (`perf-state-actions.md`). That the
    // placed record is the one the instance writes is asserted in `RealVault::load`, which CI runs
    // through `vault`, `vault_consumer`, `history_hops` and `redteam_attest`.
    let vw = acc::VaultWorld::placed();
    let more = MORE_RECORDS as usize;
    let mut pins = Pins::new();

    // The public claim crediting its destination for the first time, then crediting a holder
    // already credited once, or a thousand and one times.
    let first = "ClaimDeposit, the destination's first credit";
    let few = acc::claim_deposit(&vw, &mut vw.grown(0), first, 0);
    let many = acc::claim_deposit(&vw, &mut vw.grown(more), first, 0);
    acc::assert_flat(std::slice::from_ref(&few), &[many], more);
    pin_tx(&mut pins, &few, &CLAIM_DEPOSIT_FIRST);
    let again = "ClaimDeposit, the holder credited before";
    let mut w = vw.grown(0);
    acc::place_holder(&vw, &mut w, 1);
    let once = acc::claim_deposit(&vw, &mut w, again, 1);
    let mut w = vw.grown(more);
    acc::place_holder(&vw, &mut w, MORE_RECORDS + 1);
    let many = acc::claim_deposit(&vw, &mut w, again, MORE_RECORDS + 1);
    acc::assert_flat(std::slice::from_ref(&once), &[many], more);
    pin_tx(&mut pins, &once, &CLAIM_DEPOSIT_CREDITED);

    // The private claim (its public part `Read`, `Spend`, `CreditHolder`), with a thousand more
    // commitments too.
    let few = acc::claim_deposit_privately(&vw, &mut vw.grown(0));
    let mut w = vw.grown(more);
    acc::add_commitments(&mut w.chain, more);
    let many = acc::claim_deposit_privately(&vw, &mut w);
    acc::assert_flat(std::slice::from_ref(&few.charged), &[many.charged], more);
    assert_eq!((few.path_levels as u64, many.path_levels as u64), PATH_LEVELS, "the fee payer's membership path");
    pin_tx(&mut pins, &few.charged, &CLAIM_DEPOSIT_PRIVATELY_TX);
    pins.note(
        "added for the second run of each",
        "a thousand records, spent claims and holders (and commitments, for the private claim); the holder credited \
         1,001 times",
    );

    // The one cost that grows with use: a private-kind claim's own proof against the pools its
    // claimant already holds (its private `holdings`), which LEZ's charge does not see.
    let at_0 = acc::private_kind_claim(&vw, 0);
    let at_100 = acc::private_kind_claim(&vw, 100);
    pins.eq("a private-kind claim, holding 0 pools: the root call's cycles", at_0.0, HOLDINGS_0.0);
    pins.eq("the same: the reply's cycles", at_0.1, HOLDINGS_0.1);
    pins.eq("holding 100 pools: the root call's cycles", at_100.0, HOLDINGS_100.0);
    pins.eq("the same: the reply's cycles", at_100.1, HOLDINGS_100.1);
    let per_pool = (at_100.0 + at_100.1 - at_0.0 - at_0.1) / 100;
    assert!((1_500..3_000).contains(&per_pool), "about 2,100 cycles a pool held, not {per_pool}");
    pins.pin("derived: the claimant's own proof, per pool held (cycles)", per_pool);

    pins.print(
        "Pinned: the reference consumer's vault pattern beside accumulated accounts (dev mode)\n\
         record: lez/results/perf-state-actions.md; the first public claim also\n\
         lez/results/stage5-consumer-public.md row 1, the private claim stage5-consumer-private.md",
    );
}
