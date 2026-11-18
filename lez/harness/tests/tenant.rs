//! The malicious-tenant test (RFP-022 F.16, the B2 gate's test), written 18 November 2026.
//!
//! An independent review of the bridge's LEZ v0.3.1 port (its findings 1 and 3) found two properties and stated that
//! the deployed v0.4 bridge program has both. These tests run them through LEZ v0.4.0-rc1's own state machine and fee
//! settlement with the frozen bridge program (its image id is asserted against `docs/image-ids.md`). They are a
//! RECORD of the behaviour: each asserts the outcomes it documents, so a change of behaviour fails them.
//!
//! * R1. A configuration's attestation program and instance are free fields (`BridgeConfig`), and the nullifier
//!   account is keyed by (chain, vault, N) without the configuration id (`nullifier_seed`). A second configuration of
//!   the same vault and chain that reads Ethereum through an instance its creator controls can spend the nullifier
//!   of any public-kind deposit whose `s` it knows; the paired configuration's own claim is then refused (250).
//! * R2. `holder(dest)` is one account for every configuration. LEZ v0.4 has no limit on an actor's state, so the
//!   review's figure for v0.3.1 (1,505 holdings, then a panic at the shard limit) does not carry over; what does:
//!   every mint into `holder(dest)` decodes, searches and re-encodes its whole list, so its cost grows with the list
//!   until a claim to that destination no longer fits a transaction's execution gas limit (10,000,000).
//!
//! The rogue configuration's records. Anyone can create an instance of the real attestation program (`Init` takes
//! any checkpoint with a bootstrap that matches it, `lez/guests/program/src/main.rs`, `init`), and that instance
//! then records whatever its creator's sync committee signs. Here the attacker's records are placed at
//! `header(X, slot)` as the attestation program writes them, as this harness places every record of the honest
//! instance (`World::add_block`); everything from the rogue configuration's `Init` on is executed.
//!
//! Re-run: `RISC0_DEV_MODE=1 cargo test -p harness --locked --test tenant -- --nocapture --test-threads 1` (in `lez/`).
//! Each test writes its figures to `lez/results/tenant/`; `lez/results/tenant.md` reads them.
//!
//! **The second build** (`guests/bridge-next`, not deployed; feature `bridge-next`): the same attacks against the
//! program whose nullifier and holder accounts are keyed by the configuration id as well (`nullifier(cid, chain,
//! vault, N)`, `holder(cid, account)`, `ferrata_lez_bridge_wire::config_keyed`). Its three tests replace the three
//! above and assert the fix: the paired configuration's claim applies after the rogue configuration's (and after a
//! copied plaintext claim ordered first), and a claim or burn costs the same whatever other configurations hold for
//! the same destination. Re-run: the command above with `--features bridge-next`; they write to
//! `lez/results/bridge-next/tenant/` (`lez/results/bridge-next.md` reads them).

use std::fmt::Write as _;

use ferrata_bridge_core::inbound::DestKind;
use ferrata_lez_bridge_wire::{
    burn_actor, code, config_actor, config_seed, own_actor, register_slots, root_actor, token_actor, word_of_u128,
    BridgeConfig, BridgeMessage, BridgeState, BurnRecord, HolderState, Holding, Note, NullifierState,
};
use ferrata_lez_wire::{header_actor, RecordState};
use harness::{
    bridge::{
        code_name, eth_entry, eth_schedule_one, holder_of, registry_slots, relayer, render, role_account, role_key,
        schedule_zero, Block, Deposit, Vault, VaultSlots, World, CHAIN_ID, DENOMINATIONS, ETH, OUTSIDER_SEED,
    },
    ferrata_account, image_id_hex, TxReport,
};
use lee::{Account, PublicAccountEvidence};
use lee_core::account::ActorState;

/// `docs/image-ids.md`: the bridge program's image id, which the live Sepolia vault's configuration id commits to.
const PINNED_BRIDGE: &str = "ccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd";
/// The attacker's instance of the attestation program.
const ROGUE_INSTANCE: [u8; 32] = [0xbe; 32];
/// LEZ v0.4's execution gas limit of a transaction.
const GAS_LIMIT: u64 = fee_core::market::MAX_GAS_EXEC;

#[cfg(not(feature = "bridge-next"))]
fn frozen_bridge() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
    assert_eq!(image_id_hex(methods::FERRATA_BRIDGE_ID), PINNED_BRIDGE, "the bridge program is the frozen one");
}

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
    let dir = harness::results_dir().join("tenant");
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

