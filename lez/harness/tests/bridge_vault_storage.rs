//! The bridge program reads FerrataVault's own storage. `vault/test/RegistryStorage.t.sol` runs the
//! vault (v5, Solidity, a local EVM) through registrations, a fee change and six deposits and exports
//! every slot the vault wrote, at two moments P and Q, with what its events say
//! (`vault/test/vectors/registry-storage.json`). Here each moment becomes an Ethereum block holding
//! exactly that storage, and the bridge program attests it: `Register` of every token from the
//! vault's registry entry and schedule 0, `ImportRoot` of the vault's root and leaf count,
//! `ImportFees` of ETH's schedule 1 (and the absence of the next ones), and a claim of each deposit,
//! which mints by the schedule of its leaf index. Nothing in the vault computed a LEZ-side slot or
//! hash, and nothing here writes the vault's storage: if the two sides encoded the registry, the fee
//! schedules, the leaves or the slots differently, a transaction would be refused. Only the
//! pairing (`Init`) uses a synthetic block, since a vault built in Foundry cannot hold a configId
//! that depends on this program's image (`bridge_inbound.rs` tests the pairing). Writes
//! `results/bridge-vault-storage.md`.

use std::fs;

use ferrata_bridge_core::inbound::DestKind;
use ferrata_lez_bridge_wire::{
    code, fee_slot, registry_slot, word_of, word_of_u128, FeeSchedule, TokenEntry, Word, SLOT_COUNT, SLOT_ROOT,
};
use harness::{
    bridge::{config, controls, fee_recipient, relayer, render, Block, Deposit, Vault, VaultSlots, World, ETH},
    render_transitions, TxReport,
};
use lee::{Account, AccountId};
use serde_json::Value;

const TARGET_GAS_EXEC: u64 = 5_000_000;

fn expect(report: &TxReport, code: Option<u8>) {
    match code {
        None => assert!(
            report.ok(),
            "{}: {:?} (exit {:?})\n{}",
            report.label,
            report.charge.revert,
            report.exit_code,
            render_transitions(std::slice::from_ref(report))
        ),
        Some(c) => {
            assert_eq!(report.exit_code, Some(u32::from(c)), "{}: {:?}", report.label, report.charge.revert);
            assert!(!report.ok(), "{}", report.label);
        }
    }
    assert!(report.charge.gas_exec <= TARGET_GAS_EXEC, "{} is over TARGET_GAS_EXEC", report.label);
}

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../vault/test/vectors/registry-storage.json")).unwrap()
}

