//! RFP-021 P.7 and P.9, RFP-022 P.7: what accumulating state costs, measured in LEZ v0.4.0-rc1's
//! own state (`lee::V03State`). A LEZ node keeps that state as borsh-serialized breakpoints
//! (`lez/storage`, `BreakpointCellOwned(V03State)`), so the serialized size is what a node stores
//! for it (before RocksDB's compression); each account is a key (32 bytes) and a borsh `Account`.
//!
//! 1. Sizes: the attestation program's instance, record and job accounts; the bridge's nullifier,
//!    holder, burn and root accounts; each as `borsh(Account)` and as what it adds to the
//!    serialized state when it is created by a real transaction.
//! 2. Growth: the serialized state with K more record (or nullifier) accounts, K up to 100,000.
//! 3. Cost against accumulated state (P.9): the same statements and the same updates, charged with
//!    0 to 100,000 records held by the instance.
//!
//! `perf_state actions` measures instead every other action against accumulated state (P.9):
//! `Init`, `VerifyAll` in a public and in a private transaction, `Succeed`, the eight statement
//! types of `stage4-attest.md`, the accumulator-leaf statement, `Read` and `ReadPadded`, and the
//! reference consumer's messages, each with 0 to 100,000 more of the accounts that accumulate
//! beside it (and, for a private transaction, more commitments in LEZ's commitment set), then the
//! reference consumer's private-kind claim against its claimant's holdings, which grows
//! (`harness::accumulated`); `bench/perf/state-actions.sh` writes
//! `lez/results/perf-state-actions.md`.
//!
//! Records added for K are copies of a real record at slots no request names (the program reads a
//! record by key only, so a copy is as heavy as a real one for the state, and invisible to the
//! statements); the records written by real updates are measured in step 1 and by
//! `lez/updater/examples/perf_replay.rs`. Prints markdown; `bench/perf/state.sh` writes
//! `lez/results/perf-state.md`. Dev mode is not needed (public transactions only), except by the
//! `actions` mode's private update path (`RISC0_DEV_MODE=1`: a fake receipt; LEZ charges a private
//! transaction the same whatever its proof).

use std::time::Instant;

