//! The LEZ side: what the tool reads from a zone, the calls it builds (claim, burn), the checks it
//! runs before sending — the bridge program's own check functions from `ferrata-lez-bridge-wire`,
//! so a refusal names the code the program would exit with — and submission through LEZ's own
//! wallet library to a sequencer endpoint.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use borsh::BorshDeserialize as _;
use common::transaction::LeeTransaction;
use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{self, DestKind, OwnedDeposit},
    outbound::{recover_burns, BurnNote, PublishedBurn, ReleaseTarget},
    tree::Tree,
    BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{
    self as bw, burn_actor, burn_seed, config_actor, config_seed, holder_actor, nullifier_actor, nullifier_seed,
    root_actor, root_seed, token_actor, token_seed, BridgeMessage, BridgeState, HolderState, RootState, ScheduleRange,
    TokenState,
};
use lee::AccountId;
use lee_core::{account::Actor, program::PdaSeed};
use sequencer_service_rpc::SequencerClient;
use wallet::{AccountIdentity, AccountMention, WalletCore, WalletSequencerClient as _};

use crate::{
    codes::program_refusal,
    hx,
    seed::{Seed, DEFAULT_GAP},
    units::{u128_to_word, word_to_u128},
};

/// How many unused burn counters end a burn scan.
pub const BURN_GAP: u64 = 20;

/// Which configuration of which bridge program, and the vault it pairs with.
#[derive(Clone, Debug)]
pub struct Context {
    /// The bridge program's account.
    pub program: AccountId,
    /// `configId`.
    pub cid: Hash,
    pub chain_id: u64,
    pub vault: [u8; 20],
}

/// One account a call declares.
#[derive(Clone, Debug)]
pub enum Mention {
    /// A PDA of the bridge program.
    Pda { actor: Actor, seed: PdaSeed },
    /// A public account that signs the call (a burn's holder).
    Signer(Actor),
    /// A public account's bridge state the call credits (destination, fee recipient, relayer); with
    /// its public key when the account may not exist yet (LEZ admits a new account on evidence).
    Other(Actor, Option<lee::PublicKey>),
    /// The claimant's own private account, the root of a private claim.
    PrivateSelf(Actor),
}

impl Mention {
    pub fn actor(&self) -> Actor {
        match self {
            Mention::Pda { actor, .. }
            | Mention::Signer(actor)
            | Mention::Other(actor, _)
            | Mention::PrivateSelf(actor) => *actor,
        }
    }
}

/// A call of the bridge program, ready to sign and send.
#[derive(Clone, Debug)]
pub struct LezCall {
    pub what: String,
    pub mentions: Vec<Mention>,
    pub root_index: usize,
    pub message: BridgeMessage,
}

/// One applied public call of the bridge program, in chain order.
#[derive(Clone, Debug)]
pub struct Applied {
    pub to: Actor,
    pub message: Vec<u8>,
}

/// What a zone serves the tool: applied calls, actor states, submission.
#[allow(async_fn_in_trait)]
pub trait Ledger {
    /// Every applied (not reverted) public call whose root is an actor of `program`, in chain order.
    async fn applied(&self, program: AccountId) -> Result<Vec<Applied>>;
    /// `applied`, and the public accounts named by applied privacy-preserving transactions (a
    /// private claim names its nullifier account there), read in the same pass where the ledger can.
    /// None by default.
    async fn applied_with_private(&self, program: AccountId) -> Result<(Vec<Applied>, HashSet<AccountId>)> {
        Ok((self.applied(program).await?, HashSet::new()))
    }
    /// The bridge state of `actor` (empty if none).
    async fn actor_state(&self, actor: Actor) -> Result<Vec<u8>>;
    /// Sign and send `call`; returns the transaction hash (hex).
    async fn submit(&mut self, call: &LezCall) -> Result<String>;
}

/// The bridge state of `actor`, if any.
pub async fn read<L: Ledger>(ledger: &L, actor: Actor) -> Result<Option<BridgeState>> {
    let bytes = ledger.actor_state(actor).await?;
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(BridgeState::try_from_slice(&bytes).map_err(|e| anyhow!("the actor's state is not a bridge state: {e}"))?))
}