#[cfg(not(feature = "bridge-next"))]
#[test]
fn r1_a_rogue_configuration_freezes_a_public_kind_deposit() {
    frozen_bridge();
    let mut w = World::new();
    let mut log = Vec::new();
    let program = w.program;
    let va = w.vault_a.address;
    let cid_a = w.cid_a;
    let payer = w.chain.payer;
    let made_up = invented(&w);
    let (_, cid_b, fake) = rogue(&mut w, &mut log, &made_up);

    // B claims A's public-kind deposit (leaf 2, 1 ETH) whose s it knows (here the shared test vector's, public).
    let vector = w.vector.clone();
    let n = w.vector_n;
    let squat = fake.witness(&vector, 9);
    step(&mut log, w.claim("B: Claim of A's deposit (leaf 2, public kind) with its s", &cid_b, 9, &va, squat), None);
    let spent = NullifierState { config_id: cid_b, token: ETH, leaf_count: 9, amount: DENOMINATIONS[1] };
    assert_eq!(
        w.nullifier(&cid_b, &va, &n),
        Some(spent),
        "nullifier(chain, vault, N) is spent, recorded under configId(B)"
    );
    let held = w.holder(&cid_b, w.vector_dest).expect("holder(dest)");
    assert_eq!(held.holdings, vec![Holding { config_id: cid_b, token: ETH, amount: DENOMINATIONS[1] }]);

    // A pairs and claims the same deposit: refused, the nullifier is spent. A's other deposit still claims.
    pair_honest(&mut w, &mut log);
    let witness = w.vault_a.witness(&vector, 8);
    let refused = w.claim("A: Claim of the same deposit (leaf 2) by its owner", &cid_a, 8, &va, witness);
    let refused = step(&mut log, refused, Some(code::ALREADY_CLAIMED));
    assert_eq!(w.balance(w.vector_dest, &cid_a), 0, "nothing under A");
    let leaf6 = w.vault_a.witness(&w.payer_deposit.clone(), 8);
    step(&mut log, w.claim("A: Claim of A's other deposit (leaf 6, 10 ETH)", &cid_a, 8, &va, leaf6), None);
    assert_eq!(w.balance(payer, &cid_a), DENOMINATIONS[2]);

    // B mints the invented deposit and the payer burns it: the note is recorded under configId(B) only.
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

    // A private-kind deposit (leaf 5, 1 ETH to a private account). (a) Without s: a guessed s opens no leaf.
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
    // (b) With s (as if leaked): the plaintext path refuses a private-kind deposit, and a private claim must run as
    // the destination account itself, whose key the attacker does not have.
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
    assert_eq!(w.nullifier(&cid_b, &va, &n5), None, "the private-kind deposit's nullifier is unspent");

    let mut out = format!(
        "# R1: a rogue configuration of the same vault and chain (LEZ v0.4.0-rc1 state machine, frozen bridge {PINNED_BRIDGE})\n\n\
         configId(A) {}\nconfigId(B) {} (A's fields, instance {})\n\n",
        hex::encode(cid_a),
        hex::encode(cid_b),
        hex::encode(ROGUE_INSTANCE)
    );
    out.push_str(&table(&log));
    let _ = write!(
        out,
        "\nA's refused claim: exit {:?}, gas_exec {}, fee_base {} (charged to its sender)\n\
         holder(dest of leaf 2): {:?}\nnullifier(chain, vault, N of leaf 2): {:?}\n\
         burn(configId(B), H(note)): written; burn(configId(A), H(note)): empty\n\
         nullifier of the private-kind deposit (leaf 5): unspent\n\n{}",
        refused.exit_code,
        refused.charge.gas_exec,
        refused.charge.fee_base,
        w.holder(&cid_b, w.vector_dest).unwrap().holdings,
        w.nullifier(&cid_b, &va, &n),
        render(&log)
    );
    save("r1.txt", &out);
}