use ferrata_bridge_core::{
    chain_id_u256,
    outbound::{burn_commitment, burn_secret, burn_tag, xor_memo},
    BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{burn_actor, burn_mac_v2, own_actor, root_actor, BridgeMessage, Note};
use ferrata_lez_wire::{
    attest::{Cmp, Field, Request},
    header_actor,
};
use harness::{
    accumulated::{self as acc, Charged},
    as_finality_update,
    attest::{account_query, condition, u256, Stage4},
    bridge::{self, nullifier_of, CHAIN_ID, DENOMINATIONS, ETH},
    grouped,
    scenarios::World,
    TxReport,
};
use lee::{AccountId, V03State};
use lee_core::account::Account;

const KS: [usize; 4] = [0, 1_000, 10_000, 100_000];
/// Slots of the synthetic records: far above any real slot, so no request reads one.
const SYNTHETIC_SLOT: u64 = 1 << 40;

fn state_bytes(state: &V03State) -> (usize, f64) {
    let start = Instant::now();
    let n = borsh::to_vec(state).expect("borsh").len();
    (n, start.elapsed().as_secs_f64())
}

fn account(state: &V03State, id: AccountId) -> Account {
    state.get_account_by_id_ref(id).cloned().unwrap_or_else(|| panic!("no account {id}"))
}

fn account_bytes(state: &V03State, id: AccountId) -> usize {
    borsh::to_vec(&account(state, id)).unwrap().len()
}

/// The actor-state bytes of `id` for `program` (what the program wrote).
fn actor_bytes(state: &V03State, id: AccountId, program: AccountId) -> usize {
    account(state, id).data.actor_state(program).len()
}

/// Insert copies of `template` until `have` reaches `want`, at the ids `id_of(i)`.
fn grow(state: &mut V03State, template: &Account, have: &mut usize, want: usize, id_of: impl Fn(usize) -> AccountId) {
    while *have < want {
        state.force_insert_account(id_of(*have), template.clone());
        *have += 1;
    }
}

fn row(r: &TxReport) -> String {
    format!("{} | {} | {}", grouped(r.charge.gas_exec), grouped(r.charge.gas_stor), r.transitions.len())
}

/// The statement requests of `stage4-attest.md` measured against accumulated records.
fn statements(s: &Stage4) -> Vec<(&'static str, Request)> {
    let weth = s.fx.weth_proof();
    let t = s.fx.transfer.clone();
    let big = s.fx.in_largest.clone();
    let older = s.fx.weth_proof_at_hop();
    vec![
        (
            "account (WETH: code hash)",
            Request {
                accounts: vec![account_query(&weth, vec![condition(Field::CodeHash, Cmp::Eq, weth.code_hash)], vec![])],
                ..s.request()
            },
        ),
        (
            "storage slot, value >= 1",
            Request {
                accounts: vec![account_query(
                    &weth,
                    vec![],
                    vec![(s.fx.weth_slot, vec![condition(Field::Value, Cmp::Ge, u256(1))])],
                )],
                ..s.request()
            },
        ),
        ("event log (receipt of 7 logs)", s.log_request(t.tx_index, t.log_index, vec![])),
        ("event log (largest receipt)", s.log_request(big.tx_index, big.log_index, vec![])),
        (
            "one EIP-2935 hop, then a slot",
            Request {
                hops: vec![s.fx.hop()],
                accounts: vec![account_query(&older, vec![], vec![(s.fx.weth_slot, vec![])])],
                ..s.request()
            },
        ),
    ]
}

fn sizes_and_statements() {
    let (mut s, _) = harness::attest::accepted();
    let program = s.w.inst.program;
    let id = s.w.inst.id;
    let slot = s.fx.finalized_slot;
    let record_id = header_actor(program, &id, slot).account_id;
    let instance_id = s.w.inst.actor().account_id;
    let record = account(&s.w.chain.state, record_id);

    println!("## 1. Attestation program: account sizes\n");
    println!("| account | actor state (B) | borsh `Account` (B) | with its 32-byte key (B) |\n|---|---:|---:|---:|");
    for (what, acct) in [("instance(id)", instance_id), ("header(id, slot) record", record_id)] {
        let a = actor_bytes(&s.w.chain.state, acct, program);
        let b = account_bytes(&s.w.chain.state, acct);
        println!("| {what} | {a} | {b} | {} |", b + 32);
    }
    // The job: its actor state is the Open step's post-state (the job is cleared at Finish).
    let open = s.w.chain.reports.iter().find(|r| r.label.starts_with("finality update Open")).expect("the Open step");
    let job_state = open.transitions.iter().filter_map(|t| t.post_state_bytes).max().expect("Open writes the job");
    let job = Account::default().with_actor_state(program, vec![0; job_state].into());
    let job_b = borsh::to_vec(&job).unwrap().len();
    println!("| job(id, job_id), while open (built from Open's post-state) | {job_state} | {job_b} | {} |", job_b + 32);

    println!("\n## 2. Attestation program: the serialized state against records held, and statements charged\n");
    let reqs = statements(&s);
    print!("| records added | serialized state (B) | serialization (s) |");
    for (what, _) in &reqs {
        print!(" {what}: exec gas | stor gas | transitions |");
    }
    println!();
    print!("|---:|---:|---:|");
    for _ in &reqs {
        print!("---:|---:|---:|");
    }
    println!();
    let mut have = 0;
    let mut first: Vec<(u64, u64)> = Vec::new();
    for k in KS {
        grow(&mut s.w.chain.state, &record, &mut have, k, |i| {
            header_actor(program, &id, SYNTHETIC_SLOT + i as u64).account_id
        });
        let (bytes, secs) = state_bytes(&s.w.chain.state);
        print!("| {} | {} | {secs:.3} |", grouped(k as u64), grouped(bytes as u64));
        for (i, (what, req)) in reqs.iter().enumerate() {
            let (r, _) = s.accept(what, slot, req);
            assert!(r.ok(), "{what}: accepted");
            if k == 0 {
                first.push((r.charge.gas_exec, r.charge.gas_stor));
            }
            assert_eq!((r.charge.gas_exec, r.charge.gas_stor), first[i], "{what}: the charge does not move with K");
            print!(" {} |", row(&r));
        }
        println!();
    }
    println!("\nEvery statement was charged the same execution and storage gas at every K (asserted).");
}

fn updates_against_records() {
    println!(
        "\n## 3. Updates against records held (mainnet update_0, a handoff; then update_1 as a finality update)\n"
    );
    println!("| records added | serialized state before (B) | step | exec gas | stor gas |\n|---:|---:|---|---:|---:|");
    let mut base: Vec<(String, u64, u64)> = Vec::new();
    for k in KS {
        let mut w = World::new(2);
        let (program, id) = (w.inst.program, w.inst.id);
        let slot = w.bootstrap.header.beacon.slot;
        let record = account(&w.chain.state, header_actor(program, &id, slot).account_id);
        let mut have = 0;
        grow(&mut w.chain.state, &record, &mut have, k, |i| {
            header_actor(program, &id, SYNTHETIC_SLOT + i as u64).account_id
        });
        let (bytes, _) = state_bytes(&w.chain.state);
        let u0 = w.updates[0].clone();
        let mut reports = w.apply(&u0, "update_0").reports;
        let u1 = as_finality_update(&w.updates[1]);
        reports.extend(w.apply(&u1, "update_1 as finality update").reports);
        for (i, r) in reports.iter().enumerate() {
            assert!(r.ok(), "{}: applied", r.label);
            if k == 0 {
                base.push((r.label.clone(), r.charge.gas_exec, r.charge.gas_stor));
            }
            assert_eq!(
                (r.label.as_str(), r.charge.gas_exec, r.charge.gas_stor),
                (base[i].0.as_str(), base[i].1, base[i].2),
                "the charge does not move with K"
            );
            println!(
                "| {} | {} | {} | {} | {} |",
                grouped(k as u64),
                grouped(bytes as u64),
                r.label,
                grouped(r.charge.gas_exec),
                grouped(r.charge.gas_stor)
            );
        }
    }
    println!("\nEvery step was charged the same execution and storage gas at every K (asserted).");
}

/// A burn note, as `tests/bridge_burn.rs` builds one.
fn note(keys: &BridgeKeys, cid: &Hash, vault: &[u8; 20], i: u64, token: [u8; 20], amount: u128) -> Note {
    let recipient = [0xe1; 20];
    let r = burn_tag(&keys.view, cid, i);
    let s_out = burn_secret(&keys.auth, cid, &r);
    let c_out = burn_commitment(&s_out, &chain_id_u256(CHAIN_ID), vault, &recipient);
    let memo = xor_memo(&keys.view, &r, &recipient);
    let amount = ferrata_lez_bridge_wire::word_of_u128(amount);
    let mac = burn_mac_v2(&keys.view, &r, &memo, &token, &amount, &c_out);
    Note { r, memo, mac, token, amount, c_out }
}

fn bridge_state() {
    let mut w = bridge::World::with_roots();
    // RFP-022 P.2: each root import of the scenario, the deposits it newly covers and its cost per deposit.
    println!("\n## 4a. Bridge program: root imports and the deposits each covers (RFP-022 P.2)\n");
    println!("| import | leaves it covers | newly covered | exec gas | storage gas | exec gas per newly covered deposit |\n|---|---:|---:|---:|---:|---:|");
    let mut covered = std::collections::BTreeMap::<[u8; 32], u64>::new();
    for r in w.chain.reports.iter().filter(|r| r.label.starts_with("ImportRoot")) {
        assert!(r.ok(), "{}: applied", r.label);
        let n: u64 =
            r.label.split("n=").nth(1).and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse().ok()).unwrap();
        let cid = if r.label.contains(" A ") { w.cid_a } else { w.cid_b };
        let before = covered.get(&cid).copied().unwrap_or(0);
        let new = n.saturating_sub(before);
        covered.insert(cid, before.max(n));
        let per = r.charge.gas_exec.checked_div(new).map_or_else(|| "— (none new)".to_string(), grouped);
        println!(
            "| {} | {n} | {new} | {} | {} | {per} |",
            r.label,
            grouped(r.charge.gas_exec),
            grouped(r.charge.gas_stor)
        );
    }
    println!("\n## 4. Bridge program: accounts created by real transactions\n");
    let program = w.program;
    let (cid_a, vault_a) = (w.cid_a, w.vault_a.address);
    println!(
        "| transaction | new account | actor state (B) | borsh `Account` (B) | serialized state grew by (B) | exec gas |\n|---|---|---:|---:|---:|---:|"
    );
    let mut last = state_bytes(&w.chain.state).0;
    let mut nullifiers = Vec::new();
    let claims = [
        ("public claim: leaf 6, 10 ETH to the payer", w.payer_deposit.clone()),
        ("public claim: the vector's deposit, 1 ETH", w.vector.clone()),
    ];
    for (label, deposit) in claims {
        let witness = w.vault_a.witness(&deposit, 8);
        let r = w.claim(label, &cid_a, 8, &vault_a, witness);
        assert!(r.ok(), "{label}: {:?}", r.charge.revert);
        let n = deposit.nullifier(CHAIN_ID, &vault_a);
        let nid = nullifier_of(program, &cid_a, CHAIN_ID, &vault_a, &n).account_id;
        nullifiers.push(nid);
        let now = state_bytes(&w.chain.state).0;
        println!(
            "| {label} | nullifier (+ the destination's holder on its first credit) | {} | {} | {} | {} |",
            actor_bytes(&w.chain.state, nid, program),
            account_bytes(&w.chain.state, nid),
            grouped((now - last) as u64),
            grouped(r.charge.gas_exec)
        );
        last = now;
    }
    // A public burn of 1 ETH by the payer (holding 10 ETH from the first claim).
    let keys = BridgeKeys::from_bridge_seed(&[0x77; 32]);
    let payer = w.chain.payer;
    for i in 0..2u64 {
        let n1 = note(&keys, &cid_a, &vault_a, i, ETH, DENOMINATIONS[1]);
        let holder = own_actor(program, payer);
        let actors = w.public_burn_actors(payer, &cid_a, &n1);
        let label = format!("public burn {} of 1 ETH by the payer", i + 1);
        let r = w.send(&label, holder, &BridgeMessage::burn(cid_a, &n1), actors);
        assert!(r.ok(), "{label}: {:?}", r.charge.revert);
        let bid = burn_actor(program, &cid_a, &n1.r).account_id;
        let now = state_bytes(&w.chain.state).0;
        println!(
            "| {label} | burn record | {} | {} | {} | {} |",
            actor_bytes(&w.chain.state, bid, program),
            account_bytes(&w.chain.state, bid),
            grouped(now.abs_diff(last) as u64),
            grouped(r.charge.gas_exec)
        );
        last = now;
    }
    let rid = root_actor(program, &cid_a, 8).account_id;
    println!(
        "| (an `ImportRoot` of `with_roots`) | root(cid, 8) | {} | {} | — | — |",
        actor_bytes(&w.chain.state, rid, program),
        account_bytes(&w.chain.state, rid)
    );

    println!("\n## 5. Bridge program: the serialized state against nullifier and burn records held\n");
    println!("| records added | nullifier copies: serialized state (B) | per record (B) | burn copies: serialized state (B) | per record (B) |\n|---:|---:|---:|---:|---:|");
    let nullifier = account(&w.chain.state, nullifiers[0]);
    let burn = {
        let n1 = note(&keys, &cid_a, &vault_a, 0, ETH, DENOMINATIONS[1]);
        account(&w.chain.state, burn_actor(program, &cid_a, &n1.r).account_id)
    };
    let base = state_bytes(&w.chain.state).0;
    let (mut sn, mut sb) = (w.chain.state.clone(), w.chain.state.clone());
    let (mut hn, mut hb) = (0, 0);
    for k in KS.into_iter().skip(1) {
        grow(&mut sn, &nullifier, &mut hn, k, |i| synthetic_id(0xa1, i));
        grow(&mut sb, &burn, &mut hb, k, |i| synthetic_id(0xb2, i));
        let (bn, bb) = (state_bytes(&sn).0, state_bytes(&sb).0);
        println!(
            "| {} | {} | {:.2} | {} | {:.2} |",
            grouped(k as u64),
            grouped(bn as u64),
            (bn - base) as f64 / k as f64,
            grouped(bb as u64),
            (bb - base) as f64 / k as f64
        );
    }
}