/// What the tool learns from the zone's public history in one pass (no per-item query).
#[derive(Clone, Debug, Default)]
pub struct History {
    /// Leaf counts of the imported roots of this configuration.
    pub roots: Vec<u64>,
    /// Each import of this configuration in order of application: (leaf count, the record slot it read).
    pub imports: Vec<(u64, u64)>,
    /// Published burn notes of this configuration, by burn index (order of application).
    pub burns: Vec<PublishedBurn>,
    /// Nullifiers of plaintext claims against this vault.
    pub claimed: HashSet<Hash>,
    /// Public accounts named by applied privacy-preserving transactions: the nullifier accounts of
    /// private claims among them.
    pub touched_privately: HashSet<AccountId>,
}

/// Reads the history of `ctx`'s configuration from `applied`.
pub fn history(ctx: &Context, applied: &[Applied]) -> History {
    let mut h = History::default();
    let chain = chain_id_u256(ctx.chain_id);
    for call in applied.iter().filter(|c| c.to.program_account_id == ctx.program) {
        let Ok(message) = BridgeMessage::try_from_slice(&call.message) else { continue };
        match message {
            BridgeMessage::ImportRoot { config_id, leaf_count, record_slot, .. } if config_id == ctx.cid => {
                h.roots.push(leaf_count);
                h.imports.push((leaf_count, record_slot));
            }
            BridgeMessage::Burn { config_id, r, memo, mac, token, amount, c_out } if config_id == ctx.cid => {
                let burn_index = h.burns.len() as u64;
                h.burns.push(PublishedBurn { burn_index, note: BurnNote { r, memo, mac, token, amount, c_out } });
            }
            BridgeMessage::Claim { config_id, witness: w, .. } if config_id == ctx.cid => {
                let Some(kind) = DestKind::from_byte(w.kind) else { continue };
                let c = inbound::commitment(&w.s, kind, &w.dest);
                let leaf = inbound::deposit_leaf(w.leaf_index, &w.depositor, &w.token, &w.amount, &c);
                let fid = inbound::deposit_id(&chain, &ctx.vault, w.leaf_index, &leaf);
                h.claimed.insert(inbound::nullifier(&w.s, &fid));
            }
            _ => {}
        }
    }
    h.roots.sort_unstable();
    h.roots.dedup();
    h
}

/// The history of `ctx`'s configuration, private claims included, from one pass over the zone.
pub async fn history_of<L: Ledger>(ledger: &L, ctx: &Context) -> Result<History> {
    let (applied, touched) = ledger.applied_with_private(ctx.program).await?;
    let mut h = history(ctx, &applied);
    h.touched_privately = touched;
    Ok(h)
}

/// Whether the deposit with nullifier `n` is claimed in `history`: by a plaintext claim, or by a
/// private one, whose public part names `nullifier(chain, vault, N)`. Matched locally: no per-item query.
pub fn is_claimed(ctx: &Context, history: &History, n: &Hash) -> bool {
    history.claimed.contains(n)
        || history.touched_privately.contains(&nullifier_actor(ctx.program, ctx.chain_id, &ctx.vault, n).account_id)
}

/// The schedule range a wallet names for a claim of `leaf` (the widest its token state allows), as
/// the harness's wallet does.
pub fn range_for(token: &TokenState, leaf: u64) -> ScheduleRange {
    if token.schedules.is_empty() {
        return ScheduleRange { index: 0, from_leaf: 0, until_leaf: u64::MAX };
    }
    let j = token.schedules.iter().rposition(|s| s.from_leaf <= leaf).unwrap_or(0);
    let until = token.schedules.get(j + 1).map_or(token.known_until, |s| s.from_leaf);
    ScheduleRange { index: j as u64, from_leaf: token.schedules[j].from_leaf, until_leaf: until }
}

async fn token_state<L: Ledger>(ledger: &L, ctx: &Context, token: &[u8; 20]) -> Result<TokenState> {
    match read(ledger, token_actor(ctx.program, &ctx.cid, token)).await? {
        Some(BridgeState::Token(t)) => Ok(t),
        None => bail!(program_refusal(251, "checked before sending: no token(cid, token) account")),
        Some(other) => bail!("token(cid, token) holds another state: {other:?}"),
    }
}

