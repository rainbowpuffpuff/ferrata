//! Drives the v0.3.1 bridge program (`../guests/bridge`) through LEZ v0.3.1's own state machine
//! (`lee::V03State`, `ValidatedStateDiff`, the privacy-preserving circuit's `execute_and_prove`).
//! EXPERIMENTAL: see `../README.md`.
//!
//! * [`World`]: the state, the bridge deployed at a fixed account, a payer that signs and pays every
//!   public transaction (as the testnet requires), records of the light client placed at its
//!   `header(instance, slot)` accounts, and a [`TxReport`] per transaction: the exit code, the
//!   execution gas (the cycles LEZ's metering counts, what the sequencer charges as `gas_exec`) and
//!   the storage gas (the transaction's bytes).
//! * [`eth`]: execution blocks holding a vault's storage (synthetic tries, or captured
//!   `eth_getProof` answers and headers), and the light client's records of them.
//! * [`deposits`]: deposits, their tree, claim witnesses and burn notes, with the bridge core.

pub mod deposits;
pub mod eth;

use std::collections::HashMap;

use ferrata_bridge_core::sha256;
use ferrata_core::witness::HeaderRecord;
use ferrata_lez03_bridge_core::{
    self as bc, burn_account, burns_account, config_account, header_account, holder_account, nullifier_account,
    root_account, BridgeConfig, BridgeState, BurnFrom, ClaimWitness, Denomination, Effect, ImportProof, Instruction,
    LcRecord, Note, PairingProof, RecordRef, Root, RootState,
};
use lee::{
    error::LeeError,
    execute_and_prove,
    privacy_preserving_transaction::{self as ppt, circuit::ProgramWithDependencies, PrivacyPreservingTransaction},
    program::Program,
    public_transaction::{self as pt, PublicTransaction},
    Account, AccountId, FeeDeclaration, PrivateKey, ProgramShardSelector, ProvingInput, PublicKey, V03State,
    ValidatedStateDiff,
};
use lee_core::{
    account::{AccountData, Nonce, ShardData},
    encryption::ViewingPublicKey,
    AuthorizationSecretKey, Commitment, Identifier, NullifierPublicKey, NullifierSecretKey, NullifierWitness,
    PrivacyPreservingCircuitOutput, PrivateWitness, WitnessKind, DUMMY_COMMITMENT_HASH,
};

/// The bridge program built by `../methods`.
pub fn bridge_program() -> Program {
    Program::new(methods03::FERRATA_BRIDGE03_ELF.to_vec().into()).expect("the bridge ELF decodes")
}

/// The bridge program's image id, as bytes (the configuration's `bridge_image_id`).
pub fn bridge_image_id() -> Root {
    let mut out = [0u8; 32];
    for (i, w) in methods03::FERRATA_BRIDGE03_ID.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

/// Where the harness deploys the bridge (a header account, as `program_loader` would give).
pub fn bridge_account() -> AccountId {
    AccountId::new(sha256(b"ferrata/lez-v03-bridge/harness/bridge"))
}

/// The light client program's account in the harness: its records are placed there.
pub fn light_client_account() -> AccountId {
    AccountId::new(sha256(b"ferrata/lez-v03-bridge/harness/light-client"))
}

/// A public key of the harness, by seed.
pub fn key(seed: u8) -> PrivateKey {
    PrivateKey::try_new(sha256(&[b"ferrata/lez-v03-bridge/harness/key/".as_slice(), &[seed]].concat()))
        .expect("a valid key")
}

pub fn account_of(key: &PrivateKey) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(key))
}

/// The gas limit every harness transaction declares (the testnet's `max_gas_exec`).
pub const GAS_LIMIT: u64 = 10_000_000;
/// The storage gas limit of the testnet (`max_gas_stor`).
pub const STOR_LIMIT: u64 = 1_000_000;

/// One transaction's outcome.
#[derive(Clone, Debug)]
pub struct TxReport {
    pub label: String,
    /// `public` or `private`.
    pub kind: &'static str,
    /// The exit code of a refusal (`None` if applied, or if it failed otherwise: `error`).
    pub code: Option<u32>,
    /// Where a private transaction was refused: in the proof (`private part`) or at settlement.
    pub stage: &'static str,
    pub error: Option<String>,
    /// Execution gas: the cycles LEZ's metering counts (public transactions; a private
    /// transaction is fee-exempt on v0.3.1).
    pub gas_exec: u64,
    /// Storage gas: the transaction's borsh bytes (as `LeeTransaction`).
    pub bytes: u64,
}

