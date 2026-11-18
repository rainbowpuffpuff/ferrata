//! The Ferrata bridge program on LEZ v0.3.1 (`../README.md`): the configuration and its id, the
//! accounts and their state, the instructions, the effects the program plans and applies, and the
//! checks, shared by the guest (`../guests/bridge`) and the host (`../harness`).
//!
//! EXPERIMENTAL, UNAUDITED: a port of the inbound leg and the burn of the LEZ v0.4 program
//! (`../../lez/guests/bridge`, `../../lez/BRIDGE.md`) to the public LEZ testnet's v0.3.1 program
//! model. The v0.4 programs remain the deliverable.
//!
//! **The v0.3.1 model, and what it changes.** A v0.3.1 program has two entry points. Its *plan*
//! sees the instruction and the metadata of its input accounts (id, authorisation, which program's
//! shard), never their data, and emits one effect per input shard plus chained calls. Its *apply*
//! receives one effect and the current data of that one shard and returns the new data (its own
//! shards only) or keeps it; on another program's shard it can only check (`Plan::inspect`). So a
//! v0.4 handler that read an actor's state becomes, here, an instruction that carries the state it
//! relies on, checked by the plan, and an effect that *guards* the account: its apply refuses unless
//! the account holds exactly that state. In a privacy-preserving transaction the plan and the
//! effects on private accounts run inside the user's proof; the effects on public accounts are
//! deferred, published with the transaction and applied by the sequencer at settlement, where a
//! guard that fails rejects the whole transaction.
//!
//! Every hash and statement of the bridge is the bridge core's (`ferrata-bridge-core`): `configId`'s
//! preimage, the commitment, the deposit leaf and its Merkle path, the nullifier, the burn note's
//! commitment and MAC (made by the wallet), the burn leaf and the incremental tree of burns. The
//! Merkle-Patricia proofs of the vault's storage are verified with the light client's core
//! (`ferrata-core`'s `attest_batch`) under a record the light client wrote ([`LcRecord`]).

use borsh::{BorshDeserialize, BorshSerialize};
use ferrata_bridge_core::{
    chain_id_u256,
    encoding::{config_id_preimage, ConfigIdentity},
    inbound, outbound,
    tree::{IncrementalTree, MerklePath, TREE_DEPTH},
};
use ferrata_core::{
    attest as proven,
    witness::{ExecutionSummary, HeaderRecord},
};
use lee_core::{account::AccountId, program::PdaSeed};
use sha2::{Digest, Sha256};

pub use ferrata_bridge_core::{inbound::DestKind, sha256};

/// A 32-byte hash.
pub type Root = [u8; 32];
/// A 20-byte Ethereum address.
pub type Address = [u8; 20];
/// A 256-bit big-endian word.
pub type Word = [u8; 32];

/// ETH's token address in the vault's registry (`address(0)`).
pub const ETH: Address = [0u8; 20];

/// The tags of the v0.4 program (`lez/bridge-wire`): the same seeds and tagged hashes.
const DOMAIN: &[u8] = b"ferrata-lez/v1/bridge/";

fn tagged_hash(tag: &[u8], parts: &[&[u8]]) -> Root {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(tag);
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn encode<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

/// SHA-256 of a state's borsh encoding: what a guard compares.
pub fn state_hash<T: BorshSerialize>(value: &T) -> Root {
    sha256(&encode(value))
}

// ---------------------------------------------------------------------------------------------
// The vault's storage (SPEC-v2 §3.1; vault/README.md "Storage-slot contract")
// ---------------------------------------------------------------------------------------------

/// The deposit root, the leaf count and `storedLezConfigId`.
pub const SLOT_ROOT: Word = word_of(0);
pub const SLOT_COUNT: Word = word_of(1);
pub const SLOT_CONFIG_ID: Word = word_of(2);

/// `n` as a 256-bit big-endian word.
pub const fn word_of(n: u64) -> Word {
    let mut w = [0u8; 32];
    let b = n.to_be_bytes();
    let mut i = 0;
    while i < 8 {
        w[24 + i] = b[i];
        i += 1;
    }
    w
}

/// A word that fits 128 bits, as `u128`.
pub fn u128_of(word: &Word) -> Option<u128> {
    if word[..16] != [0u8; 16] {
        return None;
    }
    Some(u128::from_be_bytes(word[16..].try_into().expect("16 bytes")))
}

/// `n` as a 256-bit big-endian word.
pub fn word_of_u128(n: u128) -> Word {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&n.to_be_bytes());
    w
}

// ---------------------------------------------------------------------------------------------
// The configuration
// ---------------------------------------------------------------------------------------------

