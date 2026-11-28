//! The gas drop's stand-in on a running LEZ sequencer (`e2e/seqnet/`): native LEZ for the fees of a
//! user's private transactions. A private claim or burn pays its LEZ fee inside its proof, from a private
//! account of the wallet; a seed's private account starts empty. The relayer's gas drop (a sealed credit
//! to a private one-time account, swept inside the private claim) is not built in LEZ's wallet, so here:
//!
//! 1. a funded account of LEZ's local network (its published test key: the second genesis account of
//!    `lez/sequencer/service/networks/local.json`, as `lez/live/driver/src/keys.rs` lists it) pays the
//!    user's public account `/0` (a public transfer, sent by a faucet wallet of its own);
//! 2. the user's own LEZ wallet (the `ferrata` tool's wallet home, opened from the seed as the tool opens
//!    it) moves part of that to its private account `/0` (a private transfer, executed and proven by
//!    LEZ's wallet; `RISC0_DEV_MODE=1` makes the receipt a fake one, as for the tool's own calls).
//!
//! Both through LEZ's wallet library on the sequencer; the second step links the user's public account
//! `/0` to its private account `/0` for the amount moved, which the gas drop exists to avoid (labelled
//! wherever this runs). One JSON line per step on stdout.
//!
//!   seqnet-fund --sequencer URL --seed FILE --lez-wallet DIR --lez-password-file FILE --faucet-home DIR
//!               [--public-amount N] [--private-amount N]

use std::{path::PathBuf, time::Duration};

use anyhow::{anyhow, ensure, Result};
use clap::Parser;
use ferrata_cli::{lez, seed::Seed};
use lee::AccountId;
#[cfg(not(feature = "lez-040"))]
use lee::{PrivateKey, PublicKey};
use serde_json::json;
use wallet::{
    program_facades::{native_token_transfer::NativeTokenTransfer, CreditDelivery},
    AccountIdentity, WalletCore,
};

/// local.json's second genesis account (LEZ publishes its key: `wallet-import-test-accounts`).
#[cfg(not(feature = "lez-040"))]
const FAUCET_KEY: &str = "f434f8741720014586ae43356d2aec6257da086222f604ddb75d69733b86fc4c";
/// local.json's second genesis account on LEZ v0.4.0: its account secret (LEZ #1135), as v0.4.0's Justfile imports it
/// (`account import public --secret`, recipe `wallet-import-test-accounts`).
#[cfg(feature = "lez-040")]
const FAUCET_KEY: &str = "717940b1cc55e5d6b2066dbf1d9a3f26f212f4db08d02388177fcfedd8a9be1b";

/// What names the user's public /0 to the faucet: its public key up to rc3, its initial key state from v0.4.0.
#[cfg(not(feature = "lez-040"))]
type ForeignKey = PublicKey;
#[cfg(feature = "lez-040")]
type ForeignKey = lee_core::signing_key::SigningKeyState;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    sequencer: String,
    #[arg(long)]
    seed: PathBuf,
    #[arg(long)]
    lez_wallet: PathBuf,
    #[arg(long)]
    lez_password_file: PathBuf,
    #[arg(long)]
    #[arg(default_value = "seqnet-faucet")]
    faucet_home: PathBuf,
    #[arg(long, default_value_t = 3_000_000_000_000)]
    public_amount: u128,
    #[arg(long, default_value_t = 2_000_000_000_000)]
    private_amount: u128,
    /// Only step 1: native LEZ to the seed's public account /0 (a relayer's own account, which pays the fees of
    /// its public calls; e2e/seqnet's relayer root imports).
    #[arg(long)]
    public_only: bool,
    /// Only step 2: the seed's own public account /0 (funded some other way: a network's faucet or its operator)
    /// moves --private-amount to its private account /0. A gas-drop relayer's drop account on a network whose
    /// genesis keys are not LEZ's published ones (deploy/testnet: `ferrata-gasdrop-setup fund`). No --faucet-home.
    #[arg(long)]
    private_only: bool,
    /// Send nothing: print the seed's public /0 and private /0 native balances, read from the zone through its own
    /// wallet ({"public_0", "private_0"}). e2e/seqnet's check that nothing funded a user (hook-gasdrop.sh).
    #[arg(long)]
    balances: bool,
}

