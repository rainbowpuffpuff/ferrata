//! The privacy suite, LEZ half (proposal B3; RFP-022 Privacy #1–#7, F.9, R.5).
//!
//! Twenty fresh users make the round trip through vault P (a synthetic vault at `0xa0…a0` on chain
//! 1, the address and chain the Foundry half deploys the real vault at): each deposits 1 ETH (leaf
//! `u`), claims it privately into a private LEZ account of its own (RISC Zero dev mode), and later
//! burns it from that private holding for a fresh Ethereum recipient. Claims, burns and releases
//! are submitted in three independent random orders (the delay the client proposes). A second
//! universe makes the same users' deposits to public LEZ accounts and claims them on the plaintext
//! path: the deliberately linkable variant. The naive-nullifier variant (`N = fid`, the RFP's
//! pattern) cannot run, because the bridge program computes `N = H(s ‖ fid)` and its image is
//! frozen; it is the private claims' public data with `fid` written where `N` (and the nullifier
//! account derived from it) stood.
//!
//! For every LEZ transaction the test keeps everything public: the transaction bytes, the block
//! body (one transaction per block, as everywhere in this harness, read back with the checkpoint
//! program's own parser), the events, the state diff (every public account the transaction declares
//! and the fee accounts, before and after, and the commitment-set root), and the fields it can name.
//! It writes them, the scenario the Foundry half replays and the ground truth the adversary must not
//! see to `target/privacy/` at the repository root (`FERRATA_PRIVACY_DIR` overrides): `lez.json`,
//! `scenario.json`, `truth.json`. `scripts/privacy/adversary.py` reads them with the Foundry half's
//! `eth.jsonl`. Run with `RISC0_DEV_MODE=1`.

use std::{collections::BTreeSet, fs, path::PathBuf};

use checkpoint_core::{
    lez::{parse_block, Actor as CpActor, Origin, TxKind, Visitor},
    wire::BridgeMessage as CpMessage,
};
use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{deposit_id, deposit_secret, recover, DestKind, VaultDeposit},
    outbound::{
        burn_commitment, burn_id, burn_leaf, burn_secret, burn_tag, recover_burns, release_journal, verify_release,
        xor_memo, BurnNote, PublishedBurn, ReleasePublic, ReleaseWitness,
    },
    sha256,
    tree::Tree,
    BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{
    self as bw, burn_mac_v2, holder_actor, nullifier_actor, BridgeMessage, HolderState, Holding, Note,
};
use harness::{
    bridge::{
        catalog, eth_entry, private_account_id, private_keys, private_post_state, relayer, Deposit, Vault, World,
        CHAIN_ID, DENOMINATIONS, ETH,
    },
    private::{Freshness, PrivateAccount},
    settle::LeeTransaction,
    TxReport,
};
use lee::{AccountId, Actor, PrivacyPreservingTransaction};
use lee_core::{execution_state::BoundaryStep, native_token, program::StateReply, PrivateAction};
use serde_json::{json, Value};

/// Users per direction (the B3 gate's 20 pairs).
const USERS: usize = 20;
/// The fixed fee reserve of every private transaction (§B2: one reserve per operation type).
const RESERVE: u128 = 100_000_000;
/// The wallet's account gap (recovery tries accounts `0..GAP` of each kind) and burn-tag gap.
const GAP: u32 = 4;
/// Vault P, on both chains.
const VAULT_P: [u8; 20] = [0xa0; 20];
/// The one denomination every user moves: 1 ETH (index 1 on both chains).
const AMOUNT: u128 = DENOMINATIONS[1];
/// The vault's registered ETH relay fee for 1 ETH (`VaultTestBase`: 0.2% of each denomination).
const ETH_RELAY_FEE: u128 = AMOUNT / 500;
/// A claim, burn or release every minute.
const STEP_MS: u64 = 60_000;

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run this test with RISC0_DEV_MODE=1");
}

fn out_dir() -> PathBuf {
    match std::env::var("FERRATA_PRIVACY_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/privacy"),
    }
}

/// A labelled pseudo-random value (deterministic, so that a run can be repeated byte for byte).
fn h(label: &str, a: u64, b: u64) -> Hash {
    sha256(&[label.as_bytes(), &a.to_be_bytes()[..], &b.to_be_bytes()[..]].concat())
}

fn h20(label: &str, a: u64) -> [u8; 20] {
    h(label, a, 0)[..20].try_into().unwrap()
}

/// A uniformly drawn permutation of `0..n` (Fisher–Yates on `h`).
fn permutation(label: &str, n: usize) -> Vec<usize> {
    let mut p: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let r = u64::from_be_bytes(h(label, i as u64, 1)[..8].try_into().unwrap());
        p.swap(i, (r % (i as u64 + 1)) as usize);
    }
    p
}

fn hx(b: &[u8]) -> String {
    hex::encode(b)
}

fn word(v: u128) -> [u8; 32] {
    bw::word_of_u128(v)
}

/// One user: what its seed derives (the bridge keys and its LEZ accounts) and its Ethereum
/// addresses (a fresh depositor, a fresh release recipient).
struct User {
    bridge_seed: Hash,
    keys: BridgeKeys,
    lez_seed: u8,
    dest: AccountId,
    public_dest: AccountId,
    depositor: [u8; 20],
    recipient: [u8; 20],
}

