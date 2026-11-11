//! The watcher's own node's view of LEZ, as the monitor reads it.
//!
//! A node adapter (a file it rewrites, or a command the monitor runs) gives one JSON document per
//! poll ([`NodeView`]): the node's final tier (the highest final block id and the hashes of final
//! blocks), the channel's accredited key sets with the block from which each is live, and, when the
//! node computes them, the checkpoint fields at a tip and the final-tier attestations the attestors
//! published. Every field is what the node itself holds; nothing comes from the prover.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, bail, ensure, Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A 32-byte hash, key or word, written `0x` + 64 hex digits in JSON.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct H32(pub [u8; 32]);

impl fmt::Debug for H32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl fmt::Display for H32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl std::str::FromStr for H32 {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let digits = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(digits).with_context(|| format!("not hex: {s}"))?;
        let arr: [u8; 32] = bytes.try_into().map_err(|_| anyhow!("not 32 bytes: {s}"))?;
        Ok(Self(arr))
    }
}

impl Serialize for H32 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for H32 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// LEZ's signing threshold for `n` accredited keys, `t(n) = max(1, min(⌈2n/3⌉, n − 1))`, as
/// `checkpoint-core`'s `threshold` (LEZ `channel_config_threshold`; SPEC-v2 §4.2).
pub fn threshold(n: u32) -> u32 {
    let two_thirds_up = (2 * u64::from(n)).div_ceil(3);
    let cap = u64::from(n.saturating_sub(1));
    two_thirds_up.min(cap).max(1) as u32
}

/// An accredited key set of the channel, live from block `from_block` on (until the next entry).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeySet {
    pub from_block: u64,
    /// Channel keys in `accredited_keys` order.
    pub keys: Vec<H32>,
    /// The channel's `posting_timeframe`, bound into the key-set hash (SPEC-v3 D3), as the node reports it
    /// (0 on the lab devnet of the live round trip; rule (B) has no turn term).
    pub posting_timeframe: u32,
}

impl KeySet {
    pub fn n(&self) -> u32 {
        u32::try_from(self.keys.len()).unwrap_or(u32::MAX)
    }
    pub fn t(&self) -> u32 {
        threshold(self.n())
    }
    /// `keySetHash` (SPEC-v3 D3), the value every rule (B) attestor signs.
    pub fn hash(&self) -> [u8; 32] {
        let keys: Vec<[u8; 32]> = self.keys.iter().map(|k| k.0).collect();
        attestor_core::key_set_hash(&keys, self.posting_timeframe)
    }
}

/// The checkpoint journal fields the node's own checkpoint computation gives at a tip (optional).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TipState {
    /// The checkpoint state `S` at the tip, if the node computes it from the vault's final state.
    #[serde(default)]
    pub new_state: Option<H32>,
    pub burn_root: H32,
    pub burn_count: u64,
    pub imported_leaf_count: u64,
    pub sunset: bool,
}

/// One poll's view of one node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeView {
    /// The node's label in alerts.
    pub node: String,
    /// The highest block id in the node's final tier (Bedrock status `Finalized`).
    pub final_height: u64,
    /// Hashes of final blocks by id: at least every id at or below `final_height` that a pending
    /// candidate names. An id above `final_height` is refused.
    pub final_blocks: BTreeMap<u64, H32>,
    /// The channel's key sets, ascending by `from_block`.
    #[serde(default)]
    pub key_sets: Vec<KeySet>,
    /// Checkpoint fields at a tip, by tip id (optional).
    #[serde(default)]
    pub states: BTreeMap<u64, TipState>,
    /// Final-tier attestations the attestors published (rule (B)): `SignedAttestation::encode`,
    /// 264 bytes each, as `0x` hex (optional).
    #[serde(default)]
    pub attestations: Vec<String>,
}

/// Where a candidate's tip stands in one node's final tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipStatus {
    /// Final, with the candidate's hash.
    Final,
    /// The node's final block at that id has another hash: a conflicting history.
    Conflict(H32),
    /// Above the node's final height: not (yet) final in this node.
    NotFinal,
    /// At or below the final height, but the adapter did not give the block.
    Missing,
}

