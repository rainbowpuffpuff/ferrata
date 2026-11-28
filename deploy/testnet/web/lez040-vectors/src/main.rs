//! LEZ v0.4.0's own encodings, as JSON test vectors for the web clients' v0.4.0 codec (wallet/lez040.js,
//! explorer/assets/lez040.js): every value below is computed by LEZ v0.4.0's crates (lee, lee_core, key_protocol,
//! common), never by hand. Signatures are BIP-340 with fresh randomness (LEZ's `Signature::new`), so a test checks
//! them by verification and re-encodes a transaction with the signatures given here.
//!
//!   cargo run --release --locked > ../wallet/test/fixtures/lez-v040-vectors.json

use std::collections::{BTreeMap, BTreeSet};

use common::{
    HashType,
    block::{FeeReceipt, HashableBlockData, TransactionReceipt},
    transaction::LeeTransaction,
};
use key_protocol::key_management::key_tree::keys_public::ChildKeysPublic;
use lee::{
    AccountId, Actor, Authority, BoundaryStep, Delivery, EncryptedNote, EphemeralPublicKey, FeeDeclaration,
    MessageEnvelope, PrivacyPreservingTransaction, Publication, PublicAccountEvidence, PublicExecutionContext,
    PublicTransaction, RecoveryBinding, RootCall, SealedMessage, SignedTransaction as _, TransactionMessage,
    privacy_preserving_transaction::{WitnessSet as PpWitnessSet, circuit::Proof},
    public_transaction::{Message, WitnessSet},
    signature::{KeyScheme as _, PublicAccountSecret, Schnorr, Signer, initial_key, key_state, signing_key},
};
use lee_core::{
    FeeFallback, PrivateAction, ProgramImageClaim, ProvenExecution, ProvenFee,
    account::Nonce,
    native_token::{self, NATIVE_TOKEN_PROGRAM_ID},
    program::PdaSeed,
    signing_key::KeyType,
};
use serde_json::{Value, json};

/// The seed of LEZ's own key tree tests (lee/key_protocol/src/key_management/key_tree/keys_public.rs).
const SEED: [u8; 64] = [
    88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173, 134, 61, 109, 34, 103,
    121, 39, 237, 14, 107, 194, 24, 194, 191, 14, 237, 185, 12, 87, 22, 227, 38, 71, 17, 144, 251, 118, 217, 115, 33,
    222, 201, 61, 203, 246, 121, 214, 6, 187, 148, 92, 44, 253, 210, 37,
];

fn hx(b: impl AsRef<[u8]>) -> String {
    hex::encode(b)
}
fn enc<T: borsh::BorshSerialize>(v: &T) -> Vec<u8> {
    borsh::to_vec(v).expect("borsh")
}
fn dec<T: borsh::BorshDeserialize>(b: &[u8]) -> T {
    borsh::from_slice(b).expect("borsh")
}
fn id_json(id: &AccountId) -> Value {
    json!({ "hex": hx(id.value()), "base58": id.to_string() })
}
fn actor_json(a: &Actor) -> Value {
    json!({ "account": hx(a.account_id.value()), "program": hx(a.program_account_id.value()) })
}
fn authority_name(a: Authority) -> &'static str {
    match a {
        Authority::None => "none",
        Authority::Signer => "signer",
    }
}

struct Account {
    path: String,
    keys: ChildKeysPublic,
}

fn key_tree() -> Vec<Account> {
    let root = ChildKeysPublic::root(SEED);
    let mut out = vec![Account { path: "/".into(), keys: root.clone() }];
    for i in [0_u32, 1, 2, 2_147_483_661] {
        out.push(Account { path: format!("/{i}"), keys: root.nth_child(i) });
    }
    // The wallet's second level (/0/3), as `KeyTreePublic` derives a child of a child.
    out.push(Account { path: "/0/3".into(), keys: root.nth_child(0).nth_child(3) });
    out
}

fn keys_json(accounts: &[Account]) -> Value {
    let nodes: Vec<Value> = accounts
        .iter()
        .map(|a| {
            let secret = &a.keys.secret;
            let per_nonce: Vec<Value> = [0_u128, 1, 2, 7, u128::from(u64::MAX) + 9]
                .iter()
                .map(|&n| {
                    let (sk, pk) = signing_key::<Schnorr>(secret, Nonce(n));
                    let state = key_state(secret, KeyType::Schnorr, Nonce(n));
                    assert_eq!(state, Schnorr::key_state(&pk));
                    json!({
                        "nonce": n.to_string(),
                        "sk": hx(sk.value()),
                        "pk": hx(pk.value()),
                        "key_commitment": hx(Schnorr::commit(&pk)),
                        "key_state": hx(enc(&state)),
                        "key_state_display": state.to_string(),
                    })
                })
                .collect();
            json!({
                "path": a.path,
                "secret": hx(secret.value()),
                "account_id": id_json(&a.keys.account_id()),
                "initial_key": hx(enc(&initial_key(secret))),
                "keys": per_nonce,
            })
        })
        .collect();
    json!({ "seed": hx(SEED), "nodes": nodes })
}

