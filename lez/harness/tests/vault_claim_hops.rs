//! RFP-021 P.4, hops inside the user's proof: the reference consumer's private claim of the vault
//! fixture's deposit 0 (Sepolia block 12,111,477) under the record of block 12,112,773, its block
//! reached through chains of 1 to 5 EIP-2935 hops (`harness::hops::claim_hops`: real data, the
//! fixtures `sepolia-vault-v5-2026-11-11`, `sepolia-hops-2026-11-18` and `sepolia-hops-2026-11-27`).
//! The consumer's two private transitions run in the executor (the program a claimant's device
//! proves, beside LEZ's circuit, which dev mode cannot measure), for each chain length:
//!
//! * every chain verifies the same deposit, keyed on the same statement identifier: the reply sends
//!   the same `Spend` of the same nullifier, and the claimant keeps the same state;
//! * each hop costs user cycles, so the user-side cost grows with the hops: the figures are
//!   written to `results/vault-claim-hops.md` (RFP-021 P.4's table of hops, `docs/performance.md`);
//! * a chain with a link broken (a header swapped for another block's, a hop left out, the order
//!   reversed) is refused with the core's code, and nothing is sent.
//!
//! The real proofs of the padded private claim at 5 hops: `prove_vault_claim --hops 5 --shape
//! wallet` (`lez/results/p4-hops/`).

use ferrata_core::Error;
use ferrata_lez_wire::consumer::{secret_nullifier, ConsumerMessage};
use harness::{
    accumulated::VaultWorld,
    attest::{execute, Executed},
    consumer_account, consumer_program, grouped,
    hops::{claim_hops, MAX_USER_HOPS},
};
use lee::Actor;
use lee_core::program::{ReceiveInput, StateReply};

/// The claim's two private transitions in the executor: the root call at a fresh private account's
/// actor (it sends `Read`), then the record's reply (it verifies the deposit and sends `Spend`).
fn transitions(vw: &VaultWorld, message: &ConsumerMessage) -> (Executed, Option<Executed>) {
    let account = lee::AccountId::new([0x93; 32]);
    let actor = Actor::new(account, consumer_account());
    let call = ReceiveInput {
        receiver: actor,
        from: None,
        is_authorized: true,
        pre_state: Default::default(),
        message: borsh::to_vec(message).unwrap(),
    };
    let first = execute("ClaimDepositPrivately (sends Read)", consumer_program().elf(), &call);
    let Some(t) = &first.transition else { return (first, None) };
    let reply = ReceiveInput {
        receiver: actor,
        from: Some(vw.v.actor()),
        is_authorized: true,
        pre_state: t.response.post_state.clone().expect("the claim is kept for the reply"),
        message: borsh::to_vec(&StateReply { state: vw.v.w.chain.actor_state(vw.v.actor()) }).unwrap(),
    };
    let second = execute("the record's reply", consumer_program().elf(), &reply);
    (first, Some(second))
}

fn claim(vw: &VaultWorld, deposit: ferrata_lez_wire::consumer::DepositProof) -> ConsumerMessage {
    let (s, kind, dest) = vw.openings[0];
    ConsumerMessage::ClaimDepositPrivately { pool: vw.pool.clone(), slot: vw.v.slot, deposit, s, kind, dest }
}

