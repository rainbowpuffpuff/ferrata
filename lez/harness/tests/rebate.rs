//! RFP-021 soft requirement 5, "Operator incentive": a mechanism that rewards whoever advances the consensus state,
//! without making operating exclusive.
//!
//! `ferrata_rebate` (guests/rebate): `pool(terms)` holds a native balance anyone funds, its address committing to its
//! terms (instance, rebate, least advance paid for); `Land { job_id, payee }` sends the update's last step (`Finish`)
//! and pays `payee` the rebate if the instance's finalized slot advanced within that transaction, by at least the
//! least advance since the last one paid for. The attestation program is unchanged and still takes updates from
//! anyone directly. On the mainnet instance of periods 1899–1903 (`scenarios::World`, five real updates, each
//! advancing the finalized slot by one period, 8,192 slots), every job's `Open` and `Miller` steps are sent as usual;
//! only the last step differs. Writes `results/rebate.md` and `results/rebate-terms.md`.
//!
//! `RISC0_DEV_MODE=1 cargo test -p harness --features rebate --locked --test rebate -- --nocapture`
#![cfg(feature = "rebate")]

use std::{borrow::Cow, fs};

use ferrata_lez_rebate_wire::{code as rb, pool_actor, pool_seed, Pool, RebateMessage, RebateState, Terms};
use harness::{
    exit_name, render, render_transitions, results_dir, scenarios::World, settle::LeeTransaction, JobRun, TxReport,
};
use lee::{program::Program, AccountId, Actor, PublicAccountEvidence, V03State};
use lee_core::{account::Account, native_token::Message as NativeMessage};

const REBATE: u128 = 1_000_000;
/// One sync-committee period of slots: the least advance the main test's pool pays for.
const PERIOD: u64 = 8_192;

fn rebate() -> AccountId {
    AccountId::from_builtin_program(methods_rebate::FERRATA_REBATE_ID)
}

/// The operators' own accounts (each holding 1 unit, as any account that has ever been used does).
const OP1: AccountId = AccountId::new([0x01; 32]);
const OP2: AccountId = AccountId::new([0x02; 32]);
const OP3: AccountId = AccountId::new([0x03; 32]);

fn world() -> World {
    let mut w = World::new(2);
    let state = std::mem::replace(&mut w.chain.state, V03State::new());
    let state =
        state.with_public_accounts([(OP1, Account::funded(1)), (OP2, Account::funded(1)), (OP3, Account::funded(1))]);
    w.chain.state = state.with_genesis_programs([(
        Program::new_unchecked(methods_rebate::FERRATA_REBATE_ID, Cow::Borrowed(methods_rebate::FERRATA_REBATE_ELF)),
        true,
    )]);
    w
}

/// The terms of a pool for the world's instance.
fn terms(w: &World, rebate_amount: u128, min_gap: u64) -> Terms {
    Terms { ferrata: w.inst.program, instance_id: w.inst.id, rebate: rebate_amount, min_gap }
}

/// The main test's pool: one rebate per period of finalized progress at most.
fn main_terms(w: &World) -> Terms {
    terms(w, REBATE, PERIOD)
}

fn pool(t: &Terms) -> Actor {
    pool_actor(rebate(), t)
}

fn balance(w: &World, account: AccountId) -> u128 {
    w.chain.state.get_account_by_id(account).data.native_balance().expect("a canonical balance")
}

fn pool_state(w: &World, t: &Terms) -> Option<RebateState> {
    let state = w.chain.actor_state(pool(t));
    (!state.is_empty()).then(|| borsh::from_slice(&state).expect("a rebate state"))
}

fn ready(w: &World, t: &Terms) -> Pool {
    match pool_state(w, t) {
        Some(RebateState::Ready(p)) => p,
        other => panic!("the pool is not ready: {other:?}"),
    }
}

/// The block time of update `i`'s job: the transactions around it settle at the same time.
fn time_at(w: &World, i: usize) -> u64 {
    w.inst.config.slot_start_ms(w.updates[i].signature_slot) + 12_000
}

/// `Configure { terms }` sent to `pool(at)`, with the evidence of that actor (so that LEZ runs the program on it).
fn configure(w: &mut World, label: &str, at: &Terms, terms: &Terms) -> TxReport {
    let message = RebateMessage::Configure { terms: *terms };
    let evidence = vec![PublicAccountEvidence::Pda { program: rebate(), seed: pool_seed(at) }];
    let at = pool(at);
    let time = time_at(w, 0);
    w.chain.submit(label, at, borsh::to_vec(&message).unwrap(), vec![at, w.inst.actor()], evidence, time)
}

