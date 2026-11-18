//! EXPERIMENTAL, bench-only: a sync-committee handoff (a period update) in ONE public transaction
//! through LEZ v0.4's state machine, the next committee adopted by its summary
//! (`bench/RESIDUE.md` §10: sound only under the honest-supermajority assumption the light client
//! already makes), on top of the residue witness (§6) and one multiproof for the non-signers (§9).
//!
//! The program is `guests/program-residue-experimental`, an unaudited copy of `guests/program`:
//! its `VerifyAll` of an update that adopts a committee, given no adoption keys, adopts the next
//! committee by the summary step 1 proved in the signed attested state
//! (`residue_handoff::adopt_summary`); given the 512 keys it adopts them as the deployed program
//! does. It is deployed here at a named account of its own, and nowhere else.
//!
//! * mainnet update_1 as served (a handoff, 510 of 512, the real committee): with the 512 keys in
//!   the same transaction, over the cycle budget; with one node of its next-committee branch altered,
//!   refused (27); with the summary adoption, applied in one transaction, the store equal to the
//!   native policy store's and to the natively applied core job's (the 512-key adoption included),
//!   the record written; sent again, refused as stale;
//! * the same update signed by a committee whose keys the test holds (its summary written into the
//!   instance's store, as `residue_experimental.rs`): 341 signers refused (51); 342 signers at the
//!   costliest placement of the absent members found (`bench/results/residue/agg/placements.md`)
//!   and with every third member absent, applied in one transaction each (blst's batch sum; at the
//!   costliest placement also the chain of additions);
//! * the deployed program's image id, printed by the same build, is still the one
//!   `docs/image-ids.md` lists.
//!
//! Behind the feature `residue-experimental`:
//! `RISC0_DEV_MODE=1 cargo test -p harness --features residue-experimental --test residue_handoff -- --nocapture --test-threads 1`
#![cfg(feature = "residue-experimental")]

use std::borrow::Cow;

use ferrata_core::{
    fork::LcFork,
    policy::Adoption,
    steps::{self, Phase},
    types::{LightClientUpdate, SyncCommittee},
    validate::{sync_committee_signing_root, Context},
    witness::{uncompressed_keys, CommitteeSummary, CompactStore, CompactUpdate, SignerWitness},
};
use ferrata_lez_wire::{header_actor, InstanceState, Message, UncompressedKey};
use ferrata_spectests::testkit::{fake_committee, sign, FakeCommittee};
use harness::{scenarios::World, *};
use lee::{program::Program, AccountId, Actor, V03State};
use lee_core::account::ActorState;
use residue_agg::{NonSigners, Sum};

/// Where the experimental copy is deployed in this test.
const EXPERIMENTAL: AccountId = AccountId::new([0x5e; 32]);

/// The deployed light-client program's image id (`docs/image-ids.md`).
const DEPLOYED_ID: &str = "816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df";

fn experimental_program() -> Program {
    Program::new_unchecked(
        methods_residue_experimental::FERRATA_LEZ_RESIDUE_EXPERIMENTAL_ID,
        Cow::Borrowed(methods_residue_experimental::FERRATA_LEZ_RESIDUE_EXPERIMENTAL_ELF),
    )
}

fn print_header() {
    println!(
        "image ids in this build: experimental copy {}, deployed program {}",
        image_id_hex(methods_residue_experimental::FERRATA_LEZ_RESIDUE_EXPERIMENTAL_ID),
        image_id_hex(methods::FERRATA_LEZ_ID)
    );
    assert!(
        image_id_hex(methods::FERRATA_LEZ_ID).trim_start_matches("0x") == DEPLOYED_ID,
        "the deployed program's image id changed"
    );
    println!("| transaction | handlers run (cycles each) | execution gas | of MAX_GAS_EXEC 10M | storage gas (tx bytes) | base fee | result |");
    println!("|---|---|---:|---:|---:|---:|---|");
}

fn print_report(r: &TxReport) {
    let handlers: Vec<String> = r.transitions.iter().map(|t| format!("{} {}", t.handler, t.cycles)).collect();
    println!(
        "| {} | {} | {} | {:.1}% | {} | {} | {} |",
        r.label,
        handlers.join(" → "),
        r.charge.gas_exec,
        r.charge.gas_exec as f64 / 100_000.0,
        r.charge.gas_stor,
        r.charge.fee_base,
        match (&r.charge.revert, r.exit_code) {
            (None, _) => "applied".to_string(),
            (Some(_), Some(code)) => format!("refused: exit {code}"),
            (Some(e), None) => format!("reverted: {e}"),
        }
    );
}