#[test]
fn a_private_claim_through_one_to_five_hops() {
    let vw = VaultWorld::placed();
    let d = &vw.v.deposits[0];
    let (s0, _, _) = vw.openings[0];
    let n0 = secret_nullifier(&s0, &vw.v.deposit_id(0));
    let mut out = format!(
        "# The private claim's hops inside the user's proof (RFP-021 P.4)\n\n\
         Written by `lez/harness/tests/vault_claim_hops.rs`. The reference consumer's private claim \
         (`ClaimDepositPrivately`) of the vault fixture's deposit 0 (Sepolia block {}) under the record of \
         block {} (finalized slot {}), the deposit's block {} blocks back reached through a chain of `k` \
         EIP-2935 hops; the consumer's two private transitions in the executor (consumer image `{}`). \
         LEZ's privacy-preserving circuit over them, most of a private transaction's proving time, does not \
         run in dev mode: the proofs are in `lez/results/p4-hops/`.\n\n\
         | hops | root call (sends `Read`), user cycles | the record's reply (verifies, sends `Spend`), user cycles | both | over 1 hop | segments (both) | input bytes (reply) |\n\
         |---:|---:|---:|---:|---:|---:|---:|\n",
        d.block,
        vw.v.block,
        vw.v.slot,
        vw.v.block - d.block,
        harness::image_id_hex(methods::FERRATA_CONSUMER_ID),
    );
    let mut spends = Vec::new();
    let mut previous = 0u64;
    let mut first_total = 0u64;
    for k in 1..=MAX_USER_HOPS {
        let deposit = vw.v.deposit_proof_hops(0, k);
        assert_eq!(deposit.hops.len(), k);
        let (root, reply) = transitions(&vw, &claim(&vw, deposit));
        assert_eq!(root.exit_code, 0, "{k} hops: the root call passes");
        let reply = reply.expect("the reply runs");
        assert_eq!(reply.exit_code, 0, "{k} hops: the deposit verifies");
        let t = reply.transition.as_ref().unwrap();
        // The same deposit whatever the chain: the same Spend of the same nullifier, the same state kept.
        spends.push(borsh::to_vec(&(&t.response.sync_messages, &t.response.post_state)).unwrap());
        assert_eq!(t.response.sync_messages.len(), 1, "{k} hops: the reply sends one message, Spend");
        let total = root.cycles + reply.cycles;
        assert!(total > previous, "{k} hops cost more than {} hops", k - 1);
        if k == 1 {
            first_total = total;
        }
        out += &format!(
            "| {k} | {} | {} | {} | {} | {} | {} |\n",
            grouped(root.cycles),
            grouped(reply.cycles),
            grouped(total),
            if k == 1 { "—".to_string() } else { format!("+{}", grouped(total - first_total)) },
            root.segments + reply.segments,
            grouped(reply.input_bytes as u64),
        );
        previous = total;
    }
    assert!(spends.windows(2).all(|w| w[0] == w[1]), "every chain spends the same nullifier and keeps the same state");
    out += &format!(
        "\nEvery chain verifies the same deposit: the reply sends the same `Spend` (nullifier `N = H(s ‖ statement id)` = \
         `{}…`) and the claimant keeps the same state, byte for byte, for 1 to {MAX_USER_HOPS} hops.\n",
        &hex::encode(n0)[..16]
    );
    std::fs::write(harness::results_dir().join("vault-claim-hops.md"), out).unwrap();
}

#[test]
fn a_broken_chain_is_refused_and_sends_nothing() {
    let vw = VaultWorld::placed();
    let d = vw.v.deposits[0].block;
    let five = vw.v.deposit_proof_hops(0, MAX_USER_HOPS);
    let refused = |what: &str, deposit: ferrata_lez_wire::consumer::DepositProof| {
        let (root, reply) = transitions(&vw, &claim(&vw, deposit));
        assert_eq!(root.exit_code, 0, "{what}: the root call only keeps the claim");
        let reply = reply.expect("the reply runs");
        assert!(reply.transition.is_none(), "{what}: nothing is sent");
        reply.exit_code
    };
    // A header of the chain with one byte of its logs bloom flipped: it decodes, with the same
    // number, but its hash is not the one the history slot holds.
    let mut tampered = five.clone();
    let h = &mut tampered.hops[2].header;
    assert_eq!(&h[189..192], &[0xb9, 0x01, 0x00], "the logs bloom's prefix");
    h[300] ^= 1;
    assert_eq!(refused("a tampered header", tampered), u32::from(Error::HistoryHashMismatch.code()));
    // A header swapped for the next block's of the chain: its slot's proof is not there.
    let mut swapped = five.clone();
    swapped.hops[2].header = five.hops[3].header.clone();
    let swapped_code = refused("a swapped header", swapped);
    assert_ne!(swapped_code, 0);
    // A hop left out: the next hop's proofs are of another block's state than the one reached.
    let mut gap = five.clone();
    gap.hops.remove(2);
    let gap_code = refused("a hop left out", gap);
    assert_ne!(gap_code, 0);
    // The chain reversed: the first hop reaches a block its record's window does not hold (newest first).
    let mut reversed = five.clone();
    reversed.hops.reverse();
    let reversed_code = refused("the chain reversed", reversed);
    assert_ne!(reversed_code, 0);
    // The same chain, intact, verifies (the refusals above are the breaks, not the chain).
    let (_, reply) = transitions(&vw, &claim(&vw, five));
    assert_eq!(reply.expect("the reply runs").exit_code, 0);
    // More hops than the chains hold are not built.
    assert!(std::panic::catch_unwind(|| claim_hops(MAX_USER_HOPS + 1, vw.v.block, d, vw.v.hop(d))).is_err());
    println!(
        "refusal codes: tampered header {}, swapped header {swapped_code}, hop left out {gap_code}, reversed {reversed_code}",
        u32::from(Error::HistoryHashMismatch.code())
    );
}