/// Anyone funds the pool: an ordinary native transfer, here from the chain's fee payer.
fn fund(w: &mut World, t: &Terms, amount: u128, i: usize) -> TxReport {
    let from = Actor::native_balance(w.chain.payer);
    let to = pool(t).account_id;
    let message = NativeMessage::Transfer { to, amount };
    let time = time_at(w, i);
    w.chain.submit(
        "fund the pool (a native transfer)",
        from,
        borsh::to_vec(&message).unwrap(),
        vec![from, Actor::native_balance(to)],
        vec![],
        time,
    )
}

/// Update `i` with its last step sent through the pool's `Land`, naming `payee`. A refused `Land` is replayed on the
/// state before it, to print LEZ's own error (the report keeps only its message).
fn land(w: &mut World, t: &Terms, i: usize, payee: AccountId) -> JobRun {
    let update = w.updates[i].clone();
    let pool = pool(t);
    w.apply_with(&update, &format!("update_{i} via the pool"), |chain, label, job_id, _job, mut actors, time| {
        actors.extend([pool, Actor::native_balance(pool.account_id), Actor::native_balance(payee)]);
        let message = RebateMessage::Land { job_id, payee };
        let before = chain.state.clone();
        let block = chain.block_id;
        let label = label.replace("Finish (step 3)", "Land (Finish through the pool)");
        let r = chain.submit(&label, pool, borsh::to_vec(&message).unwrap(), actors, vec![], time);
        if !r.ok() {
            if let Ok(LeeTransaction::Public(tx)) = borsh::from_slice::<LeeTransaction>(&chain.last_tx) {
                let mut state = before;
                println!("{label}: LEZ says {:?}", state.transition_from_public_transaction(&tx, block, time));
            }
        }
        r
    })
}

fn refused(r: &TxReport, code: u32) {
    assert!(!r.ok(), "{} was applied", r.label);
    assert_eq!(r.exit_code, Some(code), "{}: {:?}", r.label, r.charge.revert);
    println!("refused as expected: {} → exit {code} ({})", r.label, exit_name(code));
}

