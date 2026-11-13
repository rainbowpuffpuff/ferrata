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
use lee::{AccountId, PrivateKey, PublicKey};
use serde_json::json;
use wallet::{
    program_facades::{native_token_transfer::NativeTokenTransfer, CreditDelivery},
    AccountIdentity, WalletCore,
};

/// local.json's second genesis account (LEZ publishes its key: `wallet-import-test-accounts`).
const FAUCET_KEY: &str = "f434f8741720014586ae43356d2aec6257da086222f604ddb75d69733b86fc4c";

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
    faucet_home: PathBuf,
    #[arg(long, default_value_t = 3_000_000_000_000)]
    public_amount: u128,
    #[arg(long, default_value_t = 2_000_000_000_000)]
    private_amount: u128,
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
    let key = PrivateKey::try_new(bytes).map_err(|e| anyhow!("the faucet key is not a scalar: {e}"))?;
    let id = AccountId::from(&PublicKey::new_from_private_key(&key));
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
    let seed = Seed::from_file(&a.seed)?;
    let first = &seed.public_accounts(1)?[0];
    let (public0, public0_key) = (first.id, PublicKey::new_from_private_key(&first.key));
    let private0 = seed.private_account_ids(1)?[0];

    // 1. The faucet pays the user's public account /0.
    let (faucet, faucet_id) = faucet_wallet(&a.faucet_home, &a.sequencer)?;
    let before =
        faucet.get_account_balance(public0).await.map_err(|e| anyhow!("reading the user's public balance: {e}"))?;
    let t0 = std::time::Instant::now();
    let (hash, _) = NativeTokenTransfer(&faucet)
        .transfer(
            AccountIdentity::Public(faucet_id),
            // A public account not used yet is named by its public key (LEZ's wallet CLI: `--to-pk`).
            AccountIdentity::PublicForeign(public0_key),
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
               "from": faucet_id.to_string(), "to_public_0": public0.to_string(), "amount": a.public_amount.to_string(),
               "balance_before": before.to_string(), "balance_after": after.to_string(), "send_ms": send_ms, "seen_ms": wait_ms})
    );

    // 2. The user's own wallet moves part of it to its private account /0, proven by LEZ's wallet.
    let mut user = lez::open_wallet(&a.lez_wallet, &a.sequencer, &a.lez_password_file, &seed)?;
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
    let mut seen = 0u128;
    let t1 = std::time::Instant::now();
    loop {
        user.sync_to_latest_block().await.map_err(|e| anyhow!("syncing the user's wallet: {e}"))?;
        seen = user.get_account_private(private0).and_then(|acc| acc.data.native_balance().ok()).unwrap_or(0);
        if seen > private_before {
            break;
        }
        ensure!(t1.elapsed().as_secs() < 300, "the private transfer: private /0 still holds {seen} after 300 s");
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
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
