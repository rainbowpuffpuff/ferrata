//! `ferrata`: keys, deposit, scan, recover, claim, burn, release and status for the Ferrata bridge.
//! See `../README.md` for every command, an example of each and what each makes public.

use std::{collections::HashSet, path::PathBuf};

use alloy::primitives::{Address, Bytes, B256, U256};
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use ferrata_bridge_core::{chain_id_u256, inbound};
use ferrata_cli::{
    account, eth,
    eth::IVault,
    hx,
    lez::{self, Context, Ledger as _, Sequencer},
    release, scan,
    scan::DepositState,
    seed::{Seed, DEFAULT_GAP},
    units,
};

#[derive(Parser)]
#[command(
    name = "ferrata",
    version,
    about = "The Ferrata bridge between Ethereum and a LEZ zone (prototype, unaudited)"
)]
struct Cli {
    #[command(flatten)]
    common: Common,
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct Common {
    /// The seed file: the BIP-39 mnemonic of the LEZ wallet (chmod 600).
    #[arg(long, env = "FERRATA_SEED", global = true)]
    seed: Option<PathBuf>,
    /// Any Ethereum JSON-RPC endpoint (reads are checked against the vault's own roots).
    #[arg(long, env = "FERRATA_ETH_RPC", global = true)]
    eth_rpc: Option<String>,
    /// The vault's address.
    #[arg(long, env = "FERRATA_VAULT", global = true)]
    vault: Option<Address>,
    /// First block of the vault's events (its deployment block).
    #[arg(long, env = "FERRATA_FROM_BLOCK", default_value_t = 0, global = true)]
    from_block: u64,
    /// Blocks per eth_getLogs request.
    #[arg(long, default_value_t = 10_000, global = true)]
    chunk: u64,
    /// The LEZ sequencer's RPC endpoint.
    #[arg(long, env = "FERRATA_SEQUENCER", global = true)]
    sequencer: Option<String>,
    /// The LEZ wallet's home (its config and its storage, encrypted under the password): the LEZ keystore.
    #[arg(long, env = "FERRATA_LEZ_WALLET", global = true)]
    lez_wallet: Option<PathBuf>,
    /// A file holding the LEZ wallet's password.
    #[arg(long, env = "FERRATA_LEZ_PASSWORD_FILE", global = true)]
    lez_password_file: Option<PathBuf>,
    /// The bridge program's account id on LEZ (32-byte hex).
    #[arg(long, env = "FERRATA_BRIDGE_ACCOUNT", global = true)]
    bridge_account: Option<String>,
    /// Accounts of each kind searched for deposit destinations.
    #[arg(long, default_value_t = DEFAULT_GAP, global = true)]
    gap: u32,
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    Private,
    Public,
}

#[derive(Args)]
struct Send {
    /// An Ethereum keystore file (Web3 Secret Storage); never a key on the command line.
    #[arg(long, env = "FERRATA_KEYSTORE")]
    keystore: Option<PathBuf>,
    /// A file holding the keystore's password.
    #[arg(long, env = "FERRATA_PASSWORD_FILE")]
    password_file: Option<PathBuf>,
    /// Sign and send (without it the call is only printed).
    #[arg(long)]
    send: bool,
    /// Confirm sending after reading what becomes public.
    #[arg(long)]
    yes: bool,
    /// Blocks to wait after inclusion.
    #[arg(long, default_value_t = 1)]
    confirmations: u64,
}

#[derive(Subcommand)]
enum Command {
    /// Derive from the seed: LEZ public and private accounts, kAuth, kView (secrets only with --show-secrets).
    Keys {
        /// Accounts of each kind to list.
        #[arg(long, default_value_t = 3)]
        accounts: u32,
        /// Print kAuth and kView themselves (anyone holding kAuth can claim and release this seed's funds).
        #[arg(long)]
        show_secrets: bool,
    },
    /// Build the vault deposit for a destination of the seed; print it, or send it through a keystore.
    Deposit {
        /// Token address (0x000…0 for ETH).
        #[arg(long, default_value_t = Address::ZERO)]
        token: Address,
        /// Index of the denomination in the token's registry entry.
        #[arg(long)]
        denomination: u8,
        /// Destination kind: private (default) or public (visible when claimed).
        #[arg(long, value_enum, default_value_t = Kind::Private)]
        kind: Kind,
        /// Destination account index /a in the LEZ wallet.
        #[arg(long, default_value_t = 0)]
        account: u32,
        /// The depositor's address, to print a deposit without a keystore.
        #[arg(long)]
        from: Option<Address>,
        #[command(flatten)]
        send: Send,
    },
    /// Download the vault's deposit list (checked against depositRoot) and list this seed's deposits.
    Scan,
    /// Recover everything from the seed: deposits and their state, burns and whether released.
    Recover,
    /// Claim a deposit on LEZ: private by default; plaintext only with --plaintext and its confirmation.
    Claim {
        /// Leaf index of the deposit (default: the oldest claimable one).
        #[arg(long)]
        leaf: Option<u64>,
        /// The plaintext path: links this mint to its deposit publicly.
        #[arg(long)]
        plaintext: bool,
        /// Confirms the plaintext path after reading what it makes public.
        #[arg(long)]
        i_accept_plaintext_links_deposit: bool,
        /// The relayer's LEZ account (32-byte hex), paid the registered relay fee.
        #[arg(long)]
        relayer: Option<String>,
        /// The LEZ account in the wallet that pays the fee (public claims).
        #[arg(long)]
        payer: Option<String>,
        /// The bridge program's binary (private claims prove its execution locally).
        #[arg(long)]
        bridge_program: Option<PathBuf>,
        /// Sign and submit to the sequencer (without it the claim is only checked and printed).
        #[arg(long)]
        send: bool,
        /// Confirm submitting after reading what becomes public.
        #[arg(long)]
        yes: bool,
    },
    /// Burn one denomination held by public account /a, releasable to an Ethereum recipient.
    Burn {
        /// The public account /a that holds the wrapped tokens.
        #[arg(long, default_value_t = 0)]
        account: u32,
        /// The token's Ethereum address (0x000…0 for ETH).
        #[arg(long, default_value_t = Address::ZERO)]
        token: Address,
        /// Amount in the token's units (one denomination), e.g. 0.001.
        #[arg(long)]
        amount: String,
        /// The Ethereum recipient (bound into COut, encrypted in the memo).
        #[arg(long)]
        recipient: Address,
        /// The LEZ account in the wallet that pays the fee.
        #[arg(long)]
        payer: Option<String>,
        /// Sign and submit to the sequencer (without it the burn is only checked and printed).
        #[arg(long)]
        send: bool,
        /// Confirm submitting after reading what becomes public.
        #[arg(long)]
        yes: bool,
    },
    /// Prove the release of a burn of this seed and build (or send) the vault call.
    Release {
        /// Burn counter i of the burn (default: the oldest unreleased one).
        #[arg(long)]
        burn: Option<u64>,
        /// The release guest program (.bin); with --prove, r0vm executes or proves it on the input.
        #[arg(long)]
        program: Option<PathBuf>,
        /// Run the release guest (--program) on the input in r0vm: executed under RISC0_DEV_MODE=1, else proven locally.
        #[arg(long)]
        prove: bool,
        /// Write the guest's input (for a remote prover) to this file.
        #[arg(long)]
        input_out: Option<PathBuf>,
        /// The Groth16 seal (hex file) for the vault call.
        #[arg(long)]
        seal: Option<PathBuf>,
        /// Relayer address named in the proof (0: the sender).
        #[arg(long, default_value_t = Address::ZERO)]
        relayer: Address,
        /// Claim the registered relay fee for the relayer.
        #[arg(long)]
        relay_fee: bool,
        #[command(flatten)]
        send: Send,
    },
    /// The vault's registry, fees, caps and freezes, and the zone's configuration and newest root.
    Status {
        /// Tokens to show (repeatable; default ETH).
        #[arg(long)]
        token: Vec<Address>,
    },
}

fn need<T: Clone>(v: &Option<T>, what: &str) -> Result<T> {
    v.clone().ok_or_else(|| anyhow!("{what} is needed for this command"))
}

struct Env {
    seed: Seed,
    eth: alloy::providers::DynProvider,
    vault: Address,
    chain_id: u64,
}

async fn env(c: &Common) -> Result<Env> {
    let seed = Seed::from_file(&need(&c.seed, "--seed")?)?;
    let eth = eth::provider(&need(&c.eth_rpc, "--eth-rpc")?)?;
    let vault = need(&c.vault, "--vault")?;
    let chain_id = alloy::providers::Provider::get_chain_id(&eth).await?;
    Ok(Env { seed, eth, vault, chain_id })
}

async fn zone(
    c: &Common,
    e: &Env,
    payer: Option<&str>,
    bridge_program: Option<PathBuf>,
) -> Result<(Sequencer, Context)> {
    let wallet = lez::open_wallet(
        &need(&c.lez_wallet, "--lez-wallet")?,
        &need(&c.sequencer, "--sequencer")?,
        &need(&c.lez_password_file, "--lez-password-file")?,
        &e.seed,
    )?;
    let program = account(&need(&c.bridge_account, "--bridge-account")?)?;
    let cid = eth::view(&e.eth, e.vault, IVault::lezConfigIdCall {}, None).await?;
    let payer = payer.map(account).transpose()?;
    let ctx = Context { program, cid: cid.0, chain_id: e.chain_id, vault: e.vault.into_array() };
    Ok((Sequencer { wallet, payer, bridge_program }, ctx))
}

fn confirm(send: bool, yes: bool) -> Result<bool> {
    if !send {
        println!("\nnot sent (add --send --yes to sign and send)");
        return Ok(false);
    }
    ensure!(yes, "refusing to send without --yes: read what becomes public above, then add --yes");
    Ok(true)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let c = &cli.common;
    match cli.command {
        Command::Keys { accounts, show_secrets } => {
            let seed = Seed::from_file(&need(&c.seed, "--seed")?)?;
            println!("LEZ public accounts (wallet key tree, /a):");
            for a in seed.public_accounts(accounts)? {
                println!("  /{}  {}", a.index, hx(a.id.value()));
            }
            println!("LEZ private accounts (wallet key tree, /a):");
            for (a, id) in seed.private_account_ids(accounts)?.iter().enumerate() {
                println!("  /{a}  {}", hx(id.value()));
            }
            let (k_auth, k_view) = seed.bridge_key_bytes();
            if show_secrets {
                println!("kAuth  {}  (authorises claims and releases: never share)", hx(&k_auth));
                println!("kView  {}  (discloses burns and recipients, cannot claim or release)", hx(&k_view));
            } else {
                println!(
                    "kAuth  fingerprint {}  (sha256 of the key; --show-secrets prints it)",
                    hx(&ferrata_bridge_core::sha256(&k_auth)[..8])
                );
                println!("kView  fingerprint {}", hx(&ferrata_bridge_core::sha256(&k_view)[..8]));
            }
        }

        Command::Deposit { token, denomination, kind, account: a, from, send } => {
            let e = env(c).await?;
            let signer = match (&send.keystore, &send.password_file) {
                (Some(k), Some(p)) => Some(eth::keystore_signer(k, p)?),
                (None, None) => None,
                _ => bail!("--keystore and --password-file go together"),
            };
            let depositor = match (&signer, from) {
                (Some(s), _) => s.address(),
                (None, Some(f)) => f,
                (None, None) => {
                    bail!("--from (to print) or --keystore and --password-file (to send) name the depositor")
                }
            };
            ensure!(
                a < c.gap,
                "account /{a} is outside the recovery window (--gap {}): a wallet would not find it",
                c.gap
            );
            let price = eth::deposit_price(&e.eth, e.vault, token, denomination).await?;
            let (decimals, symbol) = eth::token_meta(&e.eth, token).await?;
            let j = eth::view(&e.eth, e.vault, IVault::depositCountCall { depositor }, None).await?;
            let keys = e.seed.bridge_keys();
            let dests = e.seed.destinations(c.gap)?;
            let dk = match kind {
                Kind::Private => inbound::DestKind::Private,
                Kind::Public => inbound::DestKind::Public,
            };
            let s = inbound::deposit_secret(
                &keys.auth,
                &chain_id_u256(e.chain_id),
                &e.vault.into_array(),
                &depositor.into_array(),
                j,
            );
            let commitment = inbound::commitment(&s, dk, &dests.of(dk, a));
            let input = eth::deposit_calldata(token, denomination, B256::from(commitment), j);
            println!("deposit to the vault {} on chain {}", e.vault, e.chain_id);
            println!("  denomination  {}", units::show(price.denomination, decimals, &symbol));
            println!(
                "  mint fee      {}  (schedule {})",
                units::show(price.mint_fee, decimals, &symbol),
                price.schedule
            );
            println!("  relay fee     {}", units::show(price.relay_fee, decimals, &symbol));
            println!("  total         {}", units::show(price.total, decimals, &symbol));
            println!("  depositor     {depositor}   j = {j}");
            println!("  commitment C  {}", hx(&commitment));
            println!("public on Ethereum: the depositor, the token, the denomination and fees, the time, C.");
            println!(
                "hidden: the LEZ destination ({} account /{a}); C commits to it.",
                if dk == inbound::DestKind::Private { "private" } else { "public" }
            );
            if dk == inbound::DestKind::Public {
                println!("note: a public destination becomes visible when it is claimed (which deposit paid it stays hidden in a private claim).");
            }
            if token != eth::ETH {
                println!(
                    "an ERC-20 deposit needs an allowance of {} for the vault first",
                    units::show(price.total, decimals, &symbol)
                );
            }
            println!("call: to {}  value {}  data {}", e.vault, if token == eth::ETH { price.total } else { 0 }, input);
            if !confirm(send.send, send.yes)? {
                return Ok(());
            }
            let signer = signer.ok_or_else(|| anyhow!("--send needs --keystore and --password-file"))?;
            let p = eth::signing_provider(&need(&c.eth_rpc, "--eth-rpc")?, signer)?;
            let value = if token == eth::ETH { U256::from(price.total) } else { U256::ZERO };
            let receipt = eth::send(&p, depositor, e.vault, input, value, send.confirmations).await?;
            let leaf = eth::deposit_of_receipt(&receipt).map(|d| d.leafIndex);
            println!(
                "sent: {} in block {} (leaf {leaf:?})",
                receipt.transaction_hash,
                receipt.block_number.unwrap_or_default()
            );
        }

        Command::Scan | Command::Recover => {
            let recover = matches!(cli.command, Command::Recover);
            let e = env(c).await?;
            let list = eth::deposit_list(&e.eth, e.vault, c.from_block, c.chunk).await?;
            println!(
                "vault {}: {} deposits at block {}, root {} (equal to depositRoot: checked)",
                e.vault,
                list.deposits.len(),
                list.block,
                list.root
            );
            let refunded = eth::refunded_leaves(&e.eth, e.vault, c.from_block, c.chunk).await?;
            let (ledger, ctx, history) = if c.sequencer.is_some() {
                let (ledger, ctx) = zone(c, &e, None, None).await?;
                let history = lez::history_of(&ledger, &ctx).await?;
                (Some(ledger), ctx, history)
            } else {
                let cid = eth::view(&e.eth, e.vault, IVault::lezConfigIdCall {}, None).await?;
                let ctx = Context {
                    program: lee::AccountId::new([0; 32]),
                    cid: cid.0,
                    chain_id: e.chain_id,
                    vault: e.vault.into_array(),
                };
                (None, ctx, lez::History::default())
            };
            let rows = scan::deposits(&e.seed, &ctx, &list, c.gap, &history, &refunded)?;
            println!("deposits of this seed: {}", rows.len());
            for r in &rows {
                let d = &r.owned.deposit;
                let (decimals, symbol) = eth::token_meta(&e.eth, Address::from(d.token)).await?;
                let state = match (&r.state, ledger.is_some()) {
                    (DepositState::Claimed, _) => "claimed".to_string(),
                    (DepositState::Refunded, _) => "refunded".to_string(),
                    (_, false) => "claim state unknown (no --sequencer)".to_string(),
                    (DepositState::Claimable { root_leaf_count }, _) => {
                        format!("unclaimed, claimable under root({root_leaf_count})")
                    }
                    (DepositState::NotImported, _) => "unclaimed, not yet imported".to_string(),
                };
                println!(
                    "  leaf {:>6}  j {:>4}  {}  to {} /{}  N {}  {state}",
                    d.leaf_index,
                    d.j,
                    units::show(units::word_to_u128(&d.amount)?, decimals, &symbol),
                    if r.owned.kind == inbound::DestKind::Private { "private" } else { "public" },
                    r.owned.account_index,
                    hx(&r.owned.nullifier[..8]),
                );
            }
            if recover {
                ensure!(
                    ledger.is_some(),
                    "recover needs the zone too (--sequencer, --lez-wallet, --lez-password-file, --bridge-account)"
                );
                let released = eth::released_nullifiers(&e.eth, e.vault, c.from_block, c.chunk).await?;
                let (burns, next_i) = scan::burns(&e.seed, &ctx, &history, &released);
                println!("burns of this seed: {} (next burn counter {next_i})", burns.len());
                for b in &burns {
                    let (decimals, symbol) = eth::token_meta(&e.eth, Address::from(b.owned.burn.note.token)).await?;
                    println!(
                        "  i {:>4}  burn {:>6}  {}  to {}  {}",
                        b.owned.i,
                        b.owned.burn.burn_index,
                        units::show(units::word_to_u128(&b.owned.burn.note.amount)?, decimals, &symbol),
                        Address::from(b.owned.recipient),
                        if b.released { "released" } else { "not released" }
                    );
                }
                let unclaimed =
                    rows.iter().filter(|r| !matches!(r.state, DepositState::Claimed | DepositState::Refunded)).count();
                let unreleased = burns.iter().filter(|b| !b.released).count();
                println!("recovered: {unclaimed} unclaimed deposits, {unreleased} unreleased burns");
            }
        }

        Command::Claim {
            leaf,
            plaintext,
            i_accept_plaintext_links_deposit,
            relayer,
            payer,
            bridge_program,
            send,
            yes,
        } => {
            let e = env(c).await?;
            let (mut ledger, ctx) = zone(c, &e, payer.as_deref(), bridge_program).await?;
            let list = eth::deposit_list(&e.eth, e.vault, c.from_block, c.chunk).await?;
            let history = lez::history_of(&ledger, &ctx).await?;
            let rows = scan::deposits(&e.seed, &ctx, &list, c.gap, &history, &HashSet::new())?;
            let row = match leaf {
                Some(l) => rows
                    .iter()
                    .find(|r| r.owned.deposit.leaf_index == l)
                    .ok_or_else(|| anyhow!("leaf {l} is not a deposit of this seed"))?,
                None => rows
                    .iter()
                    .find(|r| matches!(r.state, DepositState::Claimable { .. }))
                    .ok_or_else(|| anyhow!("no claimable deposit of this seed"))?,
            };
            let leaves: Vec<_> = list
                .deposits
                .iter()
                .map(|d| inbound::deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment))
                .collect();
            let relayer = relayer.as_deref().map(ferrata_cli::h32).transpose()?;
            let plan = lez::plan_claim(&ledger, &ctx, &leaves, &row.owned, &history, !plaintext, relayer).await?;
            println!("{}: N {}", plan.call.what, hx(&plan.nullifier));
            println!("public: {}", lez::reveals(&plan.call));
            if plaintext && !i_accept_plaintext_links_deposit {
                bail!("the plaintext path links this mint to its deposit for everyone; add --i-accept-plaintext-links-deposit to use it, or claim privately");
            }
            if !confirm(send, yes)? {
                return Ok(());
            }
            let hash = ledger.submit(&plan.call).await?;
            println!("sent: {hash}");
        }

        Command::Burn { account: a, token, amount, recipient, payer, send, yes } => {
            let e = env(c).await?;
            let (mut ledger, ctx) = zone(c, &e, payer.as_deref(), None).await?;
            let (decimals, symbol) = eth::token_meta(&e.eth, token).await?;
            let amount = units::parse(&amount, decimals)?;
            let holder = e.seed.public_accounts(a + 1)?.pop().ok_or_else(|| anyhow!("no account /{a}"))?;
            let history = lez::history_of(&ledger, &ctx).await?;
            let plan = lez::plan_burn(
                &ledger,
                &ctx,
                &e.seed.bridge_keys(),
                holder.id,
                token.into_array(),
                amount,
                recipient.into_array(),
                &history,
            )
            .await?;
            println!(
                "burn {} from public account /{a} ({}), releasable to {recipient}",
                units::show(amount, decimals, &symbol),
                hx(holder.id.value())
            );
            println!("note: r {}  COut {}", hx(&plan.note.r), hx(&plan.note.c_out));
            println!("public: {}", lez::reveals(&plan.call));
            if !confirm(send, yes)? {
                return Ok(());
            }
            let hash = ledger.submit(&plan.call).await?;
            println!("sent: {hash}");
        }

        Command::Release { burn, program, prove, input_out, seal, relayer, relay_fee, send } => {
            let e = env(c).await?;
            let (ledger, ctx) = zone(c, &e, None, None).await?;
            let history = lez::history_of(&ledger, &ctx).await?;
            let released = eth::released_nullifiers(&e.eth, e.vault, c.from_block, c.chunk).await?;
            let (rows, _) = scan::burns(&e.seed, &ctx, &history, &released);
            let row = match burn {
                Some(i) => rows
                    .iter()
                    .find(|r| r.owned.i == i)
                    .ok_or_else(|| anyhow!("no burn with counter {i} of this seed"))?,
                None => rows.iter().find(|r| !r.released).ok_or_else(|| anyhow!("no unreleased burn of this seed"))?,
            };
            if row.released {
                bail!(ferrata_cli::codes::vault_refusal(
                    &ferrata_cli::codes::selector("NullifierAlreadyUsed"),
                    "checked before sending"
                )
                .expect("known"));
            }
            let roots = eth::accepted_burn_roots(&e.eth, e.vault, c.from_block, c.chunk).await?;
            let (root, count) =
                roots.iter().rev().find(|(_, n)| *n > row.owned.burn.burn_index).copied().ok_or_else(|| {
                    anyhow!(ferrata_cli::codes::vault_refusal(
                        &ferrata_cli::codes::selector("BurnRootNotAccepted"),
                        "checked before sending: no finalized checkpoint covers this burn yet"
                    )
                    .expect("known"))
                })?;
            let token = Address::from(row.owned.burn.note.token);
            let cfg = eth::view(&e.eth, e.vault, IVault::tokenConfigCall { token }, None).await?;
            let amount = U256::from_be_bytes(row.owned.burn.note.amount);
            let denom_index = cfg
                .denominations_
                .iter()
                .position(|d| *d == amount)
                .ok_or_else(|| anyhow!("the burned amount is not a denomination on the vault"))?
                as u8;
            let fee = if relay_fee {
                let state = eth::view(&e.eth, e.vault, IVault::feeStateCall { token }, None).await?;
                let f = eth::view(
                    &e.eth,
                    e.vault,
                    IVault::feesCall { token, index: U256::from(state.current), denomIndex: denom_index },
                    None,
                )
                .await?;
                u128::try_from(f.relayFee)?
            } else {
                0
            };
            let plan =
                release::plan(&ctx, &history, &row.owned, root.0, count, denom_index, relayer.into_array(), fee)?;
            let (decimals, symbol) = eth::token_meta(&e.eth, token).await?;
            println!(
                "release of burn {} (i {}) under burn root {root} ({count} burns)",
                row.owned.burn.burn_index, row.owned.i
            );
            println!("  amount     {}", units::show(u128::try_from(amount)?, decimals, &symbol));
            println!("  recipient  {}", Address::from(row.owned.recipient));
            println!("  NOut       {}", hx(&plan.nullifier));
            println!("  journal    sha256 {}", hx(&ferrata_bridge_core::sha256(&plan.journal)));
            println!("public on Ethereum: the recipient, the token, the denomination, the time, NOut, the burn root, the relayer; not which burn.");
            if let Some(path) = &input_out {
                std::fs::write(path, plan.input.encode()).with_context(|| format!("writing {}", path.display()))?;
                println!("guest input written to {} (for a remote prover)", path.display());
            }
            if prove {
                let program = program.ok_or_else(|| anyhow!("--prove needs --program (the release guest's .bin)"))?;
                let proven = release::prove(&program, &plan)?;
                println!(
                    "release guest {}: journal equal to the native one{}",
                    hx(&proven.image_id),
                    if proven.dev_mode {
                        " (RISC0_DEV_MODE: executed, receipt fake)"
                    } else {
                        "; succinct receipt verified (wrap it to Groth16 for the vault)"
                    }
                );
            }
            let seal = match &seal {
                Some(path) => Bytes::from(hex::decode(std::fs::read_to_string(path)?.trim().trim_start_matches("0x"))?),
                None => Bytes::new(),
            };
            let input = release::calldata(&plan, seal.clone());
            println!("call: to {}  data {input}", e.vault);
            if !confirm(send.send, send.yes)? {
                return Ok(());
            }
            ensure!(!seal.is_empty(), "the vault verifies a Groth16 seal: pass --seal");
            let signer = eth::keystore_signer(
                &need(&send.keystore, "--keystore")?,
                &need(&send.password_file, "--password-file")?,
            )?;
            let from = signer.address();
            let p = eth::signing_provider(&need(&c.eth_rpc, "--eth-rpc")?, signer)?;
            let receipt = eth::send(&p, from, e.vault, input, U256::ZERO, send.confirmations).await?;
            println!("sent: {} in block {}", receipt.transaction_hash, receipt.block_number.unwrap_or_default());
        }

        Command::Status { token } => {
            let eth_p = eth::provider(&need(&c.eth_rpc, "--eth-rpc")?)?;
            let vault = need(&c.vault, "--vault")?;
            let leaves = eth::view(&eth_p, vault, IVault::nextLeafIndexCall {}, None).await?;
            let root = eth::view(&eth_p, vault, IVault::depositRootCall {}, None).await?;
            let cid = eth::view(&eth_p, vault, IVault::lezConfigIdCall {}, None).await?;
            let retired = eth::view(&eth_p, vault, IVault::retiredCall {}, None).await?;
            let dep_frozen = eth::view(&eth_p, vault, IVault::depositsFrozenUntilCall {}, None).await?;
            let rel_frozen = eth::view(&eth_p, vault, IVault::releasesFrozenUntilCall {}, None).await?;
            let checkpoints = eth::view(&eth_p, vault, IVault::checkpointCountCall {}, None).await?;
            let imported = eth::view(&eth_p, vault, IVault::finalImportedLeafCountCall {}, None).await?;
            println!("vault {vault}: {leaves} deposits, root {root}, configId {cid}");
            println!("  retired {retired}; deposits frozen until {dep_frozen}; releases frozen until {rel_frozen}");
            println!("  checkpoints {checkpoints}; final imported leaf count {imported}");
            for (dir, name) in [(0u8, "deposit"), (1u8, "release")] {
                let g = eth::view(&eth_p, vault, IVault::globalCapCall { direction: dir }, None).await?;
                let show = |x: U256| if x == U256::MAX { "unlimited".to_string() } else { format!("{x} value units") };
                println!(
                    "  global {name} cap: capacity {}, refill {}/s, available {}",
                    show(g.capacity),
                    show(g.refillPerSecond),
                    show(g.available)
                );
            }
            let mut tokens = token;
            if tokens.is_empty() {
                tokens.push(eth::ETH);
            }
            for t in tokens {
                let cfg = eth::view(&eth_p, vault, IVault::tokenConfigCall { token: t }, None).await?;
                let (decimals, symbol) = eth::token_meta(&eth_p, t).await?;
                println!(
                    "token {t} ({symbol}): registered {}, deposits enabled {}, lezUnit {}",
                    cfg.registered, cfg.depositsEnabled, cfg.lezUnit
                );
                let state = eth::view(&eth_p, vault, IVault::feeStateCall { token: t }, None).await?;
                for (k, d) in cfg.denominations_.iter().enumerate() {
                    let f = eth::view(
                        &eth_p,
                        vault,
                        IVault::feesCall { token: t, index: U256::from(state.current), denomIndex: k as u8 },
                        None,
                    )
                    .await?;
                    let show = |x: U256| {
                        u128::try_from(x).map(|x| units::show(x, decimals, &symbol)).unwrap_or_else(|_| x.to_string())
                    };
                    println!(
                        "  [{k}] {}  mint fee {}  relay fee {}  burn fee {}  checkpoint fee {}",
                        show(*d),
                        show(f.mintFee),
                        show(f.relayFee),
                        show(f.burnFee),
                        show(f.checkpointFee)
                    );
                }
                for (dir, name) in [(0u8, "deposit"), (1u8, "release")] {
                    let cap = eth::view(&eth_p, vault, IVault::tokenCapCall { token: t, direction: dir }, None).await?;
                    let show = |x: U256| match u128::try_from(x) {
                        _ if x == U256::MAX => "unlimited".to_string(),
                        Ok(x) => units::show(x, decimals, &symbol),
                        Err(_) => x.to_string(),
                    };
                    println!(
                        "  {name} cap: capacity {}, refill {}/s, available {}",
                        show(cap.capacity),
                        show(cap.refillPerSecond),
                        show(cap.available)
                    );
                }
            }
            if c.sequencer.is_some() {
                let e = env(c).await?;
                let (ledger, ctx) = zone(c, &e, None, None).await?;
                let history = lez::history_of(&ledger, &ctx).await?;
                for line in lez::zone_status(&ledger, &ctx, &history).await? {
                    println!("{line}");
                }
            }
        }
    }
    Ok(())
}