impl User {
    fn new(u: usize) -> Self {
        let bridge_seed = h("ferrata/privacy-suite/bridge-seed", u as u64, 0);
        let lez_seed = 0x80 + u as u8;
        User {
            bridge_seed,
            keys: BridgeKeys::from_bridge_seed(&bridge_seed),
            lez_seed,
            dest: private_account_id(lez_seed),
            public_dest: AccountId::new(h("ferrata/privacy-suite/public-account", u as u64, 0)),
            depositor: h20("ferrata/privacy-suite/depositor", u as u64),
            recipient: h20("ferrata/privacy-suite/recipient", u as u64),
        }
    }

    /// The wallet's `a`-th account of `kind`: account 0 is the one this suite uses.
    fn account(&self, u: usize, kind: DestKind, a: u32) -> Hash {
        match (kind, a) {
            (DestKind::Private, 0) => *self.dest.value(),
            (DestKind::Public, 0) => *self.public_dest.value(),
            _ => h("ferrata/privacy-suite/other-account", u as u64, u64::from(a) * 2 + kind as u64),
        }
    }

    fn deposit(&self, u: usize, kind: DestKind) -> Deposit {
        Deposit {
            leaf_index: u as u64,
            depositor: self.depositor,
            token: ETH,
            amount: word(AMOUNT),
            s: deposit_secret(&self.keys.auth, &chain_id_u256(CHAIN_ID), &VAULT_P, &self.depositor, 0),
            kind,
            dest: self.account(u, kind, 0),
        }
    }

    /// The wallet's `i`-th burn note under `cid` for its recipient (D12's mac), and its `sOut`.
    fn note(&self, cid: &Hash, i: u64) -> (Note, Hash) {
        let r = burn_tag(&self.keys.view, cid, i);
        let s_out = burn_secret(&self.keys.auth, cid, &r);
        let c_out = burn_commitment(&s_out, &chain_id_u256(CHAIN_ID), &VAULT_P, &self.recipient);
        let memo = xor_memo(&self.keys.view, &r, &self.recipient);
        let amount = word(AMOUNT);
        let mac = burn_mac_v2(&self.keys.view, &r, &memo, &ETH, &amount, &c_out);
        (Note { r, memo, mac, token: ETH, amount, c_out }, s_out)
    }
}

/// Everything public about one LEZ transaction.
struct Record {
    id: String,
    kind: &'static str,
    variant: &'static str,
    seq: usize,
    block_id: u64,
    time_ms: u64,
    tx: Vec<u8>,
    block: Vec<u8>,
    events: Vec<Vec<u8>>,
    diff: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>,
    root_before: Vec<u8>,
    root_after: Vec<u8>,
    fields: Vec<(String, Vec<u8>)>,
    /// The calls to the bridge program the block shows in the clear (the checkpoint's parser).
    clear_calls: Vec<(String, Vec<u8>)>,
}

impl Record {
    fn json(&self) -> Value {
        json!({
            "id": self.id, "kind": self.kind, "variant": self.variant, "seq": self.seq,
            "block_id": self.block_id, "time_ms": self.time_ms,
            "tx": hx(&self.tx), "block": hx(&self.block),
            "events": self.events.iter().map(|e| hx(e)).collect::<Vec<_>>(),
            "diff": self.diff.iter().map(|(a, b, c)| json!({"account": hx(a), "before": hx(b), "after": hx(c)}))
                .collect::<Vec<_>>(),
            "commitment_root_before": hx(&self.root_before), "commitment_root_after": hx(&self.root_after),
            "fields": self.fields.iter().map(|(n, v)| json!([n, hx(v)])).collect::<Vec<_>>(),
            "clear_calls": self.clear_calls.iter().map(|(o, m)| json!([o, hx(m)])).collect::<Vec<_>>(),
        })
    }

    /// The same record with every occurrence of each `(from, to)` pair of 32-byte values replaced.
    fn substituted(&self, variant: &'static str, pairs: &[(Hash, Hash)]) -> Record {
        let sub = |b: &[u8]| -> Vec<u8> {
            let mut out = b.to_vec();
            for (from, to) in pairs {
                let mut i = 0;
                while i + 32 <= out.len() {
                    if out[i..i + 32] == from[..] {
                        out[i..i + 32].copy_from_slice(to);
                        i += 32;
                    } else {
                        i += 1;
                    }
                }
            }
            out
        };
        Record {
            id: self.id.replace(self.variant, variant),
            kind: self.kind,
            variant,
            seq: self.seq,
            block_id: self.block_id,
            time_ms: self.time_ms,
            tx: sub(&self.tx),
            block: sub(&self.block),
            events: self.events.iter().map(|e| sub(e)).collect(),
            diff: self.diff.iter().map(|(a, b, c)| (sub(a), sub(b), sub(c))).collect(),
            root_before: self.root_before.clone(),
            root_after: self.root_after.clone(),
            fields: self.fields.iter().map(|(n, v)| (n.clone(), sub(v))).collect(),
            clear_calls: self.clear_calls.iter().map(|(o, m)| (o.clone(), sub(m))).collect(),
        }
    }
}