/// One public transaction: its message, the values its signers sign, its witness set and its encodings.
fn public_vector(name: &str, note: &str, message: Message, signers: &[(&PublicAccountSecret, u128)]) -> (Value, PublicTransaction) {
    let mut sorted: Vec<_> = signers.to_vec();
    sorted.sort_by_key(|(s, n)| Signer::new(s, Nonce(*n), KeyType::Schnorr, KeyType::Schnorr).account_id());
    let next: Vec<_> =
        sorted.iter().map(|(s, n)| key_state(s, KeyType::Schnorr, Nonce(n + 1))).collect();
    let message_with_keys = message.message_with_keys(&next);
    let witness = WitnessSet::sign(
        &message,
        &signers.iter().map(|(s, n)| Signer::new(s, Nonce(*n), KeyType::Schnorr, KeyType::Schnorr)).collect::<Vec<_>>(),
    );
    let tx = PublicTransaction::new(message.clone(), witness);
    let signer_ids = tx.check_stateless().expect("the vector transaction is well formed");
    let wrapped = LeeTransaction::Public(tx.clone());
    let ctx = &message.public_context;
    let parts = json!({
        "actors": ctx.actors.iter().map(|(a, auth)| json!({ "actor": actor_json(a), "authority": authority_name(*auth) })).collect::<Vec<_>>(),
        "authorized_accounts": ctx.authorized_accounts.iter().map(|a| hx(a.value())).collect::<Vec<_>>(),
        "promotions": ctx.promotions.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
        "private_signers": ctx.private_signers.iter().map(actor_json).collect::<Vec<_>>(),
        "to": actor_json(&message.execution.root.to),
        "data": hx(&message.execution.root.message),
        "nonces": message.nonces.iter().map(|(a, n)| json!([hx(a.value()), n.0.to_string()])).collect::<Vec<_>>(),
        "fee": message.fee.map(|f| json!({ "payer": hx(f.payer.value()), "gas_limit": f.gas_limit.to_string(),
                                            "tip": f.tip.to_string(), "max_fee": f.max_fee.to_string() })),
        "evidence": message.admission_evidence.iter().map(|e| match e {
            PublicAccountEvidence::Key(k) => json!({ "key": hx(enc(k)) }),
            PublicAccountEvidence::Pda { program, seed } => json!({ "pda": { "program": hx(program.value()), "seed": hx(enc(seed)) } }),
        }).collect::<Vec<_>>(),
    });
    let v = json!({
        "name": name,
        "note": note,
        "parts": parts,
        "signers": sorted.iter().map(|(s, n)| json!({ "secret": hx(s.value()), "nonce": n.to_string() })).collect::<Vec<_>>(),
        "signer_ids": signer_ids.iter().map(|a| hx(a.value())).collect::<Vec<_>>(),
        "message": hx(enc(&message)),
        "next_keys": next.iter().map(|k| hx(enc(k))).collect::<Vec<_>>(),
        "message_with_keys": hx(message_with_keys),
        "witness_entries": tx.witness_set().entries().iter().map(|e| json!({ "auth_data": hx(&e.auth_data), "next": hx(enc(&e.next)) })).collect::<Vec<_>>(),
        "tx": hx(enc(&tx)),
        "tx_hash": hx(tx.hash()),
        "lee_transaction": hx(enc(&wrapped)),
        "wire": serde_json::to_value(&wrapped).expect("json"),
        "wire_hash": hx(wrapped.hash().0),
    });
    (v, tx)
}

fn note(fill: u8, len: usize) -> EncryptedNote {
    let mut ciphertext = Vec::new();
    ciphertext.extend_from_slice(&u32::try_from(len).unwrap().to_le_bytes());
    ciphertext.extend(std::iter::repeat_n(fill, len));
    EncryptedNote { ciphertext: dec(&ciphertext), epk: EphemeralPublicKey(vec![fill ^ 0xff; 5]) }
}

