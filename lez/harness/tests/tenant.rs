//! The malicious-tenant tests (RFP-022 F.16, the B2 gate's test; milestone B1's regression tests).
//!
//! F.16 asks that configurations of the bridge be strictly separated: one entity's configuration has no privileged
//! access over another's configuration or funds, as if each were a fully separate deployment. An independent review of
//! the bridge's LEZ v0.3.1 port found two ways in which the program before B1 (image `0xccde6171…71dd`, which the live
//! runs of November 2026 deployed) was not; `lez/results/tenant.md` records them against that program (run at commit
//! `f13faee2`). B1 keys the nullifier and holder accounts by the configuration id (`nullifier(cid, chain, vault, N)`,
//! `holder(cid, account)`, `ferrata_lez_bridge_wire`'s module docs). These tests run the same attacks through LEZ
//! v0.4.0-rc1's own state machine and fee settlement against the program of `guests/bridge`, and assert that they fail:
//!
//! * R1. A configuration's attestation program and instance are free fields (`BridgeConfig`), so anyone can make a
//!   second configuration of a vault that reads Ethereum through an instance its creator controls. Its claim of a
//!   public-kind deposit whose `s` it knows now spends `nullifier(B, …)` only: the paired configuration's claim of the
//!   deposit applies afterwards, and a copied plaintext claim ordered first no longer blocks its owner's.
//! * R2. Other configurations' holdings for a destination sit in their own `holder(cid, dest)` accounts: a claim to
//!   the destination and its owner's burn cost the same with none of them and with 12,098 of them.
//! * Separation. Every privileged message one configuration's roles can send (freezes, caps, the inbound depth, role
//!   proposals, pauses, `Sunset`, multisig approvals) is refused at another configuration's accounts, and every
//!   internal message one configuration's accounts can send (mints, credits, root and token writes, fee extensions,
//!   listing updates) is refused at another's: `separation_*` below.
//!
//! The rogue configuration's records. Anyone can create an instance of the real attestation program (`Init` takes
//! any checkpoint with a bootstrap that matches it, `lez/guests/program/src/main.rs`, `init`), and that instance
//! then records whatever its creator's sync committee signs. Here the attacker's records are placed at
//! `header(X, slot)` as the attestation program writes them, as this harness places every record of the honest
//! instance (`World::add_block`); everything from the rogue configuration's `Init` on is executed.
//!
//! Re-run: `RISC0_DEV_MODE=1 cargo test -p harness --locked --test tenant -- --nocapture --test-threads 1` (in `lez/`).
//! Each test writes its figures to `lez/results/tenant/b1/`.

use std::fmt::Write as _;

use ferrata_bridge_core::inbound::DestKind;
use ferrata_lez_bridge_wire::{
    burn_actor, code, config_actor, config_seed, own_actor, register_slots, root_actor, token_actor, word_of_u128,
    BridgeConfig, BridgeMessage, BridgeState, BurnRecord, CapParams, Controls, CosignedAction, FreezeAuthority,
    HolderState, Holding, Note, NullifierState, RootState,
};
use ferrata_lez_wire::{header_actor, RecordState};
use harness::{
    bridge::{
        code_name, controls, eth_entry, eth_schedule_one, holder_of, nullifier_of, registry_slots, relayer, render,
        role_account, role_key, schedule_zero, Block, Deposit, Vault, VaultSlots, World, CHAIN_ID, DENOMINATIONS, ETH,
        FLOOR, OUTSIDER_SEED,
    },
    ferrata_account, image_id_hex, TxReport,
};
use lee::{Account, PublicAccountEvidence};
use lee_core::account::ActorState;

/// The bridge program before B1 (`docs/image-ids.md`, the live runs' program): these tests must not run it.
const BEFORE_B1: &str = "ccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd";
/// The attacker's instance of the attestation program.
const ROGUE_INSTANCE: [u8; 32] = [0xbe; 32];
/// LEZ v0.4's execution gas limit of a transaction.
const GAS_LIMIT: u64 = fee_core::market::MAX_GAS_EXEC;

fn verdict(r: &TxReport) -> String {
    match (r.ok(), r.exit_code) {
        (true, _) => "applied".into(),
        (false, Some(c)) => format!("refused, exit {c} ({})", code_name(c).unwrap_or("not a bridge code")),
        (false, None) => format!("failed, no exit code: {}", r.charge.revert.as_deref().unwrap_or("?")),
    }
}

/// Check the outcome, keep the report.
fn step(log: &mut Vec<TxReport>, r: TxReport, want: Option<u8>) -> TxReport {
    match want {
        None => assert!(r.ok(), "{}: {}", r.label, verdict(&r)),
        Some(c) => {
            assert!(!r.ok(), "{}: applied, expected {c}", r.label);
            assert_eq!(r.exit_code, Some(u32::from(c)), "{}: {}", r.label, verdict(&r));
        }
    }
    log.push(r.clone());
    r
}

fn line(r: &TxReport) -> String {
    format!(
        "| {} | {} | {} | {} | {} |\n",
        r.label,
        verdict(r),
        r.charge.gas_exec,
        r.charge.gas_stor,
        r.charge.fee_base
    )
}

fn table(reports: &[TxReport]) -> String {
    let mut out =
        String::from("| transaction | outcome | gas_exec | gas_stor | fee_base |\n|---|---|---:|---:|---:|\n");
    for r in reports {
        out.push_str(&line(r));
    }
    out
}

fn save(name: &str, text: &str) {
    let dir = harness::results_dir().join("tenant").join("b1");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), text).unwrap();
    println!("{text}");
}

/// The attacker's record at `header(ROGUE_INSTANCE, slot)` of the real attestation program, as that program writes
/// the records of an instance (`RecordState`).
fn place_rogue_record(w: &mut World, b: &Block) {
    let state = RecordState {
        instance_id: ROGUE_INSTANCE,
        chain_id: CHAIN_ID,
        record: b.record.encode(),
        root_id: ROGUE_INSTANCE,
    };
    let actor = header_actor(ferrata_account(), &ROGUE_INSTANCE, b.slot);
    let account =
        Account::default().with_actor_state(ferrata_account(), ActorState::from(borsh::to_vec(&state).unwrap()));
    w.chain.state.force_insert_account(actor.account_id, account);
}

