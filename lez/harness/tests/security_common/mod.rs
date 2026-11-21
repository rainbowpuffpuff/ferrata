//! Shared by `security_lc.rs` and `security_privacy.rs`: a light-client chain whose committees the
//! test holds, with execution blocks built from real Merkle-Patricia tries under its headers (so
//! that the attestation program can prove facts of them), EIP-2935 hop chains between such blocks,
//! vault `Deposit` and ERC-20 `Transfer` receipts, the names and messages of the exit codes (from
//! the IDL), and the results directory `lez/results/lc-security/`.
#![allow(dead_code)]

use std::{collections::BTreeMap, fs, path::PathBuf};

use ferrata_core::{
    attest::{HISTORY_CODE_HASH, HISTORY_SERVE_WINDOW, HISTORY_STORAGE_ADDRESS},
    rlp::encode_u64,
    types::{ExecutionPayloadHeader, LightClientHeader, LightClientUpdate},
    Root,
};
use ferrata_lez_wire::{
    attest::{Address, Hop, Word},
    consumer::DEPOSIT_TOPIC,
    Config,
};
use ferrata_spectests::{
    execkit::{account_rlp, enc, header, keccak, log_rlp, receipt_with, Trie, TRANSFER},
    testkit::{fabricated_bootstrap, fake_committee, filler, header_with_payload, update_over, FakeCommittee},
};
use harness::{scenarios::World, TxReport};

/// Slots of one sync-committee period on the mainnet preset.
pub const PERIOD: u64 = 8192;

// ---------------------------------------------------------------------------------------------
// Results and code names
// ---------------------------------------------------------------------------------------------

/// `lez/results/lc-security/<name>` (created), under the harness's results directory for the tag.
pub fn results_path(name: &str) -> PathBuf {
    let dir = harness::results_dir().join("lc-security");
    fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

pub fn write(name: &str, text: &str) {
    fs::write(results_path(name), text).unwrap();
}

/// The attestation program's IDL: code -> (name, message).
pub fn idl_codes() -> BTreeMap<u32, (String, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../idl/ferrata_attestation.idl.json");
    let idl: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    idl["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["code"].as_u64().unwrap() as u32,
                (e["name"].as_str().unwrap().to_string(), e["msg"].as_str().unwrap_or("").to_string()),
            )
        })
        .collect()
}

/// A code with its name, from the IDL where it is the attestation program's, else the harness's.
pub fn named(code: u32) -> String {
    match idl_codes().get(&code) {
        Some((name, _)) => format!("{code} {name}"),
        None => format!("{code} {}", harness::exit_name(code)),
    }
}

/// What a sequence of transactions did: applied, refused with a code at a step, or refused by LEZ
/// itself (no code) at a step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Applied,
    Code { code: u32, step: String },
    Lez { revert: String, step: String },
}

impl Verdict {
    pub fn of(reports: &[TxReport]) -> Self {
        match reports.iter().find(|r| !r.ok()) {
            None => Verdict::Applied,
            Some(r) => match r.exit_code {
                Some(code) => Verdict::Code { code, step: r.label.clone() },
                None => Verdict::Lez { revert: r.charge.revert.clone().unwrap_or_default(), step: r.label.clone() },
            },
        }
    }

    pub fn code(&self) -> Option<u32> {
        match self {
            Verdict::Code { code, .. } => Some(*code),
            _ => None,
        }
    }

    pub fn show(&self) -> String {
        match self {
            Verdict::Applied => "applied".into(),
            Verdict::Code { code, step } => format!("refused, exit {} (at `{step}`)", named(*code)),
            Verdict::Lez { revert, step } => format!("refused by LEZ: \"{revert}\" (at `{step}`)"),
        }
    }
}

/// One row of a results table.
pub struct Row {
    pub case: String,
    pub expected: String,
    pub verdict: Verdict,
    pub note: String,
}

pub fn table(rows: &[Row]) -> String {
    let mut out = String::from("| Case | Expected | Observed through the program | Note |\n|---|---|---|---|\n");
    for r in rows {
        out.push_str(&format!("| {} | {} | {} | {} |\n", r.case, r.expected, r.verdict.show(), r.note));
    }
    out
}

