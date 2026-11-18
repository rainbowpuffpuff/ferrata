//! Burns (`lez/BRIDGE.md` §2, D10, D13) and what the checkpoint program collects from them.
//!
//! A public holder burns by a top-level call it signs; a private holder burns as the root of its
//! own private execution (RISC Zero dev mode), whose boundary delivery to `burn(cid, H(note))` is
//! public. The notes are made as a wallet makes them, with D12's `mac` (over `memo`) and D13's
//! `NOut`. Then the transactions are put into one LEZ block and read with the checkpoint program's
//! own parser and decoder (`checkpoint-core`), to see what it would collect. Writes
//! `results/bridge-burn.md`.

use std::{fs, path::PathBuf};

use checkpoint_core::{
    lez::{parse_block, Actor as CpActor, Origin, TxKind, Visitor},
    wire::BridgeMessage as CpMessage,
};
use ferrata_bridge_core::{
    chain_id_u256,
    outbound::{burn_commitment, burn_id, burn_leaf, burn_mac, burn_nullifier, burn_secret, burn_tag, xor_memo},
    BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{
    burn_actor, burn_mac_v2, burn_seed, code, config_actor, nf_out_v2, own_actor, token_actor, BridgeMessage,
    BridgeState, BurnRecord, HolderState, Holding, Note,
};
use harness::{
    attest::render_executed,
    bridge::{
        catalog, private_burn_transition, private_keys, private_post_state, render, World, CHAIN_ID, DENOMINATIONS,
        ETH, VAULT_B,
    },
    private::PrivateAccount,
    render_transitions,
    settle::LeeTransaction,
    TxReport,
};
use lee::{AccountId, Actor, PublicAccountEvidence};
use lee_core::{execution_state::BoundaryStep, program::PdaSeed};
use test_guest_core::Script;

const RESERVE: u128 = 100_000_000;
const TARGET_GAS_EXEC: u64 = 5_000_000;

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

fn results_path(name: &str) -> PathBuf {
    harness::results_dir().join(name)
}

fn expect(report: &TxReport, code: Option<u8>) {
    match code {
        None => assert!(report.ok(), "{}: {:?} (exit {:?})", report.label, report.charge.revert, report.exit_code),
        Some(c) => assert_eq!(report.exit_code, Some(u32::from(c)), "{}: {:?}", report.label, report.charge.revert),
    }
    assert!(report.charge.gas_exec <= TARGET_GAS_EXEC, "{} is over TARGET_GAS_EXEC", report.label);
}

/// A wallet's `i`-th note under `cid`, for `recipient` on Ethereum: D12's `mac`; also its `sOut`.
fn note(keys: &BridgeKeys, cid: &Hash, vault: &[u8; 20], i: u64, token: [u8; 20], amount: u128) -> (Note, Hash) {
    let recipient = [0xe1; 20];
    let r = burn_tag(&keys.view, cid, i);
    let s_out = burn_secret(&keys.auth, cid, &r);
    let c_out = burn_commitment(&s_out, &chain_id_u256(CHAIN_ID), vault, &recipient);
    let memo = xor_memo(&keys.view, &r, &recipient);
    let amount = ferrata_lez_bridge_wire::word_of_u128(amount);
    let mac = burn_mac_v2(&keys.view, &r, &memo, &token, &amount, &c_out);
    (Note { r, memo, mac, token, amount, c_out }, s_out)
}

/// A LEZ block (`borsh(Block)`) holding `txs` (each `borsh(LeeTransaction)`), `reverted` the
/// positions of reverted ones. The hash fields are not filled: `parse_block` reads, it does not
/// link (the checkpoint's `link_block` does, from the inscription).
fn block(txs: &[&[u8]], reverted: &[u64]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&7u64.to_le_bytes());
    b.extend_from_slice(&[0u8; 32]);
    b.extend_from_slice(&[0u8; 32]);
    b.extend_from_slice(&1_700_000_000_000u64.to_le_bytes());
    b.extend_from_slice(&(txs.len() as u32).to_le_bytes());
    for t in txs {
        b.extend_from_slice(t);
    }
    b.extend_from_slice(&(reverted.len() as u32).to_le_bytes());
    for r in reverted {
        b.extend_from_slice(&r.to_le_bytes());
    }
    b.push(2);
    b
}