fn rogue_init(w: &mut World, label: &str, config: &BridgeConfig, b: &Block) -> TxReport {
    let cid = config.id(w.program);
    let message =
        BridgeMessage::Init { config: config.clone(), record_slot: b.slot, proof: b.pairing_proof(&config.vault) };
    let at = config_actor(w.program, &cid);
    let header = header_actor(config.attestation, &config.instance_id, b.slot);
    let evidence = vec![PublicAccountEvidence::Pda { program: w.program, seed: config_seed(&cid) }];
    w.submit(label, at, &message, vec![at, header], evidence)
}

fn rogue_register(w: &mut World, label: &str, config: &BridgeConfig, b: &Block) -> TxReport {
    let cid = config.id(w.program);
    let entry = eth_entry();
    let message = BridgeMessage::Register {
        config_id: cid,
        entry: entry.clone(),
        schedule: schedule_zero(&entry),
        record_slot: b.slot,
        proof: b.vault_proof(&config.vault, &register_slots(&entry.token)),
    };
    let at = config_actor(w.program, &cid);
    let header = header_actor(config.attestation, &config.instance_id, b.slot);
    w.submit(label, at, &message, vec![at, header, token_actor(w.program, &cid, &entry.token)], vec![])
}

fn rogue_import(w: &mut World, label: &str, config: &BridgeConfig, b: &Block, root: [u8; 32], n: u64) -> TxReport {
    let cid = config.id(w.program);
    let message = BridgeMessage::ImportRoot {
        config_id: cid,
        record_slot: b.slot,
        root,
        leaf_count: n,
        proof: b.import_proof(&config.vault),
    };
    let at = config_actor(w.program, &cid);
    let header = header_actor(config.attestation, &config.instance_id, b.slot);
    w.submit(label, at, &message, vec![at, header, root_actor(w.program, &cid, n)], vec![])
}

/// The rogue configuration B: configuration A with another instance (the attacker's). Its record says that vault A
/// holds the root and count of A's eight real leaves plus `invented` (a deposit that never happened), that slot 2
/// holds configId(B), and ETH's registry entry. B pairs, registers ETH and imports that root (n = 9).
fn rogue(w: &mut World, log: &mut Vec<TxReport>, invented: &Deposit) -> (BridgeConfig, [u8; 32], Vault) {
    let b = BridgeConfig { instance_id: ROGUE_INSTANCE, ..w.config_a.clone() };
    let cid_b = b.id(w.program);
    assert_ne!(cid_b, w.cid_a);
    let mut leaves = w.vault_a.leaves.clone();
    assert_eq!(leaves.len() as u64, invented.leaf_index);
    leaves.push(invented.leaf());
    let fake = Vault { address: w.vault_a.address, leaves };
    let slots = VaultSlots {
        address: fake.address,
        root: fake.root(9),
        count: 9,
        config_id: cid_b,
        extra: registry_slots(&eth_entry(), &[]),
    };
    let forged = Block::new(9_100_000, 300, &[slots]);
    place_rogue_record(w, &forged);
    step(log, rogue_init(w, "B: Init (the attacker's record: slot 2 = configId(B))", &b, &forged), None);
    step(log, rogue_register(w, "B: Register ETH (the attacker's record)", &b, &forged), None);
    let root9 = fake.root(9);
    step(log, rogue_import(w, "B: ImportRoot n = 9 (A's 8 real leaves + 1 invented)", &b, &forged, root9, 9), None);
    (b, cid_b, fake)
}

/// The honest configuration A, paired with vault A (the harness's records of the honest instance): Init, ETH,
/// root(A, 8), fee schedule 1 from leaf 6 and the range closed at 8.
fn pair_honest(w: &mut World, log: &mut Vec<TxReport>) {
    let (a, cid_a, va) = (w.config_a.clone(), w.cid_a, w.vault_a.address);
    let root8 = w.vault_a.root(8);
    step(log, w.init("A: Init (the vault's slot 2 = configId(A))", &a, 0), None);
    step(log, w.register("A: Register ETH", &cid_a, 0, &eth_entry(), &va), None);
    step(log, w.import("A: ImportRoot n = 8", &cid_a, 1, root8, 8, &va), None);
    let fees = w.import_fees("A: ImportFees ETH schedule 1", &cid_a, 1, ETH, 1, Some(eth_schedule_one()), &va);
    step(log, fees, None);
    step(log, w.import_fees("A: ImportFees ETH, no schedule 2", &cid_a, 1, ETH, 2, None, &va), None);
}

/// A deposit that never happened: leaf 8 of vault A, 1 ETH, public kind, to the world's payer.
fn invented(w: &World) -> Deposit {
    Deposit {
        leaf_index: 8,
        depositor: [0xbb; 20],
        token: ETH,
        amount: word_of_u128(DENOMINATIONS[1]),
        s: [0x5b; 32],
        kind: DestKind::Public,
        dest: *w.chain.payer.value(),
    }
}

/// `k` holdings of other configurations (as an attacker's claims under its own configurations leave them).
fn fillers(k: usize) -> Vec<Holding> {
    (0..k as u64)
        .map(|i| {
            let mut config_id = [0xf1; 32];
            config_id[..8].copy_from_slice(&i.to_le_bytes());
            Holding { config_id, token: ETH, amount: DENOMINATIONS[0] }
        })
        .collect()
}

/// `holdings` placed at `account`'s holder of configuration `cid`, `holder(cid, account)`.
fn place_holder(w: &mut World, cid: &[u8; 32], account: lee::AccountId, holdings: Vec<Holding>) -> usize {
    let bytes = borsh::to_vec(&BridgeState::Holder(HolderState { holdings, pending: None })).unwrap();
    let id = holder_of(w.program, cid, account).account_id;
    let size = bytes.len();
    w.chain.state.force_insert_account(id, Account::default().with_actor_state(w.program, ActorState::from(bytes)));
    size
}

struct Probe {
    k: usize,
    bytes: usize,
    report: TxReport,
    /// Cycles of the transitions at the holder (the `Mint` of a claim; the debit of a burn).
    at_holder: u64,
}

fn holder_cycles(w: &World, cid: &[u8; 32], r: &TxReport, account: lee::AccountId) -> u64 {
    let holder = holder_of(w.program, cid, account);
    r.transitions.iter().filter(|t| t.receiver == holder).map(|t| t.cycles).sum()
}

