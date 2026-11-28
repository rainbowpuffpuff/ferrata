//! The client's private transactions padded as LEZ v0.4.0-rc1's wallet pads every private
//! transaction (`harness::private::Shape::Wallet`): the private attestation (`prove_attest`), the
//! private claim and the private burn (`prove_bridge`), as the binaries that are measured build
//! them (`--shape wallet`, their default), have the public shape of the wallet's own cheapest
//! private transaction (`prove_native_transfer --shape wallet`, built separately, as
//! `lez/wallet/src/lib.rs` `send_proven` builds it), and LEZ's settlement accepts each.
//!
//! Dev mode (`RISC0_DEV_MODE=1`, fake receipts): each binary runs the executor over every session,
//! settles the transaction it proved through LEZ's private settlement, and the transaction is then
//! settled again from its file on a rebuilt state (`--settle`). The real proofs, with their
//! receipts verified by the same settlement, are `lez/results/p4-minimum/` ("padded").

use std::{path::PathBuf, process::Command};

use harness::{
    compat,
    private::{CIPHERTEXT_PAD_SIZE, PADDED_PRIVATE_ACTIONS},
    settle::LeeTransaction,
};

/// What a private transaction shows of its private part: the counts and byte sizes that padding
/// is there to make equal across every transaction a wallet sends. The boundary messages, the
/// program image claims and the public context are what the transaction does (a transfer, a
/// claim, a burn), and differ between any two transactions of different programs.
#[derive(Debug, PartialEq, Eq)]
struct PublicShape {
    private_actions: usize,
    /// borsh size of each private action, sorted.
    action_bytes: Vec<usize>,
    /// borsh size of each note's ciphertext and of its epk, sorted.
    ciphertext_bytes: Vec<usize>,
    epk_bytes: Vec<usize>,
    recovery_bindings: Vec<usize>,
    async_messages: usize,
    /// The refund goes to a named account (`Some`) or to the payer's alias (`None`).
    refund_named: bool,
    /// borsh size of the fee's fallback note, if any.
    fee_fallback_bytes: Option<usize>,
    public_root: bool,
    nonces: usize,
}

fn len<T: borsh::BorshSerialize>(value: &T) -> usize {
    borsh::to_vec(value).unwrap().len()
}

fn shape_of(path: &PathBuf) -> PublicShape {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    let LeeTransaction::PrivacyPreserving(tx) = borsh::from_slice::<LeeTransaction>(&bytes).expect("a LeeTransaction")
    else {
        panic!("{} holds a public transaction", path.display())
    };
    let e = &tx.message.execution;
    let sorted = |mut v: Vec<usize>| {
        v.sort();
        v
    };
    PublicShape {
        private_actions: e.private_actions.len(),
        action_bytes: sorted(e.private_actions.iter().map(len).collect()),
        ciphertext_bytes: sorted(e.private_actions.iter().map(|a| len(&a.encrypted_post_state.ciphertext)).collect()),
        epk_bytes: sorted(e.private_actions.iter().map(|a| len(&a.encrypted_post_state.epk)).collect()),
        recovery_bindings: sorted(e.recovery_bindings.iter().map(len).collect()),
        async_messages: e.async_messages.len(),
        refund_named: compat::proven_fee_details(e).expect("a fee").refund_to.is_some(),
        fee_fallback_bytes: compat::proven_fee_details(e).expect("a fee").fallback.map(|fallback| fallback.encoded_len),
        public_root: e.public_root.is_some(),
        nonces: tx.message.nonces.len(),
    }
}

/// Run a harness binary in dev mode; it must succeed. Returns its standard output.
fn run(bin: &str, args: &[&str]) -> String {
    let out = Command::new(bin)
        .args(args)
        .env("RISC0_DEV_MODE", "1")
        .env_remove("FERRATA_EXPECT_CONSUMER_IMAGE_ID")
        .env_remove("FERRATA_EXPECT_BRIDGE_IMAGE_ID")
        .output()
        .unwrap_or_else(|e| panic!("starting {bin}: {e}"));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{bin} {args:?} failed ({}):\n{stdout}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

#[test]
fn padded_attestation_claim_and_burn_have_the_wallets_public_shape_and_settle() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wallet_padding");
    std::fs::create_dir_all(&dir).unwrap();
    let p = |name: &str| dir.join(name);
    let s = |path: &PathBuf| path.to_str().unwrap().to_string();
    let (wallet, attest, claim, burn, bare) =
        (p("wallet.borsh"), p("attest.borsh"), p("claim.borsh"), p("burn.borsh"), p("attest-bare.borsh"));

    // LEZ's wallet's own transaction, built as `send_proven` builds it.
    let transfer = env!("CARGO_BIN_EXE_prove_native_transfer");
    run(transfer, &["--allow-dev-mode", "--shape", "wallet", "--tx-out", &s(&wallet)]);
    run(transfer, &["--settle", &s(&wallet)]);

    // Ours, as the measured binaries build them by default (padded), each settled when proven
    // and again from its file.
    let attest_bin = env!("CARGO_BIN_EXE_prove_attest");
    let out = run(attest_bin, &["--allow-dev-mode", "--tx-out", &s(&attest)]);
    assert!(out.contains("shape wallet"), "prove_attest pads by default:\n{out}");
    let settled = run(attest_bin, &["--settle", &s(&attest)]);
    assert!(settled.contains("SETTLED FROM"), "{settled}");
    let bridge_bin = env!("CARGO_BIN_EXE_prove_bridge");
    let out = run(bridge_bin, &["--allow-dev-mode", "--claim-out", &s(&claim), "--burn-out", &s(&burn)]);
    assert!(out.contains("shape wallet"), "prove_bridge pads by default:\n{out}");
    let settled = run(bridge_bin, &["--settle", &s(&claim), &s(&burn)]);
    assert_eq!(settled.matches("SETTLED FROM").count(), 2, "{settled}");
    // The bare shape, for contrast: what the binaries built before.
    run(attest_bin, &["--allow-dev-mode", "--shape", "bare", "--tx-out", &s(&bare)]);

    let reference = shape_of(&wallet);
    println!("the wallet's transfer: {reference:?}");
    assert_eq!(reference.private_actions, PADDED_PRIVATE_ACTIONS, "the wallet pads to seven private actions");
    assert!(
        reference.ciphertext_bytes.windows(2).all(|w| w[0] == w[1]),
        "the wallet's notes are all one size: {reference:?}"
    );
    assert!(reference.ciphertext_bytes[0] >= CIPHERTEXT_PAD_SIZE as usize, "{reference:?}");
    assert_eq!(reference.recovery_bindings.len(), 1, "the fee's recovery note");
    assert!(!reference.refund_named, "the refund goes to the payer's alias");
    for (what, path) in [("private attestation", &attest), ("private claim", &claim), ("private burn", &burn)] {
        let shape = shape_of(path);
        println!("{what}: {shape:?}");
        assert_eq!(shape, reference, "the {what} has the wallet's public shape");
    }
    let unpadded = shape_of(&bare);
    println!("private attestation, bare: {unpadded:?}");
    assert_eq!(unpadded.private_actions, 1);
    assert_ne!(unpadded, reference, "the bare shape is told apart");
}
