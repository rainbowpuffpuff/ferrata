//! The Ferrata bridge program on LEZ v0.4 (`guests/bridge`): its messages, actor state and
//! addresses, and the two checks it runs on Ethereum facts (the import of a deposit root, and the
//! claim of a deposit), shared by the guest and the host. The plan is `lez/BRIDGE.md`.
//!
//! | actor | address | state |
//! |---|---|---|
//! | `config(cid)` | PDA `H(…/config ‖ cid)` | [`ConfigState`]: the configuration, paired with its vault |
//! | `root(cid, n)` | PDA `H(…/root ‖ cid ‖ n)` | [`RootState`]: the vault's root at leaf count `n`, written once |
//! | `nullifier(chain, vault, N)` | PDA `H(…/nullifier ‖ chain ‖ vault ‖ N)` | [`NullifierState`]: `N` spent, written once |
//! | holder | the holder's own account | [`HolderState`]: wrapped balances (the M0 fallback: no v0.4 token program) |
//! | `burn(cid, H(note))` | PDA `H(…/burn ‖ cid ‖ H(note))` | [`BurnRecord`]: a burn note, written once |
//!
//! `cid` is SPEC-v2 §1's `configId`. The nullifier's address has no `cid`: one namespace for the
//! whole program, so two configurations cannot both mint against one vault deposit.
//!
//! Message variants 0–3 are the checkpoint's (`checkpoint/crates/checkpoint-core/src/wire.rs`):
//! the checkpoint program decodes every call to this program that a LEZ block shows in the clear
//! with that decoder. `Burn` is byte for byte its variant 0 (185 bytes). `ImportRoot` is its
//! variant 1's four fields followed by the Merkle-Patricia proofs the import needs in the same
//! transaction, which the checkpoint's decoder does not accept yet (`lez/BRIDGE-RESULTS.md`).

extern crate alloc;

use alloc::vec::Vec;

use borsh::{BorshDeserialize, BorshSerialize};
use ferrata_bridge_core::{
    chain_id_u256,
    encoding::{config_id_preimage, ConfigIdentity},
    inbound,
    sha256,
    tree::{MerklePath, TREE_DEPTH},
};
use ferrata_lez_wire::attest::{AccountQuery, Attestation, Fact, Request, SlotQuery, Statement};
use lee_core::{
    account::{AccountId, Actor},
    program::PdaSeed,
};
use sha2::{Digest, Sha256};

pub use ferrata_bridge_core::inbound::DestKind;

/// A 32-byte hash.
pub type Root = [u8; 32];
/// A 20-byte Ethereum address.
pub type Address = [u8; 20];
/// A 256-bit big-endian word.
pub type Word = [u8; 32];

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

/// The vault's storage slots (SPEC-v2 §3.1): the deposit root, the leaf count, `lezConfigId`.
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

/// A word that fits 64 bits, as `u64`.
pub fn u64_of(word: &Word) -> Option<u64> {
    if word[..24] != [0u8; 24] {
        return None;
    }
    Some(u64::from_be_bytes(word[24..].try_into().expect("8 bytes")))
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

/// What claims, mints and burns of one configuration check: its chain and vault, and its one
/// registered token with its denominations (in the token's base units). Copied from the
/// configuration into every root and every holding, so neither a claim nor a burn reads the
/// configuration again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Terms {
    pub evm_chain_id: u64,
    pub vault: Address,
    pub token: Address,
    pub denominations: [u128; 3],
}

impl Terms {
    /// `amount` as one of the registered denominations, if it is one.
    pub fn denomination(&self, amount: &Word) -> Option<u128> {
        let amount = u128_of(amount)?;
        self.denominations.contains(&amount).then_some(amount)
    }
}