/// A claim (A's leaf 2, public kind, 1 ETH) to `dest`, after `place(world, cid_a, dest, k)` has placed `k` holdings
/// of other configurations for `dest` (returning the bytes placed).
fn claim_with_others(k: usize, place: fn(&mut World, &[u8; 32], lee::AccountId, usize) -> usize) -> Probe {
    let mut w = World::new();
    let mut log = Vec::new();
    let (a, cid_a, va) = (w.config_a.clone(), w.cid_a, w.vault_a.address);
    step(&mut log, w.init("A: Init", &a, 0), None);
    step(&mut log, w.register("A: Register ETH", &cid_a, 0, &eth_entry(), &va), None);
    step(&mut log, w.import("A: ImportRoot n = 5", &cid_a, 0, w.vault_a.root(5), 5, &va), None);
    let dest = w.vector_dest;
    let bytes = place(&mut w, &cid_a, dest, k);
    let witness = w.vault_a.witness(&w.vector.clone(), 5);
    let label = format!("A: Claim to dest, {k} other configurations' holdings for it (each at its own holder(cid, dest))");
    let report = w.claim(&label, &cid_a, 5, &va, witness);
    if report.ok() {
        assert_eq!(w.balance(dest, &cid_a), DENOMINATIONS[1]);
    } else {
        assert_eq!(w.balance(dest, &cid_a), 0);
        assert_eq!(w.nullifier(&cid_a, &va, &w.vector_n), None, "the deposit is not spent");
    }
    let at_holder = holder_cycles(&w, &cid_a, &report, dest);
    Probe { k, bytes, report, at_holder }
}

/// The owner's public burn of 1 ETH it holds under A (claimed: leaf 6), after `place(world, cid_a, owner, k)` has
/// placed `k` holdings of other configurations for the owner (returning the bytes placed).
fn burn_with_others(k: usize, place: fn(&mut World, &[u8; 32], lee::AccountId, usize) -> usize) -> Probe {
    let mut w = World::new();
    let mut log = Vec::new();
    pair_honest(&mut w, &mut log);
    let (cid_a, va, payer) = (w.cid_a, w.vault_a.address, w.chain.payer);
    let leaf6 = w.vault_a.witness(&w.payer_deposit.clone(), 8);
    step(&mut log, w.claim("A: Claim of leaf 6 (10 ETH to the payer)", &cid_a, 8, &va, leaf6), None);
    let bytes = place(&mut w, &cid_a, payer, k);
    let note = Note {
        r: [0x7e; 32],
        memo: [0x7d; 20],
        mac: [0x7c; 16],
        token: ETH,
        amount: word_of_u128(DENOMINATIONS[1]),
        c_out: [0x7b; 32],
    };
    let actors = w.public_burn_actors(payer, &cid_a, &note);
    let own = own_actor(w.program, payer);
    let label = format!("A: the owner's Burn of 1 ETH, {k} other configurations' holdings for it (each at its own holder)");
    let report = w.send(&label, own, &BridgeMessage::burn(cid_a, &note), actors);
    let left = if report.ok() { DENOMINATIONS[2] - DENOMINATIONS[1] } else { DENOMINATIONS[2] };
    assert_eq!(w.balance(payer, &cid_a), left);
    let at_holder = holder_cycles(&w, &cid_a, &report, payer);
    Probe { k, bytes, report, at_holder }
}

fn probe_line(p: &Probe) -> String {
    format!(
        "| {} | {} | {} | {} | {} | {} |\n",
        p.k,
        p.bytes,
        verdict(&p.report),
        p.report.charge.gas_exec,
        p.report.charge.fee_base,
        p.at_holder
    )
}

// ---------------------------------------------------------------------------------------------------------------------
// The program of guests/bridge since B1: nullifier(cid, chain, vault, N) and holder(cid, account)
// ---------------------------------------------------------------------------------------------------------------------

/// The image id of the program these tests run (written into every result), which is not the program before B1.
fn b1_program() -> String {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
    let id = image_id_hex(harness::bridge::BRIDGE_ID);
    assert_ne!(id, BEFORE_B1, "the program before B1");
    id
}