/// A distinct account id per `i` (the copies only need distinct keys).
fn synthetic_id(tag: u8, i: usize) -> AccountId {
    let mut b = [tag; 32];
    b[..8].copy_from_slice(&(i as u64).to_le_bytes());
    AccountId::new(b)
}

/// One scenario of `actions`: `run` at every K of [`KS`] on the state `prepare(k)` returns, every charge
/// asserted equal to K = 0's before its row is printed.
fn sweep<S>(
    what: &str,
    prepare: impl Fn(usize) -> S,
    state: impl Fn(&S) -> &V03State,
    run: impl Fn(&mut S) -> Vec<Charged>,
) {
    println!("\n## {what}\n");
    println!("| K | serialized state before (B) | transaction | handlers (cycles each) | exec gas | stor gas |\n|---:|---:|---|---|---:|---:|");
    let mut base: Vec<Charged> = Vec::new();
    for k in KS {
        let mut s = prepare(k);
        let (bytes, _) = state_bytes(state(&s));
        let charged = run(&mut s);
        if k == 0 {
            base = charged.clone();
        }
        acc::assert_flat(&base, &charged, k);
        for c in &charged {
            println!(
                "| {} | {} | {} | {} | {} | {} |",
                grouped(k as u64),
                grouped(bytes as u64),
                c.action,
                c.handlers_text(),
                grouped(c.gas_exec),
                grouped(c.gas_stor)
            );
        }
    }
}

