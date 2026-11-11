//! Shared input layouts of the fuzz targets, used by the targets and by the seed maker
//! (`examples/seeds.rs`), so that a seed written from a committed vector decodes the way the
//! target reads it.

use ferrata_core::fork::{ChainSpec, LcFork, Preset};

/// The light-client fork a selector byte names.
pub fn lc_fork(b: u8) -> LcFork {
    [LcFork::Altair, LcFork::Capella, LcFork::Deneb, LcFork::Electra, LcFork::Gloas][usize::from(b % 5)]
}

/// The selector byte of a light-client fork.
pub fn lc_fork_byte(f: LcFork) -> u8 {
    match f {
        LcFork::Altair => 0,
        LcFork::Capella => 1,
        LcFork::Deneb => 2,
        LcFork::Electra => 3,
        LcFork::Gloas => 4,
    }
}

/// Sync-committee size a selector byte names (minimal or mainnet preset).
pub fn committee_size(b: u8) -> usize {
    if b & 1 == 0 {
        32
    } else {
        512
    }
}

/// A cursor over the fuzzer's bytes; reads past the end yield zeros / empty slices.
pub struct Input<'a>(pub &'a [u8]);

impl<'a> Input<'a> {
    pub fn u8(&mut self) -> u8 {
        let (b, rest) = self.0.split_first().map(|(b, r)| (*b, r)).unwrap_or((0, &[]));
        self.0 = rest;
        b
    }
    pub fn take(&mut self, n: usize) -> &'a [u8] {
        let n = n.min(self.0.len());
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        a
    }
    pub fn array<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        let t = self.take(N);
        out[..t.len()].copy_from_slice(t);
        out
    }
    pub fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.array())
    }
    pub fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.array())
    }
    pub fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.array())
    }
    pub fn rest(&mut self) -> &'a [u8] {
        self.take(self.0.len())
    }
}

/// The `lc` target's input: a chain spec, the forks, a slot, a bootstrap and one update.
pub struct LcInput<'a> {
    pub spec: ChainSpec,
    pub bootstrap_fork: LcFork,
    pub store_fork: LcFork,
    pub update_fork: LcFork,
    pub current_slot: u64,
    pub genesis_validators_root: [u8; 32],
    pub bootstrap: &'a [u8],
    pub update: &'a [u8],
}

impl<'a> LcInput<'a> {
    /// Layout: preset, bootstrap fork, store fork, update fork (one byte each), current slot
    /// (u64 LE), genesis validators root (32), fork versions (9 x 4), fork epochs (9 x u64 LE),
    /// bootstrap length (u32 LE), bootstrap, update (the rest).
    pub fn parse(data: &'a [u8]) -> Self {
        let mut i = Input(data);
        let preset = if i.u8() & 1 == 0 { Preset::MINIMAL } else { Preset::MAINNET };
        let bootstrap_fork = lc_fork(i.u8());
        let store_fork = lc_fork(i.u8());
        let update_fork = lc_fork(i.u8());
        let current_slot = i.u64();
        let genesis_validators_root = i.array();
        let mut versions = [[0u8; 4]; 9];
        for v in versions.iter_mut() {
            *v = i.array();
        }
        let mut epochs = [0u64; 9];
        for e in epochs.iter_mut() {
            *e = i.u64();
        }
        let len = i.u32() as usize;
        let bootstrap = i.take(len);
        let update = i.rest();
        LcInput {
            spec: ChainSpec { preset, versions, epochs },
            bootstrap_fork,
            store_fork,
            update_fork,
            current_slot,
            genesis_validators_root,
            bootstrap,
            update,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![
            u8::from(self.spec.preset.sync_committee_size != 32),
            lc_fork_byte(self.bootstrap_fork),
            lc_fork_byte(self.store_fork),
            lc_fork_byte(self.update_fork),
        ];
        out.extend_from_slice(&self.current_slot.to_le_bytes());
        out.extend_from_slice(&self.genesis_validators_root);
        for v in &self.spec.versions {
            out.extend_from_slice(v);
        }
        for e in &self.spec.epochs {
            out.extend_from_slice(&e.to_le_bytes());
        }
        out.extend_from_slice(&(self.bootstrap.len() as u32).to_le_bytes());
        out.extend_from_slice(self.bootstrap);
        out.extend_from_slice(self.update);
        out
    }
}

/// The `mpt` target's raw mode: a key and proof nodes, each node prefixed by its u16 LE length.
pub fn encode_raw_proof(key: &[u8], nodes: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![1u8, key.len() as u8];
    out.extend_from_slice(key);
    for n in nodes {
        out.extend_from_slice(&(n.len() as u16).to_le_bytes());
        out.extend_from_slice(n);
    }
    out
}

/// Inverse of [`encode_raw_proof`] (after the mode byte).
pub fn parse_raw_proof(i: &mut Input<'_>) -> (Vec<u8>, Vec<Vec<u8>>) {
    let key_len = usize::from(i.u8() % 40);
    let key = i.take(key_len).to_vec();
    let mut nodes = Vec::new();
    while !i.0.is_empty() && nodes.len() < 64 {
        let n = usize::from(i.u16());
        nodes.push(i.take(n).to_vec());
    }
    (key, nodes)
}
