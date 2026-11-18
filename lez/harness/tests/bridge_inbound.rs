//! The bridge program's inbound leg through LEZ's state machine (`lez/BRIDGE.md` §2): `Init` with
//! the reciprocal pairing, `ImportRoot` through the attestation program's direct path, and the
//! plaintext claim with its refusals, on the synthetic vaults of `harness::bridge` (vault A's
//! first five leaves are the shared vector's claim tree). Writes `results/bridge-inbound.md`.

use std::{fs, path::PathBuf};

use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{deposit_secret, DestKind},
    BridgeKeys,
};
use ferrata_lez_bridge_wire::{
    code, config_actor, BridgeMessage, BridgeState, ConfigState, NullifierState, CODE_BLOCK,
};
use ferrata_lez_wire::{attest::evaluate, RecordState, CODE_BLOCKS};
use harness::{
    bridge::{
        config, controls, eth_entry, eth_schedule_one, fee_recipient, registry_slots, relayer, render, Block, Deposit,
        Vault, VaultSlots, World, CHAIN_ID, DENOMINATIONS, ETH, INSTANCE, MINT_FEES_1, RELAY_FEES, VAULT_B,
    },
    render_transitions, TxReport,
};

const TARGET_GAS_EXEC: u64 = 5_000_000;

fn expect(report: &TxReport, code: Option<u8>) {
    match code {
        None => assert!(report.ok(), "{}: {:?} (exit {:?})", report.label, report.charge.revert, report.exit_code),
        Some(c) => {
            assert_eq!(report.exit_code, Some(u32::from(c)), "{}: {:?}", report.label, report.charge.revert);
            assert!(!report.ok(), "{}", report.label);
        }
    }
    assert!(report.charge.gas_exec <= TARGET_GAS_EXEC, "{} is over TARGET_GAS_EXEC", report.label);
}

fn results_path(name: &str) -> PathBuf {
    harness::results_dir().join(name)
}

#[test]
fn the_bridge_codes_are_its_own_block() {
    for (_, block) in CODE_BLOCKS.iter() {
        assert!(block.end() < CODE_BLOCK.start(), "{block:?} overlaps the bridge's {CODE_BLOCK:?}");
    }
    let mut seen = std::collections::BTreeSet::new();
    for (name, value) in code::ALL {
        assert!(CODE_BLOCK.contains(value), "{name} = {value}");
        assert!(seen.insert(*value), "{name} = {value} twice");
    }
}