/// One configuration of the bridge (SPEC-v2 §1), with the prototype's one-token registry.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BridgeConfig {
    pub bedrock_genesis_id: Root,
    pub channel_id: Root,
    /// The bridge program's image id, as the vault's verifier pins it. The program cannot see its
    /// own image id; it is part of `configId`, which the vault pins.
    pub bridge_image_id: Root,
    /// The attestation program whose records the configuration reads Ethereum through.
    pub attestation: AccountId,
    /// The attestation instance those records belong to.
    pub instance_id: Root,
    pub terms: Terms,
}

impl BridgeConfig {
    /// `H(…/attestation-root ‖ program ‖ instance)`: SPEC-v2's `attestationRootId`. An instance
    /// id alone does not name the program whose records it trusts (LEZ deploys one image at many
    /// addresses), so both enter `configId`.
    pub fn attestation_root_id(&self) -> Root {
        tagged_hash(b"attestation-root", &[self.attestation.value(), &self.instance_id])
    }

    /// `H(…/terms ‖ token ‖ denominations)`: the prototype's `salt`, so that `configId` (which
    /// the vault pins and holds in slot 2) commits to the registry and an `Init` cannot install
    /// other terms. The specified registry, attested from the vault, is not built.
    pub fn salt(&self) -> Root {
        let mut denominations = [0u8; 48];
        for (i, d) in self.terms.denominations.iter().enumerate() {
            denominations[16 * i..16 * (i + 1)].copy_from_slice(&d.to_be_bytes());
        }
        tagged_hash(b"terms", &[&self.terms.token, &denominations])
    }

    /// SPEC-v2 §1 `configId` for this configuration of the bridge program deployed at
    /// `bridge_header`, with the bridge core's preimage.
    pub fn id(&self, bridge_header: AccountId) -> Root {
        sha256(&config_id_preimage(&ConfigIdentity {
            bedrock_genesis_id: self.bedrock_genesis_id,
            channel_id: self.channel_id,
            bridge_header: *bridge_header.value(),
            bridge_image_id: self.bridge_image_id,
            evm_chain_id: chain_id_u256(self.terms.evm_chain_id),
            vault: self.terms.vault,
            attestation_root_id: self.attestation_root_id(),
            salt: self.salt(),
        }))
    }
}

// ---------------------------------------------------------------------------------------------
// Addresses
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

fn pda(program: AccountId, seed: &PdaSeed) -> Actor {
    Actor::new(AccountId::for_public_pda(&program, seed), program)
}

pub fn config_actor(program: AccountId, config_id: &Root) -> Actor {
    pda(program, &config_seed(config_id))
}

pub fn root_actor(program: AccountId, config_id: &Root, leaf_count: u64) -> Actor {
    pda(program, &root_seed(config_id, leaf_count))
}

pub fn nullifier_actor(program: AccountId, evm_chain_id: u64, vault: &Address, nullifier: &Root) -> Actor {
    pda(program, &nullifier_seed(evm_chain_id, vault, nullifier))
}

pub fn burn_actor(program: AccountId, config_id: &Root, note_hash: &Root) -> Actor {
    pda(program, &burn_seed(config_id, note_hash))
}

/// The bridge program's actor at `account`: a holder's wrapped balances.
pub fn holder_actor(program: AccountId, account: AccountId) -> Actor {
    Actor::new(account, program)
}

// ---------------------------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------------------------

/// A burn note (SPEC-v2 §5.1): `(r, memo, mac, token, amount, COut)`.
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
    /// `H(…/note ‖ r ‖ memo ‖ mac ‖ token ‖ amount ‖ COut)`: the burn account is keyed on the
    /// whole note (D13), so copying a pending burn's tag alone cannot make it revert.
    pub fn hash(&self) -> Root {
        tagged_hash(b"note", &[&self.r, &self.memo, &self.mac, &self.token, &self.amount, &self.c_out])
    }
}

/// The proofs of an import: the vault's account and its slots 0 and 1 (`eth_getProof`'s
/// `accountProof` and `storageProof[i].proof`).
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

