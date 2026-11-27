//! The attack paths of the security review of 11 November 2026 (`docs/reviews/security`), each
//! run against the bridge program through LEZ's own state machine (RISC Zero dev mode).
//!
//! **C-1.** A claim mints to three accounts its sender chooses or names: the public destination
//! (inside `C`, chosen by the depositor), the relayer (a free field) and the fee recipient of the
//! configuration (chosen by whoever makes one). If any of them is the address of one of the bridge
//! program's own PDAs that holds nothing yet, the mint must not write there: (a) `config(cid)`
//! before `Init`, (b) `token(cid, token)` before `Register`, (c) the nullifier of a pending
//! claim, (d) `root(cid, n)` before its import, (e) the burn record of a pending burn. Each test
//! makes the attacker's own configuration (its vault's deposits name the targets), claims, and
//! then runs the honest step the review says the claim blocks.
//!
//! **H-1.** `ImportRoot` must run only as a call the checkpoint sees: a nested call from another
//! program, and a delivery that carries a sender (an asynchronous message), are refused.
//!
//! **F1** (the red team of 18 November 2026, BR-INIT-05; `docs/reviews/redteam-2026-11-18.md`). `Init` with
//! controls whose freeze authority is the admin: refused with the code `ProposeRoles` gives the admin as a member (the
//! program before B1, image `0xccde6171…71dd`, accepted it).

use ferrata_bridge_core::{inbound::DestKind, Hash};
use ferrata_lez_bridge_wire::{
    burn_actor, burn_seed, code, config_actor, config_seed, root_actor, root_seed, token_actor, token_seed,
    word_of_u128, BridgeConfig, BridgeMessage, BridgeState, Controls, FreezeAuthority, Note,
};
use harness::{
    bridge::{
        bridge_program, config, controls, eth_entry, fee_recipient, holder_of, nullifier_of, nullifier_seed_of,
        registry_slots, Block, Deposit, Vault, VaultSlots, World, ADMIN_SEED, CHAIN_ID, DENOMINATIONS, ETH, INSTANCE,
        RELAY_FEES,
    },
    TxReport,
};
use lee::{AccountId, Actor, PublicAccountEvidence};
use lee_core::program::{PdaSeed, ReceiveInput};
use test_guest_core::Script;

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

fn expect(report: &TxReport, code: Option<u8>) {
    match code {
        None => assert!(report.ok(), "{}: {:?} (exit {:?})", report.label, report.charge.revert, report.exit_code),
        Some(c) => assert_eq!(report.exit_code, Some(u32::from(c)), "{}: {:?}", report.label, report.charge.revert),
    }
}

/// One configuration under test: its vault's deposits (all public-kind, 0.1 ETH) and the block
/// whose record holds the vault's storage.
struct Conf {
    config: BridgeConfig,
    cid: Hash,
    vault: Vault,
    deposits: Vec<Deposit>,
    block: usize,
}

impl Conf {
    fn count(&self) -> u64 {
        self.vault.leaves.len() as u64
    }

    fn nullifier(&self, leaf: usize) -> Hash {
        self.deposits[leaf].nullifier(CHAIN_ID, &self.vault.address)
    }
}

/// The configuration id `controls` give the vault at `address`, before anything is recorded.
fn cid_of(w: &World, address: [u8; 20], controls: Controls) -> Hash {
    config(address, controls).id(w.program)
}

/// A vault at `address` whose deposits go to `dests`, with ETH registered in its storage, recorded at
/// beacon slot `slot`; its configuration has `controls`.
fn vault(w: &mut World, address: [u8; 20], controls: Controls, dests: &[Hash], slot: u64) -> Conf {
    let config = config(address, controls);
    let cid = config.id(w.program);
    let deposits: Vec<Deposit> = dests
        .iter()
        .enumerate()
        .map(|(i, dest)| Deposit {
            leaf_index: i as u64,
            depositor: [address[0] ^ 0x5a; 20],
            token: ETH,
            amount: word_of_u128(DENOMINATIONS[0]),
            s: [address[0].wrapping_add(i as u8 + 1); 32],
            kind: DestKind::Public,
            dest: *dest,
        })
        .collect();
    let vault = Vault { address, leaves: deposits.iter().map(Deposit::leaf).collect() };
    let n = vault.leaves.len() as u64;
    let slots =
        VaultSlots { address, root: vault.root(n), count: n, config_id: cid, extra: registry_slots(&eth_entry(), &[]) };
    let block = w.add_block(Block::new(slot, 500 + slot % 1000, &[slots]));
    Conf { config, cid, vault, deposits, block }
}

