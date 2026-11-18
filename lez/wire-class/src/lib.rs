//! The attestation program's reply with each statement's **nullifier-safety class**: the wire of
//! the program's second build (`guests/program-class`, not deployed; the deployed program and its
//! image id are unchanged).
//!
//! The second build is the deployed program's own `src/main.rs`, compiled against this crate under
//! the name `ferrata_lez_wire`. Everything is `ferrata_lez_wire`'s, re-exported unchanged, except
//! two names: [`attest`] (the same requests, the same verification, the same statement
//! identifiers, in the same order; the reply adds each statement's class) and
//! [`ATTESTED_SELECTOR`] (the event of a request sent as a transaction's root carries the classes
//! too, so it has a selector of its own).
//!
//! # The classes
//!
//! A statement identifier names a fact, and whether a consumer may spend it once, as a nullifier,
//! depends on what kind of fact it names. The kind is already fixed by the identifier's own
//! derivation (`ferrata_core::attest`, `ferrata_core::accumulator`); the class says it in the reply.
//!
//! | Class | Facts | Identifier over | As a nullifier |
//! |---|---|---|---|
//! | [`Event`](attest::StatementClass::Event) (1) | a log of a transaction that succeeded | chain id, block number, transaction index, the log's position in its receipt, the log's hash | safe when the consumer pins the emitting contract and the event (`docs/design/attestation.md` §7, rule 7): one identifier per emitted log |
//! | [`Leaf`](attest::StatementClass::Leaf) (2) | a leaf of a SHA-256 accumulator | chain id, contract, slot, leaf index, leaf; no root | safe when the consumer pins the contract and the slot that holds the root, and the contract writes each index once |
//! | [`Value`](attest::StatementClass::Value) (3) | an account, a storage slot | chain id, address, slot, the value; no block | not alone |
//!
//! **Event.** A finalized block number, a transaction index and a position in its receipt name one
//! log of the chain: its identifier exists once. Neither the record nor the EIP-2935 hops that
//! reached the block enter it, so another record or chain of hops gives the same identifier
//! (`lez/harness/tests/history_hops.rs` checks this on a real deposit). The attestation proves that a
//! log was emitted, not the emitter's discipline: the consumer pins the emitting contract and event.
//!
//! **Leaf.** The identifier leaves the root out, so one leaf has one identifier under every later
//! root (a nullifier keyed on a root would allow one claim per root), and indices are below 2^32
//! (otherwise `i` and `i + 2^32` would share a path). It covers the contract and the slot, and the
//! attestation proves a leaf under whatever slot the request names: it is one per event only if the
//! consumer pins the contract and the slot that holds the root, and that contract writes each index
//! once (the bridge pins its vault and slot 0; the vault's `deposit` appends at `nextLeafIndex`).
//!
//! **Value.** The identifier names a value, not an event. The default identifier has no block: a
//! value that holds at many blocks, or changes and changes back, has one identifier, so a
//! nullifier on it merges distinct events and refuses the later ones. The block-bound variant
//! (`ferrata_core::attest::storage_id_at_block`) has the block: the same unchanged value read at
//! the next block has a new identifier, so a nullifier on it allows one claim per block, a double
//! claim. A consumer uses a value for conditions ("this balance is at least X"), or spends it
//! together with an event that identifies what happened. A storage layout can make a slot unique in
//! practice (a mapping keyed by a nonce), but that is the contract's knowledge, not the
//! attestation's: the class reports what the attestation itself guarantees.
//!
//! **The bridge's nullifiers.** A deposit is a leaf of the vault's accumulator, and its nullifier is
//! `N = H("ferrata/nf/v1" ‖ s ‖ fid)` with `fid` the leaf's identifier (Leaf class: root left out;
//! the vault a configuration constant, the root's slot 0 a program constant, neither a claimant input)
//! and `s` the depositor's secret, so it is one per deposit and nobody without `s` can compute it
//! (`docs/design/bridge.md` §5.3). The reference consumer spends `H(s ‖ statement id)` of the vault's
//! `Deposit` log (Event class, rule 7 of the Recommended Consumer Pattern). Neither keys on a Value.

pub use ferrata_lez_wire::*;

