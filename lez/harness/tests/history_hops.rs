//! Chains of EIP-2935 hops through the attestation program on real Sepolia and mainnet data
//! (`harness::hops`; LEZ v0.4.0-rc1's state machine in process, dev mode): one transaction attests a
//! fact of a block several windows of 8,191 blocks before its record, through the deployed program
//! (unchanged: its request has always carried a list of hops), with the cost per hop and the
//! refusals. With the feature `program-class`, the program's second build (`guests/program-class`),
//! whose reply carries each statement's nullifier-safety class. Writes `results/history-hops-*.md`.

use ferrata_core::{
    attest::{HISTORY_CODE_HASH, HISTORY_STORAGE_ADDRESS},
    types::BeaconBlockHeader,
    witness::{ExecutionSummary, HeaderRecord},
    Error, Root,
};
use ferrata_lez_wire::{
    attest::{evaluate, Attestation, Fact, Hop, LogQuery, Request},
    ATTESTED_SELECTOR,
};
use ferrata_spectests::execkit::{account_rlp, enc, header_rlp, keccak, word, Trie};
use harness::{
    attest::{account_query, account_statement_id, log_statement_id, storage_statement_id, u256, Stage4},
    grouped,
    hops::{HopFixture, WINDOW},
    render,
    sepolia::SEPOLIA_CHAIN_ID,
    vault::{self, RealVault},
    Chain, TxReport,
};
use lee::Actor;
use serde_json::Value;

const MAX_GAS_EXEC: u64 = 10_000_000;

/// One attestation as a transaction's root. The program's verdict must be the native evaluation's:
/// the same event, or the same exit code. A request the native evaluation accepts may still not
/// fit one transaction's gas: the report then shows the revert at the gas limit.
fn attempt(
    chain: &mut Chain,
    native: (&Root, u64, &HeaderRecord),
    actor: Actor,
    label: &str,
    request: &Request,
    time: u64,
) -> (TxReport, Result<Attestation, u8>) {
    let expected = evaluate(native.0, native.1, native.2, request);
    let report = vault::attest_root(chain, label, actor, request, time);
    match &expected {
        Ok(a) if report.ok() => {
            let event = report.events.iter().find(|e| e.event.selector == ATTESTED_SELECTOR).expect("an event");
            assert_eq!(event.event.data, a.event_data(), "{label}: the event names the expected identifiers");
        }
        Ok(_) => assert!(
            report.exit_code.is_none() && report.charge.gas_exec >= MAX_GAS_EXEC,
            "{label}: accepted natively, on chain {:?} (exit {:?})",
            report.charge.revert,
            report.exit_code
        ),
        Err(code) => {
            assert_eq!(report.exit_code, Some(u32::from(*code)), "{label}: {:?}", report.charge.revert);
            assert!(report.events.is_empty(), "{label}: a refusal emits nothing");
        }
    }
    (report, expected)
}

/// The cost per hop: `k` full-window hops and one fact of the same kind at the block reached (the
/// history contract's account), for each `k`; returns the reports and a table.
#[allow(clippy::too_many_arguments)]
fn series(
    chain: &mut Chain,
    native: (&Root, u64, &HeaderRecord),
    actor: Actor,
    time: u64,
    fx: &HopFixture,
    base: &Request,
    path: &[u64],
    ks: &[usize],
) -> (Vec<TxReport>, String) {
    let mut reports = Vec::new();
    let mut table = String::from(
        "| hops | block reached | blocks back | execution gas | (gas − gas at 0 hops) / hops | storage gas (tx bytes) | result |\n\
         |---:|---:|---:|---:|---:|---:|---|\n",
    );
    let mut zero = None;
    for &k in ks {
        let end = if k == 0 { fx.record } else { path[k - 1] };
        let request = Request { hops: fx.hops(&path[..k]), accounts: vec![fx.history_account(end)], ..base.clone() };
        let label = format!("{k} hops to block {end}, the history contract's account there");
        let (report, expected) = attempt(chain, native, actor, &label, &request, time);
        let a = expected.unwrap_or_else(|code| panic!("{label}: refused natively ({code})"));
        assert_eq!((a.block_number, a.hops, a.block_hash), (end, k as u32, fx.block_hash(end)), "{label}");
        assert_eq!(a.statements[0].id, account_statement_id(native.1, &fx.history(end)), "{label}: the identifier");
        let per_hop = match (zero, k) {
            (Some(z), k) if k > 0 && report.ok() => grouped((report.charge.gas_exec - z) / k as u64),
            _ => "".into(),
        };
        if k == 0 {
            zero = Some(report.charge.gas_exec);
        }
        let result = match &report.charge.revert {
            None => "applied".to_string(),
            Some(revert) => format!("does not fit: {revert}"),
        };
        table.push_str(&format!(
            "| {k} | {end} | {} | {} | {per_hop} | {} | {result} |\n",
            grouped(fx.record - end),
            grouped(report.charge.gas_exec),
            grouped(report.charge.gas_stor)
        ));
        reports.push(report);
    }
    (reports, table)
}