fn faucet_wallet(home: &PathBuf, rpc: &str) -> Result<(WalletCore<sequencer_service_rpc::SequencerClient>, AccountId)> {
    std::fs::create_dir_all(home)?;
    let config = home.join("wallet_config.json");
    let storage = home.join("storage.json");
    std::fs::write(
        &config,
        format!(
            "{{\n  \"sequencer\": {{ \"sequencer_addr\": \"{rpc}\" }},\n  \"seq_poll_timeout\": \"1s\",\n  \
             \"seq_tx_poll_max_blocks\": 120,\n  \"seq_poll_max_retries\": 2,\n  \"seq_block_poll_max_amount\": 100,\n  \
             \"gas_limit\": 4000000\n}}\n"
        ),
    )?;
    let bytes: [u8; 32] = hex::decode(FAUCET_KEY)?.try_into().map_err(|_| anyhow!("the faucet key is not 32 bytes"))?;
    #[cfg(not(feature = "lez-040"))]
    let (key, id) = {
        let key = PrivateKey::try_new(bytes).map_err(|e| anyhow!("the faucet key is not a scalar: {e}"))?;
        let id = AccountId::from(&PublicKey::new_from_private_key(&key));
        (key, id)
    };
    #[cfg(feature = "lez-040")]
    let (key, id) = {
        let key = lee::signature::PublicAccountSecret::new(bytes);
        let id = lee::signature::public_account_id(&key);
        (key, id)
    };
    let wallet = if storage.exists() {
        WalletCore::new_update_chain(config, storage, None, "")?
    } else {
        let (mut wallet, _mnemonic) = WalletCore::new_init_storage(config, storage, None, "")?;
        wallet.storage_mut().key_chain_mut().add_imported_public_account(key);
        wallet.store_persistent_data()?;
        wallet
    };
    Ok((wallet, id))
}

#[tokio::main]
async fn main() -> Result<()> {
    let a = Args::parse();
    // The sequencer's LEZ before any wallet is opened or anything is signed (src/lezver.rs).
    ferrata_cli::lezver::require(&a.sequencer).await?;
    let seed = Seed::from_file(&a.seed)?;
    let first = &seed.public_accounts(1)?[0];
    #[cfg(not(feature = "lez-040"))]
    let (public0, public0_key) = (first.id, PublicKey::new_from_private_key(&first.key));
    #[cfg(feature = "lez-040")]
    let (public0, public0_key) = (first.id, lee::signature::initial_key(&first.key));
    let private0 = seed.private_account_ids(1)?[0];

    ensure!(!(a.public_only && a.private_only), "--public-only and --private-only exclude each other");
    if a.balances {
        let mut user = lez::open_wallet(&a.lez_wallet, &a.sequencer, &a.lez_password_file, &seed)?;
        user.sync_to_latest_block().await.map_err(|e| anyhow!("syncing the wallet: {e}"))?;
        let public = user.get_account_balance(public0).await.map_err(|e| anyhow!("reading public /0: {e}"))?;
        let private = user.get_account_private(private0).and_then(|acc| acc.data.native_balance().ok()).unwrap_or(0);
        println!(
            "{}",
            json!({"step": "balances (nothing sent)", "public_0": public.to_string(), "private_0": private.to_string(),
                   "public_0_id": public0.to_string(), "private_0_id": private0.to_string()})
        );
        return Ok(());
    }
    if !a.private_only {
        step_faucet(&a, public0, public0_key).await?;
    }
    if a.public_only {
        return Ok(());
    }
    step_private(&a, &seed, public0, private0).await
}

