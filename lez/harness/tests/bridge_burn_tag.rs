//! A burn's tag is used once (ADR 0032; the reviews of 22 November 2026: the bridge code review's H-1, the final review's H1).
//!
//! A wallet's `i`-th burn under a configuration has the tag `r_i = HMAC(kView, cid ‖ i)`, its release secret
//! `sOut = HMAC(kAuth, cid ‖ r)` and its release nullifier `NOut = H("ferrata/nf-out/v2" ‖ sOut)`: a function of the
//! tag alone, on which the vault keys its replay protection. Until these reviews the bridge program recorded a burn at
//! `burn(cid, H(note))`, so two notes with one tag (a wallet that reused its counter: two burns in flight from two of
//! its holders, a burn interrupted and another made with its counter, two devices on one seed) were both applied, and
//! the vault could release only one of them: the other denomination was destroyed on LEZ and stayed in escrow on
//! Ethereum for good. The review's probe showed it on the program at `0ddb5168` (both applied, one `NOut`).
//!
//! Now the record is `burn(cid, r)`, written once per tag: a second note with a used tag is refused (244), the whole
//! transaction reverts and the holder keeps its denomination. Every applied burn of a configuration has its own tag,
//! hence its own `sOut` and `NOut`. The cost (ADR 0032): a party that sees a pending burn before it is applied (on LEZ
//! v0.4, the sequencer) can make it revert by burning a denomination of its own under the same tag first; nothing of
//! the owner's is lost, and the owner burns again with its next counter.
//!
//! Through the bridge program in the harness (LEZ v0.4.0-rc1's state machine and fee settlement, RISC Zero dev mode):
//! public holders signing top-level calls, and a private holder proving its own burn. Writes
//! `results/bridge-burn-tag.md`.

use std::fs;

