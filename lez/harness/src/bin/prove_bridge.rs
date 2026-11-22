//! The bridge program's private paths with real proofs: the private claim of vault A's leaf 5
//! (1 ETH, private kind) by its private destination, then that holder's private burn of the 1 ETH,
//! on the state of `harness/tests/bridge_private.rs` and `bridge_burn.rs` (`World::with_roots`).
//! Each is proven by `r0vm` (`RISC0_DEV_MODE` unset or 0; on the CPU, or on a GPU with a CUDA build
//! of r0vm 3.0.5) and settled through LEZ's private settlement with the receipt verified.
//!
//! * `prove_bridge [--shape wallet|bare] [--claim-out FILE] [--burn-out FILE]`: prove and settle
//!   the claim, then the burn. The `-out` options write the proven transactions (borsh
//!   `LeeTransaction`, receipt included). `--shape wallet` (the default) pads both as LEZ
//!   v0.4.0-rc1's wallet pads every private transaction (seven private actions, 512-byte notes, the
//!   fee's recovery note: [`harness::private::Shape::Wallet`]); `--shape bare` builds them as this
//!   binary did before 11 November 2026 (one private action, no padding, the refund to a public
//!   account).
//! * `prove_bridge --settle CLAIM BURN`: rebuild the same state and settle the two transactions,
//!   in that order, without proving. Each must claim this binary's bridge image, and LEZ's
//!   settlement verifies its receipt (a fake one only passes in dev mode).
//! * `--allow-dev-mode`: let the proving run in RISC Zero dev mode (a fake receipt in seconds), to
//!   try the commands; the output then says it is not a proof.
//! * `prove_bridge --image-ids`: print the bridge image this binary holds, and stop.
//! * `prove_bridge --transfer [--transfer-out FILE]` (milestone B2): prove and settle the claim,
//!   then, instead of the burn, that holder's private `Transfer` of 0.1 ETH to a public holder
//!   (`harness/tests/bridge_roles.rs` runs it in dev mode); `--settle CLAIM TRANSFER --transfer`
//!   settles the two files.
//!
//! `FERRATA_EXPECT_BRIDGE_IMAGE_ID=<hex>` stops it before any work unless the bridge image compiled
//! into this binary is that one.

use std::{fs, time::Instant};