/// Block `n`'s header with one byte of its logs bloom flipped: it still decodes, with the same
/// number, but hashes to something else.
fn tampered(header: &[u8]) -> Vec<u8> {
    let mut h = header.to_vec();
    // list prefix (3), parent hash, uncles hash (33 each), miner (21), three roots (33 each).
    assert_eq!(&h[189..192], &[0xb9, 0x01, 0x00], "the logs bloom's prefix");
    h[300] ^= 1;
    h
}

#[test]
fn hop_chains_on_sepolia() {
    let mut v = RealVault::load();
    let fx = HopFixture::sepolia();
    let r = fx.record;
    assert_eq!(r, v.block, "the chains start at the block of the vault fixture's Gloas record");
    let (id, record, actor, time, base) = (v.w.inst.id, v.record.clone(), v.actor(), v.time, v.base());
    let native = (&id, SEPOLIA_CHAIN_ID, &record);
    let mut out = format!(
        "# Chains of EIP-2935 hops through the attestation program (Sepolia, real data)\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test history_hops` (LEZ v0.4.0-rc1's state \
         machine in process, dev mode; the deployed program `ferrata_lez`). The record: the Sepolia instance's own, \
         written from the vault fixture's Gloas finality update (finalized slot {}, block {r}); the hops: \
         `crates/ferrata-spectests/fixtures/{}` (real data fetched on 18 November 2026, every request in its SOURCE.md). \
         Each hop reaches at most {WINDOW} blocks back from the block it starts at.\n\n",
        v.slot,
        harness::hops::SEPOLIA_HOPS
    );

    // 1. The cost per hop, up to 32 full-window hops in one transaction.
    let far = fx.chain(1);
    assert_eq!((far.len(), far[0]), (32, r - WINDOW));
    let (reports, table) = series(&mut v.w.chain, native, actor, time, &fx, &base, &far, &[0, 1, 2, 4, 8, 16, 32]);
    out.push_str("## The cost per hop\n\nEach hop to the block 8,191 back (the window's inclusive edge), and the same fact at the block reached: the history contract's account (its identifier checked against a derivation from `eth_getProof`'s fields).\n\n");
    out.push_str(&table);
    out.push('\n');
    out.push_str(&render(&reports));
    let mut accepted = reports;

    // 2. Deposit 0 of FerrataVault v5 through two hops: the same identifier as through one.
    let chain_a = fx.chain(0);
    assert_eq!(chain_a, [v.deposits[1].block, v.deposits[0].block]);
    let d = v.deposits[0].clone();
    let log = |proof| LogQuery { tx_index: d.tx_index, log_index: d.log_index, proof, conditions: vec![] };
    let one = Request { hops: vec![v.hop(d.block)], logs: vec![log(v.receipt_proof(d.block, d.tx_index))], ..base.clone() };
    let two = Request { hops: fx.hops(&chain_a), ..one.clone() };
    let (r1, a1) = attempt(&mut v.w.chain, native, actor, "deposit 0's Deposit log, one hop (as EVIDENCE row 72)", &one, time);
    let (r2, a2) = attempt(&mut v.w.chain, native, actor, "deposit 0's Deposit log, two hops (through deposit 1's block)", &two, time);
    let (a1, a2) = (a1.unwrap(), a2.unwrap());
    assert!(r1.ok() && r2.ok());
    assert_eq!((a1.hops, a2.hops, a1.block_number, a2.block_number), (1, 2, d.block, d.block));
    assert_eq!(a1.statements[0].id, v.deposit_id(0), "one hop: the identifier from the JSON-RPC fields");
    assert_eq!(a2.statements, a1.statements, "two hops: the same statement, the same identifier");
    out.push_str(&format!(
        "\n## One event, two paths\n\nDeposit 0's `Deposit` log (block {}, transaction {}, position {} in its receipt) through one hop and through two: identifier `0x{}` both times, equal to a derivation from the JSON-RPC fields alone. A hop path does not enter an identifier, so it cannot mint a second nullifier for one event.\n\n",
        d.block,
        d.tx_index,
        d.log_index,
        hex::encode(a1.statements[0].id)
    ));
    accepted.push(r1);
    accepted.push(r2);

    // 3. Facts four windows back: USDC's total supply (account and slot) and an ERC-20 Transfer.
    let (b, proof, slot, value) = fx.storage(0);
    assert_eq!(b, far[3]);
    let (tx, i, json) = fx.transfer(b);
    let request = Request {
        hops: fx.hops(&far[..4]),
        accounts: vec![account_query(&proof, vec![], vec![(slot, vec![])])],
        logs: vec![LogQuery { tx_index: tx, log_index: i, proof: fx.receipt_proof(b, tx), conditions: vec![] }],
        ..base.clone()
    };
    let label = "USDC's account, its totalSupply_ slot and an ERC-20 Transfer, four hops back";
    let (report, a) = attempt(&mut v.w.chain, native, actor, label, &request, time);
    let a = a.unwrap();
    assert!(report.ok());
    assert_eq!((a.block_number, a.hops), (b, 4));
    assert_eq!(a.statements[0].id, account_statement_id(SEPOLIA_CHAIN_ID, &proof));
    assert_eq!(a.statements[1].id, storage_statement_id(SEPOLIA_CHAIN_ID, &proof.address, &slot, &value));
    assert!(matches!(a.statements[1].fact, Fact::Storage { value: held, .. } if held == value), "the value eth_call gave");
    assert_eq!(a.statements[2].id, log_statement_id(SEPOLIA_CHAIN_ID, b, tx, i, &json));
    out.push_str(&format!(
        "## Facts {} blocks back\n\nBlock {b} (four hops): USDC (`0x{}`) and its slot 11 (`totalSupply_` = {}, equal to `totalSupply()` by `eth_call` at that block), and the ERC-20 `Transfer` at transaction {tx}, position {i}: every identifier equal to a derivation from the JSON-RPC fields.\n\n",
        grouped(r - b),
        hex::encode(proof.address),
        u128::from_be_bytes(value[16..].try_into().unwrap())
    ));
    accepted.push(report);
    out.push_str(&render(&accepted[accepted.len() - 3..]));

    // 4. Refusals, each with the native evaluation's code.
    let edge = fx.hop(r, far[0]);
    let mut refused = Vec::new();
    let account = vec![fx.history_account(far[0])];
    let cases: Vec<(&str, Vec<Hop>, u8)> = vec![
        (
            "a hop one block past the window (block R − 8,192)",
            vec![Hop { header: fx.header(r - WINDOW - 1), ..edge.clone() }],
            Error::HistoryOutOfWindow.code() as u8,
        ),
        (
            "a hop to the record's own block",
            vec![Hop { header: fx.header(r), ..edge.clone() }],
            Error::HistoryOutOfWindow.code() as u8,
        ),
        (
            "a hop above the record (block R + 1)",
            vec![Hop { header: fx.header(r + 1), ..edge.clone() }],
            Error::HistoryOutOfWindow.code() as u8,
        ),
        (
            "a broken link: the second hop's proofs taken in the record's state, not in the first hop's block",
            vec![fx.hop(r, chain_a[0]), v.hop(chain_a[1])],
            Error::ProofNodeMismatch.code() as u8,
        ),
        (
            "a tampered link: the second hop's header with one byte of its logs bloom flipped",
            vec![fx.hop(r, chain_a[0]), Hop { header: tampered(&fx.header(chain_a[1])), ..fx.hop(chain_a[0], chain_a[1]) }],
            Error::HistoryHashMismatch.code() as u8,
        ),
        (
            "a link skipped: hops 1 and 3 of the 32-hop chain",
            vec![fx.hop(r, far[0]), fx.hop(far[1], far[2])],
            Error::HistoryOutOfWindow.code() as u8,
        ),
    ];
    for (label, hops, code) in cases {
        let request = Request { hops, accounts: account.clone(), ..base.clone() };
        let (report, expected) = attempt(&mut v.w.chain, native, actor, label, &request, time);
        assert_eq!(expected.err(), Some(code), "{label}");
        refused.push(report);
    }

    // The history contract's code hash, on a synthetic state under a record placed as the program
    // writes one: the right code (accepted), other code, no contract (refused).
    let target = chain_a[0];
    let key = u256(u128::from(target % WINDOW));
    let storage = Trie::new([(keccak(&key).to_vec(), enc::uint(&fx.block_hash(target)))]);
    let served: Value = serde_json::from_slice(&fx.file(&format!("block_{r}.json"))).unwrap();
    let instance: Root = [0x48; 32];
    for (n, (label, code_hash, present)) in [
        ("synthetic state: the history contract with its specified code (control)", HISTORY_CODE_HASH, true),
        ("synthetic state: the history contract's address with other code", keccak(b"other code"), true),
        ("synthetic state: no account at the history contract's address", [0; 32], false),
    ]
    .into_iter()
    .enumerate()
    {
        let mut accounts: Vec<([u8; 20], Vec<u8>)> =
            (0x40..0x50u8).map(|i| ([i; 20], account_rlp(u64::from(i), 1_000, keccak(&[0x80]), keccak(b"")))).collect();
        if present {
            accounts.push((HISTORY_STORAGE_ADDRESS, account_rlp(1, 0, storage.root(), code_hash)));
        }
        let state = Trie::new(accounts.iter().map(|(a, rlp)| (keccak(a).to_vec(), rlp.clone())));
        let mut block = served["result"].clone();
        block["stateRoot"] = Value::String(format!("0x{}", hex::encode(state.root())));
        let header = header_rlp(&block);
        let placed = HeaderRecord {
            beacon: BeaconBlockHeader { slot: 11_556_608 + 1 + n as u64, ..Default::default() },
            execution_root: [0; 32],
            execution: ExecutionSummary::BlockHash(keccak(&header)),
        };
        let at = vault::place_record(&mut v.w.chain, instance, SEPOLIA_CHAIN_ID, &placed);
        let request = Request {
            instance_id: instance,
            chain_id: SEPOLIA_CHAIN_ID,
            header: Some(header),
            hops: vec![Hop {
                account_proof: state.proof(&keccak(&HISTORY_STORAGE_ADDRESS)),
                storage_proof: storage.proof(&keccak(&key)),
                header: fx.header(target),
            }],
            accounts: vec![fx.history_account(target)],
            ..Request::default()
        };
        let (report, expected) =
            attempt(&mut v.w.chain, (&instance, SEPOLIA_CHAIN_ID, &placed), at, label, &request, time);
        if n == 0 {
            let a = expected.expect("the control is accepted");
            assert_eq!((a.block_number, a.block_hash), (target, fx.block_hash(target)));
            accepted.push(report);
        } else {
            assert_eq!(expected.err(), Some(Error::HistoryContractMissing.code() as u8), "{label}");
            refused.push(report);
        }
    }
    out.push_str("\n## Refused\n\nEach with the native evaluation's code; the last two on a synthetic state (a real header with its state root replaced) under a record placed at `header(id, slot)` as the program writes one, the control beside them accepted.\n\n");
    out.push_str(&render(&refused));
    out.push_str("\n\nThe control:\n\n");
    out.push_str(&render(&accepted[accepted.len() - 1..]));
    println!("{out}");
    for r in &accepted {
        assert!(r.charge.gas_exec <= MAX_GAS_EXEC, "{} is over MAX_GAS_EXEC", r.label);
    }
    std::fs::write(harness::results_dir().join("history-hops-sepolia.md"), out).unwrap();
}