/// A 256-bit word from a small integer.
pub fn w(n: u128) -> Word {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&n.to_be_bytes());
    out
}

/// An address left-padded to a word.
pub fn pad(a: &Address) -> Word {
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(a);
    out
}

// ---------------------------------------------------------------------------------------------
// Execution blocks with real tries
// ---------------------------------------------------------------------------------------------

/// An account of a synthetic block.
#[derive(Clone)]
pub struct AccountSpec {
    pub address: Address,
    pub nonce: u64,
    pub balance: u128,
    pub code_hash: Root,
    /// (slot, value) pairs; values must be nonzero (a zero slot is absent).
    pub storage: Vec<(Word, Word)>,
}

impl AccountSpec {
    pub fn eoa(address: Address, balance: u128) -> Self {
        Self { address, nonce: 1, balance, code_hash: keccak(b""), storage: vec![] }
    }

    pub fn contract(address: Address, storage: Vec<(Word, Word)>) -> Self {
        Self { address, nonce: 1, balance: 0, code_hash: keccak(b"code"), storage }
    }

    /// The EIP-2935 history contract holding `blocks`' hashes at their slots.
    pub fn history(blocks: &[(u64, Root)]) -> Self {
        Self {
            address: HISTORY_STORAGE_ADDRESS,
            nonce: 1,
            balance: 0,
            code_hash: HISTORY_CODE_HASH,
            storage: blocks.iter().map(|(n, hash)| (history_slot(*n), *hash)).collect(),
        }
    }
}

/// The history contract's slot of block `n`.
pub fn history_slot(n: u64) -> Word {
    w(u128::from(n % HISTORY_SERVE_WINDOW))
}

/// A block: its number, a state trie of accounts (with storage tries) and a receipts trie. Its
/// header is the RLP of a Prague header (21 fields) over these roots.
pub struct Block {
    pub number: u64,
    pub state: Trie,
    pub storage: BTreeMap<Address, Trie>,
    pub receipts: Trie,
}

impl Block {
    pub fn new(number: u64, accounts: &[AccountSpec], receipts: &[Vec<u8>]) -> Self {
        let mut storage = BTreeMap::new();
        let mut leaves = Vec::new();
        for a in accounts {
            let storage_root = if a.storage.is_empty() {
                keccak(&[0x80])
            } else {
                let t = Trie::new(a.storage.iter().map(|(k, v)| (keccak(k).to_vec(), enc::uint(v))));
                let r = t.root();
                storage.insert(a.address, t);
                r
            };
            leaves.push((keccak(&a.address).to_vec(), account_rlp(a.nonce, a.balance, storage_root, a.code_hash)));
        }
        let receipts = Trie::new(receipts.iter().enumerate().map(|(i, v)| {
            let (k, l) = encode_u64(i as u64);
            (k[..l].to_vec(), v.clone())
        }));
        Self { number, state: Trie::new(leaves), storage, receipts }
    }

    pub fn account_proof(&self, a: &Address) -> Vec<Vec<u8>> {
        self.state.proof(&keccak(a))
    }

    pub fn storage_proof(&self, a: &Address, slot: &Word) -> Vec<Vec<u8>> {
        self.storage[a].proof(&keccak(slot))
    }

    pub fn receipt_proof(&self, i: u64) -> Vec<Vec<u8>> {
        let (k, l) = encode_u64(i);
        self.receipts.proof(&k[..l])
    }

    pub fn header_rlp(&self) -> Vec<u8> {
        header(self.number, self.state.root(), self.receipts.root(), 21)
    }

    pub fn hash(&self) -> Root {
        keccak(&self.header_rlp())
    }

    /// The execution payload header a beacon block carrying this block holds.
    pub fn payload(&self) -> ExecutionPayloadHeader {
        let mut p = ExecutionPayloadHeader::empty();
        p.block_number = self.number;
        p.block_hash = self.hash();
        p.state_root = self.state.root();
        p.receipts_root = self.receipts.root();
        p.timestamp = 1_700_000_000 + 12 * self.number;
        p
    }

    /// The EIP-2935 hop from this block to `older`, whose hash this block's history contract holds.
    pub fn hop_to(&self, older: &Block) -> Hop {
        Hop {
            account_proof: self.account_proof(&HISTORY_STORAGE_ADDRESS),
            storage_proof: self.storage_proof(&HISTORY_STORAGE_ADDRESS, &history_slot(older.number)),
            header: older.header_rlp(),
        }
    }
}