/// What a claimant knows about one deposit (`ferrata_bridge_core::inbound::ClaimWitness`). The
/// path's direction bits are the leaf index itself, so they cannot disagree with it (D9).
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

/// Every message of the bridge program. The order of the variants is the encoding: 0–3 are the
/// checkpoint's (module docs).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BridgeMessage {
    /// Variant 0: At a holder's actor, as a top-level call or the root of a private execution: destroy one
    /// denomination and publish the note. Then, from that actor, at `burn(cid, H(note))`: record
    /// the note once.
    Burn { config_id: Root, r: Root, memo: Address, mac: [u8; 16], token: Address, amount: Word, c_out: Root },
    /// Variant 1: At `config(cid)`: attest the vault's slots 0 and 1 under the record at `record_slot` and
    /// write `root(cid, leaf_count)` if they hold `root` and `leaf_count`.
    ImportRoot { config_id: Root, record_slot: u64, root: Root, leaf_count: u64, proof: ImportProof },
    /// Variant 2: Reserved for the configuration's one-way `Sunset`; not built, refused.
    Sunset { config_id: Root },
    /// Variant 3: Rule (A)'s echo nonce: accepted anywhere, changes nothing.
    Echo { nonce: Root },
    /// Variant 4: At `config(cid)`, `cid` computed from `config`: attest the vault's slot 2 and write the
    /// configuration if it holds `cid` (the reciprocal pairing, B5).
    Init { config: BridgeConfig, record_slot: u64, proof: PairingProof },
    /// Variant 5: At `root(cid, n)`: the plaintext claim, for a public destination.
    Claim { config_id: Root, leaf_count: u64, witness: ClaimWitness },
    /// Variant 6: At the claimant's own actor, meant to run privately: read `root(cid, n)`, verify the
    /// claim inside this execution, spend `N`; a private destination must be this account.
    ClaimPrivately { config_id: Root, leaf_count: u64, witness: ClaimWitness },
    /// Variant 7: Reply with this actor's state (`StateReply`), LEZ v0.4's read idiom.
    Read,
    /// Variant 8: `config(cid)` → `root(cid, n)`: the verified root, written once.
    WriteRoot { root: RootState },
    /// Variant 9: An actor of this program → `nullifier(chain, vault, N)`: spend `N` once; mint to
    /// `mint_to` (a public account) if given.
    Spend { nullifier: Root, config_id: Root, terms: Terms, leaf_count: u64, amount: u128, mint_to: Option<Root> },
    /// Variant 10: `nullifier(chain, vault, N)` → a holder: credit one denomination.
    Mint { nullifier: Root, config_id: Root, terms: Terms, amount: u128 },
}

impl BridgeMessage {
    /// The `Burn` message of `note` under `config_id`.
    pub fn burn(config_id: Root, note: &Note) -> Self {
        BridgeMessage::Burn {
            config_id,
            r: note.r,
            memo: note.memo,
            mac: note.mac,
            token: note.token,
            amount: note.amount,
            c_out: note.c_out,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

/// The state of an actor of the bridge program, by role.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BridgeState {
    Config(ConfigState),
    Root(RootState),
    Nullifier(NullifierState),
    Holder(HolderState),
    Burn(BurnRecord),
}

/// `config(cid)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ConfigState {
    /// `Init` asked for the vault's slot 2 and the reply is outstanding (within one transaction).
    Pairing { config: BridgeConfig, record_slot: u64 },
    /// The configuration, paired with its vault. `pending` is an import whose attestation is
    /// outstanding (within one transaction); it is `None` between transactions.
    Ready { config: BridgeConfig, pending: Option<PendingImport> },
}

impl ConfigState {
    pub fn config(&self) -> &BridgeConfig {
        match self {
            ConfigState::Pairing { config, .. } | ConfigState::Ready { config, .. } => config,
        }
    }