#[test]
fn hop_chains_on_mainnet() {
    let mut s = Stage4::new();
    let fx = HopFixture::mainnet();
    let r = fx.record;
    assert_eq!(r, s.fx.block, "the chain starts at the block of the attestation fixture's record");
    let slot = s.fx.finalized_slot;
    let state = s.record(slot);
    let record = state.header().unwrap();
    let (id, chain_id, actor, time, base) = (state.instance_id, state.chain_id, s.header(slot), s.time, s.request());
    let native = (&id, chain_id, &record);
    let path = fx.chain(0);
    assert_eq!(path.len(), 4);
    let mut out = format!(
        "# Chains of EIP-2935 hops through the attestation program (mainnet, real data)\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test history_hops` (LEZ v0.4.0-rc1's state \
         machine in process, dev mode; the deployed program `ferrata_lez`). The record: the mainnet instance's own, \
         written from the attestation fixture's finality update (finalized slot {slot}, block {r}); the hops: \
         `crates/ferrata-spectests/fixtures/{}` (real data fetched on 18 November 2026, every request in its SOURCE.md): \
         first to block {} (as the attestation fixture's one hop), then three full windows.\n\n## The cost per hop\n\n",
        harness::hops::MAINNET_HOPS,
        path[0]
    );
    let (mut accepted, table) = series(&mut s.w.chain, native, actor, time, &fx, &base, &path, &[0, 1, 2, 4]);
    out.push_str(&table);
    out.push('\n');

    // WETH's balance of the fixture's holder and an ERC-20 Transfer, four hops back.
    let (b, proof, wslot, value) = fx.storage(0);
    assert_eq!((b, wslot), (path[3], s.fx.weth_slot));
    let (tx, i, json) = fx.transfer(b);
    let request = Request {
        hops: fx.hops(&path),
        accounts: vec![account_query(&proof, vec![], vec![(wslot, vec![])])],
        logs: vec![LogQuery { tx_index: tx, log_index: i, proof: fx.receipt_proof(b, tx), conditions: vec![] }],
        ..base.clone()
    };
    let label = "WETH's account, the holder's balance slot and an ERC-20 Transfer, four hops back";
    let (report, a) = attempt(&mut s.w.chain, native, actor, label, &request, time);
    let a = a.unwrap();
    assert!(report.ok());
    assert_eq!((a.block_number, a.hops), (b, 4));
    assert_eq!(a.statements[0].id, account_statement_id(chain_id, &proof));
    assert_eq!(a.statements[1].id, storage_statement_id(chain_id, &proof.address, &wslot, &value));
    let call: Value = serde_json::from_slice(&fx.file(&format!("call_0x{}_balanceOf_{b}.json", hex::encode(proof.address)))).unwrap();
    assert_eq!(word(call["result"].as_str().unwrap()), value, "the slot equals balanceOf by eth_call");
    assert_eq!(a.statements[2].id, log_statement_id(chain_id, b, tx, i, &json));
    accepted.push(report);
    out.push_str(&render(&accepted));
    out.push_str(&format!(
        "\nBlock {b} ({} blocks back): WETH's slot of the attestation fixture's holder equals `balanceOf` by `eth_call` at that block; every identifier equals a derivation from the JSON-RPC fields.\n\n## Refused\n\n",
        grouped(r - b)
    ));

    let first = fx.hop(r, path[0]);
    let history = fx.history(path[1]);
    let other = history.storage.iter().find(|x| x.key == u256(u128::from(path[2] % WINDOW))).unwrap();
    let account = vec![fx.history_account(path[0])];
    let mut refused = Vec::new();
    for (label, hops, code) in [
        ("a hop one block past the window (block R − 8,192)", vec![Hop { header: fx.header(r - WINDOW - 1), ..first.clone() }], Error::HistoryOutOfWindow),
        ("a hop above the record (block R + 1)", vec![Hop { header: fx.header(r + 1), ..first.clone() }], Error::HistoryOutOfWindow),
        (
            "a broken link: the second hop's proofs taken in the state of the block after it",
            vec![
                first.clone(),
                Hop { account_proof: history.proof.clone(), storage_proof: other.proof.clone(), header: fx.header(path[1]) },
            ],
            Error::ProofNodeMismatch,
        ),
    ] {
        let request = Request { hops, accounts: account.clone(), ..base.clone() };
        let (report, expected) = attempt(&mut s.w.chain, native, actor, label, &request, time);
        assert_eq!(expected.err(), Some(code.code() as u8), "{label}");
        refused.push(report);
    }
    out.push_str(&render(&refused));
    println!("{out}");
    for r in &accepted {
        assert!(r.ok() && r.charge.gas_exec <= MAX_GAS_EXEC, "{}", r.label);
    }
    std::fs::write(harness::results_dir().join("history-hops-mainnet.md"), out).unwrap();
}

