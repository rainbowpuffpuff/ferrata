//! An attestation read again after later committee rotations, on RFP-021 Reliability #3: "A
//! verified statement over a finalised header remains valid indefinitely. Later Ethereum
//! activity, including subsequent sync-committee rotations, must never invalidate an attestation
//! that was valid when produced."
//!
//! Three committee handoffs need signing keys no real committee gives out, so the chain is
//! synthetic (`ferrata_spectests::testkit`), as `tests/redteam_lc.rs`'s `synthetic()` builds one.
//! The header the instance finalizes is real where it matters: H is a fabricated beacon header
//! whose Electra execution payload header is the one the attest fixture
//! `mainnet-attest-2026-11-10` finalized (block 26,384,851), so the record `header(id, slot)`
//! binds that block — its number, hash, parent hash, state root, receipts root and timestamp
//! (`ferrata_core::witness::HeaderRecord`), the fields the program proves `eth_getProof` and
//! `eth_getBlockReceipts` data against. `harness::attest::accepted` runs the same proofs under
//! the real chain's record; this test runs them through the program on the same instance that
//! then turns over three sync-committee periods.
//!
//! The flow: finalize H through the public job (a finality update of the synthetic chain that
//! also adopts the next committee); attest the fixture's WETH storage slot and its ERC-20
//! `Transfer` log under H's record, as the root of a transaction and through the reference
//! consumer's `CallAttest`; then rotate the store three times (periods 1908, 1909, 1910), each
//! rotation an update signed by the committee then current that hands off to a new one, applied
//! through the program, each checked to have taken effect (the store's current committee is the
//! new one, and an update signed by the committee the rotation just retired is refused);
//! then read the same record again: same verdict, the same statement bytes and identifiers, the
//! record account unchanged byte for byte, and the reference consumer's claim of the transfer
//! under H's record — made only now, after the rotations — credits its sender once.

use ferrata_core::{
    fork::LcFork,
    merkle::{
        CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, EXECUTION_PAYLOAD_GINDEX, FINALIZED_ROOT_GINDEX_ELECTRA,
        NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, ZERO,
    },
    steps::HeaderRecord,
    types::{
        BeaconBlockHeader, Execution, ExecutionPayloadHeader, LightClientBootstrap, LightClientHeader,
        LightClientUpdate, SyncAggregate, SyncCommittee,
    },
    validate::{sync_committee_signing_root, Context},
    witness::{CommitteeSummary, ExecutionSummary},
    Error,
};
use ferrata_lez_wire::{
    attest::{evaluate, Attestation, Cmp, Fact, Field, Request},
    consumer::{
        address_word, balance_actor, claim_actor, claim_seed, code as consumer_code, nullifier, ClaimState,
        ConsumerMessage, ConsumerState, Pool, TRANSFER_TOPIC,
    },
    header_actor, Message, ATTESTED_SELECTOR,
};
use ferrata_spectests::testkit::{fabricated_header, fake_committee, filler, sign, sparse_tree, FakeCommittee};
use harness::{attest, consumer_account, mainnet_config, scenarios::World, scenarios::BLS, TxReport};
use lee::{AccountId, Actor, PublicAccountEvidence};
use lee_core::program::PdaSeed;

/// 8192 = 32 slots/epoch * 256 epochs/period, the mainnet preset `mainnet_config` copies.
const PERIOD_SLOTS: u64 = 8_192;
/// The sync-committee period the instance starts in; the three rotations move through 1908, 1909
/// and 1910.
const START_PERIOD: u64 = 1_907;
/// The slot of H, 64 slots into the instance's first period.
const H_SLOT_OFFSET: u64 = 64;
/// How many committee rotations the instance goes through between the two attestations.
const ROTATIONS: usize = 3;

// ---------------------------------------------------------------------------------------------
// The synthetic chain, as tests/redteam_lc.rs's synthetic() and testkit::fabricated_update
// ---------------------------------------------------------------------------------------------

fn ctx(w: &World) -> Context<'_> {
    Context { spec: &w.spec, genesis_validators_root: w.inst.config.genesis_validators_root, bls: &BLS }
}