    /// The record actor whose reply this state waits for.
    pub fn awaiting(&self) -> Option<Actor> {
        let slot = match self {
            ConfigState::Pairing { record_slot, .. } => *record_slot,
            ConfigState::Ready { pending: Some(p), .. } => p.record_slot,
            ConfigState::Ready { pending: None, .. } => return None,
        };
        let config = self.config();
        Some(ferrata_lez_wire::header_actor(config.attestation, &config.instance_id, slot))
    }
}

/// An import between its `Attest` and the reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingImport {
    pub record_slot: u64,
    pub root: Root,
    pub leaf_count: u64,
}

/// `root(cid, n)`: the vault's deposit root when it held `leaf_count` leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RootState {
    pub config_id: Root,
    pub terms: Terms,
    pub leaf_count: u64,
    pub root: Root,
    /// The beacon slot of the record the root was attested under, and its execution block.
    pub record_slot: u64,
    pub block_number: u64,
}

/// `nullifier(chain, vault, N)`: spent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NullifierState {
    pub config_id: Root,
    /// The leaf count of the root the claim read.
    pub leaf_count: u64,
    pub amount: u128,
}

/// A holder's wrapped balance under one configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Holding {
    pub config_id: Root,
    pub terms: Terms,
    /// In the token's base units; always a sum of whole denominations.
    pub amount: u128,
}

/// The bridge program's actor at a holder's account.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HolderState {
    pub holdings: Vec<Holding>,
    /// A private claim between its `Read` and the reply (within one transaction).
    pub pending: Option<PendingClaim>,
}

impl HolderState {
    /// Add `amount` to the holding of `config_id`.
    pub fn credit(&mut self, config_id: &Root, terms: &Terms, amount: u128) -> Option<()> {
        match self.holdings.iter_mut().find(|h| h.config_id == *config_id) {
            Some(h) => h.amount = h.amount.checked_add(amount)?,
            None => self.holdings.push(Holding { config_id: *config_id, terms: *terms, amount }),
        }
        Some(())
    }

    /// The holding of `config_id`.
    pub fn holding(&self, config_id: &Root) -> Option<&Holding> {
        self.holdings.iter().find(|h| h.config_id == *config_id)
    }
}

/// A private claim between its `Read` and the reply.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingClaim {
    pub config_id: Root,
    pub leaf_count: u64,
    pub witness: ClaimWitness,
}

/// `burn(cid, H(note))`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BurnRecord {
    pub config_id: Root,
    pub note: Note,
}

// ---------------------------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------------------------

/// The attestation request of an import: the vault's account and its slots 0 and 1.
pub fn import_request(config: &BridgeConfig, proof: &ImportProof) -> Request {
    Request {
        instance_id: config.instance_id,
        chain_id: config.terms.evm_chain_id,
        accounts: alloc::vec![AccountQuery {
            address: config.terms.vault,
            proof: proof.account.clone(),
            conditions: Vec::new(),
            slots: alloc::vec![
                SlotQuery { slot: SLOT_ROOT, proof: proof.root_slot.clone(), conditions: Vec::new() },
                SlotQuery { slot: SLOT_COUNT, proof: proof.count_slot.clone(), conditions: Vec::new() },
            ],
        }],
        ..Request::default()
    }
}

/// The attestation request of the pairing: the vault's account and its slot 2.
pub fn pairing_request(config: &BridgeConfig, proof: &PairingProof) -> Request {
    Request {
        instance_id: config.instance_id,
        chain_id: config.terms.evm_chain_id,
        accounts: alloc::vec![AccountQuery {
            address: config.terms.vault,
            proof: proof.account.clone(),
            conditions: Vec::new(),
            slots: alloc::vec![SlotQuery {
                slot: SLOT_CONFIG_ID,
                proof: proof.config_slot.clone(),
                conditions: Vec::new(),
            }],
        }],
        ..Request::default()
    }
}