/// R1: the rogue configuration B's claim of A's deposit applies in B's namespace only
/// (`nullifier(B, …)`, `holder(B, dest)`), and A's claim of it then applies. Within A the deposit still mints once.
#[test]
fn r1_the_paired_configuration_claims_after_a_rogue_one() {
    let image = b1_program();
    let mut w = World::new();
    let mut log = Vec::new();
    let program = w.program;
    let va = w.vault_a.address;
    let cid_a = w.cid_a;
    let payer = w.chain.payer;
    let dest = w.vector_dest;
    let made_up = invented(&w);
    let (_, cid_b, fake) = rogue(&mut w, &mut log, &made_up);

    // B claims A's public-kind deposit (leaf 2, 1 ETH) with its s: applied, under B only.
    let vector = w.vector.clone();
    let n = w.vector_n;
    let squat = fake.witness(&vector, 9);
    step(&mut log, w.claim("B: Claim of A's deposit (leaf 2, public kind) with its s", &cid_b, 9, &va, squat), None);
    let spent_b = NullifierState { config_id: cid_b, token: ETH, leaf_count: 9, amount: DENOMINATIONS[1] };
    assert_eq!(w.nullifier(&cid_b, &va, &n), Some(spent_b), "nullifier(B, chain, vault, N) spent");
    assert_eq!(w.nullifier(&cid_a, &va, &n), None, "nullifier(A, chain, vault, N) untouched");
    let under_b = vec![Holding { config_id: cid_b, token: ETH, amount: DENOMINATIONS[1] }];
    assert_eq!(w.holder(&cid_b, dest).unwrap().holdings, under_b, "holder(B, dest)");
    assert_eq!(w.holder(&cid_a, dest), None, "holder(A, dest) untouched");

    // A pairs and claims the same deposit: applied. Claimed again under A: refused, once per configuration.
    pair_honest(&mut w, &mut log);
    let witness = w.vault_a.witness(&vector, 8);
    let label = "A: Claim of the same deposit (leaf 2) by its owner";
    let paired = step(&mut log, w.claim(label, &cid_a, 8, &va, witness.clone()), None);
    let spent_a = NullifierState { config_id: cid_a, token: ETH, leaf_count: 8, amount: DENOMINATIONS[1] };
    assert_eq!(w.nullifier(&cid_a, &va, &n), Some(spent_a), "nullifier(A, chain, vault, N) spent under A");
    let under_a = vec![Holding { config_id: cid_a, token: ETH, amount: DENOMINATIONS[1] }];
    assert_eq!(w.holder(&cid_a, dest).unwrap().holdings, under_a, "holder(A, dest): 1 ETH under A");
    assert_eq!(w.holder(&cid_b, dest).unwrap().holdings, under_b, "holder(B, dest) unchanged");
    let again = w.claim("A: the same claim again", &cid_a, 8, &va, witness);
    let again = step(&mut log, again, Some(code::ALREADY_CLAIMED));
    assert_eq!(w.balance(dest, &cid_a), DENOMINATIONS[1], "minted once under A");
    let leaf6 = w.vault_a.witness(&w.payer_deposit.clone(), 8);
    step(&mut log, w.claim("A: Claim of A's other deposit (leaf 6, 10 ETH)", &cid_a, 8, &va, leaf6), None);
    assert_eq!(w.balance(payer, &cid_a), DENOMINATIONS[2]);

    // B mints the invented deposit and the payer burns it: the note is recorded under configId(B) only, which no
    // vault pins.
    let made = fake.witness(&made_up, 9);
    step(
        &mut log,
        w.claim("B: Claim of the invented deposit (leaf 8, 1 ETH to the payer)", &cid_b, 9, &va, made),
        None,
    );
    assert_eq!(w.balance(payer, &cid_b), DENOMINATIONS[1]);
    let note = Note {
        r: [0x6e; 32],
        memo: [0x6d; 20],
        mac: [0x6c; 16],
        token: ETH,
        amount: word_of_u128(DENOMINATIONS[1]),
        c_out: [0x6b; 32],
    };
    let own = own_actor(program, payer);
    let actors = w.public_burn_actors(payer, &cid_b, &note);
    let burn = w.send("B: Burn of the invented 1 ETH by the payer", own, &BridgeMessage::burn(cid_b, &note), actors);
    step(&mut log, burn, None);
    let record = w.chain.actor_state(burn_actor(program, &cid_b, &note.hash()));
    assert_eq!(
        borsh::from_slice::<BridgeState>(&record).unwrap(),
        BridgeState::Burn(BurnRecord { config_id: cid_b, note })
    );
    assert!(w.chain.actor_state(burn_actor(program, &cid_a, &note.hash())).is_empty(), "nothing under configId(A)");
    assert_eq!(w.balance(payer, &cid_a), DENOMINATIONS[2], "A's holding untouched");
    assert_eq!(w.balance(payer, &cid_b), 0, "B's 1 ETH burnt");

    // A private-kind deposit (leaf 5): as before B1, B cannot take it.
    let private = w.private_deposit.clone();
    let n5 = private.nullifier(CHAIN_ID, &va);
    let mut guess = fake.witness(&private, 9);
    guess.s = [0x42; 32];
    let r = w.claim("B: Claim of A's private-kind deposit (leaf 5), s guessed", &cid_b, 9, &va, guess.clone());
    step(&mut log, r, Some(code::CLAIM_INVALID));
    let mut as_public = guess;
    as_public.kind = DestKind::Public as u8;
    as_public.dest = *payer.value();
    let r = w.claim("B: the same as a public-kind deposit to the attacker, s guessed", &cid_b, 9, &va, as_public);
    step(&mut log, r, Some(code::CLAIM_INVALID));
    let leaked = fake.witness(&private, 9);
    let r = w.claim("B: plaintext Claim of leaf 5 with its real s", &cid_b, 9, &va, leaked.clone());
    step(&mut log, r, Some(code::WRONG_DESTINATION));
    let outsider = role_account(OUTSIDER_SEED);
    let at = own_actor(program, outsider);
    let message = w.claim_privately(&cid_b, 9, leaked);
    let mut actors = vec![at];
    actors.extend(w.private_claim_actors(&cid_b, 9, &va, &private));
    let label = "B: ClaimPrivately of leaf 5 with its real s, run as the attacker's own account";
    let r = w.submit_signed(label, at, &message, actors, vec![], &[&role_key(OUTSIDER_SEED)]);
    step(&mut log, r, Some(code::WRONG_DESTINATION));
    assert_eq!(w.nullifier(&cid_a, &va, &n5), None, "the private-kind deposit's nullifier under A is unspent");
    assert_eq!(w.nullifier(&cid_b, &va, &n5), None, "and under B");

    let mut out = format!(
        "# R1: a rogue configuration of the same vault and chain (LEZ v0.4.0-rc1 state machine, bridge program \
         {image})\n\n\
         configId(A) {}\nconfigId(B) {} (A's fields, instance {})\n\n",
        hex::encode(cid_a),
        hex::encode(cid_b),
        hex::encode(ROGUE_INSTANCE)
    );
    out.push_str(&table(&log));
    let _ = write!(
        out,
        "\nA's claim after B's: applied, gas_exec {}, fee_base {}; the same claim again: exit {:?}, gas_exec {}\n\
         holder(A, dest of leaf 2): {:?}\nholder(B, dest of leaf 2): {:?}\n\
         nullifier(A, chain, vault, N of leaf 2): {:?}\nnullifier(B, chain, vault, N of leaf 2): {:?}\n\
         burn(configId(B), H(note)): written; burn(configId(A), H(note)): empty\n\
         nullifier of the private-kind deposit (leaf 5): unspent under A and under B\n\n{}",
        paired.charge.gas_exec,
        paired.charge.fee_base,
        again.exit_code,
        again.charge.gas_exec,
        w.holder(&cid_a, dest).unwrap().holdings,
        w.holder(&cid_b, dest).unwrap().holdings,
        w.nullifier(&cid_a, &va, &n),
        w.nullifier(&cid_b, &va, &n),
        render(&log)
    );
    save("r1.txt", &out);
}