/// 1. The faucet pays the user's public account /0.
async fn step_faucet(a: &Args, public0: AccountId, public0_key: ForeignKey) -> Result<()> {
    let (faucet, faucet_id) = faucet_wallet(&a.faucet_home, &a.sequencer)?;
    let before =
        faucet.get_account_balance(public0).await.map_err(|e| anyhow!("reading the user's public balance: {e}"))?;
    let t0 = std::time::Instant::now();
    let (hash, _) = NativeTokenTransfer(&faucet)
        .transfer(
            AccountIdentity::Public(faucet_id),
            // A public account not used yet is named by its public key (LEZ's wallet CLI: `--to-pk`).
            #[cfg(not(feature = "lez-040"))]
            AccountIdentity::PublicForeign(public0_key),
            #[cfg(feature = "lez-040")]
            AccountIdentity::public_foreign(public0_key).ok_or_else(|| anyhow!("public /0's initial key names no account"))?,
            a.public_amount,
            CreditDelivery::Automatic,
        )
        .await
        .map_err(|e| anyhow!("the faucet's transfer was not sent: {e:?}"))?;
    let send_ms = t0.elapsed().as_millis() as u64;
    let want = before + a.public_amount;
    let t1 = std::time::Instant::now();
    let after = loop {
        let now = faucet.get_account_balance(public0).await.map_err(|e| anyhow!("reading the balance: {e}"))?;
        if now >= want {
            break now;
        }
        ensure!(t1.elapsed().as_secs() < 180, "the faucet's transfer: public /0 holds {now}, not {want}, after 180 s");
        tokio::time::sleep(Duration::from_secs(2)).await;
    };
    let wait_ms = t1.elapsed().as_millis() as u64;
    println!(
        "{}",
        json!({"step": "faucet public transfer (stand-in for the gas drop)", "tx": hash.to_string(),
               "from": faucet_id.to_string(), "to_public_0": public0.to_string(),
               "to_public_0_hex": format!("0x{}", hex::encode(public0.value())), "amount": a.public_amount.to_string(),
               "balance_before": before.to_string(), "balance_after": after.to_string(), "send_ms": send_ms, "seen_ms": wait_ms})
    );
    Ok(())
}

/// 2. The user's own wallet moves part of it to its private account /0, proven by LEZ's wallet.
async fn step_private(a: &Args, seed: &Seed, public0: AccountId, private0: AccountId) -> Result<()> {
    let mut user = lez::open_wallet(&a.lez_wallet, &a.sequencer, &a.lez_password_file, seed)?;
    user.sync_to_latest_block().await.map_err(|e| anyhow!("syncing the user's wallet: {e}"))?;
    let private_before = user.get_account_private(private0).and_then(|acc| acc.data.native_balance().ok()).unwrap_or(0);
    let t0 = std::time::Instant::now();
    let (hash, _) = NativeTokenTransfer(&user)
        .transfer(
            AccountIdentity::Public(public0),
            AccountIdentity::PrivateOwned(private0),
            a.private_amount,
            CreditDelivery::Automatic,
        )
        .await
        .map_err(|e| anyhow!("the user's private transfer was not sent: {e:?}"))?;
    let send_ms = t0.elapsed().as_millis() as u64;
    // The transfer's own LEZ fee is paid inside its proof, from the private account's new balance: it holds the
    // amount less that fee once applied.
    let t1 = std::time::Instant::now();
    let seen = loop {
        user.sync_to_latest_block().await.map_err(|e| anyhow!("syncing the user's wallet: {e}"))?;
        let seen = user.get_account_private(private0).and_then(|acc| acc.data.native_balance().ok()).unwrap_or(0);
        if seen > private_before {
            break seen;
        }
        ensure!(t1.elapsed().as_secs() < 300, "the private transfer: private /0 still holds {seen} after 300 s");
        tokio::time::sleep(Duration::from_secs(3)).await;
    };
    user.store_persistent_data()?;
    println!(
        "{}",
        json!({"step": "the user's private transfer public /0 -> private /0 (proven by LEZ's wallet)", "tx": hash.to_string(),
               "from_public_0": public0.to_string(), "to_private_0": private0.to_string(), "amount": a.private_amount.to_string(),
               "private_balance_before": private_before.to_string(), "private_balance_after": seen.to_string(),
               "fee_paid_in_the_proof": (private_before + a.private_amount).saturating_sub(seen).to_string(),
               "dev_mode": std::env::var("RISC0_DEV_MODE").map(|v| v == "1" || v == "true").unwrap_or(false),
               "send_ms": send_ms, "seen_ms": t1.elapsed().as_millis() as u64})
    );
    Ok(())
}