/// One private transaction of `actions`: [`sweep`] with K more commitments in LEZ's commitment
/// set as well (the caller's `prepare` adds them), the fee payer's membership path printed beside
/// the charge.
fn sweep_private<S>(
    what: &str,
    prepare: impl Fn(usize) -> S,
    state: impl Fn(&S) -> &V03State,
    run: impl Fn(&mut S) -> acc::Proven,
) {
    println!("\n## {what}\n");
    println!(
        "| K | serialized state before (B) | membership path (levels) | transaction | handlers (cycles each) | exec gas | stor gas |\n|---:|---:|---:|---|---|---:|---:|"
    );
    let mut base: Vec<Charged> = Vec::new();
    for k in KS {
        let mut s = prepare(k);
        let (bytes, _) = state_bytes(state(&s));
        // Markers for `bench/perf/state-actions.sh`, which reads the circuit's cycles from r0vm's
        // session log (`RISC0_INFO=1 RUST_LOG=risc0_zkvm=info`): the session right after the last
        // proof of one of the private transitions named at the end marker. r0vm (a subprocess)
        // writes its log to the stdout it inherits, so the markers go there too, in order; the
        // script separates both from the record.
        println!("perf-state-actions: begin|{what}|{k}");
        let proven = run(&mut s);
        let private: Vec<String> =
            proven.charged.handlers.iter().filter(|(h, _)| h != "public part").map(|(_, c)| c.to_string()).collect();
        println!("perf-state-actions: end|{what}|{k}|{}|{}", proven.path_levels, private.join(","));
        let charged = vec![proven.charged];
        if k == 0 {
            base = charged.clone();
        }
        acc::assert_flat(&base, &charged, k);
        let c = &charged[0];
        println!(
            "| {} | {} | {} | {} | {} | {} | {} |",
            grouped(k as u64),
            grouped(bytes as u64),
            proven.path_levels,
            c.action,
            c.handlers_text(),
            grouped(c.gas_exec),
            grouped(c.gas_stor)
        );
    }
}

