//! Phase 2 of the blind red team of 18 November 2026 (`vectors/redteam-2026-11-18/`): every vector of
//! area `bridge` (BR-*) run through the bridge program on LEZ v0.4's state machine, on the synthetic
//! vaults of `harness::bridge` (RISC Zero dev mode for the private claims). Each vector builds its own
//! world (its setup), applies its one mutation and observes the decisive transaction; the shared judge
//! (`redteam_common`) compares that with the vector's expectation. A vector's stated effects
//! (`effects: …`) are checked after an accepted transaction (`EffectMismatch` if one does not hold).
//! One test per group; each writes `lez/results/redteam-2026-11-18/redteam_bridge.<group>.json`.
//!
//! Run: `RISC0_DEV_MODE=1 cargo test -p harness --locked --test redteam_bridge -- --nocapture` (in `lez/`).

mod redteam_common;

use std::fmt::Debug;

use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{deposit_id, DestKind},
    outbound::{burn_commitment, burn_secret, burn_tag, xor_memo},
    sha256, BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{
    burn_actor, burn_mac_v2, config_actor, config_seed, fee_slot, holder_actor, holder_seed, nullifier_actor,
    nullifier_seed, own_actor, registry_slot, root_actor, root_seed, token_actor, token_seed, word_of_u128,
    BridgeConfig, BridgeMessage, BridgeState, BurnRecord, CapParams, ClaimWitness, ConfigState, Controls,
    CosignedAction, FeeSchedule, FreezeAuthority, HolderState, Holding, Note, NullifierState, RootState,
    ScheduleRange, TimeRecord, TokenEntry, HEADER_MARKER, MAX_FREEZE_MS, SLOT_RETIRED,
};
use ferrata_lez_wire::{header_actor, header_seed, RecordState};
use ferrata_spectests::execkit::header_rlp;
use harness::{
    bridge::{
        catalog, config, controls, eth_entry, eth_schedule_one, fee_recipient, private_claim_transitions,
        private_keys, private_post_state, registry_slots, relayer, role_account, role_key, schedule_zero, t3_entry,
        Block, Deposit, Vault, VaultSlots, World, ADMIN_SEED, ANNOUNCED_1, CHAIN_ID, DENOMINATIONS, ETH, FLOOR,
        FREEZE_SEED, INSTANCE, LEZ_UNIT, MINT_FEES_1, OUTSIDER_SEED, RELAY_FEES, T3, TOKEN_CAP, VAULT_B,
    },
    ferrata_account, private::PrivateRun, scripted_account, settle::LeeTransaction, TxReport,
};
use lee::{Account, AccountId, Actor, PublicAccountEvidence};
use lee_core::{account::ActorState, program::PdaSeed};
use redteam_common::*;
use serde_json::Value;
use test_guest_core::Script;

const FILE: &str = "redteam_bridge";
const RESERVE: u128 = 100_000_000;
const HOUR_MS: u64 = 3_600_000;
const DAY_S: u64 = 86_400;
const DAY_MS: u64 = DAY_S * 1000;
const D0: u128 = DENOMINATIONS[0];
const D1: u128 = DENOMINATIONS[1];
const D2: u128 = DENOMINATIONS[2];

/// Role accounts of these tests (keys `[seed; 32]`): a holder H and another account X, three members
/// and two watchers of a handed-over freeze authority, the admin and freeze authority of a tenant B'.
const H: u8 = 0x71;
const X: u8 = 0x72;
const M1: u8 = 0x73;
const M2: u8 = 0x74;
const M3: u8 = 0x75;
const WATCHER: u8 = 0x76;
const W2: u8 = 0x77;
const ADMIN_B: u8 = 0x78;
const FA_B: u8 = 0x79;
const EXTRA: [u8; 9] = [H, X, M1, M2, M3, WATCHER, W2, ADMIN_B, FA_B];
/// A token no configuration registered.
const U: [u8; 20] = [0x55; 20];

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn key(seed: u8) -> Hash {
    *role_account(seed).value()
}

/// A setup step that must apply (else the test's own setup is wrong: a harness error).
fn ok(r: &TxReport) {
    assert!(r.ok(), "setup step {}: {:?} (exit {:?})", r.label, r.charge.revert, r.exit_code);
}

fn want<T: PartialEq + Debug>(f: &mut Vec<String>, what: &str, got: T, expected: T) {
    if got != expected {
        f.push(format!("{what}: expected {expected:?}, found {got:?}"));
    }
}

/// The observation of `rep`; if it applied, `effects` lists the stated effects that do not hold.
fn observe(w: &World, rep: &TxReport, note: &str, effects: impl FnOnce(&World) -> Vec<String>) -> Observed {
    if !rep.ok() {
        return Observed::of_report(rep, note);
    }
    let failed = effects(w);
    if failed.is_empty() {
        Observed::of_report(rep, note)
    } else {
        Observed::EffectMismatch { note: format!("{note} [{}]: {}", rep.label, failed.join("; ")) }
    }
}

fn occurrences(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return vec![];
    }
    hay.windows(needle.len()).enumerate().filter(|(_, w)| *w == needle).map(|(i, _)| i).collect()
}

/// B-BR: `World::with_roots` (A and B paired, ETH registered on both and T2 on B, root(A, 5), root(A, 8),
/// root(A, 6), root(B, 8), ETH's schedule 1 of A from leaf 6, closed at 8 leaves) and this file's role
/// accounts.
fn base() -> World {
    let mut w = World::with_roots();
    for seed in EXTRA {
        w.chain.state.force_insert_account(role_account(seed), Account::default());
    }
    w
}

/// A world with only A paired, ETH registered and root(A, 5) imported (block 100): the baseline of the
/// import vectors, whose decisive import is block 101's (8 leaves), new to it.
fn base_n5() -> World {
    let mut w = World::new();
    let a = w.config_a.clone();
    let (cid, va) = (w.cid_a, w.vault_a.address);
    ok(&w.init("setup: Init A", &a, 0));
    ok(&w.register("setup: Register ETH (A)", &cid, 0, &eth_entry(), &va));
    let root5 = w.vault_a.root(5);
    ok(&w.import("setup: ImportRoot A n=5", &cid, 0, root5, 5, &va));
    w
}

/// Add a synthetic block holding `slots`, after every recorded slot, timestamped behind the LEZ clock.
fn add(w: &mut World, slots: VaultSlots) -> usize {
    let slot = w.next_slot();
    let number = 103 + w.blocks.len() as u64;
    w.add_block(Block::new(slot, number, &[slots]))
}

/// A vault at `address` that holds only slot 2 = the id of `config(address, ctl)` (and ETH's registry);
/// returns that configuration, its id and the block.
fn paired_vault(w: &mut World, address: [u8; 20], ctl: Controls) -> (BridgeConfig, Hash, usize) {
    let c = config(address, ctl);
    let cid = c.id(w.program);
    let slots = VaultSlots { address, root: [0; 32], count: 0, config_id: cid, extra: registry_slots(&eth_entry(), &[]) };
    (c, cid, add(w, slots))
}

fn public(leaf_index: u64, depositor: [u8; 20], token: [u8; 20], amount: u128, s: Hash, dest: Hash) -> Deposit {
    Deposit { leaf_index, depositor, token, amount: word_of_u128(amount), s, kind: DestKind::Public, dest }
}

/// Vault A after more deposits (leaf indices 8, 9, …): a new block holding A's slots at that count, with
/// ETH's entry, schedule 0 and schedule 1 as blocks 101 and 102 hold them. Returns the block and the vault.
fn extend_a(w: &mut World, more: &[Deposit]) -> (usize, Vault) {
    let mut leaves = w.vault_a.leaves.clone();
    for d in more {
        assert_eq!(leaves.len() as u64, d.leaf_index);
        leaves.push(d.leaf());
    }
    let vault = Vault { address: w.vault_a.address, leaves };
    let n = vault.leaves.len() as u64;
    let extra = registry_slots(&eth_entry(), &[eth_schedule_one()]);
    let slots = VaultSlots { address: vault.address, root: vault.root(n), count: n, config_id: w.cid_a, extra };
    (add(w, slots), vault)
}

/// Import the extended vault's root and extend ETH's schedules to its count (no schedule 2 there).
fn open_extended(w: &mut World, i: usize, vault: &Vault) -> u64 {
    let n = vault.leaves.len() as u64;
    let (cid, va) = (w.cid_a, w.vault_a.address);
    ok(&w.import(&format!("setup: ImportRoot A n={n}"), &cid, i, vault.root(n), n, &va));
    ok(&w.import_fees(&format!("setup: ImportFees ETH (A): no schedule 2 at {n} leaves"), &cid, i, ETH, 2, None, &va));
    n
}

/// The base claim: A's leaf 6 (10 ETH to the payer, public kind, fee schedule 1) under root(A, 8).
fn leaf6(w: &World) -> ClaimWitness {
    w.vault_a.witness(&w.payer_deposit.clone(), 8)
}

fn range6(w: &World) -> ScheduleRange {
    w.range(&w.cid_a, &ETH, 6)
}

/// The base claim with `witness` and `range` (the mutation is in them), relayer R.
fn claim_a8(w: &mut World, label: &str, witness: ClaimWitness, range: ScheduleRange) -> TxReport {
    let (cid, va) = (w.cid_a, w.vault_a.address);
    w.claim_with(label, &cid, 8, &va, witness, range, Some(*relayer().value()))
}

fn claim_leaf2(w: &mut World, label: &str) -> TxReport {
    let (cid, va) = (w.cid_a, w.vault_a.address);
    let wit = w.vault_a.witness(&w.vector.clone(), 5);
    w.claim(label, &cid, 5, &va, wit)
}

fn claim_payer_10(w: &mut World) {
    let (cid, va) = (w.cid_a, w.vault_a.address);
    let wit = leaf6(w);
    ok(&w.claim("setup: Claim A's leaf 6 (10 ETH to the payer)", &cid, 8, &va, wit));
}

/// The payer moves `amount` of its ETH under `cid` to holder(role account `to`).
fn give(w: &mut World, cid: &Hash, to: u8, amount: u128) {
    ok(&w.transfer(&format!("setup: Transfer {amount} to holder({to:#x})"), cid, ETH, amount, role_account(to)));
}

fn note_of(i: u8, amount: u128) -> Note {
    Note { r: [i; 32], memo: [i; 20], mac: [i; 16], token: ETH, amount: word_of_u128(amount), c_out: [i; 32] }
}

/// A public holder's burn: a top-level call at its own actor, signed by `signer`.
fn burn_at(w: &mut World, label: &str, holder: u8, signer: u8, cid: Hash, note: &Note) -> TxReport {
    let account = role_account(holder);
    let at = own_actor(w.program, account);
    let actors = w.public_burn_actors(account, &cid, note);
    w.submit_signed(label, at, &BridgeMessage::burn(cid, note), actors, vec![], &[&role_key(signer)])
}

fn freeze(cid: Hash, token: Option<[u8; 20]>, mints: bool, burns: bool, until_ms: u64, record: TimeRecord) -> BridgeMessage {
    BridgeMessage::Freeze { config_id: cid, token, mints, burns, until_ms, record }
}

fn set_cap(cid: Hash, token: Option<[u8; 20]>, mints: bool, cap: CapParams) -> BridgeMessage {
    BridgeMessage::SetCap { config_id: cid, token, mints, cap }
}

fn config_gate(w: &World, cid: &Hash) -> Option<ferrata_lez_bridge_wire::Gate> {
    match w.config_state(cid) {
        Some(ConfigState::Ready { gate, .. }) => Some(gate),
        _ => None,
    }
}

/// The admin proposes `fa` and `watchers` (block 102's record), and after a record 14 days later anyone
/// applies them. Returns that record's block.
fn handover(w: &mut World, fa: FreezeAuthority, watchers: Vec<Hash>) -> usize {
    let (cid, program) = (w.cid_a, w.program);
    let t0 = w.blocks[2].time_ms();
    let propose =
        BridgeMessage::ProposeRoles { config_id: cid, freeze_authority: fa, watchers, record: w.blocks[2].time_record() };
    ok(&w.role("setup: ProposeRoles (admin)", ADMIN_SEED, &propose, config_actor(program, &cid)));
    let day14 = w.finalize(t0 / 1000 + 14 * DAY_S);
    ok(&w.apply_roles("setup: ApplyRoles with a record 14 days later", &cid, day14));
    day14
}

/// The actors of a plaintext claim, as `World::claim_with` lists them.
fn claim_actors_of(w: &World, cid: &Hash, n: u64, vault: &[u8; 20], wit: &ClaimWitness, relayer: Option<Hash>) -> Vec<Actor> {
    let n_d = Deposit::of(wit).nullifier(CHAIN_ID, vault);
    let mut actors = vec![root_actor(w.program, cid, n)];
    let nullifier = nullifier_actor(w.program, CHAIN_ID, vault, &n_d);
    actors.extend(w.claim_actors(cid, &wit.token, nullifier, Some(AccountId::new(wit.dest)), relayer));
    actors
}

fn snapshot(w: &World, actors: &[Actor]) -> Vec<Vec<u8>> {
    actors.iter().map(|a| w.chain.actor_state(*a).to_vec()).collect()
}

