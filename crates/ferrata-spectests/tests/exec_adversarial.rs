//! Adversarial tests for the execution layer, each with the exact code it must produce:
//! tampered, truncated, extended and spliced proofs; substituted values, keys and roots;
//! inclusion and exclusion confused; the known bridge-exploit classes (forged inner nodes, a
//! proof that stops at an ancestor); receipts, logs and headers; and the EIP-2935 hop.

use ferrata_core::{
    attest::{
        attest_log, attest_storage, history_hop, Binding, HeaderRecord, HISTORY_CODE_HASH, HISTORY_SERVE_WINDOW,
        HISTORY_STORAGE_ADDRESS,
    },
    mpt::verify,
    Error,
};
use ferrata_spectests::execkit::{
    account_rlp, enc, header, keccak, record_ssz, relink, w, Rng, Trie, World, FAILED_TX, SMALL, TOKEN,
};

/// A large secure trie and one present key whose proof is at least four nodes deep.
fn deep() -> (Trie, Vec<u8>, Vec<Vec<u8>>) {
    let mut rng = Rng::new(7);
    let trie = Trie::new((0..2_000).map(|_| {
        let len = 1 + rng.below(60);
        (rng.bytes(32), rng.bytes(len))
    }));
    let key = trie.keys().into_iter().find(|k| trie.proof(k).len() >= 4).unwrap();
    let proof = trie.proof(&key);
    (trie, key, proof)
}

#[test]
fn a_tampered_node_at_every_depth_is_refused() {
    let (trie, key, proof) = deep();
    let root = trie.root();
    assert_eq!(verify(&root, &key, &proof), Ok(trie.get(&key)));
    for depth in 0..proof.len() {
        for at in [0, 1, proof[depth].len() / 2, proof[depth].len() - 1] {
            let mut p = proof.clone();
            p[depth][at] ^= 0x01;
            assert_eq!(verify(&root, &key, &p), Err(Error::ProofNodeMismatch), "depth {depth}, byte {at}");
        }
    }
}

#[test]
fn truncated_extended_and_reordered_proofs_are_refused() {
    let (trie, key, proof) = deep();
    let root = trie.root();
    let n = proof.len();
    assert_eq!(verify(&root, &key, &proof[..n - 1]), Err(Error::ProofIncomplete), "last node dropped");
    assert_eq!(verify(&root, &key, &proof[1..]), Err(Error::ProofNodeMismatch), "root dropped");
    let mut p = proof.clone();
    p.remove(1);
    assert_eq!(verify(&root, &key, &p), Err(Error::ProofNodeMismatch), "middle node dropped");
    for extra in [proof[0].clone(), proof[n - 1].clone(), vec![0x80], vec![]] {
        let mut p = proof.clone();
        p.push(extra);
        assert_eq!(verify(&root, &key, &p), Err(Error::ProofTrailingNodes), "node appended");
    }
    let mut p = proof.clone();
    p.insert(0, proof[1].clone());
    assert_eq!(verify(&root, &key, &p), Err(Error::ProofNodeMismatch), "node prepended");
    let mut p = proof.clone();
    p.swap(1, 2);
    assert_eq!(verify(&root, &key, &p), Err(Error::ProofNodeMismatch), "nodes swapped");
}

#[test]
fn substituted_values_keys_and_roots() {
    let (trie, key, proof) = deep();
    let root = trie.root();
    // Value substitution inside the leaf.
    let leaf = ferrata_core::rlp::decode(proof.last().unwrap()).unwrap();
    let [path, value] = leaf.items::<2>().unwrap();
    let mut forged_value = value.payload.to_vec();
    forged_value[0] ^= 0xff;
    let forged = enc::list(&[enc::bytes(path.payload), enc::bytes(&forged_value)]);
    let mut p = proof.clone();
    *p.last_mut().unwrap() = forged.clone();
    assert_eq!(verify(&root, &key, &p), Err(Error::ProofNodeMismatch), "value substituted");
    // Key substitution: another key's proof never yields this key's value. A key that shares the
    // whole path up to the leaf gets a correct proof of its absence; others fail.
    let mut sibling = key.clone();
    sibling[31] ^= 0x01;
    assert_eq!(verify(&root, &sibling, &proof), Ok(None), "sibling key: absent");
    let mut far = key.clone();
    far[0] ^= 0x80;
    assert_eq!(verify(&root, &far, &proof), Err(Error::ProofNodeMismatch), "far key");
    // The right key and proof under another trie's root.
    let mut rng = Rng::new(8);
    let other = Trie::new((0..50).map(|_| (rng.bytes(32), rng.bytes(8))));
    assert_eq!(verify(&other.root(), &key, &proof), Err(Error::ProofNodeMismatch), "wrong root");
}