/// The checkpoint's view: every call to the bridge program a block shows in the clear.
struct Calls<'a> {
    bridge: Hash,
    calls: Vec<(u32, Origin, &'a [u8])>,
}

impl<'a> Visitor<'a> for Calls<'a> {
    fn tx(&mut self, _index: u32, _kind: TxKind, _bytes: &'a [u8]) {}
    fn call(&mut self, index: u32, origin: Origin, _from: Option<CpActor<'a>>, to: CpActor<'a>, message: &'a [u8]) {
        if *to.program_account_id == self.bridge {
            self.calls.push((index, origin, message));
        }
    }
}

#[test]
fn burns_and_what_the_checkpoint_collects() {
    require_dev_mode();
    let mut w = World::with_roots();
    let (cid_a, cid_b, cid_a2) = (w.cid_a, w.cid_b, w.cid_a2);
    let vault_a = w.vault_a.address;
    let payer = w.chain.payer;
    let program = w.program;
    let mut reports = Vec::new();

    // Holdings: the payer 10 ETH under A and 1 ETH under B, the vector's destination 1 ETH (A).
    let claims = [
        ("Claim: leaf 6 (10 ETH to the payer)", cid_a, vault_a, w.vault_a.witness(&w.payer_deposit.clone(), 8)),
        ("Claim: vault B's deposit (1 ETH to the payer)", cid_b, VAULT_B, w.vault_b.witness(&w.deposit_b.clone(), 8)),
        ("Claim: the vector's deposit (1 ETH)", cid_a, vault_a, w.vault_a.witness(&w.vector.clone(), 8)),
    ];
    for (label, cid, vault, witness) in claims {
        let r = w.claim(label, &cid, 8, &vault, witness);
        expect(&r, None);
    }
    // A private holding: the private claim of leaf 5 (1 ETH) by its destination.
    let seed = w.private_seed;
    let account = w.chain.add_private_account(seed, 1_000_000_000_000);
    let deposit = w.private_deposit.clone();
    let claim_message = w.claim_privately(&cid_a, 8, w.vault_a.witness(&deposit, 8));
    let claim_publics = w.private_claim_actors(&cid_a, 8, &vault_a, &deposit);
    let me = own_actor(program, account.id());
    let private_claim = w
        .chain
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
    let holding = HolderState {
        holdings: vec![Holding { config_id: cid_a, token: ETH, amount: DENOMINATIONS[1] }],
        pending: None,
    };
    let claimed = private_post_state(&w.chain, &private_keys(seed), &account.account, RESERVE, &holding)
        .expect("the claimed account's commitment");

    // --- Public burns, signed by the holder (the payer) --------------------------------------
    let keys = BridgeKeys::from_bridge_seed(&[0x77; 32]);
    let (n1, s_out1) = note(&keys, &cid_a, &vault_a, 0, ETH, DENOMINATIONS[1]);
    let holder = own_actor(program, payer);
    let record1 = burn_actor(program, &cid_a, &n1.hash());
    let burn = |w: &mut World, label: &str, at: Actor, cid: Hash, note: &Note, code: Option<u8>| -> TxReport {
        let actors = w.public_burn_actors(at.account_id, &cid, note);
        let r = w.send(label, at, &BridgeMessage::burn(cid, note), actors);
        expect(&r, code);
        r
    };
    reports.push(burn(&mut w, "Burn 1 ETH (A) by the payer", holder, cid_a, &n1, None));
    assert_eq!(w.balance(payer, &cid_a), DENOMINATIONS[2] - DENOMINATIONS[1], "9 ETH left");
    assert_eq!(
        borsh::from_slice::<BridgeState>(&w.chain.actor_state(record1)).unwrap(),
        BridgeState::Burn(BurnRecord { config_id: cid_a, note: n1 }),
        "the note, written once at burn(cid, H(note))"
    );
    reports.push(burn(&mut w, "Burn: the same note again", holder, cid_a, &n1, Some(code::ALREADY_WRITTEN)));
    let (odd, _) = note(&keys, &cid_a, &vault_a, 1, ETH, DENOMINATIONS[1] / 2);
    reports.push(burn(&mut w, "Burn 0.5 ETH (not a denomination)", holder, cid_a, &odd, Some(code::NOT_REGISTERED)));
    let (other_token, _) = note(&keys, &cid_a, &vault_a, 2, [0xee; 20], DENOMINATIONS[1]);
    reports.push(burn(
        &mut w,
        "Burn 1 unit of another token (no holding of it)",
        holder,
        cid_a,
        &other_token,
        Some(code::INSUFFICIENT_BALANCE),
    ));
    let (too_much, _) = note(&keys, &cid_a, &vault_a, 3, ETH, DENOMINATIONS[2]);
    reports.push(burn(&mut w, "Burn 10 ETH with 9 held", holder, cid_a, &too_much, Some(code::INSUFFICIENT_BALANCE)));
    let (n2, _) = note(&keys, &cid_a, &vault_a, 4, ETH, DENOMINATIONS[1]);
    let vector_holder = own_actor(program, w.vector_dest);
    reports.push(burn(
        &mut w,
        "Burn at the vector's destination, not signed by it",
        vector_holder,
        cid_a,
        &n2,
        Some(code::NOT_AUTHORIZED),
    ));
    reports.push(burn(
        &mut w,
        "Burn under a configuration the payer holds nothing of",
        holder,
        cid_a2,
        &n2,
        Some(code::INSUFFICIENT_BALANCE),
    ));
    let record2 = burn_actor(program, &cid_a, &n2.hash());
    // A root call at a PDA that does not exist yet needs admission evidence (LEZ refuses it
    // otherwise, before the program runs).
    let evidence = vec![PublicAccountEvidence::Pda { program, seed: burn_seed(&cid_a, &n2.hash()) }];
    let r = w.submit(
        "Burn sent to the burn account itself",
        record2,
        &BridgeMessage::burn(cid_a, &n2),
        vec![record2],
        evidence,
    );
    expect(&r, Some(code::WRONG_SENDER));
    reports.push(r);
    // The token's account admits a burn only from a holder's actor, after its debit.
    let token_a = token_actor(program, &cid_a, &ETH);
    let r = w.send(
        "Burn sent to the token account itself (a top-level call)",
        token_a,
        &BridgeMessage::burn(cid_a, &n2),
        vec![token_a, config_actor(program, &cid_a), record2],
    );
    expect(&r, Some(code::WRONG_SENDER));
    reports.push(r);
    // D10: a nested call. The payer signs a transaction whose root is LEZ's `scripted` test program,
    // which calls the payer's holder actor: `is_authorized` holds (the payer signed), the
    // checkpoint could not see the call, and the bridge refuses it.
    let seed_pda = PdaSeed::new([7; 32]);
    let attacker =
        Actor::new(AccountId::for_public_pda(&harness::scripted_account(), &seed_pda), harness::scripted_account());
    let script = Script::default().send_sync(holder, &BridgeMessage::burn(cid_a, &n2));
    let r = w.chain.submit(
        "Burn as a nested call from another program, in a transaction the holder signed",
        attacker,
        borsh::to_vec(&script).unwrap(),
        vec![attacker, holder, record2],
        vec![PublicAccountEvidence::Pda { program: harness::scripted_account(), seed: seed_pda }],
        w.time,
    );
    expect(&r, Some(code::NOT_AUTHORIZED));
    reports.push(r);
    let (n3, _) = note(&keys, &cid_b, &VAULT_B, 0, ETH, DENOMINATIONS[1]);
    reports.push(burn(&mut w, "Burn 1 ETH (B) by the payer", holder, cid_b, &n3, None));
    assert_eq!(w.balance(payer, &cid_b), 0);
    let echo = w.send("Echo (rule (A)'s nonce)", holder, &BridgeMessage::Echo { nonce: [0xec; 32] }, vec![holder]);
    expect(&echo, None);
    reports.push(echo);
    let sunset = w.send(
        "Sunset by the payer (not the freeze authority)",
        holder,
        &BridgeMessage::Sunset { config_id: cid_a },
        vec![holder, config_actor(program, &cid_a)],
    );
    expect(&sunset, Some(code::NOT_AUTHORIZED));
    reports.push(sunset);
    assert_eq!(w.balance(payer, &cid_a), DENOMINATIONS[2] - DENOMINATIONS[1], "refusals burned nothing");
    assert_eq!(w.balance(w.vector_dest, &cid_a), DENOMINATIONS[1]);

    // --- A private burn: the holder of the private claim burns its 1 ETH --------------------
    let (np, _) = note(&keys, &cid_a, &vault_a, 5, ETH, DENOMINATIONS[1]);
    let message = BridgeMessage::burn(cid_a, &np);
    let pre = claimed.data.actor_state(program).clone();
    let executed = private_burn_transition(&w, account.id(), pre, &message);
    assert_eq!(executed.exit_code, 0);
    let executed_table = render_executed(&[executed]);
    let holder_p = PrivateAccount { keys: private_keys(seed), account: claimed.clone() };
    let record_p = burn_actor(program, &cid_a, &np.hash());
    let burn_publics = w.burn_actors(me, &cid_a, &np)[1..].to_vec();
    let private_burn = w
        .chain
        .prove_and_settle_private(
            &holder_p,
            me,
            borsh::to_vec(&message).unwrap(),
            burn_publics,
            &catalog(),
            RESERVE,
            w.time,
        )
        .expect("the private burn settles");
    assert!(private_burn.charge.revert.is_none());
    let outgoing: Vec<_> = private_burn
        .tx
        .message
        .execution
        .boundary
        .iter()
        .filter_map(|step| match step {
            BoundaryStep::PrivateToPublic(d) => Some((d.envelope.to, d.envelope.from, d.envelope.message.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(outgoing.len(), 2, "the burn's delivery and the fee: {outgoing:?}");
    assert_eq!(
        (outgoing[0].0, &outgoing[0].2),
        (token_a, &borsh::to_vec(&message).unwrap()),
        "the note, in the clear, to the token's account"
    );
    assert_ne!(outgoing[0].1.account_id, account.id(), "the holder appears as an alias");
    assert_eq!(
        borsh::from_slice::<BridgeState>(&w.chain.actor_state(record_p)).unwrap(),
        BridgeState::Burn(BurnRecord { config_id: cid_a, note: np })
    );
    let emptied = HolderState::default();
    private_post_state(&w.chain, &private_keys(seed), &claimed, RESERVE, &emptied).expect("the holding is gone");

    // --- The wallet side: D12's mac covers memo, D13's NOut ignores the burn index -----------
    assert_eq!(
        n1.mac,
        burn_mac(&keys.view, &n1.r, &n1.memo, &n1.token, &n1.amount, &n1.c_out),
        "the core's MAC is D12's"
    );
    let leaf_at = |i: u64| burn_leaf(&cid_a, i, &n1.token, &n1.amount, &n1.c_out, &n1.r);
    let v1 = |i: u64| {
        ferrata_bridge_core::sha256(&[&b"ferrata/nf-out/v1"[..], &s_out1, &burn_id(&cid_a, i, &leaf_at(i))].concat())
    };
    assert_ne!(v1(0), v1(1), "SPEC-v2's NOut changed with the index a burn is collected at");
    let nout = nf_out_v2(&s_out1);
    assert_eq!(nout, ferrata_bridge_core::sha256(&[&b"ferrata/nf-out/v2"[..], &s_out1].concat()));
    assert_eq!(nout, burn_nullifier(&s_out1), "the release guest's NOut (the bridge core's) is D13's");

    // --- What the checkpoint program collects ------------------------------------------------
    let tx = |label: &str| -> Vec<u8> {
        w.txs.iter().find(|(l, _)| l == label).unwrap_or_else(|| panic!("no transaction {label}")).1.clone()
    };
    let rows: Vec<(&str, Vec<u8>, bool)> = vec![
        ("ImportRoot A n=8", tx("ImportRoot A n=8"), false),
        ("Claim: leaf 6 (10 ETH to the payer)", tx("Claim: leaf 6 (10 ETH to the payer)"), false),
        ("Burn 1 ETH (A) by the payer", tx("Burn 1 ETH (A) by the payer"), false),
        ("Burn: the same note again", tx("Burn: the same note again"), true),
        ("Burn 1 ETH (B) by the payer", tx("Burn 1 ETH (B) by the payer"), false),
        ("Echo (rule (A)'s nonce)", tx("Echo (rule (A)'s nonce)"), false),
        (
            "private claim (leaf 5)",
            borsh::to_vec(&LeeTransaction::PrivacyPreserving(private_claim.tx.clone())).unwrap(),
            false,
        ),
        ("private burn", borsh::to_vec(&LeeTransaction::PrivacyPreserving(private_burn.tx.clone())).unwrap(), false),
    ];
    let txs: Vec<&[u8]> = rows.iter().map(|(_, b, _)| b.as_slice()).collect();
    let reverted: Vec<u64> = rows.iter().enumerate().filter(|(_, r)| r.2).map(|(i, _)| i as u64).collect();
    let bytes = block(&txs, &reverted);
    let mut visitor = Calls { bridge: *program.value(), calls: Vec::new() };
    let summary = parse_block(&bytes, &mut visitor).expect("the checkpoint parses the block");
    assert_eq!(summary.tx_count as usize, rows.len());
    let mut table = String::from(
        "| transaction | origin | message bytes | checkpoint's decoder | collected |\n|---|---|---:|---|---|\n",
    );
    let mut burns = Vec::new();
    let mut imports = Vec::new();
    let mut undecodable = Vec::new();
    for (index, origin, message) in &visitor.calls {
        let label = rows[*index as usize].0;
        let decoded = CpMessage::decode(message);
        let public = origin.is_public_execution();
        let collected = public && !summary.is_reverted(*index) && matches!(decoded, Ok(Some(_)));
        let what = match &decoded {
            Ok(Some(CpMessage::Burn(b))) => {
                if collected {
                    burns.push(*b);
                }
                "Burn".to_string()
            }
            Ok(Some(CpMessage::ImportRoot(m))) => {
                if collected {
                    imports.push((*m, message.to_vec()));
                }
                "ImportRoot (its proofs skipped)".to_string()
            }
            Ok(Some(CpMessage::Echo { .. })) => "Echo".to_string(),
            Ok(Some(CpMessage::Sunset { .. })) => "Sunset".to_string(),
            Ok(None) => "ignored (variant 4 and up)".to_string(),
            Err(e) => {
                if public {
                    undecodable.push(label);
                }
                format!("refused: {e:?}")
            }
        };
        let status = if !public {
            "no (not public execution)"
        } else if summary.is_reverted(*index) {
            "no (reverted)"
        } else if collected {
            "yes"
        } else {
            "no"
        };
        table.push_str(&format!("| {label} | {origin:?} | {} | {what} | {status} |\n", message.len()));
    }
    println!("{table}");
    // The burns it collects are exactly the three that happened, in order, with their notes.
    let expected = [(cid_a, n1), (cid_b, n3), (cid_a, np)];
    assert_eq!(burns.len(), 3, "{burns:?}");
    for (b, (cid, note)) in burns.iter().zip(expected) {
        assert_eq!(
            (b.config_id, b.r, b.memo, b.mac, b.token, b.amount, b.c_out),
            (cid, note.r, note.memo, note.mac, note.token, note.amount, note.c_out)
        );
    }
    // Its burn leaf is the bridge core's.
    assert_eq!(
        checkpoint_core::tree::burn_leaf(&cid_a, 0, &n1.token, &n1.amount, &n1.c_out, &n1.r),
        burn_leaf(&cid_a, 0, &n1.token, &n1.amount, &n1.c_out, &n1.r)
    );
    // The checkpoint derives the same configId from its pinned inputs, so these burns are the
    // ones it selects; and the program's Burn is byte for byte the checkpoint's variant 0.
    let a = &w.config_a;
    let pinned = checkpoint_core::state::Config {
        bedrock_genesis_id: a.bedrock_genesis_id,
        channel_id: a.channel_id,
        bridge_header: *program.value(),
        bridge_image_id: a.bridge_image_id,
        evm_chain_id: chain_id_u256(CHAIN_ID),
        vault: a.vault,
        attestation_root_id: a.attestation_root_id(),
        salt: a.salt(),
        n_min: 1,
        t_min: 1,
    };
    assert_eq!(pinned.config_id(), cid_a, "the checkpoint's configId is the program's");
    let encoded = borsh::to_vec(&BridgeMessage::burn(cid_a, &n1)).unwrap();
    assert_eq!(encoded.len(), checkpoint_core::wire::BURN_LEN);
    assert_eq!(CpMessage::Burn(burns[0]).encode(), encoded, "the checkpoint re-encodes the same bytes");
    // The program's ImportRoot carries its proofs after the checkpoint's four fields; the
    // checkpoint walks them strictly and counts the import (BRIDGE-RESULTS.md §5, mismatch 1, fixed
    // on the checkpoint side). From the same bytes both decoders read the same four fields.
    assert!(undecodable.is_empty(), "nothing public is undecodable: {undecodable:?}");
    assert_eq!(imports.len(), 1, "exactly the one import is collected");
    let (m, message) = &imports[0];
    match borsh::from_slice::<BridgeMessage>(message).expect("the program decodes its own message") {
        BridgeMessage::ImportRoot { config_id, record_slot, root, leaf_count, proof } => {
            assert_eq!((m.config_id, m.record_slot, m.root, m.leaf_count), (config_id, record_slot, root, leaf_count));
            assert_eq!((config_id, leaf_count), (cid_a, 8));
            assert!(
                !proof.account.is_empty() && !proof.root_slot.is_empty() && !proof.count_slot.is_empty(),
                "the import carries the three proofs"
            );
            assert!(CpMessage::decode(&message[..81]).is_err(), "the four fields alone are refused");
        }
        other => panic!("not an import: {other:?}"),
    }

    let table_reports = render(&reports);
    fs::write(
        results_path("bridge-burn.md"),
        format!(
            "# Burns (`harness/tests/bridge_burn.rs`); the private burn in RISC Zero dev mode\n\n\
             Bridge program image `{}`.\n\n## Public burns and refusals\n\n{table_reports}\n\
             ## Handler executions\n\n{}\n\
             ## The private burn\n\n{executed_table}\n\
             LEZ charged: execution gas {}, storage gas {}, base fee {}; transaction {} bytes (dev mode).\n\n\
             The private claim that made the holding: execution gas {}, storage gas {}.\n\n\
             ## What the checkpoint program collects (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)\n\n\
             One LEZ block holding the transactions below (the second burn marked reverted, as settlement \
             recorded it):\n\n{table}\n\
             NOut v2 of the first note: `{}`.\n",
            harness::image_id_hex(harness::bridge::BRIDGE_ID),
            render_transitions(&reports),
            harness::grouped(private_burn.charge.gas_exec),
            harness::grouped(private_burn.charge.gas_stor),
            harness::grouped(private_burn.charge.fee_base as u64),
            harness::grouped(private_burn.tx_bytes as u64),
            harness::grouped(private_claim.charge.gas_exec),
            harness::grouped(private_claim.charge.gas_stor),
            hex::encode(nout),
        ),
    )
    .unwrap();
}
