//! Every transaction a running LEZ sequencer has applied, one TSV line each (`e2e/seqnet/`'s record of
//! the zone's side): block, transaction hash, kind (public or privacy-preserving), reverted or not, the
//! program its call goes to (named when it is the bridge, the attestation program or the native token)
//! and the bridge message's variant, the calls a private transaction makes in the clear
//! (`lez::visible_calls`, as the `ferrata` tool reads a private burn's note), and the public accounts it
//! names. Read through LEZ's wallet library (a throwaway wallet home without keys).
//!
//!   seqnet-txs --sequencer URL --home DIR --bridge-account HEX [--attest-account HEX] > txs.tsv

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use borsh::BorshDeserialize as _;
use clap::Parser;
use common::transaction::LeeTransaction;
use ferrata_cli::lez;
use ferrata_lez_bridge_wire::BridgeMessage;
use lee::AccountId;
use wallet::{WalletCore, WalletSequencerClient as _};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    sequencer: String,
    #[arg(long)]
    home: PathBuf,
    #[arg(long)]
    bridge_account: String,
    #[arg(long)]
    attest_account: Option<String>,
}

fn account(hex_id: &str) -> Result<AccountId> {
    let bytes: [u8; 32] =
        hex::decode(hex_id.trim_start_matches("0x"))?.try_into().map_err(|_| anyhow!("an account id is 32 bytes"))?;
    Ok(AccountId::new(bytes))
}

fn variant(message: &[u8]) -> String {
    match BridgeMessage::try_from_slice(message) {
        Ok(m) => format!("{m:?}").split(['{', '(', ' ']).next().unwrap_or("?").to_string(),
        Err(_) => "undecoded".to_string(),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let a = Args::parse();
    let bridge = account(&a.bridge_account)?;
    let attest = a.attest_account.as_deref().map(account).transpose()?;
    let native = lee_core::native_token::NATIVE_TOKEN_PROGRAM_ID;
    let name = |p: &AccountId| -> String {
        if *p == bridge {
            "bridge".into()
        } else if Some(*p) == attest {
            "attestation".into()
        } else if *p == native {
            "native_token".into()
        } else {
            format!("0x{}", hex::encode(p.value()))
        }
    };
    std::fs::create_dir_all(&a.home)?;
    let config = a.home.join("wallet_config.json");
    let storage = a.home.join("storage.json");
    std::fs::write(
        &config,
        format!(
            "{{\n  \"sequencer\": {{ \"sequencer_addr\": \"{}\" }},\n  \"seq_poll_timeout\": \"1s\",\n  \
             \"seq_tx_poll_max_blocks\": 120,\n  \"seq_poll_max_retries\": 2,\n  \"seq_block_poll_max_amount\": 100,\n  \
             \"gas_limit\": 4000000\n}}\n",
            a.sequencer
        ),
    )?;
    let wallet = if storage.exists() {
        WalletCore::new_update_chain(config, storage, None, "")?
    } else {
        WalletCore::new_init_storage(config, storage, None, "")?.0
    };
    let client = wallet.client();
    let last = client.get_last_block_id().await.map_err(|e| anyhow!("get_last_block_id: {e}"))?;
    println!("block\tblock_hash\tbedrock_status\tindex\ttx_hash\tkind\treverted\tprogram\tbridge_variant\tvisible_bridge_calls\tpublic_accounts");
    let mut start = 1;
    while start <= last {
        let end = (start + 99).min(last);
        let blocks =
            client.get_block_range(start, end).await.map_err(|e| anyhow!("get_block_range {start}..{end}: {e}"))?;
        for block in blocks {
            for (i, tx) in block.body.transactions.iter().enumerate() {
                let reverted = block.body.reverted.contains(&(i as u64));
                let (kind, program, var, visible, publics) = match tx {
                    LeeTransaction::Public(t) => {
                        let root = &t.message.execution.root;
                        let p = root.to.program_account_id;
                        let v = if p == bridge { variant(&root.message) } else { "-".into() };
                        ("public", name(&p), v, "-".to_string(), "-".to_string())
                    }
                    LeeTransaction::PrivacyPreserving(t) => {
                        let root = t.message.execution.public_root.as_ref().map(|r| r.to.program_account_id);
                        let calls = lez::visible_calls(t, bridge);
                        let vis = if calls.is_empty() {
                            "-".to_string()
                        } else {
                            calls.iter().map(|c| variant(&c.message)).collect::<Vec<_>>().join(",")
                        };
                        let ids = t.message.public_account_ids();
                        (
                            "private",
                            root.map(|p| name(&p)).unwrap_or_else(|| "(private root)".into()),
                            "-".into(),
                            vis,
                            format!("{} named", ids.len()),
                        )
                    }
                };
                println!(
                    "{}\t{}\t{:?}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    block.header.block_id,
                    block.header.hash,
                    block.bedrock_status,
                    i,
                    tx.hash(),
                    kind,
                    reverted,
                    program,
                    var,
                    visible,
                    publics
                );
            }
        }
        start = end + 1;
    }
    Ok(())
}