/// A beacon header at `slot` whose execution payload header is `payload` and whose body root
/// proves it: the record of this header binds `payload`'s execution block. As
/// `testkit::fabricated_header`, with the payload given instead of filler.
fn header_of_payload(payload: &ExecutionPayloadHeader, slot: u64, tag: u64) -> LightClientHeader {
    let (body_root, branches) = sparse_tree(&[(EXECUTION_PAYLOAD_GINDEX, payload.hash_tree_root_deneb())], tag);
    LightClientHeader {
        beacon: BeaconBlockHeader {
            slot,
            proposer_index: 7,
            parent_root: filler(tag + 20),
            state_root: filler(tag + 21),
            body_root,
        },
        execution: Execution::Payload { header: payload.clone(), branch: branches[0].clone() },
    }
}

/// An Electra-layout update about the synthetic chain, as `testkit::fabricated_update`, whose
/// finalized header is given: the attested state proves it (and `next_committee`, if any) by
/// branches that verify by construction, and `fake` signs it under `ctx`.
fn update_finalizing(
    finalized: &LightClientHeader,
    attested_slot: u64,
    signature_slot: u64,
    next_committee: Option<&SyncCommittee>,
    fake: &FakeCommittee,
    ctx: &Context<'_>,
) -> LightClientUpdate {
    let n = fake.committee.pubkeys.len();
    let mut attested = fabricated_header(attested_slot, 80 + attested_slot);
    let mut leaves = vec![(FINALIZED_ROOT_GINDEX_ELECTRA, finalized.beacon.hash_tree_root())];
    if let Some(c) = next_committee {
        leaves.push((NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, c.hash_tree_root()));
    }
    let (state_root, branches) = sparse_tree(&leaves, 7_000 + attested_slot);
    attested.beacon.state_root = state_root;
    let mut branches = branches.into_iter();
    let finality_branch = branches.next().unwrap();
    let next_sync_committee_branch = match next_committee {
        Some(_) => branches.next().unwrap(),
        None => vec![ZERO; LcFork::Electra.sync_committee_branch_len()],
    };
    let bits = vec![true; n];
    let mut update = LightClientUpdate {
        attested_header: attested,
        next_sync_committee: next_committee.cloned().unwrap_or_else(|| SyncCommittee::empty(n)),
        next_sync_committee_branch,
        finalized_header: finalized.clone(),
        finality_branch,
        sync_aggregate: SyncAggregate { bits: bits.clone(), signature: [0; 96] },
        signature_slot,
    };
    update.sync_aggregate.signature = sign(fake, &bits, &sync_committee_signing_root(&update, ctx));
    update
}

// ---------------------------------------------------------------------------------------------
// Attestation through the program, as harness::attest::Stage4 sends it
// ---------------------------------------------------------------------------------------------

/// What the program must answer for `request` under the record at `slot`: its own evaluation
/// (`ferrata_lez_wire::attest::evaluate`) on the record the chain holds.
fn expected(w: &World, slot: u64, request: &Request) -> Attestation {
    let state = w.inst.record(&w.chain, slot).expect("the record at this slot");
    evaluate(&state.instance_id, state.chain_id, &state.header().expect("a record"), request)
        .unwrap_or_else(|code| panic!("refused natively ({code})"))
}

/// `Attest` of `request` under the record at `slot`, as the root of a transaction. Returns the
/// data of the event the record emits (the instance id, the chain id, the block number, then
/// every statement identifier).
fn attest_root(w: &mut World, label: &str, slot: u64, request: &Request, time: u64) -> Vec<u8> {
    let header = header_actor(w.inst.program, &w.inst.id, slot);
    let message = borsh::to_vec(&Message::Attest { request: request.clone() }).unwrap();
    let report = w.chain.submit(label, header, message, vec![header], vec![], time);
    assert!(report.ok(), "{label}: {:?}", report.charge.revert);
    assert_eq!(report.exit_code, None, "{label}");
    let event = report
        .events
        .iter()
        .find(|e| e.event.selector == ATTESTED_SELECTOR)
        .unwrap_or_else(|| panic!("{label}: no attestation event"));
    assert_eq!(event.account_id, w.inst.program, "{label}: the program emits the event");
    event.event.data.clone()
}