#[test]
fn inclusion_and_exclusion_cannot_be_confused() {
    let (trie, key, proof) = deep();
    let root = trie.root();
    // A truncated inclusion proof is not an exclusion proof.
    for keep in 1..proof.len() {
        assert_eq!(verify(&root, &key, &proof[..keep]), Err(Error::ProofIncomplete), "kept {keep}");
    }
    // An absent key's exclusion proof does not prove a present key absent...
    let mut absent = key.clone();
    absent[0] ^= 0x01;
    absent[1] ^= 0x77;
    let exclusion = trie.proof(&absent);
    assert_eq!(verify(&root, &absent, &exclusion), Ok(None));
    // (The two keys part at the second nibble, inside a full branch: the exclusion proof's next
    // node is the absent key's subtree, not the present key's.)
    assert_eq!(verify(&root, &key, &exclusion), Err(Error::ProofNodeMismatch));
    // ...and appending the present key's leaf does not turn it into an inclusion proof.
    let mut appended = exclusion.clone();
    appended.push(proof.last().unwrap().clone());
    assert_eq!(verify(&root, &absent, &appended), Err(Error::ProofTrailingNodes));
}

#[test]
fn bridge_exploit_classes() {
    let (trie, key, proof) = deep();
    let root = trie.root();
    // A forged trie holding the attacker's value for the key: refused at the root.
    let forged_trie = Trie::new(
        trie.keys()
            .into_iter()
            .map(|k| (k.clone(), if k == key { vec![0x66] } else { trie.get(&k).unwrap().to_vec() })),
    );
    let forged = forged_trie.proof(&key);
    assert_eq!(verify(&forged_trie.root(), &key, &forged), Ok(Some(&[0x66][..])), "consistent with its own root");
    assert_eq!(verify(&root, &key, &forged), Err(Error::ProofNodeMismatch), "but not with the real one");
    // A forged subtree spliced under genuine upper nodes: refused where it is attached.
    for depth in 1..proof.len() {
        let mut spliced = proof[..depth].to_vec();
        spliced.extend(forged[depth.min(forged.len() - 1)..].iter().cloned());
        assert_eq!(verify(&root, &key, &spliced), Err(Error::ProofNodeMismatch), "spliced at depth {depth}");
    }
    // A proof that stops at an ancestor never yields the ancestor (or its hash) as a value.
    for keep in 1..proof.len() {
        assert_eq!(verify(&root, &key, &proof[..keep]), Err(Error::ProofIncomplete));
    }
    // A forged leaf behind a forged, hash-consistent path (as if the root were the attacker's):
    // the structure is still checked; an 18-item "branch" or a leaf with trailing bytes cannot
    // hide a second value.
    let mut p = proof.clone();
    let last = p.len() - 1;
    let leaf = ferrata_core::rlp::decode(&proof[last]).unwrap();
    let [path, value] = leaf.items::<2>().unwrap();
    let three = enc::list(&[enc::bytes(path.payload), enc::bytes(value.payload), enc::bytes(&[0x66])]);
    let r = relink(&mut p, last, three);
    assert_eq!(verify(&r, &key, &p), Err(Error::ProofNodeInvalid), "leaf with a third item");
    let mut p = proof.clone();
    let with_tail = [enc::list(&[enc::bytes(path.payload), enc::bytes(value.payload)]), enc::bytes(&[0x66])].concat();
    let r = relink(&mut p, last, with_tail);
    assert_eq!(verify(&r, &key, &p), Err(Error::RlpMalformed), "leaf with bytes after it");
}