#[test]
fn whoever_lands_an_advance_through_the_pool_is_paid_once() {
    let mut w = world();
    let mut reports = Vec::new();
    let (op1, op2) = (OP1, OP2);

    let t = main_terms(&w);
    let r =
        configure(&mut w, "Configure the pool (rebate 1,000,000, one period of finalized progress at least)", &t, &t);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    reports.push(r);
    let start = w.inst.store(&w.chain).finalized.slot;
    assert_eq!(ready(&w, &t).paid_slot, start, "progress made before the pool existed is not paid for");
    let r = fund(&mut w, &t, 3 * REBATE, 0);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    reports.push(r);
    let pool_account = pool(&t).account_id;
    assert_eq!(balance(&w, pool_account), 3 * REBATE);

    // Update 0 through the pool: it adopts the next committee and finalizes nothing new, so nothing is paid.
    let run = land(&mut w, &t, 0, op1);
    assert!(run.reports.iter().all(TxReport::ok));
    assert!(!run.outcome.expect("applied").advanced);
    assert_eq!(w.inst.store(&w.chain).finalized.slot, start);
    assert_eq!(((balance(&w, op1) - 1), balance(&w, pool_account), ready(&w, &t).paid_count), (0, 3 * REBATE, 0));
    reports.extend(run.reports);

    // Update 1 through the pool by operator 1: it advances the finalized slot; operator 1 is paid once.
    let run = land(&mut w, &t, 1, op1);
    assert!(run.reports.iter().all(TxReport::ok), "{:?}", run.reports.last().map(|r| (&r.label, r.exit_code)));
    let after1 = w.inst.store(&w.chain).finalized.slot;
    assert!(after1 > start, "update 1 advanced the instance");
    assert_eq!(((balance(&w, op1) - 1), balance(&w, pool_account)), (REBATE, 2 * REBATE));
    let p = ready(&w, &t);
    assert_eq!((p.paid_slot, p.paid_count, p.paid_total), (after1, 1, REBATE));
    println!("update_1 via the pool: finalized slot {start} → {after1}; operator 1 paid {REBATE}");
    reports.extend(run.reports);

    // Update 2 sent directly (`Finish` to the job): permissionless as before, nobody paid.
    let update = w.updates[2].clone();
    let run = w.apply(&update, "update_2 direct");
    assert!(run.reports.iter().all(TxReport::ok));
    let after2 = w.inst.store(&w.chain).finalized.slot;
    assert!(after2 > after1);
    assert_eq!(((balance(&w, op1) - 1), (balance(&w, op2) - 1), balance(&w, pool_account)), (REBATE, 0, 2 * REBATE));
    assert_eq!(ready(&w, &t).paid_count, 1, "a direct update earns nobody a rebate");
    reports.extend(run.reports);

    // Update 3 through the pool by operator 2: paid for its own advance only, not for update 2's.
    let run = land(&mut w, &t, 3, op2);
    assert!(run.reports.iter().all(TxReport::ok), "{:?}", run.reports.last().map(|r| (&r.label, r.exit_code)));
    let after3 = w.inst.store(&w.chain).finalized.slot;
    assert!(after3 > after2);
    assert_eq!(((balance(&w, op2) - 1), balance(&w, pool_account)), (REBATE, REBATE));
    let p = ready(&w, &t);
    assert_eq!((p.paid_slot, p.paid_count, p.paid_total), (after3, 2, 2 * REBATE));
    reports.extend(run.reports);

    let land_gas: Vec<u64> = reports.iter().filter(|r| r.label.contains("Land")).map(|r| r.charge.gas_exec).collect();
    let finish_gas: Vec<u64> = reports
        .iter()
        .filter(|r| r.label.contains("direct") && r.label.contains("Finish"))
        .map(|r| r.charge.gas_exec)
        .collect();
    println!("execution gas: Land {land_gas:?}; direct Finish {finish_gas:?}");
    let table = format!(
        "# ferrata_rebate: whoever lands an advance through the pool is paid (dev mode, in-process harness)\n\n\
         Written by `RISC0_DEV_MODE=1 cargo test -p harness --features rebate --locked --test rebate -- --nocapture`. \
         RFP-021 soft requirement 5. The mainnet instance of periods 1899–1903; rebate {REBATE} native units for an \
         advance of at least {PERIOD} finalized slots since the last one paid for, pool funded with {} by a native \
         transfer. Update 0 through the pool (no advance: nobody paid), update 1 through \
         the pool (operator 1 paid), update 2 directly (nobody paid), update 3 through the pool (operator 2 paid). \
         Finalized slots {start} → {start} → {after1} → {after2} → {after3}. \
         Execution gas of `Land` {land_gas:?}, of a direct `Finish` {finish_gas:?}.\n\n## Transactions\n\n{}\n\
         ## Handlers\n\n{}",
        3 * REBATE,
        render(&reports),
        render_transitions(&reports),
    );
    fs::write(results_dir().join("rebate.md"), table).unwrap();
}