/// A public actor of the consumer calls `Attest` under the record at `slot` (`CallAttest`) and
/// keeps the reply. Returns the reply it stored, byte for byte.
fn call_attest(w: &mut World, label: &str, slot: u64, request: &Request, time: u64) -> Vec<u8> {
    let program = consumer_account();
    let seed = PdaSeed::new([0x45; 32]);
    let reader = Actor::new(AccountId::for_public_pda(&program, &seed), program);
    let header = header_actor(w.inst.program, &w.inst.id, slot);
    let message = ConsumerMessage::CallAttest { ferrata: w.inst.program, slot, request: request.clone() };
    let report = w.chain.submit(
        label,
        reader,
        borsh::to_vec(&message).unwrap(),
        vec![reader, header],
        vec![PublicAccountEvidence::Pda { program, seed }],
        time,
    );
    assert!(report.ok(), "{label}: {:?}", report.charge.revert);
    match attest::consumer_state(&w.chain, reader) {
        Some(ConsumerState::Reader(state)) => state.attestation.unwrap_or_else(|| panic!("{label}: the reply is kept")),
        other => panic!("{label}: the reader holds {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// The test
// ---------------------------------------------------------------------------------------------

#[test]
fn a_statement_over_a_finalized_header_survives_three_committee_rotations() {
    let fx = attest::Fixture::load();

    // Five committees the test holds: c[0] signs the bootstrap's period and the update that
    // finalizes H, c[1]..c[4] the four handoffs (the last committee is only adopted, never signs).
    let c: Vec<FakeCommittee> = (0..5u8).map(|i| fake_committee(512, 0x60 + i)).collect();
    let mut aggregates = std::collections::BTreeSet::new();
    for f in &c {
        assert!(aggregates.insert(f.committee.aggregate_pubkey), "the committees are distinct");
    }

    // The synthetic chain: a mainnet-preset instance whose bootstrap (period 1907) the test's
    // first committee signs for, as tests/redteam_lc.rs's synthetic().
    let base = PERIOD_SLOTS * START_PERIOD;
    let bootstrap = {
        let mut header = fabricated_header(base, 9_000);
        let (state_root, branches) =
            sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, c[0].committee.hash_tree_root())], 9_100);
        header.beacon.state_root = state_root;
        LightClientBootstrap {
            header,
            current_sync_committee: c[0].committee.clone(),
            current_sync_committee_branch: branches[0].clone(),
        }
    };
    let checkpoint = bootstrap.header.beacon.hash_tree_root();
    let mut w = World::from_parts(mainnet_config(2), checkpoint, bootstrap, vec![]);

    // H: the real block's execution payload header under a fabricated beacon header, in the
    // store's own layout (Electra: a payload header, whose fields the record keeps, not a Gloas
    // block hash the request would have to supply a header for).
    let real = match &fx.update.finalized_header.execution {
        Execution::Payload { header, .. } => header.clone(),
        other => panic!("the fixture's finalized header carries a payload header: {other:?}"),
    };
    assert_eq!(fx.update.finalized_header.beacon.slot, fx.finalized_slot, "the fixture's own record");
    assert_eq!(real.block_number, fx.block, "the payload header is the finalized block's");
    assert_eq!(real.receipts_root, fx.receipts_root(), "the receipts trie rebuilt from eth_getBlockReceipts");
    let h_slot = base + H_SLOT_OFFSET;
    let h = header_of_payload(&real, h_slot, 9_200);
    let h_record = HeaderRecord::of(&h, w.inst.fork(), &w.spec).encode();

    // Finalize H through the public job: a finality update signed by c[0] in the store's period
    // that also adopts c[1] as the next committee (the store does not know it yet), as mainnet's
    // update_0 does. The record of H is written at header(id, h_slot).
    let u_h = update_finalizing(&h, base + 96, base + 97, Some(&c[1].committee), &c[0], &ctx(&w));
    let run = w.apply(&u_h, "finality update finalizing H (adopts committee 1)");
    assert!(run.reports.iter().all(TxReport::ok), "{:?}", run.reports.last());
    assert_eq!(run.outcome.map(|o| (o.rotated, o.advanced, o.adopted)), Some((false, true, true)));
    let record = w.inst.record(&w.chain, h_slot).expect("the record of H");
    assert_eq!(record.instance_id, w.inst.id);
    assert_eq!(record.chain_id, fx.chain_id);
    assert_eq!(record.root_id, w.inst.id);
    assert_eq!(record.record, h_record, "the record is H's");
    assert_eq!(
        record.header().unwrap().execution,
        ExecutionSummary::Payload {
            block_number: real.block_number,
            block_hash: real.block_hash,
            parent_hash: real.parent_hash,
            state_root: real.state_root,
            receipts_root: real.receipts_root,
            timestamp: real.timestamp,
        },
        "the record binds the real execution block"
    );

    // The record account as it stands now; every later assertion compares against these bytes.
    let record_account = w.chain.actor_state(header_actor(w.inst.program, &w.inst.id, h_slot));
    assert!(!record_account.is_empty(), "the record account holds H's record");

    // The facts under H's record: the fixture's WETH storage slot, and its ERC-20 Transfer log.
    let weth = fx.weth_proof();
    let weth_slot = fx.weth_slot;
    let balance = weth.storage.iter().find(|p| p.key == weth_slot).expect("the balance slot").value;
    assert_eq!(balance, fx.weth_balance_by_call(fx.block), "eth_getProof and eth_call agree");
    let request = Request { instance_id: w.inst.id, chain_id: fx.chain_id, ..Request::default() };
    let storage = Request {
        accounts: vec![attest::account_query(
            &weth,
            vec![],
            vec![(weth_slot, vec![attest::condition(Field::Value, Cmp::Ge, attest::u256(1))])],
        )],
        ..request
    };
    let t = fx.transfer.clone();
    let pool = Pool {
        ferrata: w.inst.program,
        instance_id: w.inst.id,
        chain_id: fx.chain_id,
        token: t.token,
        recipient: t.to,
    };
    let log = pool.request(t.tx_index, t.log_index, fx.receipt_proof(t.tx_index), None);
    let log_id =
        attest::log_statement_id(fx.chain_id, fx.block, t.tx_index, t.log_index, fx.json_log(t.tx_index, t.log_index));

    // A block time after every signature slot below; `Attest` and the consumer's messages carry
    // no validity window (harness::attest::Stage4 sends all of its transactions at one time).
    let last_slot = PERIOD_SLOTS * (START_PERIOD + 1 + ROTATIONS as u64) + 161;
    let time = w.inst.config.slot_start_ms(last_slot) + 12_000;

    // What the program must answer for both facts, from the record the chain holds (the
    // identifiers re-derived here from the JSON-RPC fields of the fixture).
    let expected_storage = expected(&w, h_slot, &storage);
    assert_eq!(expected_storage.statements.len(), 2, "the account, then its slot");
    assert_eq!(expected_storage.block_number, fx.block);
    assert_eq!(expected_storage.slot, h_slot);
    assert_eq!(expected_storage.hops, 0);
    assert_eq!(
        expected_storage.statements[0].fact,
        Fact::Account {
            address: weth.address,
            nonce: weth.nonce,
            balance: weth.balance,
            storage_root: weth.storage_hash,
            code_hash: weth.code_hash
        }
    );
    assert_eq!(
        expected_storage.statements[1].fact,
        Fact::Storage { address: weth.address, slot: weth_slot, value: balance }
    );
    assert_eq!(
        expected_storage.statements[1].id,
        attest::storage_statement_id(fx.chain_id, &weth.address, &weth_slot, &balance),
        "the identifier re-derived from eth_getProof"
    );
    let expected_log = expected(&w, h_slot, &log);
    assert_eq!(expected_log.statements.len(), 1);
    assert_eq!(expected_log.statements[0].id, log_id, "the identifier re-derived from eth_getBlockReceipts");
    let Fact::Log { tx_index, log_index, address, topics, data, .. } = &expected_log.statements[0].fact else {
        panic!("a log fact");
    };
    assert_eq!((*tx_index, *log_index, *address), (t.tx_index, t.log_index, t.token));
    assert_eq!(topics.len(), 3);
    assert_eq!(topics[0], TRANSFER_TOPIC, "an ERC-20 Transfer");
    assert_eq!(topics[1][12..], t.from, "the transfer's sender");
    assert_eq!(topics[2], address_word(&t.to), "the pool's recipient");
    assert_eq!(data, &t.value.to_vec(), "the transferred amount");

    // Attested through the program, before any rotation: as the root of a transaction (the event
    // names the identifiers) and through the reference consumer's CallAttest (the reply is the
    // whole attestation).
    let storage_event_before =
        attest_root(&mut w, "the storage fact under H, before the rotations", h_slot, &storage, time);
    assert_eq!(storage_event_before, expected_storage.event_data(), "the event names the expected identifiers");
    let log_event_before = attest_root(&mut w, "the log fact under H, before the rotations", h_slot, &log, time);
    assert_eq!(log_event_before, expected_log.event_data());
    let reply_before =
        call_attest(&mut w, "a public actor calls Attest under H, before the rotations", h_slot, &storage, time);
    assert_eq!(reply_before, borsh::to_vec(&expected_storage).unwrap(), "the reply is the whole attestation");

    let store_before = w.inst.store(&w.chain);
    assert_eq!(store_before.finalized.slot, h_slot, "the store stands at H");
    assert_eq!(store_before.current, CommitteeSummary::of(&c[0].committee));

    // Three committee rotations. Each is a finality update of the next period, signed by the
    // committee the store holds as next, attesting the handoff to a new committee; applying it
    // rotates the store (apply_core: current <- next <- the update's), as mainnet's update_1
    // does. After each, an update of the same shape but signed by the committee the rotation
    // just retired must be refused: the signature no longer verifies.
    for r in 0..ROTATIONS {
        let period = START_PERIOD + 1 + r as u64;
        let p = PERIOD_SLOTS * period;
        let finalized = fabricated_header(p + H_SLOT_OFFSET, 40 + p + H_SLOT_OFFSET);
        let update = update_finalizing(&finalized, p + 96, p + 97, Some(&c[r + 2].committee), &c[r + 1], &ctx(&w));
        let run = w.apply(&update, &format!("rotation {r}: period {period} hands over to a new committee"));
        assert!(run.reports.iter().all(TxReport::ok), "rotation {r}: {:?}", run.reports.last());
        assert_eq!(run.outcome.map(|o| (o.rotated, o.advanced, o.adopted)), Some((true, true, true)), "rotation {r}");
        let store = w.inst.store(&w.chain);
        assert_eq!(store.finalized.slot, p + H_SLOT_OFFSET, "rotation {r} finalized a later header");
        assert_eq!(store.current, CommitteeSummary::of(&c[r + 1].committee), "rotation {r} took effect");
        assert_ne!(store.current, CommitteeSummary::of(&c[r].committee), "the old committee is gone");
        assert_eq!(store.next.as_ref(), Some(&CommitteeSummary::of(&c[r + 2].committee)));
        assert!(w.inst.record(&w.chain, p + H_SLOT_OFFSET).is_some(), "the rotation's header has its record");

        // The retired committee's signature on a well-formed newer update of the new period:
        // every other check passes, the signature does not.
        let retired = fabricated_header(p + 2 * H_SLOT_OFFSET, 41 + p + 2 * H_SLOT_OFFSET);
        let refused = update_finalizing(&retired, p + 160, p + 161, None, &c[r], &ctx(&w));
        {
            let ctx = ctx(&w);
            let mut probe = w.native.clone();
            assert_eq!(
                probe.process_update(&refused, refused.signature_slot + 1, &ctx),
                Err(Error::SignatureInvalid),
                "rotation {r}: the retired committee's update, natively"
            );
        }
        let store_now = w.inst.store(&w.chain);
        let run = w.apply(&refused, &format!("rotation {r}: an update signed by the retired committee"));
        assert_eq!(run.reports.len(), 3, "Open, then Miller, then the refusal");
        assert!(run.reports[0].ok() && run.reports[1].ok(), "rotation {r}: steps 1 and 2 pass");
        let last = run.reports.last().unwrap();
        // 40 = Error::SignatureInvalid at the final exponentiation, as harness's refusals() sees
        // for a forged signature: steps 1 and 2 pass, step 3 refuses.
        assert_eq!(last.exit_code, Some(40), "rotation {r}: {:?}", last.charge.revert);
        assert_eq!(w.inst.store(&w.chain), store_now, "rotation {r}: the refused update changes nothing");
        assert_eq!(w.inst.record(&w.chain, p + 2 * H_SLOT_OFFSET), None, "nothing was finalized by it");
    }

    // The store has moved three periods on; H is two periods stale.
    let store_after = w.inst.store(&w.chain);
    assert_ne!(store_after, store_before, "the rotations moved the store");
    assert_eq!(store_after.finalized.slot, PERIOD_SLOTS * (START_PERIOD + ROTATIONS as u64) + H_SLOT_OFFSET);
    w.check_store();

    // H's record is unchanged, byte for byte, and still answers with the same statements.
    assert_eq!(w.chain.actor_state(header_actor(w.inst.program, &w.inst.id, h_slot)), record_account);
    assert_eq!(w.inst.record(&w.chain, h_slot).unwrap().record, h_record);
    assert_eq!(expected(&w, h_slot, &storage), expected_storage, "the native evaluation of the same record");
    let storage_event_after =
        attest_root(&mut w, "the storage fact under H, after the rotations", h_slot, &storage, time);
    assert_eq!(storage_event_after, storage_event_before, "the same statement identifiers, the same bytes");
    let log_event_after = attest_root(&mut w, "the log fact under H, after the rotations", h_slot, &log, time);
    assert_eq!(log_event_after, log_event_before, "the same statement identifier, the same bytes");
    let reply_after =
        call_attest(&mut w, "a public actor calls Attest under H, after the rotations", h_slot, &storage, time);
    assert_eq!(reply_after, reply_before, "the same reply, byte for byte");
    let attested: Attestation = borsh::from_slice(&reply_after).unwrap();
    assert_eq!((attested.instance_id, attested.chain_id, attested.slot), (w.inst.id, fx.chain_id, h_slot));
    assert_eq!((attested.block_number, attested.hops), (fx.block, 0));
    assert_eq!(
        attested.statements[1].id,
        attest::storage_statement_id(fx.chain_id, &weth.address, &weth_slot, &balance),
        "the identifier the statement had before the rotations"
    );

    // The reference consumer accepts the statement now, after the rotations: the claim of the
    // transfer under H's record (the nullifier was never spent, before or during the rotations)
    // verifies the log against the record H holds and credits the sender once.
    let pool_id = pool.id();
    let spent = nullifier(&pool_id, &log_id);
    let claim = claim_actor(consumer_account(), &pool_id, &spent);
    let balance_at = balance_actor(consumer_account(), &pool_id, &t.from);
    let header = header_actor(w.inst.program, &w.inst.id, h_slot);
    let message = ConsumerMessage::Claim {
        pool: pool.clone(),
        slot: h_slot,
        tx_index: t.tx_index,
        log_index: t.log_index,
        proof: fx.receipt_proof(t.tx_index),
        header: None,
    };
    let report = w.chain.submit(
        "the consumer claims the transfer under H, after the rotations",
        claim,
        borsh::to_vec(&message).unwrap(),
        vec![claim, header, balance_at],
        vec![PublicAccountEvidence::Pda { program: consumer_account(), seed: claim_seed(&pool_id, &spent) }],
        time,
    );
    assert!(report.ok(), "{:?} (exit {:?})", report.charge.revert, report.exit_code);
    assert_eq!(report.transitions.len(), 4, "Claim, Attest, the reply, Credit");
    let Some(ConsumerState::Claim(ClaimState::Spent(deposit))) = attest::consumer_state(&w.chain, claim) else {
        panic!("the nullifier is spent");
    };
    assert_eq!((deposit.statement_id, deposit.owner, deposit.amount), (log_id, t.from, t.amount()));
    assert_eq!(deposit.block_number, fx.block, "the deposit's block is H's");
    let Some(ConsumerState::Balance(balance_state)) = attest::consumer_state(&w.chain, balance_at) else {
        panic!("the balance actor holds a balance state");
    };
    assert_eq!(
        (balance_state.pool_id, balance_state.owner, balance_state.amount, balance_state.credits),
        (pool_id, t.from, t.amount(), 1),
        "the transfer's sender is credited once"
    );
    // And the same claim again is refused by its nullifier.
    let replay = w.chain.submit(
        "the same claim again (replay)",
        claim,
        borsh::to_vec(&message).unwrap(),
        vec![claim, header, balance_at],
        vec![PublicAccountEvidence::Pda { program: consumer_account(), seed: claim_seed(&pool_id, &spent) }],
        time,
    );
    assert_eq!(replay.exit_code, Some(u32::from(consumer_code::ALREADY_CLAIMED)));
    assert_eq!(replay.transitions.len(), 1, "refused in the claim actor itself");
    assert_eq!(
        attest::consumer_state(&w.chain, balance_at).map(|s| match s {
            ConsumerState::Balance(b) => (b.amount, b.credits),
            other => panic!("a balance actor holds {other:?}"),
        }),
        Some((t.amount(), 1)),
        "still credited once"
    );

    // The consumer's use of the record changed nothing about it either.
    assert_eq!(w.chain.actor_state(header_actor(w.inst.program, &w.inst.id, h_slot)), record_account);
    println!(
        "statements over slot {h_slot} (block {}) read the same after {ROTATIONS} rotations, store in period {}",
        fx.block,
        store_after.finalized.slot / PERIOD_SLOTS
    );
}