/// One denomination the configuration accepts: `amount` base units of `token`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Denomination {
    pub token: Address,
    pub amount: u128,
}

/// At most two registry entries: ETH in one denomination and one ERC-20 in one denomination.
pub const MAX_DENOMINATIONS: usize = 2;

/// One configuration of the bridge (SPEC-v2 §1), as on v0.4 with `controls` replaced by the
/// registry: this port has no fees, caps, freezes or roles, and its registry is part of the
/// configuration, so `configId` (which the vault pins and holds in slot 2) commits to it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BridgeConfig {
    pub bedrock_genesis_id: Root,
    pub channel_id: Root,
    /// The bridge program's image id, as the vault's verifier would pin it. The program cannot see
    /// its own image id; it is part of `configId`, which the vault pins.
    pub bridge_image_id: Root,
    /// The light client program whose records the configuration reads Ethereum through.
    pub attestation: AccountId,
    /// The light client instance those records belong to.
    pub instance_id: Root,
    pub evm_chain_id: u64,
    pub vault: Address,
    pub registry: Vec<Denomination>,
}

impl BridgeConfig {
    /// `H(…/attestation-root ‖ program ‖ instance)`, as on v0.4.
    pub fn attestation_root_id(&self) -> Root {
        tagged_hash(b"attestation-root", &[self.attestation.value(), &self.instance_id])
    }

    /// `H(…/registry ‖ borsh(registry))`: this port's `salt` (v0.4's commits to its controls).
    pub fn salt(&self) -> Root {
        tagged_hash(b"registry", &[&encode(&self.registry)])
    }

    /// SPEC-v2 §1 `configId` of this configuration of the bridge program deployed at
    /// `bridge_program`, with the bridge core's preimage.
    pub fn id(&self, bridge_program: AccountId) -> Root {
        sha256(&config_id_preimage(&ConfigIdentity {
            bedrock_genesis_id: self.bedrock_genesis_id,
            channel_id: self.channel_id,
            bridge_header: *bridge_program.value(),
            bridge_image_id: self.bridge_image_id,
            evm_chain_id: chain_id_u256(self.evm_chain_id),
            vault: self.vault,
            attestation_root_id: self.attestation_root_id(),
            salt: self.salt(),
        }))
    }

    /// One or two entries, distinct tokens, non-zero amounts.
    pub fn check(&self) -> Result<(), u8> {
        let r = &self.registry;
        if r.is_empty() || r.len() > MAX_DENOMINATIONS || r.iter().any(|d| d.amount == 0) {
            return Err(code::BAD_MESSAGE);
        }
        if r.len() == 2 && r[0].token == r[1].token {
            return Err(code::BAD_MESSAGE);
        }
        Ok(())
    }

    /// Whether `amount` of `token` is the registered denomination.
    pub fn registered(&self, token: &Address, amount: u128) -> bool {
        self.registry.iter().any(|d| d.token == *token && d.amount == amount)
    }
}

// ---------------------------------------------------------------------------------------------
// Accounts (public PDAs of the bridge program; the seeds of `lez/bridge-wire`)
// ---------------------------------------------------------------------------------------------

pub fn config_seed(config_id: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"config", &[config_id]))
}

pub fn root_seed(config_id: &Root, leaf_count: u64) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"root", &[config_id, &leaf_count.to_le_bytes()]))
}

pub fn nullifier_seed(evm_chain_id: u64, vault: &Address, nullifier: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"nullifier", &[&chain_id_u256(evm_chain_id), vault, nullifier]))
}

pub fn burn_seed(config_id: &Root, note_hash: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"burn", &[config_id, note_hash]))
}

/// The burn accumulator of a configuration (new in this port).
pub fn burns_seed(config_id: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"burns", &[config_id]))
}

pub fn holder_seed(account: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"holder", &[account]))
}

fn pda(program: AccountId, seed: &PdaSeed) -> AccountId {
    AccountId::for_public_pda(&program, seed)
}

/// `config(cid)`: the configuration, written once by `Init`.
pub fn config_account(program: AccountId, config_id: &Root) -> AccountId {
    pda(program, &config_seed(config_id))
}

/// `root(cid, n)`: the vault's deposit root at `n` leaves, written once by `ImportRoot`.
pub fn root_account(program: AccountId, config_id: &Root, leaf_count: u64) -> AccountId {
    pda(program, &root_seed(config_id, leaf_count))
}

/// `nullifier(chain, vault, N)`: spent once by a claim. No `cid`: one namespace per vault.
pub fn nullifier_account(program: AccountId, evm_chain_id: u64, vault: &Address, nullifier: &Root) -> AccountId {
    pda(program, &nullifier_seed(evm_chain_id, vault, nullifier))
}

