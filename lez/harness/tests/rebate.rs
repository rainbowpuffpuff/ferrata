//! RFP-021 soft requirement 5, "Operator incentive": a mechanism that rewards whoever advances the consensus state,
//! without making operating exclusive.
//!
//! `ferrata_rebate` (guests/rebate): `pool(ferrata, instance)` holds a native balance anyone funds; `Land { job_id,
//! payee }` sends the update's last step (`Finish`) and pays `payee` a fixed rebate if the instance's finalized slot
//! advanced within that transaction. The attestation program is unchanged and still takes updates from anyone
//! directly. On the mainnet instance of periods 1899–1903 (`scenarios::World`, five real updates), every job's `Open`
//! and `Miller` steps are sent as usual; only the last step differs. Writes `results/rebate.md`.
//!
//! `RISC0_DEV_MODE=1 cargo test -p harness --features rebate --locked --test rebate -- --nocapture`
#![cfg(feature = "rebate")]

use std::{borrow::Cow, fs};

use ferrata_lez_rebate_wire::{code as rb, pool_actor, pool_seed, Pool, RebateMessage, RebateState};
use harness::{
    exit_name, render, render_transitions, results_dir, scenarios::World, settle::LeeTransaction, JobRun, TxReport,
};
use lee::{program::Program, AccountId, Actor, PublicAccountEvidence, V03State};
use lee_core::{account::Account, native_token::Message as NativeMessage};

const REBATE: u128 = 1_000_000;

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

fn pool(w: &World) -> Actor {
    pool_actor(rebate(), &w.inst.program, &w.inst.id)
}

fn balance(w: &World, account: AccountId) -> u128 {
    w.chain.state.get_account_by_id(account).data.native_balance().expect("a canonical balance")
}

fn pool_state(w: &World) -> Option<RebateState> {
    let state = w.chain.actor_state(pool(w));
    (!state.is_empty()).then(|| borsh::from_slice(&state).expect("a rebate state"))
}

fn ready(w: &World) -> Pool {
    match pool_state(w) {
        Some(RebateState::Ready(p)) => p,
        other => panic!("the pool is not ready: {other:?}"),
    }
}

/// The block time of update `i`'s job: the transactions around it settle at the same time.
fn time_at(w: &World, i: usize) -> u64 {
    w.inst.config.slot_start_ms(w.updates[i].signature_slot) + 12_000
}

fn configure(w: &mut World, label: &str, at: Actor, rebate_amount: u128) -> TxReport {
    let message = RebateMessage::Configure { ferrata: w.inst.program, instance_id: w.inst.id, rebate: rebate_amount };
    let evidence = vec![PublicAccountEvidence::Pda { program: rebate(), seed: pool_seed(&w.inst.program, &w.inst.id) }];
    let time = time_at(w, 0);
    w.chain.submit(label, at, borsh::to_vec(&message).unwrap(), vec![at, w.inst.actor()], evidence, time)
}