/// A LEZ block holding one transaction, in the layout `checkpoint-core` parses (the block id and
/// timestamp filled; the previous-hash and hash fields left zero: the harness does not chain blocks).
fn block(block_id: u64, timestamp_ms: u64, tx: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&block_id.to_le_bytes());
    b.extend_from_slice(&[0u8; 32]);
    b.extend_from_slice(&[0u8; 32]);
    b.extend_from_slice(&timestamp_ms.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(tx);
    b.extend_from_slice(&0u32.to_le_bytes());
    b.push(2);
    b
}

struct Calls {
    bridge: Hash,
    calls: Vec<(String, Vec<u8>)>,
}

impl<'a> Visitor<'a> for Calls {
    fn tx(&mut self, _index: u32, _kind: TxKind, _bytes: &'a [u8]) {}
    fn call(&mut self, _index: u32, origin: Origin, _from: Option<CpActor<'a>>, to: CpActor<'a>, message: &'a [u8]) {
        if *to.program_account_id == self.bridge {
            self.calls.push((format!("{origin:?}"), message.to_vec()));
        }
    }
}

fn clear_calls(w: &World, block_bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut visitor = Calls { bridge: *w.program.value(), calls: Vec::new() };
    parse_block(block_bytes, &mut visitor).expect("the checkpoint parses the block");
    visitor.calls
}

/// Every account a transaction with these declared actors can change: the actors' accounts, the
/// fee program's accounts and the harness's payer (the fee refund).
fn touched(w: &World, actors: &[Actor]) -> Vec<AccountId> {
    let mut ids: BTreeSet<AccountId> = actors.iter().map(|a| a.account_id).collect();
    ids.insert(system_accounts::fee_state_account_id());
    ids.insert(system_accounts::fee_escrow_account_id());
    ids.insert(system_accounts::fee_inbox_account_id());
    ids.insert(w.chain.payer);
    ids.into_iter().collect()
}

fn snapshot(w: &World, ids: &[AccountId]) -> Vec<(AccountId, Vec<u8>)> {
    ids.iter().map(|id| (*id, borsh::to_vec(&w.chain.state.get_account_by_id(*id)).unwrap())).collect()
}

fn state_diff(before: &[(AccountId, Vec<u8>)], after: &[(AccountId, Vec<u8>)]) -> Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    before
        .iter()
        .zip(after)
        .filter(|((_, b), (_, a))| a != b)
        .map(|((id, b), (_, a))| (id.value().to_vec(), b.clone(), a.clone()))
        .collect()
}

fn commitment_root(w: &World) -> Vec<u8> {
    w.chain.state.commitment_set_digest().to_vec()
}

/// The named fields of a bridge message.
fn bridge_fields(prefix: &str, message: &BridgeMessage, f: &mut Vec<(String, Vec<u8>)>) {
    let mut push = |name: &str, v: Vec<u8>| f.push((format!("{prefix}.{name}"), v));
    match message {
        BridgeMessage::Issue { config_id, token, nullifier, amount, range, leaf_count, mint_to, relayer, time_ms } => {
            push("Issue.config_id", config_id.to_vec());
            push("Issue.token", token.to_vec());
            push("Issue.nullifier", nullifier.to_vec());
            push("Issue.amount", amount.to_le_bytes().to_vec());
            push("Issue.range", borsh::to_vec(range).unwrap());
            push("Issue.leaf_count", leaf_count.to_le_bytes().to_vec());
            if let Some(m) = mint_to {
                push("Issue.mint_to", m.to_vec());
            }
            if let Some(r) = relayer {
                push("Issue.relayer", r.to_vec());
            }
            push("Issue.time_ms", time_ms.to_le_bytes().to_vec());
        }
        BridgeMessage::Burn { config_id, r, memo, mac, token, amount, c_out } => {
            push("Burn.config_id", config_id.to_vec());
            push("Burn.r", r.to_vec());
            push("Burn.memo", memo.to_vec());
            push("Burn.mac", mac.to_vec());
            push("Burn.token", token.to_vec());
            push("Burn.amount", amount.to_vec());
            push("Burn.c_out", c_out.to_vec());
        }
        BridgeMessage::Claim { config_id, leaf_count, witness, range, relayer, time_ms } => {
            push("Claim.config_id", config_id.to_vec());
            push("Claim.leaf_count", leaf_count.to_le_bytes().to_vec());
            push("Claim.witness.depositor", witness.depositor.to_vec());
            push("Claim.witness.token", witness.token.to_vec());
            push("Claim.witness.amount", witness.amount.to_vec());
            push("Claim.witness.leaf_index", witness.leaf_index.to_le_bytes().to_vec());
            for (i, s) in witness.path.iter().enumerate() {
                push(&format!("Claim.witness.path[{i}]"), s.to_vec());
            }
            push("Claim.witness.s", witness.s.to_vec());
            push("Claim.witness.kind", vec![witness.kind]);
            push("Claim.witness.dest", witness.dest.to_vec());
            push("Claim.range", borsh::to_vec(range).unwrap());
            if let Some(r) = relayer {
                push("Claim.relayer", r.to_vec());
            }
            push("Claim.time_ms", time_ms.to_le_bytes().to_vec());
        }
        BridgeMessage::ImportRoot { config_id, record_slot, root, leaf_count, .. } => {
            push("ImportRoot.config_id", config_id.to_vec());
            push("ImportRoot.record_slot", record_slot.to_le_bytes().to_vec());
            push("ImportRoot.root", root.to_vec());
            push("ImportRoot.leaf_count", leaf_count.to_le_bytes().to_vec());
        }
        BridgeMessage::Read => push("Read", Vec::new()),
        _ => {}
    }
}

