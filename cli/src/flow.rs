//! The user's commands as library functions, shared by the `ferrata` binary and its end-to-end test
//! (RFP-022 U.05–U.09; proposal B3). Each one:
//! - picks what to act on by the wallet's rules: the oldest claimable deposit whose claim delay has
//!   passed; the oldest unreleased burn, against the newest accepted burn root whose release delay
//!   has passed;
//! - runs the checks the bridge program or the vault would run, so a refusal comes before anything
//!   is signed;
//! - prints what it would send, what becomes public (with the exposure of the current choices, as the
//!   mini-app shows it) and what it costs;
//! - sends only with `send` and `yes`; a public path also needs its own acknowledgement.

use std::path::PathBuf;

use alloy::{
    primitives::{Address, Bytes, B256, U256},
    providers::DynProvider,
    sol_types::SolCall as _,
};
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use ferrata_bridge_core::{chain_id_u256, inbound, inbound::DestKind, Hash};
use lee::AccountId;

use crate::{
    codes,
    eth::{self, IVault},
    hx,
    lez::{self, BurnFrom, Context, Ledger},
    privacy, release,
    scan::{self, DepositState},
    seed::Seed,
    units,
};

/// A command's report: each line printed as it comes (`echo`) and kept for the caller.
#[derive(Default)]
pub struct Out {
    pub lines: Vec<String>,
    pub echo: bool,
}

impl Out {
    pub fn new(echo: bool) -> Self {
        Self { lines: Vec::new(), echo }
    }

    pub fn line(&mut self, s: impl Into<String>) {
        let s = s.into();
        if self.echo {
            println!("{s}");
        }
        self.lines.push(s);
    }