/// A privacy-preserving transaction with every part of its execution filled once, for decoders (its proof is
/// placeholder bytes: decoding does not verify it).
fn private_vector(a: &PublicAccountSecret, a_id: AccountId, x: Actor) -> (Value, PrivacyPreservingTransaction) {
    let actor_a = Actor::native_balance(a_id);
    let delivery = Delivery {
        envelope: MessageEnvelope { from: actor_a, to: x, message: vec![5, 6] },
        grants: BTreeSet::from([actor_a]),
        pda_seeds: BTreeSet::from([PdaSeed::new([3; 32])]),
    };
    let execution = ProvenExecution {
        boundary: vec![BoundaryStep::PrivateToPublic(delivery.clone()), BoundaryStep::PublicToPrivate(delivery), BoundaryStep::EndPrivateSubtree, BoundaryStep::EndPublicSubtree],
        async_messages: vec![
            Publication::Clear(MessageEnvelope { from: actor_a, to: x, message: vec![7] }),
            Publication::Sealed(SealedMessage { commitment: dec(&[4; 32]), note: note(0x44, 3) }),
        ],
        recovery_bindings: vec![RecoveryBinding { address: AccountId::new([6; 32]), note: note(0x66, 2) }],
        public_root: Some(RootCall { to: x, message: vec![9, 9] }),
        private_actions: vec![PrivateAction {
            nullifier: dec(&[1; 32]),
            root: [2; 32],
            commitment: dec(&[3; 32]),
            encrypted_post_state: note(0x33, 4),
        }],
        // blocks 10..20, timestamps up to 99
        validity: dec(&[1, 10, 0, 0, 0, 0, 0, 0, 0, 1, 20, 0, 0, 0, 0, 0, 0, 0, 0, 1, 99, 0, 0, 0, 0, 0, 0, 0]),
        program_image_claims: vec![
            ProgramImageClaim::Disclosed { account_id: AccountId::new([8; 32]), image_id: [1, 2, 3, 4, 5, 6, 7, 8] },
            ProgramImageClaim::Undisclosed { root: [9; 32] },
        ],
        proven_fee: Some(ProvenFee {
            refund_to: Some(a_id),
            tip: 11,
            fallback: Some(FeeFallback {
                nullifier: dec(&[0x0a; 32]),
                root: [0x0b; 32],
                commitment: dec(&[0x0c; 32]),
                ciphertext: dec(&[2, 0, 0, 0, 0xcc, 0xcd]),
                boundary: vec![BoundaryStep::EndPublicSubtree],
            }),
        }),
    };
    let mut public_context = PublicExecutionContext::new([(actor_a, Authority::Signer), (x, Authority::None)], [a_id]);
    public_context.promotions = BTreeSet::from([0, 3]);
    public_context.private_signers = BTreeSet::from([Actor::new(AccountId::new([0x0d; 32]), NATIVE_TOKEN_PROGRAM_ID)]);
    let message: lee::privacy_preserving_transaction::Message = TransactionMessage {
        public_context,
        execution,
        nonces: BTreeMap::from([(a_id, Nonce(3))]),
        fee: None,
        admission_evidence: vec![PublicAccountEvidence::Pda { program: x.program_account_id, seed: PdaSeed::new([0x0e; 32]) }],
    };
    let next = [key_state(a, KeyType::Schnorr, Nonce(4))];
    let message_with_keys = message.message_with_keys(&next);
    let witness = PpWitnessSet::sign(&message, Proof::from_inner(vec![0xab; 40]), &[Signer::new(a, Nonce(3), KeyType::Schnorr, KeyType::Schnorr)]);
    let tx = PrivacyPreservingTransaction::new(message.clone(), witness);
    let wrapped = LeeTransaction::PrivacyPreserving(tx.clone());
    let v = json!({
        "name": "private-filled",
        "note": "a privacy-preserving transaction with each part of its execution filled; placeholder proof (40 bytes of 0xab)",
        "message": hx(enc(&message)),
        "next_keys": next.iter().map(|k| hx(enc(k))).collect::<Vec<_>>(),
        "message_with_keys": hx(message_with_keys),
        "tx": hx(enc(&tx)),
        "tx_hash": hx(tx.hash()),
        "lee_transaction": hx(enc(&wrapped)),
        "wire": serde_json::to_value(&wrapped).expect("json"),
        "wire_hash": hx(wrapped.hash().0),
    });
    (v, tx)
}

