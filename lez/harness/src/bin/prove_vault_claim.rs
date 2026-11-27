//! A real proof of the reference consumer's private claim on real data (`tests/vault_consumer.rs`):
//! deposit 0 of FerrataVault v5 on Sepolia (block 12,111,477), claimed at a private account under
//! the record of block 12,112,773 that the instance wrote, one EIP-2935 hop back, with the
//! depositor's secret. The consumer's two transitions, the fee transfer and LEZ's
//! privacy-preserving circuit over them are proven by `r0vm` (`RISC0_DEV_MODE` unset or 0), and the
//! transaction is settled through LEZ's private settlement with the receipt verified.
//!
//! * `prove_vault_claim [--deposit 0|1] [--hops K] [--shape bare|wallet] [--tx-out FILE]`: prove,
//!   then settle. `--allow-dev-mode`: a fake receipt, to try it.
//! * `--hops K` (1 to 5, default 1): the deposit's block reached through a chain of `K` EIP-2935
//!   hops inside the claimant's proof (`harness::hops::claim_hops`: the vault fixture's own hop,
//!   then the fixtures `sepolia-hops-2026-11-18` and `sepolia-hops-2026-11-27`), as a private
//!   consumer that reads a recent record does for an older fact (RFP-021 P.4: the user-side limit is
//!   5 hops, `docs/performance.md`).
//! * `--shape wallet` pads the transaction as LEZ v0.4.0-rc1's wallet pads every private
//!   transaction ([`harness::private::Shape::Wallet`]); `bare` (the default) builds it as this
//!   binary did before 27 November 2026.
//! * `--tx-out FILE`: also write the proven transaction (borsh `LeeTransaction`, the receipt
//!   included) to FILE. `--settle FILE`: settle such a transaction without proving: the same state
//!   is rebuilt from the fixtures, the transaction must claim this binary's consumer image, and LEZ's
//!   settlement verifies its receipt (a fake one only passes in dev mode).
//! * `--image-ids`: print the image ids this binary holds, and stop.
//!
//! `FERRATA_EXPECT_CONSUMER_IMAGE_ID=<hex>` stops it before any work unless the consumer image
//! compiled into this binary is that one. `FERRATA_VAULT_FIXTURE_DIR`, `FERRATA_SEPOLIA_FIXTURE_DIR`
//! and `FERRATA_HOPS_FIXTURES_DIR` say where the fixtures are read when the binary runs elsewhere;
//! their SHA-256 sums are compiled in.

use std::{fs, time::Instant};

use fee_core::BlockFeeSummary;
use ferrata_lez_wire::consumer::{
    claim_actor, holder_actor, secret_nullifier, ConsumerMessage, ConsumerState, NullifierState,
};
use harness::{
    attest::{consumer_state, execute, render_executed},
    consumer_account, consumer_program, grouped,
    hops::MAX_USER_HOPS,
    image_id_hex,
    private::*,
    settle::{classify, opening_fee_state, settle_private_transaction, LeeTransaction},
    vault::{crown_openings, eth_pool, RealVault},
};
use lee::Actor;
use lee_core::{
    program::{ReceiveInput, StateReply},
    ProgramImageClaim,
};