/// A claim, built and checked.
#[derive(Clone, Debug)]
pub struct ClaimPlan {
    pub call: LezCall,
    pub leaf_count: u64,
    pub nullifier: Hash,
    pub mint_fee: u128,
    pub relay_fee: u128,
}

/// Builds the claim of `owned` from the vault's whole leaf list `leaves`: the newest imported root
/// that covers the leaf, the path, the range; then runs the program's checks.
#[allow(clippy::too_many_arguments)]
pub async fn plan_claim<L: Ledger>(
    ledger: &L,
    ctx: &Context,
    leaves: &[Hash],
    owned: &OwnedDeposit,
    history: &History,
    private: bool,
    relayer: Option<Hash>,
    dest_key: Option<lee::PublicKey>,
) -> Result<ClaimPlan> {
    let d = &owned.deposit;
    let Some(&n) = history.roots.iter().rev().find(|&&n| n > d.leaf_index) else {
        let code = if history.roots.is_empty() { 245 } else { 248 };
        bail!(program_refusal(
            code,
            "checked before sending: no imported root covers this leaf yet; wait for the next root import"
        ));
    };
    let root_state: RootState = match read(ledger, root_actor(ctx.program, &ctx.cid, n)).await? {
        Some(BridgeState::Root(r)) => r,
        _ => bail!(program_refusal(245, format!("checked before sending: root(cid, {n}) is not on the zone"))),
    };
    ensure!(n as usize <= leaves.len(), "the vault's list has {} leaves, the imported root {n}", leaves.len());
    let tree = Tree::from_leaves(leaves[..n as usize].to_vec()).map_err(|_| anyhow!("too many leaves"))?;
    ensure!(
        tree.root() == root_state.root,
        "the vault's deposit list does not give the imported root at {n} leaves: the RPC or the zone serves another vault"
    );
    let path = tree.path(d.leaf_index).ok_or_else(|| anyhow!("leaf {} is not in the tree", d.leaf_index))?;
    let witness = bw::ClaimWitness {
        depositor: d.depositor,
        token: d.token,
        amount: d.amount,
        leaf_index: d.leaf_index,
        path: path.siblings.to_vec(),
        s: owned.s,
        kind: owned.kind as u8,
        dest: owned.dest,
    };
    let token = token_state(ledger, ctx, &d.token).await?;
    let range = range_for(&token, d.leaf_index);
    let amount = word_to_u128(&d.amount)?;
    if token.entry.index_of(amount).is_none() {
        bail!(program_refusal(251, "checked before sending: the amount is not one of the token's denominations"));
    }
    if let Err(code) = bw::verify_claim(&root_state, &witness, &range) {
        bail!(program_refusal(u32::from(code), "checked before sending with the program's verify_claim"));
    }
    if !private && owned.kind != DestKind::Public {
        bail!(program_refusal(
            252,
            "checked before sending: a private-kind deposit is claimed privately (without --plaintext)"
        ));
    }
    let n_actor = nullifier_actor(ctx.program, ctx.chain_id, &ctx.vault, &owned.nullifier);
    if history.claimed.contains(&owned.nullifier) || read(ledger, n_actor).await?.is_some() {
        bail!(program_refusal(250, "checked before sending"));
    }
    let (mint_fee, _) = token
        .fees_for(amount, &range)
        .map_err(|code| anyhow!(program_refusal(u32::from(code), "checked before sending")))?;
    let relay_fee = token.entry.index_of(amount).map(|k| token.entry.relay_fees[k]).unwrap_or(0);
    let fee_recipient = AccountId::new(token.controls.fee_recipient);
    let relayer_account = relayer.map(AccountId::new).unwrap_or(fee_recipient);
    let root = root_actor(ctx.program, &ctx.cid, n);
    let mut mentions = Vec::new();
    let message = if private {
        let me = holder_actor(ctx.program, AccountId::new(owned.dest));
        if owned.kind == DestKind::Private {
            mentions.push(Mention::PrivateSelf(me));
        } else {
            bail!("a public-kind deposit is claimed in plaintext or by its own account's private execution; use the plaintext path");
        }
        mentions.push(Mention::Pda { actor: root, seed: root_seed(&ctx.cid, n) });
        BridgeMessage::ClaimPrivately { config_id: ctx.cid, leaf_count: n, witness, range, relayer }
    } else {
        mentions.push(Mention::Pda { actor: root, seed: root_seed(&ctx.cid, n) });
        BridgeMessage::Claim { config_id: ctx.cid, leaf_count: n, witness, range, relayer }
    };
    mentions.push(Mention::Pda {
        actor: token_actor(ctx.program, &ctx.cid, &d.token),
        seed: token_seed(&ctx.cid, &d.token),
    });
    mentions.push(Mention::Pda { actor: config_actor(ctx.program, &ctx.cid), seed: config_seed(&ctx.cid) });
    mentions.push(Mention::Pda { actor: n_actor, seed: nullifier_seed(ctx.chain_id, &ctx.vault, &owned.nullifier) });
    let dest = (owned.kind == DestKind::Public).then(|| AccountId::new(owned.dest));
    for account in dest.into_iter().chain([fee_recipient, relayer_account]) {
        let actor = holder_actor(ctx.program, account);
        if !mentions.iter().any(|m| m.actor() == actor) {
            let key = if Some(account) == dest { dest_key.clone() } else { None };
            mentions.push(Mention::Other(actor, key));
        }
    }
    let what = format!(
        "{} claim of leaf {} under root(cid, {n})",
        if private { "private" } else { "plaintext" },
        d.leaf_index
    );
    Ok(ClaimPlan {
        call: LezCall { what, mentions, root_index: 0, message },
        leaf_count: n,
        nullifier: owned.nullifier,
        mint_fee,
        relay_fee,
    })
}