/// `burn(cid, H(note))`: a burn note, written once.
pub fn burn_account(program: AccountId, config_id: &Root, note_hash: &Root) -> AccountId {
    pda(program, &burn_seed(config_id, note_hash))
}

/// `burns(cid)`: the configuration's burn accumulator.
pub fn burns_account(program: AccountId, config_id: &Root) -> AccountId {
    pda(program, &burns_seed(config_id))
}

/// `holder(account)`: `account`'s public wrapped balances. Its seed carries the role, so it is
/// never the address of another account of the program, whatever account a claim names (v0.4's
/// security review C-1).
pub fn holder_account(program: AccountId, account: &Root) -> AccountId {
    pda(program, &holder_seed(account))
}

// ---------------------------------------------------------------------------------------------
// The light client's record (a thin interface until the v0.3.1 light client is committed)
// ---------------------------------------------------------------------------------------------

/// `header(instance, slot)`'s seed in the v0.3.1 light client program (`lez-v03/wire`'s
/// `header_seed`, branch `testnet-v03`: `H("ferrata-lez-v03/v1/header" ‖ instance ‖ slot)`).
pub fn header_seed(instance_id: &Root, slot: u64) -> PdaSeed {
    let mut h = Sha256::new();
    h.update(b"ferrata-lez-v03/v1/");
    h.update(b"header");
    h.update(instance_id);
    h.update(slot.to_le_bytes());
    PdaSeed::new(h.finalize().into())
}

/// The light client's `header(instance, slot)` account.
pub fn header_account(attestation: AccountId, instance_id: &Root, slot: u64) -> AccountId {
    AccountId::for_public_pda(&attestation, &header_seed(instance_id, slot))
}

/// What the light client keeps in its shard of `header(instance, slot)`, written once after it
/// verified a finality update (`lez-v03/wire`'s `RecordState`, the same borsh layout).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LcRecord {
    pub instance_id: Root,
    pub chain_id: u64,
    /// `ferrata_core::witness::HeaderRecord`'s strict encoding.
    pub record: Vec<u8>,
}

/// The record an instruction relies on: the beacon slot it is keyed by and the exact bytes of the
/// light client's shard there. The plan verifies the proofs under it; a guard on the light
/// client's account checks, in the same transaction, that the shard holds exactly these bytes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RecordRef {
    pub slot: u64,
    pub state: Vec<u8>,
}

/// First element of an account proof that carries the block's RLP header (Gloas-layout records),
/// as on v0.4 (`lez/bridge-wire`).
pub const HEADER_MARKER: &[u8] = b"ferrata-lez/v1/bridge/rlp-header";

/// `account` with a header in front of it, as [`split_header`] reads it.
pub fn with_header(header: Vec<u8>, account: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut out = vec![HEADER_MARKER.to_vec(), header];
    out.extend(account);
    out
}

/// The header an account proof carries, if any, and the account's own nodes.
pub fn split_header(account: &[Vec<u8>]) -> (Option<Vec<u8>>, Vec<Vec<u8>>) {
    match account {
        [marker, header, nodes @ ..] if marker.as_slice() == HEADER_MARKER => (Some(header.clone()), nodes.to_vec()),
        _ => (None, account.to_vec()),
    }
}

/// The proofs of an import: the vault's account and its slots 0 and 1.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ImportProof {
    pub account: Vec<Vec<u8>>,
    pub root_slot: Vec<Vec<u8>>,
    pub count_slot: Vec<Vec<u8>>,
}

/// The proofs of the pairing: the vault's account and its slot 2.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PairingProof {
    pub account: Vec<Vec<u8>>,
    pub config_slot: Vec<Vec<u8>>,
}

/// The vault's slots, proven under a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VaultFacts<const K: usize> {
    pub block_number: u64,
    pub values: [Word; K],
}

fn core_refusal(error: ferrata_core::Error) -> u8 {
    error.code() as u8
}