fn signing_committee<'a>(
    full: &'a ferrata_core::store::StoreCore,
    update: &LightClientUpdate,
    spec: &ferrata_core::fork::ChainSpec,
) -> &'a SyncCommittee {
    if spec.period_at_slot(update.signature_slot) == spec.period_at_slot(full.finalized_header.beacon.slot) {
        &full.current_sync_committee
    } else {
        &full.next_sync_committee
    }
}

/// An instance of the copy, initialized and advanced by update_0 through the copy's job path.
fn world_after_update_0() -> World {
    let mut chain = Chain::new();
    let state = std::mem::replace(&mut chain.state, V03State::new());
    chain.state = state.with_named_programs([(EXPERIMENTAL, experimental_program())]);
    let config = mainnet_config(2);
    let checkpoint = root(TRUSTED_BLOCK_ROOT);
    let bootstrap = mainnet_bootstrap();
    let spec = config.spec();
    let inst = Instance::new(EXPERIMENTAL, config, checkpoint);
    let native = ferrata_core::policy::FinalizedStore::initialize_with(
        &checkpoint,
        &bootstrap,
        inst.fork(),
        &spec,
        &scenarios::BLS,
        Adoption::OnChain,
    )
    .unwrap();
    let run = inst.init(&mut chain, &bootstrap, "Init");
    assert!(run.reports.iter().all(TxReport::ok), "initialization refused: {:?}", run.reports);
    let mut w = World { chain, inst, native, spec, bootstrap, updates: mainnet_updates(), runs: vec![run] };
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    w.check_store();
    w
}

/// [`world_after_update_0`], then update_1's signing committee replaced in the instance's store by
/// a committee whose keys the test holds (its summary), written with LEZ's `force_insert_account`.
fn world_with_the_bench_committee() -> (World, FakeCommittee, CompactStore) {
    let mut w = world_after_update_0();
    let update = w.updates[1].clone();
    let store = w.inst.store(&w.chain);
    let real = signing_committee(w.native.core(), &update, &w.spec).clone();
    let fake = fake_committee(512, 7);
    let mut substituted = store.clone();
    if substituted.current == CommitteeSummary::of(&real) {
        substituted.current = CommitteeSummary::of(&fake.committee);
    } else {
        assert_eq!(substituted.next, Some(CommitteeSummary::of(&real)));
        substituted.next = Some(CommitteeSummary::of(&fake.committee));
    }
    let actor = w.inst.actor();
    let mut account = w.chain.state.get_account_by_id(actor.account_id);
    let mut state: InstanceState = borsh::from_slice(account.data.actor_state(actor.program_account_id)).unwrap();
    state.store = substituted.encode();
    account.data.set_actor_state(actor.program_account_id, ActorState::from(borsh::to_vec(&state).unwrap()));
    w.chain.state.force_insert_account(actor.account_id, account);
    assert_eq!(w.inst.store(&w.chain), substituted);
    (w, fake, substituted)
}

/// update_1 as served (a handoff), signed by the members of `fake` whose bit is set.
fn signed_by(w: &World, fake: &FakeCommittee, bits: Vec<bool>) -> LightClientUpdate {
    let gvr = w.inst.config.genesis_validators_root;
    let ctx = Context { spec: &w.spec, genesis_validators_root: gvr, bls: &scenarios::BLS };
    let mut u = w.updates[1].clone();
    u.sync_aggregate.signature = sign(fake, &bits, &sync_committee_signing_root(&u, &ctx));
    u.sync_aggregate.bits = bits;
    u
}

/// In each of the 128 groups of four members (4g .. 4g+3): member `4g + one` absent, and in the
/// groups of `two_in` also member `4g + other` (as `bench/residue-host/tests/placements.rs`).
fn groups(one: usize, other: usize, two_in: std::ops::Range<usize>) -> Vec<bool> {
    let mut bits = vec![true; 512];
    for g in 0..128 {
        bits[4 * g + one] = false;
        if two_in.contains(&g) {
            bits[4 * g + other] = false;
        }
    }
    bits
}