fn usage() -> ! {
    eprintln!(
        "usage: prove_vault_claim [--deposit 0|1] [--hops 1..={MAX_USER_HOPS}] [--shape bare|wallet] [--tx-out FILE] \
         [--allow-dev-mode] | --settle FILE [--deposit 0|1] | --image-ids"
    );
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut allow_dev, mut only_ids, mut i, mut hops, mut shape) = (false, false, 0usize, 1usize, Shape::Bare);
    let (mut tx_out, mut settle_from) = (None::<String>, None::<String>);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--allow-dev-mode" => allow_dev = true,
            "--image-ids" => only_ids = true,
            "--deposit" => i = it.next().and_then(|n| n.parse().ok()).filter(|n| *n <= 1).unwrap_or_else(|| usage()),
            "--hops" => {
                hops = it
                    .next()
                    .and_then(|n| n.parse().ok())
                    .filter(|k| (1..=MAX_USER_HOPS).contains(k))
                    .unwrap_or_else(|| usage())
            }
            "--shape" => shape = it.next().and_then(|s| Shape::parse(s)).unwrap_or_else(|| usage()),
            "--tx-out" => tx_out = Some(it.next().unwrap_or_else(|| usage()).clone()),
            "--settle" => settle_from = Some(it.next().unwrap_or_else(|| usage()).clone()),
            _ => usage(),
        }
    }
    let dev = std::env::var("RISC0_DEV_MODE").is_ok_and(|v| !v.is_empty() && v != "0" && v != "false");
    if dev && !allow_dev && settle_from.is_none() && !only_ids {
        eprintln!("RISC0_DEV_MODE is set: the receipt would be fake. Unset it, or pass --allow-dev-mode.");
        std::process::exit(2);
    }
    let consumer_image = image_id_hex(methods::FERRATA_CONSUMER_ID);
    println!(
        "consumer image {consumer_image}; program image {}; mode: {}; hops {hops}; shape {}",
        image_id_hex(methods::FERRATA_LEZ_ID),
        if dev { "DEV MODE: fake receipt, not a proof" } else { "real proof (r0vm)" },
        shape.name()
    );
    if only_ids {
        for (name, id, elf) in [
            ("program (guests/program)", methods::FERRATA_LEZ_ID, methods::FERRATA_LEZ_ELF.len()),
            ("consumer (guests/consumer)", methods::FERRATA_CONSUMER_ID, methods::FERRATA_CONSUMER_ELF.len()),
            ("bridge (guests/bridge)", methods::FERRATA_BRIDGE_ID, methods::FERRATA_BRIDGE_ELF.len()),
            ("holdings (guests/holdings)", methods::FERRATA_HOLDINGS_ID, methods::FERRATA_HOLDINGS_ELF.len()),
            ("scripted (guests/scripted)", methods::SCRIPTED_ID, methods::SCRIPTED_ELF.len()),
            (
                "program, software Keccak",
                methods::FERRATA_LEZ_KECCAK_SOFT_ID,
                methods::FERRATA_LEZ_KECCAK_SOFT_ELF.len(),
            ),
            (
                "consumer, accelerated Keccak",
                methods::FERRATA_CONSUMER_KECCAK_ACCEL_ID,
                methods::FERRATA_CONSUMER_KECCAK_ACCEL_ELF.len(),
            ),
        ] {
            println!("| {name} | `{}` | {elf} |", image_id_hex(id));
        }
        return;
    }
    if let Ok(expected) = std::env::var("FERRATA_EXPECT_CONSUMER_IMAGE_ID") {
        assert_eq!(
            consumer_image,
            expected.trim().trim_start_matches("0x").to_lowercase(),
            "this binary holds another consumer image"
        );
    }

    // `--deposit 1`: the private-kind deposit, claimed by its destination (LEZ's testnet private
    // account 0); otherwise deposit 0, public-kind, claimed by a fresh private account. The state is
    // deterministic, so `--settle` rebuilds the one a proof was made against.
    let mut v = RealVault::load();
    let (s, kind, dest) = crown_openings(&v)[i];
    let pool = eth_pool(&v);
    let n0 = secret_nullifier(&s, &v.deposit_id(i));
    let account = if i == 1 {
        v.w.chain.add_private_account_with_keys(harness::vault::lez_testnet_private_account_0(), 1_000_000_000_000)
    } else {
        v.w.chain.add_private_account(0x92, 1_000_000_000_000)
    };
    let deposit = v.deposit_proof_hops(i, hops);
    assert_eq!(deposit.hops.len(), hops);
    let message = ConsumerMessage::ClaimDepositPrivately { pool: pool.clone(), slot: v.slot, deposit, s, kind, dest };
    let program = consumer_account();
    let mut publics = vec![v.actor(), claim_actor(program, &pool.id(), &n0)];
    if i == 0 {
        publics.push(holder_actor(program, &pool.id(), &dest));
    }
    println!(
        "record: finalized slot {}, block {}; deposit {i} in block {} ({} blocks back), reached through {hops} hop(s)",
        v.slot,
        v.block,
        v.deposits[i].block,
        v.block - v.deposits[i].block
    );

    let run = match &settle_from {
        Some(path) => settle(&mut v, path),
        None => {
            // The consumer's two private transitions, executed: what the prover proves for it.
            let root = Actor::new(account.id(), program);
            let first = execute(
                "ClaimDepositPrivately (sends Read)",
                consumer_program().elf(),
                &ReceiveInput {
                    receiver: root,
                    from: None,
                    is_authorized: true,
                    pre_state: Default::default(),
                    message: borsh::to_vec(&message).unwrap(),
                },
            );
            let transition = first.transition.as_ref().expect("the root call passes");
            let reply = execute(
                "the record's reply (verifies the deposit, sends Spend)",
                consumer_program().elf(),
                &ReceiveInput {
                    receiver: root,
                    from: Some(v.actor()),
                    is_authorized: true,
                    pre_state: transition.response.post_state.clone().expect("the claim is kept for the reply"),
                    message: borsh::to_vec(&StateReply { state: v.w.chain.actor_state(v.actor()) }).unwrap(),
                },
            );
            assert_eq!(reply.exit_code, 0, "the reply verifies the deposit");
            println!("the consumer's private transitions (executor):\n{}", render_executed(&[first, reply]));
            let started = Instant::now();
            let run =
                v.w.chain
                    .prove_and_settle_private_as(
                        &account,
                        root,
                        borsh::to_vec(&message).unwrap(),
                        publics,
                        &catalog(),
                        100_000_000,
                        v.time,
                        shape,
                    )
                    .expect("proves and settles");
            let total = started.elapsed();
            println!(
                "{}: execute_and_prove {:.1} s (prove + settle {:.1} s); receipt {} bytes; transaction {} bytes",
                if dev { "DRY RUN, NOT A PROOF" } else { "REAL PROOF, receipt verified at settlement" },
                run.prove_time.as_secs_f64(),
                total.as_secs_f64(),
                grouped(run.proof_bytes as u64),
                grouped(run.tx_bytes as u64)
            );
            if let Some(path) = &tx_out {
                let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
                fs::write(path, &bytes).unwrap_or_else(|e| panic!("writing {path}: {e}"));
                println!("wrote the proven transaction to {path} ({} bytes)", bytes.len());
            }
            run
        }
    };
    let state = consumer_state(&v.w.chain, claim_actor(program, &pool.id(), &n0));
    assert_eq!(state, Some(ConsumerState::Nullifier(NullifierState::Spent { pool_id: pool.id() })), "N is spent");
    assert!(run.charge.revert.is_none(), "{:?}", run.charge.revert);
    println!("{}", describe_public_outputs(&run.tx));
    println!(
        "settled: execution gas {}, storage gas {}, base fee {}",
        grouped(run.charge.gas_exec),
        grouped(run.charge.gas_stor),
        grouped(run.charge.fee_base as u64)
    );
}