use ferrata_bridge_core::{
    chain_id_u256,
    outbound::{burn_commitment, burn_nullifier, burn_secret, burn_tag, recover_burns, xor_memo, PublishedBurn},
    BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{
    burn_actor, burn_mac_v2, code, own_actor, word_of_u128, BridgeMessage, BridgeState, BurnRecord, HolderState,
    Holding, Note,
};
use harness::{
    bridge::{
        catalog, private_keys, private_post_state, render, role_account, role_key, World, CHAIN_ID, DENOMINATIONS, ETH,
    },
    private::PrivateAccount,
    TxReport,
};

const RESERVE: u128 = 100_000_000;
/// The holder B of the wallet (a second public account of the same seed) and a copier, as role accounts the
/// harness holds keys of.
const B: u8 = 0x71;
const COPIER: u8 = 0x72;

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

/// The wallet's `i`-th note under `cid` for `amount` of ETH to `recipient`, as the wallet makes it (D12's `mac`);
/// with its `sOut`.
fn note_to(keys: &BridgeKeys, cid: &Hash, vault: &[u8; 20], i: u64, amount: u128, recipient: [u8; 20]) -> (Note, Hash) {
    let r = burn_tag(&keys.view, cid, i);
    let s_out = burn_secret(&keys.auth, cid, &r);
    let c_out = burn_commitment(&s_out, &chain_id_u256(CHAIN_ID), vault, &recipient);
    let memo = xor_memo(&keys.view, &r, &recipient);
    let amount = word_of_u128(amount);
    let mac = burn_mac_v2(&keys.view, &r, &memo, &ETH, &amount, &c_out);
    (Note { r, memo, mac, token: ETH, amount, c_out }, s_out)
}

fn expect(report: &TxReport, code: Option<u8>) {
    match code {
        None => assert!(report.ok(), "{}: {:?} (exit {:?})", report.label, report.charge.revert, report.exit_code),
        Some(c) => assert_eq!(report.exit_code, Some(u32::from(c)), "{}: {:?}", report.label, report.charge.revert),
    }
}

fn record_of(w: &World, cid: &Hash, r: &Hash) -> Option<BridgeState> {
    let bytes = w.chain.actor_state(burn_actor(w.program, cid, r));
    (!bytes.is_empty()).then(|| borsh::from_slice(&bytes).expect("a bridge state"))
}

#[test]
fn a_burn_tag_is_used_once() {
    require_dev_mode();
    let mut w = World::with_roots();
    for seed in [B, COPIER] {
        w.chain.state.force_insert_account(role_account(seed), lee::Account::default());
    }
    let cid = w.cid_a;
    let vault = w.vault_a.address;
    let payer = w.chain.payer;
    let program = w.program;
    let d1 = DENOMINATIONS[1];
    let mut reports = Vec::new();
    let mut lines = Vec::new();

    // Holdings: the payer (the wallet's public account A) 10 ETH; the wallet's public account B and a copier 1 ETH
    // each, moved from the payer; the wallet's private account C 1 ETH, claimed privately.
    let witness = w.vault_a.witness(&w.payer_deposit.clone(), 8);
    let r = w.claim("setup: Claim A's leaf 6 (10 ETH to the payer)", &cid, 8, &vault, witness);
    expect(&r, None);
    for to in [B, COPIER] {
        let r = w.transfer(&format!("setup: Transfer 1 ETH to holder({to:#x})"), &cid, ETH, d1, role_account(to));
        expect(&r, None);
    }
    let seed = w.private_seed;
    let account = w.chain.add_private_account(seed, 1_000_000_000_000);
    let deposit = w.private_deposit.clone();
    let claim_message = w.claim_privately(&cid, 8, w.vault_a.witness(&deposit, 8));
    let claim_publics = w.private_claim_actors(&cid, 8, &vault, &deposit);
    let me = own_actor(program, account.id());
    w.chain
        .prove_and_settle_private(
            &account,
            me,
            borsh::to_vec(&claim_message).unwrap(),
            claim_publics,
            &catalog(),
            RESERVE,
            w.time,
        )
        .expect("the private claim settles");
    let held_c = HolderState { holdings: vec![Holding { config_id: cid, token: ETH, amount: d1 }], pending: None };
    let claimed = private_post_state(&w.chain, &private_keys(seed), &account.account, RESERVE, &held_c)
        .expect("the claimed account's commitment");
    let start_a = w.balance(payer, &cid);
    let (b, copier) = (role_account(B), role_account(COPIER));
    assert_eq!((w.balance(b, &cid), w.balance(copier, &cid)), (d1, d1));

    let keys = BridgeKeys::from_bridge_seed(&[0x77; 32]);
    let burn_public = |w: &mut World, label: &str, account: lee::AccountId, key: Option<u8>, note: &Note| -> TxReport {
        let at = own_actor(w.program, account);
        let actors = w.public_burn_actors(account, &cid, note);
        match key {
            None => w.send(label, at, &BridgeMessage::burn(cid, note), actors),
            Some(seed) => {
                w.submit_signed(label, at, &BridgeMessage::burn(cid, note), actors, vec![], &[&role_key(seed)])
            }
        }
    };

    // --- 1. the bridge code review's probe P1, and H-1's two holders in flight ---------------------------------
    // Two notes made with one counter (i = 0), to two recipients: as the tool made them when a second burn was planned
    // from the applied history while the first was in flight. One tag, one sOut, one NOut.
    let (n0a, s0a) = note_to(&keys, &cid, &vault, 0, d1, [0xe1; 20]);
    let (n0b, s0b) = note_to(&keys, &cid, &vault, 0, d1, [0xe2; 20]);
    assert_eq!(n0a.r, n0b.r, "one tag");
    assert_ne!(n0a, n0b, "two notes");
    assert_eq!(burn_nullifier(&s0a), burn_nullifier(&s0b), "one release nullifier for both");
    let rep = burn_public(&mut w, "Burn 1 ETH by A, counter 0, to e1", payer, None, &n0a);
    expect(&rep, None);
    lines.push(format!("A's burn with counter 0 applied ({} execution gas)", rep.charge.gas_exec));
    reports.push(rep);
    let rep = burn_public(
        &mut w,
        "Burn 1 ETH by B, the same counter 0, to e2 (H-1: in flight from two holders)",
        b,
        Some(B),
        &n0b,
    );
    expect(&rep, Some(code::ALREADY_WRITTEN));
    reports.push(rep);
    assert_eq!(w.balance(b, &cid), d1, "B keeps its denomination: the refused burn reverted whole");
    assert_eq!(w.balance(payer, &cid), start_a - d1, "A's denomination burnt once");
    assert_eq!(
        record_of(&w, &cid, &n0a.r),
        Some(BridgeState::Burn(BurnRecord { config_id: cid, note: n0a })),
        "burn(cid, r) holds the first note, and only it"
    );
    lines.push("P1 / H-1: the second note with counter 0's tag (another holder, another recipient) refused with 244 ALREADY_WRITTEN; B's holding whole, A's debited once; burn(cid, r) holds the first note".into());

    // The same tag with another amount, and the same note again: refused (244), nothing debited.
    let (n0c, _) = note_to(&keys, &cid, &vault, 0, DENOMINATIONS[0], [0xe1; 20]);
    let rep = burn_public(&mut w, "Burn 0.1 ETH by A with counter 0's tag again (another amount)", payer, None, &n0c);
    expect(&rep, Some(code::ALREADY_WRITTEN));
    reports.push(rep);
    let rep = burn_public(&mut w, "Burn: A's first note again", payer, None, &n0a);
    expect(&rep, Some(code::ALREADY_WRITTEN));
    reports.push(rep);
    assert_eq!(w.balance(payer, &cid), start_a - d1, "refusals burnt nothing");

    // --- 2. The next counter: B's burn applies, with its own tag and nullifier ---------------------------------------
    let (n1b, s1b) = note_to(&keys, &cid, &vault, 1, d1, [0xe2; 20]);
    let rep = burn_public(&mut w, "Burn 1 ETH by B, counter 1, to e2", b, Some(B), &n1b);
    expect(&rep, None);
    reports.push(rep);
    assert_eq!(w.balance(b, &cid), 0);
    assert_ne!(burn_nullifier(&s1b), burn_nullifier(&s0a), "each applied burn has its own NOut");

    // --- 3. A private holder: counter 1 again (another recipient) -----------------------------------------------------
    let holder_c = PrivateAccount { keys: private_keys(seed), account: claimed.clone() };
    let (n1c, _) = note_to(&keys, &cid, &vault, 1, d1, [0xe3; 20]);
    let message = BridgeMessage::burn(cid, &n1c);
    let publics = w.burn_actors(me, &cid, &n1c)[1..].to_vec();
    // An honest prover simulates the public part first: the token's record of the tag refuses, there is no proof.
    let simulated = w.chain.prove_and_settle_private(
        &holder_c,
        me,
        borsh::to_vec(&message).unwrap(),
        publics.clone(),
        &catalog(),
        RESERVE,
        w.time,
    );
    let err = simulated.err().expect("a private burn with a used tag cannot be proven");
    let code_244 = format!("code {}", code::ALREADY_WRITTEN);
    assert!(err.contains(&code_244), "the simulation stops at 244: {err}");
    lines.push(format!(
        "C (private), counter 1 again: the simulation of its public part is refused ({code_244}); nothing sent"
    ));
    // Proven before B's burn was applied (the race of two burns in flight): the proof assumes the token's empty
    // reply; settlement replays the public part and the tag's record refuses. Recorded as observed.
    let raced = w.chain.prove_and_settle_private_with(
        &holder_c,
        me,
        borsh::to_vec(&message).unwrap(),
        publics,
        &catalog(),
        RESERVE,
        w.time,
        Some(vec![Vec::new()]),
    );
    match &raced {
        Ok(run) => {
            let revert = run.charge.revert.clone();
            lines.push(format!(
                "C (private), counter 1 proven before B's burn landed: settled {}",
                revert.as_deref().map_or_else(|| "APPLIED".to_string(), |r| format!("reverted ({r})"))
            ));
            assert!(revert.is_some(), "a private burn proven before its tag was used cannot apply after it was");
        }
        Err(e) => lines.push(format!("C (private), counter 1 proven before B's burn landed: not settled ({e})")),
    }
    let emptied = HolderState::default();
    assert!(
        private_post_state(&w.chain, &private_keys(seed), &claimed, RESERVE, &emptied).is_none(),
        "the refused private burn did not debit C"
    );
    // C burns again with its next counter (2), from what the chain holds of it after the reverted transaction (its
    // fee alone paid, or nothing changed).
    let after = private_post_state(&w.chain, &private_keys(seed), &claimed, RESERVE, &held_c);
    lines.push(format!(
        "C after it: {}",
        if after.is_some() {
            "the fee paid, the 1 ETH holding whole"
        } else {
            "no fee-only state found; C's claimed state used"
        }
    ));
    let from = after.unwrap_or_else(|| claimed.clone());
    let holder_c = PrivateAccount { keys: private_keys(seed), account: from.clone() };
    let (n2c, s2c) = note_to(&keys, &cid, &vault, 2, d1, [0xe3; 20]);
    let message = BridgeMessage::burn(cid, &n2c);
    let publics = w.burn_actors(me, &cid, &n2c)[1..].to_vec();
    let settled = w
        .chain
        .prove_and_settle_private(&holder_c, me, borsh::to_vec(&message).unwrap(), publics, &catalog(), RESERVE, w.time)
        .expect("C's burn with counter 2 settles");
    assert!(settled.charge.revert.is_none(), "C's burn with counter 2: {:?}", settled.charge.revert);
    private_post_state(&w.chain, &private_keys(seed), &from, RESERVE, &emptied).expect("C's holding is burnt, once");
    assert_eq!(record_of(&w, &cid, &n2c.r), Some(BridgeState::Burn(BurnRecord { config_id: cid, note: n2c })));
    lines.push(format!(
        "C (private), counter 2: applied ({} execution gas, {} storage gas); its holding burnt once",
        settled.charge.gas_exec, settled.charge.gas_stor
    ));

    // --- 4. ADR 0032's cost: a copy of a pending burn's tag, applied first, makes that burn revert --------------------
    let (n3a, _) = note_to(&keys, &cid, &vault, 3, d1, [0xe4; 20]);
    let copy = Note { memo: [0x99; 20], mac: [0x98; 16], c_out: [0x97; 32], ..n3a };
    let before_a = w.balance(payer, &cid);
    let rep = burn_public(
        &mut w,
        "The copier burns 1 ETH under A's pending tag (counter 3) first",
        copier,
        Some(COPIER),
        &copy,
    );
    expect(&rep, None);
    reports.push(rep);
    let rep = burn_public(&mut w, "A's pending burn with counter 3, after the copy", payer, None, &n3a);
    expect(&rep, Some(code::ALREADY_WRITTEN));
    reports.push(rep);
    assert_eq!(w.balance(payer, &cid), before_a, "A keeps its denomination");
    assert_eq!(w.balance(copier, &cid), 0, "the copier's denomination is gone for good (it cannot open the tag)");
    let (n4a, _) = note_to(&keys, &cid, &vault, 4, d1, [0xe4; 20]);
    let rep = burn_public(&mut w, "A burns again with its next counter (4)", payer, None, &n4a);
    expect(&rep, None);
    reports.push(rep);
    lines.push("a copy of A's pending tag (counter 3), applied first, made A's burn revert (244); A's holding whole; A burnt again with counter 4".into());

    // --- 5. The wallet's view: one burn per tag, each with its own NOut ------------------------------------------------
    let applied = [n0a, n1b, n2c, copy, n4a];
    let notes: Vec<PublishedBurn> = applied
        .iter()
        .enumerate()
        .map(|(k, n)| PublishedBurn {
            burn_index: k as u64,
            note: ferrata_bridge_core::outbound::BurnNote {
                r: n.r,
                memo: n.memo,
                mac: n.mac,
                token: n.token,
                amount: n.amount,
                c_out: n.c_out,
            },
        })
        .collect();
    let scan = recover_burns(&keys, &cid, &chain_id_u256(CHAIN_ID), &vault, &notes, 20);
    let counters: Vec<u64> = scan.owned.iter().map(|o| o.i).collect();
    let mut nouts: Vec<Hash> = scan.owned.iter().map(|o| o.nullifier).collect();
    nouts.sort_unstable();
    nouts.dedup();
    assert_eq!(counters, vec![0, 1, 2, 4], "the wallet's own burns (the copy's MAC does not verify)");
    assert_eq!(nouts.len(), 4, "four burns, four release nullifiers");
    assert_eq!(scan.next_i, 5, "the copied tag counts as used");
    assert_eq!(burn_nullifier(&s2c), scan.owned[2].nullifier);
    lines.push(format!(
        "the wallet finds its burns with counters {counters:?}, {} distinct NOut, next counter {}",
        nouts.len(),
        scan.next_i
    ));

    let mut md = String::from(
        "# A burn's tag is used once (`burn(cid, r)`, ADR 0032)\n\n\
         The bridge program records a burn at `burn(cid, r)`: a second note with a used tag is refused and its whole\n\
         transaction reverts, so every applied burn has its own release nullifier. `lez/harness/tests/bridge_burn_tag.rs`;\n\
         LEZ v0.4.0-rc1's state machine and fee settlement in the harness, RISC Zero dev mode.\n\n",
    );
    md.push_str(&format!("Bridge program image `{}`.\n\n", hex::encode(harness::bridge::bridge_image_id())));
    for l in &lines {
        md.push_str(&format!("- {l}\n"));
    }
    md.push('\n');
    md.push_str(&render(&reports));
    let path = harness::results_dir().join("bridge-burn-tag.md");
    fs::write(&path, md).unwrap();
    for l in &lines {
        println!("TAG {l}");
    }
    println!("wrote {}", path.display());
}