/// The handoff natively: `residue_agg::step1` (checked equal to the core's `step1`), its honest
/// residue witness, and the store the core reaches after all its steps, the 512-key adoption
/// included (which passes: the aggregate is the sum of the keys).
fn native_handoff(
    w: &World,
    store: &CompactStore,
    update: &LightClientUpdate,
    committee: &SyncCommittee,
) -> (NonSigners, [u8; residue::FP12_BYTES], [u8; residue::FP12_BYTES], CompactStore) {
    let gvr = w.inst.config.genesis_validators_root;
    let bits = &update.sync_aggregate.bits;
    let new = NonSigners::build(committee, bits).unwrap();
    let old = SignerWitness::build(committee, bits).unwrap();
    let cu = CompactUpdate::of(update);
    let mut job = residue_agg::step1(store, &cu, &new, update.signature_slot, &w.spec, &gvr, 1, Sum::Batch).unwrap();
    let core_job = steps::step1(store, &cu, &old, update.signature_slot, &w.spec, &gvr, 1).unwrap();
    assert_eq!(job, core_job, "residue_agg::step1 = the core's step1");
    assert!(job.adoption_pending(), "a handoff: applying it adopts a committee");
    job.step2().unwrap();
    let Phase::FinalExp { product } = job.phase.clone() else { panic!("step 2 leaves the product") };
    let f = residue::decode_fp12(&product).unwrap();
    let (c, wit) = residue::witness(&f);
    assert_eq!(residue::check(&f, &c, &wit), Ok(()));
    job.step3().expect("the signature verifies (final exponentiation, natively)");
    let keys = uncompressed_keys(&update.next_sync_committee).unwrap();
    job.adopt(0, &keys, keys.len()).expect("the core's 512-key adoption passes");
    let (expected, outcome) = steps::apply(store, &job, &w.spec).unwrap();
    assert!(outcome.adopted);
    (new, residue::encode_fp12(&c), residue::encode_fp12(&wit), expected)
}

/// `VerifyAll` at the copy: `signers` (a signer witness) followed by c and w, and `adoption`.
fn verify_all(
    w: &mut World,
    label: &str,
    update: &LightClientUpdate,
    signers: Vec<u8>,
    c: &[u8],
    wit: &[u8],
    adoption: Vec<UncompressedKey>,
) -> TxReport {
    let mut s = signers;
    s.extend_from_slice(c);
    s.extend_from_slice(wit);
    let msg = borsh::to_vec(&Message::VerifyAll {
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        store: w.inst.store(&w.chain).encode(),
        update: CompactUpdate::of(update).encode(LcFork::Electra),
        signers: s,
        adoption,
    })
    .unwrap();
    let verifier = Actor::new(w.chain.payer, EXPERIMENTAL);
    let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
    let time = w.inst.config.slot_start_ms(update.signature_slot) + 12_000;
    let report = w.chain.submit(label, verifier, msg, vec![verifier, w.inst.actor(), header], vec![], time);
    print_report(&report);
    report
}

/// The marked encoding of a `NonSigners` witness: 0xffff (blst's batch sum) or 0xfffe (the chain).
fn marked(n: &NonSigners, chain: bool) -> Vec<u8> {
    let mut s = if chain { vec![0xfe, 0xff] } else { vec![0xff, 0xff] };
    s.extend_from_slice(&n.encode());
    s
}

#[test]
fn a_handoff_in_one_transaction_at_mainnet_participation() {
    let mut w = world_after_update_0();
    let update = w.updates[1].clone();
    assert!(!update.next_sync_committee.is_empty(), "update_1 as served carries the next committee");
    assert_eq!(update.sync_aggregate.participants(), 510);
    let store = w.inst.store(&w.chain);
    let committee = signing_committee(w.native.core(), &update, &w.spec).clone();
    let (new, c, wit, expected) = native_handoff(&w, &store, &update, &committee);
    print_header();

    // The 512 keys adopted in the same transaction (the core's adoption): over the cycle budget.
    let keys = uncompressed_keys(&update.next_sync_committee).unwrap();
    let over = verify_all(
        &mut w,
        "EXPERIMENTAL VerifyAll, handoff update_1 (510 of 512): one multiproof + residue witness + the 512 keys adopted",
        &update,
        marked(&new, false),
        &c,
        &wit,
        keys,
    );
    assert!(!over.ok(), "the 512-key adoption does not fit with the update");
    assert_eq!(w.inst.store(&w.chain), store, "nothing changed");

    // One node of the next-committee branch altered: refused (NextCommitteeBranchInvalid, 27).
    let mut bad = update.clone();
    bad.next_sync_committee_branch[2][9] ^= 1;
    let refused = verify_all(
        &mut w,
        "EXPERIMENTAL VerifyAll, handoff with one node of its next-committee branch altered",
        &bad,
        marked(&new, false),
        &c,
        &wit,
        vec![],
    );
    assert_eq!(refused.exit_code, Some(27), "NextCommitteeBranchInvalid");
    assert_eq!(w.inst.store(&w.chain), store, "nothing changed");

    // The summary adoption: applied in one transaction.
    let report = verify_all(
        &mut w,
        "EXPERIMENTAL VerifyAll, handoff update_1 (510 of 512): one multiproof + residue witness + the summary adopted",
        &update,
        marked(&new, false),
        &c,
        &wit,
        vec![],
    );
    assert!(report.ok(), "{:?}", report.charge.revert);
    let gvr = w.inst.config.genesis_validators_root;
    let spec = w.spec.clone();
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &scenarios::BLS };
    let outcome = w.native.process_update(&update, update.signature_slot + 1, &ctx).unwrap();
    assert!(outcome.adopted, "the native policy store adopted the committee (its 512-key check)");
    w.check_store();
    assert_eq!(w.inst.store(&w.chain), expected, "the store is the core's after all its steps");
    w.check_record(update.finalized_header.beacon.slot);

    // The same handoff again: refused as stale, with the code step 1 gives natively.
    let after = w.inst.store(&w.chain);
    let want = residue_agg::step1(&after, &CompactUpdate::of(&update), &new, update.signature_slot, &w.spec, &gvr, 1, Sum::Batch)
        .unwrap_err()
        .code();
    let stale = verify_all(&mut w, "EXPERIMENTAL VerifyAll, the same handoff again", &update, marked(&new, false), &c, &wit, vec![]);
    assert_eq!(stale.exit_code, Some(u32::from(want)), "refused as stale");
    assert_eq!(w.inst.store(&w.chain), after, "nothing changed");
    println!(
        "handoff in one transaction at 510 of 512: {} execution gas ({:.1}% of MAX_GAS_EXEC); stale refusal code {want}",
        report.charge.gas_exec,
        report.charge.gas_exec as f64 / 100_000.0
    );
}