/// A plaintext claim carries the whole witness, `s` included. A transaction seen before inclusion (a block template,
/// a mempool) can be copied into a rogue configuration's claim ordered before it. The harness settles transactions in
/// the order they are submitted, one per block.
#[cfg(not(feature = "bridge-next"))]
#[test]
fn r1_a_plaintext_claim_seen_before_inclusion_is_front_run() {
    frozen_bridge();
    let mut w = World::new();
    let mut log = Vec::new();
    let va = w.vault_a.address;
    let cid_a = w.cid_a;
    let made_up = invented(&w);
    let (_, cid_b, _) = rogue(&mut w, &mut log, &made_up);
    pair_honest(&mut w, &mut log);

    // The payer's own plaintext claim of its deposit (leaf 6, public kind), as a block template carries it.
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
    // The attacker decodes it and copies the witness into B's claim (B's root, n = 9, also holds leaf 6).
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
    let r = step(&mut log, r, Some(code::ALREADY_CLAIMED));
    assert_eq!(w.balance(w.chain.payer, &cid_a), 0, "the payer holds nothing under A");
    assert_eq!(w.balance(w.chain.payer, &cid_b), DENOMINATIONS[2], "the payer holds 10 ETH under B: no vault pays it");

    let mut out =
        String::from("# R1: front-run of a plaintext claim (one transaction per block, in submission order)\n\n");
    out.push_str(&table(&log));
    let _ = writeln!(
        out,
        "\nthe honest claim: {} bytes as a message, s at a fixed offset; refused {:?}, gas_exec {}, fee_base {}",
        seen.len(),
        r.exit_code,
        r.charge.gas_exec,
        r.charge.fee_base
    );
    save("r1-frontrun.txt", &out);
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

/// `holdings` placed at `account`'s holder of configuration `cid` (`holder(account)` of the deployed program, which has
/// one for every configuration; `holder(cid, account)` of the second build).
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
    /// Gas of the setup steps (for the attacker's cost on the deployed program).
    #[cfg_attr(feature = "bridge-next", allow(dead_code))]
    register_gas: u64,
}

fn holder_cycles(w: &World, cid: &[u8; 32], r: &TxReport, account: lee::AccountId) -> u64 {
    let holder = holder_of(w.program, cid, account);
    r.transitions.iter().filter(|t| t.receiver == holder).map(|t| t.cycles).sum()
}

/// A claim (A's leaf 2, public kind, 1 ETH) to `holder(dest)` holding `k` holdings of other configurations.
#[cfg(not(feature = "bridge-next"))]
fn claim_into(k: usize) -> Probe {
    claim_with_others(k, |w, cid_a, dest, k| place_holder(w, cid_a, dest, fillers(k)))
}

/// A claim (A's leaf 2, public kind, 1 ETH) to `dest`, after `place(world, cid_a, dest, k)` has placed `k` holdings
/// of other configurations for `dest` (returning the bytes placed).
fn claim_with_others(k: usize, place: fn(&mut World, &[u8; 32], lee::AccountId, usize) -> usize) -> Probe {
    let mut w = World::new();
    let mut log = Vec::new();
    let (a, cid_a, va) = (w.config_a.clone(), w.cid_a, w.vault_a.address);
    step(&mut log, w.init("A: Init", &a, 0), None);
    let reg = step(&mut log, w.register("A: Register ETH", &cid_a, 0, &eth_entry(), &va), None);
    step(&mut log, w.import("A: ImportRoot n = 5", &cid_a, 0, w.vault_a.root(5), 5, &va), None);
    let dest = w.vector_dest;
    let bytes = place(&mut w, &cid_a, dest, k);
    let witness = w.vault_a.witness(&w.vector.clone(), 5);
    let label = if harness::bridge::SECOND_BUILD {
        format!("A: Claim to dest, {k} other configurations' holdings for it (each at its own holder(cid, dest))")
    } else {
        format!("A: Claim to holder(dest) holding {k} holdings")
    };
    let report = w.claim(&label, &cid_a, 5, &va, witness);
    if report.ok() {
        assert_eq!(w.balance(dest, &cid_a), DENOMINATIONS[1]);
    } else {
        assert_eq!(w.balance(dest, &cid_a), 0);
        assert_eq!(w.nullifier(&cid_a, &va, &w.vector_n), None, "the deposit is not spent");
    }
    let at_holder = holder_cycles(&w, &cid_a, &report, dest);
    Probe { k, bytes, report, at_holder, register_gas: reg.charge.gas_exec }
}