/// A successful receipt with one ERC-20 `Transfer(from, to, amount)` of `token`.
pub fn transfer_receipt(token: Address, from: Address, to: Address, amount: u128) -> Vec<u8> {
    receipt_with(2, enc::u64(1), 52_000, &[log_rlp(token, &[TRANSFER, pad(&from), pad(&to)], &w(amount))])
}

/// A successful receipt with one vault `Deposit(depositor, index, leafIndex, token, amount, C)`.
pub fn deposit_receipt(
    vault: Address,
    depositor: Address,
    token: Address,
    leaf_index: u64,
    amount: u128,
    commitment: Root,
) -> Vec<u8> {
    let data = [w(u128::from(leaf_index)), w(u128::from(leaf_index)), w(amount), commitment].concat();
    receipt_with(2, enc::u64(1), 61_000, &[log_rlp(vault, &[DEPOSIT_TOPIC, pad(&depositor), pad(&token)], &data)])
}

/// Filler receipts (a plain transfer of value, no logs) to put a receipt at another index.
pub fn plain_receipt(i: u64) -> Vec<u8> {
    receipt_with(2, enc::u64(1), 21_000 * (i + 1), &[])
}

// ---------------------------------------------------------------------------------------------
// A light-client chain whose committees the test holds
// ---------------------------------------------------------------------------------------------

/// An instance of the program (the mainnet configuration unless given) initialised from a
/// fabricated checkpoint at `slot0`, whose committee for each period from `slot0`'s on the test
/// holds (`committees[k]` is period `p0 + k`'s). Every update goes through LEZ's settlement next
/// to the native store (`World::apply`), so the program and the specification's store must agree.
pub struct Synth {
    pub w: World,
    pub committees: Vec<FakeCommittee>,
    pub p0: u64,
}

impl Synth {
    pub fn new(config: Config, seed: u8, slot0: u64, checkpoint: LightClientHeader, periods: usize) -> Self {
        assert_eq!(checkpoint.beacon.slot, slot0);
        let committees: Vec<FakeCommittee> =
            (0..periods).map(|k| fake_committee(512, seed.wrapping_add(k as u8))).collect();
        let bootstrap = fabricated_bootstrap(checkpoint, &committees[0], 9_100 + u64::from(seed));
        let root = bootstrap.header.beacon.hash_tree_root();
        let w = World::from_parts(config, root, bootstrap, vec![]);
        Self { w, committees, p0: slot0 / PERIOD }
    }

    /// The first slot of period `p0 + k`.
    pub fn start(&self, k: u64) -> u64 {
        (self.p0 + k) * PERIOD
    }

    /// An update signed by the first `signers` members of committee `signer`, at `signature_slot`.
    pub fn update(
        &self,
        signer: usize,
        finalized: Option<LightClientHeader>,
        attested: LightClientHeader,
        next: Option<usize>,
        signature_slot: u64,
        signers: usize,
    ) -> LightClientUpdate {
        update_over(
            finalized,
            attested,
            next.map(|k| &self.committees[k].committee),
            signature_slot,
            signers,
            &self.committees[signer],
            &self.w.ctx(),
        )
    }

    /// The sequencer time a submitter uses for `update`: 12 s into its signature slot.
    pub fn time(&self, update: &LightClientUpdate) -> u64 {
        self.w.inst.config.slot_start_ms(update.signature_slot) + 12_000
    }
}

/// A header at `slot` carrying `block`'s payload.
pub fn header_of(slot: u64, block: &Block) -> LightClientHeader {
    header_with_payload(slot, 50_000 + slot, block.payload())
}

/// A header at `slot` whose payload is filler (no state anyone proves facts of).
pub fn filler_header(slot: u64) -> LightClientHeader {
    let mut p = ExecutionPayloadHeader::empty();
    p.block_number = 1_000_000 + slot;
    p.block_hash = filler(slot);
    p.state_root = filler(slot + 1);
    p.receipts_root = filler(slot + 2);
    p.timestamp = 1_700_000_000 + 6 * slot;
    header_with_payload(slot, 60_000 + slot, p)
}
