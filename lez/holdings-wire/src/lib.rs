//! `ferrata_holdings`: a LEZ program that reads an ERC-20 balance on Ethereum through Ferrata's
//! attestation, written as an outside developer would write one. It is the worked example of
//! `docs/guides/consumer-guide.md`.
//!
//! **The statement.** "`holder` held at least `minimum` of the ERC-20 `token` at the finalized
//! Ethereum block of record `header(id, slot)`." It is a storage-slot statement: the balance is the
//! entry `holder` of the token's `mapping(address => uint256)` declared at storage slot
//! `balances_slot`, whose storage key is `keccak256(pad32(holder) ‖ balances_slot)` (Solidity's
//! layout; WETH9 declares `balanceOf` at slot 3).
//!
//! **What it uses of Ferrata.** Only the attestation's public interface (`ferrata_lez_wire`):
//! `header_actor` to address a record, `Message::Attest` with an `attest::Request` to ask it, and
//! the `attest::Attestation` it replies with. Nothing of the bridge, nothing of the reference
//! consumer.
//!
//! | actor | seed | state |
//! |---|---|---|
//! | `holding(gate, holder)` | `H("…/holding" ‖ gate id ‖ holder)` | [`HoldingsState`] |
//!
//! A **gate** ([`Gate`]) fixes everything the statement is about except the holder and the block:
//! the attestation program it trusts (by account), the instance and the chain id, the token, the
//! slot of its balances mapping and the minimum. Nothing stores a gate: its hash addresses its
//! actors, so a check made under another program, instance, chain, token or minimum lands in
//! another gate's actors.
//!
//! **One transaction.** `Check` at `holding(gate, holder)` builds the request itself (the token's
//! account and the holder's balance slot, with the condition `value ≥ minimum`), so the caller
//! supplies only the Merkle-Patricia proofs of `eth_getProof`. It sends `Attest` to
//! `header(id, slot)` of the gate's program; the reply, accepted only from that actor, must carry
//! the gate's instance and chain id and exactly the two facts asked for, and the balance must be at
//! least the minimum ([`Gate::accept`]). The actor then holds a [`Holding`]: the block number and
//! hash, the balance and the statement identifier. A later check may replace it with a newer block
//! only. A refusal of the attestation program ends the whole transaction with that program's exit
//! code, so nothing is written on a statement that was not proven.

use borsh::{BorshDeserialize, BorshSerialize};
use ferrata_core::{keccak::keccak256, Root};
use ferrata_lez_wire::attest::{
    AccountQuery, Address, Attestation, Cmp, Condition, Fact, Field, Request, SlotQuery, Word,
};
use lee_core::{
    account::{AccountId, Actor},
    program::PdaSeed,
};
use sha2::{Digest, Sha256};

/// The domain of this program's hashes.
const DOMAIN: &[u8] = b"ferrata-holdings/v1/";

fn tagged_hash(tag: &[u8], parts: &[&[u8]]) -> Root {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(tag);
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// The storage key of entry `holder` of a Solidity `mapping(address => …)` declared at
/// `mapping_slot`: `keccak256(pad32(holder) ‖ mapping_slot)`.
pub fn mapping_key(holder: &Address, mapping_slot: &Word) -> Word {
    let mut preimage = [0u8; 64];
    preimage[12..32].copy_from_slice(holder);
    preimage[32..].copy_from_slice(mapping_slot);
    keccak256(&preimage)
}

/// What a holding is checked against. Its hash names every actor of the gate.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Gate {
    /// The attestation program's account. Pinned: the gate only ever asks, and only ever accepts
    /// replies from, `header(id, slot)` of this program.
    pub ferrata: AccountId,
    /// The instance the gate is bound to.
    pub instance_id: Root,
    /// The chain id every statement must carry.
    pub chain_id: u64,
    /// The ERC-20 contract.
    pub token: Address,
    /// The storage slot at which the token declares its balances mapping (3 for WETH9).
    pub balances_slot: Word,
    /// The least balance that passes, in the token's base units (big-endian 256-bit).
    pub minimum: Word,
}

/// A holding the gate accepted: what `holding(gate, holder)` keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Holding {
    pub gate_id: Root,
    pub holder: Address,
    /// The balance proven (at least the gate's minimum).
    pub balance: Word,
    /// The beacon slot of the record the statement rests on.
    pub beacon_slot: u64,
    /// The finalized execution block the balance was proven at.
    pub block_number: u64,
    pub block_hash: Root,
    /// The storage fact's statement identifier (`ferrata_core::attest`).
    pub statement_id: Root,
}

/// A check in progress, between `Check` and the attestation's reply (within one transaction).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Pending {
    pub gate: Gate,
    pub holder: Address,
    /// The record asked: the only actor whose reply is accepted.
    pub record: Actor,
    /// The block of the holding this check would replace, if any.
    pub previous_block: Option<u64>,
}