/// The front-run: B's copy of a pending plaintext claim, ordered first, applies under B;
/// the owner's own claim then applies under A.
#[test]
fn r1_a_copied_plaintext_claim_does_not_block_the_owner() {
    let image = b1_program();
    let mut w = World::new();
    let mut log = Vec::new();
    let va = w.vault_a.address;
    let cid_a = w.cid_a;
    let made_up = invented(&w);
    let (_, cid_b, _) = rogue(&mut w, &mut log, &made_up);
    pair_honest(&mut w, &mut log);

    let deposit = w.payer_deposit.clone();
    let honest = BridgeMessage::Claim {
        config_id: cid_a,
        leaf_count: 8,
        witness: w.vault_a.witness(&deposit, 8),
        range: w.range(&cid_a, &ETH, deposit.leaf_index),
        relayer: Some(*relayer().value()),
    };
    let seen = borsh::to_vec(&honest).unwrap();
    assert!(seen.windows(32).any(|x| x == &deposit.s[..]), "s is in the plaintext claim's bytes");
    let BridgeMessage::Claim { witness: copied, .. } = borsh::from_slice::<BridgeMessage>(&seen).unwrap() else {
        unreachable!()
    };
    let mut fake_leaves = w.vault_a.leaves.clone();
    fake_leaves.push(made_up.leaf());
    let fake = Vault { address: va, leaves: fake_leaves };
    let for_b = fake.witness(&Deposit::of(&copied), 9);
    let r = w.claim("B: Claim with the witness copied from the pending claim, ordered first", &cid_b, 9, &va, for_b);
    step(&mut log, r, None);
    let BridgeMessage::Claim { config_id, leaf_count, witness, range, relayer } = honest else { unreachable!() };
    let r = w.claim_with(
        "A: the payer's own claim, applied after it",
        &config_id,
        leaf_count,
        &va,
        witness,
        range,
        relayer,
    );
    let r = step(&mut log, r, None);
    assert_eq!(w.balance(w.chain.payer, &cid_a), DENOMINATIONS[2], "the payer holds its 10 ETH under A");
    assert_eq!(w.balance(w.chain.payer, &cid_b), DENOMINATIONS[2], "and B's copy under B, which no vault pays");

    let mut out = String::from(
        "# R1: front-run of a plaintext claim (one transaction per block, in submission order)\n\n",
    );
    let _ = writeln!(out, "bridge program {image}\n");
    out.push_str(&table(&log));
    let _ = writeln!(
        out,
        "\nthe honest claim: {} bytes as a message, s at a fixed offset; applied after B's copy, gas_exec {}, fee_base {}",
        seen.len(),
        r.charge.gas_exec,
        r.charge.fee_base
    );
    save("r1-frontrun.txt", &out);
}

/// `k` holdings of other configurations for `account` as the program leaves them: each at its own
/// `holder(cid, account)`. Returns the bytes placed, in all.
fn place_others(w: &mut World, _cid_a: &[u8; 32], account: lee::AccountId, k: usize) -> usize {
    fillers(k)
        .into_iter()
        .map(|h| {
            let cid = h.config_id;
            place_holder(w, &cid, account, vec![h])
        })
        .sum()
}

/// R2: other configurations' holdings for a destination sit in other accounts, so a claim
/// to it and its owner's burn cost the same with none of them and with many. And `holder(A, dest)` takes credits
/// under A only (the executor, as `bridge_secfix.rs` checks C-1).
#[test]
fn r2_other_configurations_holdings_cost_a_claim_nothing() {
    use lee_core::program::ReceiveInput;

    let image = b1_program();
    // 6,049: where a claim failed before B1; 12,098: twice that.
    let claims: Vec<Probe> =
        [0, 1_505, 6_049, 12_098].into_iter().map(|k| claim_with_others(k, place_others)).collect();
    let burns: Vec<Probe> = [0, 6_110, 12_098].into_iter().map(|k| burn_with_others(k, place_others)).collect();
    for probes in [&claims, &burns] {
        for p in probes.iter() {
            assert!(p.report.ok(), "{}: {}", p.report.label, verdict(&p.report));
            assert!(p.report.charge.gas_exec < GAS_LIMIT);
            assert_eq!(
                p.report.charge.gas_exec, probes[0].report.charge.gas_exec,
                "{}: the same gas as with none",
                p.k
            );
            assert_eq!(p.report.charge.fee_base, probes[0].report.charge.fee_base, "{}: the same fee", p.k);
            assert_eq!(p.at_holder, probes[0].at_holder, "{}: the same cycles at holder(A, ·)", p.k);
        }
    }

    // holder(A, dest) accepts a credit only under A: from nullifier(A, …) (Mint) or token(A, token) (Credit).
    let w = World::new();
    let (program, cid_a, dest, vault, n) = (w.program, w.cid_a, w.vector_dest, w.vault_a.address, w.vector_n);
    let cid_b = BridgeConfig { instance_id: ROGUE_INSTANCE, ..w.config_a.clone() }.id(program);
    let at = |cid: &[u8; 32]| holder_of(program, cid, dest);
    let mint = |cid: &[u8; 32]| BridgeMessage::Mint {
        nullifier: n,
        config_id: *cid,
        evm_chain_id: CHAIN_ID,
        vault,
        token: ETH,
        holder: *dest.value(),
        amount: DENOMINATIONS[0],
    };
    let credit = |cid: &[u8; 32]| BridgeMessage::Credit {
        config_id: *cid,
        token: ETH,
        amount: DENOMINATIONS[0],
        to: *dest.value(),
    };
    let from_n = |cid: &[u8; 32]| harness::bridge::nullifier_of(program, cid, CHAIN_ID, &vault, &n);
    let wrong = u32::from(code::WRONG_SENDER);
    let cases = [
        (
            "Mint under B from nullifier(B, …), delivered at holder(A, dest)",
            at(&cid_a),
            from_n(&cid_b),
            mint(&cid_b),
            wrong,
        ),
        (
            "Mint under A from nullifier(B, …), delivered at holder(A, dest)",
            at(&cid_a),
            from_n(&cid_b),
            mint(&cid_a),
            wrong,
        ),
        (
            "Mint under B from nullifier(B, …), delivered at holder(B, dest)",
            at(&cid_b),
            from_n(&cid_b),
            mint(&cid_b),
            0,
        ),
        (
            "Mint under A from nullifier(A, …), delivered at holder(A, dest)",
            at(&cid_a),
            from_n(&cid_a),
            mint(&cid_a),
            0,
        ),
        (
            "Credit under B from token(B, ETH), delivered at holder(A, dest)",
            at(&cid_a),
            token_actor(program, &cid_b, &ETH),
            credit(&cid_b),
            wrong,
        ),
        (
            "Credit under A from token(B, ETH), delivered at holder(A, dest)",
            at(&cid_a),
            token_actor(program, &cid_b, &ETH),
            credit(&cid_a),
            wrong,
        ),
        (
            "Credit under A from token(A, ETH), delivered at holder(A, dest)",
            at(&cid_a),
            token_actor(program, &cid_a, &ETH),
            credit(&cid_a),
            0,
        ),
    ];
    let elf = harness::bridge::bridge_program();
    let mut executor =
        String::from("| delivered (the program's executor, no state machine) | exit code |\n|---|---:|\n");
    for (what, receiver, from, message, want) in cases {
        let input = ReceiveInput {
            receiver,
            from: Some(from),
            is_authorized: false,
            pre_state: Default::default(),
            message: borsh::to_vec(&message).unwrap(),
        };
        let got = harness::attest::execute(what, elf.elf(), &input).exit_code;
        assert_eq!(got, want, "{what}");
        let _ = writeln!(executor, "| {what} | {got} |");
    }

    let mut out = format!(
        "# R2: other configurations' holdings for the same destination (LEZ v0.4.0-rc1, bridge program {image})\n\n\
         Execution gas limit of a transaction: {GAS_LIMIT}. The k holdings of other configurations are placed as the \
         program leaves them: each at its own holder(cid, dest), one account per configuration \
         (state bytes: the k accounts' states in all); A's claim and A's owner's burn touch holder(A, ·) only.\n\n\
         ## A claim to dest (A's leaf 2, 1 ETH, public kind)\n\n\
         | k | state bytes (k accounts) | outcome | gas_exec charged | fee_base | cycles at holder(A, dest) |\n\
         |---:|---:|---|---:|---:|---:|\n"
    );
    for p in &claims {
        out.push_str(&probe_line(p));
    }
    out.push_str(
        "\n## The owner's burn of 1 ETH it holds under A\n\n\
         | k | state bytes (k accounts) | outcome | gas_exec charged | fee_base | cycles at holder(A, owner) |\n\
         |---:|---:|---|---:|---:|---:|\n",
    );
    for p in &burns {
        out.push_str(&probe_line(p));
    }
    let _ = write!(
        out,
        "\nEvery claim and every burn above: the same gas, fee and cycles as with k = 0.\n\n\
         ## holder(A, dest) takes credits under A only\n\nconfigId(A) {}\nconfigId(B) {}\n\n{executor}",
        hex::encode(cid_a),
        hex::encode(cid_b)
    );
    save("r2.txt", &out);
}