/// A private run's observation: accepted, LEZ's revert, or (not provable) the exit code of the claim's
/// private transitions in the executor.
fn private_observed(
    w: &World,
    account: AccountId,
    message: &BridgeMessage,
    cid: &Hash,
    n: u64,
    res: &Result<PrivateRun, String>,
    note: &str,
) -> Observed {
    match res {
        Ok(run) => match &run.charge.revert {
            None => Observed::Accepted { note: format!("{note} (private transaction settled)") },
            Some(revert) => Observed::Lez { revert: revert.clone(), note: note.to_string() },
        },
        Err(err) => {
            let exits: Vec<u32> =
                private_claim_transitions(w, account, message, cid, n).iter().map(|e| e.exit_code).collect();
            let note = format!("{note}; private transitions in the executor exit {exits:?}; not provable: {err}");
            match exits.iter().find(|c| **c != 0) {
                Some(code) => Observed::Code { code: *code, note },
                None => Observed::Lez { revert: err.clone(), note },
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pairing (Init)
// ---------------------------------------------------------------------------------------------

#[test]
fn init() {
    let mut r = Results::new(FILE, "init");
    // BR-INIT-01: A' (A with another fee recipient, so another salt) against vault A, whose slot 2 holds configId(A).
    r.run("BR-INIT-01", || {
        let mut w = World::new();
        let a2 = w.config_a2.clone();
        let rep = w.init("Init A' (another salt) against vault A (slot 2 = configId(A))", &a2, 0);
        Observed::of_report(&rep, "before any Init; block 100's record")
    });
    // BR-INIT-02: A's Init sent again after B-BR.
    r.run("BR-INIT-02", || {
        let mut w = base();
        let a = w.config_a.clone();
        let rep = w.init("Init A again", &a, 0);
        Observed::of_report(&rep, "B-BR, then A's Init again")
    });
    // BR-INIT-03: one byte of a node of the slot-2 storage proof flipped (the last byte of its last node, in
    // the value configId(A)).
    r.run("BR-INIT-03", || {
        let mut w = World::new();
        let (a, cid, program) = (w.config_a.clone(), w.cid_a, w.program);
        let slot = w.blocks[0].slot;
        let mut proof = w.blocks[0].pairing_proof(&a.vault);
        let last = proof.config_slot.len() - 1;
        let k = proof.config_slot[last].len() - 1;
        proof.config_slot[last][k] ^= 0x01;
        let message = BridgeMessage::Init { config: a.clone(), record_slot: slot, proof };
        let at = config_actor(program, &cid);
        let header = header_actor(ferrata_account(), &INSTANCE, slot);
        let evidence = vec![PublicAccountEvidence::Pda { program, seed: config_seed(&cid) }];
        let rep = w.submit("Init A, slot-2 proof tampered", at, &message, vec![at, header], evidence);
        Observed::of_report(&rep, &format!("byte {k} of node {last} (of {}) of the slot-2 proof flipped", last + 1))
    });
    // BR-INIT-04: A's Init delivered at config(cid of A'), not config(configId(A)).
    r.run("BR-INIT-04", || {
        let mut w = World::new();
        let (a, other, program) = (w.config_a.clone(), w.cid_a2, w.program);
        let slot = w.blocks[0].slot;
        let message = BridgeMessage::Init { config: a.clone(), record_slot: slot, proof: w.blocks[0].pairing_proof(&a.vault) };
        let at = config_actor(program, &other);
        let header = header_actor(ferrata_account(), &INSTANCE, slot);
        let evidence = vec![PublicAccountEvidence::Pda { program, seed: config_seed(&other) }];
        let rep = w.submit("Init A sent to config(configId(A'))", at, &message, vec![at, header], evidence);
        Observed::of_report(&rep, "A's Init at config(cid_a2)")
    });
    // BR-INIT-05: controls whose freeze authority is the admin, against a vault whose slot 2 holds their id.
    r.run("BR-INIT-05", || {
        let mut w = World::new();
        let bad = Controls { freeze_authority: controls().admin, ..controls() };
        let (c, cid, i) = paired_vault(&mut w, [0xc5; 20], bad);
        let rep = w.init("Init: controls.freeze_authority = controls.admin", &c, i);
        let mut note = String::from("vault 0xc5..c5 whose slot 2 holds this configuration's id");
        if let Some(ConfigState::Ready { roles, .. }) = w.config_state(&cid) {
            note += &format!(
                "; written: live freeze authority {:?} contains the admin: {}",
                roles.freeze_authority.threshold,
                roles.freeze_authority.members.contains(&controls().admin)
            );
        }
        Observed::of_report(&rep, &note)
    });
    // BR-INIT-06: token cap one unit below the floor, against a vault whose slot 2 holds this id.
    r.run("BR-INIT-06", || {
        let mut w = World::new();
        let low = Controls { token_cap: CapParams { capacity: FLOOR.capacity - 1, ..TOKEN_CAP }, ..controls() };
        let (c, _, i) = paired_vault(&mut w, [0xc6; 20], low);
        let rep = w.init("Init: token_cap.capacity = floor.capacity - 1", &c, i);
        Observed::of_report(&rep, "vault 0xc6..c6 whose slot 2 holds this configuration's id")
    });
    // BR-INIT-07: B-BR; configuration A' (vault A, another salt) against vault A's real record.
    r.run("BR-INIT-07", || {
        let mut w = base();
        let a2 = w.config_a2.clone();
        let rep = w.init("Init A' against vault A's record of block 101 (slot 2 = configId(A))", &a2, 1);
        Observed::of_report(&rep, "B-BR (A paired); A' = A with another fee recipient")
    });
    r.finish(&["bridge"], &["BR-INIT-"]);
}

// ---------------------------------------------------------------------------------------------
// ImportRoot
// ---------------------------------------------------------------------------------------------

#[test]
fn imp() {
    let mut r = Results::new(FILE, "imp");
    let note = "baseline: A paired, ETH registered, root(A, 5) imported; decisive import: block 101 (8 leaves)";
    // BR-IMP-01: root = slot 0's value with one bit flipped, true proofs.
    r.run("BR-IMP-01", || {
        let mut w = base_n5();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        let mut root = w.vault_a.root(8);
        root[31] ^= 1;
        let rep = w.import("ImportRoot A at block 101, root with one bit flipped", &cid, 1, root, 8, &va);
        Observed::of_report(&rep, note)
    });
    // BR-IMP-02: leaf_count = slot 1's value + 1.
    r.run("BR-IMP-02", || {
        let mut w = base_n5();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        let root = w.vault_a.root(8);
        let rep = w.import("ImportRoot A at block 101, leaf_count 9 (slot 1 holds 8)", &cid, 1, root, 9, &va);
        Observed::of_report(&rep, note)
    });
    // BR-IMP-03: root(A, 5) imported again.
    r.run("BR-IMP-03", || {
        let mut w = base_n5();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        let root = w.vault_a.root(5);
        let rep = w.import("ImportRoot A n=5 again", &cid, 0, root, 5, &va);
        Observed::of_report(&rep, "root(A, 5) already imported")
    });
    // BR-IMP-04: ImportRoot for configId(A) before Init.
    r.run("BR-IMP-04", || {
        let mut w = World::new();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        let root = w.vault_a.root(5);
        let rep = w.import("ImportRoot A n=5 before Init", &cid, 0, root, 5, &va);
        Observed::of_report(&rep, "no Init")
    });
    // BR-IMP-05: Sunset executed (freeze authority), then the import of a newer root.
    r.run("BR-IMP-05", || {
        let mut w = base_n5();
        let (cid, va, program) = (w.cid_a, w.vault_a.address, w.program);
        ok(&w.role("setup: Sunset A (freeze authority)", FREEZE_SEED, &BridgeMessage::Sunset { config_id: cid }, config_actor(program, &cid)));
        let root = w.vault_a.root(8);
        let rep = w.import("ImportRoot A at block 101 after Sunset", &cid, 1, root, 8, &va);
        Observed::of_report(&rep, note)
    });
    // BR-IMP-06: one byte of a node of the slot-0 proof flipped (the last byte of its last node: the root).
    r.run("BR-IMP-06", || {
        let mut w = base_n5();
        let (cid, va, program) = (w.cid_a, w.vault_a.address, w.program);
        let slot = w.blocks[1].slot;
        let mut proof = w.blocks[1].import_proof(&va);
        let last = proof.root_slot.len() - 1;
        let k = proof.root_slot[last].len() - 1;
        proof.root_slot[last][k] ^= 0x01;
        let message =
            BridgeMessage::ImportRoot { config_id: cid, record_slot: slot, root: w.vault_a.root(8), leaf_count: 8, proof };
        let at = config_actor(program, &cid);
        let header = header_actor(ferrata_account(), &INSTANCE, slot);
        let rep = w.submit("ImportRoot A at block 101, slot-0 proof tampered", at, &message, vec![at, header, root_actor(program, &cid, 8)], vec![]);
        Observed::of_report(&rep, &format!("{note}; byte {k} of node {last} of the slot-0 proof flipped"))
    });
    // BR-IMP-07: LEZ's `scripted` program forwards the import to config(A) from its own handler.
    r.run("BR-IMP-07", || {
        let mut w = base_n5();
        let (cid, va, program) = (w.cid_a, w.vault_a.address, w.program);
        let b = &w.blocks[1];
        let message = BridgeMessage::ImportRoot {
            config_id: cid,
            record_slot: b.slot,
            root: w.vault_a.root(8),
            leaf_count: 8,
            proof: b.import_proof(&va),
        };
        let at = config_actor(program, &cid);
        let header = header_actor(ferrata_account(), &INSTANCE, b.slot);
        let root = root_actor(program, &cid, 8);
        let seed = PdaSeed::new([0x51; 32]);
        let forwarder = Actor::new(AccountId::for_public_pda(&scripted_account(), &seed), scripted_account());
        let script = Script::default().send_sync(at, &message);
        let time = w.time;
        let rep = w.chain.submit(
            "ImportRoot as a nested call from another program",
            forwarder,
            borsh::to_vec(&script).unwrap(),
            vec![forwarder, at, header, root],
            vec![
                PublicAccountEvidence::Pda { program: scripted_account(), seed },
                PublicAccountEvidence::Pda { program, seed: root_seed(&cid, 8) },
            ],
            time,
        );
        Observed::of_report(&rep, note)
    });
    // BR-IMP-08: record_slot = 1, a slot the instance never recorded (block 101's true proofs).
    r.run("BR-IMP-08", || {
        let mut w = base_n5();
        let (cid, va, program) = (w.cid_a, w.vault_a.address, w.program);
        let message = BridgeMessage::ImportRoot {
            config_id: cid,
            record_slot: 1,
            root: w.vault_a.root(8),
            leaf_count: 8,
            proof: w.blocks[1].import_proof(&va),
        };
        let at = config_actor(program, &cid);
        let empty = header_actor(ferrata_account(), &INSTANCE, 1);
        let evidence = vec![PublicAccountEvidence::Pda { program: ferrata_account(), seed: header_seed(&INSTANCE, 1) }];
        let rep = w.submit("ImportRoot A naming slot 1 (no record)", at, &message, vec![at, empty, root_actor(program, &cid, 8)], evidence);
        Observed::of_report(&rep, "admission evidence for the empty header(instance, 1) supplied")
    });
    // BR-IMP-09: root(A, 7) imported from a newer record, then root(A, 6) from an older one.
    r.run("BR-IMP-09", || {
        let mut w = base_n5();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        let at = |w: &mut World, n: u64| {
            let slots = VaultSlots { address: va, root: w.vault_a.root(n), count: n, config_id: cid, extra: registry_slots(&eth_entry(), &[]) };
            add(w, slots)
        };
        let older = at(&mut w, 6);
        let newer = at(&mut w, 7);
        let (root6, root7) = (w.vault_a.root(6), w.vault_a.root(7));
        ok(&w.import("setup: ImportRoot A n=7 (newer record)", &cid, newer, root7, 7, &va));
        let before7 = w.root_state(&cid, 7);
        let rep = w.import("ImportRoot A n=6 under an older record, after n=7", &cid, older, root6, 6, &va);
        observe(&w, &rep, "n = 5; n+1 = 6 (older record), n+2 = 7 (newer record)", |w| {
            let mut f = Vec::new();
            want(&mut f, "root(A, 6)", w.root_state(&cid, 6).map(|s| (s.root, s.leaf_count)), Some((root6, 6)));
            want(&mut f, "root(A, 7) unchanged", w.root_state(&cid, 7), before7);
            f
        })
    });
    // BR-IMP-10: a Gloas-layout record; the import carries another (the parent) block's header in front of
    // the account proof.
    r.run("BR-IMP-10", || {
        const VG: [u8; 20] = [0x3c; 20];
        let mut w = World::new();
        let program = w.program;
        let c = config(VG, controls());
        let cid = c.id(program);
        let vault = Vault { address: VG, leaves: vec![[0xc1; 32], [0xc2; 32]] };
        let slots = |n: u64| VaultSlots { address: VG, root: vault.root(n), count: n, config_id: cid, extra: registry_slots(&eth_entry(), &[]) };
        let s1 = w.next_slot();
        let g1 = w.add_block(Block::gloas(s1, &[slots(1)]));
        let s2 = w.next_slot();
        let g2 = w.add_block(Block::gloas(s2, &[slots(2)]));
        ok(&w.init("setup: Init G (Gloas record, header in front)", &c, g1));
        ok(&w.register("setup: Register ETH (G)", &cid, g1, &eth_entry(), &VG));
        ok(&w.import("setup: ImportRoot G n=1", &cid, g1, vault.root(1), 1, &VG));
        let served: Value = serde_json::from_slice(&harness::sepolia::gloas_file("block.json")).unwrap();
        let mut parent = served["result"].clone();
        let hex_u64 = |v: &Value| u64::from_str_radix(v.as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
        let (number, time) = (hex_u64(&parent["number"]), hex_u64(&parent["timestamp"]));
        parent["number"] = Value::String(format!("{:#x}", number - 1));
        parent["timestamp"] = Value::String(format!("{:#x}", time - 12));
        parent["parentHash"] = Value::String(format!("0x{}", hex::encode([0x9a; 32])));
        parent["stateRoot"] = Value::String(format!("0x{}", hex::encode(w.blocks[g2].state.root())));
        let parent_header = header_rlp(&parent);
        let mut proof = w.blocks[g2].import_proof(&VG);
        assert_eq!(proof.account[0], HEADER_MARKER.to_vec(), "a Gloas proof carries the header");
        proof.account[1] = parent_header;
        let message = BridgeMessage::ImportRoot { config_id: cid, record_slot: s2, root: vault.root(2), leaf_count: 2, proof };
        let at = config_actor(program, &cid);
        let header = header_actor(ferrata_account(), &INSTANCE, s2);
        let rep = w.submit("ImportRoot G n=2 with the parent block's header", at, &message, vec![at, header, root_actor(program, &cid, 2)], vec![]);
        Observed::of_report(
            &rep,
            "B-BR on vault G under Gloas records (root(G, 1) imported); the parent header: the served block's \
             fields with number - 1, timestamp - 12, another parentHash, and the state root the proofs verify against",
        )
    });
    r.finish(&["bridge"], &["BR-IMP-"]);
}

// ---------------------------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------------------------

#[test]
fn clm() {
    let mut r = Results::new(FILE, "clm");
    let base_note = "the base claim: A's leaf 6 (10 ETH, public kind, schedule 1) under root(A, 8), relayer R";
    // BR-CLM-01: the base claim.
    r.run("BR-CLM-01", || {
        let mut w = base();
        let (cid, va, payer) = (w.cid_a, w.vault_a.address, w.chain.payer);
        let n = w.payer_deposit.nullifier(CHAIN_ID, &va);
        let before = (w.balance(payer, &cid), w.balance(fee_recipient(), &cid), w.balance(relayer(), &cid));
        let (wit, range) = (leaf6(&w), range6(&w));
        let rep = claim_a8(&mut w, "Claim A's leaf 6 under root(A, 8)", wit, range);
        observe(&w, &rep, base_note, |w| {
            let mut f = Vec::new();
            want(&mut f, "holder(dest) +denomination", w.balance(payer, &cid) - before.0, D2);
            want(&mut f, "fee recipient +mint fee", w.balance(fee_recipient(), &cid) - before.1, MINT_FEES_1[2]);
            want(&mut f, "R +relay fee", w.balance(relayer(), &cid) - before.2, RELAY_FEES[2]);
            want(&mut f, "nullifier(chain, V, N)", w.nullifier(&cid, &va, &n), Some(NullifierState { config_id: cid, token: ETH, leaf_count: 8, amount: D2 }));
            f
        })
    });
    // BR-CLM-02: the same claim again.
    r.run("BR-CLM-02", || {
        let mut w = base();
        let (wit, range) = (leaf6(&w), range6(&w));
        ok(&claim_a8(&mut w, "setup: the base claim", wit.clone(), range));
        let rep = claim_a8(&mut w, "the same Claim again", wit, range);
        Observed::of_report(&rep, base_note)
    });
    // BR-CLM-03: a public deposit at leaf 8, made after root(A, 8)'s record, claimed against root(A, 8).
    r.run("BR-CLM-03", || {
        let mut w = base();
        let (cid, va, payer) = (w.cid_a, w.vault_a.address, *w.chain.payer.value());
        let d = public(8, [0xc3; 20], ETH, D0, [0x03; 32], payer);
        let (i, ext) = extend_a(&mut w, std::slice::from_ref(&d));
        ok(&w.import_fees("setup: ImportFees ETH (A): no schedule 2 at 9 leaves", &cid, i, ETH, 2, None, &va));
        let range = w.range(&cid, &ETH, 8);
        let rep = w.claim_with("Claim leaf 8 against root(A, 8)", &cid, 8, &va, ext.witness(&d, 9), range, Some(*relayer().value()));
        Observed::of_report(&rep, &format!("vault A's 9th leaf (public, 0.1 ETH; its record not imported), path from the 9-leaf tree, range {range:?}"))
    });
    // BR-CLM-04 … BR-CLM-13, BR-CLM-32: one field of the base claim's witness changed, its range kept.
    type Mutation = fn(&mut ClaimWitness);
    let mutations: [(&str, &str, Mutation); 9] = [
        ("BR-CLM-04", "witness.s with one bit flipped", |w| w.s[31] ^= 1),
        ("BR-CLM-05", "witness.dest = the attacker's account", |w| w.dest = key(OUTSIDER_SEED)),
        ("BR-CLM-06", "witness.kind = 0x01 (private) for a public-kind deposit", |w| w.kind = 0x01),
        ("BR-CLM-07", "witness.kind = 0x03", |w| w.kind = 0x03),
        ("BR-CLM-08", "one bit of witness.path[5] flipped", |w| w.path[5][0] ^= 1),
        ("BR-CLM-09", "witness.leaf_index = 6 ^ 1 = 7", |w| w.leaf_index ^= 1),
        ("BR-CLM-10", "witness.leaf_index = 6 + 2^32", |w| w.leaf_index += 1 << 32),
        ("BR-CLM-13", "witness.depositor = another address", |w| w.depositor = [0xab; 20]),
        ("BR-CLM-32", "witness.kind = 0x09 and leaf_index = 8 (the count)", |w| {
            w.kind = 0x09;
            w.leaf_index = 8;
        }),
    ];
    for (id, what, mutate) in mutations.iter() {
        r.run(id, || {
            let mut w = base();
            let (mut wit, range) = (leaf6(&w), range6(&w));
            mutate(&mut wit);
            let rep = claim_a8(&mut w, &format!("Claim: {what}"), wit, range);
            Observed::of_report(&rep, &format!("{base_note}; mutation: {what}"))
        });
    }
    // BR-CLM-11: a 0.1 ETH (d0) deposit at A's leaf 8 claimed as 1 ETH (d1).
    r.run("BR-CLM-11", || {
        let mut w = base();
        let (cid, va, payer) = (w.cid_a, w.vault_a.address, *w.chain.payer.value());
        let d = public(8, [0xc4; 20], ETH, D0, [0x11; 32], payer);
        let (i, ext) = extend_a(&mut w, std::slice::from_ref(&d));
        let n = open_extended(&mut w, i, &ext);
        let mut wit = ext.witness(&d, n);
        wit.amount = word_of_u128(D1);
        let range = w.range(&cid, &ETH, 8);
        let rep = w.claim_with("Claim leaf 8 (a d0 deposit) with amount d1", &cid, n, &va, wit, range, Some(*relayer().value()));
        Observed::of_report(&rep, "vault A extended by a d0 deposit at leaf 8; root(A, 9) imported")
    });
    // BR-CLM-12: a second token (T3) registered in A; the base claim's witness names it.
    r.run("BR-CLM-12", || {
        let mut w = base();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        ok(&w.register("setup: Register T3 (A) at block 100", &cid, 0, &t3_entry(), &va));
        let (mut wit, range) = (leaf6(&w), range6(&w));
        wit.token = T3;
        let rep = claim_a8(&mut w, "Claim: witness.token = T3", wit, range);
        Observed::of_report(&rep, &format!("{base_note}; T3 registered in A; ETH's range kept"))
    });
    // BR-CLM-14: leaf_count = 8 + 7, no root(A, 15).
    r.run("BR-CLM-14", || {
        let mut w = base();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        let (wit, range) = (leaf6(&w), range6(&w));
        let rep = w.claim_with("Claim at root(A, 15), never imported", &cid, 15, &va, wit, range, Some(*relayer().value()));
        Observed::of_report(&rep, &format!("{base_note}; leaf_count 15 (admission evidence for the empty root PDA)"))
    });
    // BR-CLM-15: range {0, 0, 6} for leaf 6.
    r.run("BR-CLM-15", || {
        let mut w = base();
        let wit = leaf6(&w);
        let rep = claim_a8(&mut w, "Claim leaf 6 with range {0, 0, 6}", wit, ScheduleRange { index: 0, from_leaf: 0, until_leaf: 6 });
        Observed::of_report(&rep, base_note)
    });
    // BR-CLM-16: range {5, 6, 8} (schedule 5 never imported; the range covers the leaf).
    r.run("BR-CLM-16", || {
        let mut w = base();
        let wit = leaf6(&w);
        let rep = claim_a8(&mut w, "Claim leaf 6 with range {5, 6, 8}", wit, ScheduleRange { index: 5, from_leaf: 6, until_leaf: 8 });
        Observed::of_report(&rep, &format!("{base_note}; range {{index 5, from_leaf 6, until_leaf 8}}"))
    });
    // BR-CLM-17: the private-kind deposit (leaf 5) by the plaintext path, its correct opening.
    r.run("BR-CLM-17", || {
        let mut w = base();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        let wit = w.vault_a.witness(&w.private_deposit.clone(), 8);
        let rep = w.claim("Claim leaf 5 (private kind) in plaintext", &cid, 8, &va, wit);
        Observed::of_report(&rep, "A's leaf 5 (1 ETH, private kind) under root(A, 8)")
    });
    // BR-CLM-18: ClaimPrivately of the private-kind deposit run as another private account (dev mode).
    r.run("BR-CLM-18", || {
        let mut w = base();
        let (cid, va, program) = (w.cid_a, w.vault_a.address, w.program);
        let other = w.chain.add_private_account(0x61, 1_000_000_000_000);
        let private = w.private_deposit.clone();
        let message = w.claim_privately(&cid, 8, w.vault_a.witness(&private, 8));
        let publics = w.private_claim_actors(&cid, 8, &va, &private);
        let me = own_actor(program, other.id());
        let time = w.time;
        let res = w.chain.prove_and_settle_private(&other, me, borsh::to_vec(&message).unwrap(), publics, &catalog(), RESERVE, time);
        let n = private.nullifier(CHAIN_ID, &va);
        let note = format!("leaf 5 by private account 0x61, not its destination; nullifier spent after: {}", w.nullifier(&cid, &va, &n).is_some());
        private_observed(&w, other.id(), &message, &cid, 8, &res, &note)
    });
    // BR-CLM-19: ClaimPrivately of the private-kind deposit by its destination (dev mode).
    r.run("BR-CLM-19", || {
        let mut w = base();
        let (cid, va, program, seed) = (w.cid_a, w.vault_a.address, w.program, w.private_seed);
        let account = w.chain.add_private_account(seed, 1_000_000_000_000);
        let private = w.private_deposit.clone();
        let message = w.claim_privately(&cid, 8, w.vault_a.witness(&private, 8));
        let publics = w.private_claim_actors(&cid, 8, &va, &private);
        let me = own_actor(program, account.id());
        let time = w.time;
        let res = w.chain.prove_and_settle_private(&account, me, borsh::to_vec(&message).unwrap(), publics, &catalog(), RESERVE, time);
        let observed = private_observed(&w, account.id(), &message, &cid, 8, &res, "leaf 5 by its destination's private account");
        if !matches!(observed, Observed::Accepted { .. }) {
            return observed;
        }
        let mut f = Vec::new();
        let holding = HolderState { holdings: vec![Holding { config_id: cid, token: ETH, amount: D1 }], pending: None };
        let credited = private_post_state(&w.chain, &private_keys(seed), &account.account, RESERVE, &holding);
        want(&mut f, "dest's own actor credited (its commitment holds 1 ETH)", credited.is_some(), true);
        let n = private.nullifier(CHAIN_ID, &va);
        want(&mut f, "nullifier written", w.nullifier(&cid, &va, &n), Some(NullifierState { config_id: cid, token: ETH, leaf_count: 8, amount: D1 }));
        if f.is_empty() { observed } else { Observed::EffectMismatch { note: f.join("; ") } }
    });
    // BR-CLM-20: ClaimPrivately of the public-kind leaf 6 (dev mode), then its plaintext Claim.
    r.run("BR-CLM-20", || {
        let mut w = base();
        let (cid, va, program) = (w.cid_a, w.vault_a.address, w.program);
        let prover = w.chain.add_private_account(0x51, 1_000_000_000_000);
        let deposit = w.payer_deposit.clone();
        let message = w.claim_privately(&cid, 8, w.vault_a.witness(&deposit, 8));
        let publics = w.private_claim_actors(&cid, 8, &va, &deposit);
        let me = own_actor(program, prover.id());
        let time = w.time;
        let res = w.chain.prove_and_settle_private(&prover, me, borsh::to_vec(&message).unwrap(), publics, &catalog(), RESERVE, time);
        assert!(res.as_ref().is_ok_and(|run| run.charge.revert.is_none()), "setup: the private claim settles: {:?}", res.as_ref().err());
        let (wit, range) = (leaf6(&w), range6(&w));
        let rep = claim_a8(&mut w, "plaintext Claim of leaf 6 after its private claim", wit, range);
        Observed::of_report(&rep, "leaf 6 claimed privately by private account 0x51 first")
    });
    // BR-CLM-21: ETH's mints frozen (token freeze) for an hour after block 102's time.
    r.run("BR-CLM-21", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let t0 = w.blocks[2].time_ms();
        let f = freeze(cid, Some(ETH), true, false, t0 + HOUR_MS, w.blocks[2].time_record());
        ok(&w.role("setup: Freeze ETH (A) mints for an hour", FREEZE_SEED, &f, token_actor(program, &cid, &ETH)));
        let (wit, range) = (leaf6(&w), range6(&w));
        let rep = claim_a8(&mut w, "the base claim while ETH (A) mints are frozen", wit, range);
        Observed::of_report(&rep, base_note)
    });
    // BR-CLM-22: A's mints frozen (global freeze).
    r.run("BR-CLM-22", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let t0 = w.blocks[2].time_ms();
        let f = freeze(cid, None, true, false, t0 + HOUR_MS, w.blocks[2].time_record());
        ok(&w.role("setup: Freeze all mints of A for an hour", FREEZE_SEED, &f, config_actor(program, &cid)));
        let (wit, range) = (leaf6(&w), range6(&w));
        let rep = claim_a8(&mut w, "the base claim while A's mints are frozen", wit, range);
        Observed::of_report(&rep, base_note)
    });
    // BR-CLM-23: ETH's mint cap set to the floor with the bucket drained (cap 10.06 ETH, drained by the base
    // claim, then the floor); a valid claim of leaf 2.
    r.run("BR-CLM-23", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let eth = token_actor(program, &cid, &ETH);
        let exact = CapParams { capacity: 10_060_000_000, refill_per_second: FLOOR.refill_per_second };
        ok(&w.role("setup: SetCap ETH (A) mints 10.06 ETH", ADMIN_SEED, &set_cap(cid, Some(ETH), true, exact), eth));
        let (wit, range) = (leaf6(&w), range6(&w));
        ok(&claim_a8(&mut w, "setup: the base claim (10.06 ETH: drains the bucket)", wit, range));
        ok(&w.role("setup: SetCap ETH (A) mints to the floor", ADMIN_SEED, &set_cap(cid, Some(ETH), true, FLOOR), eth));
        let level = w.token_state(&cid, &ETH).map(|t| t.gate.mint.level);
        let rep = claim_leaf2(&mut w, "Claim leaf 2 (1.001 ETH) with ETH's mint bucket drained at the floor");
        Observed::of_report(&rep, &format!("token mint bucket level before: {level:?}"))
    });
    // BR-CLM-24: A's global mint bucket drained (cap 10.06 ETH, drained by the base claim); a valid claim.
    r.run("BR-CLM-24", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let exact = CapParams { capacity: 10_060_000_000, refill_per_second: FLOOR.refill_per_second };
        ok(&w.role("setup: SetCap A global mints 10.06 ETH", ADMIN_SEED, &set_cap(cid, None, true, exact), config_actor(program, &cid)));
        let (wit, range) = (leaf6(&w), range6(&w));
        ok(&claim_a8(&mut w, "setup: the base claim (drains the global bucket)", wit, range));
        let level = config_gate(&w, &cid).map(|g| g.mint.level);
        let rep = claim_leaf2(&mut w, "Claim leaf 2 with A's global mint bucket drained");
        Observed::of_report(&rep, &format!("global mint bucket level before: {level:?}"))
    });
    // BR-CLM-25: the base claim with relayer None.
    r.run("BR-CLM-25", || {
        let mut w = base();
        let (cid, va, payer) = (w.cid_a, w.vault_a.address, w.chain.payer);
        let before = (w.balance(payer, &cid), w.balance(fee_recipient(), &cid), w.balance(relayer(), &cid));
        let (wit, range) = (leaf6(&w), range6(&w));
        let rep = w.claim_with("the base claim with relayer None", &cid, 8, &va, wit, range, None);
        observe(&w, &rep, base_note, |w| {
            let mut f = Vec::new();
            want(&mut f, "fee recipient +mint fee +relay fee", w.balance(fee_recipient(), &cid) - before.1, MINT_FEES_1[2] + RELAY_FEES[2]);
            want(&mut f, "dest +denomination", w.balance(payer, &cid) - before.0, D2);
            want(&mut f, "R unchanged", w.balance(relayer(), &cid), before.2);
            f
        })
    });
    // BR-CLM-26: the base claim with a flipped path bit (refused), then the correct one.
    r.run("BR-CLM-26", || {
        let mut w = base();
        let (wit, range) = (leaf6(&w), range6(&w));
        let mut bad = wit.clone();
        bad.path[3][0] ^= 1;
        let first = claim_a8(&mut w, "the base claim, one path bit flipped", bad, range);
        let rep = claim_a8(&mut w, "the correct base claim", wit, range);
        Observed::of_report(&rep, &format!("{base_note}; the first claim: exit {:?}", first.exit_code))
    });
    // BR-CLM-27 and BR-CLM-28: A's leaf 8 the victim's deposit, leaf 9 the attacker's copy of its C.
    let copied = |w: &mut World| {
        let payer = *w.chain.payer.value();
        let victim = public(8, [0xa7; 20], ETH, D0, [0x27; 32], payer);
        let copy = Deposit { leaf_index: 9, depositor: [0xb7; 20], ..victim.clone() };
        assert_eq!(copy.commitment(), victim.commitment());
        let (i, ext) = extend_a(w, &[victim.clone(), copy.clone()]);
        let n = open_extended(w, i, &ext);
        (victim, copy, ext, n)
    };
    r.run("BR-CLM-27", || {
        let mut w = base();
        let (cid, va, payer) = (w.cid_a, w.vault_a.address, w.chain.payer);
        let (victim, _, ext, n) = copied(&mut w);
        let before = w.balance(payer, &cid);
        let rep = w.claim("the victim's Claim of leaf 8", &cid, n, &va, ext.witness(&victim, n));
        observe(&w, &rep, "vault A extended: leaf 8 the victim's 0.1 ETH, leaf 9 the attacker's copy of its C", |w| {
            let mut f = Vec::new();
            want(&mut f, "the victim's destination +0.1 ETH", w.balance(payer, &cid) - before, D0);
            f
        })
    });
    r.run("BR-CLM-28", || {
        let mut w = base();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        let (_, copy, ext, n) = copied(&mut w);
        let mut wit = ext.witness(&copy, n);
        wit.s = [0x42; 32];
        let rep = w.claim("the attacker's Claim of its copy (leaf 9) with a guessed s", &cid, n, &va, wit);
        Observed::of_report(&rep, "s guessed; kind and destination as in the copied C")
    });
    // BR-CLM-29: a public deposit (A's leaf 8) whose dest is config(A)'s account id.
    r.run("BR-CLM-29", || {
        let mut w = base();
        let (cid, va, program) = (w.cid_a, w.vault_a.address, w.program);
        let target = config_actor(program, &cid);
        let d = public(8, [0xc9; 20], ETH, D0, [0x29; 32], *target.account_id.value());
        let (i, ext) = extend_a(&mut w, std::slice::from_ref(&d));
        let n = open_extended(&mut w, i, &ext);
        let before = w.chain.actor_state(target).to_vec();
        let rep = w.claim("Claim leaf 8: dest = config(A)'s account id", &cid, n, &va, ext.witness(&d, n));
        observe(&w, &rep, "vault A extended by leaf 8 (0.1 ETH to config(A)'s id)", |w| {
            let mut f = Vec::new();
            let after = w.chain.actor_state(target).to_vec();
            if after != before {
                let (b, a) = (borsh::from_slice::<BridgeState>(&before), borsh::from_slice::<BridgeState>(&after));
                let diff = match (b, a) {
                    (Ok(BridgeState::Config(ConfigState::Ready { gate: g0, .. })), Ok(BridgeState::Config(mut s1))) => {
                        let gate1 = match &mut s1 {
                            ConfigState::Ready { gate, .. } => {
                                let g1 = *gate;
                                gate.mint = g0.mint;
                                g1
                            }
                            other => panic!("not ready: {other:?}"),
                        };
                        let rest_equal = BridgeState::Config(s1) == borsh::from_slice::<BridgeState>(&before).unwrap();
                        format!(
                            "still a ready configuration; global mint bucket level {} -> {} (this claim's value, Admit); \
                             everything else equal: {rest_equal}",
                            g0.mint.level, gate1.mint.level
                        )
                    }
                    (b, a) => format!("before {b:?}, after {a:?}"),
                };
                f.push(format!("config(A)'s state changed: {diff}"));
            }
            want(&mut f, "holder(config(A) id) credited", w.balance(target.account_id, &cid), D0);
            f
        })
    });
    // BR-CLM-30: root(B, 7) imported (a new record of vault B at 7 leaves); Claim{config A, leaf_count 7}.
    r.run("BR-CLM-30", || {
        let mut w = base();
        let (cid_a, cid_b) = (w.cid_a, w.cid_b);
        let slots = VaultSlots { address: VAULT_B, root: w.vault_b.root(7), count: 7, config_id: cid_b, extra: registry_slots(&eth_entry(), &[]) };
        let i = add(&mut w, slots);
        let root7 = w.vault_b.root(7);
        ok(&w.import("setup: ImportRoot B n=7", &cid_b, i, root7, 7, &VAULT_B));
        let wit = w.vault_b.witness(&w.deposit_b.clone(), 7);
        let range = w.range(&cid_b, &ETH, 3);
        let rep = w.claim_with("Claim{config A, leaf_count 7} of B's deposit", &cid_a, 7, &VAULT_B, wit, range, Some(*relayer().value()));
        Observed::of_report(&rep, "delivered at root(A, 7) (empty; admission evidence supplied); only root(B, 7) exists")
    });
    // BR-CLM-31: a deposit of token U (never registered in A) at A's leaf 8, leaf and root valid.
    r.run("BR-CLM-31", || {
        let mut w = base();
        let (cid, va, program, payer) = (w.cid_a, w.vault_a.address, w.program, w.chain.payer);
        let d = public(8, [0xd1; 20], U, D0, [0x31; 32], *payer.value());
        let (i, ext) = extend_a(&mut w, std::slice::from_ref(&d));
        let n = open_extended(&mut w, i, &ext);
        let wit = ext.witness(&d, n);
        let range = w.range(&cid, &U, 8);
        let actors = claim_actors_of(&w, &cid, n, &va, &wit, Some(*relayer().value()));
        let message = BridgeMessage::Claim { config_id: cid, leaf_count: n, witness: wit, range, relayer: Some(*relayer().value()) };
        let evidence = vec![PublicAccountEvidence::Pda { program, seed: token_seed(&cid, &U) }];
        let rep = w.submit("Claim leaf 8 (token U, not registered in A)", actors[0], &message, actors, evidence);
        Observed::of_report(&rep, "vault A extended by a 0.1-unit deposit of U at leaf 8, root(A, 9) imported; admission evidence for token(A, U) supplied")
    });
    r.finish(&["bridge"], &["BR-CLM-"]);
}

// ---------------------------------------------------------------------------------------------
// Burns
// ---------------------------------------------------------------------------------------------

#[test]
fn brn() {
    let mut r = Results::new(FILE, "brn");
    let holds = |amount: u128| {
        let mut w = base();
        claim_payer_10(&mut w);
        let cid = w.cid_a;
        give(&mut w, &cid, H, amount);
        w
    };
    // BR-BRN-01: H holds 2 d0; H burns a note of d0 at its own actor.
    r.run("BR-BRN-01", || {
        let mut w = holds(2 * D0);
        let (cid, program) = (w.cid_a, w.program);
        let note = note_of(1, D0);
        let rep = burn_at(&mut w, "Burn d0 by H", H, H, cid, &note);
        observe(&w, &rep, "holder(H) holds 0.2 ETH under A", |w| {
            let mut f = Vec::new();
            want(&mut f, "holding -d0", w.balance(role_account(H), &cid), D0);
            let rec = w.chain.actor_state(burn_actor(program, &cid, &note.hash())).to_vec();
            want(&mut f, "burn(A, H(note))", borsh::from_slice::<BridgeState>(&rec).ok(), Some(BridgeState::Burn(BurnRecord { config_id: cid, note })));
            f
        })
    });
    // BR-BRN-02: amount d0 - 1.
    r.run("BR-BRN-02", || {
        let mut w = holds(2 * D0);
        let cid = w.cid_a;
        let rep = burn_at(&mut w, "Burn d0 - 1 by H", H, H, cid, &note_of(2, D0 - 1));
        Observed::of_report(&rep, "holder(H) holds 0.2 ETH")
    });
    // BR-BRN-03: H holds d0, burns d1.
    r.run("BR-BRN-03", || {
        let mut w = holds(D0);
        let cid = w.cid_a;
        let rep = burn_at(&mut w, "Burn d1 by H (holds d0)", H, H, cid, &note_of(3, D1));
        Observed::of_report(&rep, "holder(H) holds 0.1 ETH")
    });
    // BR-BRN-04: BR-BRN-01's burn, then the same burn again.
    r.run("BR-BRN-04", || {
        let mut w = holds(2 * D0);
        let cid = w.cid_a;
        let note = note_of(1, D0);
        ok(&burn_at(&mut w, "setup: Burn d0 by H", H, H, cid, &note));
        let rep = burn_at(&mut w, "the same Burn again", H, H, cid, &note);
        let left = w.balance(role_account(H), &cid);
        if !rep.ok() && left != D0 {
            return Observed::EffectMismatch { note: format!("refused, but the holding is {left}, not d0 [{}]", rep.label) };
        }
        Observed::of_report(&rep, &format!("holding after: {left} (d0 = {D0})"))
    });
    // BR-BRN-05: H and X hold d0; X burns a note with H's r (another memo and mac) first; then H's burn.
    r.run("BR-BRN-05", || {
        let mut w = holds(D0);
        let cid = w.cid_a;
        give(&mut w, &cid, X, D0);
        let note_h = note_of(5, D0);
        let note_x = Note { memo: [0x99; 20], mac: [0x98; 16], ..note_h };
        let first = burn_at(&mut w, "X burns a note with H's r first", X, X, cid, &note_x);
        let rep = burn_at(&mut w, "H burns its note", H, H, cid, &note_h);
        Observed::of_report(&rep, &format!("X's burn first: applied {} exit {:?}", first.ok(), first.exit_code))
    });
    // BR-BRN-06: holder(P's account) holds d0, P (LEZ's `scripted` program, at its PDA) calls the bridge's
    // Burn at its own bridge actor from its handler.
    r.run("BR-BRN-06", || {
        let mut w = base();
        claim_payer_10(&mut w);
        let (cid, program) = (w.cid_a, w.program);
        let seed = PdaSeed::new([0x66; 32]);
        let p_account = AccountId::for_public_pda(&scripted_account(), &seed);
        let p = Actor::new(p_account, scripted_account());
        ok(&w.transfer("setup: Transfer d0 to holder(P's account)", &cid, ETH, D0, p_account));
        let note = note_of(6, D0);
        let own = own_actor(program, p_account);
        let script = Script::default().send_sync(own, &BridgeMessage::burn(cid, &note));
        let mut actors = vec![p];
        actors.extend(w.public_burn_actors(p_account, &cid, &note));
        let time = w.time;
        let rep = w.chain.submit(
            "Burn as a nested call from program P",
            p,
            borsh::to_vec(&script).unwrap(),
            actors,
            vec![PublicAccountEvidence::Pda { program: scripted_account(), seed }],
            time,
        );
        Observed::of_report(&rep, &format!("holder(P) after: {}", w.balance(p_account, &cid)))
    });
    // BR-BRN-07: X sends Burn at H's own actor, signed by X (not H).
    r.run("BR-BRN-07", || {
        let mut w = holds(D0);
        let cid = w.cid_a;
        let rep = burn_at(&mut w, "X sends Burn at H's own actor", H, X, cid, &note_of(7, D0));
        Observed::of_report(&rep, "signed by the payer and X; H did not sign")
    });
    // BR-BRN-08: ETH's burns frozen for an hour.
    r.run("BR-BRN-08", || {
        let mut w = holds(D0);
        let (cid, program) = (w.cid_a, w.program);
        let t0 = w.blocks[2].time_ms();
        let f = freeze(cid, Some(ETH), false, true, t0 + HOUR_MS, w.blocks[2].time_record());
        ok(&w.role("setup: Freeze ETH (A) burns for an hour", FREEZE_SEED, &f, token_actor(program, &cid, &ETH)));
        let rep = burn_at(&mut w, "Burn d0 by H while burns are frozen", H, H, cid, &note_of(8, D0));
        Observed::of_report(&rep, "holder(H) holds 0.1 ETH")
    });
    // BR-BRN-09: ETH's burn bucket drained (set to the floor, 1 ETH, drained by the payer's burn of 1 ETH).
    r.run("BR-BRN-09", || {
        let mut w = holds(D0);
        let (cid, program, payer) = (w.cid_a, w.program, w.chain.payer);
        ok(&w.role("setup: SetCap ETH (A) burns to the floor (1 ETH)", ADMIN_SEED, &set_cap(cid, Some(ETH), false, FLOOR), token_actor(program, &cid, &ETH)));
        let drain = note_of(0x90, D1);
        let actors = w.public_burn_actors(payer, &cid, &drain);
        ok(&w.send("setup: the payer burns 1 ETH (drains the bucket)", own_actor(program, payer), &BridgeMessage::burn(cid, &drain), actors));
        let level = w.token_state(&cid, &ETH).map(|t| t.gate.burn.level);
        let rep = burn_at(&mut w, "Burn d0 by H with the burn bucket drained", H, H, cid, &note_of(9, D0));
        Observed::of_report(&rep, &format!("burn bucket level before: {level:?}"))
    });
    // BR-BRN-10: H holds d0 under A only; Burn{config B} (B registered ETH).
    r.run("BR-BRN-10", || {
        let mut w = holds(D0);
        let cid_b = w.cid_b;
        let rep = burn_at(&mut w, "Burn d0 by H under configuration B", H, H, cid_b, &note_of(10, D0));
        Observed::of_report(&rep, "holder(H) holds 0.1 ETH under A, nothing under B")
    });
    // BR-BRN-11: amount 2^200.
    r.run("BR-BRN-11", || {
        let mut w = holds(D0);
        let cid = w.cid_a;
        let mut note = note_of(11, D0);
        note.amount = [0; 32];
        note.amount[6] = 1;
        let rep = burn_at(&mut w, "Burn of 2^200 by H", H, H, cid, &note);
        Observed::of_report(&rep, "holder(H) holds 0.1 ETH")
    });
    // BR-BRN-12: a note made as a wallet makes it (memo = recipient XOR pad); the recipient searched.
    r.run("BR-BRN-12", || {
        let mut w = holds(D0);
        let (cid, va, program) = (w.cid_a, w.vault_a.address, w.program);
        let recipient: [u8; 20] = sha256(b"redteam BR-BRN-12 recipient")[..20].try_into().unwrap();
        let keys = BridgeKeys::from_bridge_seed(&[0x77; 32]);
        let r_tag = burn_tag(&keys.view, &cid, 0);
        let s_out = burn_secret(&keys.auth, &cid, &r_tag);
        let c_out = burn_commitment(&s_out, &chain_id_u256(CHAIN_ID), &va, &recipient);
        let memo = xor_memo(&keys.view, &r_tag, &recipient);
        let amount = word_of_u128(D0);
        let mac = burn_mac_v2(&keys.view, &r_tag, &memo, &ETH, &amount, &c_out);
        let note = Note { r: r_tag, memo, mac, token: ETH, amount, c_out };
        let account = role_account(H);
        let actors = w.public_burn_actors(account, &cid, &note);
        let rep = burn_at(&mut w, "Burn d0 by H, memo = recipient XOR pad", H, H, cid, &note);
        observe(&w, &rep, "the recipient's 20 bytes searched in the transaction, every actor state it touched and its events", |w| {
            let mut f = Vec::new();
            let tx = &w.txs.last().unwrap().1;
            want(&mut f, "recipient in the transaction at", occurrences(tx, &recipient), vec![]);
            for a in actors.iter().chain([own_actor(program, account)].iter()) {
                let st = w.chain.actor_state(*a).to_vec();
                want(&mut f, &format!("recipient in the state of {:?}", a.account_id), occurrences(&st, &recipient), vec![]);
            }
            want(&mut f, "events", rep.events.len(), 0);
            f
        })
    });
    r.finish(&["bridge"], &["BR-BRN-"]);
}

// ---------------------------------------------------------------------------------------------
// Internal messages
// ---------------------------------------------------------------------------------------------

#[test]
fn int() {
    let mut r = Results::new(FILE, "int");
    let outsider = "sent by the payer (an ordinary account) as the transaction's root";
    // BR-INT-01: Mint{holder = itself, d2} at holder(itself).
    r.run("BR-INT-01", || {
        let mut w = base();
        let (cid, va, program, payer) = (w.cid_a, w.vault_a.address, w.program, w.chain.payer);
        let mint = BridgeMessage::Mint { nullifier: [7; 32], config_id: cid, evm_chain_id: CHAIN_ID, vault: va, token: ETH, holder: *payer.value(), amount: D2 };
        let at = holder_actor(program, payer);
        let ev = vec![PublicAccountEvidence::Pda { program, seed: holder_seed(payer.value()) }];
        let rep = w.submit("Mint sent to holder(payer) by the payer", at, &mint, vec![at], ev);
        Observed::of_report(&rep, outsider)
    });
    // BR-INT-02: Spend{N of leaf 6, mint_to = itself} at nullifier(chain, V, N).
    r.run("BR-INT-02", || {
        let mut w = base();
        let (cid, va, program, payer) = (w.cid_a, w.vault_a.address, w.program, w.chain.payer);
        let n = w.payer_deposit.nullifier(CHAIN_ID, &va);
        let spend = BridgeMessage::Spend { nullifier: n, config_id: cid, evm_chain_id: CHAIN_ID, vault: va, token: ETH, leaf_count: 8, amount: D2, mint_to: Some(*payer.value()), fees: vec![] };
        let at = nullifier_actor(program, CHAIN_ID, &va, &n);
        let ev = vec![PublicAccountEvidence::Pda { program, seed: nullifier_seed(CHAIN_ID, &va, &n) }];
        let rep = w.submit("Spend sent to nullifier(N) by the payer", at, &spend, vec![at, holder_actor(program, payer)], ev);
        Observed::of_report(&rep, &format!("{outsider}; nullifier spent after: {}", w.nullifier(&cid, &va, &n).is_some()))
    });
    // BR-INT-03: WriteRoot{root of its choice} at root(A, 9).
    r.run("BR-INT-03", || {
        let mut w = base();
        let (cid, va, program) = (w.cid_a, w.vault_a.address, w.program);
        let root = RootState { config_id: cid, evm_chain_id: CHAIN_ID, vault: va, leaf_count: 9, root: [0xaa; 32], record_slot: w.blocks[1].slot, block_number: 101 };
        let at = root_actor(program, &cid, 9);
        let ev = vec![PublicAccountEvidence::Pda { program, seed: root_seed(&cid, 9) }];
        let rep = w.submit("WriteRoot sent to root(A, 9) by the payer", at, &BridgeMessage::WriteRoot { root }, vec![at], ev);
        Observed::of_report(&rep, outsider)
    });
    // BR-INT-04: WriteToken{state of its choice} at token(A, U).
    r.run("BR-INT-04", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let mut state = w.token_state(&cid, &ETH).unwrap();
        state.entry.token = U;
        let at = token_actor(program, &cid, &U);
        let ev = vec![PublicAccountEvidence::Pda { program, seed: token_seed(&cid, &U) }];
        let rep = w.submit("WriteToken sent to token(A, U) by the payer", at, &BridgeMessage::WriteToken { state }, vec![at], ev);
        Observed::of_report(&rep, &format!("{outsider}; the state: ETH's of A with the token set to U"))
    });
    // BR-INT-05: Issue{A, ETH, random N, d0, mint_to = itself} at token(A, ETH).
    r.run("BR-INT-05", || {
        let mut w = base();
        let (cid, va, program, payer) = (w.cid_a, w.vault_a.address, w.program, w.chain.payer);
        let n = [0x5e; 32];
        let issue = BridgeMessage::Issue { config_id: cid, token: ETH, nullifier: n, amount: D0, range: ScheduleRange { index: 0, from_leaf: 0, until_leaf: 6 }, leaf_count: 8, mint_to: Some(*payer.value()), relayer: None };
        let at = token_actor(program, &cid, &ETH);
        let actors = vec![at, config_actor(program, &cid), nullifier_actor(program, CHAIN_ID, &va, &n), holder_actor(program, payer), holder_actor(program, fee_recipient())];
        let rep = w.send("Issue sent to token(A, ETH) by the payer", at, &issue, actors);
        Observed::of_report(&rep, outsider)
    });
    // BR-INT-06: Admit at config(A).
    r.run("BR-INT-06", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let at = config_actor(program, &cid);
        let rep = w.send("Admit sent to config(A) by the payer", at, &BridgeMessage::Admit { config_id: cid, token: ETH, mints: true, value: 1 }, vec![at]);
        Observed::of_report(&rep, outsider)
    });
    // BR-INT-07: Credit{A, ETH, d2, to = itself} at holder(itself).
    r.run("BR-INT-07", || {
        let mut w = base();
        let (cid, program, payer) = (w.cid_a, w.program, w.chain.payer);
        let at = holder_actor(program, payer);
        let ev = vec![PublicAccountEvidence::Pda { program, seed: holder_seed(payer.value()) }];
        let credit = BridgeMessage::Credit { config_id: cid, token: ETH, amount: D2, to: *payer.value() };
        let rep = w.submit("Credit sent to holder(payer) by the payer", at, &credit, vec![at], ev);
        Observed::of_report(&rep, outsider)
    });
    // BR-INT-08: ExtendFees{A, ETH, 1, a zero-fee schedule, count} at token(A, ETH).
    r.run("BR-INT-08", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let zero = FeeSchedule { index: 1, from_leaf: 8, announced_at: ANNOUNCED_1, effective_at: ANNOUNCED_1 + 7 * DAY_S, mint_fees: vec![0; 3], burn_fees: vec![0; 3], checkpoint_fees: vec![0; 3] };
        let at = token_actor(program, &cid, &ETH);
        let m = BridgeMessage::ExtendFees { config_id: cid, token: ETH, index: 1, schedule: Some(zero), count: 8 };
        let rep = w.send("ExtendFees sent to token(A, ETH) by the payer", at, &m, vec![at]);
        Observed::of_report(&rep, outsider)
    });
    // BR-INT-09: Read at config(A) as the root.
    r.run("BR-INT-09", || {
        let mut w = base();
        let at = config_actor(w.program, &w.cid_a);
        let rep = w.send("Read sent to config(A) as the root", at, &BridgeMessage::Read, vec![at]);
        Observed::of_report(&rep, outsider)
    });
    // BR-INT-10: Echo{nonce} at config(A).
    r.run("BR-INT-10", || {
        let mut w = base();
        let at = config_actor(w.program, &w.cid_a);
        let before = w.chain.actor_state(at).to_vec();
        let rep = w.send("Echo sent to config(A)", at, &BridgeMessage::Echo { nonce: [0xec; 32] }, vec![at]);
        observe(&w, &rep, "config(A)'s state compared byte for byte", |w| {
            let mut f = Vec::new();
            want(&mut f, "config(A) unchanged", w.chain.actor_state(at).to_vec() == before, true);
            f
        })
    });
    r.finish(&["bridge"], &["BR-INT-"]);
}