impl NodeView {
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.node.is_empty(), "a node view needs a label");
        if let Some((&id, _)) = self.final_blocks.iter().next_back() {
            ensure!(
                id <= self.final_height,
                "{}: block {id} given as final above finalHeight {}",
                self.node,
                self.final_height
            );
        }
        for w in self.key_sets.windows(2) {
            ensure!(w[0].from_block < w[1].from_block, "{}: keySets must ascend by fromBlock", self.node);
        }
        for ks in &self.key_sets {
            ensure!(!ks.keys.is_empty(), "{}: an empty key set", self.node);
        }
        Ok(())
    }

    pub fn tip_status(&self, tip_id: u64, tip_hash: &H32) -> TipStatus {
        if tip_id > self.final_height {
            return TipStatus::NotFinal;
        }
        match self.final_blocks.get(&tip_id) {
            Some(h) if h == tip_hash => TipStatus::Final,
            Some(h) => TipStatus::Conflict(*h),
            None => TipStatus::Missing,
        }
    }

    /// A view from a LEZ v0.4 sequencer's block record in the lab driver's `chain.tsv` format
    /// (`lez/lab/driver`: per block, `getBlock`'s id, hash and `bedrock_status`): the final tier is the
    /// run of `Finalized` blocks from the first line (a `Safe` or `Pending` block ends it). Ids must be
    /// consecutive. Key sets come from elsewhere (the channel's answer on the Logos Blockchain node).
    pub fn from_chain_tsv(node: &str, text: &str, key_sets: Vec<KeySet>) -> Result<Self> {
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let header: Vec<&str> = lines.next().context("an empty chain.tsv")?.split('\t').collect();
        let col = |name: &str| {
            header.iter().position(|c| *c == name).with_context(|| format!("chain.tsv has no {name} column"))
        };
        let (ib, ih, is) = (col("block")?, col("hash")?, col("bedrock_status")?);
        let mut final_blocks = BTreeMap::new();
        let mut final_height = 0u64;
        let mut in_final_run = true;
        let mut next: Option<u64> = None;
        for line in lines {
            let f: Vec<&str> = line.split('\t').collect();
            let field = |i: usize| f.get(i).copied().with_context(|| format!("a short chain.tsv line: {line}"));
            let id: u64 = field(ib)?.parse().with_context(|| format!("a block id: {line}"))?;
            if let Some(n) = next {
                ensure!(id == n, "chain.tsv jumps from block {} to {id}", n - 1);
            }
            next = Some(id + 1);
            let hash: H32 = field(ih)?.parse()?;
            if in_final_run && field(is)? == "Finalized" {
                final_blocks.insert(id, hash);
                final_height = id;
            } else {
                in_final_run = false;
            }
        }
        let v = Self {
            node: node.to_owned(),
            final_height,
            final_blocks,
            key_sets,
            states: BTreeMap::new(),
            attestations: Vec::new(),
        };
        v.validate()?;
        Ok(v)
    }

    /// The key set live at block `at` (the last one whose `from_block <= at`).
    pub fn live_key_set(&self, at: u64) -> Option<&KeySet> {
        self.key_sets.iter().rev().find(|k| k.from_block <= at)
    }
}

/// Where the monitor reads a node's view: `file:PATH` (the adapter rewrites the file) or
/// `cmd:COMMAND` (run with `sh -c` at every poll; its standard output is the document).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeSource {
    File(PathBuf),
    Cmd(String),
}

impl std::str::FromStr for NodeSource {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        if let Some(p) = s.strip_prefix("file:") {
            Ok(Self::File(PathBuf::from(p)))
        } else if let Some(c) = s.strip_prefix("cmd:") {
            Ok(Self::Cmd(c.to_owned()))
        } else {
            bail!("a node source is file:PATH or cmd:COMMAND, not {s}")
        }
    }
}

impl NodeSource {
    pub fn read(&self) -> Result<NodeView> {
        let bytes = match self {
            Self::File(p) => std::fs::read(p).with_context(|| format!("reading {}", p.display()))?,
            Self::Cmd(c) => {
                let out = Command::new("sh").arg("-c").arg(c).output().with_context(|| format!("running {c}"))?;
                ensure!(out.status.success(), "{c} exited with {}", out.status);
                out.stdout
            }
        };
        let view: NodeView = serde_json::from_slice(&bytes).context("a node view that does not parse")?;
        view.validate()?;
        Ok(view)
    }

    pub fn label(&self) -> String {
        match self {
            Self::File(p) => format!("file:{}", p.display()),
            Self::Cmd(c) => format!("cmd:{c}"),
        }
    }
}
