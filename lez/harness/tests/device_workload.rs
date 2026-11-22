//! The work the proofs of RFP-021 Performance #4 and RFP-022 Performance #4 measured is still the
//! work the code asks a user's device to prove.
//!
//! "Any proving the user's own device must perform has to be practical on the desktop hardware
//! Basecamp runs on. Measure and document wall-clock time and peak memory on a mid-range laptop
//! and on the lowest specification the team declares as supported, and state that minimum
//! explicitly." The measurements are committed: `lez/results/p4-minimum/README.md` (cloud
//! containers held to the declared minimum, 8 CPUs and its 16 threads, 16 GiB) and
//! `lez/results/p4-macos/README.md` (an M1 and an M4 Mac mini) — real proofs, wall clock and peak
//! memory; nothing CI can redo, because dev mode (RISC0_DEV_MODE=1, which these tests require)
//! executes without proving. What CI can hold is the half of the clause a dev-mode run can see:
//! that the work those proofs measured is still the work the code builds and runs — the same
//! transactions, the same executor sessions, the same sizes — so that a change which would
//! invalidate the record fails here, and the measurements have to be redone.
//!
//! What is checked against the record, and where the record states it:
//!
//! * **The programs' private transitions.** The prover proves each of them from a
//!   `ReceiveInput` written with `lee::program::Program::write_receive_input` over the program ELF
//!   (`prove_session`, `lee/state_machine/src/privacy_preserving_transaction/circuit/mod.rs` lines
//!   674-703 of LEZ v0.4.0-rc1, `1806e87`); `risc0_zkvm::default_executor()` runs that same session
//!   on that same input, pinned here to the same 3.0.5 as the record's `r0vm`
//!   (`lez/harness/Cargo.toml`), and the record's numbers already agree between the two ("the
//!   same here, 34,337 + 346,240", p4-minimum:77-79; the r0vm logs of the real 8-CPU proofs,
//!   p4-minimum:67-74, of the 16-CPU pass, 117-122, of the Macs, p4-macos:51-58). The programs'
//!   transitions do not vary between runs (p4-minimum:31-33) and are unchanged by the wallet's
//!   padding (p4-minimum:349-351). The segment count and the segment sizes — the work a prover
//!   does, what sets its time and memory — are asserted equal to the record's. The user cycles are
//!   asserted exactly against this tree's pins: the programs changed after the record's runs, by
//!   -652 to +625 cycles per transition (build box, 22 November 2026, the bridge program since
//!   milestone B1), inside the same segments.
//! * **The transaction the device proves, at its recorded size.** LEZ's circuit publishes the
//!   transaction's public part; the record's "transaction (B)" column is the borsh
//!   `LeeTransaction` with the real receipt inside. Only the receipt is receipt-dependent (and a
//!   dev-mode run's receipt is fake and shorter), so the receipt vector is padded out to the
//!   record's recorded receipt size and every other byte of the size must match: unpadded
//!   p4-minimum:23-25, padded 337-339 (the Macs measured the same sizes, p4-macos:26-27). The
//!   claim and the burn match to the byte; the attestation is 32 bytes larger than the record's
//!   runs measured it, bare and padded, and is pinned at this tree's size with the record's beside
//!   it. One fresh private account per shape: the first shape's note does not spend twice.
//! * **The padding the wallet's shape adds, at its recorded shape and sizes**: seven private
//!   actions of 1,704 bytes each, 516-byte note ciphertexts over 1,092-byte epks, one 1,258-byte
//!   recovery note, the fee's 1,868-byte fallback note, the refund to the payer's alias — the
//!   public shape of LEZ's own wallet's transactions and of ours padded
//!   (p4-minimum:323-324) — and, for contrast, the bare attestation's 1,485-byte action,
//!   297-byte ciphertext and named refund (p4-minimum:325).
//!
//! **Not checkable in dev mode: LEZ's circuit's own session**, 85-96 % of the proving time
//! (p4-minimum:76-77). LEZ's `execute_and_prove` ends in the private `prove`
//! (`lee/state_machine/src/privacy_preserving_transaction/circuit/mod.rs` lines 559-672), which
//! runs the circuit through `default_prover().prove_with_opts` (lines 656-658) and keeps only
//! `prove_info.receipt.inner` (line 660): the `SessionInfo` of `ProveInfo::stats` — user cycles,
//! segments, their po2 — is discarded there, and per transition, in `prove_session`, it is read
//! only to gate the exit code (line 699). No public API of LEZ v0.4.0-rc1 reports it, and
//! rebuilding the circuit's `PrivacyPreservingCircuitInput` here would re-implement LEZ's private
//! simulation and proving backend, checking a copy of the code instead of the code. Those rows
//! stay recorded facts, printed beside the checked ones so each printed proof is the whole one:
//! the circuit at 1,975,088-1,975,089 user cycles in 3 segments for the bare attestation
//! (p4-minimum:69) and the record's other rows. The circuit varies by up to 107 cycles between
//! runs of one transaction — its dummy notes are random — and the wallet's transfer by 247
//! (p4-minimum:32-33, 107); the programs' transitions do not (p4-minimum:32-33). Only a real
//! proof measures those again; `lez/results/p4-minimum/` is how.