/// The values of the vault's slots that `attestation` proves, in the order asked, if it is an
/// attestation of exactly the vault's account and those slots, for the configuration's instance
/// and chain, at the record of `record_slot`, without hops.
fn vault_slots<const K: usize>(
    config: &BridgeConfig,
    record_slot: u64,
    attestation: &Attestation,
    slots: [Word; K],
) -> Option<[Word; K]> {
    if attestation.instance_id != config.instance_id
        || attestation.chain_id != config.terms.evm_chain_id
        || attestation.slot != record_slot
        || attestation.hops != 0
        || attestation.statements.len() != K + 1
    {
        return None;
    }
    let vault = config.terms.vault;
    let [Statement { fact: Fact::Account { address, .. }, .. }, rest @ ..] = attestation.statements.as_slice() else {
        return None;
    };
    if *address != vault {
        return None;
    }
    let mut values = [[0u8; 32]; K];
    for (i, statement) in rest.iter().enumerate() {
        let Fact::Storage { address, slot, value } = &statement.fact else {
            return None;
        };
        if *address != vault || *slot != slots[i] {
            return None;
        }
        values[i] = *value;
    }
    Some(values)
}

/// Check the reply to an import: the vault's slots 0 and 1 hold the message's root and count.
pub fn check_import(
    config: &BridgeConfig,
    config_id: &Root,
    pending: &PendingImport,
    attestation: &Attestation,
) -> Result<RootState, u8> {
    let [root, count] = vault_slots(config, pending.record_slot, attestation, [SLOT_ROOT, SLOT_COUNT])
        .ok_or(code::IMPORT_MISMATCH)?;
    if root != pending.root || count != word_of(pending.leaf_count) {
        return Err(code::IMPORT_MISMATCH);
    }
    Ok(RootState {
        config_id: *config_id,
        terms: config.terms,
        leaf_count: pending.leaf_count,
        root,
        record_slot: pending.record_slot,
        block_number: attestation.block_number,
    })
}

/// Check the reply to `Init`: the vault's slot 2 holds this configuration's id.
pub fn check_pairing(
    config: &BridgeConfig,
    config_id: &Root,
    record_slot: u64,
    attestation: &Attestation,
) -> Result<(), u8> {
    let [held] =
        vault_slots(config, record_slot, attestation, [SLOT_CONFIG_ID]).ok_or(code::IMPORT_MISMATCH)?;
    if held != *config_id {
        return Err(code::PAIRING_MISMATCH);
    }
    Ok(())
}

/// What a verified claim gives: the nullifier, the denomination and the destination's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Claimed {
    pub nullifier: Root,
    pub amount: u128,
    pub kind: DestKind,
}

/// The claim statement (SPEC-v2 §3.3, D9, D13) against an imported root: the kind is known, the
/// leaf index is below the root's count, the token is the configuration's and the amount one of
/// its denominations; then the bridge core recomputes `C` and the leaf from the witness, checks
/// the path (direction bits = the leaf index) to the root, and gives `N = H(s ‖ fid)` with the
/// configuration's chain and vault.
pub fn verify_claim(root: &RootState, w: &ClaimWitness) -> Result<Claimed, u8> {
    let kind = DestKind::from_byte(w.kind).ok_or(code::CLAIM_INVALID)?;
    if w.leaf_index >= root.leaf_count {
        return Err(code::LEAF_NOT_IMPORTED);
    }
    let directions = u32::try_from(w.leaf_index).map_err(|_| code::LEAF_NOT_IMPORTED)?;
    if w.token != root.terms.token {
        return Err(code::NOT_REGISTERED);
    }
    let amount = root.terms.denomination(&w.amount).ok_or(code::NOT_REGISTERED)?;
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
    let nullifier = inbound::verify_claim(
        &root.root,
        &witness,
        &chain_id_u256(root.terms.evm_chain_id),
        &root.terms.vault,
    )
    .map_err(|_| code::CLAIM_INVALID)?;
    Ok(Claimed { nullifier, amount, kind })
}

// ---------------------------------------------------------------------------------------------
// The D12/D13 outbound encodings the bridge core does not have yet (wallet and release side; the
// program never sees `sOut` or `kView`)
// ---------------------------------------------------------------------------------------------