// ---------------------------------------------------------------------------------------------
// Controls
// ---------------------------------------------------------------------------------------------

#[test]
fn ctl() {
    let mut r = Results::new(FILE, "ctl");
    // BR-CTL-01: the admin sends Freeze{A, ETH, mints}.
    r.run("BR-CTL-01", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let t0 = w.blocks[2].time_ms();
        let f = freeze(cid, Some(ETH), true, false, t0 + HOUR_MS, w.blocks[2].time_record());
        let rep = w.role("Freeze ETH (A) mints by the admin", ADMIN_SEED, &f, token_actor(program, &cid, &ETH));
        Observed::of_report(&rep, "record: block 102")
    });
    // BR-CTL-02: freeze until the record's attested time + 30 days + 1 ms.
    r.run("BR-CTL-02", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let t0 = w.blocks[2].time_ms();
        let f = freeze(cid, Some(ETH), true, false, t0 + MAX_FREEZE_MS + 1, w.blocks[2].time_record());
        let rep = w.role("Freeze ETH (A) mints until t + 30 d + 1 ms", FREEZE_SEED, &f, token_actor(program, &cid, &ETH));
        Observed::of_report(&rep, "t = block 102's time, the record the freeze names")
    });
    // BR-CTL-03: the token's gate at block 102's time (Tick); a freeze until that time - 1.
    r.run("BR-CTL-03", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        ok(&w.tick("setup: Tick ETH (A): block 102's record", &cid, Some(ETH), 2));
        let t0 = w.blocks[2].time_ms();
        let f = freeze(cid, Some(ETH), true, false, t0 - 1, w.blocks[2].time_record());
        let rep = w.role("Freeze ETH (A) mints until the gate's time - 1", FREEZE_SEED, &f, token_actor(program, &cid, &ETH));
        Observed::of_report(&rep, "gate at block 102's time")
    });
    // BR-CTL-04: the admin sets ETH's mint cap one unit below the floor.
    r.run("BR-CTL-04", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let below = CapParams { capacity: FLOOR.capacity - 1, ..FLOOR };
        let rep = w.role("SetCap ETH (A) mints below the floor", ADMIN_SEED, &set_cap(cid, Some(ETH), true, below), token_actor(program, &cid, &ETH));
        Observed::of_report(&rep, "capacity floor - 1")
    });
    // BR-CTL-05: SetCap by the freeze authority.
    r.run("BR-CTL-05", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let rep = w.role("SetCap ETH (A) by the freeze authority", FREEZE_SEED, &set_cap(cid, Some(ETH), true, TOKEN_CAP), token_actor(program, &cid, &ETH));
        Observed::of_report(&rep, "")
    });
    // BR-CTL-06: a handover sets a watcher; FA freezes ETH's mints for 20 days; the watcher pauses them;
    // 13 h of attested time later (Tick) a claim.
    r.run("BR-CTL-06", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let day14 = handover(&mut w, FreezeAuthority::key(key(FREEZE_SEED)), vec![key(WATCHER)]);
        let t14 = w.blocks[day14].time_ms();
        let rec = w.blocks[day14].time_record();
        let eth = token_actor(program, &cid, &ETH);
        ok(&w.role("setup: Freeze ETH (A) mints for 20 days", FREEZE_SEED, &freeze(cid, Some(ETH), true, false, t14 + 20 * DAY_MS, rec.clone()), eth));
        let pause = BridgeMessage::Pause { config_id: cid, token: Some(ETH), mints: true, burns: false, record: rec };
        let p = w.role("the watcher pauses ETH (A) mints", WATCHER, &pause, eth);
        let until = w.token_state(&cid, &ETH).map(|t| t.gate.mints_until);
        let later = w.finalize(t14 / 1000 + 13 * 3600);
        ok(&w.tick("setup: Tick ETH (A): a record 13 h later", &cid, Some(ETH), later));
        let (wit, range) = (leaf6(&w), range6(&w));
        let rep = claim_a8(&mut w, "the base claim 13 h after the pause", wit, range);
        Observed::of_report(&rep, &format!("pause: applied {} exit {:?}; mints_until after it {until:?} (freeze end {})", p.ok(), p.exit_code, t14 + 20 * DAY_MS))
    });
    // BR-CTL-07: an ordinary account sends Pause{A, ETH, mints}.
    r.run("BR-CTL-07", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let pause = BridgeMessage::Pause { config_id: cid, token: Some(ETH), mints: true, burns: false, record: w.blocks[2].time_record() };
        let rep = w.role("Pause ETH (A) mints by a signer with no role", OUTSIDER_SEED, &pause, token_actor(program, &cid, &ETH));
        Observed::of_report(&rep, "no watchers set")
    });
    let propose = |w: &World, fa: FreezeAuthority, watchers: Vec<Hash>| BridgeMessage::ProposeRoles {
        config_id: w.cid_a,
        freeze_authority: fa,
        watchers,
        record: w.blocks[2].time_record(),
    };
    let multisig = || FreezeAuthority { members: vec![key(M1), key(M2), key(M3)], threshold: 2 };
    // BR-CTL-08: ProposeRoles by the freeze authority.
    r.run("BR-CTL-08", || {
        let mut w = base();
        let m = propose(&w, multisig(), vec![key(WATCHER)]);
        let at = config_actor(w.program, &w.cid_a);
        let rep = w.role("ProposeRoles by the freeze authority", FREEZE_SEED, &m, at);
        Observed::of_report(&rep, "")
    });
    // BR-CTL-09: a proposal at block 102's time t; ApplyRoles with a record at t + 13 days.
    r.run("BR-CTL-09", || {
        let mut w = base();
        let cid = w.cid_a;
        let t0 = w.blocks[2].time_ms();
        let m = propose(&w, multisig(), vec![key(WATCHER)]);
        let at = config_actor(w.program, &cid);
        ok(&w.role("setup: ProposeRoles (admin)", ADMIN_SEED, &m, at));
        let day13 = w.finalize(t0 / 1000 + 13 * DAY_S);
        let rep = w.apply_roles("ApplyRoles with a record 13 days later", &cid, day13);
        Observed::of_report(&rep, "")
    });
    // BR-CTL-10: ApplyRoles with no proposal.
    r.run("BR-CTL-10", || {
        let mut w = base();
        let cid = w.cid_a;
        let rep = w.apply_roles("ApplyRoles, nothing proposed", &cid, 2);
        Observed::of_report(&rep, "")
    });
    // BR-CTL-11: four watchers.
    r.run("BR-CTL-11", || {
        let mut w = base();
        let m = propose(&w, multisig(), vec![key(WATCHER), key(W2), key(X), key(H)]);
        let at = config_actor(w.program, &w.cid_a);
        let rep = w.role("ProposeRoles with 4 watchers", ADMIN_SEED, &m, at);
        Observed::of_report(&rep, "")
    });
    // BR-CTL-12: FreezeAuthority{[a, b], threshold 0}.
    r.run("BR-CTL-12", || {
        let mut w = base();
        let m = propose(&w, FreezeAuthority { members: vec![key(M1), key(M2)], threshold: 0 }, vec![]);
        let at = config_actor(w.program, &w.cid_a);
        let rep = w.role("ProposeRoles with threshold 0", ADMIN_SEED, &m, at);
        Observed::of_report(&rep, "")
    });
    // BR-CTL-13: FreezeAuthority{[admin, x], threshold 1}.
    r.run("BR-CTL-13", || {
        let mut w = base();
        let m = propose(&w, FreezeAuthority { members: vec![key(ADMIN_SEED), key(M1)], threshold: 1 }, vec![]);
        let at = config_actor(w.program, &w.cid_a);
        let rep = w.role("ProposeRoles with the admin as a member", ADMIN_SEED, &m, at);
        Observed::of_report(&rep, "")
    });
    // BR-CTL-14 and BR-CTL-15: FA = 2 of 3 after a handover; Cosigned freezes by [m1] and by [m1, m1].
    for (id, seeds, label) in [
        ("BR-CTL-14", vec![M1], "Cosigned freeze, signers [m1]"),
        ("BR-CTL-15", vec![M1, M1], "Cosigned freeze, signers [m1, m1]"),
    ] {
        r.run(id, || {
            let mut w = base();
            let cid = w.cid_a;
            let day14 = handover(&mut w, multisig(), vec![]);
            let t14 = w.blocks[day14].time_ms();
            let action = CosignedAction::Freeze { token: None, mints: true, burns: false, until_ms: t14 + HOUR_MS, record: w.blocks[day14].time_record() };
            let rep = w.cosign(label, &seeds, &[M1], &cid, action);
            Observed::of_report(&rep, "signed by the payer and m1")
        });
    }
    // BR-CTL-16: an ordinary account sends Sunset{A} at config(A).
    r.run("BR-CTL-16", || {
        let mut w = base();
        let at = config_actor(w.program, &w.cid_a);
        let rep = w.send("Sunset sent to config(A) by the payer", at, &BridgeMessage::Sunset { config_id: w.cid_a }, vec![at]);
        Observed::of_report(&rep, "vault not attested retired, no approval")
    });
    // BR-CTL-17: AttestRetired with true proofs of slot 37 holding a freeze authority, retired byte 0.
    r.run("BR-CTL-17", || {
        let mut w = base();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(&[0xfa; 20]);
        let slots = VaultSlots { address: va, root: w.vault_a.root(8), count: 8, config_id: cid, extra: vec![(SLOT_RETIRED, word)] };
        let i = add(&mut w, slots);
        let rep = w.attest_retired("AttestRetired A: slot 37, retired byte 0", &cid, i, &va);
        Observed::of_report(&rep, "slot 37 = 0x..fafa (freeze authority), byte 11 = 0")
    });
    // BR-CTL-18: Sunset by the freeze authority, then again.
    r.run("BR-CTL-18", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let sunset = BridgeMessage::Sunset { config_id: cid };
        ok(&w.role("setup: Sunset A (freeze authority)", FREEZE_SEED, &sunset, config_actor(program, &cid)));
        let rep = w.role("Sunset A again", FREEZE_SEED, &sunset, config_actor(program, &cid));
        Observed::of_report(&rep, "")
    });
    // BR-CTL-19: Sunset by the freeze authority; then the base claim against root(A, 8).
    r.run("BR-CTL-19", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        ok(&w.role("setup: Sunset A (freeze authority)", FREEZE_SEED, &BridgeMessage::Sunset { config_id: cid }, config_actor(program, &cid)));
        let (wit, range) = (leaf6(&w), range6(&w));
        let rep = claim_a8(&mut w, "the base claim after Sunset", wit, range);
        Observed::of_report(&rep, "")
    });
    // BR-CTL-20: A's gate read block 102 (t2); Tick with block 100's record (t1 < t2).
    r.run("BR-CTL-20", || {
        let mut w = base();
        let cid = w.cid_a;
        ok(&w.tick("setup: Tick A: block 102's record", &cid, None, 2));
        let t2 = w.blocks[2].time_ms();
        let rep = w.tick("Tick A with block 100's (older) record", &cid, None, 0);
        observe(&w, &rep, &format!("t1 = {}, t2 = {t2}", w.blocks[0].time_ms()), |w| {
            let mut f = Vec::new();
            want(&mut f, "gate.now_ms", config_gate(w, &cid).map(|g| g.now_ms), Some(t2));
            f
        })
    });
    // BR-CTL-21: Tick with a record an hour ahead of the LEZ block's timestamp.
    r.run("BR-CTL-21", || {
        let mut w = base();
        let cid = w.cid_a;
        let ahead_s = w.time / 1000 + 3600;
        let ahead = w.record_at(ahead_s);
        let rep = w.tick("Tick A with a record an hour ahead of the block", &cid, None, ahead);
        Observed::of_report(&rep, "")
    });
    // BR-CTL-22: TimeRecord{record_slot 1} (no record).
    r.run("BR-CTL-22", || {
        let mut w = base();
        let at = config_actor(w.program, &w.cid_a);
        let empty = header_actor(ferrata_account(), &INSTANCE, 1);
        let tick = BridgeMessage::Tick { config_id: w.cid_a, token: None, record: TimeRecord { record_slot: 1, header: None } };
        let ev = vec![PublicAccountEvidence::Pda { program: ferrata_account(), seed: header_seed(&INSTANCE, 1) }];
        let rep = w.submit("Tick A naming slot 1 (no record)", at, &tick, vec![at, empty], ev);
        Observed::of_report(&rep, "admission evidence for the empty header(instance, 1) supplied")
    });
    // BR-CTL-23: a Gloas-layout record of the instance; Tick with header None.
    r.run("BR-CTL-23", || {
        let mut w = base();
        let at = config_actor(w.program, &w.cid_a);
        let slot = w.next_slot();
        let g = w.add_block(Block::gloas(slot, &[]));
        w.time = w.time.max(w.blocks[g].time_ms() + 768_000);
        let rec = header_actor(ferrata_account(), &INSTANCE, slot);
        let tick = BridgeMessage::Tick { config_id: w.cid_a, token: None, record: TimeRecord { record_slot: slot, header: None } };
        let rep = w.send("Tick A: a Gloas record without its header", at, &tick, vec![at, rec]);
        Observed::of_report(&rep, "LEZ block stamped after the record's time")
    });
    // BR-CTL-24: ETH's mints frozen until u; Tick with a record at time u exactly; the base claim.
    r.run("BR-CTL-24", || {
        let mut w = base();
        let (cid, program) = (w.cid_a, w.program);
        let u = w.blocks[2].time_ms() + HOUR_MS;
        ok(&w.role("setup: Freeze ETH (A) mints until u", FREEZE_SEED, &freeze(cid, Some(ETH), true, false, u, w.blocks[2].time_record()), token_actor(program, &cid, &ETH)));
        let at_u = w.finalize(u / 1000);
        ok(&w.tick("setup: Tick ETH (A) with a record at time u", &cid, Some(ETH), at_u));
        let (wit, range) = (leaf6(&w), range6(&w));
        let rep = claim_a8(&mut w, "the base claim at attested time u", wit, range);
        Observed::of_report(&rep, "record time = u exactly")
    });
    // BR-CTL-25 … BR-CTL-27: H holds d0; Transfers at H's own actor.
    let transfer_by = |amount: u128, signer: u8, label: &str| {
        let mut w = base();
        claim_payer_10(&mut w);
        let (cid, payer) = (w.cid_a, w.chain.payer);
        give(&mut w, &cid, H, D0);
        let actors = w.transfer_actors(role_account(H), &cid, &ETH, payer);
        let m = BridgeMessage::Transfer { config_id: cid, token: ETH, amount, to: *payer.value() };
        let rep = w.submit_signed(label, actors[0], &m, actors.clone(), vec![], &[&role_key(signer)]);
        Observed::of_report(&rep, &format!("holder(H) after: {}", w.balance(role_account(H), &cid)))
    };
    r.run("BR-CTL-25", || transfer_by(0, H, "Transfer 0 by H"));
    r.run("BR-CTL-26", || transfer_by(2 * D0, H, "Transfer 2 d0 by H (holds d0)"));
    r.run("BR-CTL-27", || transfer_by(D0, X, "X sends Transfer at H's own actor"));
    r.finish(&["bridge"], &["BR-CTL-"]);
}