#[test]
fn inbound_configuration_import_and_plaintext_claim() {
    let mut w = World::new();
    let (a, b, a2) = (w.config_a.clone(), w.config_b.clone(), w.config_a2.clone());
    let (cid_a, cid_b, cid_a2) = (w.cid_a, w.cid_b, w.cid_a2);
    let va = w.vault_a.address;
    let mut reports = Vec::new();
    let mut run = |report: TxReport, code: Option<u8>| {
        expect(&report, code);
        reports.push(report);
    };

    // --- Configuration ----------------------------------------------------------------------
    let r = w.init("Init A: vault A's slot 2 holds configId(A)", &a, 0);
    run(r, None);
    let BridgeState::Config(ConfigState::Ready { config, pending: None, sunset: false, .. }) =
        borsh::from_slice::<BridgeState>(&w.chain.actor_state(config_actor(w.program, &cid_a))).unwrap()
    else {
        panic!("config(A) is not ready")
    };
    assert_eq!(config, a);
    let r = w.init("Init B (another configuration, vault B)", &b, 0);
    run(r, None);
    let r = w.init("Init A' (vault A, another fee recipient): slot 2 is configId(A)", &a2, 0);
    run(r, Some(code::PAIRING_MISMATCH));
    assert!(w.chain.actor_state(config_actor(w.program, &cid_a2)).is_empty(), "nothing written");
    let r = w.init("Init A again", &a, 1);
    run(r, Some(code::ALREADY_WRITTEN));
    // ETH's registry entry of each vault (bridge_controls.rs has the registry's own cases).
    let r = w.register("Register ETH (A): entry and schedule 0 attested at block 100", &cid_a, 0, &eth_entry(), &va);
    run(r, None);
    let r = w.register("Register ETH (B)", &cid_b, 0, &eth_entry(), &VAULT_B);
    run(r, None);

    // --- ImportRoot -------------------------------------------------------------------------
    let root5 = w.vault_a.root(5);
    let root8 = w.vault_a.root(8);
    let r = w.import("ImportRoot A: slots 0 and 1 at block 100 (5 leaves)", &cid_a, 0, root5, 5, &va);
    run(r, None);
    let stored = w.root_state(&cid_a, 5).expect("root(A, 5)");
    assert_eq!((stored.root, stored.leaf_count, stored.block_number, stored.vault), (root5, 5, 100, va));
    assert_eq!(
        stored.root,
        ferrata_bridge_core::tree::Tree::from_leaves(w.vault_a.leaves[..5].to_vec()).unwrap().root()
    );
    let r = w.import("ImportRoot A: block 101 (8 leaves)", &cid_a, 1, root8, 8, &va);
    run(r, None);
    let r = w.import("ImportRoot A: block 101 again (same count)", &cid_a, 1, root8, 8, &va);
    run(r, Some(code::ALREADY_WRITTEN));
    let r = w.import("ImportRoot A: block 101, message claims count 7", &cid_a, 1, root8, 7, &va);
    run(r, Some(code::IMPORT_MISMATCH));
    let r = w.import("ImportRoot A: block 101, message claims the 5-leaf root", &cid_a, 1, root5, 8, &va);
    run(r, Some(code::IMPORT_MISMATCH));
    // Vault B's proofs offered for configuration A: the request names vault A, so the
    // attestation program refuses the account proof with the core's code.
    let request = ferrata_lez_bridge_wire::import_request(&a, &w.blocks[1].import_proof(&VAULT_B));
    let record = RecordState {
        instance_id: INSTANCE,
        chain_id: CHAIN_ID,
        record: w.blocks[1].record.encode(),
        root_id: INSTANCE,
    };
    let core_code = evaluate(&INSTANCE, CHAIN_ID, &record.header().unwrap(), &request).expect_err("refused natively");
    let r = w.import("ImportRoot A with vault B's proofs", &cid_a, 1, w.vault_b.root(8), 8, &VAULT_B);
    run(r, Some(core_code));
    let r = w.import("ImportRoot A': no configuration (never initialised)", &cid_a2, 1, root8, 8, &va);
    run(r, Some(code::NOT_FOUND));
    let r = w.import("ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count)", &cid_a, 2, root8, 6, &va);
    run(r, None);
    let r = w.import("ImportRoot B: block 100 (8 leaves)", &cid_b, 0, w.vault_b.root(8), 8, &VAULT_B);
    run(r, None);
    // Vault A's ETH fee schedule 1 (from leaf 6), then its range closed at 8 leaves.
    let r =
        w.import_fees("ImportFees ETH (A): schedule 1 at block 101", &cid_a, 1, ETH, 1, Some(eth_schedule_one()), &va);
    run(r, None);
    let r = w.import_fees("ImportFees ETH (A): no schedule 2 at block 101 (8 leaves)", &cid_a, 1, ETH, 2, None, &va);
    run(r, None);

    // --- Plaintext claims -------------------------------------------------------------------
    let vector = w.vector.clone();
    let witness5 = w.vault_a.witness(&vector, 5);
    let r = w.claim("Claim: the shared vector's deposit (leaf 2) under root(A, 5)", &cid_a, 5, &va, witness5.clone());
    run(r, None);
    assert_eq!(vector.nullifier(CHAIN_ID, &va), w.vector_n, "the vector's N");
    assert_eq!(
        w.nullifier(&cid_a, &va, &w.vector_n),
        Some(NullifierState { config_id: cid_a, token: ETH, leaf_count: 5, amount: DENOMINATIONS[1] })
    );
    assert_eq!(w.balance(w.vector_dest, &cid_a), DENOMINATIONS[1], "1 ETH minted to the vector's destination");
    assert_eq!(w.balance(relayer(), &cid_a), RELAY_FEES[1], "the relay fee to the relayer the claim names");
    assert_eq!(w.balance(fee_recipient(), &cid_a), 0, "leaf 2 is under schedule 0: no mint fee");
    let r = w.claim("Claim: the same again (replay)", &cid_a, 5, &va, witness5.clone());
    run(r, Some(code::ALREADY_CLAIMED));
    let witness8 = w.vault_a.witness(&vector, 8);
    let r = w.claim("Claim: the same deposit under root(A, 8)", &cid_a, 8, &va, witness8);
    run(r, Some(code::ALREADY_CLAIMED));
    assert_eq!(w.balance(w.vector_dest, &cid_a), DENOMINATIONS[1], "minted once");

    let payer = w.chain.payer;
    let deposit = w.payer_deposit.clone();
    let good = w.vault_a.witness(&deposit, 8);
    let r = w.claim("Claim: leaf 6 under root(A, 6) (block 102: index >= count)", &cid_a, 6, &va, good.clone());
    run(r, Some(code::LEAF_NOT_IMPORTED));
    let mut bad = good.clone();
    bad.path[3][0] ^= 1;
    let r = w.claim("Claim: leaf 6, one sibling changed", &cid_a, 8, &va, bad);
    run(r, Some(code::CLAIM_INVALID));
    let mut bad = good.clone();
    bad.amount = ferrata_lez_bridge_wire::word_of_u128(DENOMINATIONS[1]);
    let r = w.claim("Claim: leaf 6 as 1 ETH instead of 10 (another leaf)", &cid_a, 8, &va, bad);
    run(r, Some(code::CLAIM_INVALID));
    let mut bad = good.clone();
    bad.leaf_index = 4;
    let r = w.claim("Claim: leaf 6's preimage at index 4", &cid_a, 8, &va, bad);
    run(r, Some(code::CLAIM_INVALID));
    let mut bad = good.clone();
    bad.dest = vector.dest;
    let r = w.claim("Claim: leaf 6 redirected to another destination", &cid_a, 8, &va, bad);
    run(r, Some(code::CLAIM_INVALID));
    let r =
        w.claim("Claim: leaf 6 under root(B, 8) (another configuration's vault)", &cid_b, 8, &VAULT_B, good.clone());
    run(r, Some(code::CLAIM_INVALID));
    let odd = w.vault_a.witness(&w.odd_deposit.clone(), 8);
    let r = w.claim("Claim: leaf 7, 0.5 ETH (not a denomination)", &cid_a, 8, &va, odd);
    run(r, Some(code::NOT_REGISTERED));
    let private = w.vault_a.witness(&w.private_deposit.clone(), 8);
    let r = w.claim("Claim: leaf 5 (private kind) by the plaintext path", &cid_a, 8, &va, private);
    run(r, Some(code::WRONG_DESTINATION));
    let r = w.claim("Claim: under root(A, 3), never imported", &cid_a, 3, &va, w.vault_a.witness(&vector, 3));
    run(r, Some(code::NOT_FOUND));
    let mint = BridgeMessage::Mint {
        nullifier: [7; 32],
        config_id: cid_a,
        evm_chain_id: CHAIN_ID,
        vault: va,
        token: ETH,
        holder: *payer.value(),
        amount: DENOMINATIONS[2],
    };
    let holder = harness::bridge::holder_of(w.program, &cid_a, payer);
    let seed = harness::bridge::holder_seed_of(&cid_a, payer.value());
    let evidence = vec![lee::PublicAccountEvidence::Pda { program: w.program, seed }];
    let r = w.submit("Mint sent directly to a holder", holder, &mint, vec![holder], evidence);
    run(r, Some(code::WRONG_SENDER));
    assert_eq!(w.balance(payer, &cid_a), 0, "no refusal minted");

    let r = w.claim("Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1)", &cid_a, 8, &va, good);
    run(r, None);
    assert_eq!(w.balance(payer, &cid_a), DENOMINATIONS[2], "exactly the denomination: balances stay whole");
    assert_eq!(w.balance(fee_recipient(), &cid_a), MINT_FEES_1[2], "schedule 1's mint fee");
    assert_eq!(w.balance(relayer(), &cid_a), RELAY_FEES[1] + RELAY_FEES[2]);
    let deposit_b = w.deposit_b.clone();
    let witness_b = w.vault_b.witness(&deposit_b, 8);
    let r = w.claim("Claim: vault B's deposit under root(B, 8)", &cid_b, 8, &VAULT_B, witness_b);
    run(r, None);
    assert_eq!(w.balance(payer, &cid_b), DENOMINATIONS[1], "a second configuration's holding");
    if harness::bridge::SECOND_BUILD {
        // The second build: one holder account per configuration, each holding its own configuration's only.
        assert_eq!(w.holder(&cid_a, payer).unwrap().holdings.len(), 1);
        assert_eq!(w.holder(&cid_b, payer).unwrap().holdings.len(), 1);
    } else {
        assert_eq!(w.holder(&cid_a, payer).unwrap().holdings.len(), 2);
    }

    // Every refused transaction changed nothing: the balances above are exactly the three claims.
    let table = render(&reports);
    let transitions = render_transitions(&reports);
    println!("{table}\n{transitions}");
    fs::write(
        results_path("bridge-inbound.md"),
        format!(
            "# The bridge program's inbound leg (`harness/tests/bridge_inbound.rs`)\n\n\
             Bridge program image `{}`; every transaction settled by LEZ's own functions in a block of its own.\n\n\
             {table}\n## Handler executions\n\n{transitions}",
            harness::image_id_hex(harness::bridge::BRIDGE_ID)
        ),
    )
    .unwrap();
}

