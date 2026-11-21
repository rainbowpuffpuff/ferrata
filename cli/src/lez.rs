//! The LEZ side: what the tool reads from a zone, the calls it builds (claim, burn), the checks it
//! runs before sending — the bridge program's own check functions from `ferrata-lez-bridge-wire`,
//! so a refusal names the code the program would exit with — and submission through LEZ's own
//! wallet library to a sequencer endpoint.

use std::collections::HashSet;
#[cfg(feature = "lez-wallet")]
use std::path::{Path, PathBuf};

#[cfg(feature = "lez-wallet")]
use anyhow::ensure;
use anyhow::{anyhow, bail, Context as _, Result};
use borsh::BorshDeserialize as _;
#[cfg(feature = "lez-wallet")]
use common::transaction::LeeTransaction;
use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{self, DestKind, OwnedDeposit},
    outbound::{recover_burns, BurnNote, PublishedBurn, ReleaseTarget},
    BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{
    self as bw, burn_actor, burn_seed, config_actor, config_seed, holder_actor, holder_seed, nullifier_actor,
    own_actor, root_actor, token_actor, token_seed, BridgeMessage, BridgeState, HolderState, RootState, TokenState,
};
use lee::AccountId;
use lee_core::{account::Actor, program::PdaSeed};
#[cfg(feature = "lez-wallet")]
use sequencer_service_rpc::SequencerClient;
#[cfg(feature = "lez-wallet")]
use wallet::{AccountIdentity, AccountMention, WalletCore, WalletSequencerClient as _};

use ferrata_wallet::claim;

#[cfg(feature = "lez-wallet")]
use crate::seed::{Seed, DEFAULT_GAP};
use crate::{codes::program_refusal, hx, units::u128_to_word};

/// How many unused burn counters end a burn scan.
pub const BURN_GAP: u64 = 20;

/// Which configuration of which bridge program, and the vault it pairs with (shared with
/// `ferrata_bridge_module` through `ferrata-wallet`).
pub use ferrata_wallet::Context;