impl TxReport {
    pub fn applied(&self) -> bool {
        self.code.is_none() && self.error.is_none()
    }

    pub fn verdict(&self) -> String {
        match (&self.code, &self.error) {
            (None, None) => "applied".into(),
            (Some(c), _) => format!("{c} `{}` ({})", bc::code_name(*c).unwrap_or("core"), self.stage),
            (None, Some(e)) => format!("failed: {e}"),
        }
    }
}

/// A markdown table of reports.
pub fn render(reports: &[TxReport]) -> String {
    let mut out = String::from("| # | transaction | kind | result | execution gas | storage gas (bytes) |\n");
    out.push_str("|---:|---|---|---|---:|---:|\n");
    for (i, r) in reports.iter().enumerate() {
        let gas = if r.kind == "public" { r.gas_exec.to_string() } else { "exempt".into() };
        out.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n", i + 1, r.label, r.kind, r.verdict(), gas, r.bytes));
    }
    out
}

/// The keys of a private account (`lee`'s test layout: `ask`, and the viewing seed `d ‖ z`).
#[derive(Clone)]
pub struct PrivateKeys {
    pub ask: AuthorizationSecretKey,
    pub d: [u8; 32],
    pub z: [u8; 32],
}

impl PrivateKeys {
    pub fn from_seed(seed: u8) -> Self {
        let h = |tag: &[u8]| sha256(&[b"ferrata/lez-v03-bridge/harness/private/".as_slice(), tag, &[seed]].concat());
        Self { ask: AuthorizationSecretKey(h(b"ask")), d: h(b"d"), z: h(b"z") }
    }

    pub fn nsk(&self) -> NullifierSecretKey {
        (&self.ask).into()
    }

    pub fn npk(&self) -> NullifierPublicKey {
        NullifierPublicKey::from(&self.nsk())
    }

    pub fn vpk(&self) -> ViewingPublicKey {
        ViewingPublicKey::from_seed(&self.d, &self.z)
    }
}

/// A private account of the harness: its keys and its current plaintext (`None` before it exists).
#[derive(Clone)]
pub struct PrivateHolder {
    pub keys: PrivateKeys,
    pub identifier: Identifier,
    pub account: Option<Account>,
}

impl PrivateHolder {
    pub fn new(seed: u8) -> Self {
        Self { keys: PrivateKeys::from_seed(seed), identifier: Identifier::ZERO, account: None }
    }

    pub fn id(&self) -> AccountId {
        AccountId::for_regular_private_account(&self.keys.npk(), &self.keys.vpk(), self.identifier)
    }

    /// The witness for its next transaction (authorised: the key is in the witness).
    pub fn witness(&self, state: &V03State) -> PrivateWitness {
        let nullifier = match &self.account {
            None => NullifierWitness::Init { npk: self.keys.npk(), commitment_root: DUMMY_COMMITMENT_HASH },
            Some(account) => NullifierWitness::Update {
                account: account.clone(),
                view_tag: 0,
                nsk: self.keys.nsk(),
                membership_proof: state
                    .get_proof_for_commitment(&Commitment::new(&self.id(), account))
                    .expect("the account's commitment is in the state"),
            },
        };
        PrivateWitness {
            vpk: self.keys.vpk(),
            random_seed: [7; 32],
            identifier: self.identifier,
            kind: WitnessKind::Regular { ask: Some(self.keys.ask) },
            nullifier,
        }
    }

    /// Its bridge shard.
    pub fn bridge_state(&self, bridge: AccountId) -> Option<BridgeState> {
        let data = self.account.as_ref()?.data.shard(bridge);
        if data.is_empty() {
            return None;
        }
        Some(BridgeState::decode(data).expect("a bridge state"))
    }

    /// Its plaintext after a transaction that applied `effect` to its bridge shard.
    fn after(&self, bridge: AccountId, effect: &Effect) -> Account {
        let id = self.id();
        let (nonce, data) = match &self.account {
            None => (Nonce::private_account_nonce_init(&id), AccountData::default()),
            Some(a) => (a.nonce.private_account_nonce_increment(&self.keys.nsk()), a.data.clone()),
        };
        let pre = data.shard(bridge).clone();
        let post = bc::apply(effect, &pre).expect("the effect applied in the proof").expect("a write");
        let mut data = data;
        data.set_shard(bridge, ShardData::try_from(post).expect("fits"));
        Account { nonce, data }
    }
}