fn bytes<const N: usize>(v: &Value) -> [u8; N] {
    hex::decode(v.as_str().expect("a hex string").trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn int(v: &Value) -> u128 {
    v.as_str().map_or_else(|| u128::from(v.as_u64().expect("a number")), |s| s.parse().expect("a decimal"))
}

fn ints(v: &Value) -> Vec<u128> {
    v.as_array().unwrap().iter().map(int).collect()
}

fn small(v: &Value) -> u64 {
    v.as_u64().expect("a number")
}

/// A registry entry as the vault's `TokenRegistered` and `RegistryEntryPublished` events give it.
fn entry(t: &Value) -> TokenEntry {
    TokenEntry {
        token: bytes(&t["token"]),
        lez_definition: bytes(&t["lez_definition"]),
        decimals: small(&t["decimals"]) as u8,
        lez_unit: int(&t["lez_unit"]),
        unit_weight: int(&t["unit_weight"]),
        denominations: ints(&t["denominations"]),
        relay_fees: ints(&t["relay_fees"]),
    }
}

/// A fee schedule as the vault's `FeesAnnounced` and `FeesInForce` events give it.
fn schedule(s: &Value) -> FeeSchedule {
    FeeSchedule {
        index: small(&s["index"]),
        from_leaf: small(&s["from_leaf"]),
        announced_at: small(&s["announced_at"]),
        effective_at: small(&s["effective_at"]),
        mint_fees: ints(&s["mint_fees"]),
        burn_fees: ints(&s["burn_fees"]),
        checkpoint_fees: ints(&s["checkpoint_fees"]),
    }
}

/// A deposit as the vault's `Deposit` event gives it, with the depositor's secret and destination.
fn deposit(d: &Value) -> Deposit {
    Deposit {
        leaf_index: small(&d["leaf_index"]),
        depositor: bytes(&d["depositor"]),
        token: bytes(&d["token"]),
        amount: word_of_u128(int(&d["amount"])),
        s: bytes(&d["s"]),
        kind: DestKind::from_byte(small(&d["kind"]) as u8).expect("a kind"),
        dest: bytes(&d["dest"]),
    }
}

fn storage(snapshot: &Value) -> Vec<(Word, Word)> {
    snapshot["storage"].as_array().unwrap().iter().map(|p| (bytes(&p[0]), bytes(&p[1]))).collect()
}

fn held(slots: &[(Word, Word)], slot: &Word) -> Word {
    slots.iter().find(|(s, _)| s == slot).map_or([0; 32], |(_, v)| *v)
}

fn name(token: &[u8; 20]) -> String {
    if *token == ETH {
        "ETH".into()
    } else {
        format!("token 0x{}…", hex::encode(&token[..3]))
    }
}

#[test]
fn the_bridge_reads_the_vaults_own_registry_fees_and_tree() {
    let fx = fixture();
    let vault: [u8; 20] = bytes(&fx["vault"]);
    let tokens: Vec<TokenEntry> = fx["tokens"].as_array().unwrap().iter().map(entry).collect();
    let schedules: Vec<FeeSchedule> = fx["schedules"].as_array().unwrap().iter().map(schedule).collect();
    let schedule_tokens: Vec<[u8; 20]> =
        fx["schedules"].as_array().unwrap().iter().map(|s| bytes(&s["token"])).collect();
    let deposits: Vec<Deposit> = fx["deposits"].as_array().unwrap().iter().map(deposit).collect();
    let snapshots = fx["snapshots"].as_array().unwrap();
    let (p_slots, q_slots) = (storage(&snapshots[0]), storage(&snapshots[1]));
    let (n_p, n_q) = (small(&snapshots[0]["leaf_count"]), small(&snapshots[1]["leaf_count"]));
    let eth_one =
        schedules.iter().zip(&schedule_tokens).find(|(s, t)| s.index == 1 && **t == ETH).expect("ETH's schedule 1");
    let eth_one = eth_one.0.clone();
    assert!(
        n_p > 0 && n_q > n_p && eth_one.from_leaf >= n_p && eth_one.from_leaf < n_q,
        "the scenario of the vault test"
    );

    // The LEZ side's own slots and hashes against the vault's storage and events, natively first.
    for (t, raw) in tokens.iter().zip(fx["tokens"].as_array().unwrap()) {
        assert_eq!(held(&p_slots, &registry_slot(&t.token)), t.hash(), "{}: the entry at its slot", name(&t.token));
        assert_eq!(t.hash(), bytes::<32>(&raw["entry_hash"]), "{}: the entry the event names", name(&t.token));
        t.check().expect("the vault's registration is within the LEZ side's bounds");
    }
    for ((s, token), raw) in schedules.iter().zip(&schedule_tokens).zip(fx["schedules"].as_array().unwrap()) {
        assert_eq!(held(&q_slots, &fee_slot(token, s.index)), s.hash(token), "{} schedule {}", name(token), s.index);
        assert_eq!(s.hash(token), bytes::<32>(&raw["schedule_hash"]));
    }
    let tree = Vault { address: vault, leaves: deposits.iter().map(Deposit::leaf).collect() };
    for (n, slots) in [(n_p, &p_slots), (n_q, &q_slots)] {
        assert_eq!(held(slots, &SLOT_ROOT), tree.root(n), "the vault's root is the bridge core's tree of {n} leaves");
        assert_eq!(held(slots, &SLOT_COUNT), word_of(n), "slot 1 holds the leaf count alone");
    }

    let mut w = World::new();
    // The depositors' LEZ destinations exist (a public account must exist, or be evidenced, to be
    // admitted), as `World::new`'s own destinations do.
    for d in &deposits {
        w.chain.state.force_insert_account(AccountId::new(d.dest), Account::default());
    }
    let c = config(vault, controls());
    let cid = c.id(w.program);
    let pairing = VaultSlots { address: vault, root: [0; 32], count: 0, config_id: cid, extra: vec![] };
    let i0 = w.add_block(Block::new(9_100_000, 200, &[pairing]));
    let p = w.add_block(Block::with_storage(9_100_032, 201, vault, &p_slots));
    let q = w.add_block(Block::with_storage(9_100_064, 202, vault, &q_slots));
    let mut reports = Vec::new();
    let mut run = |report: TxReport, code: Option<u8>| {
        expect(&report, code);
        reports.push(report);
    };

    let r = w.init("Init (pairing under a synthetic block: slot 2 = configId)", &c, i0);
    run(r, None);

    // --- Registration from the vault's storage at P -------------------------------------------------
    let zero_of = |token: &[u8; 20]| {
        let k = schedules.iter().zip(&schedule_tokens).position(|(s, t)| s.index == 0 && t == token).unwrap();
        schedules[k].clone()
    };
    let eth = tokens.iter().find(|t| t.token == ETH).expect("ETH registered").clone();
    let mut wrong = eth.clone();
    wrong.lez_unit *= 10;
    let r = w.register_with(
        "Register ETH with lezUnit ×10 (not the vault's entry)",
        &cid,
        p,
        &wrong,
        &zero_of(&ETH),
        &vault,
    );
    run(r, Some(code::IMPORT_MISMATCH));
    let mut wrong_zero = zero_of(&ETH);
    wrong_zero.effective_at += 1;
    let r = w.register_with("Register ETH with schedule 0 a second later", &cid, p, &eth, &wrong_zero, &vault);
    run(r, Some(code::IMPORT_MISMATCH));
    for t in &tokens {
        let label = format!("Register {}: the vault's entry and schedule 0 at P", name(&t.token));
        let r = w.register_with(&label, &cid, p, t, &zero_of(&t.token), &vault);
        run(r, None);
    }

    // --- The vault's root at P, and the claims under it ----------------------------------------------
    let r = w.import("ImportRoot: the vault's slots 0 and 1 at P", &cid, p, tree.root(n_p), n_p, &vault);
    run(r, None);
    let fee_of = |d: &Deposit| {
        let t = tokens.iter().find(|t| t.token == d.token).unwrap();
        let k = t.index_of(u128::from_be_bytes(d.amount[16..].try_into().unwrap())).unwrap();
        let s = schedules
            .iter()
            .zip(&schedule_tokens)
            .filter(|(s, tok)| **tok == d.token && s.from_leaf <= d.leaf_index)
            .max_by_key(|(s, _)| s.index)
            .unwrap()
            .0;
        (k, t.relay_fees[k], s.mint_fees[k], t.denominations[k])
    };
    let mut expected_relay: Vec<([u8; 20], u128)> = Vec::new();
    let mut expected_mint: Vec<([u8; 20], u128)> = Vec::new();
    let add =
        |list: &mut Vec<([u8; 20], u128)>, token: [u8; 20], x: u128| match list.iter_mut().find(|(t, _)| *t == token) {
            Some((_, sum)) => *sum += x,
            None => list.push((token, x)),
        };
    let mut claim_all = |w: &mut World, run: &mut dyn FnMut(TxReport, Option<u8>), n: u64, from: u64| {
        for d in deposits.iter().filter(|d| d.leaf_index >= from && d.leaf_index < n) {
            let (_, relay, mint, denomination) = fee_of(d);
            let label = format!("Claim leaf {} ({}, under root(n = {n}))", d.leaf_index, name(&d.token));
            run(w.claim(&label, &cid, n, &vault, tree.witness(d, n)), None);
            assert_eq!(w.balance_of(AccountId::new(d.dest), &cid, &d.token), denomination, "exactly the denomination");
            add(&mut expected_relay, d.token, relay);
            add(&mut expected_mint, d.token, mint);
        }
    };
    claim_all(&mut w, &mut run, n_p, 0);

    // --- ETH's schedule 1 and the vault's root at Q --------------------------------------------------
    let mut wrong_one = eth_one.clone();
    wrong_one.mint_fees[0] += eth.lez_unit;
    let r = w.import_fees("ImportFees ETH schedule 1 with another mint fee", &cid, q, ETH, 1, Some(wrong_one), &vault);
    run(r, Some(code::IMPORT_MISMATCH));
    let r =
        w.import_fees("ImportFees ETH: the vault's schedule 1 at Q", &cid, q, ETH, 1, Some(eth_one.clone()), &vault);
    run(r, None);
    for t in &tokens {
        let next = schedules.iter().zip(&schedule_tokens).filter(|(_, tok)| **tok == t.token).count() as u64;
        let label = format!("ImportFees {}: no schedule {next} at Q", name(&t.token));
        let r = w.import_fees(&label, &cid, q, t.token, next, None, &vault);
        run(r, None);
    }
    let r = w.import("ImportRoot: the vault's slots 0 and 1 at Q", &cid, q, tree.root(n_q), n_q, &vault);
    run(r, None);
    claim_all(&mut w, &mut run, n_q, n_p);

    for (token, sum) in &expected_relay {
        assert_eq!(w.balance_of(relayer(), &cid, token), *sum, "{}: the registered relay fees", name(token));
    }
    for (token, sum) in &expected_mint {
        assert_eq!(w.balance_of(fee_recipient(), &cid, token), *sum, "{}: the mint fees by leaf index", name(token));
    }
    assert!(expected_mint.iter().any(|(t, sum)| *t == ETH && *sum > 0), "a claim under schedule 1 paid its mint fee");

    let table = render(&reports);
    let transitions = render_transitions(&reports);
    println!("{table}\n{transitions}");
    fs::write(
        harness::results_dir().join("bridge-vault-storage.md"),
        format!(
            "# The bridge program on FerrataVault's own storage (`harness/tests/bridge_vault_storage.rs`)\n\n\
             Bridge program image `{}`. The blocks P and Q hold exactly the storage FerrataVault v5 wrote in \
             `vault/test/RegistryStorage.t.sol` (`vault/test/vectors/registry-storage.json`: {} and {} slots, \
             {n_p} and {n_q} leaves); `Init` under a synthetic block (the pairing is `bridge_inbound.rs`'s \
             subject). Every transaction settled by LEZ's own functions in a block of its own.\n\n{table}\n\
             ## Handler executions\n\n{transitions}",
            harness::image_id_hex(harness::bridge::BRIDGE_ID),
            p_slots.len(),
            q_slots.len(),
        ),
    )
    .unwrap();
}