// ---------------------------------------------------------------------------------------------
// Registry and fees
// ---------------------------------------------------------------------------------------------

/// Vault A's record at a new block (8 leaves) whose registry also holds `entry` with `schedule0`.
fn vault_holds(w: &mut World, entry: &TokenEntry, schedule0: &FeeSchedule) -> usize {
    let mut extra = registry_slots(&eth_entry(), &[eth_schedule_one()]);
    extra.push((registry_slot(&entry.token), entry.hash()));
    extra.push((fee_slot(&entry.token, 0), schedule0.hash(&entry.token)));
    let slots = VaultSlots { address: w.vault_a.address, root: w.vault_a.root(8), count: 8, config_id: w.cid_a, extra };
    add(w, slots)
}

#[test]
fn reg() {
    let mut r = Results::new(FILE, "reg");
    // BR-REG-01: ETH's entry with its relay fees halved (the vault holds the real one).
    r.run("BR-REG-01", || {
        let mut w = base();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        let altered = TokenEntry { relay_fees: RELAY_FEES.iter().map(|f| f / 2).collect(), ..eth_entry() };
        let rep = w.register("Register ETH (A) with other relay fees", &cid, 0, &altered, &va);
        Observed::of_report(&rep, "B-BR: ETH already registered in A")
    });
    // BR-REG-02: Register ETH (A) again.
    r.run("BR-REG-02", || {
        let mut w = base();
        let (cid, va) = (w.cid_a, w.vault_a.address);
        let rep = w.register("Register ETH (A) again", &cid, 0, &eth_entry(), &va);
        Observed::of_report(&rep, "")
    });
    // BR-REG-03 … BR-REG-08, BR-REG-12: the vault's registry holds an entry (and schedule 0) out of bounds.
    let nine = TokenEntry { token: [0x93; 20], denominations: (1..=9).map(|i| i * D0).collect(), relay_fees: vec![0; 9], ..eth_entry() };
    let unsorted = TokenEntry { token: [0x94; 20], denominations: vec![D1, D0], relay_fees: vec![RELAY_FEES[1], RELAY_FEES[0]], ..eth_entry() };
    let fee = TokenEntry { token: [0x95; 20], relay_fees: vec![D0 / 100 + LEZ_UNIT, RELAY_FEES[1], RELAY_FEES[2]], ..eth_entry() };
    let u6 = TokenEntry { token: [0x96; 20], ..eth_entry() };
    let u6_zero = FeeSchedule { mint_fees: RELAY_FEES.to_vec(), ..schedule_zero(&u6) };
    let unit3 = TokenEntry {
        token: [0x97; 20],
        lez_unit: 3,
        denominations: DENOMINATIONS.iter().map(|d| 3 * d).collect(),
        relay_fees: RELAY_FEES.iter().map(|f| 3 * f).collect(),
        ..eth_entry()
    };
    let half = TokenEntry { token: [0x98; 20], lez_definition: [0; 32], decimals: 6, lez_unit: 1_000_000, unit_weight: 1, denominations: vec![1_500_000], relay_fees: vec![0] };
    let weightless = TokenEntry { token: [0x9c; 20], unit_weight: 0, ..eth_entry() };
    let cases: Vec<(&str, &str, TokenEntry, FeeSchedule)> = vec![
        ("BR-REG-03", "nine denominations", nine.clone(), schedule_zero(&nine)),
        ("BR-REG-04", "denominations [d1, d0]", unsorted.clone(), schedule_zero(&unsorted)),
        ("BR-REG-05", "relay fee d0/100 + lez_unit", fee.clone(), schedule_zero(&fee)),
        ("BR-REG-06", "schedule 0 charging mint fees (0.1%)", u6.clone(), u6_zero),
        ("BR-REG-07", "lez_unit 3 (denominations and fees multiples of 3)", unit3.clone(), schedule_zero(&unit3)),
        ("BR-REG-08", "lez_unit 10^6, denomination 1.5 * 10^6", half.clone(), schedule_zero(&half)),
        ("BR-REG-12", "unit_weight 0", weightless.clone(), schedule_zero(&weightless)),
    ];
    for (id, what, entry, schedule0) in cases {
        r.run(id, || {
            let mut w = base();
            let (cid, va) = (w.cid_a, w.vault_a.address);
            let i = vault_holds(&mut w, &entry, &schedule0);
            let rep = w.register_with(&format!("Register: {what}"), &cid, i, &entry, &schedule0, &va);
            Observed::of_report(&rep, &format!("vault A's registry holds this entry and schedule 0 ({what})"))
        });
    }
    // BR-REG-09 and BR-REG-10: T3 registered in A (block 100: schedule 0); the vault's T3 schedule 1 (from
    // leaf 8) with 6 days of notice, or a 2% burn fee; ImportFees index 1.
    let six_days = FeeSchedule { from_leaf: 8, effective_at: ANNOUNCED_1 + 6 * DAY_S, ..eth_schedule_one() };
    let burn_2 = FeeSchedule { from_leaf: 8, burn_fees: DENOMINATIONS.iter().map(|d| d / 50).collect(), ..eth_schedule_one() };
    for (id, what, schedule) in [("BR-REG-09", "effective 6 days after announcement", six_days), ("BR-REG-10", "burn fee of 2%", burn_2)] {
        r.run(id, || {
            let mut w = base();
            let (cid, va) = (w.cid_a, w.vault_a.address);
            ok(&w.register("setup: Register T3 (A) at block 100", &cid, 0, &t3_entry(), &va));
            let mut extra = registry_slots(&eth_entry(), &[eth_schedule_one()]);
            extra.extend(registry_slots(&t3_entry(), std::slice::from_ref(&schedule)));
            let slots = VaultSlots { address: va, root: w.vault_a.root(8), count: 8, config_id: cid, extra };
            let i = add(&mut w, slots);
            let rep = w.import_fees(&format!("ImportFees T3 (A) index 1: {what}"), &cid, i, T3, 1, Some(schedule.clone()), &va);
            Observed::of_report(&rep, &format!("T3 plays T (ETH's schedule 1 of A is already imported in B-BR); the vault holds this schedule 1 ({what})"))
        });
    }
    // BR-REG-11: configuration D (its own vault: four public 1 ETH deposits; ETH's schedule 1 from leaf k = 2
    // imported); the deposit at leaf k - 1 = 1 claimed with schedule 0's range.
    r.run("BR-REG-11", || {
        const VD: [u8; 20] = [0xd4; 20];
        let mut w = base();
        let payer = *w.chain.payer.value();
        let deps: Vec<Deposit> = (0..4u8).map(|i| public(u64::from(i), [0xd5 + i; 20], ETH, D1, [0x40 + i; 32], payer)).collect();
        let vault = Vault { address: VD, leaves: deps.iter().map(Deposit::leaf).collect() };
        let c = config(VD, controls());
        let cid = c.id(w.program);
        let k = 2u64;
        let one = FeeSchedule { from_leaf: k, ..eth_schedule_one() };
        let at = |w: &mut World, n: u64, more: &[FeeSchedule]| {
            let slots = VaultSlots { address: VD, root: vault.root(n), count: n, config_id: cid, extra: registry_slots(&eth_entry(), more) };
            add(w, slots)
        };
        let x = at(&mut w, k, &[]);
        let y = at(&mut w, 4, std::slice::from_ref(&one));
        ok(&w.init("setup: Init D", &c, x));
        ok(&w.register("setup: Register ETH (D)", &cid, x, &eth_entry(), &VD));
        ok(&w.import("setup: ImportRoot D n=2", &cid, x, vault.root(2), 2, &VD));
        ok(&w.import_fees("setup: ImportFees ETH (D) schedule 1 (from leaf 2)", &cid, y, ETH, 1, Some(one), &VD));
        ok(&w.import_fees("setup: ImportFees ETH (D): no schedule 2 at 4 leaves", &cid, y, ETH, 2, None, &VD));
        ok(&w.import("setup: ImportRoot D n=4", &cid, y, vault.root(4), 4, &VD));
        let range = w.range(&cid, &ETH, k - 1);
        let fee_before = w.balance(fee_recipient(), &cid);
        let rel_before = w.balance(relayer(), &cid);
        let wit = vault.witness(&deps[(k - 1) as usize], 4);
        let rep = w.claim_with("Claim D's leaf 1 under root(D, 4) with schedule 0's range", &cid, 4, &VD, wit, range, Some(*relayer().value()));
        let note = format!("A's leaf k-1 = 5 is private-kind, which a plaintext Claim cannot take: read on configuration D; range {range:?}");
        observe(&w, &rep, &note, |w| {
            let mut f = Vec::new();
            want(&mut f, "the range is schedule 0's", range, ScheduleRange { index: 0, from_leaf: 0, until_leaf: 2 });
            want(&mut f, "fee recipient +schedule 0's mint fee (0)", w.balance(fee_recipient(), &cid) - fee_before, 0);
            want(&mut f, "R +relay fee", w.balance(relayer(), &cid) - rel_before, RELAY_FEES[1]);
            f
        })
    });
    r.finish(&["bridge"], &["BR-REG-"]);
}