/// `Init`, `Register` (ETH) and `ImportRoot` (all its leaves) of `c`, each expected to succeed.
fn open(w: &mut World, label: &str, c: &Conf) {
    expect(&w.init(&format!("Init {label}"), &c.config, c.block), None);
    expect(&w.register(&format!("Register ETH ({label})"), &c.cid, c.block, &eth_entry(), &c.vault.address), None);
    let (root, n) = (c.vault.root(c.count()), c.count());
    expect(&w.import(&format!("ImportRoot {label} n={n}"), &c.cid, c.block, root, n, &c.vault.address), None);
}

/// The attacker's plaintext claim of leaf `leaf` of `att`, naming `relayer`, with admission evidence
/// for every PDA in `targets` (what an attacker adds so that LEZ admits an account no one wrote yet).
fn attack_claim(
    w: &mut World,
    label: &str,
    att: &Conf,
    leaf: usize,
    relayer: Option<Hash>,
    targets: &[PdaSeed],
) -> TxReport {
    let program = w.program;
    let d = att.deposits[leaf].clone();
    let n = att.count();
    let witness = att.vault.witness(&d, n);
    let root = root_actor(program, &att.cid, n);
    let nullifier = nullifier_of(program, &att.cid, CHAIN_ID, &att.vault.address, &att.nullifier(leaf));
    let mut actors = vec![root];
    actors.extend(w.claim_actors(&att.cid, &ETH, nullifier, Some(AccountId::new(d.dest)), relayer));
    // The attacker's configuration has its own fee recipient (the relay fee's account when no
    // relayer is named).
    let fee_actor = holder_of(program, &att.cid, AccountId::new(att.config.controls.fee_recipient));
    if !actors.contains(&fee_actor) {
        actors.push(fee_actor);
    }
    let evidence = targets.iter().map(|seed| PublicAccountEvidence::Pda { program, seed: *seed }).collect();
    let message = BridgeMessage::Claim { config_id: att.cid, leaf_count: n, witness, relayer };
    w.submit(label, root, &message, actors, evidence)
}

fn is_empty(w: &World, actor: Actor) -> bool {
    w.chain.actor_state(actor).is_empty()
}

const HONEST: [u8; 20] = [0xa1; 20];
const HONEST2: [u8; 20] = [0xa2; 20];
const ATTACKER: [u8; 20] = [0xb7; 20];

/// C-1 (a), by the destination and by the fee recipient: an attacker's claim names `config(cid)`
/// of two configurations that are not initialised yet; both `Init`s still succeed.
#[test]
fn c1_a_a_claim_cannot_block_init() {
    require_dev_mode();
    let mut w = World::new();
    let program = w.program;
    let payer_id = *w.chain.payer.value();
    let (cid_h, cid_h2) = (cid_of(&w, HONEST, controls()), cid_of(&w, HONEST2, controls()));
    let (target, target2) = (config_actor(program, &cid_h), config_actor(program, &cid_h2));
    let honest = vault(&mut w, HONEST, controls(), &[payer_id], 9_200_001);
    let honest2 = vault(&mut w, HONEST2, controls(), &[payer_id], 9_200_002);
    // The attacker's configuration: its fee recipient is the second target.
    let att_controls = Controls { fee_recipient: *target2.account_id.value(), ..controls() };
    let att = vault(&mut w, ATTACKER, att_controls, &[*target.account_id.value()], 9_200_003);
    open(&mut w, "attacker", &att);
    let r = attack_claim(
        &mut w,
        "attacker's claim: dest = config(H), relay fee to config(H2)",
        &att,
        0,
        None,
        &[config_seed(&cid_h), config_seed(&cid_h2)],
    );
    expect(&r, None);
    assert!(is_empty(&w, target), "the claim wrote nothing at config(H)");
    assert!(is_empty(&w, target2), "the claim wrote nothing at config(H2)");
    // The minted wrapped tokens are the destination's and the fee recipient's, where holdings live.
    assert_eq!(w.balance_of(target.account_id, &att.cid, &ETH), DENOMINATIONS[0]);
    assert_eq!(w.balance_of(target2.account_id, &att.cid, &ETH), RELAY_FEES[0]);
    expect(&w.init("Init H after the attacker's claim", &honest.config, honest.block), None);
    expect(&w.init("Init H2 after the attacker's claim", &honest2.config, honest2.block), None);
    assert!(matches!(w.config_state(&cid_h), Some(ferrata_lez_bridge_wire::ConfigState::Ready { .. })));
    assert!(matches!(w.config_state(&cid_h2), Some(ferrata_lez_bridge_wire::ConfigState::Ready { .. })));
}