/// The second build (`guests/program-class` at `harness::CLASSED`): the same verification and
/// identifiers, and each statement's class in the reply and in the event.
#[cfg(feature = "program-class")]
mod classed {
    use super::*;
    use ferrata_lez_wire::{consumer::ConsumerMessage, header_actor, RecordState};
    use ferrata_lez_wire_class::{
        attest::{evaluate as classed_evaluate, Attestation as Classed, StatementClass},
        ATTESTED_SELECTOR as CLASSED_SELECTOR,
    };
    use harness::CLASSED;
    use lee::Account;
    use lee_core::account::ActorState;

    /// `Attest` at the second build as a transaction's root: the event is the classed one.
    fn root(chain: &mut Chain, native: (&Root, u64, &HeaderRecord), actor: Actor, label: &str, request: &Request, time: u64) -> (TxReport, Classed) {
        let expected = classed_evaluate(native.0, native.1, native.2, request).unwrap_or_else(|c| panic!("{label}: {c}"));
        let report = vault::attest_root(chain, label, actor, request, time);
        assert!(report.ok(), "{label}: {:?} (exit {:?})", report.charge.revert, report.exit_code);
        let event = report.events.iter().find(|e| e.event.selector == CLASSED_SELECTOR).expect("a classed event");
        assert_eq!(event.account_id, CLASSED);
        assert_eq!(event.event.data, expected.event_data(), "{label}: classes and identifiers");
        (report, expected)
    }