/// D13: `NOut = H("ferrata/nf-out/v2" ‖ sOut)`, independent of the burn's index.
pub fn nf_out_v2(s_out: &Root) -> Root {
    let mut preimage = Vec::with_capacity(17 + 32);
    preimage.extend_from_slice(b"ferrata/nf-out/v2");
    preimage.extend_from_slice(s_out);
    sha256(&preimage)
}

/// D12: `mac = HMAC(kView, "ferrata/burn-mac/v2" ‖ r ‖ memo ‖ token ‖ amount ‖ COut)[0..16]`.
pub fn burn_mac_v2(
    k_view: &ferrata_bridge_core::BridgeKey,
    r: &Root,
    memo: &Address,
    token: &Address,
    amount: &Word,
    c_out: &Root,
) -> [u8; 16] {
    let mut m = Vec::with_capacity(19 + 32 + 20 + 20 + 32 + 32);
    for part in [&b"ferrata/burn-mac/v2"[..], r, memo, token, amount, c_out] {
        m.extend_from_slice(part);
    }
    k_view.prf(&m)[..16].try_into().expect("16 bytes")
}

// ---------------------------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------------------------

/// The exit codes the bridge program owns: 240–255, the block `ferrata_lez_wire::CODE_BLOCKS`
/// leaves unassigned, so that a refusal of the attestation program (1–219) inside a bridge
/// transaction stays distinguishable. Sixteen codes; related refusals share one.
pub const CODE_BLOCK: core::ops::RangeInclusive<u8> = 240..=255;

macro_rules! exit_codes {
    ($($(#[doc = $doc:literal])* $name:ident = $value:literal;)*) => {
        $($(#[doc = $doc])* pub const $name: u8 = $value;)*
        /// Every code of this module, with its name.
        pub const ALL: &[(&str, u8)] = &[$((stringify!($name), $name)),*];
    };
}

pub mod code {
    exit_codes! {
        /// A message that does not decode, or that the bridge does not take (`Sunset`: not built).
        BAD_MESSAGE = 240;
        /// Stored state that does not decode, or is another role's, or a transaction-internal
        /// step (a pending import or claim) found where none can be.
        BAD_STATE = 241;
        /// The receiver is not the actor the message names.
        WRONG_RECEIVER = 242;
        /// An internal message from an actor other than the expected one, or a read with nobody
        /// to reply to.
        WRONG_SENDER = 243;
        /// `Init` of a configuration that exists, or a second import at the same leaf count.
        ALREADY_WRITTEN = 244;
        /// No configuration (an import before `Init`), or no imported root (a claim).
        NOT_FOUND = 245;
        /// The vault's slot 2 does not hold the configuration's id.
        PAIRING_MISMATCH = 246;
        /// The attested facts are not the vault's slots with the message's root and count (or of
        /// another instance, chain or record).
        IMPORT_MISMATCH = 247;
        /// A leaf index at or above the imported root's leaf count.
        LEAF_NOT_IMPORTED = 248;
        /// The opening, the leaf and the path do not hash to the root (a wrong leaf, path or
        /// destination), or an unknown kind.
        CLAIM_INVALID = 249;
        /// The deposit's nullifier is spent.
        ALREADY_CLAIMED = 250;
        /// A token other than the configuration's, or an amount that is not one of its
        /// denominations.
        NOT_REGISTERED = 251;
        /// A destination this path cannot credit: a plaintext claim of a private-kind deposit, or
        /// a private-kind claim not run as its destination.
        WRONG_DESTINATION = 252;
        /// A burn not authorised by its holder, or not a top-level call or private root (D10).
        NOT_AUTHORIZED = 253;
        /// A holding smaller than the denomination burned.
        INSUFFICIENT_BALANCE = 254;
        /// A burn note that is already published.
        BURN_EXISTS = 255;
    }
}