/// A burn, built and checked.
#[derive(Clone, Debug)]
pub struct BurnPlan {
    pub call: LezCall,
    pub note: BurnNote,
    /// The burn counter `i` of the note's tag.
    pub i: u64,
}

/// Builds the burn of one denomination `amount` of `token` held by `holder` under `ctx`, to be
/// released to `recipient` on `ctx`'s vault.
#[allow(clippy::too_many_arguments)]
pub async fn plan_burn<L: Ledger>(
    ledger: &L,
    ctx: &Context,
    keys: &BridgeKeys,
    holder: AccountId,
    token: [u8; 20],
    amount: u128,
    recipient: [u8; 20],
    history: &History,
) -> Result<BurnPlan> {
    let t = token_state(ledger, ctx, &token).await?;
    if t.entry.index_of(amount).is_none() {
        bail!(program_refusal(251, "checked before sending: the amount is not one of the token's denominations"));
    }
    let held = match read(ledger, holder_actor(ctx.program, holder)).await? {
        Some(BridgeState::Holder(HolderState { holdings, .. })) => {
            holdings.iter().find(|h| h.config_id == ctx.cid && h.token == token).map_or(0, |h| h.amount)
        }
        _ => 0,
    };
    if held < amount {
        bail!(program_refusal(254, format!("checked before sending: the holding is {held} base units")));
    }
    let chain = chain_id_u256(ctx.chain_id);
    let next_i = recover_burns(keys, &ctx.cid, &chain, &ctx.vault, &history.burns, BURN_GAP).next_i;
    let target = ReleaseTarget { chain_id: chain, vault: ctx.vault, recipient };
    let note = BurnNote::create(keys, &ctx.cid, next_i, &target, token, u128_to_word(amount));
    let wire = bw::Note { r: note.r, memo: note.memo, mac: note.mac, token, amount: note.amount, c_out: note.c_out };
    let note_hash = wire.hash();
    let mentions = vec![
        Mention::Signer(holder_actor(ctx.program, holder)),
        Mention::Pda { actor: token_actor(ctx.program, &ctx.cid, &token), seed: token_seed(&ctx.cid, &token) },
        Mention::Pda { actor: config_actor(ctx.program, &ctx.cid), seed: config_seed(&ctx.cid) },
        Mention::Pda { actor: burn_actor(ctx.program, &ctx.cid, &note_hash), seed: burn_seed(&ctx.cid, &note_hash) },
    ];
    let call = LezCall {
        what: format!("burn with tag counter {next_i}"),
        mentions,
        root_index: 0,
        message: BridgeMessage::burn(ctx.cid, &wire),
    };
    Ok(BurnPlan { call, note, i: next_i })
}