use ferrata_bridge_core::{
    chain_id_u256,
    outbound::{burn_commitment, burn_secret, burn_tag, xor_memo},
    BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{burn_mac_v2, own_actor, word_of_u128, BridgeMessage, HolderState, Holding, Note};
use harness::{
    attest::{Executed, Stage4},
    bridge::{
        catalog as bridge_catalog, private_burn_transition, private_claim_transitions, private_keys,
        private_post_state, World, DENOMINATIONS, ETH,
    },
    consumer_account, grouped,
    private::{self, PrivateAccount, PrivateRun, Shape},
    settle::LeeTransaction,
};
use lee::{
    privacy_preserving_transaction::{circuit::Proof, WitnessSet},
    Actor, PrivacyPreservingTransaction,
};

/// Every one of these private transactions' fee reserve, as `prove_attest` and `prove_bridge`
/// reserve it (`lez/harness/src/bin/prove_attest.rs`, `prove_bridge.rs`).
const RESERVE: u128 = 100_000_000;

// ---------------------------------------------------------------------------------------------
// The record's rows. Every cite is a line of a committed record: "p4-minimum:N" is
// `lez/results/p4-minimum/README.md` line N; "p4-macos:N" is `lez/results/p4-macos/README.md`
// line N.
// ---------------------------------------------------------------------------------------------

/// The programs' private transitions the real proofs proved, per proof in their order: user
/// cycles, segments, cycles with padding (each segment's `2^po2`). The record states them from
/// the r0vm logs of the 8-CPU proofs (p4-minimum:67-74), of the 16-CPU pass (117-122) and of the
/// Macs' runs (p4-macos:51-58); they do not vary between runs (p4-minimum:32-33) and are
/// unchanged by the wallet's padding (p4-minimum:349-351). The segments and their padded sizes,
/// what a prover proves and what sets its time and memory, are asserted equal to the record's.
const ATTEST_TRANSITIONS: [(u64, usize, u64); 2] = [(34_337, 1, 131_072), (346_240, 1, 524_288)];
const CLAIM_TRANSITIONS: [(u64, usize, u64); 2] = [(41_160, 1, 131_072), (111_596, 1, 262_144)];
const BURN_TRANSITION: (u64, usize, u64) = (25_377, 1, 131_072);

/// The same transitions' user cycles as this tree's programs execute them (build box, 22 November
/// 2026). The programs changed after the record's runs by a few hundred cycles per transition, inside
/// the same segments and padded sizes; these exact pins catch the next change, which must then be
/// checked against the record's segments again (the asserts below) or the measurements redone.
const ATTEST_TRANSITIONS_NOW: [u64; 2] = [34_717, 346_763];
const CLAIM_TRANSITIONS_NOW: [u64; 2] = [41_785, 110_944];
const BURN_TRANSITION_NOW: u64 = 25_773;

/// Each proof's receipt and transaction sizes as the record states them, `(receipt bytes,
/// transaction bytes)`: the unpadded first pass (p4-minimum:23-25) and the padded pass (337-339;
/// the Macs measured the same sizes, p4-macos:26-27).
const ATTEST_BARE: (usize, usize) = (227_170, 231_214);
const ATTEST_PADDED: (usize, usize) = (239_214, 255_302);
const CLAIM_BARE: (usize, usize) = (227_538, 231_950);
const CLAIM_PADDED: (usize, usize) = (239_628, 256_130);
const BURN_BARE: (usize, usize) = (226_998, 230_870);
const BURN_PADDED: (usize, usize) = (239_046, 254_966);

/// This tree's private attestation transaction, at the same receipt sizes: 32 bytes larger than the
/// record's runs measured it, bare and padded (build box, 22 November 2026); the claim's and the
/// burn's are the record's to the byte. The record's proving times and memory are of the smaller
/// transaction: LEZ's circuit, which dominates them, reads the message, and its cycles cannot be
/// measured in dev mode (module header), so this difference is reported, not hidden.
const ATTEST_BARE_NOW: usize = 231_246;
const ATTEST_PADDED_NOW: usize = 255_334;

/// The private part's public shape, at the sizes the record's padded-section table states
/// (p4-minimum:323-325): the same fields `tests/wallet_padding.rs` compares.
struct ShapeRow {
    actions: usize,
    /// Each private action's borsh size.
    action_bytes: usize,
    /// Each note's borsh ciphertext size.
    ciphertext: usize,
    /// Each note's borsh epk size.
    epk: usize,
    /// The recovery notes: their count and each binding's borsh size.
    recovery: (usize, usize),
    /// The fee's fallback note's borsh size.
    fallback: usize,
    /// The refund goes to a named account (the public payer) or to the payer's alias.
    refund_named: bool,
}

/// LEZ's wallet's own transaction and each of ours padded, field for field (p4-minimum:323-324).
const WALLET_ROW: ShapeRow = ShapeRow {
    actions: 7,
    action_bytes: 1_704,
    ciphertext: 516,
    epk: 1_092,
    recovery: (1, 1_258),
    fallback: 1_868,
    refund_named: false,
};

/// Ours as the binaries built them before the padding: the private attestation (p4-minimum:325).
const BARE_ATTESTATION_ROW: ShapeRow = ShapeRow {
    actions: 1,
    action_bytes: 1_485,
    ciphertext: 297,
    epk: 1_092,
    recovery: (0, 0),
    fallback: 1_493,
    refund_named: true,
};

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

/// The borsh size of `value` (`tests/wallet_padding.rs` measures the same way).
fn len<T: borsh::BorshSerialize>(value: &T) -> usize {
    borsh::to_vec(value).unwrap().len()
}

/// One executor session beside the record's row for it (`record`: user cycles, segments, cycles
/// with padding; `now`: this tree's user cycles). The segments and the cycles with padding must be
/// the record's: they are the work a prover does, so a difference means the p4 measurements no
/// longer describe the proof. The user cycles must be `now` exactly (the transitions do not vary
/// between runs, p4-minimum:32-33). Each row is printed into the proof's table before the checks.
fn check_session(what: &str, executed: &Executed, record: (u64, usize, u64), now: u64, cite: &str) {
    let (cycles, segments, padded) = record;
    println!(
        "| {what} (this tree) | {} | {} | {} | the record: {} / {segments} / {} ({cite}) |",
        grouped(executed.cycles),
        executed.segments,
        grouped(executed.padded_cycles),
        grouped(cycles),
        grouped(padded)
    );
    assert_eq!(executed.exit_code, 0, "{what}: the transition did not verify");
    assert_eq!(
        executed.cycles,
        now,
        "{what}: {} user cycles, this tree's pin is {} (the record measured {}, {cite}): the program \
         changed; check its segments against the record's, then move the pin",
        grouped(executed.cycles),
        grouped(now),
        grouped(cycles)
    );
    assert_eq!(
        executed.segments, segments,
        "{what}: {} segments, the record measured {} ({cite})",
        executed.segments, segments
    );
    assert_eq!(
        executed.padded_cycles,
        padded,
        "{what}: {} cycles with padding, the record measured {} ({cite})",
        grouped(executed.padded_cycles),
        grouped(padded)
    );
}

/// A row of the record that no dev-mode run can measure — LEZ's circuit's session, and the whole
/// proof's totals, which the circuit dominates — printed into the same table beside the checked
/// rows, clearly marked, so the printed proof is the whole one. See the module header for why.
fn record_row(what: &str, user_cycles: &str, segments: &str, padded: &str, cite: &str) {
    println!(
        "| {what} (the record only: dev mode cannot measure it) | {user_cycles} | {segments} | {padded} | ({cite}) |"
    );
}

/// The borsh size of the proven transaction as a `LeeTransaction`, with the proof vector padded
/// out to `receipt` bytes: every receipt-independent byte of the record's transaction — the
/// circuit output LEZ publishes, over the empty signature vector — at one size. The record's
/// "transaction (B)" column is this number with the real receipt inside, so padding a dev-mode
/// proof's (fake, shorter) receipt vector out to the record's receipt size must reproduce it.
fn transaction_bytes(run: &PrivateRun, receipt: usize) -> usize {
    let tx = PrivacyPreservingTransaction::new(
        run.tx.message.clone(),
        WitnessSet::from_raw_parts(Vec::new(), Proof::from_inner(vec![0u8; receipt])),
    );
    borsh::to_vec(&LeeTransaction::PrivacyPreserving(tx)).expect("borsh serialization is infallible").len()
}

/// The transaction the device proves, at its recorded size: `record` is the record's
/// `(receipt bytes, transaction bytes)` pair for this proof and shape.
fn check_size(what: &str, shape: Shape, run: &PrivateRun, record: (usize, usize), now: usize, cite: &str) {
    let (receipt, recorded) = record;
    let bytes = transaction_bytes(run, receipt);
    println!(
        "{what} ({}): transaction {} bytes, the record's {}-byte receipt inside — the record: {} bytes ({cite})",
        shape.name(),
        grouped(bytes as u64),
        grouped(receipt as u64),
        grouped(recorded as u64)
    );
    assert_eq!(
        bytes,
        now,
        "{what} ({}): {} transaction bytes with the record's {receipt}-byte receipt inside, expected {} (the \
         record states {}, {cite}): the transaction changed; compare it with the record, then move the pin",
        shape.name(),
        grouped(bytes as u64),
        grouped(now as u64),
        grouped(recorded as u64)
    );
}

/// The private part's public shape at the sizes the record states: the padding the wallet's shape
/// adds, or (for the attestation) the bare shape's row. The record's own columns
/// (p4-minimum:323-325); the wallet's row is LEZ's own transfer's row, equal to each of ours.
fn check_shape(what: &str, shape: Shape, run: &PrivateRun, row: &ShapeRow, cite: &str) {
    let e = &run.tx.message.execution;
    assert_eq!(
        e.private_actions.len(),
        row.actions,
        "{what} ({}): {} private actions, the record states {} ({cite})",
        shape.name(),
        e.private_actions.len(),
        row.actions
    );
    let actions: Vec<usize> = e.private_actions.iter().map(len).collect();
    assert!(
        actions.iter().all(|&b| b == row.action_bytes),
        "{what} ({}): private actions of {} bytes each, the record states {} ({cite}): {actions:?}",
        shape.name(),
        actions.first().copied().unwrap_or_default(),
        row.action_bytes
    );
    let ciphertexts: Vec<usize> = e.private_actions.iter().map(|a| len(&a.encrypted_post_state.ciphertext)).collect();
    assert!(
        ciphertexts.iter().all(|&b| b == row.ciphertext),
        "{what} ({}): note ciphertexts of {} bytes, the record states {} ({cite}): {ciphertexts:?}",
        shape.name(),
        ciphertexts.first().copied().unwrap_or_default(),
        row.ciphertext
    );
    let epks: Vec<usize> = e.private_actions.iter().map(|a| len(&a.encrypted_post_state.epk)).collect();
    assert!(
        epks.iter().all(|&b| b == row.epk),
        "{what} ({}): note epks of {} bytes, the record states {} ({cite}): {epks:?}",
        shape.name(),
        epks.first().copied().unwrap_or_default(),
        row.epk
    );
    let recoveries: Vec<usize> = e.recovery_bindings.iter().map(len).collect();
    assert_eq!(
        recoveries.len(),
        row.recovery.0,
        "{what} ({}): {} recovery notes, the record states {} ({cite})",
        shape.name(),
        recoveries.len(),
        row.recovery.0
    );
    if row.recovery.0 > 0 {
        assert!(
            recoveries.iter().all(|&b| b == row.recovery.1),
            "{what} ({}): recovery notes of {} bytes, the record states {} ({cite})",
            shape.name(),
            recoveries.first().copied().unwrap_or_default(),
            row.recovery.1
        );
    }
    let fee = e.fee.as_ref().expect("a fee");
    assert_eq!(
        fee.fallback.as_ref().map(len),
        Some(row.fallback),
        "{what} ({}): the fee's fallback note, the record states {} bytes ({cite})",
        shape.name(),
        row.fallback
    );
    assert_eq!(
        fee.refund_to.is_some(),
        row.refund_named,
        "{what} ({}): the refund goes to {} ({cite})",
        shape.name(),
        if row.refund_named { "a named account" } else { "the payer's alias" }
    );
    println!(
        "{what} ({}): {} private actions of {} bytes, {}-byte ciphertexts over {}-byte epks, {} \
         recovery note(s) of {} bytes, the fee's {}-byte fallback note — the record's row ({cite})",
        shape.name(),
        row.actions,
        grouped(row.action_bytes as u64),
        grouped(row.ciphertext as u64),
        grouped(row.epk as u64),
        row.recovery.0,
        grouped(row.recovery.1 as u64),
        grouped(row.fallback as u64)
    );
}

fn print_header(what: &str, cite: &str) {
    println!("# {what} — the record: {cite}");
    println!("| session | user cycles | segments | cycles with padding | the record |");
    println!("|---|---:|---:|---:|---|");
}

/// A wallet's `i`-th burn note under `cid`, for one Ethereum recipient, exactly as `prove_bridge`
/// builds it (`lez/harness/src/bin/prove_bridge.rs`, which cites `tests/bridge_burn.rs`).
fn wallet_note(keys: &BridgeKeys, cid: &Hash, vault: &[u8; 20], i: u64, token: [u8; 20], amount: u128) -> Note {
    let recipient = [0xe1; 20];
    let r = burn_tag(&keys.view, cid, i);
    let s_out = burn_secret(&keys.auth, cid, &r);
    let c_out = burn_commitment(&s_out, &chain_id_u256(harness::bridge::CHAIN_ID), vault, &recipient);
    let memo = xor_memo(&keys.view, &r, &recipient);
    let amount = word_of_u128(amount);
    let mac = burn_mac_v2(&keys.view, &r, &memo, &token, &amount, &c_out);
    Note { r, memo, mac, token, amount, c_out }
}

/// The bridge world exactly as `prove_bridge` builds it before proving anything
/// (`lez/harness/src/bin/prove_bridge.rs`): the paired vaults' configurations and roots imported,
/// the private destination of vault A's leaf 5 funded for the fee, the private claim ready to
/// prove. Deterministic, rebuilt per shape.
struct BridgeRun {
    w: World,
    account: PrivateAccount,
    me: Actor,
    cid: Hash,
    vault: [u8; 20],
    seed: u8,
    message: BridgeMessage,
    actors: Vec<Actor>,
}

fn bridge_run() -> BridgeRun {
    let mut w = World::with_roots();
    let seed = w.private_seed;
    let account = w.chain.add_private_account(seed, 1_000_000_000_000);
    let deposit = w.private_deposit.clone();
    assert_eq!(account.id().value(), &deposit.dest, "the deposit is for this private account");
    let cid = w.cid_a;
    let vault = w.vault_a.address;
    let witness = w.vault_a.witness(&deposit, 8);
    let message = w.claim_privately(&cid, 8, witness.clone());
    let me = own_actor(w.program, account.id());
    let actors = w.private_claim_actors(&cid, 8, &vault, &deposit);
    BridgeRun { w, account, me, cid, vault, seed, message, actors }
}

/// The private attestation of `prove_attest`, in both shapes the record measured (the bare one of
/// the first pass and the 16-CPU pass, the padded one of the padding pass and the Macs): the
/// consumer's two private transitions are the executor sessions the prover proves, exactly at the
/// record's counts, and the transaction each shape builds has the record's size and shape.
#[test]
fn the_private_attestation_proves_the_work_the_record_measured() {
    require_dev_mode();
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let t = s.fx.transfer.clone();
    let pool = s.pool();
    let header = s.header(slot);
    let message = s.private_message(&pool, slot, &t);

    print_header("private attestation", "p4-minimum:23,67-69,325,337");
    let account = s.w.chain.add_private_account(0x81, 1_000_000_000_000);
    let executed = s.private_transitions(account.id(), slot, &message);
    assert_eq!(executed.len(), 2, "the consumer verifies the fact in two transitions");
    let names = ["the consumer's private transition 1", "the consumer's private transition 2"];
    for (((what, e), record), now) in names.iter().zip(&executed).zip(ATTEST_TRANSITIONS).zip(ATTEST_TRANSITIONS_NOW) {
        check_session(what, e, record, now, "p4-minimum:67-68 (16 CPUs: 117; the Macs: p4-macos:51,53)");
    }

    for shape in [Shape::Bare, Shape::Wallet] {
        // The record's runs built each shape on a fresh chain with account 0x81; a second spend of
        // the same note on this chain would not settle, so the wallet shape gets its own account.
        // No checked field depends on the account's identity: every field is a fixed-size array.
        let seed = if shape == Shape::Bare { 0x81 } else { 0x82 };
        let account = s.w.chain.add_private_account(seed, 1_000_000_000_000);
        let root = Actor::new(account.id(), consumer_account());
        let run =
            s.w.chain
                .prove_and_settle_private_as(
                    &account,
                    root,
                    borsh::to_vec(&message).unwrap(),
                    vec![header],
                    &private::catalog(),
                    RESERVE,
                    s.time,
                    shape,
                )
                .expect("proves and settles");
        match shape {
            Shape::Bare => {
                record_row("LEZ's circuit, bare", "1,975,088-1,975,089", "3", "2,621,440", "p4-minimum:69");
                record_row("the whole proof, bare", "2,355,665-2,355,666", "5", "3,276,800", "p4-minimum:23");
                check_size("the private attestation", shape, &run, ATTEST_BARE, ATTEST_BARE_NOW, "p4-minimum:23");
                check_shape("the private attestation", shape, &run, &BARE_ATTESTATION_ROW, "p4-minimum:325");
            }
            Shape::Wallet => {
                record_row(
                    "LEZ's circuit, padded",
                    "2,953,575-2,955,964",
                    "4",
                    "3,670,016",
                    "p4-minimum:337, p4-macos:20",
                );
                record_row("the whole proof, padded", "3,334,152-3,336,541", "6", "-", "p4-minimum:337, p4-macos:20");
                check_size("the private attestation", shape, &run, ATTEST_PADDED, ATTEST_PADDED_NOW, "p4-minimum:337");
                check_shape("the private attestation", shape, &run, &WALLET_ROW, "p4-minimum:324");
            }
        }
    }
}

/// The private claim of `prove_bridge`, in both shapes: the bridge program's two private
/// transitions at the record's exact counts, the transaction at its recorded size, and (padded)
/// the wallet's public shape. The circuit's rows and the totals it dominates are printed from the
/// record beside them — dev mode cannot measure them (see the module header).
#[test]
fn the_private_claim_proves_the_work_the_record_measured() {
    require_dev_mode();
    for shape in [Shape::Bare, Shape::Wallet] {
        let BridgeRun { mut w, account, me, cid, message, actors, .. } = bridge_run();
        print_header(&format!("private claim ({})", shape.name()), "p4-minimum:24,70-72,338");
        let executed = private_claim_transitions(&w, account.id(), &message, &cid, 8);
        assert_eq!(executed.len(), 2, "the claim verifies in two transitions");
        let names = ["the bridge program's private transition 1", "the bridge program's private transition 2"];
        for (((what, e), record), now) in names.iter().zip(&executed).zip(CLAIM_TRANSITIONS).zip(CLAIM_TRANSITIONS_NOW)
        {
            check_session(what, e, record, now, "p4-minimum:70-71 (16 CPUs: 119; the Macs: p4-macos:52)");
        }
        match shape {
            Shape::Bare => {
                record_row("LEZ's circuit, bare", "1,984,079-1,984,201", "3", "2,621,440", "p4-minimum:72,120");
                record_row("the whole proof, bare", "2,136,850-2,136,957", "5", "3,014,656", "p4-minimum:24");
            }
            Shape::Wallet => {
                record_row(
                    "LEZ's circuit, padded",
                    "2,970,148-2,971,802",
                    "4",
                    "3,670,016",
                    "p4-minimum:338, p4-macos:21",
                );
                record_row("the whole proof, padded", "3,122,904-3,124,558", "6", "-", "p4-minimum:338, p4-macos:21");
            }
        }
        let run = w
            .chain
            .prove_and_settle_private_as(
                &account,
                me,
                borsh::to_vec(&message).unwrap(),
                actors,
                &bridge_catalog(),
                RESERVE,
                w.time,
                shape,
            )
            .expect("the claim proves and settles");
        match shape {
            Shape::Bare => {
                check_size("the private claim", shape, &run, CLAIM_BARE, CLAIM_BARE.1, "p4-minimum:24");
            }
            Shape::Wallet => {
                check_size("the private claim", shape, &run, CLAIM_PADDED, CLAIM_PADDED.1, "p4-minimum:338");
                check_shape("the private claim", shape, &run, &WALLET_ROW, "p4-minimum:324");
            }
        }
    }
}

/// The private burn of `prove_bridge`, which proves the claim first and then burns the claimed
/// 1 ETH: the bridge program's transition at the record's exact count (in both shapes, and after
/// each shape's own claim — the record found the programs' transitions unchanged by the padding,
/// p4-minimum:349-351), the transaction at its recorded size, and (padded) the wallet's public
/// shape. The circuit's rows and the whole proof's totals are printed from the record beside
/// them.
#[test]
fn the_private_burn_proves_the_work_the_record_measured() {
    require_dev_mode();
    for shape in [Shape::Bare, Shape::Wallet] {
        let BridgeRun { mut w, account, me, cid, vault, seed, message, actors } = bridge_run();
        // The claim first, as the measured binary proves the two in one run; the burn is proven
        // against the state it leaves.
        let claim = w
            .chain
            .prove_and_settle_private_as(
                &account,
                me,
                borsh::to_vec(&message).unwrap(),
                actors,
                &bridge_catalog(),
                RESERVE,
                w.time,
                shape,
            )
            .expect("the claim proves and settles");
        check_size(
            "the private claim",
            shape,
            &claim,
            if shape == Shape::Bare { CLAIM_BARE } else { CLAIM_PADDED },
            if shape == Shape::Bare { CLAIM_BARE.1 } else { CLAIM_PADDED.1 },
            if shape == Shape::Bare { "p4-minimum:24" } else { "p4-minimum:338" },
        );
        print_header(&format!("private burn ({})", shape.name()), "p4-minimum:25,73-74,339");

        // The holder of the claimed 1 ETH, and its burn, both exactly as `prove_bridge` builds
        // them: the claimed account the chain holds, the burn of leaf 5's note under vault A.
        let holding = HolderState {
            holdings: vec![Holding { config_id: cid, token: ETH, amount: DENOMINATIONS[1] }],
            pending: None,
        };
        let claimed = private_post_state(&w.chain, &private_keys(seed), &account.account, RESERVE, &holding)
            .expect("the chain holds the claimed account's commitment (1 ETH held)");
        let keys = BridgeKeys::from_bridge_seed(&[0x77; 32]);
        let note = wallet_note(&keys, &cid, &vault, 5, ETH, DENOMINATIONS[1]);
        let burn_message = BridgeMessage::burn(cid, &note);
        let pre = claimed.data.actor_state(w.program).clone();
        let executed = private_burn_transition(&w, account.id(), pre, &burn_message);
        check_session(
            "the bridge program's private transition",
            &executed,
            BURN_TRANSITION,
            BURN_TRANSITION_NOW,
            "p4-minimum:73 (16 CPUs: 121; the Macs: p4-macos:52)",
        );
        match shape {
            Shape::Bare => {
                record_row("LEZ's circuit, bare", "1,888,045-1,888,111", "3", "2,359,296", "p4-minimum:74");
                record_row("the whole proof, bare", "1,913,422-1,913,488", "4", "2,490,368", "p4-minimum:25");
            }
            Shape::Wallet => {
                record_row(
                    "LEZ's circuit, padded",
                    "2,876,329-2,878,382",
                    "4",
                    "3,407,872",
                    "p4-minimum:339, p4-macos:22",
                );
                record_row("the whole proof, padded", "2,901,706-2,903,759", "5", "-", "p4-minimum:339, p4-macos:22");
            }
        }

        let holder = PrivateAccount { keys: private_keys(seed), account: claimed.clone() };
        let publics = w.burn_actors(me, &cid, &note)[1..].to_vec();
        let run = w
            .chain
            .prove_and_settle_private_as(
                &holder,
                me,
                borsh::to_vec(&burn_message).unwrap(),
                publics,
                &bridge_catalog(),
                RESERVE,
                w.time,
                shape,
            )
            .expect("the burn proves and settles");
        match shape {
            Shape::Bare => {
                check_size("the private burn", shape, &run, BURN_BARE, BURN_BARE.1, "p4-minimum:25");
            }
            Shape::Wallet => {
                check_size("the private burn", shape, &run, BURN_PADDED, BURN_PADDED.1, "p4-minimum:339");
                check_shape("the private burn", shape, &run, &WALLET_ROW, "p4-minimum:324");
            }
        }
    }
}
