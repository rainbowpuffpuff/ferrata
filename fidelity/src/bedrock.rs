//! Logos Blockchain transactions built with `logos-blockchain-core` at `d480efc6` (the revision
//! LEZ v0.4.0-rc1 pins): block inscriptions as LEZ's sequencer makes them, and channel
//! configuration changes.

use logos_blockchain_binary_codec::canonical::BinaryEncode as _;
use logos_blockchain_core::mantle::{
    channel::{SlotTimeframe, SlotTimeout},
    ledger::{BoundedInputs, Inputs, Outputs},
    ops::{
        channel::{
            channel_transfer::ChannelTransferOp, config::ChannelConfigOp, inscribe::InscriptionOp,
            withdraw::ChannelWithdrawOp, ChannelId, MsgId, VerifiedChannelKeys,
        },
        transfer::TransferOp,
    },
    traits::Hashable as _,
    transactions::Ops,
    Note, NoteId, Op,
};
use logos_blockchain_groth16::Fr;
use logos_blockchain_key_management_system_keys::keys::{Ed25519Key, ZkPublicKey};

pub type Hash = [u8; 32];

/// A sequencer's channel signing key.
pub struct Sequencer(pub Ed25519Key);

impl Sequencer {
    pub fn from_seed(seed: u64) -> Self {
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&seed.to_le_bytes());
        bytes[8..16].copy_from_slice(b"ferrata!");
        Self(Ed25519Key::from_bytes(&bytes))
    }

    pub fn public(&self) -> Hash {
        self.0.public_key().to_bytes()
    }

    fn sign(&self, tx_hash: &Hash) -> [u8; 64] {
        self.0.sign_payload(tx_hash).to_bytes()
    }
}

/// A signed Mantle transaction (encoded `Ops`), with what upstream computes about it.
#[derive(Clone, Debug)]
pub struct SignedTx {
    pub ops: Vec<u8>,
    /// Position of the op of interest (the inscription, or the configuration op).
    pub op_index: usize,
    /// The op of interest's signature (inscription) over `tx_hash`.
    pub signature: [u8; 64],
    pub tx_hash: Hash,
    /// `InscriptionOp::id()` / `ChannelConfigOp::id()`.
    pub op_id: Hash,
}

/// The fee transfer a node wallet appends when it funds a transaction: one funding note in, one
/// change note out (`fund_ops`, zone-sdk `tx_builder.rs`).
fn fee_transfer(salt: u64) -> TransferOp {
    let input = NoteId(Fr::from(1_000_000 + salt));
    TransferOp::new(
        Inputs::new(BoundedInputs::try_from_iter([input]).expect("one input")),
        Outputs::new([Note::new(9_999 - salt % 1000, ZkPublicKey::from(Fr::from(77u64)))]),
    )
}

/// Extra channel ops some LEZ inscriptions carry (deposit pins, atomic withdrawals).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extra {
    None,
    ChannelTransfer,
    ChannelWithdraw,
}

/// An inscription of `payload` on `channel`, chained on `parent`, in the transaction shape LEZ's
/// sequencer submits (`Op::ChannelInscribe` first, the wallet's fee `Transfer` last), signed by
/// `seq` over the transaction hash (`sign_and_submit`, `lez/sequencer/actors/bedrock`).
pub fn inscribe(seq: &Sequencer, channel: Hash, payload: &[u8], parent: Hash, extra: Extra, salt: u64) -> SignedTx {
    let op = InscriptionOp {
        channel_id: ChannelId::from(channel),
        inscription: payload.to_vec().try_into().expect("block fits an inscription"),
        parent: MsgId::from(parent),
        signer: seq.0.public_key().into_unverified(),
    };
    let op_id: Hash = op.id().into();
    let mut ops = vec![Op::ChannelInscribe(op)];
    match extra {
        Extra::None => {}
        Extra::ChannelTransfer => ops.push(Op::ChannelTransfer(ChannelTransferOp {
            channel_id: ChannelId::from(channel),
            inputs: Inputs::new(
                BoundedInputs::try_from_iter([NoteId(Fr::from(5u64)), NoteId(Fr::from(6u64))]).unwrap(),
            ),
            outputs: Outputs::new([Note::new(5, ZkPublicKey::from(Fr::from(8u64)))]),
        })),
        Extra::ChannelWithdraw => ops.push(Op::ChannelWithdraw(ChannelWithdrawOp {
            channel_id: ChannelId::from(channel),
            inputs: Inputs::new(BoundedInputs::try_from_iter([NoteId(Fr::from(9u64))]).unwrap()),
        })),
    }
    ops.push(Op::Transfer(fee_transfer(salt)));
    let ops = Ops::try_from(ops).expect("few ops");
    let tx_hash = ops.hash();
    let signature = seq.sign(tx_hash.as_signing_bytes());
    SignedTx { ops: ops.encode_to_vec(), op_index: 0, signature, tx_hash: tx_hash.0, op_id }
}

/// A channel configuration change to `keys` (in that order), chained on `parent`, carried in a
/// transaction with a fee transfer, and its `(index, signature)` multi-signature by `signers`
/// (indices into the *current* key list).
pub struct ConfigChange {
    pub tx: SignedTx,
    pub signatures: Vec<(u16, [u8; 64])>,
}

pub fn config_change(
    channel: Hash,
    parent: Hash,
    keys: &[&Sequencer],
    configuration_threshold: u16,
    signers: &[(u16, &Sequencer)],
) -> ConfigChange {
    let new_keys: Vec<_> = keys.iter().map(|k| k.0.public_key()).collect();
    let op = ChannelConfigOp {
        channel: ChannelId::from(channel),
        parent: MsgId::from(parent),
        keys: VerifiedChannelKeys::try_from(new_keys).expect("non-empty"),
        posting_timeframe: SlotTimeframe::from(30u32),
        posting_timeout: SlotTimeout::from(60u32),
        configuration_threshold,
        transfer_threshold: configuration_threshold,
    };
    let op_id: Hash = op.id().into();
    let ops = Ops::try_from(vec![Op::ChannelConfig(op), Op::Transfer(fee_transfer(424_242))]).unwrap();
    let tx_hash = ops.hash();
    let signatures = signers.iter().map(|(i, s)| (*i, s.sign(tx_hash.as_signing_bytes()))).collect();
    ConfigChange {
        tx: SignedTx { ops: ops.encode_to_vec(), op_index: 0, signature: [0; 64], tx_hash: tx_hash.0, op_id },
        signatures,
    }
}