    pub fn extend(&mut self, lines: Vec<String>) {
        for l in lines {
            self.line(l);
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

/// The Ethereum side a command reads: any JSON-RPC endpoint, the vault, where its events start.
pub struct Eth {
    pub p: DynProvider,
    pub vault: Address,
    pub chain_id: u64,
    pub from_block: u64,
    pub chunk: u64,
}

/// A signer for Ethereum: a provider that signs, and its address.
pub struct Signer {
    pub p: DynProvider,
    pub address: Address,
}

/// Seconds per Ethereum slot: blocks × 12 s is a lower bound of the time passed (a missed slot only
/// adds time), measured without asking a provider about any one deposit's block.
pub const SLOT_SECONDS: u64 = 12;

/// The block from which a deposit made in `deposit_block` leaves its claim delay of `hours`.
pub fn claimable_from(deposit_block: u64, hours: u64) -> u64 {
    deposit_block + hours * 3600 / SLOT_SECONDS
}

fn kind_name(k: DestKind) -> &'static str {
    if k == DestKind::Private {
        "private"
    } else {
        "public"
    }
}

fn confirm(out: &mut Out, send: bool, yes: bool) -> Result<bool> {
    if !send {
        out.line("not sent: a dry run (add --send --yes to sign and send)");
        return Ok(false);
    }
    ensure!(yes, "refusing to send without --yes: read what becomes public above, then add --yes");
    Ok(true)
}

fn vault_refusal(name: &str, how: &str) -> anyhow::Error {
    anyhow!(codes::vault_refusal(&codes::selector(name), how).expect("a known vault error"))
}

/// The fees a release of `amount` of `token` pays now: (denomination index, burn fee, checkpoint
/// fee, relay fee) of the fee schedule in force.
pub async fn release_fees(eth: &Eth, token: Address, amount: u128) -> Result<(u8, u128, u128, u128)> {
    let cfg = eth::view(&eth.p, eth.vault, IVault::tokenConfigCall { token }, None).await?;
    ensure!(cfg.registered, vault_refusal("TokenNotRegistered", "checked before sending"));
    let k = cfg
        .denominations_
        .iter()
        .position(|d| *d == U256::from(amount))
        .ok_or_else(|| vault_refusal("InvalidDenominationIndex", "checked before sending: not a denomination"))?
        as u8;
    let state = eth::view(&eth.p, eth.vault, IVault::feeStateCall { token }, None).await?;
    let now = eth::now(&eth.p).await?;
    let schedule = if state.pendingAt != 0 && now >= state.pendingAt { state.current + 1 } else { state.current };
    let f = eth::view(&eth.p, eth.vault, IVault::feesCall { token, index: U256::from(schedule), denomIndex: k }, None)
        .await?;
    Ok((k, u128::try_from(f.burnFee)?, u128::try_from(f.checkpointFee)?, u128::try_from(f.relayFee)?))
}

// ---- deposit -------------------------------------------------------------------------------------

pub struct DepositOpts {
    pub token: Address,
    pub denomination: u8,
    pub public: bool,
    pub accept_public: bool,
    pub account: u32,
    pub gap: u32,
    /// The depositor, when only printing (`--from`).
    pub from: Option<Address>,
    /// Send the ERC-20 `approve` the deposit needs first.
    pub approve: bool,
    pub send: bool,
    pub yes: bool,
    pub confirmations: u64,
}

pub struct Deposited {
    pub commitment: Hash,
    pub nullifier: Hash,
    pub j: u64,
    pub sent: Option<(B256, Option<u64>)>,
}

/// `ferrata deposit`.
pub async fn deposit(
    eth: &Eth,
    seed: &Seed,
    signer: Option<&Signer>,
    o: &DepositOpts,
    out: &mut Out,
) -> Result<Deposited> {
    let depositor = match (signer, o.from) {
        (Some(s), _) => s.address,
        (None, Some(f)) => f,
        (None, None) => bail!("--from (to print) or --keystore and --password-file (to send) name the depositor"),
    };
    ensure!(
        o.account < o.gap,
        "account /{} is outside the recovery window (--gap {}): a wallet restored from the seed would not find it",
        o.account,
        o.gap
    );
    let price = eth::deposit_price(&eth.p, eth.vault, o.token, o.denomination).await?;
    let (decimals, symbol) = eth::token_meta(&eth.p, o.token).await?;
    let j = eth::view(&eth.p, eth.vault, IVault::depositCountCall { depositor }, None).await?;
    let keys = seed.bridge_keys();
    let dests = seed.destinations(o.gap)?;
    let dk = if o.public { DestKind::Public } else { DestKind::Private };
    let chain = chain_id_u256(eth.chain_id);
    let s = inbound::deposit_secret(&keys.auth, &chain, &eth.vault.into_array(), &depositor.into_array(), j);
    let dest = dests.of(dk, o.account);
    let commitment = inbound::commitment(&s, dk, &dest);
    let input = eth::deposit_calldata(o.token, o.denomination, B256::from(commitment), j);
    let value = if o.token == eth::ETH { U256::from(price.total) } else { U256::ZERO };
    // The claim delay this deposit will get: it depends on N, which depends on the leaf index the
    // vault assigns; the leaf is the next one unless another deposit lands first.
    let next_leaf = eth::view(&eth.p, eth.vault, IVault::nextLeafIndexCall {}, None).await?;
    let leaf = inbound::deposit_leaf(
        next_leaf,
        &depositor.into_array(),
        &o.token.into_array(),
        &U256::from(price.denomination).to_be_bytes::<32>(),
        &commitment,
    );
    let fid = inbound::deposit_id(&chain, &eth.vault.into_array(), next_leaf, &leaf);
    let nullifier = inbound::nullifier(&s, &fid);
    let delay = seed.claim_delay_hours(&nullifier);
    let show = |x: u128| units::show(x, decimals, &symbol);
    out.line(format!("deposit to the vault {} on chain {}", eth.vault, eth.chain_id));
    out.line(format!("  denomination  {}", show(price.denomination)));
    out.line(format!("  mint fee      {}  (schedule {})", show(price.mint_fee), price.schedule));
    out.line(format!("  relay fee     {}  (paid to the relayer that submits your claim)", show(price.relay_fee)));
    out.line(format!("  total         {}", show(price.total)));
    out.line(format!("  depositor     {depositor}   j = {j}"));
    out.line(format!("  destination   your {} LEZ account /{} (hidden: C commits to it)", kind_name(dk), o.account));
    out.line(format!("  commitment C  {}", hx(&commitment)));
    out.line(format!("  claim delay   {delay} h after the deposit (from your seed; `ferrata claim` waits for it)"));
    let mut approve_call: Option<Bytes> = None;
    if o.token != eth::ETH {
        let allowance =
            eth::view(&eth.p, o.token, eth::IERC20Meta::allowanceCall { owner: depositor, spender: eth.vault }, None)
                .await?;
        out.line(format!(
            "  allowance     {} of {} needed for the vault",
            u128::try_from(allowance).map(show).unwrap_or_else(|_| allowance.to_string()),
            show(price.total)
        ));
        if allowance < U256::from(price.total) {
            if o.approve {
                let data = Bytes::from(
                    eth::IERC20Meta::approveCall { spender: eth.vault, amount: U256::from(price.total) }.abi_encode(),
                );
                out.line(format!(
                    "would send first: approve({}, {}) to the token {}  data {data}",
                    eth.vault, price.total, o.token
                ));
                approve_call = Some(data);
            } else {
                out.line("  the deposit needs that allowance first: add --approve to send the approve before it");
            }
        }
    }
    out.line(format!("would send: to {}  value {}  data {}", eth.vault, value, input));
    if let Some(data) = &approve_call {
        match eth::preflight(&eth.p, depositor, o.token, data.clone(), U256::ZERO).await {
            Ok(f) => out.line(format!(
                "cost: {} to the vault; the approve's network fee about {} ({} gas, eth_estimateGas); the deposit's is estimated once the allowance is in place",
                show(price.total),
                units::show(f.fee(), 18, "ETH"),
                f.gas
            )),
            Err(e) if signer.is_some() && o.send => return Err(e),
            Err(e) => {
                out.line(format!("cost: {} to the vault; the approve does not pass eth_call now: {e}", show(price.total)))
            }
        }
    } else {
        match eth::preflight(&eth.p, depositor, eth.vault, input.clone(), value).await {
            Ok(f) => out.line(format!(
                "cost: {} to the vault, and a network fee of about {} ({} gas at {} wei, eth_estimateGas)",
                show(price.total),
                units::show(f.fee(), 18, "ETH"),
                f.gas,
                f.gas_price
            )),
            Err(e) if signer.is_some() && o.send => return Err(e),
            Err(e) => {
                out.line(format!("cost: {} to the vault; the call does not pass eth_call now: {e}", show(price.total)))
            }
        }
    }
    let leaks = privacy::deposit_leaks(&privacy::DepositChoice {
        private: !o.public,
        delay_hours: Some(delay),
        relayer: true,
        account: o.account,
        gap: o.gap,
        denomination: &show(price.denomination),
    });
    out.extend(privacy::render(&privacy::DEPOSIT, &leaks));
    if approve_call.is_some() {
        out.line("  the approve is public too: your address, the token, the vault and the amount, which the deposit shows anyway");
    }
    if o.public && !o.accept_public {
        if o.send {
            bail!("refusing a public destination without --i-accept-public-account: the account and its balance become public on LEZ when the deposit is claimed; deposit to a private account (the default) instead");
        }
        out.line("a public destination is sent only with --i-accept-public-account");
    }
    if !confirm(out, o.send, o.yes)? {
        return Ok(Deposited { commitment, nullifier, j, sent: None });
    }
    let signer = signer.ok_or_else(|| anyhow!("--send needs --keystore and --password-file"))?;
    if let Some(data) = approve_call {
        let rc = eth::send(&signer.p, depositor, o.token, data, U256::ZERO, o.confirmations).await?;
        out.line(format!("approved: {} in block {}", rc.transaction_hash, rc.block_number.unwrap_or_default()));
    }
    let receipt = eth::send(&signer.p, depositor, eth.vault, input, value, o.confirmations).await?;
    let leaf = eth::deposit_of_receipt(&receipt).map(|d| d.leafIndex);
    out.line(format!("sent: {} in block {}", receipt.transaction_hash, receipt.block_number.unwrap_or_default()));
    if leaf != Some(next_leaf) {
        out.line("note: another deposit landed first, so this deposit's claim delay differs from the one printed (`ferrata scan` shows it)");
    }
    Ok(Deposited { commitment, nullifier, j, sent: Some((receipt.transaction_hash, leaf)) })
}

// ---- claim ---------------------------------------------------------------------------------------

pub struct ClaimOpts {
    /// The deposit (default: the oldest claimable one whose delay has passed).
    pub leaf: Option<u64>,
    pub plaintext: bool,
    pub accept_plaintext: bool,
    /// Claim before the deposit's claim delay ends.
    pub no_delay: bool,
    /// A LEZ relayer account paid the registered relay fee.
    pub relayer: Option<Hash>,
    pub gap: u32,
    pub send: bool,
    pub yes: bool,
}

pub struct Claimed {
    pub plan: lez::ClaimPlan,
    pub leaf: u64,
    pub sent: Option<String>,
}

/// `ferrata claim`: private by default.
pub async fn claim<L: Ledger>(
    eth: &Eth,
    seed: &Seed,
    ledger: &mut L,
    ctx: &Context,
    o: &ClaimOpts,
    out: &mut Out,
) -> Result<Claimed> {
    let list = eth::deposit_list(&eth.p, eth.vault, eth.from_block, eth.chunk).await?;
    let history = lez::history_of(ledger, ctx).await?;
    let refunded = eth::refunded_leaves(&eth.p, eth.vault, eth.from_block, eth.chunk).await?;
    let rows = scan::deposits(seed, ctx, &list, o.gap, &history, &refunded)?;
    let hold = |leaf: u64, n: &Hash| {
        let hours = seed.claim_delay_hours(n);
        let from = claimable_from(list.blocks[leaf as usize], hours);
        (hours, from, list.block < from)
    };
    let row = match o.leaf {
        Some(l) => rows
            .iter()
            .find(|r| r.owned.deposit.leaf_index == l)
            .ok_or_else(|| anyhow!("leaf {l} is not a deposit of this seed (`ferrata scan` lists them)"))?,
        None => {
            let claimable: Vec<_> = rows.iter().filter(|r| matches!(r.state, DepositState::Claimable { .. })).collect();
            let ready =
                claimable.iter().find(|r| o.no_delay || !hold(r.owned.deposit.leaf_index, &r.owned.nullifier).2);
            match ready.or(claimable.first()) {
                Some(r) => *r,
                None => rows.iter().find(|r| r.state == DepositState::NotImported).ok_or_else(|| {
                    anyhow!("nothing to claim: every deposit of this seed is claimed or refunded (`ferrata recover` lists them)")
                })?,
            }
        }
    };
    match row.state {
        DepositState::Claimed => {
            bail!(codes::program_refusal(250, "checked before sending: the zone's history shows it claimed"))
        }
        DepositState::Refunded => bail!("this deposit was refunded on Ethereum: there is nothing to claim"),
        _ => {}
    }
    let d = &row.owned.deposit;
    let (hours, from_block, held) = hold(d.leaf_index, &row.owned.nullifier);
    let private = !o.plaintext;
    let prover = seed.private_account_ids(1)?.first().copied();
    let leaves: Vec<Hash> = list
        .deposits
        .iter()
        .map(|d| inbound::deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment))
        .collect();
    let plan = lez::plan_claim(ledger, ctx, &leaves, &row.owned, &history, private, o.relayer, prover).await?;
    let (decimals, symbol) = eth::token_meta(&eth.p, Address::from(d.token)).await?;
    let show = |x: u128| units::show(x, decimals, &symbol);
    let amount = units::word_to_u128(&d.amount)?;
    out.line(format!("claim on LEZ (configuration {}):", hx(&ctx.cid)));
    out.line(format!(
        "  deposit      {} to your {} account /{}, under root(cid, {})",
        show(amount),
        kind_name(row.owned.kind),
        row.owned.account_index,
        plan.leaf_count
    ));
    out.line(format!(
        "  would send   {} to the bridge program {}",
        if private {
            "ClaimPrivately, proven locally by LEZ's wallet (the witness stays in the proof)"
        } else {
            "Claim in a public transaction (the witness in the clear)"
        },
        hx(ctx.program.value())
    ));
    out.line(format!("  nullifier N  {}", hx(&plan.nullifier)));
    out.line(format!(
        "  cost         nothing more in {symbol}: the mint fee {} and the relay fee {} were paid with the deposit; the LEZ fee is paid {}",
        show(plan.mint_fee),
        show(plan.relay_fee),
        if private { "inside the proof, by the private account that proves" } else { "by the payer account, in public" }
    ));
    out.line(format!(
        "  delay        {hours} h after the deposit: from Ethereum block {from_block} (now {}){}",
        list.block,
        if held { ", not reached" } else { ", passed" }
    ));
    let leaks = privacy::claim_leaks(&privacy::ClaimChoice {
        private_claim: private,
        dest_private: Some(row.owned.kind == DestKind::Private),
        held,
        skip_delay: o.no_delay,
        fee_hidden: private,
    });
    out.extend(privacy::render(if private { &privacy::CLAIM } else { &privacy::CLAIM_PLAINTEXT }, &leaks));
    out.line(format!("public: {}", lez::reveals(&plan.call)));
    if held && !o.no_delay {
        bail!(
            "held by your claim delay: this deposit can be claimed from Ethereum block {from_block}, about {} min from now (a claim right after its deposit narrows which deposit it could be); wait, or add --no-delay to claim now, less privately",
            (from_block - list.block) * SLOT_SECONDS / 60
        );
    }
    if o.plaintext && !o.accept_plaintext {
        bail!("refusing the plaintext path without --i-accept-plaintext-links-deposit: it links this mint to its deposit for everyone; claim privately (the default) instead");
    }
    let sent = if confirm(out, o.send, o.yes)? {
        let h = ledger.submit(&plan.call).await?;
        out.line(format!("sent: {h}"));
        Some(h)
    } else {
        None
    };
    Ok(Claimed { plan, leaf: d.leaf_index, sent })
}

// ---- burn ----------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct BurnOpts {
    /// Burn from public account /a instead of private account /a.
    pub public: bool,
    pub accept_public: bool,
    pub account: u32,
    pub token: Address,
    pub amount: String,
    pub recipient: Address,
    pub send: bool,
    pub yes: bool,
}