/// C-1 (b), by the relayer (the review's test): a claim with `relayer = token(cid, ETH)` before
/// `Register`; `Register` still succeeds.
#[test]
fn c1_b_a_claim_cannot_block_register() {
    require_dev_mode();
    let mut w = World::new();
    let program = w.program;
    let payer_id = *w.chain.payer.value();
    let honest = vault(&mut w, HONEST, controls(), &[payer_id], 9_200_011);
    let att = vault(&mut w, ATTACKER, controls(), &[*fee_recipient().value()], 9_200_012);
    open(&mut w, "attacker", &att);
    expect(&w.init("Init H", &honest.config, honest.block), None);
    let target = token_actor(program, &honest.cid, &ETH);
    let r = attack_claim(
        &mut w,
        "attacker's claim: relayer = token(H, ETH)",
        &att,
        0,
        Some(*target.account_id.value()),
        &[token_seed(&honest.cid, &ETH)],
    );
    expect(&r, None);
    assert!(is_empty(&w, target), "the claim wrote nothing at token(H, ETH)");
    assert_eq!(w.balance_of(target.account_id, &att.cid, &ETH), RELAY_FEES[0]);
    let r = w.register("Register ETH (H) after the attacker's claim", &honest.cid, honest.block, &eth_entry(), &HONEST);
    expect(&r, None);
    assert!(w.token_state(&honest.cid, &ETH).is_some());
}

/// C-1 (c): an attacker who sees a pending claim's `N` names `nullifier(chain, vault, N)` as its
/// own claim's destination and gets it in first; the victim's claim still succeeds.
#[test]
fn c1_c_a_claim_cannot_destroy_a_pending_claim() {
    require_dev_mode();
    let mut w = World::new();
    let program = w.program;
    let payer_id = *w.chain.payer.value();
    let payer = w.chain.payer;
    let honest = vault(&mut w, HONEST, controls(), &[payer_id], 9_200_021);
    let target = nullifier_of(program, &honest.cid, CHAIN_ID, &HONEST, &honest.nullifier(0));
    let att = vault(&mut w, ATTACKER, controls(), &[*target.account_id.value()], 9_200_022);
    open(&mut w, "attacker", &att);
    open(&mut w, "H", &honest);
    let r = attack_claim(
        &mut w,
        "attacker's claim: dest = the victim's nullifier account",
        &att,
        0,
        None,
        &[nullifier_seed_of(&honest.cid, CHAIN_ID, &HONEST, &honest.nullifier(0))],
    );
    expect(&r, None);
    assert!(is_empty(&w, target), "the claim wrote nothing at the nullifier");
    let witness = honest.vault.witness(&honest.deposits[0], 1);
    let r = w.claim("the victim's claim after the attacker's", &honest.cid, 1, &HONEST, witness);
    expect(&r, None);
    assert_eq!(w.balance_of(payer, &honest.cid, &ETH), DENOMINATIONS[0], "the victim has its deposit");
    assert!(w.nullifier(&honest.cid, &HONEST, &honest.nullifier(0)).is_some(), "N is spent by the victim's claim");
}

/// C-1 (d): a claim names `root(cid, n)` at a count not imported yet; the import still succeeds.
#[test]
fn c1_d_a_claim_cannot_block_an_import() {
    require_dev_mode();
    let mut w = World::new();
    let program = w.program;
    let payer_id = *w.chain.payer.value();
    let honest = vault(&mut w, HONEST, controls(), &[payer_id], 9_200_031);
    let target = root_actor(program, &honest.cid, 1);
    let att = vault(&mut w, ATTACKER, controls(), &[*target.account_id.value()], 9_200_032);
    open(&mut w, "attacker", &att);
    expect(&w.init("Init H", &honest.config, honest.block), None);
    let r = attack_claim(&mut w, "attacker's claim: dest = root(H, 1)", &att, 0, None, &[root_seed(&honest.cid, 1)]);
    expect(&r, None);
    assert!(is_empty(&w, target), "the claim wrote nothing at root(H, 1)");
    let r = w.import(
        "ImportRoot H n=1 after the attacker's claim",
        &honest.cid,
        honest.block,
        honest.vault.root(1),
        1,
        &HONEST,
    );
    expect(&r, None);
    assert_eq!(w.root_state(&honest.cid, 1).map(|r| r.root), Some(honest.vault.root(1)));
}