/// The pools a claimant already holds privately, for [`holdings_growth`].
const POOLS: [usize; 5] = [0, 1, 10, 100, 1_000];

/// The reference consumer's cost that grows with use, measured as it grows: a private-kind vault
/// claim's private transitions against the claimant's `holdings` (not a comparison; nothing
/// asserted equal).
fn holdings_growth(vw: &acc::VaultWorld) {
    println!(
        "\n## The reference consumer's private-kind claim against the claimant's own holdings (grows; avoidable)\n"
    );
    println!(
        "The reference consumer credits a private-kind deposit in the claimant's own private state, one `(pool id, \
         amount)` entry per pool it holds (`ClaimantState.holdings`); the claim decodes, searches and re-encodes the \
         list. Deposit 1 of FerrataVault v5 claimed by its destination (LEZ's private test account 0) in RISC Zero's \
         executor, the claimant already holding `pools` other pools: the claim's two private transitions (LEZ's \
         circuit, which hashes and encrypts the larger state, grows with it too; not measured here). It is the \
         claimant's own proving work: LEZ charges the private transaction its flat receipt gas plus the public part \
         (`Read`, `Spend`), which does not see the list. It is avoidable: a claimant that sends each pool's deposits \
         to a private account of its own keeps one entry per account."
    );
    println!(
        "\n| pools already held | ClaimDepositPrivately (executor) | the record's reply (executor) | private cycles | over 0 pools, per pool |\n|---:|---:|---:|---:|---:|"
    );
    let (a0, b0) = acc::private_kind_claim(vw, 0);
    for pools in POOLS {
        let (a, b) = acc::private_kind_claim(vw, pools);
        let per = if pools == 0 {
            "—".to_string()
        } else {
            format!("{:.1}", ((a + b) as f64 - (a0 + b0) as f64) / pools as f64)
        };
        println!("| {} | {} | {} | {} | {per} |", grouped(pools as u64), grouped(a), grouped(b), grouped(a + b));
    }
}