/// The named fields of a delivered message: a bridge message, the native token's, or a reply.
fn delivery_fields(prefix: &str, to: &Actor, bytes: &[u8], program: AccountId, f: &mut Vec<(String, Vec<u8>)>) {
    if to.program_account_id == program {
        if let Ok(m) = borsh::from_slice::<BridgeMessage>(bytes) {
            bridge_fields(prefix, &m, f);
            return;
        }
    }
    if to.program_account_id == native_token::NATIVE_TOKEN_PROGRAM_ID {
        if let Ok(native_token::Message::Credit(amount)) = borsh::from_slice(bytes) {
            f.push((format!("{prefix}.native.Credit"), amount.to_le_bytes().to_vec()));
            return;
        }
    }
    if let Ok(reply) = borsh::from_slice::<StateReply>(bytes) {
        f.push((format!("{prefix}.StateReply.state"), reply.state.into_inner()));
    }
}

fn action_fields(prefix: &str, a: &PrivateAction, f: &mut Vec<(String, Vec<u8>)>) {
    f.push((format!("{prefix}.nullifier"), borsh::to_vec(&a.nullifier).unwrap()));
    f.push((format!("{prefix}.commitment_root"), borsh::to_vec(&a.root).unwrap()));
    f.push((format!("{prefix}.commitment"), borsh::to_vec(&a.commitment).unwrap()));
    // `Vec`-backed: the length prefix (4 bytes) stays outside the field.
    f.push((
        format!("{prefix}.note.ciphertext"),
        borsh::to_vec(&a.encrypted_post_state.ciphertext).unwrap()[4..].to_vec(),
    ));
    f.push((format!("{prefix}.note.epk"), borsh::to_vec(&a.encrypted_post_state.epk).unwrap()[4..].to_vec()));
}

fn boundary_fields(prefix: &str, steps: &[BoundaryStep], program: AccountId, f: &mut Vec<(String, Vec<u8>)>) {
    for (i, step) in steps.iter().enumerate() {
        let (dir, d) = match step {
            BoundaryStep::PrivateToPublic(d) => ("out", d),
            BoundaryStep::PublicToPrivate(d) => ("in", d),
            _ => continue,
        };
        let p = format!("{prefix}[{i}].{dir}");
        f.push((format!("{p}.from"), d.envelope.from.account_id.value().to_vec()));
        f.push((format!("{p}.to"), d.envelope.to.account_id.value().to_vec()));
        delivery_fields(&format!("{p}.message"), &d.envelope.to, &d.envelope.message, program, f);
    }
}

/// Every field a privacy-preserving transaction publishes that this test can name.
fn private_fields(tx: &PrivacyPreservingTransaction, program: AccountId) -> Vec<(String, Vec<u8>)> {
    let m = &tx.message;
    let e = &m.execution;
    let mut f = Vec::new();
    for (i, a) in m.public_context.actors.iter().enumerate() {
        f.push((
            format!("public_context.actors[{i}]"),
            [&a.account_id.value()[..], &a.program_account_id.value()[..]].concat(),
        ));
    }
    boundary_fields("boundary", &e.boundary, program, &mut f);
    for (j, a) in e.private_actions.iter().enumerate() {
        action_fields(&format!("private_actions[{j}]"), a, &mut f);
    }
    if let Some(fee) = &e.fee {
        if let Some(r) = &fee.refund_to {
            f.push(("fee.refund_to".into(), r.value().to_vec()));
        }
        if let Some(fb) = &fee.fallback {
            action_fields("fee.fallback.action", &fb.action, &mut f);
            boundary_fields("fee.fallback.boundary", &fb.boundary, program, &mut f);
        }
    }
    f.push(("validity".into(), borsh::to_vec(&e.validity).unwrap()));
    let whole = borsh::to_vec(&LeeTransaction::PrivacyPreserving(tx.clone())).unwrap();
    let message_len = borsh::to_vec(m).unwrap().len();
    f.push(("witness_set".into(), whole[1 + message_len..].to_vec()));
    f
}