/// The state of `holding(gate, holder)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum HoldingsState {
    Pending(Pending),
    Held(Holding),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum HoldingsMessage {
    /// At `holding(gate, holder)`: prove, against `header(id, slot)` of the gate's program, that
    /// `holder` held at least the gate's minimum of its token. `account_proof` is
    /// `eth_getProof(token, [key]).accountProof` and `storage_proof` its `storageProof[0].proof`,
    /// where `key = mapping_key(holder, balances_slot)`, at the record's block. `header` is that
    /// block's RLP header for a record that binds a block hash only (Gloas layout), else `None`.
    Check {
        gate: Gate,
        slot: u64,
        holder: Address,
        account_proof: Vec<Vec<u8>>,
        storage_proof: Vec<Vec<u8>>,
        header: Option<Vec<u8>>,
    },
}

/// Exit codes of this program's own refusals: 230–239. A refusal of the attestation program ends
/// the transaction with that program's code (1–219, `ferrata_lez_wire::CODE_BLOCKS`), so these
/// must not collide with them; 230–239 are also unused by the reference consumer (220–229) and the
/// bridge program (240–255), so no code of a Ferrata program means two things.
pub mod code {
    /// The message does not decode.
    pub const BAD_MESSAGE: u8 = 230;
    /// Stored state that does not decode.
    pub const BAD_STATE: u8 = 231;
    /// `Check` at another actor than `holding(gate, holder)`.
    pub const WRONG_RECEIVER: u8 = 232;
    /// While a check waits: a message from another actor than the record it asked.
    pub const WRONG_SENDER: u8 = 233;
    /// A reply that is not an attestation.
    pub const BAD_REPLY: u8 = 234;
    /// An attestation of another instance than the gate's.
    pub const WRONG_INSTANCE: u8 = 235;
    /// An attestation for another chain than the gate's.
    pub const WRONG_CHAIN: u8 = 236;
    /// An attestation whose facts are not the two asked for (the token's account, then the
    /// holder's balance slot), or proven at an older block than the record's.
    pub const WRONG_FACTS: u8 = 237;
    /// A balance below the gate's minimum.
    pub const BELOW_MINIMUM: u8 = 238;
    /// A check at a block no newer than the holding it would replace.
    pub const STALE: u8 = 239;
}

impl Gate {
    /// `H("…/gate" ‖ borsh(gate))`.
    pub fn id(&self) -> Root {
        tagged_hash(b"gate", &[&borsh::to_vec(self).expect("borsh serialization is infallible")])
    }

    /// The storage key of `holder`'s balance in the token.
    pub fn balance_key(&self, holder: &Address) -> Word {
        mapping_key(holder, &self.balances_slot)
    }

    /// The request `Check` sends: the token's account, and under it the holder's balance slot
    /// with the condition `value ≥ minimum`. The record's own block, no hops.
    pub fn request(
        &self,
        holder: &Address,
        account_proof: Vec<Vec<u8>>,
        storage_proof: Vec<Vec<u8>>,
        header: Option<Vec<u8>>,
    ) -> Request {
        Request {
            instance_id: self.instance_id,
            chain_id: self.chain_id,
            header,
            hops: vec![],
            accounts: vec![AccountQuery {
                address: self.token,
                proof: account_proof,
                conditions: vec![],
                slots: vec![SlotQuery {
                    slot: self.balance_key(holder),
                    proof: storage_proof,
                    conditions: vec![Condition { field: Field::Value, cmp: Cmp::Ge, rhs: self.minimum }],
                }],
            }],
            logs: vec![],
            leaves: vec![],
            relations: vec![],
        }
    }

    /// Accept the reply to [`Gate::request`] for `holder`: the gate's instance and chain, the
    /// record's own block, exactly the token's account and then the holder's balance slot, and a
    /// balance of at least the minimum (the attestation program checked the condition; it is
    /// checked again here, so this function alone states what the program accepts).
    pub fn accept(&self, holder: &Address, attestation: &Attestation) -> Result<Holding, u8> {
        if attestation.instance_id != self.instance_id {
            return Err(code::WRONG_INSTANCE);
        }
        if attestation.chain_id != self.chain_id {
            return Err(code::WRONG_CHAIN);
        }
        if attestation.hops != 0 {
            return Err(code::WRONG_FACTS);
        }
        let [account, storage] = attestation.statements.as_slice() else {
            return Err(code::WRONG_FACTS);
        };
        match account.fact {
            Fact::Account { address, .. } if address == self.token => {}
            _ => return Err(code::WRONG_FACTS),
        }
        let Fact::Storage { address, slot, value } = storage.fact else {
            return Err(code::WRONG_FACTS);
        };
        if address != self.token || slot != self.balance_key(holder) {
            return Err(code::WRONG_FACTS);
        }
        // Big-endian words of equal length: byte order is numeric order.
        if value < self.minimum {
            return Err(code::BELOW_MINIMUM);
        }
        Ok(Holding {
            gate_id: self.id(),
            holder: *holder,
            balance: value,
            beacon_slot: attestation.slot,
            block_number: attestation.block_number,
            block_hash: attestation.block_hash,
            statement_id: storage.id,
        })
    }
}

pub fn holding_seed(gate_id: &Root, holder: &Address) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"holding", &[gate_id, holder]))
}

/// `holding(gate, holder)` of the program deployed at `program`.
pub fn holding_actor(program: AccountId, gate_id: &Root, holder: &Address) -> Actor {
    Actor::new(AccountId::for_public_pda(&program, &holding_seed(gate_id, holder)), program)
}