/// Anyone funds the pool: an ordinary native transfer, here from the chain's fee payer.
fn fund(w: &mut World, amount: u128, i: usize) -> TxReport {
    let from = Actor::native_balance(w.chain.payer);
    let to = pool(w).account_id;
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
fn land(w: &mut World, i: usize, payee: AccountId) -> JobRun {
    let update = w.updates[i].clone();
    let pool = pool(w);
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

    let at = pool(&w);
    let r = configure(&mut w, "Configure the pool (rebate 1,000,000)", at, REBATE);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    reports.push(r);
    let start = w.inst.store(&w.chain).finalized.slot;
    assert_eq!(ready(&w).paid_slot, start, "progress made before the pool existed is not paid for");
    let r = fund(&mut w, 3 * REBATE, 0);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    reports.push(r);
    let pool_account = pool(&w).account_id;
    assert_eq!(balance(&w, pool_account), 3 * REBATE);

    // Update 0 through the pool: it adopts the next committee and finalizes nothing new, so nothing is paid.
    let run = land(&mut w, 0, op1);
    assert!(run.reports.iter().all(TxReport::ok));
    assert!(!run.outcome.expect("applied").advanced);
    assert_eq!(w.inst.store(&w.chain).finalized.slot, start);
    assert_eq!(((balance(&w, op1) - 1), balance(&w, pool_account), ready(&w).paid_count), (0, 3 * REBATE, 0));
    reports.extend(run.reports);

    // Update 1 through the pool by operator 1: it advances the finalized slot; operator 1 is paid once.
    let run = land(&mut w, 1, op1);
    assert!(run.reports.iter().all(TxReport::ok), "{:?}", run.reports.last().map(|r| (&r.label, r.exit_code)));
    let after1 = w.inst.store(&w.chain).finalized.slot;
    assert!(after1 > start, "update 1 advanced the instance");
    assert_eq!(((balance(&w, op1) - 1), balance(&w, pool_account)), (REBATE, 2 * REBATE));
    let p = ready(&w);
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
    assert_eq!(ready(&w).paid_count, 1, "a direct update earns nobody a rebate");
    reports.extend(run.reports);

    // Update 3 through the pool by operator 2: paid for its own advance only, not for update 2's.
    let run = land(&mut w, 3, op2);
    assert!(run.reports.iter().all(TxReport::ok), "{:?}", run.reports.last().map(|r| (&r.label, r.exit_code)));
    let after3 = w.inst.store(&w.chain).finalized.slot;
    assert!(after3 > after2);
    assert_eq!(((balance(&w, op2) - 1), balance(&w, pool_account)), (REBATE, REBATE));
    let p = ready(&w);
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
         RFP-021 soft requirement 5. The mainnet instance of periods 1899–1903; rebate {REBATE} native units, pool \
         funded with {} by a native transfer. Update 0 through the pool (no advance: nobody paid), update 1 through \
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

    // `Land` at a pool that does not exist: the rebate program refuses, the update does not land this way.
    let update = w.updates[0].clone();
    let p = pool(&w);
    let seed = pool_seed(&w.inst.program, &w.inst.id);
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
    let at = pool(&w);
    let r = configure(&mut w, "Configure an unfunded pool", at, REBATE);
    assert!(r.ok());
    let at = pool(&w);
    let r = configure(&mut w, "Configure it again", at, REBATE);
    refused(&r, u32::from(rb::ALREADY_CONFIGURED));
    let wrong = pool_actor(rebate(), &w.inst.program, &[9; 32]);
    let r = configure(&mut w, "Configure at another instance's pool actor", wrong, REBATE);
    assert!(!r.ok(), "Configure at a wrong actor was applied");
    println!("refused as expected: {} → exit {:?}", r.label, r.exit_code);
    let run = land(&mut w, 0, op);
    assert!(run.reports.iter().all(TxReport::ok), "an empty pool blocked the update");
    let run = land(&mut w, 1, op);
    assert!(run.reports.iter().all(TxReport::ok), "an empty pool blocked the update");
    assert!(run.outcome.expect("applied").advanced, "update 1 advanced the instance");
    assert_eq!(((balance(&w, op) - 1), ready(&w).paid_count), (0, 0), "an empty pool pays nothing");

    // Half a rebate in the pool: the next advance through it is paid what the pool holds.
    let r = fund(&mut w, REBATE / 2, 2);
    assert!(r.ok());
    let run = land(&mut w, 2, op);
    assert!(run.reports.iter().all(TxReport::ok), "{:?}", run.reports.last().map(|r| (&r.label, r.exit_code)));
    assert_eq!((balance(&w, op) - 1), REBATE / 2);
    assert_eq!(balance(&w, pool(&w).account_id), 0);

    // A `Land` naming a job that does not exist: the attestation program refuses `Finish`, nothing is paid.
    let before = pool_state(&w);
    let p = pool(&w);
    let message = RebateMessage::Land { job_id: [0xee; 32], payee: op };
    let job = ferrata_lez_wire::job_actor(w.inst.program, &w.inst.id, &[0xee; 32]);
    let time = time_at(&w, 2);
    let actors = vec![p, Actor::native_balance(p.account_id), Actor::native_balance(op), w.inst.actor(), job];
    let r =
        w.chain.submit("Land of a job that does not exist", p, borsh::to_vec(&message).unwrap(), actors, vec![], time);
    assert!(!r.ok(), "Land of no job was applied");
    println!(
        "refused as expected: {} → exit {:?} ({})",
        r.label,
        r.exit_code,
        r.exit_code.map(exit_name).unwrap_or_default()
    );
    assert_eq!(pool_state(&w), before);
    assert_eq!((balance(&w, op) - 1), REBATE / 2);
}