// ---------------------------------------------------------------------------------------------------------------------
// Separation: no configuration's roles or accounts reach another's
// ---------------------------------------------------------------------------------------------------------------------

/// The tenant T's admin and freeze authority: accounts of their own, apart from configuration A's.
const T_ADMIN_SEED: u8 = 0x41;
const T_FREEZE_SEED: u8 = 0x42;

/// The tenant T: a configuration of vault A (A's vault, chain, programs) with its own admin, freeze authority and
/// fee recipient, reading Ethereum through an instance its creator controls (the only way to pair a second
/// configuration with a vault whose slot 2 holds A's id). Paired, ETH registered, root of A's 8 leaves imported.
fn tenant(w: &mut World, log: &mut Vec<TxReport>) -> (BridgeConfig, [u8; 32], Block) {
    let t_controls = Controls {
        admin: *role_account(T_ADMIN_SEED).value(),
        freeze_authority: *role_account(T_FREEZE_SEED).value(),
        fee_recipient: *role_account(T_ADMIN_SEED).value(),
        ..controls()
    };
    let t = BridgeConfig { instance_id: ROGUE_INSTANCE, controls: t_controls, ..w.config_a.clone() };
    let cid_t = t.id(w.program);
    let slots = VaultSlots {
        address: w.vault_a.address,
        root: w.vault_a.root(8),
        count: 8,
        config_id: cid_t,
        extra: registry_slots(&eth_entry(), &[]),
    };
    let forged = Block::new(9_100_000, 300, &[slots]);
    place_rogue_record(w, &forged);
    step(log, rogue_init(w, "T: Init (its creator's record: slot 2 = configId(T))", &t, &forged), None);
    step(log, rogue_register(w, "T: Register ETH", &t, &forged), None);
    let root8 = w.vault_a.root(8);
    step(log, rogue_import(w, "T: ImportRoot n = 8", &t, &forged, root8, 8), None);
    (t, cid_t, forged)
}