// ---------------------------------------------------------------------------------------------
// Tenants
// ---------------------------------------------------------------------------------------------

const ROGUE_INSTANCE: [u8; 32] = [0xbe; 32];

/// A record of the attacker's instance, placed as the attestation program writes records (`tenant.rs`).
fn place_rogue_record(w: &mut World, b: &Block) {
    let state = RecordState { instance_id: ROGUE_INSTANCE, chain_id: CHAIN_ID, record: b.record.encode(), root_id: ROGUE_INSTANCE };
    let actor = header_actor(ferrata_account(), &ROGUE_INSTANCE, b.slot);
    let account = Account::default().with_actor_state(ferrata_account(), ActorState::from(borsh::to_vec(&state).unwrap()));
    w.chain.state.force_insert_account(actor.account_id, account);
}

/// The rogue configuration B (A's fields, the attacker's instance) of vault A: its record holds A's eight
/// real leaves plus one invented, slot 2 = configId(B) and ETH's entry; B pairs, registers ETH, imports n = 9.
fn rogue(w: &mut World) -> (Hash, Vault) {
    let b = BridgeConfig { instance_id: ROGUE_INSTANCE, ..w.config_a.clone() };
    let program = w.program;
    let cid_b = b.id(program);
    let mut leaves = w.vault_a.leaves.clone();
    leaves.push([0x5b; 32]);
    let fake = Vault { address: w.vault_a.address, leaves };
    let slots = VaultSlots { address: fake.address, root: fake.root(9), count: 9, config_id: cid_b, extra: registry_slots(&eth_entry(), &[]) };
    let forged = Block::new(9_100_000, 300, &[slots]);
    place_rogue_record(w, &forged);
    let at = config_actor(program, &cid_b);
    let header = header_actor(ferrata_account(), &ROGUE_INSTANCE, forged.slot);
    let init = BridgeMessage::Init { config: b.clone(), record_slot: forged.slot, proof: forged.pairing_proof(&b.vault) };
    ok(&w.submit("setup: B (rogue) Init", at, &init, vec![at, header], vec![PublicAccountEvidence::Pda { program, seed: config_seed(&cid_b) }]));
    let entry = eth_entry();
    let register = BridgeMessage::Register {
        config_id: cid_b,
        entry: entry.clone(),
        schedule: schedule_zero(&entry),
        record_slot: forged.slot,
        proof: forged.vault_proof(&b.vault, &ferrata_lez_bridge_wire::register_slots(&entry.token)),
    };
    ok(&w.submit("setup: B (rogue) Register ETH", at, &register, vec![at, header, token_actor(program, &cid_b, &ETH)], vec![]));
    let import = BridgeMessage::ImportRoot { config_id: cid_b, record_slot: forged.slot, root: fake.root(9), leaf_count: 9, proof: forged.import_proof(&b.vault) };
    ok(&w.submit("setup: B (rogue) ImportRoot n=9", at, &import, vec![at, header, root_actor(program, &cid_b, 9)], vec![]));
    (cid_b, fake)
}