// ---- a sequencer endpoint, through LEZ's wallet library --------------------------------------------

/// A zone reached through a sequencer's RPC with LEZ's wallet. The wallet's home (its config and
/// its storage, encrypted under the password) is the LEZ keystore the user names by path.
pub struct Sequencer {
    pub wallet: WalletCore<SequencerClient>,
    /// The account that pays the fee (it must be in the wallet).
    pub payer: Option<AccountId>,
    /// The bridge program's binary, for private execution (`ClaimPrivately`).
    pub bridge_program: Option<PathBuf>,
}

fn wallet_config_json(rpc: &str) -> String {
    format!(
        "{{\n  \"sequencer\": {{ \"sequencer_addr\": \"{rpc}\" }},\n  \"seq_poll_timeout\": \"1s\",\n  \
         \"seq_tx_poll_max_blocks\": 120,\n  \"seq_poll_max_retries\": 2,\n  \"seq_block_poll_max_amount\": 100,\n  \
         \"gas_limit\": 4000000\n}}\n"
    )
}

/// Opens (or creates from the seed) the LEZ wallet at `home`, its storage encrypted under the
/// password in `password_file`, and checks that its first accounts are the seed's.
pub fn open_wallet(home: &Path, rpc: &str, password_file: &Path, seed: &Seed) -> Result<WalletCore<SequencerClient>> {
    let password = std::fs::read_to_string(password_file)
        .with_context(|| format!("reading the password file {}", password_file.display()))?;
    let password = password.trim_end_matches(['\n', '\r']);
    std::fs::create_dir_all(home)?;
    let config_path = home.join("wallet_config.json");
    let storage_path = home.join("storage.json");
    std::fs::write(&config_path, wallet_config_json(rpc))?;
    let wallet = if storage_path.exists() {
        WalletCore::new_update_chain(config_path, storage_path, None, password)?
    } else {
        let (mut wallet, _fresh) = WalletCore::new_init_storage(config_path, storage_path, None, password)?;
        wallet.restore_storage(seed.mnemonic());
        let root = key_protocol::key_management::key_tree::chain_index::ChainIndex::root();
        let expected = seed.public_accounts(DEFAULT_GAP)?;
        for account in &expected {
            let (id, _) = wallet.create_new_account_public(Some(root.clone()));
            ensure!(id == account.id, "LEZ's wallet derived another public account /{} than the tool", account.index);
        }
        let private = seed.private_account_ids(DEFAULT_GAP)?;
        for (a, want) in private.iter().enumerate() {
            let (id, _) = wallet.create_new_account_private(Some(root.clone()));
            ensure!(id == *want, "LEZ's wallet derived another private account /{a} than the tool");
        }
        wallet.store_persistent_data()?;
        wallet
    };
    Ok(wallet)
}

fn mention(m: &Mention) -> AccountMention {
    let actor = m.actor();
    let program = actor.program_account_id;
    match m {
        Mention::Pda { seed, .. } => {
            AccountIdentity::PublicPda { program, seed: *seed }.select_program_actor_state(program)
        }
        Mention::Signer(_) => AccountIdentity::Public(actor.account_id).select_program_actor_state(program),
        Mention::Other(_, Some(key)) => AccountIdentity::PublicForeign(key.clone()).select_program_actor_state(program),
        Mention::Other(_, None) => AccountIdentity::PublicNoSign(actor.account_id).select_program_actor_state(program),
        Mention::PrivateSelf(_) => AccountIdentity::PrivateOwned(actor.account_id).select_program_actor_state(program),
    }
}

impl Ledger for Sequencer {
    async fn applied(&self, program: AccountId) -> Result<Vec<Applied>> {
        Ok(self.applied_with_private(program).await?.0)
    }