pub struct Burned {
    pub plan: lez::BurnPlan,
    pub sent: Option<String>,
}

/// The holding of private account `id` under `ctx`, as the wallet sees it.
pub async fn private_holding<L: Ledger>(
    ledger: &mut L,
    ctx: &Context,
    id: AccountId,
    token: &[u8; 20],
) -> Result<u128> {
    Ok(ledger
        .private_account(id)
        .await?
        .map(|a| lez::holding_in(a.data.actor_state(ctx.program), ctx, token))
        .unwrap_or(0))
}

/// `ferrata burn`: from a private account by default.
pub async fn burn<L: Ledger>(
    eth: &Eth,
    seed: &Seed,
    ledger: &mut L,
    ctx: &Context,
    o: &BurnOpts,
    out: &mut Out,
) -> Result<Burned> {
    let (decimals, symbol) = eth::token_meta(&eth.p, o.token).await?;
    let show = |x: u128| units::show(x, decimals, &symbol);
    let amount = units::parse(&o.amount, decimals).with_context(|| format!("--amount {}", o.amount))?;
    ensure!(o.recipient != Address::ZERO, vault_refusal("ZeroRecipient", "checked before sending"));
    let token = o.token.into_array();
    let (from, label) = if o.public {
        let a =
            seed.public_accounts(o.account + 1)?.pop().ok_or_else(|| anyhow!("no public account /{}", o.account))?;
        (BurnFrom::Public(a.id), format!("public account /{} ({})", o.account, hx(a.id.value())))
    } else {
        let id = seed
            .private_account_ids(o.account + 1)?
            .pop()
            .ok_or_else(|| anyhow!("no private account /{}", o.account))?;
        let held = private_holding(ledger, ctx, id, &token).await?;
        (BurnFrom::Private { account: id, held }, format!("private account /{}", o.account))
    };
    let history = lez::history_of(ledger, ctx).await?;
    let plan =
        lez::plan_burn(ledger, ctx, &seed.bridge_keys(), from, token, amount, o.recipient.into_array(), &history)
            .await?;
    let (_, burn_fee, checkpoint_fee, relay_fee) = release_fees(eth, o.token, amount).await?;
    let list = eth::deposit_list(&eth.p, eth.vault, eth.from_block, eth.chunk).await?;
    let fresh = !list.deposits.iter().any(|d| d.depositor == o.recipient.into_array());
    out.line(format!("burn on LEZ (configuration {}):", hx(&ctx.cid)));
    out.line(format!("  amount       {} from your {label}", show(amount)));
    out.line(format!("  recipient    {} on Ethereum (bound into COut, encrypted in the memo)", o.recipient));
    out.line(format!(
        "  would send   Burn to the bridge program {}, {}; note r {}  COut {}  burn counter {}",
        hx(ctx.program.value()),
        if o.public { "signed by the holder" } else { "proven locally by LEZ's wallet" },
        hx(&plan.note.r),
        hx(&plan.note.c_out),
        plan.i
    ));
    let deducted = burn_fee + checkpoint_fee;
    out.line(format!(
        "  cost         at release, the burn fee {} and the checkpoint fee {} in force now are deducted: the recipient receives {} through a relayer (relay fee {}), {} if you send the release yourself; the LEZ fee is paid {}",
        show(burn_fee),
        show(checkpoint_fee),
        show(amount.saturating_sub(deducted + relay_fee)),
        show(relay_fee),
        show(amount.saturating_sub(deducted)),
        if o.public { "by the holder, in public" } else { "inside the proof" }
    ));
    out.extend(privacy::render(&privacy::BURN, &privacy::burn_leaks(!o.public, fresh, true)));
    out.line(format!("public: {}", lez::reveals(&plan.call)));
    if o.public && !o.accept_public {
        bail!("refusing to burn from a public account without --i-accept-public-account: the holder, its balance and the burn are public on LEZ; burn from a private account (the default) instead");
    }
    let sent = if confirm(out, o.send, o.yes)? {
        let h = ledger.submit(&plan.call).await?;
        out.line(format!("sent: {h}"));
        Some(h)
    } else {
        None
    };
    Ok(Burned { plan, sent })
}

