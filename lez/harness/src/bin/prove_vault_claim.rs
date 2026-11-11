//! A real proof of the reference consumer's private claim on real data (`tests/vault_consumer.rs`):
//! deposit 0 of FerrataVault v5 on Sepolia (block 12,111,477), claimed at a private account under
//! the record of block 12,112,773 that the instance wrote, one EIP-2935 hop back, with the
//! depositor's secret. The consumer's two transitions, the fee transfer and LEZ's
//! privacy-preserving circuit over them are proven by `r0vm` (`RISC0_DEV_MODE` unset or 0), and the
//! transaction is settled through LEZ's private settlement with the receipt verified.
//!
//! * `prove_vault_claim`: prove, then settle. `--allow-dev-mode`: a fake receipt, to try it.

use std::time::Instant;

use ferrata_lez_wire::consumer::{
    claim_actor, holder_actor, secret_nullifier, ConsumerMessage, ConsumerState, NullifierState,
};
use harness::{
    attest::consumer_state,
    consumer_account, grouped, image_id_hex,
    private::*,
    vault::{crown_openings, eth_pool, RealVault},
};
use lee::Actor;

fn main() {
    let allow_dev = std::env::args().any(|a| a == "--allow-dev-mode");
    let dev = std::env::var("RISC0_DEV_MODE").is_ok_and(|v| !v.is_empty() && v != "0" && v != "false");
    if dev && !allow_dev {
        eprintln!("RISC0_DEV_MODE is set: the receipt would be fake. Unset it, or pass --allow-dev-mode.");
        std::process::exit(2);
    }
    println!(
        "consumer image {}; program image {}; mode: {}",
        image_id_hex(methods::FERRATA_CONSUMER_ID),
        image_id_hex(methods::FERRATA_LEZ_ID),
        if dev { "DEV MODE: fake receipt, not a proof" } else { "real proof (r0vm)" }
    );
    if std::env::args().any(|a| a == "--image-ids") {
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
    // `--deposit 1`: the private-kind deposit, claimed by its destination (LEZ's testnet private
    // account 0); otherwise deposit 0, public-kind, claimed by a fresh private account.
    let i = match std::env::args().position(|a| a == "--deposit") {
        Some(at) => std::env::args().nth(at + 1).and_then(|n| n.parse().ok()).expect("--deposit 0|1"),
        None => 0usize,
    };
    let mut v = RealVault::load();
    let (s, kind, dest) = crown_openings(&v)[i];
    let pool = eth_pool(&v);
    let n0 = secret_nullifier(&s, &v.deposit_id(i));
    let account = if i == 1 {
        v.w.chain.add_private_account_with_keys(harness::vault::lez_testnet_private_account_0(), 1_000_000_000_000)
    } else {
        v.w.chain.add_private_account(0x92, 1_000_000_000_000)
    };
    let message = ConsumerMessage::ClaimDepositPrivately {
        pool: pool.clone(),
        slot: v.slot,
        deposit: v.deposit_proof(i),
        s,
        kind,
        dest,
    };
    let program = consumer_account();
    let mut publics = vec![v.actor(), claim_actor(program, &pool.id(), &n0)];
    if i == 0 {
        publics.push(holder_actor(program, &pool.id(), &dest));
    }
    println!("record: finalized slot {}, block {}; deposit {i} in block {}", v.slot, v.block, v.deposits[i].block);
    let started = Instant::now();
    let run =
        v.w.chain
            .prove_and_settle_private(
                &account,
                Actor::new(account.id(), program),
                borsh::to_vec(&message).unwrap(),
                publics,
                &catalog(),
                100_000_000,
                v.time,
            )
            .expect("proves and settles");
    let total = started.elapsed();
    let state = consumer_state(&v.w.chain, claim_actor(program, &pool.id(), &n0));
    assert_eq!(state, Some(ConsumerState::Nullifier(NullifierState::Spent { pool_id: pool.id() })), "N is spent");
    assert!(run.charge.revert.is_none(), "{:?}", run.charge.revert);
    println!(
        "{}: execute_and_prove {:.1} s (prove + settle {:.1} s); receipt {} bytes; transaction {} bytes",
        if dev { "DRY RUN, NOT A PROOF" } else { "REAL PROOF, receipt verified at settlement" },
        run.prove_time.as_secs_f64(),
        total.as_secs_f64(),
        grouped(run.proof_bytes as u64),
        grouped(run.tx_bytes as u64)
    );
    println!("{}", describe_public_outputs(&run.tx));
    println!(
        "settled: execution gas {}, storage gas {}, base fee {}",
        grouped(run.charge.gas_exec),
        grouped(run.charge.gas_stor),
        grouped(run.charge.fee_base as u64)
    );
}