#[test]
fn a_342_signer_handoff_in_one_transaction() {
    print_header();
    // 341 signers: below the policy (BelowSupermajority, 51).
    {
        let (mut w, fake, substituted) = world_with_the_bench_committee();
        let bits: Vec<bool> = (0..512).map(|i| i >= 171).collect();
        let u = signed_by(&w, &fake, bits);
        assert_eq!(u.sync_aggregate.participants(), 341);
        let new = NonSigners::build(&fake.committee, &u.sync_aggregate.bits).unwrap();
        let zero = [0u8; residue::FP12_BYTES];
        let r = verify_all(&mut w, "EXPERIMENTAL VerifyAll, handoff signed by 341 of 512", &u, marked(&new, false), &zero, &zero, vec![]);
        assert_eq!(r.exit_code, Some(51), "BelowSupermajority");
        assert_eq!(w.inst.store(&w.chain), substituted, "nothing changed");
    }
    let placements: [(&str, Vec<bool>, bool); 3] = [
        ("every absent index odd (4g+3, and 4g+1 in groups 0..41): the costliest placement found", groups(3, 1, 0..42), false),
        ("every third member absent", (0..512).map(|i| i % 3 != 2).collect(), false),
        ("every absent index odd, the chain of additions", groups(3, 1, 0..42), true),
    ];
    for (label, bits, chain) in placements {
        let (mut w, fake, substituted) = world_with_the_bench_committee();
        let u = signed_by(&w, &fake, bits);
        assert_eq!(u.sync_aggregate.participants(), 342);
        let (new, c, wit, expected) = native_handoff(&w, &substituted, &u, &fake.committee);
        let report = verify_all(
            &mut w,
            &format!("EXPERIMENTAL VerifyAll, handoff signed by 342 of 512 ({label}): one multiproof + residue witness + the summary adopted"),
            &u,
            marked(&new, chain),
            &c,
            &wit,
            vec![],
        );
        assert!(report.ok(), "{label}: {:?}", report.charge.revert);
        assert_eq!(w.inst.store(&w.chain), expected, "{label}: the store is the core's after all its steps");
        assert!(w.inst.record(&w.chain, u.finalized_header.beacon.slot).is_some(), "{label}: the record is written");
        println!(
            "handoff in one transaction at 342 of 512 ({label}): {} execution gas ({:.1}% of MAX_GAS_EXEC)",
            report.charge.gas_exec,
            report.charge.gas_exec as f64 / 100_000.0
        );
    }
}

/// An independent review's finding 7, in the copy: a `Config` whose sync-committee period is zero epochs
/// would make every update panic in the period arithmetic (a division by zero) and be charged the whole
/// gas limit; the copy's `check_config` refuses it at `Init` (`BAD_CONFIG`, 212), and nothing is created.
#[test]
fn a_zero_period_config_is_refused_at_init() {
    let mut chain = Chain::new();
    let state = std::mem::replace(&mut chain.state, V03State::new());
    chain.state = state.with_named_programs([(EXPERIMENTAL, experimental_program())]);
    let mut config = mainnet_config(2);
    config.epochs_per_sync_committee_period = 0;
    let inst = Instance::new(EXPERIMENTAL, config, root(TRUSTED_BLOCK_ROOT));
    print_header();
    let run = inst.init(&mut chain, &mainnet_bootstrap(), "EXPERIMENTAL zero-period");
    print_report(&run.reports[0]);
    assert_eq!(run.reports[0].exit_code, Some(212), "BAD_CONFIG");
    assert!(run.reports.iter().all(|r| !r.ok()), "nothing applies");
}