/// The values of the vault's `slots` under the record `record`, for `config`'s light client
/// instance and chain: the steps of v0.4's `attest::evaluate` and `vault_slots` for one account
/// and its slots, without hops, logs or leaves. In order: the record decodes
/// ([`lc_code::BAD_STATE`]), names the configuration's instance ([`lc_code::WRONG_INSTANCE`]) and
/// chain (the core's `ChainMismatch`) and is keyed by its own beacon slot
/// ([`code::IMPORT_MISMATCH`]); the execution block comes from the record's payload fields, or, for
/// a record that binds a block hash only (Gloas), from the RLP header the proof carries in front
/// ([`lc_code::HEADER_REQUIRED`] without one, [`lc_code::BAD_QUERY`] with one where none is
/// needed); the account and its slots are proven under that block's state root (the core's codes).
pub fn vault_facts<const K: usize>(
    config: &BridgeConfig,
    record: &RecordRef,
    account: &[Vec<u8>],
    slots: [(Word, &[Vec<u8>]); K],
) -> Result<VaultFacts<K>, u8> {
    let state: LcRecord = borsh::from_slice(&record.state).map_err(|_| lc_code::BAD_STATE)?;
    if state.instance_id != config.instance_id {
        return Err(lc_code::WRONG_INSTANCE);
    }
    if state.chain_id != config.evm_chain_id {
        return Err(core_refusal(ferrata_core::Error::ChainMismatch));
    }
    let recorded = HeaderRecord::decode(&state.record).map_err(|_| lc_code::BAD_STATE)?;
    if recorded.beacon.slot != record.slot {
        return Err(code::IMPORT_MISMATCH);
    }
    let (header, nodes) = split_header(account);
    let chain = config.evm_chain_id;
    let block = match (&recorded.execution, &header) {
        (ExecutionSummary::BlockHash(_), Some(header)) => {
            proven::HeaderRecord::from_recorded_block_hash(&recorded, chain, header)
        }
        (ExecutionSummary::BlockHash(_), None) => return Err(lc_code::HEADER_REQUIRED),
        (ExecutionSummary::Payload { .. }, Some(_)) => return Err(lc_code::BAD_QUERY),
        (ExecutionSummary::Payload { .. } | ExecutionSummary::None, _) => {
            proven::HeaderRecord::from_recorded(&recorded, chain)
        }
    }
    .map_err(core_refusal)?;
    let queries: Vec<proven::SlotQuery<'_, Vec<u8>>> =
        slots.iter().map(|(slot, proof)| proven::SlotQuery { slot: *slot, proof, conditions: &[] }).collect();
    let accounts =
        [proven::AccountQuery { address: config.vault, proof: nodes.as_slice(), conditions: &[], slots: &queries }];
    let batch = proven::Batch { accounts: &accounts, logs: &[], relations: &[] };
    let facts = proven::attest_batch(&block, chain, &batch).map_err(core_refusal)?;
    if facts.len() != K + 1 {
        return Err(code::IMPORT_MISMATCH);
    }
    let mut values = [[0u8; 32]; K];
    for (i, fact) in facts[1..].iter().enumerate() {
        match fact.fact {
            proven::Fact::Storage(s) if s.address == config.vault && s.slot == slots[i].0 => values[i] = s.value,
            _ => return Err(code::IMPORT_MISMATCH),
        }
    }
    Ok(VaultFacts { block_number: block.number(), values })
}

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

/// `config(cid)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ConfigState {
    pub config: BridgeConfig,
}

/// `root(cid, n)`: the vault's deposit root when it held `leaf_count` leaves (v0.4's layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RootState {
    pub config_id: Root,
    pub evm_chain_id: u64,
    pub vault: Address,
    pub leaf_count: u64,
    pub root: Root,
    /// The beacon slot of the record the root was attested under, and its execution block.
    pub record_slot: u64,
    pub block_number: u64,
}

/// `nullifier(chain, vault, N)`: what was claimed with it (v0.4's layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NullifierState {
    pub config_id: Root,
    pub token: Address,
    pub leaf_count: u64,
    pub amount: u128,
}

/// One wrapped balance: base units of `token` under configuration `config_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Holding {
    pub config_id: Root,
    pub token: Address,
    pub amount: u128,
}

/// Wrapped balances: at `holder(account)` (public), or in a private account's own shard of the
/// bridge program (private). The testnet has no token program: the bridge keeps the balances.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HolderState {
    pub holdings: Vec<Holding>,
}

impl HolderState {
    pub fn credit(&mut self, config_id: &Root, token: &Address, amount: u128) -> Option<()> {
        match self.holdings.iter_mut().find(|h| h.config_id == *config_id && h.token == *token) {
            Some(h) => h.amount = h.amount.checked_add(amount)?,
            None => self.holdings.push(Holding { config_id: *config_id, token: *token, amount }),
        }
        Some(())
    }

    pub fn holding(&self, config_id: &Root, token: &Address) -> u128 {
        self.holdings.iter().find(|h| h.config_id == *config_id && h.token == *token).map_or(0, |h| h.amount)
    }

