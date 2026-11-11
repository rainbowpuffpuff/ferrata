//! The gas drop's receipt (bridge.md §6; v3 specification, D13).
//!
//! A privacy-preserving claim on LEZ v0.4 pays its native fee from the prover's own private account,
//! so a first claim needs native LEZ. The claimant shows a relayer a small receipt proving that some
//! deposit under an imported root has nullifier `N`; the relayer sends one native credit per `N` to a
//! one-time address, which the claim sweeps, and the claim pays the relayer the fixed relay fee in the
//! wrapped token. The receipt's journal binds `(N, oneTimeAddress, relayer)`, so it cannot be replayed
//! to another relayer or redirected to another address, and a relayer refuses a second drop for an `N`.
//!
//! The statement is the claim's own ([`verify_claim`]: the opening of `C`, the leaf binding the
//! depositor, the path with direction bits equal to `leafIndex`, `fid`, `N`), with the two bound
//! addresses copied into the journal. The relayer learns `N` (the claim publishes it anyway), the
//! root, a time and the one-time address; never which deposit, its depositor, token or amount.

use crate::encoding::{ClaimInput, CLAIM_INPUT_LEN};
use crate::inbound::verify_claim;
use crate::{sha256, Address, Hash, VerifyError, U256};

/// Length of [`GasDropInput`]'s encoding: the claim measurement guest's 1260 bytes, then the two
/// bound addresses.
pub const GAS_DROP_INPUT_LEN: usize = CLAIM_INPUT_LEN + 64;
/// Length of [`GasDropJournal`]'s encoding.
pub const GAS_DROP_JOURNAL_LEN: usize = 212;

/// `DROP_DOMAIN = sha256("ferrata/gas-drop/v1")`: the journal's first word, so that no other
/// statement's journal can be read as a gas-drop receipt.
pub fn drop_domain() -> Hash {
    sha256(b"ferrata/gas-drop/v1")
}

/// The gas-drop guest's input (not a protocol encoding: private to the claimant's prover), 1324
/// bytes: [`ClaimInput`] (1260) ‖ `oneTimeAddress` (32, a LEZ account id) ‖ `relayer` (32, the LEZ
/// account id of the relayer the receipt is for).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GasDropInput {
    /// The claim witness under an imported root, with the configuration's chain and vault.
    pub claim: ClaimInput,
    /// The account the native credit goes to; the claim sweeps it into its fee payer.
    pub one_time_address: Hash,
    /// The relayer the receipt is shown to, and the claim names.
    pub relayer: Hash,
}

impl GasDropInput {
    /// The 1324 input bytes.
    pub fn encode(&self) -> [u8; GAS_DROP_INPUT_LEN] {
        let mut out = [0u8; GAS_DROP_INPUT_LEN];
        out[..CLAIM_INPUT_LEN].copy_from_slice(&self.claim.encode());
        out[CLAIM_INPUT_LEN..CLAIM_INPUT_LEN + 32].copy_from_slice(&self.one_time_address);
        out[CLAIM_INPUT_LEN + 32..].copy_from_slice(&self.relayer);
        out
    }

    /// Parses the 1324 input bytes (panics where [`ClaimInput::decode`] does).
    pub fn decode(bytes: &[u8; GAS_DROP_INPUT_LEN]) -> Self {
        let mut claim = [0u8; CLAIM_INPUT_LEN];
        claim.copy_from_slice(&bytes[..CLAIM_INPUT_LEN]);
        let mut one_time_address = [0u8; 32];
        one_time_address.copy_from_slice(&bytes[CLAIM_INPUT_LEN..CLAIM_INPUT_LEN + 32]);
        let mut relayer = [0u8; 32];
        relayer.copy_from_slice(&bytes[CLAIM_INPUT_LEN + 32..]);
        Self { claim: ClaimInput::decode(&claim), one_time_address, relayer }
    }
}

/// What the gas-drop guest commits, 212 bytes:
/// `DROP_DOMAIN`(32) ‖ `root`(32) ‖ `evmChainId`(32) ‖ `vault`(20) ‖ `N`(32) ‖ `oneTimeAddress`(32) ‖
/// `relayer`(32).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GasDropJournal {
    /// The deposit root the witness is under; the relayer checks it is an imported root.
    pub root: Hash,
    /// The vault's chain.
    pub chain_id: U256,
    /// The vault.
    pub vault: Address,
    /// The deposit's nullifier: one drop per `N`.
    pub nullifier: Hash,
    /// Where the credit goes.
    pub one_time_address: Hash,
    /// The only relayer that may act on the receipt.
    pub relayer: Hash,
}