#[test]
fn receipts_logs_and_records() {
    let world = World::new(3);
    let record = world.record(1, 700);
    // The proof of receipt 5 asked about receipt 4: the walk reaches another leaf.
    assert_eq!(attest_log(&record, 1, 4, 0, &world.receipt_proof(5)).unwrap_err(), Error::ProofNodeMismatch);
    // No receipt at index 40 (a valid exclusion proof).
    assert_eq!(attest_log(&record, 1, 40, 0, &world.receipt_proof(40)).unwrap_err(), Error::ReceiptNotIncluded);
    // Receipt 6 has two logs.
    assert!(attest_log(&record, 1, 6, 1, &world.receipt_proof(6)).is_ok());
    assert_eq!(attest_log(&record, 1, 6, 2, &world.receipt_proof(6)).unwrap_err(), Error::LogIndexOutOfRange);
    // Receipt 7 failed: its logs are not attested even though it has some.
    assert_eq!(
        attest_log(&record, 1, FAILED_TX, 0, &world.receipt_proof(FAILED_TX)).unwrap_err(),
        Error::ReceiptFailed
    );
    // A storage proof of one account replayed under another.
    let slot = world.slots[&TOKEN][0].0;
    let token_storage = world.storage[&TOKEN].proof(&keccak(&slot));
    let small_account = world.state.proof(&keccak(&SMALL));
    assert_eq!(
        attest_storage(&record, 1, &SMALL, &small_account, &slot, &token_storage).unwrap_err(),
        Error::ProofNodeMismatch
    );
    // An account proof under a record of another block.
    let other = World::new(4).record(1, 701);
    let token_account = world.state.proof(&keccak(&TOKEN));
    assert_eq!(
        attest_storage(&other, 1, &TOKEN, &token_account, &slot, &token_storage).unwrap_err(),
        Error::ProofNodeMismatch
    );
    // A record of another chain.
    assert_eq!(
        attest_storage(&record, 11155111, &TOKEN, &token_account, &slot, &token_storage).unwrap_err(),
        Error::ChainMismatch
    );
}

/// A chain of three blocks for history hops: block `r`'s state holds the history contract with
/// the hashes of `n1` and `n1 + 1`; block `n1`'s state holds it with the hash of `n2`.
struct History {
    r: u64,
    record: HeaderRecord,
    state: Trie,
    storage: Trie,
    headers: Vec<(u64, Vec<u8>)>,
    n1_state: Trie,
    n1_storage: Trie,
}

fn slot_of(n: u64) -> [u8; 32] {
    w(u128::from(n % HISTORY_SERVE_WINDOW))
}

fn contract_state(storage: &Trie, code_hash: [u8; 32], extra: u64) -> Trie {
    let mut rng = Rng::new(extra);
    let mut accounts: Vec<(Vec<u8>, Vec<u8>)> =
        (0..100).map(|_| (keccak(&rng.bytes(20)).to_vec(), account_rlp(1, 1, keccak(&[0x80]), keccak(b"")))).collect();
    accounts.push((keccak(&HISTORY_STORAGE_ADDRESS).to_vec(), account_rlp(1, 0, storage.root(), code_hash)));
    Trie::new(accounts)
}

fn history() -> History {
    let r = 1_000_000;
    let n1 = r - 100;
    let n2 = n1 - HISTORY_SERVE_WINDOW;
    let h2 = header(n2, [0x12; 32], [0x13; 32], 21);
    let n1_storage = Trie::new([(keccak(&slot_of(n2)).to_vec(), enc::uint(&keccak(&h2)))]);
    let n1_state = contract_state(&n1_storage, HISTORY_CODE_HASH, 1);
    let h1 = header(n1, n1_state.root(), [0x14; 32], 21);
    let h1b = header(n1 + 1, [0x15; 32], [0x16; 32], 21);
    let oldest = header(r - HISTORY_SERVE_WINDOW, [0x17; 32], [0x18; 32], 20);
    let entries: Vec<(Vec<u8>, Vec<u8>)> = [(n1, &h1), (n1 + 1, &h1b), (r - HISTORY_SERVE_WINDOW, &oldest)]
        .iter()
        .map(|(n, h)| (keccak(&slot_of(*n)).to_vec(), enc::uint(&keccak(h))))
        .chain((0..40u64).map(|i| (keccak(&slot_of(r - 2_000 - i)).to_vec(), enc::uint(&keccak(&i.to_be_bytes())))))
        .collect();
    let storage = Trie::new(entries);
    let state = contract_state(&storage, HISTORY_CODE_HASH, 2);
    let record =
        HeaderRecord::from_ssz(&record_ssz(1, 31_000_000, r, 0, keccak(b"r"), state.root(), [0; 32], 0, 0)).unwrap();
    History {
        r,
        record,
        state,
        storage,
        headers: vec![(n1, h1), (n1 + 1, h1b), (n2, h2), (r - HISTORY_SERVE_WINDOW, oldest)],
        n1_state,
        n1_storage,
    }
}