/// One account a call declares.
#[derive(Clone, Debug)]
pub enum Mention {
    /// A PDA of the bridge program.
    Pda { actor: Actor, seed: PdaSeed },
    /// A public account that signs the call (a burn's holder).
    Signer(Actor),
    /// A public account named with its public key when it may not exist yet (LEZ admits a new
    /// account on evidence). A claim credits `holder(account)` PDAs, named as [`Mention::Pda`].
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

impl From<ferrata_wallet::Mention> for Mention {
    fn from(m: ferrata_wallet::Mention) -> Self {
        match m {
            ferrata_wallet::Mention::Pda { actor, seed } => Mention::Pda { actor, seed },
            ferrata_wallet::Mention::Signer(actor) => Mention::Signer(actor),
            ferrata_wallet::Mention::PrivateSelf(actor) => Mention::PrivateSelf(actor),
            ferrata_wallet::Mention::Public(actor) => Mention::Other(actor, None),
        }
    }
}

/// A refusal of the shared wallet code as the tool reports it (with the program's code and meaning).
pub fn refused(r: ferrata_wallet::Refusal) -> anyhow::Error {
    match r {
        ferrata_wallet::Refusal::Program { code, detail } => anyhow!(program_refusal(u32::from(code), detail)),
        ferrata_wallet::Refusal::Wallet(detail) => anyhow!(detail),
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
    /// The wallet's own view of its private account `id`, decrypted from the zone (LEZ's wallet
    /// syncs and decrypts it); `None` if the wallet does not know it.
    async fn private_account(&mut self, _id: AccountId) -> Result<Option<lee::Account>> {
        Ok(None)
    }
}

/// The bridge program's calls a privacy-preserving transaction makes in the clear: its public root,
/// if any, and its `PrivateToPublic` deliveries to the program's actors (a private burn's note
/// reaches `token(cid, token)` this way; the checkpoint counts the same calls).
pub fn visible_calls(tx: &lee::PrivacyPreservingTransaction, program: AccountId) -> Vec<Applied> {
    let execution = &tx.message.execution;
    let mut out = Vec::new();
    if let Some(root) = &execution.public_root {
        if root.to.program_account_id == program {
            out.push(Applied { to: root.to, message: root.message.clone() });
        }
    }
    for step in &execution.boundary {
        if let lee_core::execution_state::BoundaryStep::PrivateToPublic(d) = step {
            if d.envelope.to.program_account_id == program {
                out.push(Applied { to: d.envelope.to, message: d.envelope.message.clone() });
            }
        }
    }
    out
}

/// The amount of `token` under `ctx`'s configuration in a holder's bridge state (0 if none).
pub fn holding_in(state: &[u8], ctx: &Context, token: &[u8; 20]) -> u128 {
    match BridgeState::try_from_slice(state) {
        Ok(BridgeState::Holder(HolderState { holdings, .. })) => {
            holdings.iter().find(|h| h.config_id == ctx.cid && h.token == *token).map_or(0, |h| h.amount)
        }
        _ => 0,
    }
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

/// The schedule range a wallet names for a claim of `leaf` (the widest its token state allows).
pub use ferrata_wallet::claim::range_for;

async fn token_state<L: Ledger>(ledger: &L, ctx: &Context, token: &[u8; 20]) -> Result<TokenState> {
    match read(ledger, token_actor(ctx.program, &ctx.cid, token)).await? {
        Some(BridgeState::Token(t)) => Ok(t),
        None => bail!(program_refusal(251, "checked before sending: no token(cid, token) account")),
        Some(other) => bail!("token(cid, token) holds another state: {other:?}"),
    }
}

/// The freezes a mint (`mints`) or a burn of `token` meets, as the bridge program checks them: the token's
/// gate, then the configuration's (`Gate::admit` at `token(cid, token)` and at `config(cid)`), each frozen while
/// its attested clock is before the freeze's end. `None` when neither is frozen. The refusal says until when
/// and what to do, and names no deposit and no burn (U.08).
pub async fn gate_refusal<L: Ledger>(
    ledger: &L,
    ctx: &Context,
    token: &TokenState,
    mints: bool,
) -> Result<Option<crate::codes::Refusal>> {
    let what = if mints { "mints (claims)" } else { "burns" };
    let consumed = if mints {
        "nothing is sent and nothing is consumed: the deposit stays claimable"
    } else {
        "nothing is sent and nothing is burned: the holding stays yours"
    };
    let frozen = |g: &bw::Gate| {
        let until = if mints { g.mints_until } else { g.burns_until };
        (g.now_ms < until).then_some(until)
    };
    let mut scopes = vec![("of this token", frozen(&token.gate))];
    if let Some(BridgeState::Config(bw::ConfigState::Ready { gate, .. })) =
        read(ledger, config_actor(ctx.program, &ctx.cid)).await?
    {
        scopes.push(("of the whole configuration", frozen(&gate)));
    }
    Ok(scopes.into_iter().find_map(|(scope, until)| {
        until.map(|until| {
            program_refusal(
                bw::code::FROZEN as u32,
                format!(
                    "checked before sending: {what} {scope} are frozen until Ethereum time {} ({} as the bridge's \
                     attested clock counts it; a freeze authority may lift it sooner); {consumed}",
                    until / 1000,
                    crate::units::utc(until / 1000)
                ),
            )
        })
    }))
}

/// One gate's freezes in words, for `status`.
pub fn gate_line(scope: &str, g: &bw::Gate) -> String {
    let one = |until: u64| {
        if g.now_ms < until {
            format!("frozen until {} ({})", until / 1000, crate::units::utc(until / 1000))
        } else {
            "not frozen".to_string()
        }
    };
    format!(
        "  {scope}: mints {}; burns {}; clock {} ({})",
        one(g.mints_until),
        one(g.burns_until),
        g.now_ms / 1000,
        crate::units::utc(g.now_ms / 1000)
    )
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
/// that covers the leaf, read with its token's state and the nullifier from the zone, then the shared
/// builder (`ferrata_wallet::claim`: the path, the range, the program's checks, the accounts).
#[allow(clippy::too_many_arguments)]
pub async fn plan_claim<L: Ledger>(
    ledger: &L,
    ctx: &Context,
    leaves: &[Hash],
    owned: &OwnedDeposit,
    history: &History,
    private: bool,
    relayer: Option<Hash>,
    prover: Option<AccountId>,
) -> Result<ClaimPlan> {
    let n = claim::pick_import(&history.roots, owned.deposit.leaf_index).map_err(refused)?;
    let root_state: RootState = match read(ledger, root_actor(ctx.program, &ctx.cid, n)).await? {
        Some(BridgeState::Root(r)) => r,
        _ => bail!(program_refusal(245, format!("checked before sending: root(cid, {n}) is not on the zone"))),
    };
    let token = token_state(ledger, ctx, &owned.deposit.token).await?;
    if let Some(r) = gate_refusal(ledger, ctx, &token, true).await? {
        bail!(r);
    }
    let n_actor = nullifier_actor(ctx.program, ctx.chain_id, &ctx.vault, &owned.nullifier);
    let spent = history.claimed.contains(&owned.nullifier) || read(ledger, n_actor).await?.is_some();
    let inputs = claim::ClaimInputs { leaves, leaf_count: n, root: &root_state, token: &token, spent };
    let plan = claim::build_claim(ctx, owned, &inputs, private, relayer, prover).map_err(refused)?;
    Ok(ClaimPlan {
        call: LezCall {
            what: plan.what,
            mentions: plan.mentions.into_iter().map(Mention::from).collect(),
            root_index: plan.root_index,
            message: plan.message,
        },
        leaf_count: plan.leaf_count,
        nullifier: plan.nullifier,
        mint_fee: plan.mint_fee,
        relay_fee: plan.relay_fee,
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

/// Who burns: a public account (it signs; its holding is read from the zone) or a private account
/// of the wallet (it proves; its holding is the wallet's own view of it).
#[derive(Clone, Copy, Debug)]
pub enum BurnFrom {
    Public(AccountId),
    Private { account: AccountId, held: u128 },
}

/// Builds the burn of one denomination `amount` of `token` held by `holder` under `ctx`, to be
/// released to `recipient` on `ctx`'s vault.
#[allow(clippy::too_many_arguments)]
pub async fn plan_burn<L: Ledger>(
    ledger: &L,
    ctx: &Context,
    keys: &BridgeKeys,
    holder: BurnFrom,
    token: [u8; 20],
    amount: u128,
    recipient: [u8; 20],
    history: &History,
) -> Result<BurnPlan> {
    let t = token_state(ledger, ctx, &token).await?;
    if t.entry.index_of(amount).is_none() {
        bail!(program_refusal(251, "checked before sending: the amount is not one of the token's denominations"));
    }
    let held = match holder {
        BurnFrom::Public(account) => match read(ledger, holder_actor(ctx.program, account)).await? {
            Some(BridgeState::Holder(HolderState { holdings, .. })) => {
                holdings.iter().find(|h| h.config_id == ctx.cid && h.token == token).map_or(0, |h| h.amount)
            }
            _ => 0,
        },
        BurnFrom::Private { held, .. } => held,
    };
    if held < amount {
        bail!(program_refusal(254, format!("checked before sending: the holding is {held} base units")));
    }
    if let Some(r) = gate_refusal(ledger, ctx, &t, false).await? {
        bail!(r);
    }
    let chain = chain_id_u256(ctx.chain_id);
    let next_i = recover_burns(keys, &ctx.cid, &chain, &ctx.vault, &history.burns, BURN_GAP).next_i;
    let target = ReleaseTarget { chain_id: chain, vault: ctx.vault, recipient };
    let note = BurnNote::create(keys, &ctx.cid, next_i, &target, token, u128_to_word(amount));
    let wire = bw::Note { r: note.r, memo: note.memo, mac: note.mac, token, amount: note.amount, c_out: note.c_out };
    let note_hash = wire.hash();
    let mut mentions = match holder {
        // Signed at the holder's own actor, which forwards the note to its `holder(account)`.
        BurnFrom::Public(account) => vec![
            Mention::Signer(own_actor(ctx.program, account)),
            Mention::Pda { actor: holder_actor(ctx.program, account), seed: holder_seed(account.value()) },
        ],
        // Proven at the private account's own actor, which debits its private holding and sends the
        // note to the token's account in the clear.
        BurnFrom::Private { account, .. } => vec![Mention::PrivateSelf(own_actor(ctx.program, account))],
    };
    mentions.extend([
        Mention::Pda { actor: token_actor(ctx.program, &ctx.cid, &token), seed: token_seed(&ctx.cid, &token) },
        Mention::Pda { actor: config_actor(ctx.program, &ctx.cid), seed: config_seed(&ctx.cid) },
        Mention::Pda { actor: burn_actor(ctx.program, &ctx.cid, &note_hash), seed: burn_seed(&ctx.cid, &note_hash) },
    ]);
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
#[cfg(feature = "lez-wallet")]
pub struct Sequencer {
    pub wallet: WalletCore<SequencerClient>,
    /// The account that pays the fee (it must be in the wallet).
    pub payer: Option<AccountId>,
    /// The bridge program's binary, for private execution (`ClaimPrivately`, a private `Burn`).
    pub bridge_program: Option<PathBuf>,
    /// Whether the wallet has synced its private accounts with the zone since it last sent.
    pub synced: bool,
}

#[cfg(feature = "lez-wallet")]
fn wallet_config_json(rpc: &str) -> String {
    format!(
        "{{\n  \"sequencer\": {{ \"sequencer_addr\": \"{rpc}\" }},\n  \"seq_poll_timeout\": \"1s\",\n  \
         \"seq_tx_poll_max_blocks\": 120,\n  \"seq_poll_max_retries\": 2,\n  \"seq_block_poll_max_amount\": 100,\n  \
         \"gas_limit\": 4000000\n}}\n"
    )
}

/// Opens (or creates from the seed) the LEZ wallet at `home`, its storage encrypted under the
/// password in `password_file`, and checks that its first accounts are the seed's.
#[cfg(feature = "lez-wallet")]
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

#[cfg(feature = "lez-wallet")]
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

#[cfg(feature = "lez-wallet")]
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
                        LeeTransaction::PrivacyPreserving(tx) => {
                            touched.extend(tx.message.public_account_ids());
                            out.extend(visible_calls(tx, program));
                        }
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
        self.synced = false;
        let mentions: Vec<AccountMention> = call.mentions.iter().map(mention).collect();
        let message = borsh::to_vec(&call.message)?;
        if matches!(call.mentions.first(), Some(Mention::PrivateSelf(_))) {
            let path = self.bridge_program.as_ref().ok_or_else(|| {
                anyhow!("a private claim or burn needs --bridge-program (the bridge program's binary) to prove its execution")
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
                .map_err(|e| anyhow!("the wallet did not send the private transaction: {e}: {e:?}"))?;
            return Ok(hash.to_string());
        }
        let hash = self
            .wallet
            .send_pub_tx_paid_by(mentions, call.root_index, message, self.payer)
            .await
            .map_err(|e| anyhow!("the wallet did not send it: {e}: {e:?}"))?;
        Ok(hash.to_string())
    }

    async fn private_account(&mut self, id: AccountId) -> Result<Option<lee::Account>> {
        if !self.synced {
            self.wallet
                .sync_to_latest_block()
                .await
                .map_err(|e| anyhow!("syncing the LEZ wallet with the zone: {e}"))?;
            self.synced = true;
        }
        Ok(self.wallet.get_account_private(id))
    }
}

// ---- the mini-app devnet's zone, over its HTTP methods (a stand-in for a sequencer) ----------------

/// The zone of the mini-app's devnet (`cli/examples/miniapp_devnet.rs`: LEZ v0.4.0-rc1's state transition
/// with the bridge program, in the LEZ harness), reached over the devnet's HTTP methods in place of a
/// sequencer's RPC. A stand-in for tests: reads are `zone.applied` and `zone.actor_state`; a call is
/// sent with `module.submit`, which the devnet signs and settles as LEZ's wallet sends a public call
/// (this side holds no LEZ key). Loopback addresses only.
#[derive(Clone, Debug)]
pub struct DevnetZone {
    /// The devnet's endpoint, `http://127.0.0.1:PORT/rpc`.
    pub url: String,
}

impl DevnetZone {
    pub fn new(url: &str) -> Result<Self> {
        let u = url::Url::parse(url).with_context(|| format!("the devnet's URL {url}"))?;
        if u.scheme() != "http" {
            bail!("the devnet's URL must be http:// (a local devnet), not {}", u.scheme());
        }
        match u.host_str() {
            Some("127.0.0.1" | "localhost" | "[::1]") => {}
            other => bail!("the devnet zone is a local stand-in: {other:?} is not a loopback address"),
        }
        Ok(Self { url: url.to_string() })
    }

    /// One method of the devnet: `{"method", "params"}` posted to its endpoint; its `result`, or its error.
    pub async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let u = url::Url::parse(&self.url)?;
        let host = u.host_str().unwrap_or("127.0.0.1").trim_matches(['[', ']']).to_string();
        let port = u.port_or_known_default().unwrap_or(80);
        let path = if u.path().is_empty() { "/" } else { u.path() };
        let body = serde_json::json!({ "method": method, "params": params }).to_string();
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        let mut stream = tokio::net::TcpStream::connect((host.as_str(), port))
            .await
            .with_context(|| format!("connection to the devnet at {}", self.url))?;
        stream.write_all(request.as_bytes()).await?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await?;
        let at =
            raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| anyhow!("the devnet's reply has no body"))?;
        let reply: serde_json::Value =
            serde_json::from_slice(&raw[at + 4..]).with_context(|| format!("the devnet's reply to {method}"))?;
        if reply["ok"] != serde_json::Value::Bool(true) {
            bail!("the devnet refused {method}: {}", reply["error"].as_str().unwrap_or("no reason given"));
        }
        Ok(reply["result"].clone())
    }
}

fn devnet_account(v: &serde_json::Value, what: &str) -> Result<AccountId> {
    Ok(AccountId::new(crate::h32(v.as_str().ok_or_else(|| anyhow!("{what}: not a string"))?)?))
}

impl Ledger for DevnetZone {
    async fn applied(&self, program: AccountId) -> Result<Vec<Applied>> {
        Ok(self.applied_with_private(program).await?.0)
    }

    async fn applied_with_private(&self, program: AccountId) -> Result<(Vec<Applied>, HashSet<AccountId>)> {
        let r = self.call("zone.applied", serde_json::json!({ "program": hx(program.value()) })).await?;
        let mut out = Vec::new();
        for a in r["applied"].as_array().ok_or_else(|| anyhow!("zone.applied: no list"))? {
            let to = Actor::new(devnet_account(&a["account"], "account")?, devnet_account(&a["program"], "program")?);
            let message = hex::decode(a["message"].as_str().unwrap_or_default().trim_start_matches("0x"))?;
            out.push(Applied { to, message });
        }
        let mut touched = HashSet::new();
        for t in r["touched"].as_array().ok_or_else(|| anyhow!("zone.applied: no touched list"))? {
            touched.insert(devnet_account(t, "touched")?);
        }
        Ok((out, touched))
    }

    async fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        let r = self
            .call(
                "zone.actor_state",
                serde_json::json!({ "program": hx(actor.program_account_id.value()), "account": hx(actor.account_id.value()) }),
            )
            .await?;
        Ok(hex::decode(r["state"].as_str().unwrap_or_default().trim_start_matches("0x"))?)
    }

    async fn submit(&mut self, call: &LezCall) -> Result<String> {
        let mut accounts = Vec::new();
        for m in &call.mentions {
            let a = m.actor();
            let mut v =
                serde_json::json!({ "account": hx(a.account_id.value()), "program": hx(a.program_account_id.value()) });
            match m {
                Mention::Pda { seed, .. } => {
                    v["kind"] = "pda".into();
                    v["seed"] = hx(seed.as_bytes()).into();
                }
                Mention::Signer(_) => v["kind"] = "signer".into(),
                Mention::Other(_, None) => v["kind"] = "public".into(),
                Mention::Other(_, Some(_)) => bail!("the devnet zone admits no new public account by its key"),
                Mention::PrivateSelf(_) => v["kind"] = "private_self".into(),
            }
            accounts.push(v);
        }
        let r = self
            .call(
                "module.submit",
                serde_json::json!({
                    "what": call.what, "message": hx(&borsh::to_vec(&call.message)?),
                    "accounts": accounts, "root_index": call.root_index,
                }),
            )
            .await?;
        if r["settled"] != serde_json::Value::Bool(true) {
            match r["exit_code"].as_u64() {
                Some(code) => bail!(program_refusal(code as u32, "returned by the program on the devnet")),
                None => bail!("the devnet did not settle it: {}", r["revert"]),
            }
        }
        Ok(format!("devnet block {}", r["block"]))
    }
}

/// The zone a relayer reaches: a sequencer through LEZ's wallet, or the mini-app devnet's zone.
pub enum Zone {
    #[cfg(feature = "lez-wallet")]
    Sequencer(Sequencer),
    Devnet(DevnetZone),
}

impl Zone {
    /// `sequencer` or `devnet`.
    pub fn kind(&self) -> &'static str {
        match self {
            #[cfg(feature = "lez-wallet")]
            Zone::Sequencer(_) => "sequencer",
            Zone::Devnet(_) => "devnet",
        }
    }
}

impl Ledger for Zone {
    async fn applied(&self, program: AccountId) -> Result<Vec<Applied>> {
        match self {
            #[cfg(feature = "lez-wallet")]
            Zone::Sequencer(s) => s.applied(program).await,
            Zone::Devnet(d) => d.applied(program).await,
        }
    }

    async fn applied_with_private(&self, program: AccountId) -> Result<(Vec<Applied>, HashSet<AccountId>)> {
        match self {
            #[cfg(feature = "lez-wallet")]
            Zone::Sequencer(s) => s.applied_with_private(program).await,
            Zone::Devnet(d) => d.applied_with_private(program).await,
        }
    }

    async fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        match self {
            #[cfg(feature = "lez-wallet")]
            Zone::Sequencer(s) => s.actor_state(actor).await,
            Zone::Devnet(d) => d.actor_state(actor).await,
        }
    }

    async fn submit(&mut self, call: &LezCall) -> Result<String> {
        match self {
            #[cfg(feature = "lez-wallet")]
            Zone::Sequencer(s) => s.submit(call).await,
            Zone::Devnet(d) => d.submit(call).await,
        }
    }

    async fn private_account(&mut self, id: AccountId) -> Result<Option<lee::Account>> {
        match self {
            #[cfg(feature = "lez-wallet")]
            Zone::Sequencer(s) => s.private_account(id).await,
            Zone::Devnet(d) => d.private_account(id).await,
        }
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
            out.push(gate_line("freezes of the whole configuration", &gate));
            if let Some(BridgeState::Token(t)) = read(ledger, token_actor(ctx.program, &ctx.cid, &[0; 20])).await? {
                out.push(gate_line("freezes of ETH", &t.gate));
            }
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
        BridgeMessage::Burn { .. } if matches!(call.mentions.first(), Some(Mention::PrivateSelf(_))) => {
            "private burn: the note (r, memo, mac, COut), the token, the denomination and the time; \
             not the holder (a private account, shown as an alias) or the Ethereum recipient (encrypted in the memo)"
        }
        BridgeMessage::Burn { .. } => {
            "burn: the holder's account (a public holding), the token, the denomination, the time and the note (r, memo, mac, COut); \
             not the Ethereum recipient (encrypted in the memo)"
        }
        _ => "the whole message",
    }
}