/// `Init`, `Register`, `ImportRoot` and a plaintext claim under a record in the Gloas layout, which
/// binds the block hash only (every Sepolia record since Gloas): a real Sepolia header with a
/// synthetic state root, carried in front of the account proof (`bridge-wire::with_header`), for
/// the pairing, the registry slots and the import alike. Without it the attestation program
/// refuses with `HEADER_REQUIRED` (215); with a header that does not hash to the recorded hash,
/// with the core's code. Writes `results/bridge-gloas.md`.
#[test]
fn inbound_under_a_gloas_record() {
    const VAULT_C: [u8; 20] = [0x33; 20];
    let mut w = World::new();
    let program = w.program;
    let c = config(VAULT_C, controls());
    let cid = c.id(program);
    let payer = w.chain.payer;
    let keys = BridgeKeys::from_bridge_seed(&[0x5a; 32]);
    let depositor = [0xd1; 20];
    let deposit = Deposit {
        leaf_index: 0,
        depositor,
        token: ETH,
        amount: ferrata_lez_bridge_wire::word_of_u128(DENOMINATIONS[0]),
        s: deposit_secret(&keys.auth, &chain_id_u256(CHAIN_ID), &VAULT_C, &depositor, 0),
        kind: DestKind::Public,
        dest: *payer.value(),
    };
    let vault = Vault { address: VAULT_C, leaves: vec![deposit.leaf(), [0xc1; 32]] };
    let extra = registry_slots(&eth_entry(), &[]);
    let slots = VaultSlots { address: VAULT_C, root: vault.root(2), count: 2, config_id: cid, extra };
    let i = w.add_block(Block::gloas(9_000_096, &[slots]));
    let served: serde_json::Value = serde_json::from_slice(&harness::sepolia::gloas_file("block.json")).unwrap();
    let number =
        u64::from_str_radix(served["result"]["number"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
    let record = w.blocks[i].record.clone();
    let native = |request: &ferrata_lez_wire::attest::Request| evaluate(&INSTANCE, CHAIN_ID, &record, request);
    let mut reports = Vec::new();
    let mut run = |report: TxReport, code: Option<u8>| {
        expect(&report, code);
        reports.push(report);
    };

    // Without the header: the attestation program's HEADER_REQUIRED, natively and in the guest.
    let header = w.blocks[i].header.take();
    let bare = native(&ferrata_lez_bridge_wire::pairing_request(&c, &w.blocks[i].pairing_proof(&VAULT_C)));
    assert_eq!(bare.expect_err("refused natively"), ferrata_lez_wire::code::HEADER_REQUIRED);
    let r = w.init("Init C, Gloas record, no header", &c, i);
    run(r, Some(ferrata_lez_wire::code::HEADER_REQUIRED));
    // Another block's header (its last byte changed): the core's hash mismatch code.
    let mut other = header.clone().expect("a Gloas block has a header");
    *other.last_mut().unwrap() ^= 1;
    w.blocks[i].header = Some(other);
    let wrong = native(&ferrata_lez_bridge_wire::pairing_request(&c, &w.blocks[i].pairing_proof(&VAULT_C)))
        .expect_err("refused natively");
    let r = w.init("Init C, Gloas record, another block's header", &c, i);
    run(r, Some(wrong));
    assert!(w.chain.actor_state(config_actor(program, &cid)).is_empty(), "nothing written");

    // With the header: the configuration, the registration, the import and the claim.
    w.blocks[i].header = header;
    let r = w.init("Init C, Gloas record, header in front of the account proof", &c, i);
    run(r, None);
    let header = w.blocks[i].header.take();
    let r = w.register("Register ETH (C), Gloas record, no header", &cid, i, &eth_entry(), &VAULT_C);
    run(r, Some(ferrata_lez_wire::code::HEADER_REQUIRED));
    w.blocks[i].header = header;
    let r = w.register("Register ETH (C), Gloas record, with the header", &cid, i, &eth_entry(), &VAULT_C);
    run(r, None);
    let header = w.blocks[i].header.take();
    let r = w.import("ImportRoot C, Gloas record, no header", &cid, i, vault.root(2), 2, &VAULT_C);
    run(r, Some(ferrata_lez_wire::code::HEADER_REQUIRED));
    w.blocks[i].header = header;
    let r = w.import("ImportRoot C, Gloas record, with the header (2 leaves)", &cid, i, vault.root(2), 2, &VAULT_C);
    run(r, None);
    let stored = w.root_state(&cid, 2).expect("root(C, 2)");
    assert_eq!(
        (stored.root, stored.leaf_count, stored.block_number, stored.vault),
        (vault.root(2), 2, number, VAULT_C)
    );
    let r = w.claim("Claim: leaf 0 under root(C, 2)", &cid, 2, &VAULT_C, vault.witness(&deposit, 2));
    run(r, None);
    assert_eq!(w.balance(payer, &cid), DENOMINATIONS[0], "exactly the denomination");
    assert_eq!(w.balance(relayer(), &cid), RELAY_FEES[0], "the registered relay fee");
    assert_eq!(w.balance(fee_recipient(), &cid), 0, "schedule 0: no mint fee");
    let r = w.claim("Claim: leaf 0 again (replay)", &cid, 2, &VAULT_C, vault.witness(&deposit, 2));
    run(r, Some(code::ALREADY_CLAIMED));

    let table = render(&reports);
    let transitions = render_transitions(&reports);
    println!("{table}\n{transitions}");
    fs::write(
        results_path("bridge-gloas.md"),
        format!(
            "# The bridge program under a record in the Gloas layout (`harness/tests/bridge_inbound.rs`)\n\n\
             Bridge program image `{}`; the record binds the hash of a real Sepolia header (block {number}) \
             whose state root is replaced by the synthetic state's; every transaction settled by LEZ's own \
             functions in a block of its own.\n\n{table}\n## Handler executions\n\n{transitions}",
            harness::image_id_hex(harness::bridge::BRIDGE_ID)
        ),
    )
    .unwrap();
}
