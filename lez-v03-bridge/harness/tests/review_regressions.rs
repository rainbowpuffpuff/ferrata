//! Regression tests of review-lez-v03-bridge (18 Nov 2026), through LEZ v0.3.1's block application; R1 and R2 are
//! the reviewer's tests (spike/tmp/review-lez-v03-bridge/tests/review_bridge.rs), turned around to show the fixes.
//!
//! R1. A configuration B of the SAME vault and chain whose light client is a program its creator controls (here the
//!     repository's own stand-in record writer, `guests/records`, which anyone can deploy) inits, imports the real
//!     root under a record it wrote, and claims a real deposit of configuration A with its witness. With nullifiers
//!     keyed by the configuration (`nullifier(cid, chain, vault, N)`), A's own claim of that deposit still APPLIES
//!     afterwards; B's mints stay in B's holdings and B's burns in `burns(cid_B)`, which no vault pins (the vault pins
//!     cid_A in slot 2), so B gains nothing the vault would pay.
//! R2. A destination's holdings are one account per configuration (`holder(cid, dest)`): another configuration's full
//!     account does not stop A's claim to the same destination; and a state over the shard limit (placed: no
//!     configuration can fill its own, at most two registered tokens) is refused with 255 `CAPPED`, not a panic.

use ferrata_bridge_core::{inbound::DestKind, sha256};
use ferrata_lez03_bridge_core::{code, header_account, holder_account, BridgeConfig, BridgeState, Denomination, HolderState, Holding, LcRecord, ETH};
use harness03::{
    account_of,
    deposits::{self, Deposit},
    eth::{vault_slots, Block},
    key, World, GAS_LIMIT,
};
use lee::{
    program::Program,
    public_transaction::{self as pt, PublicTransaction},
    Account, AccountId, FeeDeclaration, ProgramShardSelector, V03State, ValidatedStateDiff,
};
use lee_core::account::ShardData;

const CHAIN: u64 = 11_155_111;
const VAULT: [u8; 20] = [0xAA; 20];
const ETH_DENOMINATION: u128 = 1_000_000_000_000_000;

/// The stand-in's instruction (`guests/records/src/main.rs`, `Write`), the same borsh layout.
#[derive(borsh::BorshSerialize)]
struct Write {
    instance_id: [u8; 32],
    slot: u64,
    state: Vec<u8>,
}

