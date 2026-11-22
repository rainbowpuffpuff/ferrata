//! RFP-022 Performance #7: "Document the growth rate and on-chain storage cost of all bridge state that
//! accumulates with usage, projected at 1M and 10M operations." `docs/performance.md` (RFP-022 §P.7) projects
//! LEZ's growth from three per-operation accounts: `root(cid, n)` per import, `nullifier(chain, vault, N)` per
//! claim and `burn(cid, H(note))` per burn, the first measured and the other two estimated from their types. This
//! test measures all three as the bridge program writes them through LEZ v0.4.0-rc1's state machine (the harness,
//! dev mode), pins them, and checks the projection's premise: a second claim and a second burn each add an account
//! of the same size, and a claim or a burn adds nothing to the configuration's or the token's account (the
//! baseline is taken after `World::with_roots` has imported the roots, so an import is not measured here).

use ferrata_bridge_core::{
    chain_id_u256,
    outbound::{burn_commitment, burn_secret, burn_tag, xor_memo},
    BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{
    burn_actor, burn_mac_v2, config_actor, own_actor, root_actor, token_actor, word_of_u128, BridgeMessage, Note,
};
use harness::bridge::{nullifier_of, World, CHAIN_ID, DENOMINATIONS, ETH};

/// `docs/performance.md`, RFP-022 §P.7: the sizes its projection multiplies (117 B measured; 77 B and 185 B
/// estimated from the types).
const DOC_ROOT: usize = 117;
const DOC_NULLIFIER: usize = 77;
const DOC_BURN: usize = 185;

/// A burn note of `keys` (as `tests/bridge_burn.rs` builds one), the `i`-th of the wallet.
fn note(keys: &BridgeKeys, cid: &Hash, vault: &[u8; 20], i: u64) -> Note {
    let recipient = [0xe1; 20];
    let r = burn_tag(&keys.view, cid, i);
    let s_out = burn_secret(&keys.auth, cid, &r);
    let c_out = burn_commitment(&s_out, &chain_id_u256(CHAIN_ID), vault, &recipient);
    let memo = xor_memo(&keys.view, &r, &recipient);
    let amount = word_of_u128(DENOMINATIONS[1]);
    let mac = burn_mac_v2(&keys.view, &r, &memo, &ETH, &amount, &c_out);
    Note { r, memo, mac, token: ETH, amount, c_out }
}

#[test]
fn each_import_claim_and_burn_adds_one_account_of_a_fixed_size() {
    let mut w = World::with_roots();
    let (cid, vault, payer, program) = (w.cid_a, w.vault_a.address, w.chain.payer, w.program);
    let size = |w: &World, actor| w.chain.actor_state(actor).len();
    let config = config_actor(program, &cid);
    let token = token_actor(program, &cid, &ETH);

    // An import: the root at 8 leaves, which `World::with_roots` imports through the program.
    let root = size(&w, root_actor(program, &cid, 8));
    assert_eq!(root, DOC_ROOT, "root(cid, 8): {root} bytes; performance.md measured {DOC_ROOT}");

    // Two claims: each writes one nullifier account; the configuration and the token keep their sizes.
    let (config0, token0) = (size(&w, config), size(&w, token));
    let mut nullifiers = Vec::new();
    for (label, deposit) in
        [("Claim: leaf 6", w.payer_deposit.clone()), ("Claim: the vector's deposit", w.vector.clone())]
    {
        let witness = w.vault_a.witness(&deposit, 8);
        let r = w.claim(label, &cid, 8, &vault, witness);
        assert!(r.ok(), "{label}: {:?} (exit {:?})", r.charge.revert, r.exit_code);
        let n = deposit.nullifier(CHAIN_ID, &vault);
        nullifiers.push(size(&w, nullifier_of(program, &cid, CHAIN_ID, &vault, &n)));
    }
    assert_eq!((size(&w, config), size(&w, token)), (config0, token0), "claims grow neither config nor token");

    // Two public burns of 1 ETH by the payer: each writes one burn record; config and token keep their sizes.
    let keys = BridgeKeys::from_bridge_seed(&[0x77; 32]);
    let holder = own_actor(program, payer);
    let mut burns = Vec::new();
    for i in 0..2 {
        let note = note(&keys, &cid, &vault, i);
        let actors = w.public_burn_actors(payer, &cid, &note);
        let r = w.send(&format!("Burn {i}: 1 ETH by the payer"), holder, &BridgeMessage::burn(cid, &note), actors);
        assert!(r.ok(), "burn {i}: {:?} (exit {:?})", r.charge.revert, r.exit_code);
        burns.push(size(&w, burn_actor(program, &cid, &note.hash())));
    }
    assert_eq!((size(&w, config), size(&w, token)), (config0, token0), "burns grow neither config nor token");

    println!("| account | per | bytes (this tree) | performance.md |");
    println!("|---|---|---:|---:|");
    println!("| root(cid, n) | import | {root} | {DOC_ROOT} (measured) |");
    println!("| nullifier(chain, vault, N) | claim | {} | {DOC_NULLIFIER} (estimated) |", nullifiers[0]);
    println!("| burn(cid, H(note)) | burn | {} | {DOC_BURN} (estimated) |", burns[0]);
    assert_eq!(nullifiers[0], nullifiers[1], "every claim's nullifier account has one size");
    assert_eq!(burns[0], burns[1], "every burn record has one size");
    assert_eq!(nullifiers[0], DOC_NULLIFIER, "a nullifier account: {} bytes", nullifiers[0]);
    assert_eq!(burns[0], DOC_BURN, "a burn record: {} bytes", burns[0]);
}
