//! The gas drop (bridge.md §6, v3 D13; M0 check 2) on a running LEZ v0.4.0-rc3 sequencer, with LEZ's own wallet
//! (`lez/live/hooks/gasdrop.sh` runs the steps in order; the relayer's receipt check is `ferrata-gasdrop relay`, cli/).
//!
//! - `gas-drop-keys`: the claimant's new wallet (a fresh mnemonic, never written to the records): its private account
//!   (the deposit's destination), a private one-time account (the address the relayer credits) and a public account;
//!   the public ids and the one-time account's public keys to `--keys`.
//! - `gas-drop-prepare` (after crown-b `--operator-only yes`): the claimant's balances before, read from its wallet and
//!   from the chain; the negative control (the same `ClaimPrivately`, with nothing to pay its fee from: the wallet's
//!   dry run and its refusal, nothing sent); the gas-drop guest's input (the claim witness under `root(cid, 2)`, the
//!   one-time address, the relayer), the roots LEZ imported and the claimed nullifiers for the relayer.
//! - `gas-drop-credit` (the relayer's credit command): checks that the one-time account's keys derive the receipt's
//!   address, then sends one native credit from the relayer's private account (LEZ's test private account 1), sealed
//!   to the one-time account (`Publication::Sealed`), proven for real; prints its transaction.
//! - `gas-drop-claim`: the sealed credit read from the chain by the claimant's wallet, the dry run (it must claim that
//!   credit before the root), then the `ClaimPrivately` that sweeps it into the one-time account and pays its fee from
//!   it, proven for real and settled; the balances, the refund and the relay fee after.
//!
//! Everything a step decides is in `<gasdrop-dir>/<step>.json`. The relayer is paid the relay fee by the claim
//! itself (`relayer: Some(R)`, which the final bridge pays in the wrapped token, lez/guests/bridge `issue`).

use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use common::HashType;
use ferrata_bridge_core::{
    encoding::ClaimInput,
    gasdrop::{verify_gas_drop, GasDropInput},
    inbound::{ClaimWitness as CoreWitness, DestKind as CoreKind},
    tree::MerklePath,
};
use ferrata_lez_bridge_wire::{config_seed, nullifier_seed, root_seed, token_seed, BridgeMessage, BridgeState};
use lee::{privacy_preserving_transaction::circuit::ProgramCatalog, program::Program, AccountId, Publication};
use lee_core::{
    encryption::ViewingPublicKey,
    native_token::{self, NATIVE_TOKEN_PROGRAM_ID},
    NullifierPublicKey, PrivateAccountKind,
};
use serde_json::{json, Value};
use wallet::{AccountIdentity, AccountMention, DeclaredFee, DryRunReport, ExecutionFailureKind, PendingMessage, WalletCore};

use super::{
    bridge_state, deployment_accounts, fee_recipient, h, holder_pda, holder_state, range_for, witness_to, Chain, Identity,
    BRIDGE, ETH,
};
use crate::{
    keys::{account_of, derived_key},
    live::{check_loopback, mention, Live, Sent},
    manifest::Row,
    plan::{Pda, SEPOLIA_CHAIN_ID},
    private::{describe_private, private_wallet_at, TapClient},
    programs::GuestBinary,
    Args,
};

/// The deposit the claimant claims: the hook's second deposit (leaf 1, its `j` 1), private, to the claimant's account.
const LEAF: u64 = 1;
/// The leaf count of the imported root the claim and the receipt name.
const LEAF_COUNT: u64 = 2;
/// The sweep scan's limit in LEZ's wallet (`MAX_SWEEP_SCAN_BLOCKS`, lez/wallet/src/lib.rs at v0.4.0-rc3).
const MAX_SWEEP_SCAN_BLOCKS: u64 = 64;

/// The relayer's LEZ account id: the one the receipt binds and the claim pays the relay fee to (a throwaway key).
pub fn relayer_id() -> AccountId {
    account_of(&derived_key("gasdrop/relayer"))
}

fn hexa(id: &AccountId) -> String {
    format!("0x{}", hex::encode(id.value()))
}

fn h32(text: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(text.trim().trim_start_matches("0x"))?;
    bytes.try_into().map_err(|b: Vec<u8>| anyhow!("{} bytes, not 32", b.len()))
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)? + "\n").with_context(|| format!("writing {}", path.display()))
}

fn read_json(path: &Path) -> Result<Value> {
    serde_json::from_str(&fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?).map_err(Into::into)
}

