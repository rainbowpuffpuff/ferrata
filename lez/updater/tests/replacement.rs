//! RFP-021 Functionality #10, the consumer's side: moving to a replacement instance, as
//! `docs/guides/consumer-guide.md` §9.3 describes it. Two instances of the attestation program hold a record of the
//! same Ethereum block; a consumer bound to the first moves to the second by binding its requests to the second's
//! id. Then: the second attests the same fact with the same statement identifier; the old binding is refused at the
//! new record and the new binding at the old one (211), so a consumer is bound to exactly one instance at a time;
//! the first instance's record still attests (a statement never expires); and a nullifier namespace that includes
//! the instance, as the reference consumer's pool does, gives the same fact a new nullifier under the replacement,
//! which is why the recipe carries the predecessor's nullifiers over.
//!
//! The replacement is modelled as a second instance with its own id: the fixture's checkpoint under another
//! configuration (its committee adopted in four parts instead of two), so that both instances hold a record of the one
//! block the fixture has proofs for (mainnet block 26,384,851). For a consumer this is what a replacement is:
//! another instance id it decides to trust. Both instances are advanced by `ferrata-updater` on LEZ v0.4.0-rc1's
//! state machine in process (dev mode).
//!
//! `RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked --test replacement -- --nocapture`
#![cfg(feature = "harness")]

use anyhow::{bail, Result};
use ferrata_core::Root;
use ferrata_lez_wire::{
    attest::{evaluate, Attestation, Request},
    code, consumer, Message, RecordState,
};
use ferrata_updater::{
    chain::preset,
    config::{Fees, Policy},
    ledger::{HarnessLedger, Ledger, Tx},
    source::Source,
    updater::{Instance, Plan, Updater},
};
use harness::attest::{account_query, fixture_file, Fixture};
use lee::{Actor, PrivateKey};

const GAS: u64 = 4_800_000;
const MAX_FEE: u128 = 1_000_000_000;

struct FixtureNode {
    checkpoint: Root,
    bootstrap: Vec<u8>,
    finality: Vec<u8>,
}

impl Source for FixtureNode {
    fn name(&self) -> &str {
        "fixture-node"
    }
    fn bootstrap(&mut self, block_root: &Root) -> Result<Vec<u8>> {
        if *block_root == self.checkpoint {
            return Ok(self.bootstrap.clone());
        }
        bail!("no bootstrap for that root")
    }
    fn updates(&mut self, _: u64, _: u64) -> Result<Vec<Vec<u8>>> {
        Ok(vec![])
    }
    fn finality_update(&mut self) -> Result<Vec<u8>> {
        Ok(self.finality.clone())
    }
}

fn no_sleep(_: u64) {}

/// An instance of the fixture's checkpoint under `config`, advanced by an updater to the fixture's finalized slot.
fn advanced(l: &mut HarnessLedger, fx: &Fixture, config: ferrata_lez_wire::Config, payer: PrivateKey) -> Instance {
    let node = FixtureNode {
        checkpoint: fx.checkpoint,
        bootstrap: fixture_file("bootstrap.ssz"),
        finality: fixture_file("finality_update.ssz"),
    };
    let inst = Instance::new("mainnet", HarnessLedger::program(), config, fx.checkpoint);
    let policy = Policy { defer_window: 1, retry_base_ms: 0, ..Policy::default() };
    let mut u = Updater::new("updater", inst, payer, Fees::default(), policy, vec![Box::new(node)], None).unwrap();
    u.sleep = no_sleep;
    for _ in 0..50 {
        match u.tick(l).unwrap() {
            Plan::UpToDate => break,
            Plan::Stuck(w) | Plan::Waiting(w) => panic!("{w}"),
            _ => {}
        }
    }
    let slot = u.inst.store(l).unwrap().expect("active").finalized.slot;
    assert_eq!(slot, fx.finalized_slot);
    u.inst
}

fn request(fx: &Fixture, instance_id: Root) -> Request {
    let proof = fx.weth_proof();
    Request {
        instance_id,
        chain_id: fx.chain_id,
        accounts: vec![account_query(&proof, vec![], vec![(fx.weth_slot, vec![])])],
        ..Request::default()
    }
}