fn other_actions() {
    assert_eq!(
        std::env::var("RISC0_DEV_MODE").as_deref(),
        Ok("1"),
        "run with RISC0_DEV_MODE=1 (the private transactions' fake receipts; bench/perf/state-actions.sh sets it)"
    );
    println!(
        "Each action runs on copies of one world; the copy for K also holds K more of the accounts that accumulate \
         beside it, copied from accounts the programs wrote, at keys no measured transaction names \
         (`lez/harness/src/accumulated.rs`): for `Init`, records of another instance and other instances; for \
         `VerifyAll` and `Succeed`, records of the instance (slots from 2^40) and other instances; for the statements \
         and the reference consumer, records of the instance and the consumer's spent claims, balances (same pool, \
         other owners) and readers; for the synthetic vault's leaf statements, records of its instance; for the vault \
         pattern, records of the Sepolia instance and the consumer's spent claims and holders (same pool, other \
         destinations). Job accounts are not copied. Every private transaction also has K more commitments in LEZ's \
         commitment set, so its fee payer's membership path, which LEZ's circuit hashes one SHA-256 per level, is \
         longer (printed; the circuit's own cycles, from r0vm's session log, are in the last table); its \
         first \"handlers\" are its private \
         transitions' cycles in RISC Zero's executor (what the sender proves, not charged by LEZ), its last the public \
         part LEZ executes, charged with the flat private receipt gas. The accounts an action reads that change with \
         use are measured as they change: a reader empty, then holding the record or reply it read before; a balance \
         or a holder credited K + 1 times before; and, in the table after the sweeps, a claimant's private holdings. \
         Every charge is asserted equal at every K before it is printed; the two tables after the sweeps are not \
         comparisons."
    );
    let uw = acc::UpdateWorld::new();
    sweep("`Init` and its adoption (a new chain)", |k| uw.chain_for_init(k), |c| &c.state, acc::init_on);
    sweep(
        "`VerifyAll` in one public transaction (update_1 as a finality update, after update_0)",
        |k| uw.grown(k),
        |w| &w.chain.state,
        |w| vec![acc::verify_all_public(w)],
    );
    sweep_private(
        "`VerifyAll` in one private transaction: the proven update path",
        |k| {
            let mut w = uw.grown(k);
            acc::add_commitments(&mut w.chain, k);
            w
        },
        |w| &w.chain.state,
        acc::verify_all_private,
    );
    sweep("`Succeed` and the `Spawn` it sends", |k| uw.grown(k), |w| &w.chain.state, |w| vec![acc::succeed(w)]);
    drop(uw);
    let lw = acc::LeafWorld::new();
    sweep(
        "The accumulator-leaf statement and the vault's slots (the synthetic vault of `stage5-vault-synthetic.md`)",
        |k| lw.grown(k),
        |c| &c.state,
        |c| acc::attest_leaves(&lw, c),
    );
    drop(lw);
    let cw = acc::ConsumerWorld::new();
    let requests = acc::statement_requests(&cw.base);
    sweep(
        "The eight statement types of `stage4-attest.md`, each the root of a transaction",
        |k| cw.grown(k),
        |s| &s.w.chain.state,
        |s| acc::attest_all(s, &requests),
    );
    sweep(
        "`Read` and `ReadPadded` (the module's read idiom), sent by a public reader of the consumer",
        |k| cw.grown(k),
        |s| &s.w.chain.state,
        acc::read_record,
    );
    sweep("`CallAttest` from one public reader, three times", |k| cw.grown(k), |s| &s.w.chain.state, acc::call_attest);
    sweep(
        "The reference consumer's claim (`Claim` → `Attest` → reply → `Credit`)",
        |k| cw.grown(k),
        |s| &s.w.chain.state,
        |s| vec![acc::claim(s, "Claim, the sender's first credit", 0)],
    );
    sweep(
        "The same claim, the sender's balance credited K + 1 times before",
        |k| {
            let mut s = cw.grown(k);
            acc::place_balance(&mut s, k as u64 + 1);
            (s, k as u64 + 1)
        },
        |(s, _)| &s.w.chain.state,
        |(s, before)| vec![acc::claim(s, "Claim, the sender credited before", *before)],
    );
    sweep_private(
        "The reference consumer's private read, `AttestPrivately` (one private transaction)",
        |k| {
            let mut s = cw.grown(k);
            acc::add_commitments(&mut s.w.chain, k);
            s
        },
        |s| &s.w.chain.state,
        acc::attest_privately,
    );
    drop(cw);
    let vw = acc::VaultWorld::new();
    sweep(
        "The vault pattern's public claim (`ClaimDeposit` → `Attest` → reply → `CreditHolder`), FerrataVault v5's deposit 0",
        |k| vw.grown(k),
        |w| &w.chain.state,
        |w| vec![acc::claim_deposit(&vw, w, "ClaimDeposit, the destination's first credit", 0)],
    );
    sweep(
        "The same claim, the destination's holder credited K + 1 times before",
        |k| {
            let mut w = vw.grown(k);
            acc::place_holder(&vw, &mut w, k as u64 + 1);
            (w, k as u64 + 1)
        },
        |(w, _)| &w.chain.state,
        |(w, before)| vec![acc::claim_deposit(&vw, w, "ClaimDeposit, the holder credited before", *before)],
    );
    sweep_private(
        "The vault pattern's private claim (`ClaimDepositPrivately`; public part `Read` → `Spend` → `CreditHolder`)",
        |k| {
            let mut w = vw.grown(k);
            acc::add_commitments(&mut w.chain, k);
            w
        },
        |w| &w.chain.state,
        |w| acc::claim_deposit_privately(&vw, w),
    );
    holdings_growth(&vw);
    println!(
        "\nEvery transaction of the swept tables was charged the same handler cycles, execution gas and storage gas at every K (asserted)."
    );
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        None => {
            println!("# perf_state: LEZ state sizes and costs against accumulated state\n");
            sizes_and_statements();
            updates_against_records();
            bridge_state();
        }
        Some("actions") => {
            println!("# perf_state actions: every other action against accumulated state\n");
            other_actions();
        }
        Some(other) => panic!("unknown mode {other}: none, or `actions`"),
    }
}