// ---- release -------------------------------------------------------------------------------------

/// How a release reaches the vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Via {
    /// A relayer submits it and is paid the registered relay fee; `None`: any relayer (the journal
    /// names address 0, and the vault pays whoever sends it).
    Relayer(Option<Address>),
    /// The user sends it from a keystore; no relay fee.
    Myself,
}

pub struct ReleaseOpts {
    /// Burn counter i (default: the oldest unreleased burn).
    pub burn: Option<u64>,
    pub via: Via,
    /// The Groth16 seal (selector ‖ seal) for the vault call.
    pub seal: Option<Bytes>,
    /// Write the relayer's job (the journal and the seal) here.
    pub job_out: Option<PathBuf>,
    pub input_out: Option<PathBuf>,
    pub program: Option<PathBuf>,
    pub prove: bool,
    pub send: bool,
    pub yes: bool,
    pub confirmations: u64,
}

pub struct Released {
    pub plan: release::ReleasePlan,
    pub root: eth::AcceptedRoot,
    pub input: Bytes,
    pub job: Option<PathBuf>,
    pub sent: Option<B256>,
}

/// The wallet's release rule: among the accepted burn roots that cover `burn_index`, the newest whose
/// release delay has passed at `now` (the vault opens a root at `finalizedAt + delay`). A burn no
/// root covers is refused with `BurnRootNotAccepted`; one whose covering roots are all still in their
/// delay with `ReleaseDelayNotElapsed`, naming when the first opens.
pub async fn pick_root(eth: &Eth, token: Address, burn_index: u64) -> Result<(eth::AcceptedRoot, u64)> {
    let roots = eth::accepted_roots(&eth.p, eth.vault, eth.from_block, eth.chunk).await?;
    let now = eth::now(&eth.p).await?;
    let mut first_open: Option<u64> = None;
    for r in roots.iter().rev().filter(|r| r.count > burn_index) {
        let delay = eth::view(
            &eth.p,
            eth.vault,
            IVault::releaseDelayForCall { token, checkpointIndex: U256::from(r.index) },
            None,
        )
        .await?;
        let opens = r.finalized_at.saturating_add(u64::try_from(delay).unwrap_or(u64::MAX));
        if now > opens {
            return Ok((*r, delay.try_into().unwrap_or(u64::MAX)));
        }
        first_open = Some(first_open.map_or(opens, |o: u64| o.min(opens)));
    }
    match first_open {
        Some(t) => Err(vault_refusal(
            "ReleaseDelayNotElapsed",
            &format!(
                "checked before sending: the roots that cover this burn are in their release delay; the first opens at Ethereum time {t}, in {} s",
                t.saturating_sub(now) + 1
            ),
        )),
        None => Err(vault_refusal(
            "BurnRootNotAccepted",
            "checked before sending: no finalized checkpoint covers this burn yet; wait for the next checkpoint",
        )),
    }
}