    pub fn debit(&mut self, config_id: &Root, token: &Address, amount: u128) -> Option<()> {
        let i = self.holdings.iter().position(|h| h.config_id == *config_id && h.token == *token)?;
        let h = &mut self.holdings[i];
        h.amount = h.amount.checked_sub(amount)?;
        if h.amount == 0 {
            self.holdings.remove(i);
        }
        Some(())
    }
}

/// A burn note (SPEC-v2 §5.1, D12): `(r, memo, mac, token, amount, COut)`. `COut` commits to the
/// Ethereum recipient (`burn_commitment(sOut, chain, vault, recipient)`); `memo` is the recipient
/// masked with the holder's view key; neither shows the recipient (as on v0.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Note {
    pub r: Root,
    pub memo: Address,
    pub mac: [u8; 16],
    pub token: Address,
    /// `uint256`, big-endian.
    pub amount: Word,
    pub c_out: Root,
}

impl Note {
    /// `H(…/note ‖ r ‖ memo ‖ mac ‖ token ‖ amount ‖ COut)`, as on v0.4: the burn account is keyed
    /// on the whole note.
    pub fn hash(&self) -> Root {
        tagged_hash(b"note", &[&self.r, &self.memo, &self.mac, &self.token, &self.amount, &self.c_out])
    }

    /// The note's leaf at position `burn_index` of the configuration's burn tree (the bridge core's
    /// `burn_leaf`, SPEC-v2 §5.1).
    pub fn leaf(&self, config_id: &Root, burn_index: u64) -> Root {
        outbound::burn_leaf(config_id, burn_index, &self.token, &self.amount, &self.c_out, &self.r)
    }
}

/// `burn(cid, H(note))`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BurnRecord {
    pub config_id: Root,
    pub note: Note,
}

/// `burns(cid)`: the burn tree's count and frontier (the bridge core's `IncrementalTree`); its root
/// is the burn root a release on Ethereum would be proven against.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BurnTree {
    pub config_id: Root,
    pub count: u64,
    pub frontier: Vec<Root>,
}

impl BurnTree {
    pub fn new(config_id: Root) -> Self {
        Self { config_id, count: 0, frontier: IncrementalTree::new().frontier().to_vec() }
    }

    fn tree(&self) -> Option<IncrementalTree> {
        let frontier: [Root; TREE_DEPTH] = self.frontier.as_slice().try_into().ok()?;
        IncrementalTree::from_frontier(self.count, &frontier).ok()
    }

    /// The tree's root.
    pub fn root(&self) -> Option<Root> {
        Some(self.tree()?.root())
    }

    /// Appends `note`'s leaf at the next index; the index.
    pub fn append(&mut self, note: &Note) -> Option<u64> {
        let mut tree = self.tree()?;
        let index = tree.append(note.leaf(&self.config_id, self.count)).ok()?;
        self.count = tree.len();
        self.frontier = tree.frontier().to_vec();
        Some(index)
    }
}

/// Every account state of the program, so that any shard decodes on its own.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BridgeState {
    Config(ConfigState),
    Root(RootState),
    Nullifier(NullifierState),
    Holder(HolderState),
    Burn(BurnRecord),
    Burns(BurnTree),
}

impl BridgeState {
    pub fn decode(bytes: &[u8]) -> Result<Self, u8> {
        borsh::from_slice(bytes).map_err(|_| code::BAD_STATE)
    }

    pub fn encode(&self) -> Vec<u8> {
        encode(self)
    }
}

// ---------------------------------------------------------------------------------------------
// The claim statement
// ---------------------------------------------------------------------------------------------

/// What a claimant knows about one deposit (v0.4's layout).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ClaimWitness {
    pub depositor: Address,
    pub token: Address,
    /// `uint256`, big-endian.
    pub amount: Word,
    pub leaf_index: u64,
    /// The 32 siblings, leaf level first.
    pub path: Vec<Root>,
    pub s: Root,
    /// `0x01` private, `0x02` public (SPEC-v2 §3.3).
    pub kind: u8,
    pub dest: Root,
}

/// What a verified claim gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Claimed {
    pub nullifier: Root,
    pub amount: u128,
    pub kind: DestKind,
}