/// Every field of a public transaction this test can name.
fn public_fields(tx_bytes: &[u8], program: AccountId) -> Vec<(String, Vec<u8>)> {
    let Ok(LeeTransaction::Public(tx)) = borsh::from_slice::<LeeTransaction>(tx_bytes) else {
        panic!("a public transaction")
    };
    let m = &tx.message;
    let mut f = Vec::new();
    for (i, a) in m.public_context.actors.iter().enumerate() {
        f.push((
            format!("public_context.actors[{i}]"),
            [&a.account_id.value()[..], &a.program_account_id.value()[..]].concat(),
        ));
    }
    f.push(("root.to".into(), m.execution.root.to.account_id.value().to_vec()));
    delivery_fields("root.message", &m.execution.root.to, &m.execution.root.message, program, &mut f);
    f.push(("fee".into(), borsh::to_vec(&m.execution.fee).unwrap()));
    f.push(("nonces".into(), borsh::to_vec(&m.nonces).unwrap()));
    let message_len = borsh::to_vec(m).unwrap().len();
    f.push(("witness_set".into(), tx_bytes[1 + message_len..].to_vec()));
    f
}

/// Settle one public transaction through `run` and keep what it published.
#[allow(clippy::too_many_arguments)]
fn collect_public(
    w: &mut World,
    id: String,
    kind: &'static str,
    variant: &'static str,
    seq: usize,
    actors: &[Actor],
    run: impl FnOnce(&mut World) -> TxReport,
) -> Record {
    let ids = touched(w, actors);
    let before = snapshot(w, &ids);
    let root_before = commitment_root(w);
    let report = run(w);
    assert!(report.ok(), "{id}: {:?} (exit {:?})", report.charge.revert, report.exit_code);
    let after = snapshot(w, &ids);
    let tx = w.txs.last().unwrap().1.clone();
    let block_id = w.chain.block_id - 1;
    let body = block(block_id, w.time, &tx);
    Record {
        id,
        kind,
        variant,
        seq,
        block_id,
        time_ms: w.time,
        fields: public_fields(&tx, w.program),
        clear_calls: clear_calls(w, &body),
        tx,
        block: body,
        events: report.events.iter().map(|e| borsh::to_vec(e).unwrap()).collect(),
        diff: state_diff(&before, &after),
        root_before,
        root_after: commitment_root(w),
    }
}

/// Prove (dev mode) and settle one private transaction with fresh randomness and keep what it
/// published.
#[allow(clippy::too_many_arguments)]
fn collect_private(
    w: &mut World,
    id: String,
    kind: &'static str,
    seq: usize,
    sender: &PrivateAccount,
    message: &BridgeMessage,
    publics: Vec<Actor>,
    fresh: Freshness,
) -> Record {
    let inbox = Actor::native_balance(system_accounts::fee_inbox_account_id());
    let ids = touched(w, &[publics.clone(), vec![inbox]].concat());
    let before = snapshot(w, &ids);
    let root_before = commitment_root(w);
    let me = holder_actor(w.program, sender.id());
    let time = w.time;
    let run = w
        .chain
        .prove_and_settle_private_fresh(
            sender,
            me,
            borsh::to_vec(message).unwrap(),
            publics,
            &catalog(),
            RESERVE,
            time,
            None,
            fresh,
        )
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    assert!(run.charge.revert.is_none(), "{id}: {:?}", run.charge.revert);
    let after = snapshot(w, &ids);
    let tx = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
    assert_eq!(tx.len(), run.tx_bytes);
    let block_id = w.chain.block_id - 1;
    let body = block(block_id, time, &tx);
    Record {
        id,
        kind,
        variant: "private",
        seq,
        block_id,
        time_ms: time,
        fields: private_fields(&run.tx, w.program),
        clear_calls: clear_calls(w, &body),
        tx,
        block: body,
        events: run.events.iter().map(|e| borsh::to_vec(e).unwrap()).collect(),
        diff: state_diff(&before, &after),
        root_before,
        root_after: commitment_root(w),
    }
}

fn field<'a>(r: &'a Record, suffix: &str) -> &'a [u8] {
    &r.fields.iter().find(|(n, _)| n.ends_with(suffix)).unwrap_or_else(|| panic!("{}: no field {suffix}", r.id)).1
}

/// Init, Register (ETH) and ImportRoot (all `USERS` leaves) of vault P in `w`; returns the import.
fn set_up_vault(w: &mut World, leaves: Vec<Hash>, variant: &'static str) -> (Hash, Vault, Record) {
    let (config, cid, b) = w.add_vault(VAULT_P, leaves.clone(), 9_000_096, 103);
    let vault = Vault { address: VAULT_P, leaves };
    for report in [w.init("Init P", &config, b), w.register("Register ETH (P)", &cid, b, &eth_entry(), &VAULT_P)] {
        assert!(report.ok(), "{}: {:?} (exit {:?})", report.label, report.charge.revert, report.exit_code);
    }
    let n = USERS as u64;
    let root = vault.root(n);
    let config_actor = bw::config_actor(w.program, &cid);
    let header = ferrata_lez_wire::header_actor(harness::ferrata_account(), &harness::bridge::INSTANCE, 9_000_096);
    let actors = [config_actor, header, bw::root_actor(w.program, &cid, n)];
    let import = collect_public(w, format!("lez:import-root:{variant}:0"), "import-root", variant, 0, &actors, |w| {
        w.import("ImportRoot P n=20", &cid, b, root, n, &VAULT_P)
    });
    (cid, vault, import)
}