/// The owner's public burn of 1 ETH it holds under A, its `holder(owner)` holding `k` other holdings before it.
#[cfg(not(feature = "bridge-next"))]
fn burn_from(k: usize) -> Probe {
    burn_with_others(k, |w, cid_a, payer, k| {
        let own_holding = w.holder(cid_a, payer).unwrap().holdings;
        let mut holdings = fillers(k);
        holdings.extend(own_holding);
        place_holder(w, cid_a, payer, holdings)
    })
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
    let label = if harness::bridge::SECOND_BUILD {
        format!("A: the owner's Burn of 1 ETH, {k} other configurations' holdings for it (each at its own holder)")
    } else {
        format!("A: the owner's Burn of 1 ETH, its holder holding {k} other holdings")
    };
    let report = w.send(&label, own, &BridgeMessage::burn(cid_a, &note), actors);
    let left = if report.ok() { DENOMINATIONS[2] - DENOMINATIONS[1] } else { DENOMINATIONS[2] };
    assert_eq!(w.balance(payer, &cid_a), left);
    let at_holder = holder_cycles(&w, &cid_a, &report, payer);
    Probe { k, bytes, report, at_holder, register_gas: 0 }
}

/// The largest `k` at which `probe` applies, from `lo` (applies) by doubling then bisection; every probe kept.
#[cfg(not(feature = "bridge-next"))]
fn threshold(probe: fn(usize) -> Probe, lo: usize, seen: &mut Vec<Probe>) -> usize {
    let mut ok = lo;
    let mut bad = None;
    let mut k = lo.max(1);
    while bad.is_none() && k <= 1 << 20 {
        k *= 2;
        let p = probe(k);
        if p.report.ok() {
            ok = k;
        } else {
            bad = Some(k);
        }
        seen.push(p);
    }
    bisect(probe, ok, bad.expect("a size at which it no longer applies"), seen)
}

/// The largest `k` in `ok..bad` at which `probe` applies (it applies at `ok`, not at `bad`).
#[cfg(not(feature = "bridge-next"))]
fn bisect(probe: fn(usize) -> Probe, mut ok: usize, mut bad: usize, seen: &mut Vec<Probe>) -> usize {
    while bad - ok > 1 {
        let mid = (ok + bad) / 2;
        let p = probe(mid);
        if p.report.ok() {
            ok = mid;
        } else {
            bad = mid;
        }
        seen.push(p);
    }
    ok
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

#[cfg(not(feature = "bridge-next"))]
#[test]
fn r2_a_holder_filled_with_other_configurations_holdings() {
    frozen_bridge();
    let mut claims = vec![claim_into(0), claim_into(1_505)];
    assert!(claims.iter().all(|p| p.report.ok()), "no shard limit on LEZ v0.4: 1,505 holdings take a claim");
    let k_max = threshold(claim_into, 1_505, &mut claims);
    claims.sort_by_key(|p| p.k);
    let last = claims.iter().find(|p| p.k == k_max).unwrap();
    let first_bad = claims.iter().find(|p| p.k == k_max + 1).unwrap();
    assert!(last.report.charge.gas_exec <= GAS_LIMIT);
    // Past it: no exit code (not a refusal), the whole gas limit charged, nothing minted, the deposit unspent.
    assert!(!first_bad.report.ok() && first_bad.report.exit_code.is_none(), "{}", verdict(&first_bad.report));
    assert_eq!(first_bad.report.charge.gas_exec, GAS_LIMIT);

    let mut burns = vec![burn_from(0), burn_from(k_max + 1)];
    assert!(burns[0].report.ok());
    let k_burn = if burns[1].report.ok() {
        threshold(burn_from, k_max + 1, &mut burns)
    } else {
        bisect(burn_from, 0, k_max + 1, &mut burns)
    };
    burns.sort_by_key(|p| p.k);

    // The attacker's cost to bring a holder from empty to k_max + 1 holdings: one claim per holding, the i-th into a
    // holder of i holdings (gas linear in i between the probes at 0 and k_max), and one Register per token.
    let c0 = claims[0].report.charge.gas_exec;
    let slope = (last.report.charge.gas_exec - c0) as f64 / k_max as f64;
    let n = (k_max + 1) as f64;
    let claims_gas = n * c0 as f64 + slope * n * (n - 1.0) / 2.0;
    let register_gas = claims[0].register_gas as f64;
    let total = claims_gas + n * register_gas;

    let mut out = format!(
        "# R2: holder(dest) with k holdings of other configurations (LEZ v0.4.0-rc1, frozen bridge {PINNED_BRIDGE})\n\n\
         Execution gas limit of a transaction: {GAS_LIMIT}. Holdings placed directly (borsh: 4 + 68 k + 1 bytes).\n\n\
         ## A claim to it (leaf 2, 1 ETH, public kind)\n\n\
         | k | state bytes | outcome | gas_exec charged | fee_base | cycles at the holder |\n|---:|---:|---|---:|---:|---:|\n"
    );
    for p in &claims {
        out.push_str(&probe_line(p));
    }
    let _ = write!(
        out,
        "\nlargest k at which a claim to it applies: {k_max}; at {}: {}\n\n\
         ## The owner's burn of 1 ETH it holds there (k other holdings in front of it)\n\n\
         | k | state bytes | outcome | gas_exec charged | fee_base | cycles at the holder |\n|---:|---:|---|---:|---:|---:|\n",
        k_max + 1,
        verdict(&first_bad.report)
    );
    for p in &burns {
        out.push_str(&probe_line(p));
    }
    let _ = write!(
        out,
        "\nlargest k at which the owner's burn applies: {k_burn}\n\n\
         ## The attacker's cost (derived from the measured gas above)\n\n\
         claim into an empty holder: {c0} gas; growth: {slope:.1} gas per holding already there (k = 0 to {k_max});\n\
         Register: {register_gas} gas per token (one holding per (configuration, token) pair)\n\
         {} claims: {claims_gas:.3e} gas; {} Registers: {:.3e} gas; total about {total:.3e} execution gas\n",
        k_max + 1,
        k_max + 1,
        n * register_gas
    );
    save("r2.txt", &out);
}

// ---------------------------------------------------------------------------------------------------------------------
// The second build (`guests/bridge-next`, feature `bridge-next`): nullifier(cid, chain, vault, N) and holder(cid, account)
// ---------------------------------------------------------------------------------------------------------------------

/// `docs/image-ids.md`, "Second build of the bridge program, not deployed": the image id of `guests/bridge-next`.
#[cfg(feature = "bridge-next")]
const SECOND_BRIDGE: &str = "c5cad2cd470807a1799613a1f5f699f9d4f4074821a9fb26389ca917a4d0d6da";

#[cfg(feature = "bridge-next")]
fn second_build() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
    assert_eq!(image_id_hex(harness::bridge::BRIDGE_ID), SECOND_BRIDGE, "the bridge program is its second build");
    assert_ne!(SECOND_BRIDGE, PINNED_BRIDGE);
}