/// The state, the bridge, the payer and the reports.
pub struct World {
    pub state: V03State,
    pub block: u64,
    /// Milliseconds.
    pub time: u64,
    pub bridge: AccountId,
    pub light_client: AccountId,
    pub program: Program,
    pub payer: PrivateKey,
    pub reports: Vec<TxReport>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// A fresh state with the bridge at [`bridge_account`] and a funded payer.
    pub fn new() -> Self {
        let program = bridge_program();
        let bridge = bridge_account();
        let payer = key(0);
        let state = V03State::new()
            .with_named_programs([(bridge, program.clone())])
            .with_public_account_balances([(account_of(&payer), 1_000_000_000_000_000)]);
        Self {
            state,
            block: 2,
            time: 1_800_000_000_000,
            bridge,
            light_client: light_client_account(),
            program,
            payer,
            reports: Vec::new(),
        }
    }

    /// A configuration of this world's bridge for `vault` on `chain`, ETH (`eth` wei) and one
    /// ERC-20 (`token`, `units`).
    pub fn config(&self, chain: u64, vault: [u8; 20], instance_id: Root, registry: Vec<Denomination>) -> BridgeConfig {
        BridgeConfig {
            bedrock_genesis_id: sha256(b"ferrata/lez-v03-bridge/harness/bedrock-genesis"),
            channel_id: [1; 32],
            bridge_image_id: bridge_image_id(),
            attestation: self.light_client,
            instance_id,
            evm_chain_id: chain,
            vault,
            registry,
        }
    }

    pub fn config_id(&self, config: &BridgeConfig) -> Root {
        config.id(self.bridge)
    }

    /// Place the light client's record of `record` at `header(instance, slot)`, as the light client
    /// writes it after verifying a finality update; the reference an instruction carries.
    pub fn place_record(&mut self, instance_id: Root, chain_id: u64, record: &HeaderRecord) -> RecordRef {
        let slot = record.beacon.slot;
        let state = LcRecord { instance_id, chain_id, record: record.encode(), root_id: instance_id };
        let bytes = borsh::to_vec(&state).unwrap();
        let account = header_account(self.light_client, &instance_id, slot);
        let held = Account::default().with_shard(self.light_client, ShardData::try_from(bytes.clone()).unwrap());
        self.state.force_insert_account(account, held);
        RecordRef { slot, state: bytes }
    }

    /// The bridge shard of a public account, decoded.
    pub fn bridge_state(&self, account: AccountId) -> Option<BridgeState> {
        let a = self.state.get_account_by_id(account);
        let data = a.data.shard(self.bridge);
        if data.is_empty() {
            return None;
        }
        Some(BridgeState::decode(data).expect("a bridge state"))
    }

    fn sel(&self, account: AccountId) -> ProgramShardSelector {
        ProgramShardSelector::new(account, self.bridge)
    }

    fn advance(&mut self) {
        self.block += 1;
        self.time += 1_000;
    }

    /// A public transaction to the bridge, signed by the payer (and `signers`), with a fee declaration.
    pub fn public(
        &mut self,
        label: &str,
        instruction: &Instruction,
        selectors: Vec<ProgramShardSelector>,
        signers: &[&PrivateKey],
    ) -> TxReport {
        let payer = self.payer.clone();
        let mut keys: Vec<&PrivateKey> = vec![&payer];
        keys.extend(signers.iter().copied().filter(|k| account_of(k) != account_of(&payer)));
        let nonces = keys.iter().map(|k| self.state.get_account_by_id(account_of(k)).nonce).collect();
        let fee = FeeDeclaration::new(account_of(&payer), GAS_LIMIT, 0, 1_000_000_000_000);
        let message = pt::Message::try_new_with_fees(self.bridge, selectors, nonces, instruction, fee).unwrap();
        let witness_set = pt::WitnessSet::for_message(&message, &keys);
        let tx = PublicTransaction::new(message, witness_set);
        let bytes = borsh::to_vec(&tx).unwrap().len() as u64 + 1;
        let budget = lee::program::DEFAULT_PUBLIC_CYCLE_BUDGET;
        let (charge, metered) = ValidatedStateDiff::from_public_transaction_metered(&tx, &self.state, self.block, self.time, budget);
        let outcome =
            ValidatedStateDiff::from_public_transaction_with_cycle_budget(&tx, &self.state, self.block, self.time, budget);
        let (code, error) = match &outcome {
            Ok(_) => (None, None),
            Err(LeeError::ProgramExitedWithCode { code, .. }) => (Some(*code), None),
            Err(e) => (None, Some(e.to_string())),
        };
        if let Ok(diff) = metered {
            drop(self.state.apply_state_diff(diff));
        }
        self.advance();
        let report = TxReport {
            label: label.into(),
            kind: "public",
            code,
            stage: "public",
            error,
            gas_exec: charge.cycles.min(GAS_LIMIT),
            bytes,
        };
        self.reports.push(report.clone());
        report
    }

