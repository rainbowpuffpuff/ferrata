//! A deployment (configuration and sequencer keys), a LEZ chain whose every block is inscribed by
//! the round-robin sequencer, and checkpoint inputs assembled from them.

use anyhow::Result;
use checkpoint_core::input::{CheckpointInput, SegmentInput, Support};
use checkpoint_core::journal::SegmentJournal;
use checkpoint_core::mantle::{ConfigChange as CoreConfigChange, ElidedInscription};
use checkpoint_core::state::{Config, State};
use checkpoint_core::tree::Frontier;
use checkpoint_core::wire::{BridgeMessage, Burn, ImportRoot};
use checkpoint_core::{hash::sha256, Hash};
use lee::{AccountId, Actor};

use crate::bedrock::{inscribe, Extra, Sequencer, SignedTx};
use crate::lez_build::Chain;

pub fn label(s: &str) -> Hash {
    sha256(s.as_bytes())
}

/// LEZ's testnet channel id (`lez/sequencer/service/networks/testnet.json`: `0101…01`).
pub const CHANNEL: Hash = [1u8; 32];

pub struct Deployment {
    pub config: Config,
    pub sequencers: Vec<Sequencer>,
    /// The bridge program's public actor account burns are sent to.
    pub bridge_account: AccountId,
}

impl Deployment {
    pub fn new(n: usize, n_min: u32, t_min: u32) -> Self {
        let mut evm_chain_id = [0u8; 32];
        evm_chain_id[31] = 1;
        let config = Config {
            bedrock_genesis_id: label("fidelity: bedrock genesis"),
            channel_id: CHANNEL,
            bridge_header: label("fidelity: bridge program header account"),
            bridge_image_id: label("fidelity: bridge program image"),
            evm_chain_id,
            vault: label("fidelity: vault")[..20].try_into().unwrap(),
            attestation_root_id: label("fidelity: RFP-021 lineage root"),
            salt: label("fidelity: salt"),
            n_min,
            t_min,
        };
        let sequencers = (0..n as u64).map(Sequencer::from_seed).collect();
        Self { config, sequencers, bridge_account: AccountId::new(label("fidelity: bridge config account")) }
    }

    pub fn config_id(&self) -> Hash {
        self.config.config_id()
    }

    pub fn keys(&self) -> Vec<Hash> {
        self.sequencers.iter().map(Sequencer::public).collect()
    }

    pub fn bridge(&self) -> Actor {
        Actor::new(self.bridge_account, AccountId::new(self.config.bridge_header))
    }

    /// A `Burn` for this configuration (or `config_id` if given), distinct per `i`.
    pub fn burn(&self, i: u64, config_id: Option<Hash>) -> BridgeMessage {
        let mut amount = [0u8; 32];
        amount[24..].copy_from_slice(&(1_000_000u64 + i).to_be_bytes());
        BridgeMessage::Burn(Burn {
            config_id: config_id.unwrap_or_else(|| self.config_id()),
            r: label(&format!("burn tag {i}")),
            memo: label(&format!("memo {i}"))[..20].try_into().unwrap(),
            mac: label(&format!("mac {i}"))[..16].try_into().unwrap(),
            token: [0u8; 20],
            amount,
            c_out: label(&format!("C_out {i}")),
        })
    }

    pub fn import_root(&self, leaf_count: u64) -> BridgeMessage {
        BridgeMessage::ImportRoot(ImportRoot {
            config_id: self.config_id(),
            record_slot: leaf_count,
            root: label(&format!("deposit root {leaf_count}")),
            leaf_count,
        })
    }

    /// The state at the genesis block of `chain`: this key set, no burns.
    pub fn genesis_state(&self, chain: &Chain) -> State {
        let g = &chain.blocks[0];
        State {
            tip_id: g.header.block_id,
            tip_hash: g.header.hash.0,
            keys: self.keys(),
            posting_timeframe: crate::bedrock::POSTING_TIMEFRAME,
            config_tip_hash: label("fidelity: configuration tip at genesis"),
            burns: Frontier::default(),
            imported_leaf_count: 0,
            sunset: false,
        }
    }
}

/// A chain with every block inscribed by the sequencer whose turn it is (`block_id mod n`), each
/// inscription chained on the previous one's `MsgId`.
pub struct Inscribed {
    pub bytes: Vec<Vec<u8>>,
    pub inscriptions: Vec<SignedTx>,
    pub producer: Vec<usize>,
}

