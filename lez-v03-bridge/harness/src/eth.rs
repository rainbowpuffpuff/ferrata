//! Execution blocks holding a vault's storage, and the light client's records of them.
//!
//! * [`Block`]: a synthetic block whose state trie holds the vault with exactly the slots given
//!   (among 40 other accounts), under a record in the payload layout or, with
//!   [`Block::gloas`], in the Gloas layout (a header whose hash the record binds, carried in front
//!   of the account proof). The arrangement of the v0.4 harness (`lez/BRIDGE-RESULTS.md` §8a).
//! * [`Captured`]: a block captured from a node: its RLP header and an `eth_getProof` answer for
//!   the vault (Sepolia's, recorded by the crown run under `e2e/runs/crown-live-3/captures`, or a
//!   local anvil's). Its record binds the header's hash (Gloas layout), as the light client's
//!   record of Sepolia's finalized block does.

use std::path::Path;

use ferrata_core::{
    types::BeaconBlockHeader,
    witness::{ExecutionSummary, HeaderRecord},
};
use ferrata_lez03_bridge_core::{with_header, ImportProof, PairingProof, Word, SLOT_CONFIG_ID, SLOT_COUNT, SLOT_ROOT};
use ferrata_spectests::execkit::{account_proof, account_rlp, enc, header, keccak, AccountProof, Trie};

/// A synthetic execution block and its record.
pub struct Block {
    pub record: HeaderRecord,
    pub state: Trie,
    pub vault: [u8; 20],
    pub storage: Trie,
    pub header: Option<Vec<u8>>,
}

fn state_with(vault: [u8; 20], slots: &[(Word, Word)]) -> (Trie, Trie) {
    let storage =
        Trie::new(slots.iter().filter(|(_, v)| *v != [0u8; 32]).map(|(k, v)| (keccak(k).to_vec(), enc::uint(v))));
    let mut accounts = vec![(vault, account_rlp(1, 0, storage.root(), keccak(b"vault code")))];
    for i in 0x40..0x68u8 {
        accounts.push(([i; 20], account_rlp(u64::from(i), 1_000 + u128::from(i), keccak(&[0x80]), keccak(b""))));
    }
    (Trie::new(accounts.iter().map(|(a, rlp)| (keccak(a).to_vec(), rlp.clone()))), storage)
}

/// The vault's slots 0, 1, 2: the root, the count, the configuration id.
pub fn vault_slots(root: [u8; 32], count: u64, config_id: [u8; 32]) -> Vec<(Word, Word)> {
    vec![(SLOT_ROOT, root), (SLOT_COUNT, ferrata_lez03_bridge_core::word_of(count)), (SLOT_CONFIG_ID, config_id)]
}

impl Block {
    /// Block `number` at beacon slot `slot`, holding the vault with `slots`, under a payload record.
    pub fn new(slot: u64, number: u64, vault: [u8; 20], slots: &[(Word, Word)]) -> Self {
        let (state, storage) = state_with(vault, slots);
        let record = HeaderRecord {
            beacon: BeaconBlockHeader { slot, ..Default::default() },
            execution_root: [3; 32],
            execution: ExecutionSummary::Payload {
                block_number: number,
                block_hash: keccak(&number.to_be_bytes()),
                parent_hash: keccak(&(number - 1).to_be_bytes()),
                state_root: state.root(),
                receipts_root: [7; 32],
                timestamp: 1_700_000_000 + 12 * number,
            },
        };
        Self { record, state, vault, storage, header: None }
    }

    /// The same, under a record in the Gloas layout: it binds the hash of a header (21 fields,
    /// Prague) whose state root is the block's; the proofs carry the header.
    pub fn gloas(slot: u64, number: u64, vault: [u8; 20], slots: &[(Word, Word)]) -> Self {
        let (state, storage) = state_with(vault, slots);
        let rlp = header(number, state.root(), [7; 32], 21);
        let record = HeaderRecord {
            beacon: BeaconBlockHeader { slot, ..Default::default() },
            execution_root: [0; 32],
            execution: ExecutionSummary::BlockHash(keccak(&rlp)),
        };
        Self { record, state, vault, storage, header: Some(rlp) }
    }

    fn account(&self) -> Vec<Vec<u8>> {
        let nodes = self.state.proof(&keccak(&self.vault));
        match &self.header {
            Some(h) => with_header(h.clone(), nodes),
            None => nodes,
        }
    }

    pub fn import_proof(&self) -> ImportProof {
        ImportProof {
            account: self.account(),
            root_slot: self.storage.proof(&keccak(&SLOT_ROOT)),
            count_slot: self.storage.proof(&keccak(&SLOT_COUNT)),
        }
    }

    pub fn pairing_proof(&self) -> PairingProof {
        PairingProof { account: self.account(), config_slot: self.storage.proof(&keccak(&SLOT_CONFIG_ID)) }
    }

    /// The pairing proof without the header (a Gloas record then refuses: 215).
    pub fn pairing_proof_without_header(&self) -> PairingProof {
        PairingProof {
            account: self.state.proof(&keccak(&self.vault)),
            config_slot: self.storage.proof(&keccak(&SLOT_CONFIG_ID)),
        }
    }
}

/// A block captured from a node: the RLP header (whose hash the record binds) and the vault's
/// `eth_getProof` answer at that block.
pub struct Captured {
    pub header: Vec<u8>,
    pub proof: AccountProof,
    pub record: HeaderRecord,
}

impl Captured {
    /// `header.rlp` and the `eth_getProof` answer `proof_file` of `dir`, recorded under the
    /// light client's record of beacon slot `slot`.
    pub fn load(dir: &Path, proof_file: &str, slot: u64) -> Self {
        let header = std::fs::read(dir.join("header.rlp")).expect("header.rlp");
        let proof = account_proof(&std::fs::read(dir.join(proof_file)).expect("the eth_getProof answer"));
        let record = HeaderRecord {
            beacon: BeaconBlockHeader { slot, ..Default::default() },
            execution_root: [0; 32],
            execution: ExecutionSummary::BlockHash(keccak(&header)),
        };
        Self { header, proof, record }
    }

    fn storage_proof(&self, slot: &Word) -> Vec<Vec<u8>> {
        self.proof.storage.iter().find(|s| s.key == *slot).expect("the slot was captured").proof.clone()
    }

    /// The captured value of `slot`.
    pub fn value(&self, slot: &Word) -> Word {
        self.proof.storage.iter().find(|s| s.key == *slot).expect("the slot was captured").value
    }

    fn account(&self) -> Vec<Vec<u8>> {
        with_header(self.header.clone(), self.proof.proof.clone())
    }

    pub fn import_proof(&self) -> ImportProof {
        ImportProof {
            account: self.account(),
            root_slot: self.storage_proof(&SLOT_ROOT),
            count_slot: self.storage_proof(&SLOT_COUNT),
        }
    }

    pub fn pairing_proof(&self) -> PairingProof {
        PairingProof { account: self.account(), config_slot: self.storage_proof(&SLOT_CONFIG_ID) }
    }
}
