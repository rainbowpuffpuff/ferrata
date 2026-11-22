//! The bridge program (`guests/bridge`, `lez/BRIDGE.md`) through LEZ's state machine.
//!
//! **What is synthetic.** The Ethereum side: two vaults' storage (slot 0 the deposit root, slot 1
//! the leaf count, slot 2 `lezConfigId`) in the state tries of three synthetic execution blocks,
//! built with the reference trie of `ferrata-spectests`; and the records of those blocks, placed
//! in the attestation program's `header(id, slot)` actors at genesis instead of being written by
//! the light client after a finality update (Stages 1–4 measured that part on mainnet data).
//! Everything after that runs through LEZ: the bridge program calls the attestation program's
//! `Attest` on those records inside its own transactions.
//!
//! **The shared vectors.** Vault A's first five leaves are the claim tree of
//! `bridge/vectors/inbound-v2.json` (`claim`), so `root(5)` is the vector's root and the claim of
//! its leaf 2 must spend the vector's `N`. Leaves 5–7 are this harness's own deposits.
//!
//! **Which build.** The bridge program of `guests/bridge` (`methods::FERRATA_BRIDGE_*`), whose nullifier and
//! holder accounts are keyed by the configuration id ([`holder_of`], [`nullifier_of`]; milestone B1).

use std::borrow::Cow;

use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{commitment, deposit_id, deposit_leaf, deposit_secret, nullifier as core_nullifier, DestKind},
    tree::Tree,
    BridgeKeys, Hash,
};
use ferrata_core::{
    types::BeaconBlockHeader,
    witness::{ExecutionSummary, HeaderRecord},
    Root,
};
use ferrata_lez_bridge_wire::{
    self as bw, code, config_actor, config_seed, fee_slot, own_actor, register_slots, registry_slot, root_actor,
    root_seed, token_actor, BridgeConfig, BridgeMessage, BridgeState, CapParams, ClaimWitness, Controls, FeeSchedule,
    HolderState, ImportProof, NullifierState, PairingProof, RootState, ScheduleRange, TimeRecord, TokenEntry,
    TokenState, VaultProof, Word, SLOT_CONFIG_ID, SLOT_COUNT, SLOT_ROOT,
};
use ferrata_lez_wire::{header_actor, RecordState};
use ferrata_spectests::execkit::{account_rlp, enc, header_rlp, keccak, Trie};
use lee::{program::Program, Account, AccountId, Actor, PrivateKey, PublicAccountEvidence, PublicKey};
use lee_core::{
    account::ActorState,
    program::{PdaSeed, ReceiveInput},
};
use serde_json::Value;

use crate::{ferrata_account, Chain, TxReport};

/// The bridge program these tests run (`guests/bridge`).
pub const BRIDGE_ID: [u32; 8] = methods::FERRATA_BRIDGE_ID;
pub const BRIDGE_ELF: &[u8] = methods::FERRATA_BRIDGE_ELF;

pub fn bridge_program() -> Program {
    Program::new_unchecked(BRIDGE_ID, Cow::Borrowed(BRIDGE_ELF))
}

/// Where the genesis deployment of the bridge program lives: `bridgeHeader`.
pub fn bridge_account() -> AccountId {
    AccountId::from_builtin_program(BRIDGE_ID)
}

/// `holder(cid, account)`.
pub fn holder_of(program: AccountId, cid: &Hash, account: AccountId) -> Actor {
    bw::holder_actor(program, cid, account)
}

/// The seed of [`holder_of`].
pub fn holder_seed_of(cid: &Hash, account: &Hash) -> PdaSeed {
    bw::holder_seed(cid, account)
}

/// `nullifier(cid, chain, vault, N)`.
pub fn nullifier_of(program: AccountId, cid: &Hash, chain_id: u64, vault: &[u8; 20], n: &Hash) -> Actor {
    bw::nullifier_actor(program, cid, chain_id, vault, n)
}

/// The seed of [`nullifier_of`].
pub fn nullifier_seed_of(cid: &Hash, chain_id: u64, vault: &[u8; 20], n: &Hash) -> PdaSeed {
    bw::nullifier_seed(cid, chain_id, vault, n)
}

/// The bridge program's image id as 32 bytes (risc0's little-endian words).
pub fn bridge_image_id() -> Root {
    let mut out = [0u8; 32];
    for (i, w) in BRIDGE_ID.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

/// The handler a transition of the bridge program ran, for the tables.
pub fn handler_name(input: &ReceiveInput) -> String {
    let name = match borsh::from_slice::<BridgeMessage>(&input.message) {
        Ok(BridgeMessage::Burn { .. }) => "Burn",
        Ok(BridgeMessage::ImportRoot { .. }) => "ImportRoot",
        Ok(BridgeMessage::Sunset { .. }) => "Sunset",
        Ok(BridgeMessage::Echo { .. }) => "Echo",
        Ok(BridgeMessage::Init { .. }) => "Init",
        Ok(BridgeMessage::Claim { .. }) => "Claim",
        Ok(BridgeMessage::ClaimPrivately { .. }) => "ClaimPrivately",
        Ok(BridgeMessage::Read) => "Read",
        Ok(BridgeMessage::WriteRoot { .. }) => "WriteRoot",
        Ok(BridgeMessage::Spend { .. }) => "Spend",
        Ok(BridgeMessage::Mint { .. }) => "Mint",
        Ok(BridgeMessage::Register { .. }) => "Register",
        Ok(BridgeMessage::ImportFees { .. }) => "ImportFees",
        Ok(BridgeMessage::WriteToken { .. }) => "WriteToken",
        Ok(BridgeMessage::ExtendFees { .. }) => "ExtendFees",
        Ok(BridgeMessage::Issue { .. }) => "Issue",
        Ok(BridgeMessage::Admit { .. }) => "Admit",
        Ok(BridgeMessage::Freeze { .. }) => "Freeze",
        Ok(BridgeMessage::SetCap { .. }) => "SetCap",
        Ok(BridgeMessage::Tick { .. }) => "Tick",
        Ok(BridgeMessage::AttestRetired { .. }) => "AttestRetired",
        Ok(BridgeMessage::ProposeRoles { .. }) => "ProposeRoles",
        Ok(BridgeMessage::ApplyRoles { .. }) => "ApplyRoles",
        Ok(BridgeMessage::CancelRoles { .. }) => "CancelRoles",
        Ok(BridgeMessage::Pause { .. }) => "Pause",
        Ok(BridgeMessage::Cosigned { .. }) => "Cosigned",
        Ok(BridgeMessage::Transfer { .. }) => "Transfer",
        Ok(BridgeMessage::Credit { .. }) => "Credit",
        Ok(BridgeMessage::SetDepth { .. }) => "SetDepth",
        Ok(BridgeMessage::AttestListing { .. }) => "AttestListing",
        Ok(BridgeMessage::SetListing { .. }) => "SetListing",
        // Anything that is not one of the bridge's messages is a reply it waits for.
        Err(_) => "(reply)",
    };
    format!("bridge {name}")
}

/// The name of a bridge exit code.
pub fn code_name(c: u32) -> Option<&'static str> {
    code::ALL.iter().find(|(_, v)| u32::from(*v) == c).map(|(n, _)| *n)
}