impl Inscribed {
    pub fn new(dep: &Deployment, chain: &Chain) -> Self {
        let n = dep.sequencers.len();
        let mut parent = [0u8; 32];
        let (mut bytes, mut inscriptions, mut producer) = (Vec::new(), Vec::new(), Vec::new());
        for (i, block) in chain.blocks.iter().enumerate() {
            let b = chain.inscribed(i);
            let who = usize::try_from(block.header.block_id).unwrap() % n;
            let tx =
                inscribe(&dep.sequencers[who], dep.config.channel_id, &b, parent, Extra::None, block.header.block_id);
            parent = tx.op_id;
            bytes.push(b);
            inscriptions.push(tx);
            producer.push(who);
        }
        Self { bytes, inscriptions, producer }
    }
}

/// A configuration change as carried in an input: the whole transaction, the op's position, and
/// its `(key index, signature)` pairs.
pub type OwnedConfigChange = (Vec<u8>, u8, Vec<(u16, [u8; 64])>);

/// An owned checkpoint input ([`OwnedInput::view`] lends it as the core's [`CheckpointInput`]).
#[derive(Clone)]
pub struct OwnedInput {
    pub config: Config,
    pub parent: State,
    pub config_changes: Vec<OwnedConfigChange>,
    pub blocks: Vec<Vec<u8>>,
    pub t_index: u32,
    pub echo_nonce: Hash,
    pub producer: SignedTx,
    pub support: Vec<(u32, SignedTx)>,
    pub segments: Vec<SegmentJournal>,
}

fn elided(tx: &SignedTx) -> ElidedInscription<'_> {
    ElidedInscription::split(&tx.ops, tx.op_index, &tx.signature).expect("an inscription")
}

impl OwnedInput {
    pub fn view(&self) -> CheckpointInput<'_> {
        CheckpointInput {
            config: self.config.clone(),
            parent: self.parent.clone(),
            config_changes: self
                .config_changes
                .iter()
                .map(|(ops, op_index, sigs)| CoreConfigChange {
                    ops,
                    op_index: *op_index,
                    signatures: sigs.iter().map(|(i, s)| (*i, s)).collect(),
                })
                .collect(),
            blocks: self.blocks.iter().map(Vec::as_slice).collect(),
            t_index: self.t_index,
            echo_nonce: self.echo_nonce,
            producer: elided(&self.producer),
            support: self
                .support
                .iter()
                .map(|(block_index, tx)| Support { block_index: *block_index, inscription: elided(tx) })
                .collect(),
            segments: self.segments.clone(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        self.view().encode()
    }

    /// Total bytes of the blocks carried.
    pub fn block_bytes(&self) -> usize {
        self.blocks.iter().map(Vec::len).sum()
    }
}

/// A checkpoint over chain blocks `parent_index+1 ..= e`, with `T = t`, `Y` found from
/// `echo_nonce`, and support from the first `support` distinct keys (other than `T`'s producer)
/// that inscribed blocks after `y`.
#[allow(clippy::too_many_arguments, reason = "a scenario constructor: each position is its own argument")]
pub fn checkpoint_input(
    dep: &Deployment,
    ins: &Inscribed,
    parent: State,
    parent_index: usize,
    t: usize,
    y: usize,
    e: usize,
    echo_nonce: Hash,
    support: usize,
) -> OwnedInput {
    let rel = |i: usize| u32::try_from(i - parent_index - 1).unwrap();
    let mut chosen = Vec::new();
    let mut keys = Vec::new();
    for i in y + 1..=e {
        let who = ins.producer[i];
        if chosen.len() < support && who != ins.producer[t] && !keys.contains(&who) {
            keys.push(who);
            chosen.push((rel(i), ins.inscriptions[i].clone()));
        }
    }
    OwnedInput {
        config: dep.config.clone(),
        parent,
        config_changes: Vec::new(),
        blocks: ins.bytes[parent_index + 1..=e].to_vec(),
        t_index: rel(t),
        echo_nonce,
        producer: ins.inscriptions[t].clone(),
        support: chosen,
        segments: Vec::new(),
    }
}

/// The owned bytes of a segment input over chain blocks `from ..= to`.
pub fn segment_input(dep: &Deployment, ins: &Inscribed, parent: &State, from: usize, to: usize) -> Vec<u8> {
    let blocks: Vec<&[u8]> = ins.bytes[from..=to].iter().map(Vec::as_slice).collect();
    SegmentInput { config: dep.config.clone(), parent: parent.clone(), blocks }.encode()
}

/// Runs a segment natively and returns its journal and end state.
pub fn run_segment(
    dep: &Deployment,
    ins: &Inscribed,
    parent: &State,
    from: usize,
    to: usize,
) -> Result<(SegmentJournal, State)> {
    let bytes = segment_input(dep, ins, parent, from, to);
    let input = SegmentInput::decode(&bytes).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    checkpoint_core::rule::run_segment(&input).map_err(|e| anyhow::anyhow!("{e:?}"))
}