/// R1 against the second build: the rogue configuration B's claim of A's deposit applies in B's namespace only
/// (`nullifier(B, …)`, `holder(B, dest)`), and A's claim of it then applies. Within A the deposit still mints once.
#[cfg(feature = "bridge-next")]
#[test]
fn r1_next_the_paired_configuration_claims_after_a_rogue_one() {
    second_build();
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

    // A private-kind deposit (leaf 5): as on the deployed program, B cannot take it.
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
        "# R1 against the second build: a rogue configuration of the same vault and chain (LEZ v0.4.0-rc1 state machine, \
         second build {SECOND_BRIDGE}, not deployed)\n\n\
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

/// The front-run against the second build: B's copy of a pending plaintext claim, ordered first, applies under B;
/// the owner's own claim then applies under A.
#[cfg(feature = "bridge-next")]
#[test]
fn r1_next_a_copied_plaintext_claim_no_longer_blocks_the_owner() {
    second_build();
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
        "# R1 against the second build: front-run of a plaintext claim (one transaction per block, in submission order)\n\n",
    );
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

/// `k` holdings of other configurations for `account` as the second build leaves them: each at its own
/// `holder(cid, account)`. Returns the bytes placed, in all.
#[cfg(feature = "bridge-next")]
fn place_others(w: &mut World, _cid_a: &[u8; 32], account: lee::AccountId, k: usize) -> usize {
    fillers(k)
        .into_iter()
        .map(|h| {
            let cid = h.config_id;
            place_holder(w, &cid, account, vec![h])
        })
        .sum()
}

/// R2 against the second build: other configurations' holdings for a destination sit in other accounts, so a claim
/// to it and its owner's burn cost the same with none of them and with many. And `holder(A, dest)` takes credits
/// under A only (the executor, as `bridge_secfix.rs` checks C-1).
#[cfg(feature = "bridge-next")]
#[test]
fn r2_next_other_configurations_holdings_cost_a_claim_nothing() {
    use lee_core::program::ReceiveInput;

    second_build();
    // 6,049: where a claim fails on the deployed program; 12,098: twice that.
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
        "# R2 against the second build: other configurations' holdings for the same destination (LEZ v0.4.0-rc1, \
         second build {SECOND_BRIDGE}, not deployed)\n\n\
         Execution gas limit of a transaction: {GAS_LIMIT}. The k holdings of other configurations are placed as the \
         second build would leave them: each at its own holder(cid, dest), one account per configuration \
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