use fee_core::BlockFeeSummary;
use ferrata_bridge_core::{
    chain_id_u256,
    outbound::{burn_commitment, burn_secret, burn_tag, xor_memo},
    BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{
    burn_actor, burn_mac_v2, own_actor, token_actor, word_of_u128, BridgeMessage, BridgeState, BurnRecord, HolderState,
    Holding, Note,
};
use harness::{
    bridge::{
        bridge_account, catalog, private_keys, private_post_state, role_account, World, CHAIN_ID, DENOMINATIONS, ETH,
    },
    grouped, image_id_hex,
    private::{PrivateAccount, PrivateRun, Shape},
    settle::{classify, opening_fee_state, settle_private_transaction, LeeTransaction},
};
use lee::Actor;
use lee_core::{execution_state::BoundaryStep, ProgramImageClaim};

/// The fee reserve of each private transaction, as in the bridge tests.
const RESERVE: u128 = 100_000_000;

fn usage() -> ! {
    eprintln!(
        "usage: prove_bridge [--shape wallet|bare] [--claim-out FILE] [--burn-out FILE] [--transfer] [--transfer-out FILE] [--allow-dev-mode] \
         | --settle CLAIM BURN | --image-ids"
    );
    std::process::exit(2)
}

fn dev_mode() -> bool {
    matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true"))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut claim_out, mut burn_out, mut settle_from, mut allow_dev_mode, mut only_ids) =
        (None, None, None, false, false);
    let mut shape = Shape::Wallet;
    let (mut transfer, mut transfer_out) = (false, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--claim-out" => claim_out = Some(it.next().unwrap_or_else(|| usage()).clone()),
            "--shape" => shape = Shape::parse(it.next().unwrap_or_else(|| usage())).unwrap_or_else(|| usage()),
            "--burn-out" => burn_out = Some(it.next().unwrap_or_else(|| usage()).clone()),
            "--settle" => {
                let claim = it.next().unwrap_or_else(|| usage()).clone();
                let burn = it.next().unwrap_or_else(|| usage()).clone();
                settle_from = Some((claim, burn));
            }
            "--allow-dev-mode" => allow_dev_mode = true,
            "--transfer" => transfer = true,
            "--transfer-out" => transfer_out = Some(it.next().unwrap_or_else(|| usage()).clone()),
            "--image-ids" => only_ids = true,
            _ => usage(),
        }
    }
    let dev = dev_mode();
    let bridge_image = image_id_hex(harness::bridge::BRIDGE_ID);
    println!(
        "bridge image {bridge_image} ({} ELF bytes); RISC0_DEV_MODE {}; shape {}",
        harness::bridge::BRIDGE_ELF.len(),
        if dev { "on" } else { "off" },
        shape.name()
    );
    if only_ids {
        return;
    }
    if let Ok(expected) = std::env::var("FERRATA_EXPECT_BRIDGE_IMAGE_ID") {
        assert_eq!(bridge_image, expected.trim().to_lowercase(), "this binary holds another bridge image");
    }
    assert!(
        settle_from.is_some() || !dev || allow_dev_mode,
        "unset RISC0_DEV_MODE (or set it to 0) for a real proof; --allow-dev-mode tries the commands with a fake receipt"
    );

    // The state both proofs are made against and settled on: the bridge's configurations paired
    // and their roots imported (public transactions), then the private destination of leaf 5
    // funded for the fee. Deterministic.
    let mut w = World::with_roots();
    let program = w.program;
    let (cid, vault) = (w.cid_a, w.vault_a.address);
    let seed = w.private_seed;
    let account = w.chain.add_private_account(seed, 1_000_000_000_000);
    let deposit = w.private_deposit.clone();
    assert_eq!(account.id().value(), &deposit.dest, "the deposit is for this private account");
    let n = deposit.nullifier(CHAIN_ID, &vault);
    let me = own_actor(program, account.id());
    let witness = w.vault_a.witness(&deposit, 8);
    let claim_message = w.claim_privately(&cid, 8, witness.clone());
    println!(
        "private claim: vault A leaf {} (1 ETH, private kind) under root n = 8, configuration {}; N = {}",
        deposit.leaf_index,
        hex::encode(&cid[..6]),
        hex::encode(n)
    );

    // --- The private claim -------------------------------------------------------------------
    let claim_actors = w.private_claim_actors(&cid, 8, &vault, &deposit);
    let claim = match &settle_from {
        Some((path, _)) => settle(&mut w, path, "private claim"),
        None => prove(&mut w, "private claim", &account, me, &claim_message, claim_actors, claim_out.as_deref(), shape),
    };
    assert!(claim.charge.revert.is_none(), "the claim's public part reverted: {:?}", claim.charge.revert);
    assert!(w.nullifier(&cid, &vault, &n).is_some(), "the nullifier account is written");
    let holding =
        HolderState { holdings: vec![Holding { config_id: cid, token: ETH, amount: DENOMINATIONS[1] }], pending: None };
    let claimed = private_post_state(&w.chain, &private_keys(seed), &account.account, RESERVE, &holding)
        .expect("the chain holds the claimed account's commitment (1 ETH held)");
    // Nothing of the deposit is in the transaction; N, the configuration and the root read are.
    let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(claim.tx.clone())).unwrap();
    let mut secret: Vec<(String, Vec<u8>)> = vec![
        ("destination".into(), deposit.dest.to_vec()),
        ("s".into(), deposit.s.to_vec()),
        ("C".into(), deposit.commitment().to_vec()),
        ("leaf".into(), deposit.leaf().to_vec()),
        ("depositor".into(), deposit.depositor.to_vec()),
        ("witness".into(), borsh::to_vec(&witness).unwrap()),
    ];
    for (i, sibling) in witness.path.iter().enumerate().filter(|(_, s)| **s != [0u8; 32]) {
        secret.push((format!("sibling {i}"), sibling.to_vec()));
    }
    let found: Vec<&String> =
        secret.iter().filter(|(_, needle)| occurs(&bytes, needle)).map(|(what, _)| what).collect();
    assert!(found.is_empty(), "private inputs in the transaction: {found:?}");
    for (what, needle) in [("N", n.to_vec()), ("configId", cid.to_vec()), ("the root read", w.vault_a.root(8).to_vec())]
    {
        assert!(occurs(&bytes, &needle), "{what} is public");
    }
    println!(
        "private claim: of its {} transaction bytes, none holds the destination, s, C, the leaf, the depositor, the witness or \
         any of {} path siblings; N, configId and the root read are present",
        bytes.len(),
        secret.len() - 6
    );
    report("private claim", &claim, &bytes);
    if transfer {
        let path = settle_from.as_ref().map(|(_, second)| second.as_str());
        return private_transfer(&mut w, &account, &claimed, path, transfer_out.as_deref(), shape);
    }

    // --- The private burn of the claimed 1 ETH -----------------------------------------------
    let keys = BridgeKeys::from_bridge_seed(&[0x77; 32]);
    let note = wallet_note(&keys, &cid, &vault, 5, ETH, DENOMINATIONS[1]);
    let burn_message = BridgeMessage::burn(cid, &note);
    let record = burn_actor(program, &cid, &note.r);
    let holder = PrivateAccount { keys: private_keys(seed), account: claimed.clone() };
    let burn = match &settle_from {
        Some((_, path)) => settle(&mut w, path, "private burn"),
        None => {
            let publics = w.burn_actors(me, &cid, &note)[1..].to_vec();
            prove(&mut w, "private burn", &holder, me, &burn_message, publics, burn_out.as_deref(), shape)
        }
    };
    assert!(burn.charge.revert.is_none(), "the burn's public part reverted: {:?}", burn.charge.revert);
    let outgoing: Vec<_> = burn
        .tx
        .message
        .execution
        .boundary
        .iter()
        .filter_map(|step| match step {
            BoundaryStep::PrivateToPublic(d) => Some((d.envelope.to, d.envelope.from, d.envelope.message.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(outgoing.len(), 2, "the burn's delivery and the fee: {outgoing:?}");
    assert_eq!(
        (outgoing[0].0, &outgoing[0].2),
        (token_actor(program, &cid, &ETH), &borsh::to_vec(&burn_message).unwrap()),
        "the note, in the clear"
    );
    assert_ne!(outgoing[0].1.account_id, account.id(), "the holder appears as an alias");
    assert_eq!(
        borsh::from_slice::<BridgeState>(&w.chain.actor_state(record)).unwrap(),
        BridgeState::Burn(BurnRecord { config_id: cid, note }),
        "the note is recorded once"
    );
    private_post_state(&w.chain, &private_keys(seed), &claimed, RESERVE, &HolderState::default())
        .expect("the chain holds the holder's commitment without the 1 ETH");
    let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(burn.tx.clone())).unwrap();
    println!("private burn: one private-to-public delivery of the 185-byte Burn to the token's account, from an alias");
    report("private burn", &burn, &bytes);
}

/// B2: the private holder's `Transfer` of 0.1 ETH (of its claimed 1 ETH) to a public holder,
/// proven (or settled from `settle_path`) and checked: one private-to-public delivery, the
/// `Transfer` to the token's account from an alias; the recipient's `holder(account)` credited; the
/// holder's 0.9 ETH found only in its new commitment; its account id nowhere in the bytes.
fn private_transfer(
    w: &mut World,
    account: &PrivateAccount,
    claimed: &lee::Account,
    settle_path: Option<&str>,
    out: Option<&str>,
    shape: Shape,
) {
    let program = w.program;
    let cid = w.cid_a;
    let seed = w.private_seed;
    let me = own_actor(program, account.id());
    let recipient = role_account(0x46);
    let tenth = DENOMINATIONS[0];
    let message = BridgeMessage::Transfer { config_id: cid, token: ETH, amount: tenth, to: *recipient.value() };
    let holder = PrivateAccount { keys: private_keys(seed), account: claimed.clone() };
    let run = match settle_path {
        Some(path) => settle(w, path, "private transfer"),
        None => {
            let publics = vec![token_actor(program, &cid, &ETH), harness::bridge::holder_of(program, &cid, recipient)];
            prove(w, "private transfer", &holder, me, &message, publics, out, shape)
        }
    };
    assert!(run.charge.revert.is_none(), "the transfer's public part reverted: {:?}", run.charge.revert);
    let outgoing: Vec<_> = run
        .tx
        .message
        .execution
        .boundary
        .iter()
        .filter_map(|step| match step {
            BoundaryStep::PrivateToPublic(d) => Some((d.envelope.to, d.envelope.from, d.envelope.message.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        (outgoing[0].0, &outgoing[0].2),
        (token_actor(program, &cid, &ETH), &borsh::to_vec(&message).unwrap()),
        "the transfer, in the clear, to the token's account"
    );
    assert_ne!(outgoing[0].1.account_id, account.id(), "the holder appears as an alias");
    assert_eq!(w.balance(recipient, &cid), tenth, "holder(recipient) credited");
    let left = HolderState {
        holdings: vec![Holding { config_id: cid, token: ETH, amount: DENOMINATIONS[1] - tenth }],
        pending: None,
    };
    private_post_state(&w.chain, &private_keys(seed), claimed, RESERVE, &left)
        .expect("the chain holds the holder's commitment with 0.9 ETH");
    let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
    assert!(!occurs(&bytes, account.id().value()), "the private account's id is in the transaction");
    println!(
        "private transfer: {} private-to-public deliveries, the first the {}-byte Transfer to the token's account, from \
         an alias; the private account's id is not in the {} bytes; 0.9 ETH left in its commitment",
        outgoing.len(),
        outgoing[0].2.len(),
        bytes.len()
    );
    report("private transfer", &run, &bytes);
}

fn occurs(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A wallet's `i`-th note under `cid`, for one Ethereum recipient, with D12's `mac`
/// (`harness/tests/bridge_burn.rs` makes it the same way).
fn wallet_note(keys: &BridgeKeys, cid: &Hash, vault: &[u8; 20], i: u64, token: [u8; 20], amount: u128) -> Note {
    let recipient = [0xe1; 20];
    let r = burn_tag(&keys.view, cid, i);
    let s_out = burn_secret(&keys.auth, cid, &r);
    let c_out = burn_commitment(&s_out, &chain_id_u256(CHAIN_ID), vault, &recipient);
    let memo = xor_memo(&keys.view, &r, &recipient);
    let amount = word_of_u128(amount);
    let mac = burn_mac_v2(&keys.view, &r, &memo, &token, &amount, &c_out);
    Note { r, memo, mac, token, amount, c_out }
}

/// Prove (for real unless dev mode) and settle one private transaction of `sender` whose root is
/// `message` at `root`, with `actors` public, in `shape`; optionally write it to `out`.
#[allow(clippy::too_many_arguments)]
fn prove(
    w: &mut World,
    what: &str,
    sender: &PrivateAccount,
    root: Actor,
    message: &BridgeMessage,
    actors: Vec<Actor>,
    out: Option<&str>,
    shape: Shape,
) -> PrivateRun {
    let started = Instant::now();
    let time = w.time;
    let run = w
        .chain
        .prove_and_settle_private_as(
            sender,
            root,
            borsh::to_vec(message).unwrap(),
            actors,
            &catalog(),
            RESERVE,
            time,
            shape,
        )
        .unwrap_or_else(|e| panic!("{what}: {e}"));
    let total = started.elapsed();
    let (label, receipt) = if dev_mode() {
        ("DRY RUN, NOT A PROOF (RISC0_DEV_MODE=1)", "fake receipt")
    } else {
        ("REAL PROOF", "succinct InnerReceipt, borsh")
    };
    println!(
        "{what}: {label}: execute_and_prove {:.1} s (prove + settle {:.1} s); receipt ({receipt}) {} bytes; transaction {} bytes",
        run.prove_time.as_secs_f64(),
        total.as_secs_f64(),
        run.proof_bytes,
        run.tx_bytes
    );
    if let Some(path) = out {
        let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
        fs::write(path, &bytes).unwrap_or_else(|e| panic!("writing {path}: {e}"));
        println!("{what}: wrote the proven transaction to {path} ({} bytes)", bytes.len());
    }
    run
}

/// Settle the transaction in `path` on `w`, as `Chain::prove_and_settle_private` settles the one it
/// proves: the same opening fee state, block and time.
fn settle(w: &mut World, path: &str, what: &str) -> PrivateRun {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    let lee_tx: LeeTransaction = borsh::from_slice(&bytes).expect("a borsh LeeTransaction");
    let LeeTransaction::PrivacyPreserving(tx) = &lee_tx else { panic!("{path} holds a public transaction") };
    let claims_bridge = tx.message.execution.program_image_claims.iter().any(|claim| {
        matches!(claim, ProgramImageClaim::Disclosed { account_id, image_id }
            if *account_id == bridge_account() && *image_id == harness::bridge::BRIDGE_ID)
    });
    assert!(claims_bridge, "{path}: the transaction does not claim this binary's bridge image");
    let view = classify(&lee_tx).expect("classifies");
    let opening = opening_fee_state(&w.chain.state);
    let mut summary = BlockFeeSummary::default();
    let started = Instant::now();
    let (block_id, time) = (w.chain.block_id, w.time);
    let (events, charge) =
        settle_private_transaction(&lee_tx, &view, &mut w.chain.state, &opening, block_id, time, &mut summary)
            .unwrap_or_else(|e| panic!("{what}: settlement refuses {path}: {e}"));
    w.chain.block_id += 1;
    let proof_bytes = tx.witness_set.proof().clone().into_inner().len();
    let verified = if dev_mode() { "RISC0_DEV_MODE on: a fake receipt passes too" } else { "receipt verified" };
    println!(
        "{what}: SETTLED FROM {path} ({verified}): receipt {proof_bytes} bytes, transaction {} bytes, settled in {:.1} s",
        bytes.len(),
        started.elapsed().as_secs_f64()
    );
    PrivateRun { tx: tx.clone(), charge, events, prove_time: Default::default(), tx_bytes: bytes.len(), proof_bytes }
}

/// What LEZ charged, and where the receipt sits in the transaction's bytes (for a one-bit flip).
fn report(what: &str, run: &PrivateRun, bytes: &[u8]) {
    let receipt = run.tx.witness_set.proof().clone().into_inner();
    let at = bytes.windows(receipt.len().min(64)).position(|w| w == &receipt[..receipt.len().min(64)]);
    println!(
        "{what}: settled: execution gas {} (PRIVATE_VERIFY_GAS {} + public part {} cycles), storage gas {}, base fee {}; \
         receipt at bytes {}..{} of {}",
        grouped(run.charge.gas_exec),
        grouped(fee_core::market::PRIVATE_VERIFY_GAS),
        grouped(run.charge.cycles),
        grouped(run.charge.gas_stor),
        grouped(run.charge.fee_base as u64),
        at.map_or("?".into(), |a| grouped(a as u64)),
        at.map_or("?".into(), |a| grouped((a + receipt.len()) as u64)),
        grouped(bytes.len() as u64)
    );
}