fn main() {
    let accounts = key_tree();
    let secret = |p: &str| accounts.iter().find(|a| a.path == p).unwrap().keys.secret.clone();
    let id = |p: &str| accounts.iter().find(|a| a.path == p).unwrap().keys.account_id();
    let (sa, sb, sc) = (secret("/0"), secret("/1"), secret("/2147483661"));
    let (a, b, c) = (id("/0"), id("/1"), id("/2147483661"));
    let program = AccountId::new([0x50; 32]);
    let x = Actor::new(AccountId::new([0x58; 32]), program);

    let transfer_data = enc(&native_token::Message::Transfer { to: b, amount: 123_456_789 });
    let (v1, t1) = public_vector(
        "transfer-nonce0",
        "a native transfer from /0 at nonce 0 (its witness carries /0's initial key) to /1, admitted by /1's initial key",
        Message::new(
            Actor::native_balance(a),
            transfer_data,
            [(Actor::native_balance(a), Authority::Signer), (Actor::native_balance(b), Authority::None)],
            BTreeMap::from([(a, Nonce(0))]),
            Some(FeeDeclaration::new(a, 50_000, 3, 999_999)),
            vec![PublicAccountEvidence::Key(initial_key(&sb))],
        ),
        &[(&sa, 0)],
    );
    let (v2, t2) = public_vector(
        "call-two-signers",
        "a program call signed by /0 at nonce 5 and /2147483661 at nonce 2, paid by /2147483661, with a PDA's evidence",
        Message::new(
            x,
            vec![1, 2, 3, 4],
            [(Actor::native_balance(a), Authority::Signer), (Actor::native_balance(c), Authority::Signer), (x, Authority::None)],
            BTreeMap::from([(a, Nonce(5)), (c, Nonce(2))]),
            Some(FeeDeclaration::new(c, 1_000_000, 0, u128::from(u64::MAX) * 3)),
            vec![PublicAccountEvidence::Pda { program, seed: PdaSeed::new([0x5e; 32]) }],
        ),
        &[(&sc, 2), (&sa, 5)],
    );
    let (v3, t3) = public_vector(
        "fee-exempt",
        "no fee declaration (as LEZ's fee-exempt shapes), /1 at nonce 1",
        Message::new(Actor::native_balance(b), vec![2], [(Actor::native_balance(b), Authority::Signer)], BTreeMap::from([(b, Nonce(1))]), None, vec![]),
        &[(&sb, 1)],
    );
    let (vp, tp) = private_vector(&sa, a, x);

    let receipts = vec![
        TransactionReceipt { success: true, fee: FeeReceipt { gas_exec: 21_000, gas_stor: 310, fee_base: 1_234_567, tip: 3, refund: 999 } },
        TransactionReceipt { success: false, fee: FeeReceipt { gas_exec: 1_000_000, gas_stor: 0, fee_base: u128::from(u64::MAX) + 1, tip: 0, refund: 7 } },
        TransactionReceipt { success: true, fee: FeeReceipt { gas_exec: 5, gas_stor: 6, fee_base: 7, tip: 11, refund: 0 } },
    ];
    // Three user transactions with receipts; the fee-exempt one stands last for the trailing system transactions,
    // which have none.
    let hashable = HashableBlockData {
        block_id: 7,
        prev_block_hash: HashType([9; 32]),
        timestamp: 1_764_000_000_000,
        transactions: vec![LeeTransaction::Public(t1), LeeTransaction::Public(t2), LeeTransaction::PrivacyPreserving(tp), LeeTransaction::Public(t3)],
        receipts: receipts.clone(),
    };
    let block = hashable.into_pending_block();
    assert!(block.body.is_reverted(1) && !block.body.is_reverted(0) && !block.body.is_reverted(3));
    let empty = HashableBlockData { block_id: 1, prev_block_hash: HashType([0; 32]), timestamp: 0, transactions: vec![], receipts: vec![] }.into_pending_block();

    let out = json!({
        "lez": { "tag": "v0.4.0", "commit": "a255b42c0", "made_by": "deploy/testnet/web/lez040-vectors (LEZ v0.4.0's crates)" },
        "keys": keys_json(&accounts),
        "public": [v1, v2, v3],
        "private": [vp],
        "blocks": [
            { "name": "four-transactions", "note": "transactions 0, 1, 3 public, 2 privacy-preserving; receipts for the first three; 1 reverted",
              "borsh": hx(enc(&block)), "json": serde_json::to_value(&block).expect("json"), "hash": hx(block.header.hash.0),
              "computed_hash": hx(block.recompute_hash().0),
              "tx_hashes": block.body.transactions.iter().map(|t| hx(t.hash().0)).collect::<Vec<_>>(),
              "reverted": (0..block.body.transactions.len()).filter(|&i| block.body.is_reverted(i)).collect::<Vec<_>>() },
            { "name": "empty", "borsh": hx(enc(&empty)), "json": serde_json::to_value(&empty).expect("json"), "hash": hx(empty.header.hash.0) },
        ],
        "receipts": receipts.iter().map(|r| json!({ "borsh": hx(enc(r)), "json": serde_json::to_string(r).expect("json") })).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&out).expect("json"));
}