    async fn applied_with_private(&self, program: AccountId) -> Result<(Vec<Applied>, HashSet<AccountId>)> {
        let client = self.wallet.client();
        let mut touched = HashSet::new();
        let last = client.get_last_block_id().await.map_err(|e| anyhow!("get_last_block_id: {e}"))?;
        let mut out = Vec::new();
        let mut start = 1;
        while start <= last {
            let end = (start + 99).min(last);
            let blocks =
                client.get_block_range(start, end).await.map_err(|e| anyhow!("get_block_range {start}..{end}: {e}"))?;
            for block in blocks {
                for (i, tx) in block.body.transactions.iter().enumerate() {
                    if block.body.reverted.contains(&(i as u64)) {
                        continue;
                    }
                    match tx {
                        LeeTransaction::Public(tx) => {
                            let root = &tx.message.execution.root;
                            if root.to.program_account_id == program {
                                out.push(Applied { to: root.to, message: root.message.clone() });
                            }
                        }
                        LeeTransaction::PrivacyPreserving(tx) => touched.extend(tx.message.public_account_ids()),
                    }
                }
            }
            start = end + 1;
        }
        Ok((out, touched))
    }

    async fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        let account =
            self.wallet.client().get_account(actor.account_id).await.map_err(|e| anyhow!("get_account: {e}"))?;
        Ok(account.map(|a| a.data.actor_state(actor.program_account_id).to_vec()).unwrap_or_default())
    }

    async fn submit(&mut self, call: &LezCall) -> Result<String> {
        let mentions: Vec<AccountMention> = call.mentions.iter().map(mention).collect();
        let message = borsh::to_vec(&call.message)?;
        if matches!(call.mentions.first(), Some(Mention::PrivateSelf(_))) {
            let path = self.bridge_program.as_ref().ok_or_else(|| {
                anyhow!("a private claim needs --bridge-program (the bridge program's binary) to prove its execution")
            })?;
            let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            let program = call.mentions[0].actor().program_account_id;
            let catalog = lee::privacy_preserving_transaction::circuit::ProgramCatalog::from([(
                program,
                lee::program::Program::new(std::borrow::Cow::Owned(bytes))
                    .map_err(|e| anyhow!("the bridge program does not load: {e}"))?,
            )]);
            let (hash, _) = self
                .wallet
                .send_privacy_preserving_tx_with_pre_check(mentions, call.root_index, message, &catalog, None, |_| {
                    Ok(())
                })
                .await
                .map_err(|e| anyhow!("the wallet did not send the private claim: {e}: {e:?}"))?;
            return Ok(hash.to_string());
        }
        let hash = self
            .wallet
            .send_pub_tx_paid_by(mentions, call.root_index, message, self.payer)
            .await
            .map_err(|e| anyhow!("the wallet did not send it: {e}: {e:?}"))?;
        Ok(hash.to_string())
    }
}

// ---- the light-client instance a configuration reads ---------------------------------------------

/// `instance(id)` of the attestation program, as the zone holds it.
#[derive(Clone, Debug)]
pub struct InstanceView {
    pub active: bool,
    /// Jobs applied (the activation included).
    pub applied: u64,
    /// `finalized_header.beacon.slot` of the instance's store.
    pub finalized_slot: u64,
}

/// A `header(id, slot)` record: its beacon slot and what it binds of the execution block.
#[derive(Clone, Debug)]
pub struct RecordView {
    pub slot: u64,
    pub execution: ferrata_core::witness::ExecutionSummary,
}

impl RecordView {
    pub fn show(&self) -> String {
        use ferrata_core::witness::ExecutionSummary as E;
        match &self.execution {
            E::Payload { block_number, block_hash, timestamp, .. } => {
                format!(
                    "header(id, {}): execution block {block_number} {} (time {timestamp})",
                    self.slot,
                    hx(block_hash)
                )
            }
            E::BlockHash(h) => format!("header(id, {}): Gloas record, bound block hash {}", self.slot, hx(h)),
            E::None => format!("header(id, {}): Altair record, no execution data", self.slot),
        }
    }
}

