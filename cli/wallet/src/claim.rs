//! A deposit's claim: which imported root it goes under, its path, the bridge program's checks
//! (`verify_claim`, the token's registry entry and fixed relay fee, the nullifier), the accounts it
//! names and its message, plaintext `Claim` or private `ClaimPrivately`. Imported fee schedules are
//! not needed: the mint fee was taken on Ethereum at deposit.

use std::collections::HashSet;

use ferrata_bridge_core::{
    inbound::{deposit_leaf, DestKind, OwnedDeposit, VaultDeposit},
    tree::{MerklePath, Tree},
    Hash,
};
use ferrata_lez_bridge_wire::{
    self as bw, code, config_actor, config_seed, holder_actor, holder_seed, nullifier_actor, nullifier_seed, own_actor,
    root_actor, root_seed, token_actor, token_seed, BridgeMessage, RootState, TokenState,
};
use lee_core::account::AccountId;

use crate::{Context, Mention, Refusal};

/// The imported root a claim of `leaf_index` goes under: the newest whose leaf count covers it.
/// `imported`: the leaf counts of the configuration's imported roots, any order.
pub fn pick_import(imported: &[u64], leaf_index: u64) -> Result<u64, Refusal> {
    match imported.iter().copied().filter(|&n| n > leaf_index).max() {
        Some(n) => Ok(n),
        None => Err(Refusal::program(
            if imported.is_empty() { code::NOT_FOUND } else { code::LEAF_NOT_IMPORTED },
            "checked before sending: no imported root covers this leaf yet; wait for the next root import",
        )),
    }
}

/// The anonymity set of a private claim (RFP-022 F.8): how many deposits of the vault an observer of both chains cannot
/// tell it apart from. A private claim shows the token and the denomination (its fixed relay fee and the token's
/// buckets) and the imported root it reads (`root(cid, n)`, so leaf indexes below `n`); it hides the leaf, the depositor
/// and the commitment. No fee schedule range is shown, so deposits on both sides of a fee change share one set. The set
/// is the deposits of the same token and amount below `leaf_count`, less the deposits `opened`: those whose leaf
/// (`deposit_leaf`) a plaintext claim has published, under any configuration of the program (its witness names the leaf
/// index and `s`, so an observer can tell whether a claim spends that deposit). Deposits claimed privately stay in:
/// their nullifiers do not say which leaves they spent. A refunded deposit needs no exclusion: a refund is of a leaf no
/// imported root covers. Timing is not counted here: a claim made soon after its deposit narrows the set by time, which
/// the claim delay (`ferrata scan`) spreads.
pub fn anonymity_set(
    deposits: &[VaultDeposit],
    opened: &HashSet<Hash>,
    token: &[u8; 20],
    amount: &[u8; 32],
    leaf_count: u64,
) -> usize {
    deposits
        .iter()
        .filter(|d| d.token == *token && d.amount == *amount)
        .filter(|d| d.leaf_index < leaf_count)
        .filter(|d| !opened.contains(&deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment)))
        .count()
}

/// What the claim of one deposit reads from the chains.
pub struct ClaimInputs<'a> {
    /// The vault's deposit leaves, in order (at least the first `leaf_count`).
    pub leaves: &'a [Hash],
    /// The imported root it goes under ([`pick_import`]) and its state at `root(cid, n)`.
    pub leaf_count: u64,
    pub root: &'a RootState,
    /// `token(cid, token)`'s state.
    pub token: &'a TokenState,
    /// Whether the deposit's nullifier is spent: `nullifier(chain, vault, N)` exists, or the zone's
    /// history shows a plaintext claim with it.
    pub spent: bool,
}

/// A claim, built and checked.
#[derive(Clone, Debug)]
pub struct ClaimPlan {
    pub what: String,
    pub mentions: Vec<Mention>,
    pub root_index: usize,
    pub message: BridgeMessage,
    pub leaf_count: u64,
    pub path: MerklePath,
    pub nullifier: Hash,
    pub relay_fee: u128,
}