/// The selector of the event a request sent as a transaction's root emits in the second build:
/// [`attest::Attestation::event_data`]. The deployed program's `FRT-ATT1` event carries no class.
pub const ATTESTED_SELECTOR: [u8; 8] = *b"FRT-ATC1";

/// `ferrata_lez_wire::attest` with the class in the reply.
pub mod attest {
    use borsh::{BorshDeserialize, BorshSerialize};
    use ferrata_core::{witness::HeaderRecord, Root};
    pub use ferrata_lez_wire::attest::{
        AccountQuery, Address, Cmp, Condition, Fact, Field, Hop, LeafQuery, LogQuery, Operand, Relation, Request,
        SlotQuery, Word,
    };
    use ferrata_lez_wire::attest as plain;

    /// Whether a statement identifier may be spent as a nullifier (the crate documentation).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
    #[borsh(use_discriminant = true)]
    #[repr(u8)]
    pub enum StatementClass {
        /// A log: one identifier per emitted log. Safe to nullify when the consumer pins the emitter and event.
        Event = 1,
        /// An accumulator leaf: one identifier per leaf index of a contract that writes each index once.
        /// Safe to nullify when the consumer pins that contract and the slot that holds the root.
        Leaf = 2,
        /// An account or a storage value: names a value, not an event. Not a nullifier alone.
        Value = 3,
    }

    impl StatementClass {
        /// The class of a fact of the reply.
        pub fn of(fact: &Fact) -> Self {
            match fact {
                Fact::Log { .. } => Self::Event,
                Fact::AccumulatorLeaf { .. } => Self::Leaf,
                Fact::Account { .. } | Fact::Storage { .. } => Self::Value,
            }
        }

        /// Whether the identifier is one per event (Event, Leaf) once the consumer pins the emitter and the
        /// event (Event) or the contract and the slot (Leaf): then spending it once spends the event once.
        pub fn is_nullifiable(self) -> bool {
            matches!(self, Self::Event | Self::Leaf)
        }

        /// The class's byte in the reply and in the event.
        pub fn byte(self) -> u8 {
            self as u8
        }
    }

    /// A fact with its statement identifier and its class.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct Statement {
        pub id: Root,
        pub class: StatementClass,
        pub fact: Fact,
    }

    /// The reply: the deployed program's reply, field for field, with a class beside each
    /// identifier.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct Attestation {
        pub instance_id: Root,
        pub chain_id: u64,
        /// The beacon slot of the record the statements rest on.
        pub slot: u64,
        /// The execution block the facts were proven at: the record's, or the last hop's.
        pub block_number: u64,
        pub block_hash: Root,
        /// How many EIP-2935 hops lie between the record and that block.
        pub hops: u32,
        pub statements: Vec<Statement>,
    }

    impl Attestation {
        /// The deployed program's reply with each statement's class; identifiers, facts and their
        /// order unchanged.
        pub fn classify(a: plain::Attestation) -> Self {
            Self {
                instance_id: a.instance_id,
                chain_id: a.chain_id,
                slot: a.slot,
                block_number: a.block_number,
                block_hash: a.block_hash,
                hops: a.hops,
                statements: a
                    .statements
                    .into_iter()
                    .map(|s| Statement { id: s.id, class: StatementClass::of(&s.fact), fact: s.fact })
                    .collect(),
            }
        }

        /// The same reply without the classes, as the deployed program gives it.
        pub fn unclassified(&self) -> plain::Attestation {
            plain::Attestation {
                instance_id: self.instance_id,
                chain_id: self.chain_id,
                slot: self.slot,
                block_number: self.block_number,
                block_hash: self.block_hash,
                hops: self.hops,
                statements: self
                    .statements
                    .iter()
                    .map(|s| plain::Statement { id: s.id, fact: s.fact.clone() })
                    .collect(),
            }
        }

        /// The data of the event emitted for a request sent as a transaction's root
        /// ([`crate::ATTESTED_SELECTOR`]): the instance id, the chain id and the block number
        /// (little-endian), then each statement's class byte and identifier.
        pub fn event_data(&self) -> Vec<u8> {
            let mut data = Vec::with_capacity(48 + 33 * self.statements.len());
            data.extend_from_slice(&self.instance_id);
            data.extend_from_slice(&self.chain_id.to_le_bytes());
            data.extend_from_slice(&self.block_number.to_le_bytes());
            for statement in &self.statements {
                data.push(statement.class.byte());
                data.extend_from_slice(&statement.id);
            }
            data
        }
    }

    /// `ferrata_lez_wire::attest::evaluate`, every check and refusal code as there, with the classes.
    pub fn evaluate(
        instance_id: &Root,
        chain_id: u64,
        recorded: &HeaderRecord,
        request: &Request,
    ) -> Result<Attestation, u8> {
        plain::evaluate(instance_id, chain_id, recorded, request).map(Attestation::classify)
    }
}