    /// A privacy-preserving transaction to the bridge: `holder` (a private account) and the public
    /// accounts `public`, proven by LEZ's circuit (RISC Zero dev mode in the tests unless
    /// `RISC0_DEV_MODE` is unset), then validated and settled. On success the holder's plaintext
    /// is advanced by `effect` and its new commitment checked in the state.
    pub fn private(
        &mut self,
        label: &str,
        instruction: &Instruction,
        holder: &mut PrivateHolder,
        effect: &Effect,
        public: Vec<AccountId>,
    ) -> (TxReport, Option<PrivacyPreservingTransaction>) {
        let mut selectors = vec![self.sel(holder.id())];
        selectors.extend(public.into_iter().map(|a| self.sel(a)));
        let input = ProvingInput {
            shard_selectors: selectors,
            private_witnesses: vec![holder.witness(&self.state)],
            instruction_data: borsh::to_vec(instruction).unwrap(),
            ..Default::default()
        };
        let deps = ProgramWithDependencies::new(self.program.clone(), self.bridge, HashMap::new());
        let mut report =
            TxReport { label: label.into(), kind: "private", code: None, stage: "", error: None, gas_exec: 0, bytes: 0 };
        let proven = execute_and_prove(input, &deps);
        let (output, proof) = match proven {
            Ok(p) => p,
            Err(LeeError::ProgramExitedWithCode { code, .. }) => {
                report.code = Some(code);
                report.stage = "private part: no proof";
                self.reports.push(report.clone());
                return (report, None);
            }
            Err(e) => {
                report.error = Some(e.to_string());
                self.reports.push(report.clone());
                return (report, None);
            }
        };
        let post = holder.after(self.bridge, effect);
        let expected = Commitment::new(&holder.id(), &post);
        let commitments = output.commitments();
        let message = ppt::Message::from_circuit_output(vec![], output);
        let witness_set = ppt::WitnessSet::for_message(&message, proof, &[]);
        let tx = PrivacyPreservingTransaction::new(message, witness_set);
        report.bytes = borsh::to_vec(&tx).unwrap().len() as u64 + 1;
        match ValidatedStateDiff::from_privacy_preserving_transaction(&tx, &self.state, self.block, self.time) {
            Ok(diff) => {
                drop(self.state.apply_state_diff(diff));
                assert!(commitments.contains(&expected), "{label}: the holder's new commitment is the expected plaintext's");
                holder.account = Some(post);
            }
            Err(LeeError::ProgramExitedWithCode { code, .. }) => {
                report.code = Some(code);
                report.stage = "settlement";
            }
            Err(e) => report.error = Some(e.to_string()),
        }
        self.advance();
        self.reports.push(report.clone());
        (report, Some(tx))
    }

    // -----------------------------------------------------------------------------------------
    // The instructions, with their accounts
    // -----------------------------------------------------------------------------------------

    pub fn init(&mut self, label: &str, config: &BridgeConfig, record: &RecordRef, proof: PairingProof) -> TxReport {
        let cid = self.config_id(config);
        let selectors = vec![
            self.sel(config_account(self.bridge, &cid)),
            ProgramShardSelector::new(
                header_account(config.attestation, &config.instance_id, record.slot),
                config.attestation,
            ),
        ];
        let i = Instruction::Init { config: config.clone(), record: record.clone(), proof };
        self.public(label, &i, selectors, &[])
    }

    pub fn import_root(
        &mut self,
        label: &str,
        config: &BridgeConfig,
        record: &RecordRef,
        root: Root,
        leaf_count: u64,
        proof: ImportProof,
    ) -> TxReport {
        let cid = self.config_id(config);
        let selectors = vec![
            self.sel(config_account(self.bridge, &cid)),
            ProgramShardSelector::new(
                header_account(config.attestation, &config.instance_id, record.slot),
                config.attestation,
            ),
            self.sel(root_account(self.bridge, &cid, leaf_count)),
        ];
        let i = Instruction::ImportRoot { config: config.clone(), record: record.clone(), root, leaf_count, proof };
        self.public(label, &i, selectors, &[])
    }

    /// The root state `ImportRoot` wrote at `root(cid, n)`.
    pub fn root_state(&self, config: &BridgeConfig, leaf_count: u64) -> Option<RootState> {
        match self.bridge_state(root_account(self.bridge, &self.config_id(config), leaf_count))? {
            BridgeState::Root(r) => Some(r),
            _ => None,
        }
    }