/// [`crate::render`], with the bridge's codes named.
pub fn render(reports: &[TxReport]) -> String {
    let mut out = crate::render(reports);
    for (name, value) in code::ALL {
        out = out.replace(&format!("exit {value} (code {value})"), &format!("exit {value} ({name})"));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The synthetic Ethereum side
// ---------------------------------------------------------------------------------------------

/// The chain id of the vectors and of this world.
pub const CHAIN_ID: u64 = 1;
/// The synthetic attestation instance the records belong to.
pub const INSTANCE: Root = [0x1d; 32];
/// Native ETH.
pub const ETH: [u8; 20] = [0; 20];
pub const GWEI: u128 = 1_000_000_000;
/// The registered denominations: 0.1, 1 and 10 ETH.
pub const DENOMINATIONS: [u128; 3] = [100_000_000 * GWEI, 1_000_000_000 * GWEI, 10_000_000_000 * GWEI];
/// Vault B: another configuration's vault.
pub const VAULT_B: [u8; 20] = [0x22; 20];
/// ETH's LEZ unit: 1 gwei (LEZ keeps nine decimals of ETH).
pub const LEZ_UNIT: u128 = GWEI;
/// ETH's relay fees, fixed at registration: 0.1% of each denomination.
pub const RELAY_FEES: [u128; 3] = [100_000 * GWEI, 1_000_000 * GWEI, 10_000_000 * GWEI];
/// Vault A's second ETH fee schedule (from leaf 6): mint and burn fees 0.5%, checkpoint fees 0.1%.
pub const MINT_FEES_1: [u128; 3] = [500_000 * GWEI, 5_000_000 * GWEI, 50_000_000 * GWEI];
/// When the admin announced it (Ethereum seconds), and when it took effect: seven days later.
pub const ANNOUNCED_1: u64 = 1_699_000_000;
/// A six-decimal token registered on vault B beside ETH; its LEZ unit is one base unit.
pub const T2: [u8; 20] = [0x7e; 20];
pub const T2_DENOMINATIONS: [u128; 2] = [100_000_000, 1_000_000_000];
/// A token of vault A whose schedule 1 the vault wrote out of bounds (blocks 101 and 102 differ).
pub const T3: [u8; 20] = [0x73; 20];
/// Caps in value units (ETH: one unit per gwei, `unitWeight` 1): the floor, a token's and the
/// global defaults.
pub const FLOOR: CapParams = CapParams { capacity: 1_000_000_000, refill_per_second: 100_000 };
pub const TOKEN_CAP: CapParams = CapParams { capacity: 100_000_000_000, refill_per_second: 10_000_000 };
pub const GLOBAL_CAP: CapParams = CapParams { capacity: 200_000_000_000, refill_per_second: 10_000_000 };

/// The signing key of a role account (`ADMIN_SEED`, `FREEZE_SEED`, `OUTSIDER_SEED`).
pub fn role_key(seed: u8) -> PrivateKey {
    PrivateKey::try_new([seed; 32]).unwrap()
}
pub const ADMIN_SEED: u8 = 0x21;
pub const FREEZE_SEED: u8 = 0x22;
/// A signer that holds no role.
pub const OUTSIDER_SEED: u8 = 0x23;

pub fn role_account(seed: u8) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(&role_key(seed)))
}

/// The fee recipient and a relayer: public accounts without keys (they only receive).
pub fn fee_recipient() -> AccountId {
    AccountId::new([0xfe; 32])
}
pub fn relayer() -> AccountId {
    AccountId::new([0xec; 32])
}

/// ETH's registry entry: 18 decimals, LEZ unit 1 gwei, three denominations, 0.1% relay fees.
pub fn eth_entry() -> TokenEntry {
    TokenEntry {
        token: ETH,
        lez_definition: [0; 32],
        decimals: 18,
        lez_unit: LEZ_UNIT,
        unit_weight: 1,
        denominations: DENOMINATIONS.to_vec(),
        relay_fees: RELAY_FEES.to_vec(),
    }
}

/// T2's entry: six decimals, LEZ unit 1, two denominations, no relay fee.
pub fn t2_entry() -> TokenEntry {
    TokenEntry {
        token: T2,
        lez_definition: [0x72; 32],
        decimals: 6,
        lez_unit: 1,
        unit_weight: 1,
        denominations: T2_DENOMINATIONS.to_vec(),
        relay_fees: vec![0; 2],
    }
}

/// T3's entry (vault A): like ETH's.
pub fn t3_entry() -> TokenEntry {
    TokenEntry { token: T3, ..eth_entry() }
}

/// Schedule 0 of an entry: written at registration, no protocol fee.
pub fn schedule_zero(entry: &TokenEntry) -> FeeSchedule {
    let zero = vec![0; entry.denominations.len()];
    FeeSchedule {
        index: 0,
        from_leaf: 0,
        announced_at: 0,
        effective_at: 0,
        mint_fees: zero.clone(),
        burn_fees: zero.clone(),
        checkpoint_fees: zero,
    }
}

/// Vault A's ETH schedule 1: from leaf 6, announced seven days before it took effect.
pub fn eth_schedule_one() -> FeeSchedule {
    FeeSchedule {
        index: 1,
        from_leaf: 6,
        announced_at: ANNOUNCED_1,
        effective_at: ANNOUNCED_1 + bw::FEE_NOTICE_SECONDS,
        mint_fees: MINT_FEES_1.to_vec(),
        burn_fees: MINT_FEES_1.to_vec(),
        checkpoint_fees: RELAY_FEES.to_vec(),
    }
}

/// T3's schedule 1 as the vault wrote it in block 101 (one day of notice) and in block 102 (a 2%
/// mint fee).
pub fn t3_bad_schedules() -> [FeeSchedule; 2] {
    let short_notice = FeeSchedule { effective_at: ANNOUNCED_1 + 24 * 3600, ..eth_schedule_one() };
    let too_high = FeeSchedule { mint_fees: DENOMINATIONS.iter().map(|d| d / 50).collect(), ..eth_schedule_one() };
    [short_notice, too_high]
}