/// F.16's "strict separation, as if each were a fully separate deployment": every privileged message the tenant T's
/// roles can send is refused at configuration A's accounts (and applied at T's own: the same message is well formed),
/// a record of T's instance cannot feed A, and every internal message T's accounts can send is refused at A's
/// accounts. A's configuration and token state are byte for byte what they were.
#[test]
fn separation_another_configurations_roles_and_accounts_reach_nothing() {
    use lee_core::program::ReceiveInput;

    let image = b1_program();
    let mut w = World::new();
    w.add_public_accounts(&[role_account(T_ADMIN_SEED), role_account(T_FREEZE_SEED)]);
    let mut log = Vec::new();
    let (program, cid_a, va) = (w.program, w.cid_a, w.vault_a.address);
    pair_honest(&mut w, &mut log);
    let (_t, cid_t, forged) = tenant(&mut w, &mut log);
    let (config_at_a, config_at_t) = (config_actor(program, &cid_a), config_actor(program, &cid_t));
    let a_before = (w.config_state(&cid_a), w.token_state(&cid_a, &ETH));

    // (1) T's roles, through the state machine: each message naming A, then the same naming T.
    let record = w.blocks[2].time_record();
    let until = w.blocks[2].time_ms() + 3_600_000;
    let cap: CapParams = FLOOR;
    let for_cid = |cid: [u8; 32], which: usize| -> BridgeMessage {
        match which {
            0 => BridgeMessage::SetCap { config_id: cid, token: None, mints: true, cap },
            1 => BridgeMessage::SetCap { config_id: cid, token: Some(ETH), mints: false, cap },
            2 => BridgeMessage::SetDepth { config_id: cid, slots: 216_000 },
            3 => BridgeMessage::ProposeRoles {
                config_id: cid,
                freeze_authority: FreezeAuthority::key(*role_account(T_FREEZE_SEED).value()),
                watchers: vec![*role_account(T_ADMIN_SEED).value()],
                record: record.clone(),
            },
            4 => BridgeMessage::CancelRoles { config_id: cid },
            5 => BridgeMessage::Freeze { config_id: cid, token: None, mints: true, burns: true, until_ms: until, record: record.clone() },
            6 => BridgeMessage::Freeze { config_id: cid, token: Some(ETH), mints: true, burns: true, until_ms: until, record: record.clone() },
            7 => BridgeMessage::Pause { config_id: cid, token: None, mints: true, burns: true, record: record.clone() },
            _ => BridgeMessage::Sunset { config_id: cid },
        }
    };
    // (message, T's role that sends it, whether the same message naming T applies at T: those that read no record
    // there, since T's records are its own instance's)
    let rows: [(usize, &str, u8, bool); 9] = [
        (0, "SetCap (global mints)", T_ADMIN_SEED, true),
        (1, "SetCap (ETH burns)", T_ADMIN_SEED, true),
        (2, "SetDepth 216,000", T_ADMIN_SEED, true),
        (3, "ProposeRoles", T_ADMIN_SEED, false),
        (4, "CancelRoles", T_ADMIN_SEED, false),
        (5, "Freeze (global)", T_FREEZE_SEED, false),
        (6, "Freeze (ETH)", T_FREEZE_SEED, false),
        (7, "Pause (global)", T_FREEZE_SEED, false),
        (8, "Sunset", T_FREEZE_SEED, true),
    ];
    let mut matrix = String::from(
        "| message | sent by | naming A: outcome | naming T: outcome |\n|---|---|---|---|\n",
    );
    for (which, what, seed, at_t) in rows {
        let who = if seed == T_ADMIN_SEED { "T's admin" } else { "T's freeze authority" };
        let target_a = match &for_cid(cid_a, which) {
            BridgeMessage::SetCap { token: Some(t), .. } | BridgeMessage::Freeze { token: Some(t), .. } => {
                token_actor(program, &cid_a, t)
            }
            _ => config_at_a,
        };
        let r = w.role(&format!("{who}: {what} naming A"), seed, &for_cid(cid_a, which), target_a);
        let r = step(&mut log, r, Some(code::NOT_AUTHORIZED));
        let mut t_outcome = String::from("(not sent: it reads a record, and T's are its own instance's)");
        if at_t {
            let target_t = match &for_cid(cid_t, which) {
                BridgeMessage::SetCap { token: Some(t), .. } => token_actor(program, &cid_t, t),
                _ => config_at_t,
            };
            let rt = w.role(&format!("{who}: {what} naming T"), seed, &for_cid(cid_t, which), target_t);
            let rt = step(&mut log, rt, None);
            t_outcome = verdict(&rt);
        }
        let _ = writeln!(matrix, "| {what} | {who} | {} | {t_outcome} |", verdict(&r));
    }
    // Multisig approvals: T's freeze authority as the only signer, naming A.
    for (label, action) in [
        ("T's freeze authority: Cosigned Sunset naming A", CosignedAction::Sunset),
        (
            "T's freeze authority: Cosigned Freeze (global) naming A",
            CosignedAction::Freeze { token: None, mints: true, burns: true, until_ms: until, record: record.clone() },
        ),
    ] {
        let r = w.cosign(label, &[T_FREEZE_SEED], &[T_FREEZE_SEED], &cid_a, action);
        let r = step(&mut log, r, Some(code::NOT_AUTHORIZED));
        let what = label.trim_start_matches("T's freeze authority: ");
        let _ = writeln!(matrix, "| {what} | T's freeze authority | {} | |", verdict(&r));
    }
    // A top-level Sunset at config(A) by anyone: A's vault is not attested retired, A's multisig approved nothing.
    let r = w.send("anyone: Sunset at config(A) itself", config_at_a, &BridgeMessage::Sunset { config_id: cid_a }, vec![config_at_a]);
    let r = step(&mut log, r, Some(code::NOT_AUTHORIZED));
    let _ = writeln!(matrix, "| Sunset at config(A) itself | anyone | {} | |", verdict(&r));
    // T's records cannot feed A: an import naming A under T's record slot reads A's own instance there (nothing).
    let fake9 = {
        let mut leaves = w.vault_a.leaves.clone();
        leaves.push([0x99; 32]);
        Vault { address: va, leaves }.root(9)
    };
    let import = BridgeMessage::ImportRoot {
        config_id: cid_a,
        record_slot: forged.slot,
        root: fake9,
        leaf_count: 9,
        proof: forged.import_proof(&va),
    };
    let header = header_actor(ferrata_account(), &harness::bridge::INSTANCE, forged.slot);
    let r = w.send("anyone: ImportRoot naming A under T's record slot", config_at_a, &import, vec![config_at_a, header, root_actor(program, &cid_a, 9)]);
    assert!(!r.ok(), "{}: {}", r.label, verdict(&r));
    log.push(r.clone());
    assert!(w.root_state(&cid_a, 9).is_none(), "nothing imported under A");
    let _ = writeln!(matrix, "| ImportRoot naming A under T's record slot | anyone | {} | |", verdict(&r));

    let a_after = (w.config_state(&cid_a), w.token_state(&cid_a, &ETH));
    assert_eq!(a_before, a_after, "A's configuration and token state are untouched");

    // (2) Internal messages, delivered by the program's executor (no state machine) from T's accounts to A's, and
    // from A's own (the sender check passes: the code is the next check's on an empty state).
    let n = w.vector_n;
    let dest = w.vector_dest;
    let root_of = |cid: [u8; 32]| RootState {
        config_id: cid,
        evm_chain_id: CHAIN_ID,
        vault: va,
        leaf_count: 9,
        root: fake9,
        record_slot: forged.slot,
        block_number: 300,
    };
    let token_of = |cid: [u8; 32]| {
        let mut s = w.token_state(&cid_a, &ETH).unwrap();
        s.config_id = cid;
        s
    };
    let spend = |cid: [u8; 32]| BridgeMessage::Spend {
        nullifier: n,
        config_id: cid,
        evm_chain_id: CHAIN_ID,
        vault: va,
        token: ETH,
        leaf_count: 8,
        amount: DENOMINATIONS[1],
        mint_to: Some(*dest.value()),
        fees: vec![],
    };
    let mint = |cid: [u8; 32]| BridgeMessage::Mint {
        nullifier: n,
        config_id: cid,
        evm_chain_id: CHAIN_ID,
        vault: va,
        token: ETH,
        holder: *dest.value(),
        amount: DENOMINATIONS[1],
    };
    let credit = |cid: [u8; 32]| BridgeMessage::Credit { config_id: cid, token: ETH, amount: DENOMINATIONS[1], to: *dest.value() };
    let extend = |cid: [u8; 32]| BridgeMessage::ExtendFees { config_id: cid, token: ETH, index: 2, schedule: None, count: 9 };
    let listing = |cid: [u8; 32]| BridgeMessage::SetListing { config_id: cid, token: ETH, delisted: true, record_slot: u64::MAX };
    let admit = |cid: [u8; 32]| BridgeMessage::Admit { config_id: cid, token: ETH, mints: true, value: 1 };
    let (cfg, tok) = (|cid: &[u8; 32]| config_actor(program, cid), |cid: &[u8; 32]| token_actor(program, cid, &ETH));
    let null = |cid: &[u8; 32]| nullifier_of(program, cid, CHAIN_ID, &va, &n);
    let hold = |cid: &[u8; 32]| holder_of(program, cid, dest);
    let wrong = u32::from(code::WRONG_SENDER);
    let cases: Vec<(&str, lee::Actor, lee::Actor, BridgeMessage, u32)> = vec![
        ("WriteRoot under A, from config(T), at root(A, 9)", root_actor(program, &cid_a, 9), cfg(&cid_t), BridgeMessage::WriteRoot { root: root_of(cid_a) }, wrong),
        ("WriteRoot under T, from config(T), at root(A, 9)", root_actor(program, &cid_a, 9), cfg(&cid_t), BridgeMessage::WriteRoot { root: root_of(cid_t) }, wrong),
        ("WriteRoot under A, from config(A), at root(A, 9) (control)", root_actor(program, &cid_a, 9), cfg(&cid_a), BridgeMessage::WriteRoot { root: root_of(cid_a) }, 0),
        ("WriteToken under A, from config(T), at token(A, ETH)", tok(&cid_a), cfg(&cid_t), BridgeMessage::WriteToken { state: token_of(cid_a) }, wrong),
        ("WriteToken under A, from config(A), at token(A, ETH) (control)", tok(&cid_a), cfg(&cid_a), BridgeMessage::WriteToken { state: token_of(cid_a) }, 0),
        ("ExtendFees under A, from config(T), at token(A, ETH)", tok(&cid_a), cfg(&cid_t), extend(cid_a), wrong),
        ("ExtendFees under T, from config(T), at token(A, ETH)", tok(&cid_a), cfg(&cid_t), extend(cid_t), wrong),
        ("SetListing under A, from config(T), at token(A, ETH)", tok(&cid_a), cfg(&cid_t), listing(cid_a), wrong),
        ("SetListing under T, from config(T), at token(A, ETH)", tok(&cid_a), cfg(&cid_t), listing(cid_t), wrong),
        ("SetListing under A, from config(A), at token(A, ETH) (control: no token state)", tok(&cid_a), cfg(&cid_a), listing(cid_a), u32::from(code::NOT_REGISTERED)),
        ("Admit under A, from token(T, ETH), at config(A)", cfg(&cid_a), tok(&cid_t), admit(cid_a), wrong),
        ("Admit under T, from token(T, ETH), at config(A)", cfg(&cid_a), tok(&cid_t), admit(cid_t), wrong),
        ("Admit under A, from token(A, ETH), at config(A) (control: no configuration state)", cfg(&cid_a), tok(&cid_a), admit(cid_a), u32::from(code::NOT_FOUND)),
        ("Spend under A, from token(T, ETH), at nullifier(A, ...)", null(&cid_a), tok(&cid_t), spend(cid_a), wrong),
        ("Spend under T, from token(T, ETH), at nullifier(A, ...)", null(&cid_a), tok(&cid_t), spend(cid_t), wrong),
        ("Mint under A, from nullifier(T, ...), at holder(A, dest)", hold(&cid_a), null(&cid_t), mint(cid_a), wrong),
        ("Mint under T, from nullifier(T, ...), at holder(A, dest)", hold(&cid_a), null(&cid_t), mint(cid_t), wrong),
        ("Mint under A, from nullifier(A, ...), at holder(A, dest) (control)", hold(&cid_a), null(&cid_a), mint(cid_a), 0),
        ("Credit under A, from token(T, ETH), at holder(A, dest)", hold(&cid_a), tok(&cid_t), credit(cid_a), wrong),
        ("Credit under T, from token(T, ETH), at holder(A, dest)", hold(&cid_a), tok(&cid_t), credit(cid_t), wrong),
    ];
    let elf = harness::bridge::bridge_program();
    let mut executor = String::from("| delivered (the program's executor) | exit code |\n|---|---:|\n");
    for (what, receiver, from, message, want) in cases {
        let input = ReceiveInput {
            receiver,
            from: Some(from),
            is_authorized: false,
            pre_state: Default::default(),
            message: borsh::to_vec(&message).unwrap(),
        };
        let got = harness::attest::execute(what, elf.elf(), &input).exit_code;
        assert_eq!(got, want, "{what}");
        let _ = writeln!(executor, "| {what} | {got} |");
    }

    let mut out = format!(
        "# Separation of configurations (LEZ v0.4.0-rc1 state machine, bridge program {image})\n\n\
         configId(A) {} (vault A's own; roles: the world's admin and freeze authority)\n\
         configId(T) {} (vault A, an instance its creator controls; roles: accounts of seeds 0x41 and 0x42)\n\n\
         ## T's roles\n\n{matrix}\n\
         A's configuration and ETH token state after every attempt: identical to before.\n\n\
         ## Internal messages\n\n{executor}\n## Every transaction\n\n",
        hex::encode(cid_a),
        hex::encode(cid_t)
    );
    out.push_str(&table(&log));
    save("separation.txt", &out);
}