#[test]
fn an_empty_pool_blocks_nothing_and_refusals_pay_nothing() {
    let mut w = world();
    let op = OP3;
    let t = main_terms(&w);

    // `Land` at a pool that does not exist: the rebate program refuses, the update does not land this way.
    let update = w.updates[0].clone();
    let p = pool(&t);
    let seed = pool_seed(&t);
    // Through the instance directly (World::apply would stop on the refusal it is not told to expect).
    let layout = w.inst.layout_for(&w.inst.store(&w.chain), &update);
    let full = w.native.core().clone();
    let label = "update_0 via a pool never configured";
    let run = w.inst.update_in_with(
        &mut w.chain,
        &update,
        layout,
        &full,
        label,
        |chain, label, job_id, _job, mut actors, time| {
            actors.extend([p, Actor::native_balance(p.account_id), Actor::native_balance(op)]);
            let message = RebateMessage::Land { job_id, payee: op };
            let evidence = vec![PublicAccountEvidence::Pda { program: rebate(), seed }];
            chain.submit(label, p, borsh::to_vec(&message).unwrap(), actors, evidence, time)
        },
    );
    refused(run.reports.last().expect("the last step"), u32::from(rb::NOT_CONFIGURED));

    // A configured pool with nothing in it: the update lands, nothing is paid.
    let mut w = world();
    let r = configure(&mut w, "Configure an unfunded pool", &t, &t);
    assert!(r.ok());
    let r = configure(&mut w, "Configure it again", &t, &t);
    refused(&r, u32::from(rb::ALREADY_CONFIGURED));
    // `Configure` at another instance's pool actor, with that actor's own evidence: the program refuses it.
    let other = Terms { instance_id: [9; 32], ..t };
    let r = configure(&mut w, "Configure at another instance's pool actor", &other, &t);
    refused(&r, u32::from(rb::WRONG_RECEIVER));
    assert!(pool_state(&w, &other).is_none());
    let run = land(&mut w, &t, 0, op);
    assert!(run.reports.iter().all(TxReport::ok), "an empty pool blocked the update");
    let run = land(&mut w, &t, 1, op);
    assert!(run.reports.iter().all(TxReport::ok), "an empty pool blocked the update");
    assert!(run.outcome.expect("applied").advanced, "update 1 advanced the instance");
    assert_eq!(((balance(&w, op) - 1), ready(&w, &t).paid_count), (0, 0), "an empty pool pays nothing");

    // Half a rebate in the pool: the next paid advance through it is paid what the pool holds.
    let r = fund(&mut w, &t, REBATE / 2, 2);
    assert!(r.ok());
    let run = land(&mut w, &t, 2, op);
    assert!(run.reports.iter().all(TxReport::ok), "{:?}", run.reports.last().map(|r| (&r.label, r.exit_code)));
    assert_eq!((balance(&w, op) - 1), REBATE / 2);
    assert_eq!(balance(&w, pool(&t).account_id), 0);

    // A `Land` naming a job that does not exist, with the job actor's evidence so that LEZ runs `Finish` on it: the
    // attestation program refuses (no job), the whole transaction ends, nothing is paid.
    let before = pool_state(&w, &t);
    let p = pool(&t);
    let message = RebateMessage::Land { job_id: [0xee; 32], payee: op };
    let job = ferrata_lez_wire::job_actor(w.inst.program, &w.inst.id, &[0xee; 32]);
    let evidence = vec![PublicAccountEvidence::Pda {
        program: w.inst.program,
        seed: ferrata_lez_wire::job_seed(&w.inst.id, &[0xee; 32]),
    }];
    let time = time_at(&w, 2);
    let actors = vec![p, Actor::native_balance(p.account_id), Actor::native_balance(op), w.inst.actor(), job];
    let r = w.chain.submit(
        "Land of a job that does not exist",
        p,
        borsh::to_vec(&message).unwrap(),
        actors,
        evidence,
        time,
    );
    refused(&r, u32::from(ferrata_lez_wire::code::NO_JOB));
    assert_eq!(pool_state(&w, &t), before);
    assert_eq!((balance(&w, op) - 1), REBATE / 2);
}