impl History {
    fn header(&self, n: u64) -> &[u8] {
        &self.headers.iter().find(|(m, _)| *m == n).unwrap().1
    }
    fn hop(&self, n: u64, header: &[u8]) -> Result<HeaderRecord, Error> {
        let account = self.state.proof(&keccak(&HISTORY_STORAGE_ADDRESS));
        let storage = self.storage.proof(&keccak(&slot_of(n)));
        history_hop(&self.record, &account, &storage, header)
    }
}

#[test]
fn history_hops_and_their_refusals() {
    let h = history();
    let (n1, n2) = (h.r - 100, h.r - 100 - HISTORY_SERVE_WINDOW);
    // One hop, and the window's inclusive edge.
    let rec = h.hop(n1, h.header(n1)).unwrap();
    assert_eq!(
        (rec.number(), rec.block_hash(), rec.chain_id(), rec.anchor_slot()),
        (n1, keccak(h.header(n1)), 1, 31_000_000)
    );
    assert_eq!(
        (rec.state_root(), rec.receipts_root(), rec.binding()),
        (h.n1_state.root(), [0x14; 32], Binding::History { hops: 1 })
    );
    let edge = h.r - HISTORY_SERVE_WINDOW;
    assert_eq!(h.hop(edge, h.header(edge)).unwrap().number(), edge);
    // Hops chain: from the derived record, one more hop with proofs under its own state.
    let account = h.n1_state.proof(&keccak(&HISTORY_STORAGE_ADDRESS));
    let storage = h.n1_storage.proof(&keccak(&slot_of(n2)));
    let rec2 = history_hop(&rec, &account, &storage, h.header(n2)).unwrap();
    assert_eq!((rec2.number(), rec2.binding(), rec2.anchor_slot()), (n2, Binding::History { hops: 2 }, 31_000_000));
    // Out of the window: the record's own block, a later block, one block too old.
    for n in [h.r, h.r + 1, h.r - HISTORY_SERVE_WINDOW - 1] {
        let hdr = header(n, [1; 32], [2; 32], 21);
        assert_eq!(h.hop(n, &hdr), Err(Error::HistoryOutOfWindow), "block {n}");
    }
    // The header of a block the proof is not for: its slot's proof is another key's.
    assert_eq!(h.hop(n1, h.header(n1 + 1)), Err(Error::ProofNodeMismatch), "slot mismatch");
    // A substituted header with the right number: the slot does not hold its hash.
    let substituted = header(n1, [0x99; 32], [0x14; 32], 21);
    assert_eq!(h.hop(n1, &substituted), Err(Error::HistoryHashMismatch));
    // A block whose hash was never stored (the slot is empty: a valid exclusion proof).
    let unstored = header(h.r - 50, [1; 32], [2; 32], 21);
    assert_eq!(h.hop(h.r - 50, &unstored), Err(Error::HistoryHashMismatch));
    // A header of an unknown shape, before anything else.
    assert_eq!(h.hop(n1, &header(n1, [1; 32], [2; 32], 19)), Err(Error::HeaderUnsupported));
    // No contract, or a contract with other code, at the history address.
    let storage_proof = h.storage.proof(&keccak(&slot_of(n1)));
    let mut rng = Rng::new(3);
    let without =
        Trie::new((0..50).map(|_| (keccak(&rng.bytes(20)).to_vec(), account_rlp(1, 1, keccak(&[0x80]), keccak(b"")))));
    let rec_without =
        HeaderRecord::from_ssz(&record_ssz(1, 1, h.r, 0, [0; 32], without.root(), [0; 32], 0, 0)).unwrap();
    assert_eq!(
        history_hop(&rec_without, &without.proof(&keccak(&HISTORY_STORAGE_ADDRESS)), &storage_proof, h.header(n1)),
        Err(Error::HistoryContractMissing)
    );
    let other_code = contract_state(&h.storage, keccak(b"other code"), 2);
    let rec_other =
        HeaderRecord::from_ssz(&record_ssz(1, 1, h.r, 0, [0; 32], other_code.root(), [0; 32], 0, 0)).unwrap();
    assert_eq!(
        history_hop(&rec_other, &other_code.proof(&keccak(&HISTORY_STORAGE_ADDRESS)), &storage_proof, h.header(n1)),
        Err(Error::HistoryContractMissing)
    );
    // The history contract's account proof under another state root.
    let other_record = World::new(5).record(1, h.r);
    assert_eq!(
        history_hop(&other_record, &h.state.proof(&keccak(&HISTORY_STORAGE_ADDRESS)), &storage_proof, h.header(n1)),
        Err(Error::ProofNodeMismatch)
    );
}