/// The claim statement (SPEC-v2 §3.3, D9, D13) against an imported root, as on v0.4 without the
/// fee schedule range: the kind is known, the leaf index is below the root's count; then the bridge
/// core recomputes `C` and the leaf from the witness, checks the path (direction bits = the leaf
/// index) to the root and gives `N = H(s ‖ fid)` with the root's chain and vault.
pub fn verify_claim(root: &RootState, w: &ClaimWitness) -> Result<Claimed, u8> {
    let kind = DestKind::from_byte(w.kind).ok_or(code::CLAIM_INVALID)?;
    if w.leaf_index >= root.leaf_count {
        return Err(code::LEAF_NOT_IMPORTED);
    }
    let directions = u32::try_from(w.leaf_index).map_err(|_| code::LEAF_NOT_IMPORTED)?;
    let amount = u128_of(&w.amount).ok_or(code::NOT_REGISTERED)?;
    let siblings: [Root; TREE_DEPTH] = w.path.as_slice().try_into().map_err(|_| code::CLAIM_INVALID)?;
    let witness = inbound::ClaimWitness {
        depositor: w.depositor,
        token: w.token,
        amount: w.amount,
        leaf_index: w.leaf_index,
        path: MerklePath { directions, siblings },
        s: w.s,
        kind,
        dest: w.dest,
    };
    let nullifier = inbound::verify_claim(&root.root, &witness, &chain_id_u256(root.evm_chain_id), &root.vault)
        .map_err(|_| code::CLAIM_INVALID)?;
    Ok(Claimed { nullifier, amount, kind })
}

// ---------------------------------------------------------------------------------------------
// Instructions and effects
// ---------------------------------------------------------------------------------------------

/// Where a burn debits: the holder account's own shard of the bridge (a private account's
/// balance), or its `holder(account)` (a public account's balance).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BurnFrom {
    Own,
    Holder,
}

/// The instructions, with the accounts each takes (in this order; each the bridge's shard unless
/// said):
///
/// | instruction | accounts |
/// |---|---|
/// | `Init` | `config(cid)`; the light client's `header(instance, slot)` (its shard) |
/// | `ImportRoot` | `config(cid)`; `header(instance, slot)` (the light client's shard); `root(cid, n)` |
/// | `Claim` | `root(cid, n)`; `nullifier(chain, vault, N)`; `holder(dest)` |
/// | `ClaimPrivately` | the destination (a private account, authorised); `root(cid, n)`; `nullifier(chain, vault, N)` |
/// | `Burn`, `Own` | the holder (authorised); `burn(cid, H(note))`; `burns(cid)` |
/// | `Burn`, `Holder` | the holder (authorised); `holder(holder)`; `burn(cid, H(note))`; `burns(cid)` |
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Pair the configuration with its vault: the vault's slot 2 holds `configId` under the record.
    Init { config: BridgeConfig, record: RecordRef, proof: PairingProof },
    /// Import the vault's root and leaf count (slots 0 and 1) under the record. A top-level call
    /// only (D10).
    ImportRoot { config: BridgeConfig, record: RecordRef, root: Root, leaf_count: u64, proof: ImportProof },
    /// The plaintext claim of a public-kind deposit to `holder(dest)`.
    Claim { config: BridgeConfig, root: RootState, witness: ClaimWitness },
    /// The claim of a private-kind deposit, run by its destination account (a private account,
    /// in a privacy-preserving transaction), crediting its own shard.
    ClaimPrivately { config: BridgeConfig, root: RootState, witness: ClaimWitness },
    /// Burn one denomination for a release on Ethereum. A top-level call only (D10).
    Burn { config: BridgeConfig, note: Note, from: BurnFrom },
}

/// What the plan asks of each account; the apply carries it out on the account's data.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
    /// On the light client's `header(instance, slot)`: it holds a record with exactly this hash.
    RecordIs { hash: Root },
    /// On `config(cid)`: it holds a configuration state with exactly this hash.
    ConfigIs { hash: Root },
    /// On `root(cid, n)`: it holds a root state with exactly this hash.
    RootIs { hash: Root },
    /// Write the configuration, once.
    WriteConfig(ConfigState),
    /// Write the root, once.
    WriteRoot(RootState),
    /// Spend the nullifier, once.
    Spend(NullifierState),
    /// Credit a holding.
    Credit { config_id: Root, token: Address, amount: u128 },
    /// Debit a holding.
    Debit { config_id: Root, token: Address, amount: u128 },
    /// Write the burn note, once.
    RecordBurn(BurnRecord),
    /// Append the note's leaf to the configuration's burn tree.
    AppendBurn { config_id: Root, note: Note },
}