/// The review's attacks on the pool's terms (review-rfp-I M-2, L-1): a squatter cannot set the terms of the pool a
/// funder pays, one advance pays one rebate, and progress split into small advances is paid at most once per
/// `min_gap`. Writes `results/rebate-terms.md`.
#[test]
fn a_pools_terms_cannot_be_squatted_and_split_progress_is_paid_once_per_gap() {
    let mut w = world();
    let mut reports = Vec::new();
    // The funder's terms: 1,000,000 per two periods of finalized progress at least. The squatter's: everything the
    // pool holds, for any advance at all.
    let funder = terms(&w, REBATE, 2 * PERIOD);
    let squatter = terms(&w, u128::MAX, 0);

    // The squat: the squatter sends its terms to the address the funder will pay. Refused: that address commits to
    // the funder's terms.
    let r = configure(
        &mut w,
        "squat: Configure the funder's pool address with rebate 2^128 - 1 and no gap",
        &funder,
        &squatter,
    );
    refused(&r, u32::from(rb::WRONG_RECEIVER));
    assert!(pool_state(&w, &funder).is_none());
    reports.push(r);
    // The squatter's terms make a pool of their own, at their own address, which nobody funds.
    let r = configure(&mut w, "Configure the squatter's own terms (its own address)", &squatter, &squatter);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    assert_ne!(pool(&squatter), pool(&funder));
    reports.push(r);
    // The funder configures its own terms (anyone could: the address allows no others) and funds them.
    let r = configure(&mut w, "Configure the funder's pool (rebate 1,000,000, two periods at least)", &funder, &funder);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    reports.push(r);
    let r = configure(&mut w, "Configure the funder's pool again", &funder, &funder);
    refused(&r, u32::from(rb::ALREADY_CONFIGURED));
    reports.push(r);
    let r = fund(&mut w, &funder, 3 * REBATE, 0);
    assert!(r.ok());
    reports.push(r);
    let pool_account = pool(&funder).account_id;
    let start = w.inst.store(&w.chain).finalized.slot;
    assert_eq!(ready(&w, &funder).paid_slot, start);

    // Progress split into one-period advances: update 0 (no advance) and update 1 (one period) earn nothing; update
    // 2 completes two periods since the pool's start and earns one rebate, not the pool; update 3 (one more period)
    // earns nothing.
    let (op1, op2) = (OP1, OP2);
    let run = land(&mut w, &funder, 0, op1);
    assert!(run.reports.iter().all(TxReport::ok));
    reports.extend(run.reports);
    let run = land(&mut w, &funder, 1, op1);
    assert!(run.reports.iter().all(TxReport::ok));
    assert!(run.outcome.expect("applied").advanced);
    let after1 = w.inst.store(&w.chain).finalized.slot;
    assert_eq!(after1, start + PERIOD);
    assert_eq!(((balance(&w, op1) - 1), balance(&w, pool_account), ready(&w, &funder).paid_count), (0, 3 * REBATE, 0));
    reports.extend(run.reports);
    let run = land(&mut w, &funder, 2, op2);
    assert!(run.reports.iter().all(TxReport::ok));
    let after2 = w.inst.store(&w.chain).finalized.slot;
    assert_eq!(after2, start + 2 * PERIOD, "exactly the least advance paid for");
    assert_eq!(((balance(&w, op2) - 1), balance(&w, pool_account)), (REBATE, 2 * REBATE), "one rebate, not the pool");
    let p = ready(&w, &funder);
    assert_eq!((p.paid_slot, p.paid_count, p.paid_total), (after2, 1, REBATE));
    reports.extend(run.reports);
    let run = land(&mut w, &funder, 3, op1);
    assert!(run.reports.iter().all(TxReport::ok));
    let after3 = w.inst.store(&w.chain).finalized.slot;
    assert_eq!(after3, after2 + PERIOD);
    assert_eq!(((balance(&w, op1) - 1), balance(&w, pool_account), ready(&w, &funder).paid_count), (0, 2 * REBATE, 1));
    reports.extend(run.reports);
    // The bound: over `progress` slots of finalized progress the pool pays at most rebate × (1 + progress / min_gap).
    let progress = after3 - start;
    let paid = ready(&w, &funder).paid_total;
    assert!(paid <= REBATE * u128::from(1 + progress / (2 * PERIOD)), "{paid} over {progress} slots");
    println!("split progress: {progress} finalized slots in four landings, paid {paid} (one rebate)");

    let table = format!(
        "# ferrata_rebate: a pool's terms cannot be squatted; split progress is paid once per gap (dev mode)\n\n\
         Written by `RISC0_DEV_MODE=1 cargo test -p harness --features rebate --locked --test rebate -- --nocapture` \
         (`a_pools_terms_cannot_be_squatted_and_split_progress_is_paid_once_per_gap`). RFP-021 soft requirement 5; \
         review-rfp-I M-2 and L-1. The funder's terms: rebate {REBATE}, at least {} finalized slots since the last \
         advance paid for; the squatter's: rebate 2^128 - 1, no gap. Pool funded with {}. Finalized slots {start} → \
         {start} → {after1} → {after2} → {after3}; paid once, {paid}.\n\n## Transactions\n\n{}",
        2 * PERIOD,
        3 * REBATE,
        render(&reports),
    );
    fs::write(results_dir().join("rebate-terms.md"), table).unwrap();
}

/// The payment rule alone, at its boundaries (no chain): no advance, an advance one slot short of the gap, exactly
/// the gap, a pool holding less than the rebate, no gap at all, and a gap so large that it never pays.
#[test]
fn the_payment_rule_at_its_boundaries() {
    let pool = |min_gap: u64| Pool {
        terms: Terms { ferrata: AccountId::new([7; 32]), instance_id: [8; 32], rebate: REBATE, min_gap },
        paid_slot: 1_000,
        paid_count: 0,
        paid_total: 0,
    };
    let p = pool(100);
    assert_eq!(p.earned(1_050, 1_050, 10 * REBATE), 0, "no advance");
    assert_eq!(p.earned(1_050, 1_099, 10 * REBATE), 0, "one slot short of the gap");
    assert_eq!(p.earned(1_050, 1_100, 10 * REBATE), REBATE, "exactly the gap");
    assert_eq!(p.earned(1_050, 1_101, 10 * REBATE), REBATE, "one slot past the gap");
    assert_eq!(p.earned(1_050, 1_100, REBATE / 3), REBATE / 3, "what the pool holds, if less");
    assert_eq!(p.earned(1_050, 1_100, 0), 0, "an empty pool");
    assert_eq!(p.earned(1_200, 1_150, 10 * REBATE), 0, "the slot went back");
    assert_eq!(pool(0).earned(1_000, 1_001, 10 * REBATE), REBATE, "no gap: every advance");
    assert_eq!(
        pool(u64::MAX).earned(1_000, 1_000_000_000_000, 10 * REBATE),
        0,
        "a gap larger than any progress: never"
    );
}