/// A public transaction to the records program, signed and paid by the world's payer.
fn write_record(w: &mut World, records: AccountId, instance_id: [u8; 32], slot: u64, state: Vec<u8>) -> Result<(), String> {
    let payer = w.payer.clone();
    let nonces = vec![w.state.get_account_by_id(account_of(&payer)).nonce];
    let fee = FeeDeclaration::new(account_of(&payer), GAS_LIMIT, 0, 1_000_000_000_000);
    let at = header_account(records, &instance_id, slot);
    let message = pt::Message::try_new_with_fees(
        records,
        vec![ProgramShardSelector::new(at, records)],
        nonces,
        &Write { instance_id, slot, state },
        fee,
    )
    .unwrap();
    let ws = pt::WitnessSet::for_message(&message, &[&payer]);
    let tx = PublicTransaction::new(message, ws);
    let budget = lee::program::DEFAULT_PUBLIC_CYCLE_BUDGET;
    let out = ValidatedStateDiff::from_public_transaction_with_cycle_budget(&tx, &w.state, w.block, w.time, budget);
    w.block += 1;
    w.time += 1_000;
    match out {
        Ok((diff, _charge)) => {
            drop(w.state.apply_state_diff(diff));
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[test]
fn r1_a_rogue_configuration_no_longer_freezes_the_paired_configurations_deposit() {
    let mut w = World::new();
    // The attacker deploys the stand-in record writer (any program that writes chosen bytes would do).
    let records = AccountId::new(sha256(b"review-lez-v03-bridge/attacker-records"));
    let records_program = Program::new(methods03::FERRATA_RECORDS03_ELF.to_vec().into()).unwrap();
    w.state = V03State::new()
        .with_named_programs([(w.bridge, w.program.clone()), (records, records_program)])
        .with_public_account_balances([(account_of(&w.payer), 1_000_000_000_000_000)]);

    let registry = vec![Denomination { token: ETH, amount: ETH_DENOMINATION }];
    // A: the honest configuration (its light client places finalized records).
    let instance_a = sha256(b"review/instance-a");
    let config_a = w.config(CHAIN, VAULT, instance_a, registry.clone());
    let cid_a = w.config_id(&config_a);
    // B: the same bridge, chain, vault and registry; the attacker's record writer and an instance of its choosing.
    let instance_b = sha256(b"review/instance-b");
    let config_b = BridgeConfig { attestation: records, instance_id: instance_b, ..config_a.clone() };
    let cid_b = w.config_id(&config_b);
    assert_ne!(cid_a, cid_b);

    // The real vault: two deposits (deposit 0 public kind to alice), slot 2 = cid_A.
    let wallet = deposits::keys("review-lez-v03-bridge: victim seed");
    let alice = key(1);
    let alice_id = *account_of(&alice).value();
    let dep = |j: u64, dest: [u8; 32]| Deposit::new(&wallet, CHAIN, &VAULT, [0xD0; 20], j, j, ETH, ETH_DENOMINATION, DestKind::Public, dest);
    let real = vec![dep(0, alice_id), dep(1, alice_id)];
    let leaves: Vec<_> = real.iter().map(Deposit::leaf).collect();
    let real_root = deposits::root(&leaves, 2);

    // 1. The attacker's record: a block where the vault holds the REAL root and count, and slot 2 = cid_B.
    let fake = Block::new(300, 3000, VAULT, &vault_slots(real_root, 2, cid_b));
    let state_b = borsh::to_vec(&LcRecord { instance_id: instance_b, chain_id: CHAIN, record: fake.record.encode() }).unwrap();
    write_record(&mut w, records, instance_b, 300, state_b.clone()).expect("anyone writes a record with the stand-in");
    let r_b = ferrata_lez03_bridge_core::RecordRef { slot: 300, state: state_b };
    let t = w.init("B: Init (the attacker's record says slot 2 = cid_B)", &config_b, &r_b, fake.pairing_proof());
    assert!(t.applied(), "{}", t.verdict());
    let t = w.import_root("B: ImportRoot n = 2 (the real root, under the attacker's record)", &config_b, &r_b, real_root, 2, fake.import_proof());
    assert!(t.applied(), "{}", t.verdict());
    let root_b = w.root_state(&config_b, 2).unwrap();
    // 2. B claims the victim's deposit 0 with its witness (s known: a public seed, or a claim seen before inclusion).
    let d0 = real[0].witness(&leaves, 2);
    let t = w.claim("B: Claim of A's deposit 0 (credited to alice under cid_B)", &config_b, &root_b, &d0);
    assert!(t.applied(), "{}", t.verdict());
    assert_eq!(w.public_holding(&config_b, &alice_id, &ETH), ETH_DENOMINATION);

    // 3. The honest configuration now pairs, imports and claims deposit 0: refused, the nullifier is spent.
    let honest = Block::new(310, 3010, VAULT, &vault_slots(real_root, 2, cid_a));
    let r_a = w.place_record(instance_a, CHAIN, &honest.record);
    let t = w.init("A: Init (the real vault's slot 2 = cid_A)", &config_a, &r_a, honest.pairing_proof());
    assert!(t.applied(), "{}", t.verdict());
    let t = w.import_root("A: ImportRoot n = 2", &config_a, &r_a, real_root, 2, honest.import_proof());
    assert!(t.applied(), "{}", t.verdict());
    let root_a = w.root_state(&config_a, 2).unwrap();
    let t = w.claim("A: Claim of deposit 0 by its owner (after B's)", &config_a, &root_a, &d0);
    assert!(t.applied(), "the paired configuration's claim applies: {}", t.verdict());
    assert_eq!(w.public_holding(&config_a, &alice_id, &ETH), ETH_DENOMINATION, "alice holds it under A");
    assert_ne!(holder_account(w.bridge, &cid_a, &alice_id), holder_account(w.bridge, &cid_b, &alice_id));
    let t = w.claim("A: the same claim again (replay, refused)", &config_a, &root_a, &d0);
    assert_eq!(t.code, Some(u32::from(code::ALREADY_CLAIMED)), "{}", t.verdict());
    // Deposit 1, not squatted, still claims under A.
    let t = w.claim("A: Claim of deposit 1", &config_a, &root_a, &real[1].witness(&leaves, 2));
    assert!(t.applied(), "{}", t.verdict());

    // 4. B mints a deposit that never happened (leaf 2 to mallory) and burns it.
    let mallory = key(9);
    let mallory_id = *account_of(&mallory).value();
    let invented = Deposit::new(&wallet, CHAIN, &VAULT, [0xBB; 20], 7, 2, ETH, ETH_DENOMINATION, DestKind::Public, mallory_id);
    let mut leaves3 = leaves.clone();
    leaves3.push(invented.leaf());
    let fake3 = Block::new(301, 3001, VAULT, &vault_slots(deposits::root(&leaves3, 3), 3, cid_b));
    let state3 = borsh::to_vec(&LcRecord { instance_id: instance_b, chain_id: CHAIN, record: fake3.record.encode() }).unwrap();
    write_record(&mut w, records, instance_b, 301, state3.clone()).unwrap();
    let r3 = ferrata_lez03_bridge_core::RecordRef { slot: 301, state: state3 };
    let t = w.import_root("B: ImportRoot n = 3 (an invented leaf)", &config_b, &r3, deposits::root(&leaves3, 3), 3, fake3.import_proof());
    assert!(t.applied(), "{}", t.verdict());
    let root_b3 = w.root_state(&config_b, 3).unwrap();
    let t = w.claim("B: Claim of the invented deposit (to mallory)", &config_b, &root_b3, &invented.witness(&leaves3, 3));
    assert!(t.applied(), "{}", t.verdict());
    let note = deposits::note(&deposits::keys("mallory"), &cid_b, 0, CHAIN, VAULT, [0xEE; 20], ETH, ETH_DENOMINATION);
    let t = w.burn_public("B: Burn by mallory (a leaf in burns(cid_B))", &config_b, &note, &mallory);
    assert!(t.applied(), "{}", t.verdict());
    assert_eq!(w.burn_tree(&config_b).unwrap().count, 1);
    assert!(w.burn_tree(&config_a).is_none(), "nothing in A's burn tree");
    // What the vault would pay: it pins cid_A (slot 2 of the real vault), so only burns(cid_A) can ever back a
    // release; B's tree and B's holdings are B's alone.
    assert_ne!(cid_a, cid_b);
    // A's own burn lands in A's tree only.
    let note_a = deposits::note(&deposits::keys("alice"), &cid_a, 0, CHAIN, VAULT, [0xAB; 20], ETH, ETH_DENOMINATION);
    let t = w.burn_public("A: Burn by alice (a leaf in burns(cid_A))", &config_a, &note_a, &alice);
    assert!(t.applied(), "{}", t.verdict());
    assert_eq!(w.burn_tree(&config_a).unwrap().count, 1);
    assert_eq!(w.burn_tree(&config_b).unwrap().count, 1);

    println!("R1 cid_A {} cid_B {}", hex::encode(cid_a), hex::encode(cid_b));
    for (i, r) in w.reports.iter().enumerate() {
        println!("R1 | {} | {} | {} | {} |", i + 1, r.label, r.verdict(), r.gas_exec);
    }
}

/// R2, turned around: holdings are one account per configuration and destination.
#[test]
fn r2_holdings_are_per_configuration_and_a_full_account_is_a_refusal() {
    let mut w = World::new();
    let registry = vec![Denomination { token: ETH, amount: ETH_DENOMINATION }];
    let instance = sha256(b"review/instance-a");
    let config = w.config(CHAIN, VAULT, instance, registry.clone());
    let cid = w.config_id(&config);
    let other = w.config(CHAIN, VAULT, sha256(b"review/instance-b"), registry);
    let cid_other = w.config_id(&other);
    let wallet = deposits::keys("review-lez-v03-bridge: victim seed");
    let alice = key(1);
    let alice_id = *account_of(&alice).value();
    let full = |n: u32| {
        let mut h = HolderState::default();
        for i in 0..n {
            h.holdings.push(Holding { config_id: sha256(&i.to_le_bytes()), token: ETH, amount: 1 });
        }
        BridgeState::Holder(h).encode()
    };
    // Another configuration's account for alice, as full as a shard allows (1,505 holdings: 102,345 bytes).
    let bytes = full(1505);
    println!("R2 holder bytes {} (limit 102400)", bytes.len());
    w.state.force_insert_account(
        holder_account(w.bridge, &cid_other, &alice_id),
        Account::default().with_shard(w.bridge, ShardData::try_from(bytes).expect("1,505 holdings fit")),
    );
    let d0 = Deposit::new(&wallet, CHAIN, &VAULT, [0xD0; 20], 0, 0, ETH, ETH_DENOMINATION, DestKind::Public, alice_id);
    let d1 = Deposit::new(&wallet, CHAIN, &VAULT, [0xD0; 20], 1, 1, ETH, ETH_DENOMINATION, DestKind::Public, alice_id);
    let leaves = vec![d0.leaf(), d1.leaf()];
    let b = Block::new(400, 4000, VAULT, &vault_slots(deposits::root(&leaves, 2), 2, cid));
    let r = w.place_record(instance, CHAIN, &b.record);
    assert!(w.init("Init", &config, &r, b.pairing_proof()).applied());
    assert!(w.import_root("ImportRoot n = 2", &config, &r, deposits::root(&leaves, 2), 2, b.import_proof()).applied());
    let root = w.root_state(&config, 2).unwrap();
    let t = w.claim("Claim of deposit 0 to alice (another configuration's account for alice is full)", &config, &root, &d0.witness(&leaves, 2));
    assert!(t.applied(), "{}", t.verdict());
    assert_eq!(w.public_holding(&config, &alice_id, &ETH), ETH_DENOMINATION);
    // A's own account for alice made full (unreachable: it only ever holds A's registered tokens, at most two), then a
    // claim: refused with a code and charged its cycles, not a panic charged the whole gas limit.
    let bytes = full(1505);
    w.state.force_insert_account(
        holder_account(w.bridge, &cid, &alice_id),
        Account::default().with_shard(w.bridge, ShardData::try_from(bytes).unwrap()),
    );
    let t = w.claim("Claim of deposit 1 to alice (A's account for alice placed full)", &config, &root, &d1.witness(&leaves, 2));
    println!("R2 | claim to a full holder | {} | gas {}", t.verdict(), t.gas_exec);
    assert_eq!(t.code, Some(u32::from(code::CAPPED)), "{}", t.verdict());
    assert!(t.gas_exec < GAS_LIMIT, "a refusal is charged its cycles, not the whole limit");
    for (i, r) in w.reports.iter().enumerate() {
        println!("R2 | {} | {} | {} | {} |", i + 1, r.label, r.verdict(), r.gas_exec);
    }
}