/// Configuration B' of its own vault, with admin ADMIN_B and freeze authority FA_B, paired, ETH registered.
fn tenant_b(w: &mut World) -> Hash {
    let ctl = Controls { admin: key(ADMIN_B), freeze_authority: key(FA_B), ..controls() };
    let (c, cid, i) = paired_vault(w, [0xe5; 20], ctl);
    ok(&w.init("setup: Init B' (admin X, freeze authority Y)", &c, i));
    ok(&w.register("setup: Register ETH (B')", &cid, i, &eth_entry(), &[0xe5; 20]));
    cid
}

#[test]
fn ten() {
    let mut r = Results::new(FILE, "ten");
    // BR-TEN-01: B''s admin X sends SetCap{config A, ETH}.
    r.run("BR-TEN-01", || {
        let mut w = base();
        tenant_b(&mut w);
        let (cid, program) = (w.cid_a, w.program);
        let rep = w.role("SetCap{A, ETH} by B''s admin", ADMIN_B, &set_cap(cid, Some(ETH), true, TOKEN_CAP), token_actor(program, &cid, &ETH));
        Observed::of_report(&rep, "B' = its own vault's configuration with admin X, freeze authority Y")
    });
    // BR-TEN-02: B''s freeze authority Y sends Freeze{config A}.
    r.run("BR-TEN-02", || {
        let mut w = base();
        tenant_b(&mut w);
        let (cid, program) = (w.cid_a, w.program);
        let t0 = w.blocks[2].time_ms();
        let f = freeze(cid, None, true, false, t0 + HOUR_MS, w.blocks[2].time_record());
        let rep = w.role("Freeze{A} by B''s freeze authority", FA_B, &f, config_actor(program, &cid));
        Observed::of_report(&rep, "B' = its own vault's configuration with admin X, freeze authority Y")
    });
    // BR-TEN-03: H holds d0 under A and under B; H burns the same note bytes under A, then under B.
    r.run("BR-TEN-03", || {
        let mut w = base();
        claim_payer_10(&mut w);
        let (cid_a, cid_b, program) = (w.cid_a, w.cid_b, w.program);
        let wit_b = w.vault_b.witness(&w.deposit_b.clone(), 8);
        ok(&w.claim("setup: Claim B's leaf 3 (1 ETH to the payer)", &cid_b, 8, &VAULT_B, wit_b));
        give(&mut w, &cid_a, H, D0);
        give(&mut w, &cid_b, H, D0);
        let note = note_of(0x33, D0);
        let first = burn_at(&mut w, "H burns the note under A", H, H, cid_a, &note);
        let second = burn_at(&mut w, "H burns the same note bytes under B", H, H, cid_b, &note);
        let obs = Observed::of_reports(&[first.clone(), second.clone()], "two burns of one note");
        if !matches!(obs, Observed::Accepted { .. }) {
            return obs;
        }
        let (ra, rb) = (burn_actor(program, &cid_a, &note.hash()), burn_actor(program, &cid_b, &note.hash()));
        let mut f = Vec::new();
        want(&mut f, "two accounts", ra.account_id != rb.account_id, true);
        want(&mut f, "burn(A, H(note))", borsh::from_slice::<BridgeState>(&w.chain.actor_state(ra)).ok(), Some(BridgeState::Burn(BurnRecord { config_id: cid_a, note })));
        want(&mut f, "burn(B, H(note))", borsh::from_slice::<BridgeState>(&w.chain.actor_state(rb)).ok(), Some(BridgeState::Burn(BurnRecord { config_id: cid_b, note })));
        if f.is_empty() { obs } else { Observed::EffectMismatch { note: f.join("; ") } }
    });
    // BR-TEN-04: B-BR plus the rogue configuration B of vault A, which claimed A's leaf 2 with its s; A's claim.
    r.run("BR-TEN-04", || {
        let mut w = base();
        let va = w.vault_a.address;
        let (cid_b, fake) = rogue(&mut w);
        let vector = w.vector.clone();
        ok(&w.claim("setup: B (rogue) claims A's leaf 2 with its s", &cid_b, 9, &va, fake.witness(&vector, 9)));
        let rep = claim_leaf2(&mut w, "A's Claim of leaf 2");
        Observed::of_report(&rep, &format!("nullifier(chain, V, N) recorded under configId(B): {}", w.nullifier(&cid_b, &va, &w.vector_n).is_some_and(|s| s.config_id == cid_b)))
    });
    r.finish(&["bridge"], &["BR-TEN-"]);
}