    pub fn claim(&mut self, label: &str, config: &BridgeConfig, root: &RootState, witness: &ClaimWitness) -> TxReport {
        let nullifier = deposits::nullifier_of(witness, root.evm_chain_id, &root.vault);
        let selectors = vec![
            self.sel(root_account(self.bridge, &root.config_id, root.leaf_count)),
            self.sel(nullifier_account(self.bridge, root.evm_chain_id, &root.vault, &nullifier)),
            self.sel(holder_account(self.bridge, &witness.dest)),
        ];
        let i = Instruction::Claim { config: config.clone(), root: *root, witness: witness.clone() };
        self.public(label, &i, selectors, &[])
    }

    pub fn claim_privately(
        &mut self,
        label: &str,
        config: &BridgeConfig,
        root: &RootState,
        witness: &ClaimWitness,
        holder: &mut PrivateHolder,
    ) -> (TxReport, Option<PrivacyPreservingTransaction>) {
        let nullifier = deposits::nullifier_of(witness, root.evm_chain_id, &root.vault);
        let public = vec![
            root_account(self.bridge, &root.config_id, root.leaf_count),
            nullifier_account(self.bridge, root.evm_chain_id, &root.vault, &nullifier),
        ];
        let amount = bc::u128_of(&witness.amount).unwrap_or(0);
        let effect = Effect::Credit { config_id: root.config_id, token: witness.token, amount };
        let i = Instruction::ClaimPrivately { config: config.clone(), root: *root, witness: witness.clone() };
        self.private(label, &i, holder, &effect, public)
    }

    /// A public holder's burn, signed by `owner`.
    pub fn burn_public(&mut self, label: &str, config: &BridgeConfig, note: &Note, owner: &PrivateKey) -> TxReport {
        let cid = self.config_id(config);
        let owner_id = account_of(owner);
        let selectors = vec![
            self.sel(owner_id),
            self.sel(holder_account(self.bridge, owner_id.value())),
            self.sel(burn_account(self.bridge, &cid, &note.hash())),
            self.sel(burns_account(self.bridge, &cid)),
        ];
        let i = Instruction::Burn { config: config.clone(), note: *note, from: BurnFrom::Holder };
        self.public(label, &i, selectors, &[owner])
    }

    /// A private holder's burn: the root of its own privacy-preserving transaction.
    pub fn burn_private(
        &mut self,
        label: &str,
        config: &BridgeConfig,
        note: &Note,
        holder: &mut PrivateHolder,
    ) -> (TxReport, Option<PrivacyPreservingTransaction>) {
        let cid = self.config_id(config);
        let public = vec![burn_account(self.bridge, &cid, &note.hash()), burns_account(self.bridge, &cid)];
        let amount = bc::u128_of(&note.amount).unwrap_or(0);
        let effect = Effect::Debit { config_id: cid, token: note.token, amount };
        let i = Instruction::Burn { config: config.clone(), note: *note, from: BurnFrom::Own };
        self.private(label, &i, holder, &effect, public)
    }

    /// The holding of `account` at `holder(account)`.
    pub fn public_holding(&self, config: &BridgeConfig, account: &Root, token: &[u8; 20]) -> u128 {
        match self.bridge_state(holder_account(self.bridge, account)) {
            Some(BridgeState::Holder(h)) => h.holding(&self.config_id(config), token),
            _ => 0,
        }
    }

    /// The configuration's burn tree, as `burns(cid)` holds it.
    pub fn burn_tree(&self, config: &BridgeConfig) -> Option<bc::BurnTree> {
        match self.bridge_state(burns_account(self.bridge, &self.config_id(config)))? {
            BridgeState::Burns(t) => Some(t),
            _ => None,
        }
    }
}

/// Every byte string of `needles` that occurs in `haystack`, by name: what a transaction shows.
pub fn occurrences<'a>(haystack: &[u8], needles: &[(&'a str, Vec<u8>)]) -> Vec<&'a str> {
    needles
        .iter()
        .filter(|(_, n)| !n.is_empty() && haystack.windows(n.len()).any(|w| w == n.as_slice()))
        .map(|(name, _)| *name)
        .collect()
}

/// The circuit output of a private transaction, for the privacy checks.
pub fn public_part(tx: &PrivacyPreservingTransaction) -> &ppt::Message {
    &tx.message
}

/// Unused import guard for the circuit output type (kept for readers of the API).
pub type CircuitOutput = PrivacyPreservingCircuitOutput;