/// `instance(id)`, if the zone holds it; its store's finalized slot read from the `CompactStore`.
pub async fn instance<L: Ledger>(
    ledger: &L,
    attestation: AccountId,
    instance_id: &Hash,
) -> Result<Option<InstanceView>> {
    let bytes = ledger.actor_state(ferrata_lez_wire::instance_actor(attestation, instance_id)).await?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let state = ferrata_lez_wire::InstanceState::try_from_slice(&bytes)
        .map_err(|e| anyhow!("instance(id) holds no instance state: {e}"))?;
    let store = ferrata_core::witness::CompactStore::decode(&state.store)
        .map_err(|e| anyhow!("instance(id)'s store does not decode: {e:?}"))?;
    Ok(Some(InstanceView { active: state.active, applied: state.applied, finalized_slot: store.finalized.slot }))
}

/// `header(id, slot)`'s record, if written.
pub async fn record<L: Ledger>(
    ledger: &L,
    attestation: AccountId,
    instance_id: &Hash,
    slot: u64,
) -> Result<Option<RecordView>> {
    let bytes = ledger.actor_state(ferrata_lez_wire::header_actor(attestation, instance_id, slot)).await?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let state = ferrata_lez_wire::RecordState::try_from_slice(&bytes)
        .map_err(|e| anyhow!("header(id, {slot}) holds no record: {e}"))?;
    let header = state.header().map_err(|code| {
        anyhow!("header(id, {slot})'s record does not decode (the attestation program's code {code})")
    })?;
    Ok(Some(RecordView { slot: header.beacon.slot, execution: header.execution }))
}

/// The zone's part of `status`: the configuration, its instance's newest record, the newest import
/// and the record it read, the burns published.
pub async fn zone_status<L: Ledger>(ledger: &L, ctx: &Context, history: &History) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let config = match read(ledger, config_actor(ctx.program, &ctx.cid)).await? {
        Some(BridgeState::Config(bw::ConfigState::Ready { config, gate, sunset, .. })) => {
            out.push(format!(
                "zone: configuration ready, sunset {sunset}; attestation program {}, instance {}",
                hx(config.attestation.value()),
                hx(&config.instance_id)
            ));
            out.push(format!("  gate {gate:?}"));
            config
        }
        other => {
            out.push(format!("zone: configuration {other:?}"));
            return Ok(out);
        }
    };
    match instance(ledger, config.attestation, &config.instance_id).await? {
        Some(i) => {
            out.push(format!(
                "  instance: active {}, {} jobs applied, finalized slot {}",
                i.active, i.applied, i.finalized_slot
            ));
            match record(ledger, config.attestation, &config.instance_id, i.finalized_slot).await? {
                Some(r) => out.push(format!("  newest record: {}", r.show())),
                None => out.push(format!("  newest record: header(id, {}) not written yet", i.finalized_slot)),
            }
        }
        None => out.push("  instance(id) is not on this zone (its records are read where written)".to_string()),
    }
    match history.imports.iter().max_by_key(|(n, _)| *n) {
        Some((n, slot)) => {
            let read_from = match record(ledger, config.attestation, &config.instance_id, *slot).await? {
                Some(r) => r.show(),
                None => format!("header(id, {slot}), not readable"),
            };
            out.push(format!(
                "  newest imported root: root(cid, {n}) ({} imports), from {read_from}",
                history.roots.len()
            ));
        }
        None => out.push("  no root imported yet".to_string()),
    }
    out.push(format!("  burns published: {}", history.burns.len()));
    Ok(out)
}

/// A plain description of what `call` makes public.
pub fn reveals(call: &LezCall) -> &'static str {
    match &call.message {
        BridgeMessage::Claim { .. } => {
            "PLAINTEXT CLAIM: the whole witness is public — which deposit (leaf, depositor, j), the secret s, the destination; \
             anyone can link this mint to the Ethereum deposit"
        }
        BridgeMessage::ClaimPrivately { .. } => {
            "private claim: N, the token, the denomination, the fee schedule's range, the root read, the relayer, when (its block); \
             not the deposit, the path, s or the private destination"
        }
        BridgeMessage::Burn { .. } => {
            "burn: the holder's account (a public holding), the token, the denomination, the time and the note (r, memo, mac, COut); \
             not the Ethereum recipient (encrypted in the memo)"
        }
        _ => "the whole message",
    }
}