/// `ferrata release`.
#[allow(clippy::too_many_arguments)]
pub async fn release_burn<L: Ledger>(
    eth: &Eth,
    seed: &Seed,
    ledger: &mut L,
    ctx: &Context,
    signer: Option<&Signer>,
    o: &ReleaseOpts,
    out: &mut Out,
) -> Result<Released> {
    let history = lez::history_of(ledger, ctx).await?;
    let released = eth::released_nullifiers(&eth.p, eth.vault, eth.from_block, eth.chunk).await?;
    let (rows, _) = scan::burns(seed, ctx, &history, &released);
    let row = match o.burn {
        Some(i) => rows
            .iter()
            .find(|r| r.owned.i == i)
            .ok_or_else(|| anyhow!("no burn with counter {i} of this seed (`ferrata recover` lists them)"))?,
        None => rows
            .iter()
            .find(|r| !r.released)
            .ok_or_else(|| anyhow!("nothing to release: every burn of this seed is released"))?,
    };
    if row.released {
        return Err(vault_refusal(
            "NullifierAlreadyUsed",
            "checked before sending: the vault's Release events show it released",
        ));
    }
    let token = Address::from(row.owned.burn.note.token);
    let amount = units::word_to_u128(&row.owned.burn.note.amount)?;
    let now = eth::now(&eth.p).await?;
    for (until, name) in [
        (eth::view(&eth.p, eth.vault, IVault::releasesFrozenUntilCall {}, None).await?, "ReleasesAreFrozen"),
        (
            eth::view(&eth.p, eth.vault, IVault::tokenReleasesFrozenUntilCall { token }, None).await?,
            "TokenReleasesAreFrozen",
        ),
    ] {
        if until > U256::from(now) {
            return Err(vault_refusal(name, &format!("checked before sending: frozen until Ethereum time {until}")));
        }
    }
    let (root, delay) = pick_root(eth, token, row.owned.burn.burn_index).await?;
    let (k, burn_fee, checkpoint_fee, registered_relay_fee) = release_fees(eth, token, amount).await?;
    let (relayer, fee) = match &o.via {
        Via::Relayer(r) => (r.unwrap_or(Address::ZERO), registered_relay_fee),
        Via::Myself => (Address::ZERO, 0),
    };
    let plan = release::plan(ctx, &history, &row.owned, root.root.0, root.count, k, relayer.into_array(), fee)?;
    let (decimals, symbol) = eth::token_meta(&eth.p, token).await?;
    let show = |x: u128| units::show(x, decimals, &symbol);
    let recipient = Address::from(row.owned.recipient);
    out.line(format!("release on Ethereum (vault {}, chain {}):", eth.vault, eth.chain_id));
    out.line(format!("  burn         {} to {recipient} (your burn counter {})", show(amount), row.owned.i));
    out.line(format!(
        "  burn root    {} of checkpoint {} ({} burns): the newest accepted root whose release delay ({delay} s) has passed",
        root.root, root.index, root.count
    ));
    out.line(format!("  NOut         {}", hx(&plan.nullifier)));
    out.line(format!("  journal      sha256 {}", hx(&ferrata_bridge_core::sha256(&plan.journal))));
    let deducted = burn_fee + checkpoint_fee;
    out.line(format!(
        "  cost         the burn fee {} and the checkpoint fee {} are deducted{}: the recipient receives {}",
        show(burn_fee),
        show(checkpoint_fee),
        if fee > 0 { format!(", the relay fee {} goes to the relayer", show(fee)) } else { String::new() },
        show(amount.saturating_sub(deducted + fee))
    ));
    if let Some(path) = &o.input_out {
        std::fs::write(path, plan.input.encode()).with_context(|| format!("writing {}", path.display()))?;
        out.line(format!("guest input written to {} (for a remote prover)", path.display()));
    }
    let mut succinct: Option<Vec<u8>> = None;
    if o.prove {
        let program =
            o.program.as_ref().ok_or_else(|| anyhow!("--prove needs --program (the release guest's .bin)"))?;
        let proven = release::prove(program, &plan)?;
        out.line(format!(
            "release guest {}: journal equal to the native one{}",
            hx(&proven.image_id),
            if proven.dev_mode {
                " (RISC0_DEV_MODE: executed, receipt fake)"
            } else {
                "; succinct receipt verified (a relayer wraps it to Groth16 for the vault)"
            }
        ));
        if !proven.dev_mode {
            succinct = Some(proven.receipt);
        }
    }
    let input = release::calldata(&plan, o.seal.clone().unwrap_or_default());
    let fresh = {
        let list = eth::deposit_list(&eth.p, eth.vault, eth.from_block, eth.chunk).await?;
        !list.deposits.iter().any(|d| d.depositor == row.owned.recipient)
    };
    match &o.via {
        Via::Relayer(r) => out.line(format!(
            "  would send   release(...) to the vault {}, submitted by {} (it pays the gas and is paid the relay fee); nothing is sent from your accounts",
            eth.vault,
            r.map_or("any relayer".to_string(), |a| format!("the relayer {a}"))
        )),
        Via::Myself => {
            let from = signer.map(|s| s.address);
            out.line(format!(
                "  would send   release(...) to the vault {} from {}",
                eth.vault,
                from.map_or("your keystore's address".to_string(), |a| a.to_string())
            ));
            match (from, &o.seal) {
                (Some(from), Some(_)) => match eth::preflight(&eth.p, from, eth.vault, input.clone(), U256::ZERO).await {
                    Ok(f) => out.line(format!(
                        "  network fee  about {} ({} gas at {} wei, eth_estimateGas)",
                        units::show(f.fee(), 18, "ETH"),
                        f.gas,
                        f.gas_price
                    )),
                    Err(e) => return Err(e),
                },
                _ => out.line("  network fee  paid by the sender (estimated once --seal and --keystore are given)"),
            }
        }
    }
    out.line(format!("call: to {}  data {input}", eth.vault));
    out.extend(privacy::render(&privacy::RELEASE, &privacy::release_leaks(fresh, matches!(o.via, Via::Relayer(_)))));
    let mut job = None;
    if let Via::Relayer(_) = &o.via {
        if let Some(path) = &o.job_out {
            let body = match (&o.seal, &succinct) {
                (Some(seal), _) => serde_json::json!({ "kind": "release", "journal": hx(&plan.journal), "seal": hx(seal) }),
                (None, Some(receipt)) => serde_json::json!({
                    "kind": "release-succinct",
                    "receipt": serde_json::from_slice::<serde_json::Value>(receipt).context("the receipt as JSON")?,
                }),
                (None, None) => bail!("a relayer's job needs the proof: --seal (Groth16), or --prove --program for a succinct receipt the relayer wraps"),
            };
            std::fs::write(path, serde_json::to_vec_pretty(&body)?)
                .with_context(|| format!("writing {}", path.display()))?;
            out.line(format!(
                "relayer job written to {}: the journal and the proof, nothing secret (hand it to a relayer's inbox)",
                path.display()
            ));
            job = Some(path.clone());
        } else {
            out.line("not sent: hand it to a relayer with --job-out FILE (or send it yourself with --self-send)");
        }
        ensure!(
            !o.send,
            "a relayer sends this release: write its job with --job-out, or use --self-send to send it yourself"
        );
        return Ok(Released { plan, root, input, job, sent: None });
    }
    if !confirm(out, o.send, o.yes)? {
        return Ok(Released { plan, root, input, job, sent: None });
    }
    ensure!(o.seal.is_some(), "the vault verifies a Groth16 seal: pass --seal");
    let signer = signer.ok_or_else(|| anyhow!("--send needs --keystore and --password-file"))?;
    let receipt = eth::send(&signer.p, signer.address, eth.vault, input.clone(), U256::ZERO, o.confirmations).await?;
    out.line(format!("sent: {} in block {}", receipt.transaction_hash, receipt.block_number.unwrap_or_default()));
    Ok(Released { plan, root, input, job, sent: Some(receipt.transaction_hash) })
}