/// C-1 (e): a claim names the burn record of a pending burn as its relayer; the burn still succeeds.
#[test]
fn c1_e_a_claim_cannot_block_a_burn() {
    require_dev_mode();
    let mut w = World::new();
    let program = w.program;
    let payer_id = *w.chain.payer.value();
    let payer = w.chain.payer;
    let honest = vault(&mut w, HONEST, controls(), &[payer_id], 9_200_041);
    let att = vault(&mut w, ATTACKER, controls(), &[*fee_recipient().value()], 9_200_042);
    open(&mut w, "attacker", &att);
    open(&mut w, "H", &honest);
    let witness = honest.vault.witness(&honest.deposits[0], 1);
    expect(&w.claim("the payer's claim (0.1 ETH)", &honest.cid, 1, &HONEST, witness), None);
    let note = Note {
        r: [0x4e; 32],
        memo: [0x4d; 20],
        mac: [0x4c; 16],
        token: ETH,
        amount: word_of_u128(DENOMINATIONS[0]),
        c_out: [0x4b; 32],
    };
    let target = burn_actor(program, &honest.cid, &note.r);
    let r = attack_claim(
        &mut w,
        "attacker's claim: relayer = the pending burn's record",
        &att,
        0,
        Some(*target.account_id.value()),
        &[burn_seed(&honest.cid, &note.r)],
    );
    expect(&r, None);
    assert!(is_empty(&w, target), "the claim wrote nothing at the burn record");
    // The payer's burn: a top-level call it signs at its own actor of the program.
    let own = Actor::new(payer, program);
    let mut actors = vec![own, holder_of(program, &honest.cid, payer)];
    actors.extend(w.burn_actors(own, &honest.cid, &note).into_iter().skip(1));
    let mut unique = Vec::new();
    for a in actors {
        if !unique.contains(&a) {
            unique.push(a);
        }
    }
    let r = w.send("the payer's burn after the attacker's claim", own, &BridgeMessage::burn(honest.cid, &note), unique);
    expect(&r, None);
    assert!(matches!(borsh::from_slice::<BridgeState>(&w.chain.actor_state(target)).unwrap(), BridgeState::Burn(_)));
    assert_eq!(w.balance_of(payer, &honest.cid, &ETH), 0, "the denomination is burned");
}

/// H-1, nested call: LEZ's `scripted` program forwards an `ImportRoot` to `config(cid)` (the block
/// shows only a call to `scripted`); the bridge refuses it, and the visible import of the same count
/// afterwards succeeds.
#[test]
fn h1_a_nested_import_is_refused() {
    require_dev_mode();
    let mut w = World::new();
    let program = w.program;
    let payer_id = *w.chain.payer.value();
    let honest = vault(&mut w, HONEST, controls(), &[payer_id], 9_200_051);
    expect(&w.init("Init H", &honest.config, honest.block), None);
    let b = &w.blocks[honest.block];
    let message = BridgeMessage::ImportRoot {
        config_id: honest.cid,
        record_slot: b.slot,
        root: honest.vault.root(1),
        leaf_count: 1,
        proof: b.import_proof(&HONEST),
    };
    let config = config_actor(program, &honest.cid);
    let header = ferrata_lez_wire::header_actor(harness::ferrata_account(), &INSTANCE, b.slot);
    let root = root_actor(program, &honest.cid, 1);
    let seed_pda = PdaSeed::new([0x51; 32]);
    let forwarder =
        Actor::new(AccountId::for_public_pda(&harness::scripted_account(), &seed_pda), harness::scripted_account());
    let script = Script::default().send_sync(config, &message);
    let r = w.chain.submit(
        "ImportRoot as a nested call from another program",
        forwarder,
        borsh::to_vec(&script).unwrap(),
        vec![forwarder, config, header, root],
        vec![
            PublicAccountEvidence::Pda { program: harness::scripted_account(), seed: seed_pda },
            PublicAccountEvidence::Pda { program, seed: root_seed(&honest.cid, 1) },
        ],
        w.time,
    );
    expect(&r, Some(code::NOT_AUTHORIZED));
    assert!(is_empty(&w, root), "no root was written by the hidden import");
    let r = w.import("ImportRoot H n=1, top-level", &honest.cid, honest.block, honest.vault.root(1), 1, &HONEST);
    expect(&r, None);
}