/// Carry out `effect` on a shard holding `pre`: `Ok(None)` keeps it, `Ok(Some(bytes))` replaces it,
/// `Err(code)` refuses the transaction.
pub fn apply(effect: &Effect, pre: &[u8]) -> Result<Option<Vec<u8>>, u8> {
    let guard = |hash: &Root, absent: u8, other: u8| {
        if pre.is_empty() {
            Err(absent)
        } else if sha256(pre) != *hash {
            Err(other)
        } else {
            Ok(None)
        }
    };
    let create = |state: BridgeState, taken: u8| {
        if pre.is_empty() {
            Ok(Some(state.encode()))
        } else {
            Err(taken)
        }
    };
    let holder = || -> Result<HolderState, u8> {
        if pre.is_empty() {
            return Ok(HolderState::default());
        }
        match BridgeState::decode(pre)? {
            BridgeState::Holder(h) => Ok(h),
            _ => Err(code::BAD_STATE),
        }
    };
    match effect {
        Effect::RecordIs { hash } => guard(hash, lc_code::NO_RECORD, code::IMPORT_MISMATCH),
        Effect::ConfigIs { hash } => guard(hash, code::NOT_FOUND, code::BAD_STATE),
        Effect::RootIs { hash } => guard(hash, code::NOT_FOUND, code::NOT_FOUND),
        Effect::WriteConfig(state) => create(BridgeState::Config(state.clone()), code::ALREADY_WRITTEN),
        Effect::WriteRoot(state) => create(BridgeState::Root(*state), code::ALREADY_WRITTEN),
        Effect::Spend(state) => create(BridgeState::Nullifier(*state), code::ALREADY_CLAIMED),
        Effect::RecordBurn(record) => create(BridgeState::Burn(*record), code::ALREADY_WRITTEN),
        Effect::Credit { config_id, token, amount } => {
            let mut h = holder()?;
            h.credit(config_id, token, *amount).ok_or(code::BAD_STATE)?;
            Ok(Some(BridgeState::Holder(h).encode()))
        }
        Effect::Debit { config_id, token, amount } => {
            let mut h = holder()?;
            h.debit(config_id, token, *amount).ok_or(code::INSUFFICIENT_BALANCE)?;
            Ok(Some(BridgeState::Holder(h).encode()))
        }
        Effect::AppendBurn { config_id, note } => {
            let mut tree = if pre.is_empty() {
                BurnTree::new(*config_id)
            } else {
                match BridgeState::decode(pre)? {
                    BridgeState::Burns(t) if t.config_id == *config_id => t,
                    _ => return Err(code::BAD_STATE),
                }
            };
            tree.append(note).ok_or(code::BAD_STATE)?;
            Ok(Some(BridgeState::Burns(tree).encode()))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------------------------

macro_rules! exit_codes {
    ($($(#[doc = $doc:literal])* $name:ident = $value:literal;)*) => {
        $($(#[doc = $doc])* pub const $name: u8 = $value;)*
        /// Every code of this module, with its name.
        pub const ALL: &[(&str, u8)] = &[$((stringify!($name), $name)),*];
    };
}

/// The bridge's own refusals: v0.4's codes (`lez/bridge-wire`, 240–255), with the same meaning;
/// those of v0.4's controls (`FROZEN`, `CAPPED`) never occur here.
pub mod code {
    exit_codes! {
        /// An instruction that does not decode, accounts that are not the ones it names, or a
        /// registry out of bounds.
        BAD_MESSAGE = 240;
        /// Stored state that does not decode, or is another role's.
        BAD_STATE = 241;
        /// Not used (v0.4's freezes).
        FROZEN = 242;
        /// An account that is not the one the instruction names.
        WRONG_SENDER = 243;
        /// A write-once account already written: the configuration, a root at the same leaf
        /// count, a burn note.
        ALREADY_WRITTEN = 244;
        /// No configuration (an import before `Init`), or no such imported root (a claim).
        NOT_FOUND = 245;
        /// The vault's slot 2 does not hold the configuration's id.
        PAIRING_MISMATCH = 246;
        /// The attested facts are not the vault's slots holding what the instruction says, or the
        /// light client's account does not hold the record the instruction carries.
        IMPORT_MISMATCH = 247;
        /// A leaf index at or above the imported root's leaf count.
        LEAF_NOT_IMPORTED = 248;
        /// The opening, the leaf and the path do not hash to the root (a wrong leaf, path or
        /// destination), or an unknown kind.
        CLAIM_INVALID = 249;
        /// The deposit's nullifier is spent.
        ALREADY_CLAIMED = 250;
        /// A token or an amount the configuration does not register.
        NOT_REGISTERED = 251;
        /// A destination this path cannot credit: a plaintext claim of a private-kind deposit, a
        /// private claim of a public-kind deposit, or a private claim not run as its destination.
        WRONG_DESTINATION = 252;
        /// A burn not authorised by its holder, or not a top-level call (D10); an `ImportRoot` that
        /// is not a top-level call (D10).
        NOT_AUTHORIZED = 253;
        /// A holding smaller than the denomination burned.
        INSUFFICIENT_BALANCE = 254;
        /// Not used (v0.4's caps).
        CAPPED = 255;
    }
}

/// The light client's refusals the bridge reproduces when it reads a record (`lez/wire`'s codes).
pub mod lc_code {
    exit_codes! {
        /// A record that does not decode.
        BAD_STATE = 209;
        /// A record of another instance than the configuration's.
        WRONG_INSTANCE = 211;
        /// The light client's account holds no record.
        NO_RECORD = 213;
        /// A block header sent for a record that holds the payload's fields.
        BAD_QUERY = 214;
        /// No block header for a record that binds a block hash only (Gloas).
        HEADER_REQUIRED = 215;
    }
}

/// The name of an exit code of the bridge, of the record checks, or `None`.
pub fn code_name(c: u32) -> Option<&'static str> {
    let c = u8::try_from(c).ok()?;
    code::ALL.iter().chain(lc_code::ALL).find(|(_, v)| *v == c).map(|(n, _)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(i: u8) -> Note {
        Note { r: [i; 32], memo: [1; 20], mac: [2; 16], token: ETH, amount: word_of(1_000), c_out: [3; 32] }
    }

    #[test]
    fn burn_tree_matches_the_bridge_core_full_tree() {
        let cid = [9u8; 32];
        let mut acc = BurnTree::new(cid);
        let mut leaves = Vec::new();
        for i in 0..5u8 {
            let index = acc.append(&note(i)).unwrap();
            assert_eq!(index, u64::from(i));
            leaves.push(note(i).leaf(&cid, u64::from(i)));
        }
        let full = ferrata_bridge_core::tree::Tree::from_leaves(leaves).unwrap();
        assert_eq!(acc.root().unwrap(), full.root());
        assert_eq!(acc.count, 5);
    }

    #[test]
    fn guards_and_write_once() {
        let state = BridgeState::Root(RootState {
            config_id: [1; 32],
            evm_chain_id: 1,
            vault: [2; 20],
            leaf_count: 3,
            root: [4; 32],
            record_slot: 5,
            block_number: 6,
        });
        let bytes = state.encode();
        let BridgeState::Root(r) = &state else { unreachable!() };
        assert_eq!(apply(&Effect::RootIs { hash: sha256(&bytes) }, &bytes), Ok(None));
        assert_eq!(apply(&Effect::RootIs { hash: sha256(&bytes) }, &[]), Err(code::NOT_FOUND));
        assert_eq!(apply(&Effect::RootIs { hash: [0; 32] }, &bytes), Err(code::NOT_FOUND));
        assert_eq!(apply(&Effect::WriteRoot(*r), &[]), Ok(Some(bytes.clone())));
        // Write-once is strict: even the same state again is refused (a nullifier must never be
        // re-spendable by writing identical data).
        assert_eq!(apply(&Effect::WriteRoot(*r), &bytes), Err(code::ALREADY_WRITTEN));
        let spent = NullifierState { config_id: [1; 32], token: ETH, leaf_count: 3, amount: 7 };
        let once = apply(&Effect::Spend(spent), &[]).unwrap().unwrap();
        assert_eq!(apply(&Effect::Spend(spent), &once), Err(code::ALREADY_CLAIMED));
    }

    #[test]
    fn credit_and_debit() {
        let c = Effect::Credit { config_id: [1; 32], token: ETH, amount: 5 };
        let after = apply(&c, &[]).unwrap().unwrap();
        let d = Effect::Debit { config_id: [1; 32], token: ETH, amount: 6 };
        assert_eq!(apply(&d, &after), Err(code::INSUFFICIENT_BALANCE));
        let d = Effect::Debit { config_id: [1; 32], token: ETH, amount: 5 };
        let empty = apply(&d, &after).unwrap().unwrap();
        assert_eq!(BridgeState::decode(&empty).unwrap(), BridgeState::Holder(HolderState::default()));
    }

    #[test]
    fn header_travels_in_front_of_the_account_proof() {
        let nodes = vec![vec![0xf8, 0x51, 0x80], vec![0xe2, 0x01]];
        let header = vec![0xf9, 0x02, 0x5a, 0xa0];
        assert_eq!(split_header(&nodes), (None, nodes.clone()));
        assert_eq!(split_header(&with_header(header.clone(), nodes.clone())), (Some(header), nodes));
        assert!(HEADER_MARKER[0] < 0xc0);
    }

    #[test]
    fn codes_are_distinct() {
        let mut all: Vec<u8> = code::ALL.iter().chain(lc_code::ALL).map(|(_, c)| *c).collect();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), code::ALL.len() + lc_code::ALL.len());
    }
}