// ---- scan and recover ----------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct Recovered {
    pub deposits: usize,
    pub claimed: usize,
    pub refunded: usize,
    /// Unclaimed deposits as (kind, account index).
    pub unclaimed: Vec<(DestKind, u32)>,
    pub burns: usize,
    pub released: usize,
    pub next_burn_counter: u64,
    /// Non-zero holdings: (kind, account index, token, amount).
    pub holdings: Vec<(DestKind, u32, Address, u128)>,
}

/// `ferrata scan` (`zone: None`: Ethereum only) and `ferrata recover` (`burns`: also the burns,
/// whether released, and the holdings): everything from the seed and the endpoints, no local state.
pub async fn recover<L: Ledger>(
    eth: &Eth,
    seed: &Seed,
    gap: u32,
    zone: Option<(&mut L, &Context)>,
    burns: bool,
    out: &mut Out,
) -> Result<Recovered> {
    let list = eth::deposit_list(&eth.p, eth.vault, eth.from_block, eth.chunk).await?;
    out.line(format!(
        "vault {}: {} deposits at block {}, root {} (equal to depositRoot: checked)",
        eth.vault,
        list.deposits.len(),
        list.block,
        list.root
    ));
    let refunded = eth::refunded_leaves(&eth.p, eth.vault, eth.from_block, eth.chunk).await?;
    let cid = eth::view(&eth.p, eth.vault, IVault::lezConfigIdCall {}, None).await?;
    let (ctx, history) = match &zone {
        Some((ledger, ctx)) => ((*ctx).clone(), lez::history_of(&**ledger, ctx).await?),
        None => (
            Context {
                program: AccountId::new([0; 32]),
                cid: cid.0,
                chain_id: eth.chain_id,
                vault: eth.vault.into_array(),
            },
            lez::History::default(),
        ),
    };
    let rows = scan::deposits(seed, &ctx, &list, gap, &history, &refunded)?;
    let mut r = Recovered { deposits: rows.len(), ..Default::default() };
    out.line(format!("deposits of this seed: {}", rows.len()));
    let mut tokens: Vec<Address> = vec![eth::ETH];
    for row in &rows {
        let d = &row.owned.deposit;
        let token = Address::from(d.token);
        if !tokens.contains(&token) {
            tokens.push(token);
        }
        let (decimals, symbol) = eth::token_meta(&eth.p, token).await?;
        let from = claimable_from(list.blocks[d.leaf_index as usize], seed.claim_delay_hours(&row.owned.nullifier));
        let state = match (&row.state, zone.is_some()) {
            (DepositState::Claimed, _) => {
                r.claimed += 1;
                "claimed".to_string()
            }
            (DepositState::Refunded, _) => {
                r.refunded += 1;
                "refunded".to_string()
            }
            (_, false) => "claim state unknown (no --sequencer)".to_string(),
            (DepositState::Claimable { root_leaf_count }, _) if list.block < from => {
                format!("unclaimed, claimable under root({root_leaf_count}) from block {from} (your delay)")
            }
            (DepositState::Claimable { root_leaf_count }, _) => {
                format!("unclaimed, claimable under root({root_leaf_count})")
            }
            (DepositState::NotImported, _) => "unclaimed, not yet imported".to_string(),
        };
        if !matches!(row.state, DepositState::Claimed | DepositState::Refunded) {
            r.unclaimed.push((row.owned.kind, row.owned.account_index));
        }
        out.line(format!(
            "  leaf {:>6}  j {:>4}  {}  to {} /{}  N {}  {state}",
            d.leaf_index,
            d.j,
            units::show(units::word_to_u128(&d.amount)?, decimals, &symbol),
            kind_name(row.owned.kind),
            row.owned.account_index,
            hx(&row.owned.nullifier[..8]),
        ));
    }
    if !burns {
        return Ok(r);
    }
    let Some((ledger, ctx)) = zone else {
        bail!("recover needs the zone too (--sequencer, --lez-wallet, --lez-password-file, --bridge-account)");
    };
    let released = eth::released_nullifiers(&eth.p, eth.vault, eth.from_block, eth.chunk).await?;
    let (burn_rows, next_i) = scan::burns(seed, ctx, &history, &released);
    r.burns = burn_rows.len();
    r.next_burn_counter = next_i;
    out.line(format!("burns of this seed: {} (next burn counter {next_i})", burn_rows.len()));
    for b in &burn_rows {
        let (decimals, symbol) = eth::token_meta(&eth.p, Address::from(b.owned.burn.note.token)).await?;
        if b.released {
            r.released += 1;
        }
        out.line(format!(
            "  i {:>4}  burn {:>6}  {}  to {}  {}",
            b.owned.i,
            b.owned.burn.burn_index,
            units::show(units::word_to_u128(&b.owned.burn.note.amount)?, decimals, &symbol),
            Address::from(b.owned.recipient),
            if b.released { "released" } else { "not released" }
        ));
    }
    // Holdings: public ones from the zone, private ones as the wallet sees them.
    let publics = seed.public_accounts(gap)?;
    let privates = seed.private_account_ids(gap)?;
    for token in &tokens {
        let (decimals, symbol) = eth::token_meta(&eth.p, *token).await?;
        let t = token.into_array();
        for a in &publics {
            let state = ledger.actor_state(ferrata_lez_bridge_wire::holder_actor(ctx.program, a.id)).await?;
            let held = lez::holding_in(&state, ctx, &t);
            if held > 0 {
                r.holdings.push((DestKind::Public, a.index, *token, held));
                out.line(format!("  holding  public /{}  {}", a.index, units::show(held, decimals, &symbol)));
            }
        }
        for (a, id) in privates.iter().enumerate() {
            let held = private_holding(ledger, ctx, *id, &t).await?;
            if held > 0 {
                r.holdings.push((DestKind::Private, a as u32, *token, held));
                out.line(format!("  holding  private /{a}  {}", units::show(held, decimals, &symbol)));
            }
        }
    }
    out.line(format!(
        "recovered: {} unclaimed deposits, {} unreleased burns, {} holdings",
        r.unclaimed.len(),
        r.burns - r.released,
        r.holdings.len()
    ));
    Ok(r)
}