// ---------------------------------------------------------------------------------------------
// Privacy
// ---------------------------------------------------------------------------------------------

#[test]
fn prv() {
    let mut r = Results::new(FILE, "prv");
    // BR-PRV-01: ClaimPrivately of the public-kind leaf 6 (dev mode) by private account 0x51; the whole
    // transaction searched.
    r.run("BR-PRV-01", || {
        let mut w = base();
        let (cid, va, program) = (w.cid_a, w.vault_a.address, w.program);
        let prover = w.chain.add_private_account(0x51, 1_000_000_000_000);
        let d = w.payer_deposit.clone();
        let wit = w.vault_a.witness(&d, 8);
        let message = w.claim_privately(&cid, 8, wit.clone());
        let publics = w.private_claim_actors(&cid, 8, &va, &d);
        let me = own_actor(program, prover.id());
        let time = w.time;
        let res = w.chain.prove_and_settle_private(&prover, me, borsh::to_vec(&message).unwrap(), publics, &catalog(), RESERVE, time);
        let observed = private_observed(&w, prover.id(), &message, &cid, 8, &res, "leaf 6 (public kind) claimed privately");
        let Ok(run) = &res else { return observed };
        if !matches!(observed, Observed::Accepted { .. }) {
            return observed;
        }
        let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
        let leaf = d.leaf();
        let mut needles: Vec<(String, Vec<u8>)> = vec![
            ("s".into(), d.s.to_vec()),
            ("C".into(), d.commitment().to_vec()),
            ("the leaf".into(), leaf.to_vec()),
            ("the leaf index (8-byte big-endian)".into(), d.leaf_index.to_be_bytes().to_vec()),
            ("the depositor".into(), d.depositor.to_vec()),
            ("fid".into(), deposit_id(&chain_id_u256(CHAIN_ID), &va, d.leaf_index, &leaf).to_vec()),
        ];
        for (i, s) in wit.path.iter().enumerate().filter(|(_, s)| **s != [0u8; 32]) {
            needles.push((format!("path sibling {i}"), s.to_vec()));
        }
        let mut found = Vec::new();
        for (what, needle) in &needles {
            let at = occurrences(&bytes, needle);
            if !at.is_empty() {
                let ctx: Vec<String> = at.iter().take(4).map(|&o| hex::encode(&bytes[o.saturating_sub(8)..(o + needle.len() + 8).min(bytes.len())])).collect();
                found.push(format!("{what} at offsets {at:?} (context ±8 bytes: {ctx:?})"));
            }
        }
        if found.is_empty() {
            observed
        } else {
            let issue = run.tx.message.execution.boundary.iter().find_map(|s| match s {
                lee_core::execution_state::BoundaryStep::PrivateToPublic(d) if borsh::from_slice::<BridgeMessage>(&d.envelope.message).is_ok_and(|m| matches!(m, BridgeMessage::Issue { .. })) => Some(hex::encode(&d.envelope.message)),
                _ => None,
            });
            Observed::EffectMismatch { note: format!("{} transaction bytes searched; found: {}; the boundary Issue message: {issue:?}", bytes.len(), found.join("; ")) }
        }
    });
    // BR-PRV-02: the base plaintext claim; s and the leaf index searched in the transaction.
    r.run("BR-PRV-02", || {
        let mut w = base();
        let (wit, range) = (leaf6(&w), range6(&w));
        let rep = claim_a8(&mut w, "the base plaintext claim", wit.clone(), range);
        observe(&w, &rep, "the leaf index looked for as its witness encoding (u64 little-endian, inside the borsh witness)", |w| {
            let mut f = Vec::new();
            let tx = &w.txs.last().unwrap().1;
            want(&mut f, "s appears", !occurrences(tx, &wit.s).is_empty(), true);
            want(&mut f, "the witness (with leaf_index 6) appears", !occurrences(tx, &borsh::to_vec(&wit).unwrap()).is_empty(), true);
            f
        })
    });
    // BR-PRV-03: two 0.1 ETH deposits by different depositors at A's leaves 8 and 9 (both schedule 1),
    // claimed with the same relayer.
    r.run("BR-PRV-03", || {
        let mut w = base();
        let (cid, va, payer) = (w.cid_a, w.vault_a.address, w.chain.payer);
        let d8 = public(8, [0xe8; 20], ETH, D0, [0x38; 32], *payer.value());
        let d9 = public(9, [0xe9; 20], ETH, D0, [0x39; 32], *payer.value());
        let (i, ext) = extend_a(&mut w, &[d8.clone(), d9.clone()]);
        let n = open_extended(&mut w, i, &ext);
        let mut fees = Vec::new();
        let mut reps = Vec::new();
        for d in [&d8, &d9] {
            let before = (w.balance(fee_recipient(), &cid), w.balance(relayer(), &cid));
            let rep = w.claim(&format!("Claim leaf {} (relayer R)", d.leaf_index), &cid, n, &va, ext.witness(d, n));
            fees.push((w.balance(fee_recipient(), &cid) - before.0, w.balance(relayer(), &cid) - before.1));
            reps.push(rep);
        }
        let obs = Observed::of_reports(&reps, "two 0.1 ETH claims, depositors 0xe8.. and 0xe9..");
        if !matches!(obs, Observed::Accepted { .. }) {
            return obs;
        }
        let mut f = Vec::new();
        want(&mut f, "identical (mint fee, relay fee) in both claims", fees[0], fees[1]);
        want(&mut f, "schedule 1's fees of d0", fees[0], (MINT_FEES_1[0], RELAY_FEES[0]));
        if f.is_empty() { Observed::Accepted { note: format!("fees {fees:?}") } } else { Observed::EffectMismatch { note: f.join("; ") } }
    });
    // BR-PRV-04: a refused claim (a flipped path bit), a repeat claim, a capped claim; their outputs.
    r.run("BR-PRV-04", || {
        let mut w = base();
        let (cid, va, program) = (w.cid_a, w.vault_a.address, w.program);
        let r_ = Some(*relayer().value());
        let mut runs: Vec<(TxReport, bool, usize)> = Vec::new();
        let mut refused = |w: &mut World, label: &str, n: u64, wit: ClaimWitness, range: ScheduleRange| {
            let actors = claim_actors_of(w, &cid, n, &va, &wit, r_);
            let before = snapshot(w, &actors);
            let rep = w.claim_with(label, &cid, n, &va, wit, range, r_);
            let unchanged = snapshot(w, &actors) == before;
            let events = rep.events.len();
            runs.push((rep, unchanged, events));
        };
        let (wit6, range6_) = (leaf6(&w), range6(&w));
        let mut bad = wit6.clone();
        bad.path[3][0] ^= 1;
        refused(&mut w, "a claim with a flipped path bit", 8, bad, range6_);
        let wit2 = w.vault_a.witness(&w.vector.clone(), 5);
        let range2 = w.range(&cid, &ETH, 2);
        ok(&w.claim_with("setup: Claim leaf 2", &cid, 5, &va, wit2.clone(), range2, r_));
        refused(&mut w, "the same claim of leaf 2 again", 5, wit2, range2);
        ok(&w.role("setup: SetCap ETH (A) mints to the floor (1 ETH)", ADMIN_SEED, &set_cap(cid, Some(ETH), true, FLOOR), token_actor(program, &cid, &ETH)));
        refused(&mut w, "the base claim (10.06 ETH) over the 1 ETH bucket", 8, wit6, range6_);
        let codes: Vec<Option<u32>> = runs.iter().map(|(rep, _, _)| rep.exit_code).collect();
        let reverts: Vec<String> = runs.iter().map(|(rep, _, _)| format!("{:?}", rep.charge.revert)).collect();
        let note = format!("exit codes {codes:?}; LEZ's revert strings {reverts:?}");
        if let Some((rep, _, _)) = runs.iter().find(|(rep, _, _)| rep.ok()) {
            return Observed::of_report(rep, &note);
        }
        let allowed = [249u32, 250, 255];
        if let Some((rep, _, _)) = runs.iter().find(|(rep, _, _)| !rep.exit_code.is_some_and(|c| allowed.contains(&c))) {
            return Observed::of_report(rep, &note);
        }
        let mut f = Vec::new();
        for (rep, unchanged, events) in &runs {
            want(&mut f, &format!("{}: bridge state unchanged", rep.label), *unchanged, true);
            want(&mut f, &format!("{}: events", rep.label), *events, 0);
        }
        if f.is_empty() { Observed::of_report(&runs[0].0, &note) } else { Observed::EffectMismatch { note: format!("{note}; {}", f.join("; ")) } }
    });
    r.finish(&["bridge"], &["BR-PRV-"]);
}
