//! `ferrata`: keys, deposit, scan, recover, claim, burn, release and status for the Ferrata bridge.
//! See `../README.md` for every command, an example of each and what each makes public. The
//! commands' logic is in `ferrata_cli::flow`, which the end-to-end test drives too.

use std::path::PathBuf;

use alloy::primitives::{Address, Bytes, U256};
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use ferrata_cli::{
    account, eth,
    eth::IVault,
    flow, hx,
    lez::{self, Context, Ledger as _, Sequencer},
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
    /// Refuse to run unless the Ethereum RPC serves this chain id (1 mainnet, 11155111 Sepolia).
    #[arg(long, env = "FERRATA_CHAIN_ID", global = true)]
    chain_id: Option<u64>,
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
    /// Print one JSON object instead of the report: {"command", "report" (its lines), "result"}; on an
    /// error, {"error"}. For scripts and integrations.
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
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
    /// Sign and send (without it the command is a dry run: it prints what it would send, what becomes public and the cost).
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
    /// Deposit one denomination to a LEZ account of the seed (private by default); a dry run without --send.
    Deposit {
        /// Token address (0x000…0 for ETH).
        #[arg(long, default_value_t = Address::ZERO)]
        token: Address,
        /// Index of the denomination in the token's registry entry (`ferrata status` lists them).
        #[arg(long)]
        denomination: u8,
        /// Destination kind: private (default) or public (its account and balance become public when claimed).
        #[arg(long, value_enum, default_value_t = Kind::Private)]
        kind: Kind,
        /// Confirms a public destination after reading what it makes public.
        #[arg(long)]
        i_accept_public_account: bool,
        /// Destination account index /a in the LEZ wallet.
        #[arg(long, default_value_t = 0)]
        account: u32,
        /// The depositor's address, to print a deposit without a keystore.
        #[arg(long)]
        from: Option<Address>,
        /// Send the ERC-20 approve the deposit needs first (ETH needs none).
        #[arg(long)]
        approve: bool,
        #[command(flatten)]
        send: Send,
    },
    /// Download the vault's deposit list (checked against depositRoot) and list this seed's deposits.
    Scan,
    /// Recover everything from the seed alone: deposits and their state, burns and whether released, holdings.
    Recover,
    /// Claim a deposit on LEZ: private and after the deposit's claim delay by default.
    Claim {
        /// Leaf index of the deposit (default: the oldest claimable one whose delay has passed).
        #[arg(long)]
        leaf: Option<u64>,
        /// Claim before the deposit's claim delay ends (less private: the claim's time points at the deposit).
        #[arg(long)]
        no_delay: bool,
        /// The plaintext path: links this mint to its deposit publicly.
        #[arg(long)]
        plaintext: bool,
        /// Confirms the plaintext path after reading what it makes public.
        #[arg(long)]
        i_accept_plaintext_links_deposit: bool,
        /// A LEZ relayer account (32-byte hex) paid the registered relay fee in wrapped tokens.
        #[arg(long)]
        relayer: Option<String>,
        /// The LEZ account in the wallet that pays the fee (plaintext claims; a private claim pays inside its proof).
        #[arg(long)]
        payer: Option<String>,
        /// The bridge program's binary (a private claim proves its execution locally).
        #[arg(long, env = "FERRATA_BRIDGE_PROGRAM")]
        bridge_program: Option<PathBuf>,
        /// The relayer's inbox (a directory): a private claim's LEZ fee is then paid from that relayer's gas
        /// drop, a credit sealed to a fresh one-time private account (on by default whenever it is set).
        #[arg(long, env = "FERRATA_GAS_DROP_INBOX")]
        gas_drop_inbox: Option<PathBuf>,
        /// The gas-drop guest (ELF) the receipt is proven with.
        #[arg(long, env = "FERRATA_GASDROP_ELF")]
        gas_drop_elf: Option<PathBuf>,
        /// Seconds to wait for the relayer's credit.
        #[arg(long, default_value_t = 600)]
        gas_drop_wait: u64,
        /// Pay a private claim's LEZ fee from your own private account even when a relayer's inbox is set.
        #[arg(long)]
        no_gas_drop: bool,
        /// Sign and submit to the sequencer (without it the command is a dry run).
        #[arg(long)]
        send: bool,
        /// Confirm submitting after reading what becomes public.
        #[arg(long)]
        yes: bool,
    },
    /// Burn one denomination on LEZ, releasable to an Ethereum recipient: from a private account by default.
    Burn {
        /// Burn from account /a of this kind: private (default) or public (the holder and the burn become public).
        #[arg(long, value_enum, default_value_t = Kind::Private)]
        from: Kind,
        /// Confirms burning from a public account after reading what it makes public.
        #[arg(long)]
        i_accept_public_account: bool,
        /// The account /a that holds the wrapped tokens.
        #[arg(long, default_value_t = 0)]
        account: u32,
        /// The token's Ethereum address (0x000…0 for ETH).
        #[arg(long, default_value_t = Address::ZERO)]
        token: Address,
        /// Amount in the token's units (one denomination), e.g. 0.1.
        #[arg(long)]
        amount: String,
        /// The Ethereum recipient (bound into COut, encrypted in the memo); a fresh address is best.
        #[arg(long)]
        recipient: Address,
        /// The LEZ account in the wallet that pays the fee (public burns).
        #[arg(long)]
        payer: Option<String>,
        /// The bridge program's binary (a private burn proves its execution locally).
        #[arg(long, env = "FERRATA_BRIDGE_PROGRAM")]
        bridge_program: Option<PathBuf>,
        /// Sign and submit to the sequencer (without it the command is a dry run).
        #[arg(long)]
        send: bool,
        /// Confirm submitting after reading what becomes public.
        #[arg(long)]
        yes: bool,
    },
    /// Release a burn of this seed on Ethereum, against the newest accepted root whose release delay has passed.
    Release {
        /// Burn counter i of the burn (default: the oldest unreleased one).
        #[arg(long)]
        burn: Option<u64>,
        /// Send the release yourself from --keystore (no relay fee; your gas payer is linked to the recipient).
        #[arg(long)]
        self_send: bool,
        /// The relayer named in the proof (default: any relayer, paid the registered relay fee by the vault).
        #[arg(long)]
        relayer: Option<Address>,
        /// Write the relayer's job (journal and proof, nothing secret) to this file, for its inbox.
        #[arg(long)]
        job_out: Option<PathBuf>,
        /// The release guest program (.bin); with --prove, r0vm executes or proves it on the input.
        #[arg(long)]
        program: Option<PathBuf>,
        /// Run the release guest (--program) in r0vm: executed under RISC0_DEV_MODE=1, else proven locally (succinct).
        #[arg(long)]
        prove: bool,
        /// Write the guest's input (for a remote prover) to this file.
        #[arg(long)]
        input_out: Option<PathBuf>,
        /// The Groth16 seal (hex file) for the vault call.
        #[arg(long)]
        seal: Option<PathBuf>,
        #[command(flatten)]
        send: Send,
    },
    /// The attestation side against a running LEZ sequencer: an instance's configuration and state, `init`,
    /// `advance` (a consensus-state update), `attest` (a storage slot, an account or a log under a record), each a dry
    /// run with the program's codes unless --send --yes (`src/attest.rs`, `lez/ATTEST-CLI.md`).
    #[cfg(feature = "lez-wallet")]
    Attest(ferrata_cli::attest::AttestArgs),
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

async fn eth_side(c: &Common) -> Result<flow::Eth> {
    let url = need(&c.eth_rpc, "--eth-rpc (or FERRATA_ETH_RPC: any Ethereum JSON-RPC endpoint)")?;
    let p = eth::provider(&url)?;
    let vault = need(&c.vault, "--vault (or FERRATA_VAULT: the vault's address)")?;
    let chain_id = alloy::providers::Provider::get_chain_id(&p)
        .await
        .with_context(|| format!("the Ethereum RPC {url} does not answer"))?;
    if let Some(want) = c.chain_id {
        ensure!(
            chain_id == want,
            "the Ethereum RPC {url} serves chain {chain_id}, not {want} (--chain-id): refusing to go on"
        );
    }
    ensure!(
        !alloy::providers::Provider::get_code_at(&p, vault).await?.is_empty(),
        "no contract at {vault} on chain {chain_id}: check --vault and --eth-rpc"
    );
    Ok(flow::Eth { p, vault, chain_id, from_block: c.from_block, chunk: c.chunk })
}

fn seed_of(c: &Common) -> Result<Seed> {
    Seed::from_file(&need(&c.seed, "--seed (or FERRATA_SEED: the seed file)")?)
}

async fn zone(
    c: &Common,
    e: &flow::Eth,
    seed: &Seed,
    payer: Option<&str>,
    bridge_program: Option<PathBuf>,
) -> Result<(Sequencer, Context)> {
    let rpc = need(&c.sequencer, "--sequencer (or FERRATA_SEQUENCER: the LEZ sequencer's RPC endpoint)")?;
    let wallet = lez::open_wallet(
        &need(&c.lez_wallet, "--lez-wallet (or FERRATA_LEZ_WALLET: the LEZ wallet's home)")?,
        &rpc,
        &need(&c.lez_password_file, "--lez-password-file (or FERRATA_LEZ_PASSWORD_FILE)")?,
        seed,
    )?;
    let program = account(&need(&c.bridge_account, "--bridge-account (or FERRATA_BRIDGE_ACCOUNT)")?)?;
    let cid = eth::view(&e.p, e.vault, IVault::lezConfigIdCall {}, None).await?;
    let payer = payer.map(account).transpose()?;
    let ctx = Context { program, cid: cid.0, chain_id: e.chain_id, vault: e.vault.into_array() };
    let seq = Sequencer { wallet, payer, bridge_program, synced: false, private_fee_payer: None, drop_from: None };
    // Reach the sequencer once, so that a wrong endpoint fails here with its address.
    seq.actor_state(lee_core::account::Actor::new(program, program))
        .await
        .with_context(|| format!("the LEZ sequencer {rpc} does not answer"))?;
    Ok((seq, ctx))
}

fn signer_of(c: &Common, send: &Send) -> Result<Option<flow::Signer>> {
    match (&send.keystore, &send.password_file) {
        (Some(k), Some(p)) => {
            let s = eth::keystore_signer(k, p)?;
            let address = s.address();
            let url = need(&c.eth_rpc, "--eth-rpc")?;
            Ok(Some(flow::Signer { p: eth::signing_provider(&url, s)?, address }))
        }
        (None, None) => Ok(None),
        _ => bail!("--keystore and --password-file go together"),
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let json = cli.common.json;
    let mut out = flow::Out::new(!json);
    match run(cli, &mut out).await {
        Ok((command, result)) => {
            if json {
                println!("{}", serde_json::json!({ "command": command, "report": out.lines, "result": result }));
            }
        }
        Err(e) => {
            if json {
                println!("{}", serde_json::json!({ "error": format!("{e:#}"), "report": out.lines }));
            } else {
                eprintln!("error: {e:#}");
            }
            std::process::exit(1);
        }
    }
}

fn kind_str(k: ferrata_bridge_core::inbound::DestKind) -> &'static str {
    if k == ferrata_bridge_core::inbound::DestKind::Private {
        "private"
    } else {
        "public"
    }
}

async fn run(cli: Cli, out: &mut flow::Out) -> Result<(&'static str, serde_json::Value)> {
    use serde_json::json;
    let c = &cli.common;
    let result = match cli.command {
        #[cfg(feature = "lez-wallet")]
        Command::Attest(a) => {
            let code = ferrata_cli::attest::run(a, c.sequencer.clone(), c.chain_id).await?;
            std::process::exit(code);
        }
        Command::Keys { accounts, show_secrets } => {
            let seed = seed_of(c)?;
            out.line("LEZ public accounts (wallet key tree, /a):");
            for a in seed.public_accounts(accounts)? {
                out.line(format!("  /{}  {}", a.index, hx(a.id.value())));
            }
            out.line("LEZ private accounts (wallet key tree, /a):");
            for (a, id) in seed.private_account_ids(accounts)?.iter().enumerate() {
                out.line(format!("  /{a}  {}", hx(id.value())));
            }
            let (k_auth, k_view) = seed.bridge_key_bytes();
            if show_secrets {
                out.line(format!("kAuth  {}  (authorises claims and releases: never share)", hx(&k_auth)));
                out.line(format!("kView  {}  (discloses burns and recipients, cannot claim or release)", hx(&k_view)));
            } else {
                out.line(format!(
                    "kAuth  fingerprint {}  (sha256 of the key; --show-secrets prints it)",
                    hx(&ferrata_bridge_core::sha256(&k_auth)[..8])
                ));
                out.line(format!("kView  fingerprint {}", hx(&ferrata_bridge_core::sha256(&k_view)[..8])));
            }
            ("keys", json!({}))
        }

        Command::Deposit { token, denomination, kind, i_accept_public_account, account: a, from, approve, send } => {
            let seed = seed_of(c)?;
            let e = eth_side(c).await?;
            let signer = signer_of(c, &send)?;
            let o = flow::DepositOpts {
                token,
                denomination,
                public: kind == Kind::Public,
                accept_public: i_accept_public_account,
                account: a,
                gap: c.gap,
                from,
                approve,
                send: send.send,
                yes: send.yes,
                confirmations: send.confirmations,
                // The deposit journal beside the user's other local state: the LEZ wallet's home, else the keystore's
                // directory.
                journal: c.lez_wallet.clone().or_else(|| {
                    send.keystore.as_deref().and_then(std::path::Path::parent).map(std::path::Path::to_path_buf)
                }),
            };
            let d = flow::deposit(&e, &seed, signer.as_ref(), &o, out).await?;
            let result = json!({
                "commitment": hx(&d.commitment),
                "nullifier": d.nullifier.map(|n| hx(&n)),
                "j": d.j,
                "sent": d.sent.map(|(tx, leaf)| json!({ "tx": tx.map(|t| t.to_string()), "leaf": leaf })),
                "earlier": d.earlier,
            });
            ("deposit", result)
        }

        Command::Scan | Command::Recover => {
            let recover = matches!(cli.command, Command::Recover);
            let seed = seed_of(c)?;
            let e = eth_side(c).await?;
            let r = if c.sequencer.is_some() || recover {
                let (mut ledger, ctx) = zone(c, &e, &seed, None, None).await?;
                flow::recover(&e, &seed, c.gap, Some((&mut ledger, &ctx)), recover, c.lez_wallet.as_deref(), out)
                    .await?
            } else {
                flow::recover::<Sequencer>(&e, &seed, c.gap, None, false, None, out).await?
            };
            let result = json!({
                "deposits": r.deposits,
                "claimed": r.claimed,
                "refunded": r.refunded,
                "unclaimed": r.unclaimed.iter().map(|(k, a)| json!({ "kind": kind_str(*k), "account": a })).collect::<Vec<_>>(),
                "burns": r.burns,
                "released": r.released,
                "next_burn_counter": r.next_burn_counter,
                "holdings": r.holdings.iter().map(|(k, a, t, n)| json!({ "kind": kind_str(*k), "account": a, "token": t.to_string(), "amount": n.to_string() })).collect::<Vec<_>>(),
            });
            (if recover { "recover" } else { "scan" }, result)
        }

        Command::Claim {
            leaf,
            no_delay,
            plaintext,
            i_accept_plaintext_links_deposit,
            relayer,
            payer,
            bridge_program,
            gas_drop_inbox,
            gas_drop_elf,
            gas_drop_wait,
            no_gas_drop,
            send,
            yes,
        } => {
            let seed = seed_of(c)?;
            let e = eth_side(c).await?;
            let gas_drop = match gas_drop_inbox {
                Some(inbox) if !no_gas_drop && !plaintext => Some(flow::GasDropOpts {
                    inbox,
                    elf: need(&gas_drop_elf, "--gas-drop-elf (or FERRATA_GASDROP_ELF: the gas-drop guest)")?,
                    record: need(&c.lez_wallet, "--lez-wallet")?.join("ferrata-gas-drops.json"),
                    wait_secs: gas_drop_wait,
                }),
                _ => None,
            };
            let (mut ledger, ctx) = zone(c, &e, &seed, payer.as_deref(), bridge_program).await?;
            let o = flow::ClaimOpts {
                leaf,
                plaintext,
                accept_plaintext: i_accept_plaintext_links_deposit,
                no_delay,
                relayer: relayer.as_deref().map(ferrata_cli::h32).transpose()?,
                gap: c.gap,
                send,
                yes,
                gas_drop,
            };
            let r = flow::claim(&e, &seed, &mut ledger, &ctx, &o, out).await?;
            let result = json!({
                "nullifier": hx(&r.plan.nullifier),
                "root_leaf_count": r.plan.leaf_count,
                "private": !o.plaintext,
                "gasDrop": o.gas_drop.is_some(),
                "sent": r.sent,
            });
            ("claim", result)
        }

        Command::Burn {
            from,
            i_accept_public_account,
            account: a,
            token,
            amount,
            recipient,
            payer,
            bridge_program,
            send,
            yes,
        } => {
            let seed = seed_of(c)?;
            let e = eth_side(c).await?;
            let (mut ledger, ctx) = zone(c, &e, &seed, payer.as_deref(), bridge_program).await?;
            let o = flow::BurnOpts {
                public: from == Kind::Public,
                accept_public: i_accept_public_account,
                account: a,
                token,
                amount,
                recipient,
                send,
                yes,
                journal: c.lez_wallet.clone(),
            };
            let r = flow::burn(&e, &seed, &mut ledger, &ctx, &o, out).await?;
            let result = json!({
                "burn_counter": r.plan.i,
                "r": hx(&r.plan.note.r),
                "c_out": hx(&r.plan.note.c_out),
                "sent": r.sent,
                "already_sent": r.already,
            });
            ("burn", result)
        }

        Command::Release { burn, self_send, relayer, job_out, program, prove, input_out, seal, send } => {
            let seed = seed_of(c)?;
            let e = eth_side(c).await?;
            let (mut ledger, ctx) = zone(c, &e, &seed, None, None).await?;
            ensure!(!(self_send && relayer.is_some()), "--self-send and --relayer exclude each other");
            let seal = match &seal {
                Some(path) => Some(Bytes::from(
                    hex::decode(
                        std::fs::read_to_string(path)
                            .with_context(|| format!("reading the seal {}", path.display()))?
                            .trim()
                            .trim_start_matches("0x"),
                    )
                    .context("the seal file is not hex")?,
                )),
                None => None,
            };
            let signer = signer_of(c, &send)?;
            let o = flow::ReleaseOpts {
                burn,
                journal: c.lez_wallet.clone(),
                via: if self_send { flow::Via::Myself } else { flow::Via::Relayer(relayer) },
                seal,
                job_out,
                input_out,
                program,
                prove,
                send: send.send,
                yes: send.yes,
                confirmations: send.confirmations,
            };
            let r = flow::release_burn(&e, &seed, &mut ledger, &ctx, signer.as_ref(), &o, out).await?;
            let result = json!({
                "nullifier": hx(&r.plan.nullifier),
                "burn_root": r.root.root.to_string(),
                "checkpoint": r.root.index,
                "calldata": r.input.to_string(),
                "job": r.job.map(|p| p.display().to_string()),
                "sent": r.sent.map(|h| h.to_string()),
            });
            ("release", result)
        }

        Command::Status { token } => {
            let eth_p = eth::provider(&need(&c.eth_rpc, "--eth-rpc (or FERRATA_ETH_RPC)")?)?;
            let vault = need(&c.vault, "--vault (or FERRATA_VAULT)")?;
            let leaves = eth::view(&eth_p, vault, IVault::nextLeafIndexCall {}, None).await?;
            let root = eth::view(&eth_p, vault, IVault::depositRootCall {}, None).await?;
            let cid = eth::view(&eth_p, vault, IVault::lezConfigIdCall {}, None).await?;
            let retired = eth::view(&eth_p, vault, IVault::retiredCall {}, None).await?;
            let dep_frozen = eth::view(&eth_p, vault, IVault::depositsFrozenUntilCall {}, None).await?;
            let rel_frozen = eth::view(&eth_p, vault, IVault::releasesFrozenUntilCall {}, None).await?;
            let checkpoints = eth::view(&eth_p, vault, IVault::checkpointCountCall {}, None).await?;
            let imported = eth::view(&eth_p, vault, IVault::finalImportedLeafCountCall {}, None).await?;
            out.line(format!("vault {vault}: {leaves} deposits, root {root}, configId {cid}"));
            out.line(format!(
                "  retired {retired}; deposits frozen until {dep_frozen}; releases frozen until {rel_frozen}"
            ));
            out.line(format!("  checkpoints {checkpoints}; final imported leaf count {imported}"));
            for (dir, name) in [(0u8, "deposit"), (1u8, "release")] {
                let g = eth::view(&eth_p, vault, IVault::globalCapCall { direction: dir }, None).await?;
                let show = |x: U256| if x == U256::MAX { "unlimited".to_string() } else { format!("{x} value units") };
                out.line(format!(
                    "  global {name} cap: capacity {}, refill {}/s, available {}",
                    show(g.capacity),
                    show(g.refillPerSecond),
                    show(g.available)
                ));
            }
            let mut tokens = token;
            if tokens.is_empty() {
                tokens.push(eth::ETH);
            }
            for t in tokens {
                let cfg = eth::view(&eth_p, vault, IVault::tokenConfigCall { token: t }, None).await?;
                let (decimals, symbol) = eth::token_meta(&eth_p, t).await?;
                out.line(format!(
                    "token {t} ({symbol}): registered {}, deposits enabled {}, lezUnit {}",
                    cfg.registered, cfg.depositsEnabled, cfg.lezUnit
                ));
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
                    out.line(format!(
                        "  [{k}] {}  mint fee {}  relay fee {}  burn fee {}  checkpoint fee {}",
                        show(*d),
                        show(f.mintFee),
                        show(f.relayFee),
                        show(f.burnFee),
                        show(f.checkpointFee)
                    ));
                }
                let dep_frozen =
                    eth::view(&eth_p, vault, IVault::tokenDepositsFrozenUntilCall { token: t }, None).await?;
                let rel_frozen =
                    eth::view(&eth_p, vault, IVault::tokenReleasesFrozenUntilCall { token: t }, None).await?;
                let delay = eth::view(
                    &eth_p,
                    vault,
                    IVault::releaseDelayForCall { token: t, checkpointIndex: U256::from(checkpoints) },
                    None,
                )
                .await?;
                out.line(format!(
                    "  deposits frozen until {dep_frozen}; releases frozen until {rel_frozen}; release delay {delay} s after a checkpoint"
                ));
                for (dir, name) in [(0u8, "deposit"), (1u8, "release")] {
                    let cap = eth::view(&eth_p, vault, IVault::tokenCapCall { token: t, direction: dir }, None).await?;
                    let show = |x: U256| match u128::try_from(x) {
                        _ if x == U256::MAX => "unlimited".to_string(),
                        Ok(x) => units::show(x, decimals, &symbol),
                        Err(_) => x.to_string(),
                    };
                    out.line(format!(
                        "  {name} cap: capacity {}, refill {}/s, available {}",
                        show(cap.capacity),
                        show(cap.refillPerSecond),
                        show(cap.available)
                    ));
                }
            }
            if c.sequencer.is_some() {
                let seed = seed_of(c)?;
                let e = eth_side(c).await?;
                let (ledger, ctx) = zone(c, &e, &seed, None, None).await?;
                let history = lez::history_of(&ledger, &ctx).await?;
                for line in lez::zone_status(&ledger, &ctx, &history).await? {
                    out.line(line);
                }
            }
            ("status", json!({}))
        }
    };
    Ok(result)
}