fn utc_now() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = (s / 86_400, s % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// The claimant's wallet files (as `private_wallet_at`'s, without importing a test account).
fn wallet_files(home: &Path, rpc: &str) -> Result<(PathBuf, PathBuf)> {
    check_loopback(rpc)?;
    fs::create_dir_all(home)?;
    let config_path = home.join("wallet_config.json");
    fs::write(
        &config_path,
        format!(
            "{{\n  \"sequencer\": {{ \"sequencer_addr\": \"{rpc}\" }},\n  \"seq_poll_timeout\": \"1s\",\n  \
             \"seq_tx_poll_max_blocks\": 120,\n  \"seq_poll_max_retries\": 2,\n  \"seq_block_poll_max_amount\": 100\n}}\n"
        ),
    )?;
    Ok((config_path, home.join("storage.json")))
}

fn load_claimant(home: &Path, rpc: &str, client: TapClient) -> Result<WalletCore<TapClient>> {
    let (config_path, storage_path) = wallet_files(home, rpc)?;
    ensure!(storage_path.exists(), "no claimant wallet at {} (gas-drop-keys)", home.display());
    WalletCore::new_update_chain_with_client(config_path, storage_path, None, "", client)
}

/// The claimant's accounts, from `--keys`.
struct Keys {
    main: AccountId,
    one_time: AccountId,
    public: AccountId,
}

impl Keys {
    fn load(path: &Path) -> Result<Self> {
        let v = read_json(path)?;
        let id = |k: &str| -> Result<AccountId> {
            Ok(AccountId::new(h32(v[k]["hex"].as_str().ok_or_else(|| anyhow!("{}: no {k}.hex", path.display()))?)?))
        };
        Ok(Self { main: id("main")?, one_time: id("oneTime")?, public: id("public")? })
    }
}

/// A private account's native balance as the wallet holds it: `None` when the wallet holds no state for it.
fn private_balance<C: wallet::WalletSequencerClient>(wallet: &WalletCore<C>, id: AccountId) -> Value {
    match wallet.get_account_private(id) {
        None => json!({ "wallet": "no state", "native": "0" }),
        Some(account) => match account.data.native_balance() {
            Ok(balance) => json!({ "wallet": "state", "native": balance.to_string() }),
            Err(e) => json!({ "wallet": format!("unreadable: {e:?}"), "native": null }),
        },
    }
}

fn pending_json(p: &PendingMessage) -> Value {
    json!({
        "position": format!("{:?}", p.position),
        "publication": match &p.publication { Publication::Sealed(_) => "Sealed", Publication::Clear { .. } => "Clear" },
        "to": p.body.to.account_id.to_string(),
        "recipientAccount": p.recipient.account_id().to_string(),
        "blinded": p.recipient.opening.is_some(),
        "nativeCredit": p.native_credit().map(|c| c.to_string()),
    })
}

fn dry_json(report: &DryRunReport) -> Value {
    match report {
        DryRunReport::Private { fee, quote, cycles, fee_at_head, headroom_blocks, exec_ceiling, stor_ceiling, headroom_reserve, claims, swept } => {
            json!({
                "payer": fee.payer.to_string(), "reserve": fee.reserve.to_string(), "refundTo": fee.refund_to.map(|a| a.to_string()),
                "height": quote.height, "baseFeeExec": quote.base_fee_exec, "baseFeeStor": quote.base_fee_stor,
                "publicCycles": cycles, "feeAtHead": fee_at_head.to_string(), "headroomBlocks": headroom_blocks,
                "execCeiling": exec_ceiling, "storCeiling": stor_ceiling, "headroomReserve": headroom_reserve.to_string(),
                "claims": claims, "swept": swept.to_string(),
            })
        }
        DryRunReport::Public { .. } => json!({ "public": format!("{report}") }),
    }
}

/// The public view of `id` on the chain: the native balance the sequencer serves (any account id has one, 0 if unknown).
async fn chain_balance(live: &Live, id: AccountId) -> Result<String> {
    Ok(live.balance(id).await?.to_string())
}

/// What the claim and the receipt need from the chain and the captures.
struct Claim {
    bridge: AccountId,
    cid: [u8; 32],
    vault: [u8; 20],
    message: BridgeMessage,
    publics: Vec<Pda>,
    nullifier: [u8; 32],
    n_pda: Pda,
    roots: Vec<[u8; 32]>,
    input: GasDropInput,
}

async fn claim_parts(live: &Live, bridge_bin: &GuestBinary, chain: &Chain, identity: &Identity, keys: &Keys) -> Result<Claim> {
    let bridge = deployment_accounts(BRIDGE, bridge_bin).0;
    ensure!(identity.bridge_header == bridge, "session A deployed another bridge program");
    let stage = chain.stages.last().ok_or_else(|| anyhow!("no captures (--capture DIR, in order)"))?;
    ensure!(stage.deposits.len() as u64 == LEAF_COUNT, "the last capture holds {} deposits, not {LEAF_COUNT}", stage.deposits.len());
    let vault = stage.vault;
    let cid = identity.bridge_config(vault).id(bridge);
    let (w, nullifier, kind) = witness_to(&vault, &stage.deposits, LEAF, super::DestKind::Private, *keys.main.value())?;
    ensure!(kind == super::DestKind::Private, "the gas drop's deposit is private");
    let config_pda = Pda { program: bridge, seed: config_seed(&cid) };
    let token_pda = Pda { program: bridge, seed: token_seed(&cid, &ETH) };
    let root_pda = Pda { program: bridge, seed: root_seed(&cid, LEAF_COUNT) };
    let n_pda = Pda { program: bridge, seed: nullifier_seed(SEPOLIA_CHAIN_ID, &vault, &nullifier) };
    let mut roots = Vec::new();
    for n in 1..=LEAF_COUNT {
        match bridge_state(live, Pda { program: bridge, seed: root_seed(&cid, n) }).await? {
            Some(BridgeState::Root(r)) => roots.push(r.root),
            other => bail!("root(cid, {n}) is not imported: {other:?}"),
        }
    }
    let root = *roots.last().expect("two roots");
    let range = range_for(live, token_pda, LEAF).await?;
    let relayer = relayer_id();
    let message = BridgeMessage::ClaimPrivately {
        config_id: cid,
        leaf_count: LEAF_COUNT,
        witness: w.clone(),
        range,
        relayer: Some(*relayer.value()),
    };
    let publics = vec![root_pda, token_pda, config_pda, n_pda, holder_pda(bridge, fee_recipient()), holder_pda(bridge, relayer)];
    let siblings: [[u8; 32]; 32] = w.path.clone().try_into().map_err(|_| anyhow!("the path is not 32 siblings"))?;
    let input = GasDropInput {
        claim: ClaimInput {
            root,
            chain_id: ferrata_bridge_core::chain_id_u256(SEPOLIA_CHAIN_ID),
            vault,
            witness: CoreWitness {
                depositor: w.depositor,
                token: w.token,
                amount: w.amount,
                leaf_index: w.leaf_index,
                path: MerklePath { directions: w.leaf_index as u32, siblings },
                s: w.s,
                kind: CoreKind::Private,
                dest: w.dest,
            },
        },
        one_time_address: *keys.one_time.value(),
        relayer: *relayer.value(),
    };
    let journal = verify_gas_drop(&input).map_err(|e| anyhow!("the gas-drop statement does not hold natively: {e:?}"))?;
    ensure!(journal.nullifier == nullifier, "the receipt's N is not the claim's");
    Ok(Claim { bridge, cid, vault, message, publics, nullifier, n_pda, roots, input })
}

fn claim_mentions(c: &Claim, keys: &Keys) -> Vec<AccountMention> {
    let mut mentions = vec![AccountIdentity::PrivateOwned(keys.main).select_program_actor_state(c.bridge)];
    mentions.extend(c.publics.iter().map(|p| mention(*p)));
    mentions
}

/// The wallet's dry run of a private send: the report it returns instead of proving.
fn expect_dry(result: Result<(HashType, Vec<lee_core::SharedSecretKey>), ExecutionFailureKind>, what: &str) -> Result<DryRunReport> {
    match result {
        Err(ExecutionFailureKind::DryRun(report)) => Ok(*report),
        Err(e) => bail!("{what}: the dry run failed: {e}: {e:?}"),
        Ok(_) => bail!("{what}: a dry run sent a transaction"),
    }
}

fn holdings_json(state: Option<BridgeState>) -> Value {
    match state {
        Some(BridgeState::Holder(h)) => json!(h
            .holdings
            .iter()
            .map(|x| json!({ "token": format!("0x{}", hex::encode(x.token)), "amount": x.amount.to_string() }))
            .collect::<Vec<_>>()),
        None => json!([]),
        Some(other) => json!(format!("not a holder: {other:?}")),
    }
}

fn eth_held(state: &Value) -> u128 {
    state
        .as_array()
        .and_then(|a| a.iter().find(|x| x["token"] == format!("0x{}", hex::encode(ETH))))
        .and_then(|x| x["amount"].as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// `gas-drop-keys`: the claimant's new wallet and its three accounts.
pub async fn keys(live: &mut Live, dir: &Path, keys_out: &Path) -> Result<()> {
    let home = dir.join("claimant");
    let (config_path, storage_path) = wallet_files(&home, &live.opts.rpc)?;
    ensure!(!storage_path.exists(), "the claimant's wallet exists already at {}", home.display());
    let client = TapClient::recording(live.wallet.client_owned(), &dir.join("keys-rpc.jsonl"), &dir.join("keys-tx.borsh"))?;
    let (mut wallet, _mnemonic) = WalletCore::new_init_storage_with_client(config_path, storage_path, None, "", client)?;
    let (main, _) = wallet.create_new_account_private(None);
    let (one_time, _) = wallet.create_new_account_private(None);
    let (public, _) = wallet.create_new_account_public(None);
    let tip = wallet.start_at_chain_tip().await?;
    wallet.store_persistent_data()?;
    let found = wallet
        .storage()
        .key_chain()
        .private_account(one_time)
        .ok_or_else(|| anyhow!("the wallet does not hold the one-time account"))?;
    let (npk, vpk) = (found.key_chain.nullifier_public_key.0, found.key_chain.viewing_public_key.to_bytes().to_vec());
    let derived = AccountId::for_private_account(
        &NullifierPublicKey(npk),
        &ViewingPublicKey::from_bytes(vpk.clone()).map_err(|e| anyhow!("{e:?}"))?,
        &PrivateAccountKind::Regular,
    );
    ensure!(derived == one_time, "the one-time account's keys do not derive its id");
    let id = |a: &AccountId| json!({ "id": a.to_string(), "hex": hexa(a) });
    let value = json!({
        "_note": "the claimant's accounts (gas-drop-keys): a new wallet, its mnemonic kept out of the records",
        "createdAtBlock": tip,
        "main": id(&main),
        "oneTime": id(&one_time),
        "public": id(&public),
        "oneTimeNpk": format!("0x{}", hex::encode(npk)),
        "oneTimeVpk": format!("0x{}", hex::encode(&vpk)),
        "oneTimeVpkBytes": vpk.len(),
        "relayer": id(&relayer_id()),
    });
    write_json(keys_out, &value)?;
    println!("[gas-drop] claimant wallet at block {tip}: private {main}, one-time {one_time}, public {public}; relayer {}", relayer_id());
    Ok(())
}

/// `gas-drop-prepare`: the balances before, the negative control, the receipt's input and the relayer's view.
pub async fn prepare(live: &mut Live, args: &Args, bridge_bin: &GuestBinary, dir: &Path) -> Result<()> {
    let keys = Keys::load(&args.path("keys")?)?;
    let identity = Identity::load(&args.path("identity")?)?;
    let chain = crate::full_chain(args)?;
    let c = claim_parts(live, bridge_bin, &chain, &identity, &keys).await?;
    live.sync().await?;
    let head = live.head().await?;
    let tx_out = dir.join("negative-tx.borsh");
    let _ = fs::remove_file(&tx_out);
    let client = TapClient::recording(live.wallet.client_owned(), &dir.join("prepare-rpc.jsonl"), &tx_out)?;
    let mut wallet = load_claimant(&dir.join("claimant"), &live.opts.rpc, client)?;
    wallet.sync_to_block(head).await?;
    let pending: Vec<Value> = wallet.owned_pending_messages().await?.iter().map(pending_json).collect();
    let before = json!({
        "atBlock": head,
        "private": { "main": private_balance(&wallet, keys.main), "oneTime": private_balance(&wallet, keys.one_time) },
        "chain": {
            "public": chain_balance(live, keys.public).await?,
            "mainId": chain_balance(live, keys.main).await?,
            "oneTimeId": chain_balance(live, keys.one_time).await?,
        },
        "walletPublicAccounts": wallet.storage().key_chain().public_account_ids().map(|(a, _)| a.to_string()).collect::<Vec<_>>(),
        "pendingMessages": pending,
    });
    println!("[gas-drop] the claimant before the drop: {before}");

    // The negative control: the same claim, with nothing to pay its fee from.
    let catalog = ProgramCatalog::from([(c.bridge, Program::new(std::borrow::Cow::Owned(bridge_bin.bytes.clone()))?)]);
    let message = borsh::to_vec(&c.message)?;
    wallet.set_dry_run(true);
    let dry = expect_dry(
        wallet
            .send_privacy_preserving_tx_with_pre_check(claim_mentions(&c, &keys), 0, message.clone(), &catalog, Some(keys.one_time), |_| Ok(()))
            .await,
        "the negative control",
    )?;
    wallet.set_dry_run(false);
    println!("[gas-drop] negative control, dry run: {dry}");
    let seq_head_before = live.head().await?;
    let started = Instant::now();
    let attempt = wallet
        .send_privacy_preserving_tx_with_pre_check(claim_mentions(&c, &keys), 0, message, &catalog, Some(keys.one_time), |_| Ok(()))
        .await;
    let seconds = started.elapsed().as_secs_f64();
    let (refused, error) = match attempt {
        Ok((hash, _)) => (false, format!("SENT {hash}")),
        Err(e) => (true, format!("{e}: {e:?}")),
    };
    let sent_file = tx_out.exists();
    println!("[gas-drop] negative control (no credit), after {seconds:.1} s: refused {refused}: {error}");
    let reason_is_fee = error.contains("Fee reserve") || error.contains("holds less than");
    let negative = json!({
        "what": "the same ClaimPrivately (deposit 1 into the claimant's private account, fee payer the one-time account, relayer named) before any credit",
        "dryRun": dry_json(&dry),
        "refused": refused,
        "refusedBy": if refused { "wallet" } else { "none" },
        "error": error,
        "reasonIsFee": reason_is_fee,
        "transactionWritten": sent_file,
        "seconds": (seconds * 10.0).round() / 10.0,
        "sequencerHeadBefore": seq_head_before,
        "sequencerHeadAfter": live.head().await?,
    });
    ensure!(refused && !sent_file, "the negative control was not refused before sending: {error}");

    // The receipt's input, the request the claimant hands the relayer, and the relayer's view of the zone.
    fs::write(dir.join("gasdrop-input.hex"), hex::encode(c.input.encode()))?;
    let claimed = match bridge_state(live, c.n_pda).await? {
        Some(BridgeState::Nullifier(_)) => format!("0x{}\n", hex::encode(c.nullifier)),
        _ => String::new(),
    };
    fs::write(dir.join("claimed.txt"), &claimed)?;
    fs::write(dir.join("roots.txt"), c.roots.iter().map(|r| format!("0x{}\n", hex::encode(r))).collect::<String>())?;
    let k = read_json(&args.path("keys")?)?;
    write_json(
        &dir.join("request.json"),
        &json!({ "oneTimeAddress": k["oneTime"]["hex"], "oneTimeNpk": k["oneTimeNpk"], "oneTimeVpk": k["oneTimeVpk"] }),
    )?;
    let value = json!({
        "at": utc_now(),
        "configId": h(&c.cid),
        "vault": format!("0x{}", hex::encode(c.vault)),
        "chainId": SEPOLIA_CHAIN_ID,
        "nullifier": h(&c.nullifier),
        "leaf": LEAF,
        "leafCount": LEAF_COUNT,
        "importedRoots": c.roots.iter().map(|r| h(r)).collect::<Vec<_>>(),
        "claimedBefore": !claimed.is_empty(),
        "relayer": hexa(&relayer_id()),
        "before": before,
        "negativeControl": negative,
    });
    write_json(&dir.join("prepare.json"), &value)?;
    Ok(())
}

/// `gas-drop-credit --to HEX --amount N --request FILE`: the relayer's sealed credit (its credit command).
pub async fn credit(live: &mut Live, args: &Args, dir: &Path) -> Result<()> {
    let to = h32(args.get("to").ok_or_else(|| anyhow!("--to is required"))?)?;
    let amount: u128 = args.get("amount").ok_or_else(|| anyhow!("--amount is required"))?.parse()?;
    let request = read_json(&args.path("request")?)?;
    let npk = NullifierPublicKey(h32(request["oneTimeNpk"].as_str().ok_or_else(|| anyhow!("no oneTimeNpk"))?)?);
    let vpk_bytes = hex::decode(request["oneTimeVpk"].as_str().ok_or_else(|| anyhow!("no oneTimeVpk"))?.trim_start_matches("0x"))?;
    let vpk = ViewingPublicKey::from_bytes(vpk_bytes).map_err(|e| anyhow!("the one-time account's viewing key: {e:?}"))?;
    let derived = AccountId::for_private_account(&npk, &vpk, &PrivateAccountKind::Regular);
    ensure!(*derived.value() == to, "the request's keys derive {derived}, not the receipt's one-time address 0x{}", hex::encode(to));

    let rdir = dir.join("relayer");
    fs::create_dir_all(&rdir)?;
    let tx_out = rdir.join("transaction.borsh");
    let _ = fs::remove_file(&tx_out);
    live.sync().await?;
    let client = TapClient::recording(live.wallet.client_owned(), &rdir.join("rpc-transcript.jsonl"), &tx_out)?;
    let (mut wallet, me) = private_wallet_at(&rdir.join("wallet"), &live.opts.rpc, client, 1)?;
    let tip = wallet.start_at_chain_tip().await?;
    eprintln!("[gas-drop] relayer's private wallet at block {tip}: account {me}");
    let recipient = || AccountIdentity::PrivateForeign { npk, vpk: vpk.clone(), kind: PrivateAccountKind::Regular };
    let delivery = |w: &WalletCore<TapClient>| -> Result<wallet::AsyncDelivery> {
        let (_, mut delivery) = w
            .seal_destination(recipient().select_program_actor_state(NATIVE_TOKEN_PROGRAM_ID))
            .map_err(|e| anyhow!("sealing to the one-time account: {e}"))?;
        // As LEZ's native transfer facade does for a credit left pending (program_facades/mod.rs).
        delivery.promotions.listed_only = true;
        Ok(delivery)
    };
    let message = Program::serialize_message(native_token::Message::Transfer { to: derived, amount })?;
    // LEZ's NativeTokenTransfer::transfer checks the sender's note balance before the send claims its pending
    // credits; LEZ's test private account holds only its genesis credit, which this send claims: the same send
    // without that pre-check.
    wallet.set_dry_run(true);
    let dry = expect_dry(
        wallet
            .send_tx_with_pre_check(vec![AccountIdentity::PrivateOwned(me).balance()], 0, message.clone(), &ProgramCatalog::default(), None, delivery(&wallet)?, None, |_| Ok(()))
            .await,
        "the relayer's credit",
    )?;
    wallet.set_dry_run(false);
    eprintln!("[gas-drop] relayer's credit, dry run: {dry}");

    let step = "gas drop: the relayer's native credit, sealed to the one-time address (real proof)".to_string();
    live.rows.push(Row { phase: live.phase.clone(), step: step.clone(), status: "unfinished".into(), note: "the proof had not finished when the run was recorded".into(), ..Row::default() });
    live.save()?;
    let pending_row = live.rows.len() - 1;
    let since_head = live.head().await?;
    let started = Instant::now();
    let sent = wallet
        .send_tx_with_pre_check(vec![AccountIdentity::PrivateOwned(me).balance()], 0, message, &ProgramCatalog::default(), None, delivery(&wallet)?, None, |_| Ok(()))
        .await;
    let seconds = started.elapsed().as_secs_f64();
    live.rows.remove(pending_row);
    let hash = match sent {
        Ok((hash, _)) => hash,
        Err(e) => {
            live.rows.push(Row { phase: live.phase.clone(), step, status: "rejected".into(), note: format!("after {seconds:.1} s: {e}: {e:?}"), ..Row::default() });
            live.save()?;
            bail!("the relayer's credit was not sent: {e}");
        }
    };
    let described = describe_private(&tx_out).unwrap_or_else(|e| format!("transaction not described: {e}"));
    let outcome = live
        .settle(Sent {
            step,
            hash,
            sent_at: Instant::now(),
            since_head,
            payer: None,
            balance_before: 0,
            harness: None,
            note: format!("execute_and_prove and submission {seconds:.1} s; {described}"),
        })
        .await?;
    live.save()?;
    ensure!(outcome.applied(), "the relayer's credit: {} {} {}", outcome.row.status, outcome.row.exit, outcome.row.note);
    let value = json!({
        "at": utc_now(),
        "relayerPrivateAccount": me.to_string(),
        "relayerPrivateAccountIs": "LEZ's test private account 1 (its genesis pending credit funds it; this send claims it)",
        "to": format!("0x{}", hex::encode(to)),
        "amount": amount.to_string(),
        "keysDeriveTo": true,
        "dryRun": dry_json(&dry),
        "tx": outcome.row.tx_hash,
        "block": outcome.row.block,
        "status": outcome.row.status,
        "provenAndSentS": (seconds * 10.0).round() / 10.0,
        "described": described,
        "row": { "fee": outcome.row.fee, "execGas": outcome.row.exec_gas, "storGas": outcome.row.stor_gas },
    });
    write_json(&dir.join("credit.json"), &value)?;
    // The relayer tool reads the credit's transaction from the last line.
    println!("{}", outcome.row.tx_hash);
    Ok(())
}

/// `gas-drop-claim`: the claim that sweeps the sealed credit and pays its fee from it.
pub async fn claim(live: &mut Live, args: &Args, bridge_bin: &GuestBinary, dir: &Path) -> Result<()> {
    let keys = Keys::load(&args.path("keys")?)?;
    let identity = Identity::load(&args.path("identity")?)?;
    let amount: u128 = args.get("amount").ok_or_else(|| anyhow!("--amount is required"))?.parse()?;
    let chain = crate::full_chain(args)?;
    let c = claim_parts(live, bridge_bin, &chain, &identity, &keys).await?;
    let relayer = relayer_id();
    let tx_out = dir.join("claim-tx.borsh");
    let _ = fs::remove_file(&tx_out);
    live.sync().await?;
    let client = TapClient::recording(live.wallet.client_owned(), &dir.join("claim-rpc.jsonl"), &tx_out)?;
    let mut wallet = load_claimant(&dir.join("claimant"), &live.opts.rpc, client)?;

    // The sealed credit, as the claimant's wallet reads it from the chain.
    let pending = wallet.owned_pending_messages().await?;
    let head = live.head().await?;
    let synced = wallet.storage().last_synced_block();
    ensure!(head.saturating_sub(synced) <= MAX_SWEEP_SCAN_BLOCKS, "the claimant's wallet is synced to {synced} of {head}");
    let credits: Vec<&PendingMessage> =
        pending.iter().filter(|p| p.recipient.account_id() == keys.one_time && p.native_credit().is_some()).collect();
    ensure!(credits.len() == 1, "{} native credits pending to the one-time account, not 1", credits.len());
    let credit = credits[0];
    let sealed = matches!(credit.publication, Publication::Sealed(_));
    let binding = wallet.get_recovery_binding(keys.one_time).await?;
    let before = json!({
        "atBlock": head,
        "walletSyncedTo": synced,
        "private": { "main": private_balance(&wallet, keys.main), "oneTime": private_balance(&wallet, keys.one_time) },
        "chain": { "public": chain_balance(live, keys.public).await?, "mainId": chain_balance(live, keys.main).await?, "oneTimeId": chain_balance(live, keys.one_time).await? },
        "pendingMessages": pending.iter().map(pending_json).collect::<Vec<_>>(),
        "credit": pending_json(credit),
        "creditSealed": sealed,
        "recoveryBindingAtOneTimeAddress": binding.is_some(),
        "relayerHolder": holdings_json(holder_state(live, c.bridge, relayer).await?),
    });
    println!("[gas-drop] the claimant before its claim: {before}");
    ensure!(credit.native_credit() == Some(amount), "the pending credit is {:?}, not the relayer's {amount}", credit.native_credit());

    let catalog = ProgramCatalog::from([(c.bridge, Program::new(std::borrow::Cow::Owned(bridge_bin.bytes.clone()))?)]);
    let message = borsh::to_vec(&c.message)?;
    wallet.set_dry_run(true);
    let dry = expect_dry(
        wallet
            .send_privacy_preserving_tx_with_pre_check(claim_mentions(&c, &keys), 0, message.clone(), &catalog, Some(keys.one_time), |_| Ok(()))
            .await,
        "the claim",
    )?;
    wallet.set_dry_run(false);
    println!("[gas-drop] the claim, dry run: {dry}");
    if let DryRunReport::Private { claims, swept, fee, .. } = &dry {
        ensure!(*claims == 1 && *swept == amount && fee.payer == keys.one_time, "the claim would not sweep the credit: {}", dry_json(&dry));
    } else {
        bail!("the claim's dry run is not a private one");
    }

    let step = "gas drop: ClaimPrivately of deposit 1 into a fresh private account, its fee paid from the swept sealed credit (real proof)".to_string();
    live.rows.push(Row { phase: live.phase.clone(), step: step.clone(), status: "unfinished".into(), note: "the proof had not finished when the run was recorded".into(), ..Row::default() });
    live.save()?;
    let pending_row = live.rows.len() - 1;
    let since_head = live.head().await?;
    let started = Instant::now();
    let sent = wallet
        .send_privacy_preserving_tx_with_pre_check(claim_mentions(&c, &keys), 0, message, &catalog, Some(keys.one_time), |_| Ok(()))
        .await;
    let seconds = started.elapsed().as_secs_f64();
    live.rows.remove(pending_row);
    let hash = match sent {
        Ok((hash, _)) => hash,
        Err(e) => {
            live.rows.push(Row { phase: live.phase.clone(), step, status: "rejected".into(), note: format!("after {seconds:.1} s: {e}: {e:?}"), ..Row::default() });
            live.save()?;
            bail!("the claim was not sent: {e}");
        }
    };
    let declared = wallet.take_declared_fee();
    let reserve = match declared {
        Some(DeclaredFee::Private(f)) => Some(f.reserve),
        _ => None,
    };
    let described = describe_private(&tx_out).unwrap_or_else(|e| format!("transaction not described: {e}"));
    let outcome = live
        .settle(Sent {
            step,
            hash,
            sent_at: Instant::now(),
            since_head,
            payer: None,
            balance_before: 0,
            harness: None,
            note: format!("execute_and_prove and submission {seconds:.1} s; {described}"),
        })
        .await?;
    live.save()?;
    ensure!(outcome.applied(), "the claim: {} {} {}", outcome.row.status, outcome.row.exit, outcome.row.note);

    // After: a new instance of the claimant's wallet, from its stored keys, follows the chain from where the keys were
    // made to the head (no state of the sending instance carries over: what it lists is what the chain holds).
    drop(wallet);
    live.sync().await?;
    let head = live.head().await?;
    let client = TapClient::recording(live.wallet.client_owned(), &dir.join("after-rpc.jsonl"), &dir.join("after-tx.borsh"))?;
    let mut wallet = load_claimant(&dir.join("claimant"), &live.opts.rpc, client)?;
    wallet.sync_to_block(head).await?;
    let after_pending = wallet.owned_pending_messages().await?;
    let main_holder = wallet
        .get_account_private(keys.main)
        .map(|a| a.data.actor_state(c.bridge).to_vec())
        .filter(|s| !s.is_empty())
        .map(|s| borsh::from_slice::<BridgeState>(&s))
        .transpose()?;
    let main_holdings = holdings_json(main_holder);
    let relayer_holdings = holdings_json(holder_state(live, c.bridge, relayer).await?);
    let fee_recipient_holdings = holdings_json(holder_state(live, c.bridge, fee_recipient()).await?);
    let spent = matches!(bridge_state(live, c.n_pda).await?, Some(BridgeState::Nullifier(_)));
    fs::write(dir.join("claimed-after.txt"), if spent { format!("0x{}\n", hex::encode(c.nullifier)) } else { String::new() })?;
    let one_time_after = private_balance(&wallet, keys.one_time);
    let note_after: u128 = one_time_after["native"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
    let refunds: Vec<&PendingMessage> =
        after_pending.iter().filter(|p| p.recipient.account_id() == keys.one_time && p.native_credit().is_some()).collect();
    let refund: u128 = refunds.iter().filter_map(|p| p.native_credit()).sum();
    let fee: u128 = outcome.row.fee.parse().unwrap_or(0);
    let credit_left = refunds.iter().any(|p| p.position == credit.position);
    let after = json!({
        "atBlock": head,
        "private": { "main": private_balance(&wallet, keys.main), "oneTime": one_time_after },
        "mainHoldings": main_holdings,
        "chain": { "public": chain_balance(live, keys.public).await?, "mainId": chain_balance(live, keys.main).await?, "oneTimeId": chain_balance(live, keys.one_time).await? },
        "pendingMessages": after_pending.iter().map(pending_json).collect::<Vec<_>>(),
        "relayerHolder": relayer_holdings,
        "feeRecipientHolder": fee_recipient_holdings,
        "nullifierSpent": spent,
    });
    println!("[gas-drop] after the claim: {after}");
    let value = json!({
        "at": utc_now(),
        "nullifier": h(&c.nullifier),
        "relayer": hexa(&relayer),
        "before": before,
        "dryRun": dry_json(&dry),
        "tx": outcome.row.tx_hash,
        "block": outcome.row.block,
        "status": outcome.row.status,
        "provenAndSentS": (seconds * 10.0).round() / 10.0,
        "described": described,
        "row": { "fee": outcome.row.fee, "execGas": outcome.row.exec_gas, "storGas": outcome.row.stor_gas, "baseFeeExec": outcome.row.base_fee_exec, "baseFeeStor": outcome.row.base_fee_stor },
        "after": after,
        "accounting": {
            "credit": amount.to_string(),
            "reserve": reserve.map(|r| r.to_string()),
            "feeCharged": fee.to_string(),
            "oneTimeNoteAfter": note_after.to_string(),
            "refundPending": refund.to_string(),
            "refundIsBlinded": refunds.iter().all(|p| p.recipient.opening.is_some()),
            "creditStillPending": credit_left,
            "creditEqualsNotePlusRefundPlusFee": note_after + refund + fee == amount,
            "relayFeeToRelayer": eth_held(&relayer_holdings).to_string(),
            "wrappedEthToClaimant": eth_held(&main_holdings).to_string(),
        },
    });
    write_json(&dir.join("claim.json"), &value)?;
    ensure!(spent && !credit_left, "after the claim: nullifier spent {spent}, credit still pending {credit_left}");
    Ok(())
}

/// The gas-drop commands (main.rs).
pub async fn run(live: &mut Live, args: &Args, bridge_bin: &GuestBinary) -> Result<()> {
    let dir = args.path("gasdrop-dir")?;
    fs::create_dir_all(&dir)?;
    match args.command.as_str() {
        "gas-drop-keys" => keys(live, &dir, &args.path("keys")?).await,
        "gas-drop-prepare" => prepare(live, args, bridge_bin, &dir).await,
        "gas-drop-credit" => credit(live, args, &dir).await,
        "gas-drop-claim" => claim(live, args, bridge_bin, &dir).await,
        other => bail!("unknown gas-drop command {other}"),
    }
}