#[cfg(test)]
mod tests {
    use super::attest::{Attestation, Fact, StatementClass};
    use ferrata_lez_wire::attest as plain;

    fn reply() -> plain::Attestation {
        let facts = [
            Fact::Account { address: [1; 20], nonce: 1, balance: [2; 32], storage_root: [3; 32], code_hash: [4; 32] },
            Fact::Storage { address: [1; 20], slot: [5; 32], value: [6; 32] },
            Fact::Log { tx_index: 7, log_index: 0, address: [8; 20], topics: vec![[9; 32]], data: vec![10, 11] },
            Fact::AccumulatorLeaf { address: [1; 20], slot: [0; 32], leaf_index: 3, leaf: [12; 32], root: [13; 32] },
        ];
        plain::Attestation {
            instance_id: [0xaa; 32],
            chain_id: 11_155_111,
            slot: 11_556_608,
            block_number: 12_080_009,
            block_hash: [0xbb; 32],
            hops: 4,
            statements: facts
                .into_iter()
                .enumerate()
                .map(|(i, fact)| plain::Statement { id: [i as u8 + 0x10; 32], fact })
                .collect(),
        }
    }

    #[test]
    fn each_fact_has_its_class_and_bytes_one_to_three() {
        let a = Attestation::classify(reply());
        let classes: Vec<StatementClass> = a.statements.iter().map(|s| s.class).collect();
        assert_eq!(classes, [StatementClass::Value, StatementClass::Value, StatementClass::Event, StatementClass::Leaf]);
        assert_eq!(classes.iter().map(|c| c.is_nullifiable()).collect::<Vec<_>>(), [false, false, true, true]);
        for (class, byte) in [(StatementClass::Event, 1u8), (StatementClass::Leaf, 2), (StatementClass::Value, 3)] {
            assert_eq!(borsh::to_vec(&class).unwrap(), [byte], "{class:?} on the wire");
            assert_eq!(class.byte(), byte);
            assert_eq!(borsh::from_slice::<StatementClass>(&[byte]).unwrap(), class);
        }
        assert!(borsh::from_slice::<StatementClass>(&[0]).is_err() && borsh::from_slice::<StatementClass>(&[4]).is_err());
    }

    #[test]
    fn identifiers_facts_and_order_are_the_deployed_programs() {
        let plain_reply = reply();
        let a = Attestation::classify(plain_reply.clone());
        assert_eq!(a.unclassified(), plain_reply);
        // The reply is the deployed one with one byte after each identifier.
        let classed = borsh::to_vec(&a).unwrap();
        assert_eq!(classed.len(), borsh::to_vec(&plain_reply).unwrap().len() + a.statements.len());
        assert_eq!(borsh::from_slice::<Attestation>(&classed).unwrap(), a);
    }

    #[test]
    fn the_event_carries_a_class_byte_before_each_identifier() {
        let a = Attestation::classify(reply());
        let data = a.event_data();
        assert_eq!(&data[..48], &reply().event_data()[..48], "instance, chain id and block as the deployed event");
        assert_eq!(data.len(), 48 + 33 * 4);
        for (i, s) in a.statements.iter().enumerate() {
            assert_eq!(data[48 + 33 * i], s.class.byte());
            assert_eq!(&data[49 + 33 * i..81 + 33 * i], &s.id);
        }
        assert_ne!(super::ATTESTED_SELECTOR, ferrata_lez_wire::ATTESTED_SELECTOR);
    }
}