#[test]
fn privacy_suite_lez_half() {
    require_dev_mode();
    let dir = out_dir();
    fs::create_dir_all(&dir).unwrap();
    let chain = chain_id_u256(CHAIN_ID);
    let users: Vec<User> = (0..USERS).map(User::new).collect();
    let claim_order = permutation("ferrata/privacy-suite/claim-order", USERS);
    let burn_order = permutation("ferrata/privacy-suite/burn-order", USERS);
    let release_order = permutation("ferrata/privacy-suite/release-order", USERS);
    let plaintext_order = permutation("ferrata/privacy-suite/plaintext-order", USERS);
    let mut records: Vec<Record> = Vec::new();

    // ---- Universe A: private destinations, the default path -----------------------------------
    let mut w = World::new();
    let accounts: Vec<PrivateAccount> =
        users.iter().map(|u| w.chain.add_private_account(u.lez_seed, 1_000_000_000_000)).collect();
    for (u, user) in users.iter().enumerate() {
        assert_eq!(accounts[u].id(), user.dest, "user {u}'s private account");
    }
    let deposits: Vec<Deposit> = users.iter().enumerate().map(|(u, user)| user.deposit(u, DestKind::Private)).collect();
    let (cid, vault, import) = set_up_vault(&mut w, deposits.iter().map(Deposit::leaf).collect(), "private");
    records.push(import);
    let n = USERS as u64;

    // Claims, in claim order.
    let mut holders: Vec<Option<PrivateAccount>> = (0..USERS).map(|_| None).collect();
    let mut nullifiers = vec![[0u8; 32]; USERS];
    for (k, &u) in claim_order.iter().enumerate() {
        w.time += STEP_MS;
        let d = &deposits[u];
        let message = w.claim_privately(&cid, n, vault.witness(d, n));
        let publics = w.private_claim_actors(&cid, n, &VAULT_P, d);
        let fresh = Freshness {
            alias_opening: h("ferrata/privacy-suite/alias-opening", 1, k as u64),
            random_seed: h("ferrata/privacy-suite/random-seed", 1, k as u64),
        };
        let r = collect_private(
            &mut w,
            format!("lez:claim:private:{k:02}"),
            "claim",
            k,
            &accounts[u],
            &message,
            publics,
            fresh,
        );
        let expected = d.nullifier(CHAIN_ID, &VAULT_P);
        assert_eq!(field(&r, "Issue.nullifier"), expected, "{}: N", r.id);
        let mut r = r;
        let account = nullifier_actor(w.program, CHAIN_ID, &VAULT_P, &expected).account_id;
        r.fields.push(("derived.nullifier_account".into(), account.value().to_vec()));
        nullifiers[u] = expected;
        records.push(r);
        let holder =
            HolderState { holdings: vec![Holding { config_id: cid, token: ETH, amount: AMOUNT }], pending: None };
        let post =
            private_post_state(&w.chain, &private_keys(users[u].lez_seed), &accounts[u].account, RESERVE, &holder)
                .expect("the chain holds the claimed account's commitment");
        holders[u] = Some(PrivateAccount { keys: private_keys(users[u].lez_seed), account: post });
    }

    // Burns, in burn order: burn index k is the k-th burn the checkpoint collects.
    let mut notes: Vec<Option<(Note, Hash)>> = (0..USERS).map(|_| None).collect();
    let mut burn_leaves = Vec::new();
    for (k, &u) in burn_order.iter().enumerate() {
        w.time += STEP_MS;
        let (note, s_out) = users[u].note(&cid, 0);
        let message = BridgeMessage::burn(cid, &note);
        let holder = holders[u].as_ref().unwrap();
        let me = holder_actor(w.program, holder.id());
        let publics = w.burn_actors(me, &cid, &note)[1..].to_vec();
        let fresh = Freshness {
            alias_opening: h("ferrata/privacy-suite/alias-opening", 2, k as u64),
            random_seed: h("ferrata/privacy-suite/random-seed", 2, k as u64),
        };
        let r =
            collect_private(&mut w, format!("lez:burn:private:{k:02}"), "burn", k, holder, &message, publics, fresh);
        // The checkpoint's view of the block: exactly this note, in the clear, from public execution.
        let collected: Vec<_> = r
            .clear_calls
            .iter()
            .filter_map(|(_, m)| match CpMessage::decode(m) {
                Ok(Some(CpMessage::Burn(b))) => Some(b),
                _ => None,
            })
            .collect();
        assert_eq!(collected.len(), 1, "{}: one burn in the clear", r.id);
        let mut r = r;
        let record = bw::burn_actor(w.program, &cid, &note.hash()).account_id;
        r.fields.push(("derived.burn_record_account".into(), record.value().to_vec()));
        assert_eq!(
            (collected[0].r, collected[0].c_out, collected[0].memo, collected[0].mac),
            (note.r, note.c_out, note.memo, note.mac)
        );
        records.push(r);
        burn_leaves.push(burn_leaf(&cid, k as u64, &ETH, &note.amount, &note.c_out, &note.r));
        notes[u] = Some((note, s_out));
    }
    let burn_tree = Tree::from_leaves(burn_leaves.clone()).unwrap();
    let burn_root = burn_tree.root();

    // Releases (computed here, executed by the Foundry half), in release order over burn indices:
    // the release statement of the bridge core (what the frozen release guest runs), NOut v1.
    let mut releases_private = Vec::new();
    let mut releases_naive = Vec::new();
    for &b in &release_order {
        let u = burn_order[b];
        let (note, s_out) = notes[u].as_ref().unwrap();
        let public = ReleasePublic {
            chain_id: chain,
            vault: VAULT_P,
            config_id: cid,
            token: ETH,
            amount: word(AMOUNT),
            recipient: users[u].recipient,
            relayer: h20("ferrata/privacy-suite/eth-relayer", 0),
            relay_fee: word(ETH_RELAY_FEE),
        };
        let witness =
            ReleaseWitness { burn_index: b as u64, path: burn_tree.path(b as u64).unwrap(), s_out: *s_out, r: note.r };
        let nout = verify_release(&burn_root, &witness, &public).expect("the release statement holds");
        let naive = burn_id(&cid, b as u64, &burn_leaves[b]);
        for (list, nf) in [(&mut releases_private, nout), (&mut releases_naive, naive)] {
            list.push(json!({
                "recipient": format!("0x{}", hx(&users[u].recipient)),
                "nullifier": format!("0x{}", hx(&nf)),
                "journal": format!("0x{}", hx(&release_journal(&burn_root, &nf, &public))),
            }));
        }
    }

    // ---- The naive-nullifier variant: N = fid, written into the private claims' public data -----
    let naive_claims: Vec<Record> = records
        .iter()
        .filter(|r| r.kind == "claim")
        .map(|r| {
            let u = claim_order[r.seq];
            let d = &deposits[u];
            let fid = deposit_id(&chain, &VAULT_P, d.leaf_index, &d.leaf());
            let pda = |x: &Hash| *nullifier_actor(w.program, CHAIN_ID, &VAULT_P, x).account_id.value();
            r.substituted("naive", &[(nullifiers[u], fid), (pda(&nullifiers[u]), pda(&fid))])
        })
        .collect();
    records.extend(naive_claims);

    // ---- Recovery with the bridge core, from each wallet's seed and the whole deposit list ------
    let vault_deposits: Vec<VaultDeposit> = deposits
        .iter()
        .map(|d| VaultDeposit {
            depositor: d.depositor,
            j: 0,
            leaf_index: d.leaf_index,
            token: d.token,
            amount: d.amount,
            commitment: d.commitment(),
        })
        .collect();
    let half = USERS / 2;
    let spent_at_half: BTreeSet<Hash> = claim_order[..half].iter().map(|&u| nullifiers[u]).collect();
    let mut inbound_recovered = Vec::new();
    let mut unclaimed_users = 0;
    for (u, user) in users.iter().enumerate() {
        let owned =
            recover(&user.keys.auth, &chain, &VAULT_P, &vault_deposits, GAP, |kind, a| user.account(u, kind, a));
        assert_eq!(owned.len(), 1, "user {u} finds exactly its own deposit");
        assert_eq!((owned[0].deposit.leaf_index, owned[0].nullifier), (u as u64, nullifiers[u]));
        let unclaimed = owned.iter().filter(|o| !spent_at_half.contains(&o.nullifier)).count();
        unclaimed_users += unclaimed;
        inbound_recovered.push(json!({"user": u, "owned": owned.len(), "unclaimed_after_half": unclaimed}));
    }
    assert_eq!(unclaimed_users, USERS - half, "after half the claims, the other half is recovered as unclaimed");
    // The core's burn recovery checks the v1 mac; LEZ notes carry D12's v2 mac.
    let published: Vec<PublishedBurn> = burn_order
        .iter()
        .enumerate()
        .map(|(k, &u)| {
            let (note, _) = notes[u].as_ref().unwrap();
            PublishedBurn {
                burn_index: k as u64,
                note: BurnNote {
                    r: note.r,
                    memo: note.memo,
                    mac: note.mac,
                    token: note.token,
                    amount: note.amount,
                    c_out: note.c_out,
                },
            }
        })
        .collect();
    let core_burns_found: usize = users
        .iter()
        .map(|user| recover_burns(&user.keys, &cid, &chain, &VAULT_P, &published, u64::from(GAP)).owned.len())
        .sum();
    println!("bridge core recover_burns over the 20 LEZ notes (v2 mac): {core_burns_found} found");

    // ---- Universe B: public destinations, the plaintext path (linkable) -----------------------
    let mut wb = World::new();
    wb.add_public_accounts(&users.iter().map(|u| u.public_dest).collect::<Vec<_>>());
    let deposits_b: Vec<Deposit> =
        users.iter().enumerate().map(|(u, user)| user.deposit(u, DestKind::Public)).collect();
    let (cid_b, vault_b, import_b) = set_up_vault(&mut wb, deposits_b.iter().map(Deposit::leaf).collect(), "plaintext");
    assert_eq!(cid_b, cid, "one configuration of vault P in both universes");
    records.push(import_b);
    for (k, &u) in plaintext_order.iter().enumerate() {
        wb.time += STEP_MS;
        let d = &deposits_b[u];
        let witness = vault_b.witness(d, n);
        let range = wb.range(&cid, &ETH, d.leaf_index);
        let nf = nullifier_actor(wb.program, CHAIN_ID, &VAULT_P, &d.nullifier(CHAIN_ID, &VAULT_P));
        let mut actors = vec![bw::root_actor(wb.program, &cid, n)];
        actors.extend(wb.claim_actors(&cid, &ETH, nf, Some(user_account(&users[u])), Some(*relayer().value())));
        let time = wb.time;
        let mut r =
            collect_public(&mut wb, format!("lez:claim:plaintext:{k:02}"), "claim", "plaintext", k, &actors, |w| {
                w.claim_with("plaintext claim", &cid, n, &VAULT_P, witness, range, Some(*relayer().value()), time)
            });
        r.fields.push(("derived.nullifier_account".into(), nf.account_id.value().to_vec()));
        records.push(r);
    }

    // ---- Outputs -------------------------------------------------------------------------------
    let deposits_json = |ds: &[Deposit]| -> Vec<Value> {
        ds.iter()
            .map(|d| {
                json!({"depositor": format!("0x{}", hx(&d.depositor)), "commitment": format!("0x{}", hx(&d.commitment()))})
            })
            .collect()
    };
    let scenario = json!({
        "chain_id": CHAIN_ID,
        "vault": format!("0x{}", hx(&VAULT_P)),
        "config_id": format!("0x{}", hx(&cid)),
        "amount_wei": AMOUNT.to_string(),
        "relay_fee_wei": ETH_RELAY_FEE.to_string(),
        "relayer": format!("0x{}", hx(&h20("ferrata/privacy-suite/eth-relayer", 0))),
        "users": USERS,
        "A": {"deposits": deposits_json(&deposits), "root": format!("0x{}", hx(&vault.root(n)))},
        "B": {"deposits": deposits_json(&deposits_b), "root": format!("0x{}", hx(&vault_b.root(n)))},
        "burn_root": format!("0x{}", hx(&burn_root)),
        "burn_count": USERS,
        "releases": {"private": releases_private, "naive": releases_naive},
    });
    let truth = json!({
        "users": users.iter().enumerate().map(|(u, user)| json!({
            "user": u,
            "bridge_seed": hx(&user.bridge_seed),
            "accounts": {"private": [hx(&user.account(u, DestKind::Private, 0))], "public": [hx(&user.account(u, DestKind::Public, 0))]},
            "depositor": hx(&user.depositor),
            "recipient": hx(&user.recipient),
            "leaf_index": u,
        })).collect::<Vec<_>>(),
        "claim_order": claim_order,
        "plaintext_order": plaintext_order,
        "burn_order": burn_order,
        "release_order": release_order,
        "recovery_snapshot": half,
        "gap": GAP,
        "rust_recovery": {"inbound": inbound_recovered, "core_recover_burns_found": core_burns_found},
    });
    // Names of the accounts the state diffs show (the same in both universes but the public destinations).
    let mut labels = serde_json::Map::new();
    let mut label = |id: AccountId, name: &str| {
        labels.insert(hx(id.value()), json!(name));
    };
    label(system_accounts::fee_state_account_id(), "LEZ fee state");
    label(system_accounts::fee_escrow_account_id(), "LEZ fee escrow");
    label(system_accounts::fee_inbox_account_id(), "LEZ fee inbox");
    label(w.chain.payer, "the harness's fee payer (refund_to)");
    label(relayer(), "the relayer (its wrapped balance: relay fees)");
    label(harness::bridge::fee_recipient(), "the fee recipient (its wrapped balance: mint fees)");
    label(bw::config_actor(w.program, &cid).account_id, "config(cid) (global gate, time)");
    label(bw::token_actor(w.program, &cid, &ETH).account_id, "token(cid, ETH) (token gate, time)");
    label(bw::root_actor(w.program, &cid, n).account_id, "root(cid, 20)");
    for user in &users {
        label(user.public_dest, "a public destination (plaintext universe)");
    }
    let lez = json!({
        "accounts": labels,
        "bridge_image_id": harness::image_id_hex(methods::FERRATA_BRIDGE_ID),
        "program": hx(w.program.value()),
        "chain_id": CHAIN_ID,
        "vault": hx(&VAULT_P),
        "config_id": hx(&cid),
        "records": records.iter().map(Record::json).collect::<Vec<_>>(),
    });
    for (name, v) in [("scenario.json", &scenario), ("truth.json", &truth), ("lez.json", &lez)] {
        fs::write(dir.join(name), serde_json::to_string_pretty(v).unwrap()).unwrap();
    }
    println!(
        "privacy suite, LEZ half: {} records ({} private claims, {} burns, {} plaintext claims, {} naive) in {}",
        records.len(),
        records.iter().filter(|r| r.kind == "claim" && r.variant == "private").count(),
        records.iter().filter(|r| r.kind == "burn").count(),
        records.iter().filter(|r| r.variant == "plaintext" && r.kind == "claim").count(),
        records.iter().filter(|r| r.variant == "naive").count(),
        dir.display()
    );
}

fn user_account(user: &User) -> AccountId {
    user.public_dest
}