fn attest(l: &mut HarnessLedger, payer: &PrivateKey, header: Actor, req: &Request, time_ms: u64) -> (bool, Option<u32>) {
    let tx = Tx {
        label: "Attest".into(),
        root: header,
        message: borsh::to_vec(&Message::Attest { request: req.clone() }).unwrap(),
        actors: vec![header],
        evidence: vec![],
        time_ms,
    };
    let out = l.submit(payer, &tx, GAS, MAX_FEE).unwrap();
    (out.applied, out.exit_code)
}

fn native(l: &HarnessLedger, header: Actor, req: &Request) -> Result<Attestation, u8> {
    let rec: RecordState = borsh::from_slice(&l.actor_state(header).unwrap()).expect("a record");
    evaluate(&rec.instance_id, rec.chain_id, &rec.header().unwrap(), req)
}

#[test]
fn a_consumer_moves_to_a_replacement_instance() {
    let fx = Fixture::load();
    let (old_op, new_op, consumer_key) = (
        PrivateKey::try_new([0x51; 32]).unwrap(),
        PrivateKey::try_new([0x52; 32]).unwrap(),
        PrivateKey::try_new([0x53; 32]).unwrap(),
    );
    let mut l = HarnessLedger::new(&[old_op.clone(), new_op.clone(), consumer_key.clone()]);
    let time = preset("mainnet", 2).unwrap().slot_start_ms(fx.update.signature_slot) + 24_000;

    // The instance the consumer trusts first, and its statement.
    let old = advanced(&mut l, &fx, preset("mainnet", 2).unwrap(), old_op);
    let old_header = old.header(fx.finalized_slot);
    let bound_old = request(&fx, old.id);
    let before = native(&l, old_header, &bound_old).expect("attests");
    assert_eq!(attest(&mut l, &consumer_key, old_header, &bound_old, time), (true, None));

    // The replacement: another instance, initialised and advanced by someone else.
    let new = advanced(&mut l, &fx, preset("mainnet", 4).unwrap(), new_op);
    assert_ne!(new.id, old.id, "the replacement is another instance");
    let new_header = new.header(fx.finalized_slot);

    // The consumer re-binds its requests to the replacement's id: the same fact, the same identifier.
    let bound_new = request(&fx, new.id);
    let after = native(&l, new_header, &bound_new).expect("attests under the replacement");
    assert_eq!(attest(&mut l, &consumer_key, new_header, &bound_new, time), (true, None));
    let ids = |a: &Attestation| a.statements.iter().map(|s| s.id).collect::<Vec<_>>();
    assert_eq!(ids(&after), ids(&before), "statement identifiers do not depend on the instance");

    // Bound to exactly one instance: each binding is refused at the other's record.
    let wrong = u32::from(code::WRONG_INSTANCE);
    assert_eq!(native(&l, new_header, &bound_old).err(), Some(code::WRONG_INSTANCE));
    assert_eq!(attest(&mut l, &consumer_key, new_header, &bound_old, time), (false, Some(wrong)));
    assert_eq!(attest(&mut l, &consumer_key, old_header, &bound_new, time), (false, Some(wrong)));

    // The old record still attests: a statement over a finalized header never expires.
    assert_eq!(attest(&mut l, &consumer_key, old_header, &bound_old, time), (true, None));

    // Nullifiers. The reference consumer's pool includes the instance, so the same transfer would get a new
    // nullifier under the replacement: the recipe's step 4 refuses every nullifier the predecessor spent, derived
    // from the statement identifier and the predecessor's pool (constants the successor holds).
    let pool = |instance_id: Root| consumer::Pool {
        ferrata: HarnessLedger::program(),
        instance_id,
        chain_id: fx.chain_id,
        token: fx.transfer.token,
        recipient: fx.transfer.to,
    };
    let sid = before.statements[0].id;
    let n_old = consumer::nullifier(&pool(old.id).id(), &sid);
    let n_new = consumer::nullifier(&pool(new.id).id(), &sid);
    assert_ne!(n_old, n_new, "an instance-keyed namespace needs the predecessor's nullifiers carried over");
    println!(
        "replacement: statement {} attested under instance {} and its replacement {} with the same identifier; each \
         binding refused at the other's record ({wrong}); the old record still attests; reference-consumer nullifiers \
         {} (old pool) and {} (new pool) differ",
        hex::encode(sid),
        hex::encode(&old.id[..4]),
        hex::encode(&new.id[..4]),
        hex::encode(&n_old[..4]),
        hex::encode(&n_new[..4])
    );
}