    #[test]
    fn the_second_build_replies_with_each_statements_class() {
        // The mainnet instance on the second build follows the fixture's finality update itself.
        let mut s = Stage4::at(CLASSED);
        let mut deployed = Stage4::new();
        let fx = HopFixture::mainnet();
        let slot = s.fx.finalized_slot;
        let state = s.record(slot);
        let record = state.header().unwrap();
        assert_eq!(state.instance_id, deployed.w.inst.id, "the instance id does not name the program");
        let path = fx.chain(0);
        let (b, proof, wslot, _) = fx.storage(0);
        let (tx, i, _) = fx.transfer(b);
        let request = Request {
            hops: fx.hops(&path),
            accounts: vec![account_query(&proof, vec![], vec![(wslot, vec![])])],
            logs: vec![LogQuery { tx_index: tx, log_index: i, proof: fx.receipt_proof(b, tx), conditions: vec![] }],
            ..s.request()
        };
        let mut out = String::from(
            "# The attestation program's second build: each statement's nullifier-safety class\n\n\
             Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --features program-class --test history_hops` \
             (LEZ v0.4.0-rc1's state machine in process, dev mode). `ferrata_lez_class` (`guests/program-class`: the \
             deployed program's own `src/main.rs` against `wire-class`) deployed at `CLASSED`; its mainnet instance wrote \
             its own record from the attestation fixture's finality update. The deployed program beside it, on the same \
             request.\n\n",
        );
        // As a transaction's root.
        let (at, time) = (s.header(slot), s.time);
        let (r1, classed) = root(&mut s.w.chain, (&state.instance_id, state.chain_id, &record), at, "second build: WETH account, balance slot, Transfer, four hops back (root)", &request, time);
        let classes: Vec<StatementClass> = classed.statements.iter().map(|x| x.class).collect();
        assert_eq!(classes, [StatementClass::Value, StatementClass::Value, StatementClass::Event]);
        let plain = deployed.accept("deployed program: the same request (root)", slot, &request).1;
        assert_eq!(classed.unclassified(), plain, "the same identifiers, facts, block and hops");
        // Called by the reference consumer, built for the deployed program's reply: the second
        // build hands it the classed reply (one byte per statement more), which it cannot decode and
        // refuses (222, BAD_REPLY), so a consumer pinned to the deployed format never misreads it.
        // The deployed program through the same consumer: accepted, the plain reply held.
        let (reader, evidence) = s.reader();
        let message = ConsumerMessage::CallAttest { ferrata: CLASSED, slot, request: request.clone() };
        let r2 = s.w.chain.submit(
            "second build called by the reference consumer (deployed format): refused",
            reader,
            borsh::to_vec(&message).unwrap(),
            vec![reader, at],
            evidence,
            time,
        );
        assert_eq!(r2.exit_code, Some(222), "{:?}", r2.charge.revert);
        let handed = r2.transitions.last().expect("the reply's transition");
        assert_eq!(handed.from.map(|a| a.account_id), Some(at.account_id), "the reply comes from the second build");
        assert_eq!(handed.message_bytes, borsh::to_vec(&classed).unwrap().len(), "the classed reply was handed over");
        let (r2b, reply) = deployed.call("deployed program called by the reference consumer: accepted", slot, &request);
        assert!(r2b.ok(), "{:?}", r2b.charge.revert);
        assert_eq!(reply.as_ref(), Some(&plain), "the consumer holds the deployed reply");
        let plain_len = r2b.transitions.last().unwrap().message_bytes;
        assert_eq!(plain_len, borsh::to_vec(&plain).unwrap().len());
        assert_eq!(handed.message_bytes, plain_len + classed.statements.len());
        out.push_str(&format!(
            "Called by the reference consumer: the second build's reply, {} bytes, is the deployed program's {} plus one byte per statement; the consumer, built for the deployed format, refuses it (222, `BAD_REPLY`) and holds the deployed program's reply.\n\n",
            handed.message_bytes, plain_len
        ));
        // A refusal keeps its code.
        let above = Request { hops: vec![Hop { header: fx.header(fx.record + 1), ..fx.hop(fx.record, path[0]) }], ..request.clone() };
        let r3 = s.attest("second build: a hop above the record", slot, &above);
        assert_eq!(r3.exit_code, Some(u32::from(Error::HistoryOutOfWindow.code())));
        out.push_str(&format!(
            "Classes of the reply: {classes:?}; identifiers equal to the deployed program's reply ({}).\n\n",
            classed.statements.iter().map(|x| format!("`{}…`", &hex::encode(x.id)[..8])).collect::<Vec<_>>().join(", ")
        ));
        let mut reports = vec![r1, r2, r2b, r3];

        // An accumulator leaf (Leaf) and the vault's slots (Value): the synthetic vault under a
        // record placed at the second build's header(id, slot).
        let syn = vault::synthetic();
        let actor = header_actor(CLASSED, &vault::SYNTHETIC_INSTANCE, syn.record.beacon.slot);
        let st = RecordState { instance_id: vault::SYNTHETIC_INSTANCE, chain_id: SEPOLIA_CHAIN_ID, record: syn.record.encode(), root_id: vault::SYNTHETIC_INSTANCE };
        let account = Account::default().with_actor_state(actor.program_account_id, ActorState::from(borsh::to_vec(&st).unwrap()));
        s.w.chain.state.force_insert_account(actor.account_id, account);
        let base = Request { instance_id: vault::SYNTHETIC_INSTANCE, chain_id: SEPOLIA_CHAIN_ID, header: Some(syn.header.clone()), ..Request::default() };
        let leaf = (0u64, syn.leaves[0], vault::tree(&syn.leaves, 0).1);
        let request = vault::vault_request(base, &syn.proof, &syn.root, &[leaf]);
        let (r4, leafy) = root(&mut s.w.chain, (&vault::SYNTHETIC_INSTANCE, SEPOLIA_CHAIN_ID, &syn.record), actor, "second build: the synthetic vault's account, slots 0 and 1, leaf 0", &request, time);
        let classes: Vec<StatementClass> = leafy.statements.iter().map(|x| x.class).collect();
        assert_eq!(classes, [StatementClass::Value, StatementClass::Value, StatementClass::Value, StatementClass::Leaf]);
        assert_eq!(classes.iter().filter(|c| c.is_nullifiable()).count(), 1);
        out.push_str(&format!("The synthetic vault: classes {classes:?}.\n\n"));
        reports.push(r4);
        out.push_str(&render(&reports));
        println!("{out}");
        std::fs::write(harness::results_dir().join("history-hops-class.md"), out).unwrap();
    }
}