fn h32(v: &Value) -> Hash {
    hex::decode(v.as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn h20(v: &Value) -> [u8; 20] {
    hex::decode(v.as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

/// `bridge/vectors/inbound-v2.json`, compiled in (so a staged binary, `prove_bridge`, needs no
/// checkout beside it).
pub fn inbound_vectors() -> Value {
    serde_json::from_str(include_str!("../../../bridge/vectors/inbound-v2.json")).unwrap()
}

/// One deposit whose preimage is known.
#[derive(Clone, Debug)]
pub struct Deposit {
    pub leaf_index: u64,
    pub depositor: [u8; 20],
    pub token: [u8; 20],
    pub amount: [u8; 32],
    pub s: Hash,
    pub kind: DestKind,
    pub dest: Hash,
}

impl Deposit {
    /// The deposit a claim witness names.
    pub fn of(w: &ClaimWitness) -> Self {
        Deposit {
            leaf_index: w.leaf_index,
            depositor: w.depositor,
            token: w.token,
            amount: w.amount,
            s: w.s,
            kind: DestKind::from_byte(w.kind).unwrap_or(DestKind::Public),
            dest: w.dest,
        }
    }

    pub fn commitment(&self) -> Hash {
        commitment(&self.s, self.kind, &self.dest)
    }

    pub fn leaf(&self) -> Hash {
        deposit_leaf(self.leaf_index, &self.depositor, &self.token, &self.amount, &self.commitment())
    }

    /// `N` under a vault of chain `chain_id`.
    pub fn nullifier(&self, chain_id: u64, vault: &[u8; 20]) -> Hash {
        core_nullifier(&self.s, &deposit_id(&chain_id_u256(chain_id), vault, self.leaf_index, &self.leaf()))
    }
}

/// A vault: its address and every leaf it ever appended.
#[derive(Clone, Debug)]
pub struct Vault {
    pub address: [u8; 20],
    pub leaves: Vec<Hash>,
}

impl Vault {
    /// The tree after its first `n` leaves.
    pub fn tree(&self, n: u64) -> Tree {
        Tree::from_leaves(self.leaves[..n as usize].to_vec()).expect("fits")
    }

    pub fn root(&self, n: u64) -> Hash {
        self.tree(n).root()
    }

    /// The claim witness of `d` under the root at leaf count `n`.
    pub fn witness(&self, d: &Deposit, n: u64) -> ClaimWitness {
        let path = self.tree(n).path(d.leaf_index).expect("the leaf is in the tree");
        assert_eq!(u64::from(path.directions), d.leaf_index);
        ClaimWitness {
            depositor: d.depositor,
            token: d.token,
            amount: d.amount,
            leaf_index: d.leaf_index,
            path: path.siblings.to_vec(),
            s: d.s,
            kind: d.kind as u8,
            dest: d.dest,
        }
    }
}

/// What one vault's storage holds in one block: slots 0–2 and the registry and fee slots.
#[derive(Clone, Debug)]
pub struct VaultSlots {
    pub address: [u8; 20],
    pub root: Hash,
    pub count: u64,
    pub config_id: Hash,
    pub extra: Vec<(Word, Hash)>,
}

/// The registry slots of `entry` with schedule 0 (as a vault writes them at registration), and
/// `more` schedules.
pub fn registry_slots(entry: &TokenEntry, more: &[FeeSchedule]) -> Vec<(Word, Hash)> {
    let mut slots = vec![(registry_slot(&entry.token), entry.hash())];
    for s in std::iter::once(&schedule_zero(entry)).chain(more) {
        slots.push((fee_slot(&entry.token, s.index), s.hash(&entry.token)));
    }
    slots
}

/// A synthetic execution block: its state trie, the vaults' storage tries, and its record; for a
/// record in the Gloas layout (which binds the block hash only), the block's RLP header, which
/// the proofs carry in front of the account's nodes (`bridge-wire::with_header`).
pub struct Block {
    pub slot: u64,
    pub record: HeaderRecord,
    pub state: Trie,
    pub storage: Vec<([u8; 20], Trie)>,
    pub header: Option<Vec<u8>>,
}

/// The state trie holding `vaults` (slots 0, 1, 2 and their `extra` slots) among 40 other
/// accounts, and the vaults' storage tries.
fn vault_state(vaults: &[VaultSlots]) -> (Trie, Vec<([u8; 20], Trie)>) {
    let mut accounts = Vec::new();
    let mut storage = Vec::new();
    for v in vaults {
        let base = [
            (keccak(&SLOT_ROOT).to_vec(), enc::uint(&v.root)),
            (keccak(&SLOT_COUNT).to_vec(), enc::uint(&v.count.to_be_bytes())),
            (keccak(&SLOT_CONFIG_ID).to_vec(), enc::uint(&v.config_id)),
        ];
        let trie = Trie::new(base.into_iter().chain(v.extra.iter().map(|(k, x)| (keccak(k).to_vec(), enc::uint(x)))));
        accounts.push((v.address, account_rlp(1, 0, trie.root(), keccak(b"vault code"))));
        storage.push((v.address, trie));
    }
    for i in 0x40..0x68u8 {
        accounts.push(([i; 20], account_rlp(u64::from(i), 1_000 + u128::from(i), keccak(&[0x80]), keccak(b""))));
    }
    (Trie::new(accounts.iter().map(|(a, rlp)| (keccak(a).to_vec(), rlp.clone()))), storage)
}

/// The state trie holding the vault at `address` with exactly `slots` (a real vault's own storage:
/// every slot it wrote, zero values left out, as Ethereum's state does) among the same 40 other
/// accounts, and the vault's storage trie.
fn storage_state(address: [u8; 20], slots: &[(Word, Hash)]) -> (Trie, Vec<([u8; 20], Trie)>) {
    let trie =
        Trie::new(slots.iter().filter(|(_, x)| *x != [0u8; 32]).map(|(k, x)| (keccak(k).to_vec(), enc::uint(x))));
    let mut accounts = vec![(address, account_rlp(1, 0, trie.root(), keccak(b"vault code")))];
    for i in 0x40..0x68u8 {
        accounts.push(([i; 20], account_rlp(u64::from(i), 1_000 + u128::from(i), keccak(&[0x80]), keccak(b""))));
    }
    (Trie::new(accounts.iter().map(|(a, rlp)| (keccak(a).to_vec(), rlp.clone()))), vec![(address, trie)])
}

impl Block {
    /// A block numbered `number`, at beacon slot `slot`, holding `vaults` among 40 other accounts,
    /// under a record in the payload layout.
    pub fn new(slot: u64, number: u64, vaults: &[VaultSlots]) -> Self {
        Self::payload(slot, number, vault_state(vaults))
    }

    /// A block holding the vault at `address` with exactly the storage `slots` it wrote
    /// (`vault/test/RegistryStorage.t.sol` exports FerrataVault's own), under a payload record.
    pub fn with_storage(slot: u64, number: u64, address: [u8; 20], slots: &[(Word, Hash)]) -> Self {
        Self::payload(slot, number, storage_state(address, slots))
    }

    fn payload(slot: u64, number: u64, (state, storage): (Trie, Vec<([u8; 20], Trie)>)) -> Self {
        let record = HeaderRecord {
            beacon: BeaconBlockHeader { slot, ..Default::default() },
            execution_root: [3; 32],
            execution: ExecutionSummary::Payload {
                block_number: number,
                block_hash: keccak(&number.to_be_bytes()),
                parent_hash: keccak(&(number - 1).to_be_bytes()),
                state_root: state.root(),
                receipts_root: [7; 32],
                timestamp: RECORD_TIME_BASE + 12 * number,
            },
        };
        Self { slot, record, state, storage, header: None }
    }

    /// The same block with its execution timestamp set to `seconds` (a payload record).
    pub fn at(mut self, seconds: u64) -> Self {
        let ExecutionSummary::Payload { timestamp, .. } = &mut self.record.execution else {
            panic!("only a payload record carries its timestamp");
        };
        *timestamp = seconds;
        self
    }

    /// The execution block's timestamp in milliseconds: the attested time a `Tick` reading this
    /// block's record moves a gate to (a Gloas record's, from its header).
    pub fn time_ms(&self) -> u64 {
        match (&self.record.execution, &self.header) {
            (ExecutionSummary::Payload { timestamp, .. }, _) => timestamp * 1000,
            (ExecutionSummary::BlockHash(_), Some(header)) => {
                ferrata_core::exec::decode_header(header).expect("a header").timestamp * 1000
            }
            _ => panic!("a record without an execution block"),
        }
    }

    /// What a `Tick` or `Freeze` names to read this block's record.
    pub fn time_record(&self) -> TimeRecord {
        TimeRecord { record_slot: self.slot, header: self.header.clone() }
    }

    /// A block at beacon slot `slot` holding `vaults`, under a record in the Gloas layout: a real
    /// Sepolia header (the replay fixture's bound block, `harness::vault::synthetic`'s) with the
    /// synthetic state root, whose hash the record binds.
    pub fn gloas(slot: u64, vaults: &[VaultSlots]) -> Self {
        let (state, storage) = vault_state(vaults);
        let served: Value = serde_json::from_slice(&crate::sepolia::gloas_file("block.json")).unwrap();
        let mut block = served["result"].clone();
        block["stateRoot"] = Value::String(format!("0x{}", hex::encode(state.root())));
        let header = header_rlp(&block);
        let record = HeaderRecord {
            beacon: BeaconBlockHeader { slot, ..Default::default() },
            execution_root: [0; 32],
            execution: ExecutionSummary::BlockHash(keccak(&header)),
        };
        Self { slot, record, state, storage, header: Some(header) }
    }

    /// The vault's account proof, with the header in front when the record binds a hash only.
    fn account_proof(&self, vault: &[u8; 20]) -> Vec<Vec<u8>> {
        let nodes = self.state.proof(&keccak(vault));
        match &self.header {
            Some(header) => bw::with_header(header.clone(), nodes),
            None => nodes,
        }
    }

    fn storage_of(&self, vault: &[u8; 20]) -> &Trie {
        &self.storage.iter().find(|(a, _)| a == vault).expect("a vault of this block").1
    }

    pub fn import_proof(&self, vault: &[u8; 20]) -> ImportProof {
        let storage = self.storage_of(vault);
        ImportProof {
            account: self.account_proof(vault),
            root_slot: storage.proof(&keccak(&SLOT_ROOT)),
            count_slot: storage.proof(&keccak(&SLOT_COUNT)),
        }
    }

    /// The proofs of `vault`'s account and `slots` (an absent slot gets its exclusion proof).
    pub fn vault_proof(&self, vault: &[u8; 20], slots: &[Word]) -> VaultProof {
        let storage = self.storage_of(vault);
        VaultProof {
            account: self.account_proof(vault),
            slots: slots.iter().map(|s| storage.proof(&keccak(s))).collect(),
        }
    }

    pub fn pairing_proof(&self, vault: &[u8; 20]) -> PairingProof {
        PairingProof {
            account: self.account_proof(vault),
            config_slot: self.storage_of(vault).proof(&keccak(&SLOT_CONFIG_ID)),
        }
    }

    /// The attestation program's `header(INSTANCE, slot)` account holding this block's record.
    fn record_account(&self) -> (AccountId, Account) {
        let state =
            RecordState { instance_id: INSTANCE, chain_id: CHAIN_ID, record: self.record.encode(), root_id: INSTANCE };
        let actor = header_actor(ferrata_account(), &INSTANCE, self.slot);
        let account =
            Account::default().with_actor_state(ferrata_account(), ActorState::from(borsh::to_vec(&state).unwrap()));
        (actor.account_id, account)
    }
}

// ---------------------------------------------------------------------------------------------
// The world
// ---------------------------------------------------------------------------------------------

/// Execution timestamps of the synthetic blocks: block `number` is at `RECORD_TIME_BASE + 12 ×
/// number` seconds, so blocks 100–102 are 40 minutes behind the world's LEZ clock (a record reaches
/// LEZ only after its block is final, minutes later).
pub const RECORD_TIME_BASE: u64 = 1_699_996_400;
/// How far the world's LEZ clock runs ahead of a record that [`World::finalize`] adds: two epochs.
pub const FINALITY_LAG_MS: u64 = 2 * 32 * 12 * 1000;

/// The roles and caps of this world's configurations.
pub fn controls() -> Controls {
    Controls {
        admin: *role_account(ADMIN_SEED).value(),
        freeze_authority: *role_account(FREEZE_SEED).value(),
        fee_recipient: *fee_recipient().value(),
        floor: FLOOR,
        token_cap: TOKEN_CAP,
        global_cap: GLOBAL_CAP,
    }
}

/// The configuration of `vault` with `controls`, reading Ethereum through the attestation
/// program's genesis deployment and the synthetic instance.
pub fn config(vault: [u8; 20], controls: Controls) -> BridgeConfig {
    BridgeConfig {
        bedrock_genesis_id: [0x0b; 32],
        channel_id: [0x01; 32],
        bridge_image_id: bridge_image_id(),
        attestation: ferrata_account(),
        instance_id: INSTANCE,
        evm_chain_id: CHAIN_ID,
        vault,
        controls,
    }
}

/// The chain with the bridge program, the three synthetic records and two vaults' deposits.
pub struct World {
    pub chain: Chain,
    pub program: AccountId,
    /// The configuration of vault A, of vault B, and a second one of vault A (another fee recipient).
    pub config_a: BridgeConfig,
    pub config_b: BridgeConfig,
    pub config_a2: BridgeConfig,
    pub cid_a: Hash,
    pub cid_b: Hash,
    pub cid_a2: Hash,
    pub vault_a: Vault,
    pub vault_b: Vault,
    /// The shared vector's claim (vault A, leaf 2, public destination `vector_dest`).
    pub vector: Deposit,
    pub vector_n: Hash,
    pub vector_dest: AccountId,
    /// Vault A, leaf 5: 1 ETH to a private account (seed `private_seed`), private kind.
    pub private_deposit: Deposit,
    pub private_seed: u8,
    /// Vault A, leaf 6: 10 ETH to the payer, public kind (fee schedule 1).
    pub payer_deposit: Deposit,
    /// Vault A, leaf 7: 0.5 ETH to the payer (not a denomination; a vault would refuse it).
    pub odd_deposit: Deposit,
    /// Vault B, leaf 3: 1 ETH to the payer.
    pub deposit_b: Deposit,
    /// Vault B, leaf 5: 1,000 units of T2 to the payer.
    pub deposit_t2: Deposit,
    /// S1: A has 5 leaves; S2: A has 8; S3: A's root is that of 8 leaves but its count says 6.
    pub blocks: Vec<Block>,
    pub time: u64,
    /// Every public transaction submitted, as `borsh(LeeTransaction)`: what a block carries.
    pub txs: Vec<(String, Vec<u8>)>,
}

/// The private account the private deposit is for (`Chain::add_private_account` keys).
pub fn private_account_id(seed: u8) -> AccountId {
    lee::test_utils::TestPrivateKeys {
        ask: lee_core::AuthorizationSecretKey([seed; 32]),
        d: [seed.wrapping_add(1); 32],
        z: [seed.wrapping_add(2); 32],
    }
    .account_id()
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        let mut chain = Chain::new();
        let program = bridge_account();
        let v = inbound_vectors();
        let claim = &v["claim"];
        let vault_address = h20(&claim["vault"]);
        assert_eq!(h32(&claim["evm_chain_id"]), chain_id_u256(CHAIN_ID));
        let config_a = config(vault_address, controls());
        let config_b = config(VAULT_B, controls());
        let config_a2 = config(vault_address, Controls { fee_recipient: [0xfd; 32], ..controls() });
        let (cid_a, cid_b, cid_a2) = (config_a.id(program), config_b.id(program), config_a2.id(program));

        // Vault A: the vector's five leaves, then three of this harness's own.
        let mut leaves: Vec<Hash> = claim["leaves"].as_array().unwrap().iter().map(h32).collect();
        assert_eq!(Tree::from_leaves(leaves.clone()).unwrap().root(), h32(&claim["root"]), "the vector's root");
        let vector = Deposit {
            leaf_index: claim["leaf_index"].as_u64().unwrap(),
            depositor: h20(&claim["depositor"]),
            token: h20(&claim["token"]),
            amount: h32(&claim["amount"]),
            s: h32(&claim["s"]),
            kind: DestKind::from_byte(claim["kind"].as_u64().unwrap() as u8).unwrap(),
            dest: h32(&claim["dest"]),
        };
        assert_eq!(vector.leaf(), leaves[vector.leaf_index as usize], "the vector's leaf");
        let vector_n = h32(&claim["N"]);
        let depositor = [0xd1; 20];
        let keys = BridgeKeys::from_bridge_seed(&[0x5a; 32]);
        let private_seed = 0x31;
        let own = |leaf_index: u64, j: u64, amount: u128, kind: DestKind, dest: Hash| Deposit {
            leaf_index,
            depositor,
            token: ETH,
            amount: bw::word_of_u128(amount),
            s: deposit_secret(&keys.auth, &chain_id_u256(CHAIN_ID), &vault_address, &depositor, j),
            kind,
            dest,
        };
        let private_deposit = own(5, 0, DENOMINATIONS[1], DestKind::Private, *private_account_id(private_seed).value());
        let payer_deposit = own(6, 1, DENOMINATIONS[2], DestKind::Public, *chain.payer.value());
        let odd_deposit = own(7, 2, DENOMINATIONS[1] / 2, DestKind::Public, *chain.payer.value());
        for d in [&private_deposit, &payer_deposit, &odd_deposit] {
            assert_eq!(leaves.len() as u64, d.leaf_index);
            leaves.push(d.leaf());
        }
        let vault_a = Vault { address: vault_address, leaves };

        // Vault B: eight leaves, the fourth a deposit of 1 ETH to the payer, the sixth one of
        // 1,000 units of T2.
        let deposit_b = Deposit {
            leaf_index: 3,
            depositor,
            token: ETH,
            amount: bw::word_of_u128(DENOMINATIONS[1]),
            s: deposit_secret(&keys.auth, &chain_id_u256(CHAIN_ID), &VAULT_B, &depositor, 0),
            kind: DestKind::Public,
            dest: *chain.payer.value(),
        };
        let deposit_t2 = Deposit {
            leaf_index: 5,
            token: T2,
            amount: bw::word_of_u128(T2_DENOMINATIONS[1]),
            s: deposit_secret(&keys.auth, &chain_id_u256(CHAIN_ID), &VAULT_B, &depositor, 1),
            ..deposit_b.clone()
        };
        let leaves_b = (0..8u8)
            .map(|i| match u64::from(i) {
                3 => deposit_b.leaf(),
                5 => deposit_t2.leaf(),
                _ => keccak(&[0xb0, i]),
            })
            .collect();
        let vault_b = Vault { address: VAULT_B, leaves: leaves_b };

        // The vaults' registries: vault B has ETH and T2 (schedule 0 only); vault A has ETH, whose
        // schedule 1 took effect at leaf 6 (so it is in blocks 101 and 102, not 100), and T3, whose
        // schedule 1 is out of bounds (a vault that wrote it: one value in each block).
        let mut registry_b = registry_slots(&eth_entry(), &[]);
        registry_b.extend(registry_slots(&t2_entry(), &[]));
        let slots_b =
            VaultSlots { address: VAULT_B, root: vault_b.root(8), count: 8, config_id: cid_b, extra: registry_b };
        let [t3_notice, t3_fee] = t3_bad_schedules();
        let slots_a = |root_n: u64, count: u64, eth_more: &[FeeSchedule], t3_more: &[FeeSchedule]| {
            let mut extra = registry_slots(&eth_entry(), eth_more);
            extra.extend(registry_slots(&t3_entry(), t3_more));
            VaultSlots { address: vault_address, root: vault_a.root(root_n), count, config_id: cid_a, extra }
        };
        let one = [eth_schedule_one()];
        let blocks = vec![
            Block::new(9_000_000, 100, &[slots_a(5, 5, &[], &[]), slots_b.clone()]),
            Block::new(9_000_032, 101, &[slots_a(8, 8, &one, &[t3_notice]), slots_b.clone()]),
            Block::new(9_000_064, 102, &[slots_a(8, 6, &one, &[t3_fee]), slots_b]),
        ];

        // Genesis: the bridge program, the records, and the public accounts that receive mints or
        // sign as roles (a public account must exist, or be evidenced, to be admitted).
        let vector_dest = AccountId::new(vector.dest);
        let mut accounts: Vec<(AccountId, Account)> = blocks.iter().map(Block::record_account).collect();
        for id in [vector_dest, fee_recipient(), relayer()]
            .into_iter()
            .chain([ADMIN_SEED, FREEZE_SEED, OUTSIDER_SEED].map(role_account))
        {
            accounts.push((id, Account::default()));
        }
        let state = std::mem::take(&mut chain.state);
        chain.state = state.with_public_accounts(accounts).with_genesis_programs([(bridge_program(), true)]);
        Self {
            chain,
            program,
            config_a,
            config_b,
            config_a2,
            cid_a,
            cid_b,
            cid_a2,
            vault_a,
            vault_b,
            vector,
            vector_n,
            vector_dest,
            private_deposit,
            private_seed,
            payer_deposit,
            odd_deposit,
            deposit_b,
            deposit_t2,
            blocks,
            time: 1_700_000_000_000,
            txs: Vec::new(),
        }
    }

    /// A later finalized block at Ethereum time `seconds` (no vault in its state), its record
    /// placed as the attestation program writes it, and the world's LEZ clock moved to
    /// [`FINALITY_LAG_MS`] after it if it was behind. Returns its index in `blocks`.
    pub fn finalize(&mut self, seconds: u64) -> usize {
        let index = self.record_at(seconds);
        self.time = self.time.max(seconds * 1000 + FINALITY_LAG_MS);
        index
    }

    /// A record of a later block at Ethereum time `seconds`, the LEZ clock left where it is (a
    /// record ahead of it is one only a captured sync committee could have had written).
    pub fn record_at(&mut self, seconds: u64) -> usize {
        let slot = self.next_slot();
        let number = 1_000 + self.blocks.len() as u64;
        self.add_block(Block::new(slot, number, &[]).at(seconds))
    }

    /// A beacon slot after every recorded one.
    pub fn next_slot(&self) -> u64 {
        self.blocks.iter().map(|b| b.slot).max().unwrap_or(9_000_000) + 32
    }

    /// The newest execution timestamp among the recorded blocks, in seconds.
    pub fn newest_record_seconds(&self) -> u64 {
        self.blocks.iter().map(|b| b.time_ms() / 1000).max().expect("a record")
    }

    /// Add `block` after genesis: its record placed at `header(INSTANCE, slot)` as the
    /// attestation program writes it. Returns its index in `blocks`.
    pub fn add_block(&mut self, block: Block) -> usize {
        let (id, account) = block.record_account();
        self.chain.state.force_insert_account(id, account);
        self.blocks.push(block);
        self.blocks.len() - 1
    }

    fn state(&self, actor: Actor) -> Option<BridgeState> {
        let state = self.chain.actor_state(actor);
        (!state.is_empty()).then(|| borsh::from_slice(&state).expect("a bridge state"))
    }

    pub fn root_state(&self, cid: &Hash, n: u64) -> Option<RootState> {
        match self.state(root_actor(self.program, cid, n)) {
            Some(BridgeState::Root(root)) => Some(root),
            None => None,
            other => panic!("not a root: {other:?}"),
        }
    }

    pub fn token_state(&self, cid: &Hash, token: &[u8; 20]) -> Option<TokenState> {
        match self.state(token_actor(self.program, cid, token)) {
            Some(BridgeState::Token(state)) => Some(state),
            None => None,
            other => panic!("not a token: {other:?}"),
        }
    }

    pub fn config_state(&self, cid: &Hash) -> Option<bw::ConfigState> {
        match self.state(config_actor(self.program, cid)) {
            Some(BridgeState::Config(state)) => Some(state),
            None => None,
            other => panic!("not a configuration: {other:?}"),
        }
    }

    /// The public holdings of `account` (under `cid`), at `holder(account)` (`holder(cid, account)` of the second
    /// build).
    pub fn holder(&self, cid: &Hash, account: AccountId) -> Option<HolderState> {
        match self.state(holder_of(self.program, cid, account)) {
            Some(BridgeState::Holder(holder)) => Some(holder),
            None => None,
            other => panic!("not a holder: {other:?}"),
        }
    }

    /// `N` of `vault` spent (under `cid`), at `nullifier(chain, vault, N)` (`nullifier(cid, chain, vault, N)` of the
    /// second build).
    pub fn nullifier(&self, cid: &Hash, vault: &[u8; 20], n: &Hash) -> Option<NullifierState> {
        match self.state(nullifier_of(self.program, cid, CHAIN_ID, vault, n)) {
            Some(BridgeState::Nullifier(spent)) => Some(spent),
            None => None,
            other => panic!("not a nullifier: {other:?}"),
        }
    }

    /// The balance of `token` that `account` holds under `cid`.
    pub fn balance_of(&self, account: AccountId, cid: &Hash, token: &[u8; 20]) -> u128 {
        self.holder(cid, account).and_then(|h| h.holding(cid, token).map(|h| h.amount)).unwrap_or(0)
    }

    /// The ETH balance `account` holds under `cid`.
    pub fn balance(&self, account: AccountId, cid: &Hash) -> u128 {
        self.balance_of(account, cid, &ETH)
    }

    /// Admission evidence for a PDA root actor that does not exist yet.
    fn evidence(&self, actor: Actor, seed: PdaSeed) -> Vec<PublicAccountEvidence> {
        if self.chain.state.get_account_by_id_ref(actor.account_id).is_some() {
            vec![]
        } else {
            vec![PublicAccountEvidence::Pda { program: self.program, seed }]
        }
    }

    /// Sign (by the payer and `signers`), settle and keep one public transaction whose root is
    /// `message` at `root`.
    pub fn submit_signed(
        &mut self,
        label: &str,
        root: Actor,
        message: &BridgeMessage,
        actors: Vec<Actor>,
        evidence: Vec<PublicAccountEvidence>,
        signers: &[&PrivateKey],
    ) -> TxReport {
        let message = borsh::to_vec(message).unwrap();
        let report = self.chain.submit_signed(label, root, message, actors, evidence, self.time, signers);
        assert_eq!(
            report.tx_bytes,
            self.chain.last_tx.len(),
            "{label}: the kept transaction is the settled one's size"
        );
        self.txs.push((label.to_string(), self.chain.last_tx.clone()));
        report
    }

    /// Sign (by the payer), settle and keep one public transaction whose root is `message` at `root`.
    pub fn submit(
        &mut self,
        label: &str,
        root: Actor,
        message: &BridgeMessage,
        actors: Vec<Actor>,
        evidence: Vec<PublicAccountEvidence>,
    ) -> TxReport {
        self.submit_signed(label, root, message, actors, evidence, &[])
    }

    /// `Init` of `config`, attesting its vault's slot 2 under block `block`.
    pub fn init(&mut self, label: &str, config: &BridgeConfig, block: usize) -> TxReport {
        let cid = config.id(self.program);
        let b = &self.blocks[block];
        let message =
            BridgeMessage::Init { config: config.clone(), record_slot: b.slot, proof: b.pairing_proof(&config.vault) };
        let root = config_actor(self.program, &cid);
        let header = header_actor(ferrata_account(), &INSTANCE, b.slot);
        let evidence = self.evidence(root, config_seed(&cid));
        self.submit(label, root, &message, vec![root, header], evidence)
    }

    /// `ImportRoot` for configuration `cid` under block `block`, claiming `(root, n)`, with the
    /// proofs of `vault`'s slots in that block.
    pub fn import(&mut self, label: &str, cid: &Hash, block: usize, root: Hash, n: u64, vault: &[u8; 20]) -> TxReport {
        let b = &self.blocks[block];
        let message = BridgeMessage::ImportRoot {
            config_id: *cid,
            record_slot: b.slot,
            root,
            leaf_count: n,
            proof: b.import_proof(vault),
        };
        let config = config_actor(self.program, cid);
        let header = header_actor(ferrata_account(), &INSTANCE, b.slot);
        let evidence = self.evidence(config, config_seed(cid));
        self.submit(label, config, &message, vec![config, header, root_actor(self.program, cid, n)], evidence)
    }

    /// `Register` of `entry` (with schedule 0) for configuration `cid` under block `block`, with the
    /// proofs of `vault`'s registry slots.
    pub fn register(
        &mut self,
        label: &str,
        cid: &Hash,
        block: usize,
        entry: &TokenEntry,
        vault: &[u8; 20],
    ) -> TxReport {
        self.register_with(label, cid, block, entry, &schedule_zero(entry), vault)
    }

    pub fn register_with(
        &mut self,
        label: &str,
        cid: &Hash,
        block: usize,
        entry: &TokenEntry,
        schedule: &FeeSchedule,
        vault: &[u8; 20],
    ) -> TxReport {
        let b = &self.blocks[block];
        let message = BridgeMessage::Register {
            config_id: *cid,
            entry: entry.clone(),
            schedule: schedule.clone(),
            record_slot: b.slot,
            proof: b.vault_proof(vault, &register_slots(&entry.token)),
        };
        let config = config_actor(self.program, cid);
        let header = header_actor(ferrata_account(), &INSTANCE, b.slot);
        let token = token_actor(self.program, cid, &entry.token);
        let evidence = self.evidence(config, config_seed(cid));
        self.submit(label, config, &message, vec![config, header, token], evidence)
    }

    /// `ImportFees` of `token`'s schedule `index` (`None`: its absence) under block `block`.
    #[allow(clippy::too_many_arguments)]
    pub fn import_fees(
        &mut self,
        label: &str,
        cid: &Hash,
        block: usize,
        token: [u8; 20],
        index: u64,
        schedule: Option<FeeSchedule>,
        vault: &[u8; 20],
    ) -> TxReport {
        let b = &self.blocks[block];
        let message = BridgeMessage::ImportFees {
            config_id: *cid,
            token,
            index,
            schedule,
            record_slot: b.slot,
            proof: b.vault_proof(vault, &bw::fees_slots(&token, index)),
        };
        let config = config_actor(self.program, cid);
        let header = header_actor(ferrata_account(), &INSTANCE, b.slot);
        let evidence = self.evidence(config, config_seed(cid));
        self.submit(label, config, &message, vec![config, header, token_actor(self.program, cid, &token)], evidence)
    }

    /// The fee schedule range a wallet names for a claim of `leaf` of `token`: the schedule the
    /// leaf falls under and the widest range the token's state allows (the whole known range).
    pub fn range(&self, cid: &Hash, token: &[u8; 20], leaf: u64) -> ScheduleRange {
        match self.token_state(cid, token) {
            None => ScheduleRange { index: 0, from_leaf: 0, until_leaf: u64::MAX },
            Some(t) => {
                let j = t.schedules.iter().rposition(|s| s.from_leaf <= leaf).unwrap_or(0);
                let until = t.schedules.get(j + 1).map_or(t.known_until, |s| s.from_leaf);
                ScheduleRange { index: j as u64, from_leaf: t.schedules[j].from_leaf, until_leaf: until }
            }
        }
    }

    /// The public actors of a claim beyond its first: the token, the configuration, the
    /// nullifier, the public destination, the fee recipient and the relayer (each once).
    pub fn claim_actors(
        &self,
        cid: &Hash,
        token: &[u8; 20],
        nullifier: Actor,
        dest: Option<AccountId>,
        relayer: Option<Hash>,
    ) -> Vec<Actor> {
        let mut actors = vec![token_actor(self.program, cid, token), config_actor(self.program, cid), nullifier];
        let accounts =
            dest.into_iter().chain([fee_recipient(), relayer.map(AccountId::new).unwrap_or_else(fee_recipient)]);
        for account in accounts {
            let actor = holder_of(self.program, cid, account);
            if !actors.contains(&actor) {
                actors.push(actor);
            }
        }
        actors
    }

    /// The plaintext `Claim` of `witness` at `root(cid, n)` (the nullifier of `vault`'s deposit),
    /// with the wallet's range and the world's relayer.
    pub fn claim(&mut self, label: &str, cid: &Hash, n: u64, vault: &[u8; 20], witness: ClaimWitness) -> TxReport {
        let range = self.range(cid, &witness.token, witness.leaf_index);
        self.claim_with(label, cid, n, vault, witness, range, Some(*relayer().value()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn claim_with(
        &mut self,
        label: &str,
        cid: &Hash,
        n: u64,
        vault: &[u8; 20],
        witness: ClaimWitness,
        range: ScheduleRange,
        relayer: Option<Hash>,
    ) -> TxReport {
        let root = root_actor(self.program, cid, n);
        let n_d = Deposit::of(&witness).nullifier(CHAIN_ID, vault);
        let nullifier = nullifier_of(self.program, cid, CHAIN_ID, vault, &n_d);
        let mut actors = vec![root];
        actors.extend(self.claim_actors(cid, &witness.token, nullifier, Some(AccountId::new(witness.dest)), relayer));
        let evidence = self.evidence(root, root_seed(cid, n));
        let message = BridgeMessage::Claim { config_id: *cid, leaf_count: n, witness, range, relayer };
        self.submit(label, root, &message, actors, evidence)
    }

    /// A `ClaimPrivately` of `witness` under `root(cid, n)`, with the wallet's range and the
    /// world's relayer.
    pub fn claim_privately(&self, cid: &Hash, n: u64, witness: ClaimWitness) -> BridgeMessage {
        let range = self.range(cid, &witness.token, witness.leaf_index);
        let relayer = Some(*relayer().value());
        BridgeMessage::ClaimPrivately { config_id: *cid, leaf_count: n, witness, range, relayer }
    }

    /// The public actors of a private claim of `deposit` (of `vault`) under `root(cid, n)`.
    pub fn private_claim_actors(&self, cid: &Hash, n: u64, vault: &[u8; 20], deposit: &Deposit) -> Vec<Actor> {
        let nullifier = nullifier_of(self.program, cid, CHAIN_ID, vault, &deposit.nullifier(CHAIN_ID, vault));
        let dest = (deposit.kind == DestKind::Public).then(|| AccountId::new(deposit.dest));
        let mut actors = vec![root_actor(self.program, cid, n)];
        actors.extend(self.claim_actors(cid, &deposit.token, nullifier, dest, Some(*relayer().value())));
        actors
    }

    /// The actors of a public holder's burn of `note` under `cid`: its own actor (the root it
    /// signs), its `holder(account)`, the token, the configuration, the record.
    pub fn public_burn_actors(&self, account: AccountId, cid: &Hash, note: &bw::Note) -> Vec<Actor> {
        let mut actors = self.burn_actors(own_actor(self.program, account), cid, note);
        actors.insert(1, holder_of(self.program, cid, account));
        actors
    }

    /// The actors of a burn of `note` under `cid` at `holder`: the holder, the token, the
    /// configuration, the record.
    pub fn burn_actors(&self, holder: Actor, cid: &Hash, note: &bw::Note) -> Vec<Actor> {
        vec![
            holder,
            token_actor(self.program, cid, &note.token),
            config_actor(self.program, cid),
            bw::burn_actor(self.program, cid, &note.r),
        ]
    }

    /// Any bridge message as the root of a transaction at `root`.
    pub fn send(&mut self, label: &str, root: Actor, message: &BridgeMessage, actors: Vec<Actor>) -> TxReport {
        self.submit(label, root, message, actors, vec![])
    }

    /// A role message (`Sunset`, `Freeze`, `SetCap`) as a top-level call at the actor of the
    /// account of `seed`, signed by it, forwarded to `target` (a `Freeze` also mentions the record
    /// it names).
    pub fn role(&mut self, label: &str, seed: u8, message: &BridgeMessage, target: Actor) -> TxReport {
        let root = own_actor(self.program, role_account(seed));
        let key = role_key(seed);
        let mut actors = vec![root];
        // Every role message passes `config(cid)`, which checks the role against its live roles.
        let config = match message {
            BridgeMessage::Sunset { config_id }
            | BridgeMessage::Freeze { config_id, .. }
            | BridgeMessage::SetCap { config_id, .. }
            | BridgeMessage::SetDepth { config_id, .. }
            | BridgeMessage::ProposeRoles { config_id, .. }
            | BridgeMessage::CancelRoles { config_id }
            | BridgeMessage::Pause { config_id, .. } => Some(config_actor(self.program, config_id)),
            _ => None,
        };
        for actor in config.into_iter().chain([target]) {
            if !actors.contains(&actor) {
                actors.push(actor);
            }
        }
        if let BridgeMessage::Freeze { record, .. }
        | BridgeMessage::Pause { record, .. }
        | BridgeMessage::ProposeRoles { record, .. } = message
        {
            actors.push(header_actor(ferrata_account(), &INSTANCE, record.record_slot));
        }
        self.submit_signed(label, root, message, actors, vec![], &[&key])
    }

    /// A `Cosigned` message naming the accounts of `seeds` as its signers, signed by the accounts
    /// of `signing` (the payer signs every transaction too): a top-level call at the first
    /// signer's own actor, through the signers' own actors to `config(cid)` (and the token and the
    /// record a freeze names).
    pub fn cosign(
        &mut self,
        label: &str,
        seeds: &[u8],
        signing: &[u8],
        cid: &Hash,
        action: bw::CosignedAction,
    ) -> TxReport {
        let signers: Vec<Hash> = seeds.iter().map(|s| *role_account(*s).value()).collect();
        let keys: Vec<PrivateKey> = signing.iter().map(|s| role_key(*s)).collect();
        let mut actors: Vec<Actor> = Vec::new();
        for actor in seeds.iter().map(|s| own_actor(self.program, role_account(*s))) {
            if !actors.contains(&actor) {
                actors.push(actor);
            }
        }
        actors.push(config_actor(self.program, cid));
        if let bw::CosignedAction::Freeze { token, record, .. } = &action {
            if let Some(t) = token {
                actors.push(token_actor(self.program, cid, t));
            }
            actors.push(header_actor(ferrata_account(), &INSTANCE, record.record_slot));
        }
        let root = actors[0];
        let message = BridgeMessage::Cosigned { config_id: *cid, signers, action };
        let key_refs: Vec<&PrivateKey> = keys.iter().collect();
        self.submit_signed(label, root, &message, actors, vec![], &key_refs)
    }

    /// `AttestRetired` for `cid`: the vault's `retired` slot in block `block`.
    pub fn attest_retired(&mut self, label: &str, cid: &Hash, block: usize, vault: &[u8; 20]) -> TxReport {
        let b = &self.blocks[block];
        let message = BridgeMessage::AttestRetired {
            config_id: *cid,
            record_slot: b.slot,
            proof: b.vault_proof(vault, &[bw::SLOT_RETIRED]),
        };
        let root = config_actor(self.program, cid);
        let header = header_actor(ferrata_account(), &INSTANCE, b.slot);
        self.send(label, root, &message, vec![root, header])
    }

    /// `AttestListing` of `token` for `cid`: the vault's listing slot of `token` in block `block` (F.10).
    pub fn attest_listing(
        &mut self,
        label: &str,
        cid: &Hash,
        token: [u8; 20],
        block: usize,
        vault: &[u8; 20],
    ) -> TxReport {
        let b = &self.blocks[block];
        let message = BridgeMessage::AttestListing {
            config_id: *cid,
            token,
            record_slot: b.slot,
            proof: b.vault_proof(vault, &[bw::listing_slot(&token)]),
        };
        let root = config_actor(self.program, cid);
        let header = header_actor(ferrata_account(), &INSTANCE, b.slot);
        self.send(label, root, &message, vec![root, header, token_actor(self.program, cid, &token)])
    }

    /// `ApplyRoles` for `cid`, reading the record of block `block`.
    pub fn apply_roles(&mut self, label: &str, cid: &Hash, block: usize) -> TxReport {
        let b = &self.blocks[block];
        let root = config_actor(self.program, cid);
        let header = header_actor(ferrata_account(), &INSTANCE, b.slot);
        let message = BridgeMessage::ApplyRoles { config_id: *cid, record: b.time_record() };
        self.send(label, root, &message, vec![root, header])
    }

    /// The actors of a public holder's `Transfer`: its own actor, its `holder(account)`, the
    /// token, the recipient's `holder(to)`.
    pub fn transfer_actors(&self, account: AccountId, cid: &Hash, token: &[u8; 20], to: AccountId) -> Vec<Actor> {
        let mut actors = vec![own_actor(self.program, account), holder_of(self.program, cid, account)];
        for actor in [token_actor(self.program, cid, token), holder_of(self.program, cid, to)] {
            if !actors.contains(&actor) {
                actors.push(actor);
            }
        }
        actors
    }

    /// The payer's `Transfer` of `amount` of `token` under `cid` to `to` (it signs every
    /// transaction).
    pub fn transfer(&mut self, label: &str, cid: &Hash, token: [u8; 20], amount: u128, to: AccountId) -> TxReport {
        let payer = self.chain.payer;
        let actors = self.transfer_actors(payer, cid, &token, to);
        let message = BridgeMessage::Transfer { config_id: *cid, token, amount, to: *to.value() };
        self.send(label, actors[0], &message, actors)
    }

    /// `Tick` at `config(cid)` or `token(cid, token)`, reading the record of block `block`, in a
    /// block stamped with the world's LEZ time.
    pub fn tick(&mut self, label: &str, cid: &Hash, token: Option<[u8; 20]>, block: usize) -> TxReport {
        let at = match &token {
            None => config_actor(self.program, cid),
            Some(t) => token_actor(self.program, cid, t),
        };
        let b = &self.blocks[block];
        let record = header_actor(ferrata_account(), &INSTANCE, b.slot);
        let message = BridgeMessage::Tick { config_id: *cid, token, record: b.time_record() };
        self.send(label, at, &message, vec![at, record])
    }

    /// Another vault, before any transaction (its address outside `0x40…40`–`0x67…67`, the filler
    /// accounts of [`Block::new`]): `leaves` are its whole deposit tree, ETH is registered
    /// in its storage with schedule 0 only, and a synthetic block at beacon slot `slot` (number
    /// `number`) holding it is recorded at the attestation program, as the records of
    /// [`World::new`] are. Returns its configuration (this world's controls), the configuration id
    /// and the block's index for [`World::init`], [`World::register`] and [`World::import`].
    pub fn add_vault(
        &mut self,
        address: [u8; 20],
        leaves: Vec<Hash>,
        slot: u64,
        number: u64,
    ) -> (BridgeConfig, Hash, usize) {
        assert!(!(0x40..0x68).contains(&address[0]) || address != [address[0]; 20], "a filler account's address");
        let config = config(address, controls());
        let cid = config.id(self.program);
        let vault = Vault { address, leaves };
        let count = vault.leaves.len() as u64;
        let slots = VaultSlots {
            address,
            root: vault.root(count),
            count,
            config_id: cid,
            extra: registry_slots(&eth_entry(), &[]),
        };
        let block = Block::new(slot, number, &[slots]);
        let state = std::mem::take(&mut self.chain.state);
        self.chain.state = state.with_public_accounts([block.record_account()]);
        self.blocks.push(block);
        (config, cid, self.blocks.len() - 1)
    }

    /// Public accounts that exist from genesis (so that a claim can mint to them), before any
    /// transaction.
    pub fn add_public_accounts(&mut self, ids: &[AccountId]) {
        let state = std::mem::take(&mut self.chain.state);
        self.chain.state = state.with_public_accounts(ids.iter().map(|id| (*id, Account::default())));
    }

    /// Inits of A and B, the registrations (ETH on both, T2 on B), the imports every claim test
    /// starts from — `root(cid_a, 5)` (S1), `root(cid_a, 8)` (S2), `root(cid_a, 6)` (S3, the lagging
    /// count) and `root(cid_b, 8)` — and vault A's ETH fee schedule 1 with its range closed at 8.
    pub fn with_roots() -> Self {
        let mut w = Self::new();
        let (a, b) = (w.config_a.clone(), w.config_b.clone());
        let (cid_a, cid_b, va) = (w.cid_a, w.cid_b, w.vault_a.address);
        for report in [
            w.init("Init A", &a, 0),
            w.init("Init B", &b, 0),
            w.register("Register ETH (A)", &cid_a, 0, &eth_entry(), &va),
            w.register("Register ETH (B)", &cid_b, 0, &eth_entry(), &VAULT_B),
            w.register("Register T2 (B)", &cid_b, 0, &t2_entry(), &VAULT_B),
            w.import("ImportRoot A n=5", &cid_a, 0, w.vault_a.root(5), 5, &va),
            w.import("ImportRoot A n=8", &cid_a, 1, w.vault_a.root(8), 8, &va),
            w.import("ImportRoot A n=6 (lagging count)", &cid_a, 2, w.vault_a.root(8), 6, &va),
            w.import("ImportRoot B n=8", &cid_b, 0, w.vault_b.root(8), 8, &VAULT_B),
            w.import_fees("ImportFees ETH (A): schedule 1", &cid_a, 1, ETH, 1, Some(eth_schedule_one()), &va),
            w.import_fees("ImportFees ETH (A): no schedule 2 at 8 leaves", &cid_a, 1, ETH, 2, None, &va),
        ] {
            assert!(report.ok(), "{}: {:?} (exit {:?})", report.label, report.charge.revert, report.exit_code);
        }
        w
    }
}

// ---------------------------------------------------------------------------------------------
// Private execution (dev mode in the tests)
// ---------------------------------------------------------------------------------------------

/// The programs a private transaction of the bridge tests runs: the bridge, privately at the
/// claimant's or holder's account and publicly at its PDAs.
pub fn catalog() -> lee::privacy_preserving_transaction::circuit::ProgramCatalog {
    lee::privacy_preserving_transaction::circuit::ProgramCatalog::from([(bridge_account(), bridge_program())])
}

/// The keys of `Chain::add_private_account(seed, …)`.
pub fn private_keys(seed: u8) -> lee::test_utils::TestPrivateKeys {
    lee::test_utils::TestPrivateKeys {
        ask: lee_core::AuthorizationSecretKey([seed; 32]),
        d: [seed.wrapping_add(1); 32],
        z: [seed.wrapping_add(2); 32],
    }
}

/// A private account's state after a private transaction of the bridge: the fee `reserve` gone
/// from its native balance, the bridge's actor state `holder`, and the nonce advanced as LEZ
/// advances a private account's nonce. Returned only if the chain holds its commitment, so the
/// reconstruction is checked, not assumed.
pub fn private_post_state(
    chain: &Chain,
    keys: &lee::test_utils::TestPrivateKeys,
    before: &Account,
    reserve: u128,
    holder: &HolderState,
) -> Option<Account> {
    let balance = before.data.native_balance().ok()?.checked_sub(reserve)?;
    let mut post = before.clone();
    post.data.set_actor_state(
        lee_core::native_token::NATIVE_TOKEN_PROGRAM_ID,
        lee_core::native_token::encode_balance(balance),
    );
    post.data.set_actor_state(
        bridge_account(),
        ActorState::from(borsh::to_vec(&BridgeState::Holder(holder.clone())).unwrap()),
    );
    let advanced = Account { nonce: post.nonce.private_account_nonce_increment(&keys.nsk()), ..post.clone() };
    [advanced, post].into_iter().find(|candidate| {
        chain.state.get_proof_for_commitment(&lee_core::Commitment::new(&keys.account_id(), candidate)).is_some()
    })
}

/// The two private transitions of a `ClaimPrivately` in the executor alone (what a prover proves
/// for the bridge): the root call at `account`'s actor, which sends `Read`, then the root's reply,
/// in which the claim is verified. The second runs only if the first succeeds.
pub fn private_claim_transitions(
    w: &World,
    account: AccountId,
    message: &BridgeMessage,
    cid: &Hash,
    n: u64,
) -> Vec<crate::attest::Executed> {
    let actor = own_actor(w.program, account);
    let call = ReceiveInput {
        receiver: actor,
        from: None,
        is_authorized: true,
        pre_state: Default::default(),
        message: borsh::to_vec(message).unwrap(),
    };
    let elf = bridge_program();
    let first = crate::attest::execute("ClaimPrivately (sends Read)", elf.elf(), &call);
    let Some(transition) = &first.transition else { return vec![first] };
    let root = root_actor(w.program, cid, n);
    let reply = ReceiveInput {
        receiver: actor,
        from: Some(root),
        is_authorized: true,
        pre_state: transition.response.post_state.clone().expect("the claim is kept for the reply"),
        message: borsh::to_vec(&lee_core::program::StateReply { state: w.chain.actor_state(root) }).unwrap(),
    };
    let second = crate::attest::execute("the root's reply (verifies the claim, sends Issue)", elf.elf(), &reply);
    vec![first, second]
}

/// One private holder's `Burn` in the executor alone.
pub fn private_burn_transition(
    w: &World,
    account: AccountId,
    pre_state: ActorState,
    message: &BridgeMessage,
) -> crate::attest::Executed {
    let call = ReceiveInput {
        receiver: own_actor(w.program, account),
        from: None,
        is_authorized: true,
        pre_state,
        message: borsh::to_vec(message).unwrap(),
    };
    crate::attest::execute("Burn at the private holder (debits, sends Burn)", bridge_program().elf(), &call)
}