/// Settle the transaction in `path` on `v`, as `Chain::prove_and_settle_private` settles the one it
/// proves: the same opening fee state, block and time.
fn settle(v: &mut RealVault, path: &str) -> PrivateRun {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    let lee_tx: LeeTransaction = borsh::from_slice(&bytes).expect("a borsh LeeTransaction");
    let LeeTransaction::PrivacyPreserving(tx) = &lee_tx else { panic!("{path} holds a public transaction") };
    let claims_consumer = tx.message.execution.program_image_claims.iter().any(|claim| {
        matches!(claim, ProgramImageClaim::Disclosed { account_id, image_id }
            if *account_id == consumer_account() && *image_id == methods::FERRATA_CONSUMER_ID)
    });
    assert!(claims_consumer, "the transaction does not claim this binary's consumer image");
    let view = classify(&lee_tx).expect("classifies");
    let opening = opening_fee_state(&v.w.chain.state);
    let mut summary = BlockFeeSummary::default();
    let started = Instant::now();
    let block_id = v.w.chain.block_id;
    let (events, charge) =
        settle_private_transaction(&lee_tx, &view, &mut v.w.chain.state, &opening, block_id, v.time, &mut summary)
            .expect("settlement accepts the transaction");
    v.w.chain.block_id += 1;
    let proof_bytes = tx.witness_set.proof().clone().into_inner().len();
    let verified = if matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true")) {
        "RISC0_DEV_MODE on: a fake receipt passes too"
    } else {
        "receipt verified"
    };
    println!(
        "SETTLED FROM {path} ({verified}): receipt {proof_bytes} bytes, transaction {} bytes, settled in {:.1} s",
        bytes.len(),
        started.elapsed().as_secs_f64()
    );
    let tx: lee::PrivacyPreservingTransaction = tx.clone();
    PrivateRun { tx, charge, events, prove_time: Default::default(), tx_bytes: bytes.len(), proof_bytes }
}