/// H-1, a delivery with a sender (how LEZ presents an asynchronous message: `from` is the sending
/// actor), run in the executor: refused before any attestation is asked.
#[test]
fn h1_b_import_with_a_sender_is_refused() {
    require_dev_mode();
    let mut w = World::new();
    let program = w.program;
    let payer_id = *w.chain.payer.value();
    let honest = vault(&mut w, HONEST, controls(), &[payer_id], 9_200_061);
    expect(&w.init("Init H", &honest.config, honest.block), None);
    let b = &w.blocks[honest.block];
    let message = BridgeMessage::ImportRoot {
        config_id: honest.cid,
        record_slot: b.slot,
        root: honest.vault.root(1),
        leaf_count: 1,
        proof: b.import_proof(&HONEST),
    };
    let config = config_actor(program, &honest.cid);
    let sender = Actor::new(AccountId::new([0x5e; 32]), harness::scripted_account());
    let elf = bridge_program();
    for (what, from) in [("ImportRoot delivered with a sender", Some(sender)), ("ImportRoot, top-level", None)] {
        let input = ReceiveInput {
            receiver: config,
            from,
            is_authorized: false,
            pre_state: w.chain.actor_state(config),
            message: borsh::to_vec(&message).unwrap(),
        };
        let executed = harness::attest::execute(what, elf.elf(), &input);
        let want = if from.is_some() { u32::from(code::NOT_AUTHORIZED) } else { 0 };
        assert_eq!(executed.exit_code, want, "{what}");
    }
}

/// C-1, the receiving side, in the executor: a `Mint` from the right nullifier is accepted only at
/// `holder(account)` of the account it names; at that account itself (here `config(cid)`, a PDA of
/// the program) it is refused.
#[test]
fn c1_f_a_mint_is_accepted_only_at_its_holder() {
    require_dev_mode();
    let w = World::new();
    let program = w.program;
    let (vault, n) = ([0xa3; 20], [0x6e; 32]);
    let cid = cid_of(&w, HONEST, controls());
    let from = nullifier_of(program, &cid, CHAIN_ID, &vault, &n);
    let target = config_actor(program, &cid);
    let named = *target.account_id.value();
    let mint = BridgeMessage::Mint {
        nullifier: n,
        config_id: cid,
        evm_chain_id: CHAIN_ID,
        vault,
        token: ETH,
        holder: named,
        amount: 1,
    };
    let elf = bridge_program();
    for (what, receiver, want) in [
        ("Mint naming config(cid), delivered at config(cid)", target, u32::from(code::WRONG_SENDER)),
        ("Mint naming config(cid), delivered at holder(config(cid))", holder_of(program, &cid, target.account_id), 0),
    ] {
        let input = ReceiveInput {
            receiver,
            from: Some(from),
            is_authorized: true,
            pre_state: Default::default(),
            message: borsh::to_vec(&mint).unwrap(),
        };
        assert_eq!(harness::attest::execute(what, elf.elf(), &input).exit_code, want, "{what}");
    }
}

const F1_VAULT: [u8; 20] = [0xc5; 20];

/// F1: `Init` of a configuration whose freeze authority is its admin, against a vault whose slot 2 holds that
/// configuration's id (BR-INIT-05): refused with 240 (`BAD_MESSAGE`), nothing written (the program before B1 applied it,
/// and its live freeze authority was the admin). `ProposeRoles` with the admin as a member is refused with 240 too
/// (BR-CTL-13).
#[test]
fn f1_init_refuses_the_admin_as_freeze_authority() {
    require_dev_mode();
    let mut w = World::new();
    let (program, admin) = (w.program, controls().admin);
    let payer_id = *w.chain.payer.value();
    let bad = vault(&mut w, F1_VAULT, Controls { freeze_authority: admin, ..controls() }, &[payer_id], 9_200_010);
    let init = w.init("Init: controls.freeze_authority = controls.admin", &bad.config, bad.block);
    println!(
        "F1: Init with the admin as freeze authority: ok {}, exit {:?}, execution gas {}",
        init.ok(),
        init.exit_code,
        init.charge.gas_exec
    );
    expect(&init, Some(code::BAD_MESSAGE));
    assert!(w.config_state(&bad.cid).is_none(), "the refused Init wrote nothing at config(cid)");
    // The code ProposeRoles gives the admin as a member, on configuration A (distinct roles, paired as always).
    let (a, cid_a) = (w.config_a.clone(), w.cid_a);
    expect(&w.init("Init A", &a, 0), None);
    let propose = BridgeMessage::ProposeRoles {
        config_id: cid_a,
        freeze_authority: FreezeAuthority { members: vec![admin, controls().freeze_authority], threshold: 1 },
        watchers: vec![],
        record: w.blocks[2].time_record(),
    };
    let r = w.role("ProposeRoles with the admin as a member", ADMIN_SEED, &propose, config_actor(program, &cid_a));
    expect(&r, Some(code::BAD_MESSAGE));
    assert_eq!(init.exit_code, r.exit_code, "Init refuses with ProposeRoles' code");
}