/// The claim of `owned` (`private`: `ClaimPrivately`, proven by the claimant's private account; else
/// `Claim`), with the bridge program's checks run first so that a refusal names the program's code.
/// A private-kind deposit is proven by its destination; a public-kind one by `prover`, any private
/// account of the wallet (the mint still lands at the public destination). `relayer`: who is paid the
/// relay fee (default: the token's fee recipient).
pub fn build_claim(
    ctx: &Context,
    owned: &OwnedDeposit,
    inputs: &ClaimInputs,
    private: bool,
    relayer: Option<Hash>,
    prover: Option<AccountId>,
) -> Result<ClaimPlan, Refusal> {
    let d = &owned.deposit;
    let n = inputs.leaf_count;
    if (inputs.leaves.len() as u64) < n {
        return Err(Refusal::wallet(format!(
            "the vault's list has {} leaves, the imported root {n}",
            inputs.leaves.len()
        )));
    }
    let tree =
        Tree::from_leaves(inputs.leaves[..n as usize].to_vec()).map_err(|_| Refusal::wallet("too many leaves"))?;
    if tree.root() != inputs.root.root {
        return Err(Refusal::wallet(format!(
            "the vault's deposit list does not give the imported root at {n} leaves: the RPC or the zone serves another vault"
        )));
    }
    let path =
        tree.path(d.leaf_index).ok_or_else(|| Refusal::wallet(format!("leaf {} is not in the tree", d.leaf_index)))?;
    let witness = bw::ClaimWitness {
        depositor: d.depositor,
        token: d.token,
        amount: d.amount,
        leaf_index: d.leaf_index,
        path: path.siblings.to_vec(),
        s: owned.s,
        kind: owned.kind as u8,
        dest: owned.dest,
    };
    let token = inputs.token;
    let amount = bw::u128_of(&d.amount)
        .ok_or_else(|| Refusal::program(code::NOT_REGISTERED, "checked before sending: the amount is above 2^128"))?;
    if token.entry.index_of(amount).is_none() {
        return Err(Refusal::program(
            code::NOT_REGISTERED,
            "checked before sending: the amount is not one of the token's denominations",
        ));
    }
    if let Err(code) = bw::verify_claim(inputs.root, &witness) {
        return Err(Refusal::program(code, "checked before sending with the program's verify_claim"));
    }
    if !private && owned.kind != DestKind::Public {
        return Err(Refusal::program(
            code::WRONG_DESTINATION,
            "checked before sending: a private-kind deposit is claimed privately",
        ));
    }
    if inputs.spent {
        return Err(Refusal::program(code::ALREADY_CLAIMED, "checked before sending: the nullifier is spent"));
    }
    let relay_fee = token.relay_fee_for(amount).map_err(|code| Refusal::program(code, "checked before sending"))?;
    let fee_recipient = AccountId::new(token.controls.fee_recipient);
    let relayer_account = relayer.map(AccountId::new).unwrap_or(fee_recipient);
    let root = root_actor(ctx.program, &ctx.cid, n);
    let n_actor = nullifier_actor(ctx.program, &ctx.cid, ctx.chain_id, &ctx.vault, &owned.nullifier);
    let mut mentions = Vec::new();
    let message = if private {
        let root_account = match owned.kind {
            DestKind::Private => AccountId::new(owned.dest),
            DestKind::Public => prover.ok_or_else(|| {
                Refusal::wallet(
                    "a private claim of a public-kind deposit is proven by a private account of the wallet; none given",
                )
            })?,
        };
        mentions.push(Mention::PrivateSelf(own_actor(ctx.program, root_account)));
        mentions.push(Mention::Pda { actor: root, seed: root_seed(&ctx.cid, n) });
        BridgeMessage::ClaimPrivately { config_id: ctx.cid, leaf_count: n, witness, relayer }
    } else {
        mentions.push(Mention::Pda { actor: root, seed: root_seed(&ctx.cid, n) });
        BridgeMessage::Claim { config_id: ctx.cid, leaf_count: n, witness, relayer }
    };
    mentions.push(Mention::Pda {
        actor: token_actor(ctx.program, &ctx.cid, &d.token),
        seed: token_seed(&ctx.cid, &d.token),
    });
    mentions.push(Mention::Pda { actor: config_actor(ctx.program, &ctx.cid), seed: config_seed(&ctx.cid) });
    mentions.push(Mention::Pda {
        actor: n_actor,
        seed: nullifier_seed(&ctx.cid, ctx.chain_id, &ctx.vault, &owned.nullifier),
    });
    let dest = (owned.kind == DestKind::Public).then(|| AccountId::new(owned.dest));
    // Mints land at `holder(cid, account)` PDAs (security review C-1); the mint's seed admits a new one.
    for account in dest.into_iter().chain([fee_recipient, relayer_account]) {
        let actor = holder_actor(ctx.program, &ctx.cid, account);
        if !mentions.iter().any(|m| m.actor() == actor) {
            mentions.push(Mention::Pda { actor, seed: holder_seed(&ctx.cid, account.value()) });
        }
    }
    let what = format!(
        "{} claim of leaf {} under root(cid, {n})",
        if private { "private" } else { "plaintext" },
        d.leaf_index
    );
    Ok(ClaimPlan { what, mentions, root_index: 0, message, leaf_count: n, path, nullifier: owned.nullifier, relay_fee })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_covering_import() {
        assert_eq!(pick_import(&[5, 8, 6], 5), Ok(8));
        assert_eq!(pick_import(&[5, 8], 4), Ok(8));
        assert!(matches!(pick_import(&[], 0), Err(Refusal::Program { code: 245, .. })));
        assert!(matches!(pick_import(&[5], 5), Err(Refusal::Program { code: 248, .. })));
    }

    #[test]
    fn the_anonymity_set_is_the_same_token_and_amount_under_the_root() {
        let word = |v: u8| {
            let mut w = [0u8; 32];
            w[31] = v;
            w
        };
        let eth = [0u8; 20];
        let other = [0x7e; 20];
        let d = |leaf_index: u64, token: [u8; 20], amount: u8| VaultDeposit {
            depositor: [0xd1; 20],
            j: leaf_index,
            leaf_index,
            token,
            amount: word(amount),
            commitment: [leaf_index as u8; 32],
        };
        // Leaves 0..10: ETH 1 at 0, 2, 3, 5, 8, 9; ETH 2 at 1, 6; the other token's 1 at 4, 7.
        let list: Vec<VaultDeposit> = [
            (0, eth, 1),
            (1, eth, 2),
            (2, eth, 1),
            (3, eth, 1),
            (4, other, 1),
            (5, eth, 1),
            (6, eth, 2),
            (7, other, 1),
            (8, eth, 1),
            (9, eth, 1),
        ]
        .into_iter()
        .map(|(i, t, a)| d(i, t, a))
        .collect();
        let none = HashSet::new();
        assert_eq!(anonymity_set(&list, &none, &eth, &word(1), 10), 6);
        assert_eq!(anonymity_set(&list, &none, &eth, &word(1), 6), 4, "only the leaves under root(cid, 6)");
        assert_eq!(anonymity_set(&list, &none, &eth, &word(2), 10), 2);
        assert_eq!(anonymity_set(&list, &none, &other, &word(1), 10), 2);
        // Plaintext claims published leaves 2 and 8 (and a leaf of another vault, which matches none of this list):
        // an observer sees which deposits those claims spent, so they leave every later claim's set.
        let leaf = |d: &VaultDeposit| deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment);
        let mut elsewhere = list[2].clone();
        elsewhere.commitment = [0xee; 32];
        let opened: HashSet<Hash> = [leaf(&list[2]), leaf(&list[8]), leaf(&elsewhere)].into();
        assert_eq!(anonymity_set(&list, &opened, &eth, &word(1), 10), 4, "leaves 0, 3, 5 and 9");
        assert_eq!(anonymity_set(&list, &opened, &eth, &word(1), 6), 3, "leaves 0, 3 and 5");
        assert_eq!(anonymity_set(&list, &opened, &eth, &word(2), 10), 2, "no deposit of 2 opened");
    }
}