impl GasDropJournal {
    /// The 212 journal bytes.
    pub fn encode(&self) -> [u8; GAS_DROP_JOURNAL_LEN] {
        let mut out = [0u8; GAS_DROP_JOURNAL_LEN];
        let mut at = 0;
        for part in [
            &drop_domain()[..],
            &self.root[..],
            &self.chain_id[..],
            &self.vault[..],
            &self.nullifier[..],
            &self.one_time_address[..],
            &self.relayer[..],
        ] {
            out[at..at + part.len()].copy_from_slice(part);
            at += part.len();
        }
        debug_assert_eq!(at, GAS_DROP_JOURNAL_LEN);
        out
    }

    /// Parses a journal: `None` unless it is exactly 212 bytes starting with `DROP_DOMAIN`.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != GAS_DROP_JOURNAL_LEN || bytes[..32] != drop_domain() {
            return None;
        }
        let word = |at: usize| -> Hash {
            let mut h = [0u8; 32];
            h.copy_from_slice(&bytes[at..at + 32]);
            h
        };
        let mut vault = [0u8; 20];
        vault.copy_from_slice(&bytes[96..116]);
        Some(Self {
            root: word(32),
            chain_id: word(64),
            vault,
            nullifier: word(116),
            one_time_address: word(148),
            relayer: word(180),
        })
    }
}

/// The gas-drop statement: the claim statement over `input.claim`, and the journal binding its `N`
/// to the one-time address and the relayer.
pub fn verify_gas_drop(input: &GasDropInput) -> Result<GasDropJournal, VerifyError> {
    let c = &input.claim;
    let nullifier = verify_claim(&c.root, &c.witness, &c.chain_id, &c.vault)?;
    Ok(GasDropJournal {
        root: c.root,
        chain_id: c.chain_id,
        vault: c.vault,
        nullifier,
        one_time_address: input.one_time_address,
        relayer: input.relayer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbound::{commitment, deposit_id, deposit_leaf, nullifier, ClaimWitness, DestKind};
    use crate::tree::{root_from_path, MerklePath};

    fn h(tag: &str) -> Hash {
        sha256(tag.as_bytes())
    }

    fn input() -> GasDropInput {
        let (s, dest) = (h("s"), h("dest"));
        let (depositor, token, amount) = ([7u8; 20], [0u8; 20], h("amount"));
        let leaf_index = 5u64;
        let leaf = deposit_leaf(leaf_index, &depositor, &token, &amount, &commitment(&s, DestKind::Private, &dest));
        let mut siblings = [[0u8; 32]; 32];
        for (i, sib) in siblings.iter_mut().enumerate() {
            *sib = h(&format!("sibling {i}"));
        }
        let path = MerklePath { directions: leaf_index as u32, siblings };
        GasDropInput {
            claim: ClaimInput {
                root: root_from_path(&leaf, &path),
                chain_id: h("chain"),
                vault: [9u8; 20],
                witness: ClaimWitness { depositor, token, amount, leaf_index, path, s, kind: DestKind::Private, dest },
            },
            one_time_address: h("one-time address"),
            relayer: h("relayer"),
        }
    }

    #[test]
    fn journal_binds_n_one_time_address_and_relayer() {
        let i = input();
        let j = verify_gas_drop(&i).expect("valid witness");
        let w = &i.claim.witness;
        let leaf = deposit_leaf(w.leaf_index, &w.depositor, &w.token, &w.amount, &commitment(&w.s, w.kind, &w.dest));
        assert_eq!(j.nullifier, nullifier(&w.s, &deposit_id(&i.claim.chain_id, &i.claim.vault, w.leaf_index, &leaf)));
        assert_eq!((j.one_time_address, j.relayer, j.root), (i.one_time_address, i.relayer, i.claim.root));
        let bytes = j.encode();
        assert_eq!(GasDropJournal::decode(&bytes), Some(j));
        // Another relayer or another address is another journal: the receipt cannot be replayed.
        let mut other = i.clone();
        other.relayer = h("other relayer");
        assert_ne!(verify_gas_drop(&other).unwrap().encode(), bytes);
        other = i.clone();
        other.one_time_address = h("other address");
        assert_ne!(verify_gas_drop(&other).unwrap().encode(), bytes);
    }

    #[test]
    fn wrong_witness_or_foreign_journal_is_refused() {
        let mut bad = input();
        bad.claim.witness.s[0] ^= 1;
        assert_eq!(verify_gas_drop(&bad), Err(VerifyError::RootMismatch));
        let mut bad = input();
        bad.claim.witness.path.directions ^= 1;
        assert_eq!(verify_gas_drop(&bad), Err(VerifyError::DirectionsMismatch));
        let mut bytes = verify_gas_drop(&input()).unwrap().encode();
        assert!(GasDropJournal::decode(&bytes[..211]).is_none());
        bytes[0] ^= 1;
        assert!(GasDropJournal::decode(&bytes).is_none(), "a journal without the domain is not a gas-drop receipt");
    }

    #[test]
    fn input_round_trips() {
        let i = input();
        assert_eq!(GasDropInput::decode(&i.encode()), i);
    }
}
