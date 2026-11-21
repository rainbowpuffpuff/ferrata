//! A deposit's claim: which imported root it goes under, its path, the bridge program's checks
//! (`verify_claim`, the token's registry entry and fee schedules, the nullifier), the accounts it
//! names and its message, plaintext `Claim` or private `ClaimPrivately`.

use ferrata_bridge_core::{
    inbound::{DestKind, OwnedDeposit},
    tree::{MerklePath, Tree},
    Hash,
};
use ferrata_lez_bridge_wire::{
    self as bw, code, config_actor, config_seed, holder_actor, holder_seed, nullifier_actor, nullifier_seed, own_actor,
    root_actor, root_seed, token_actor, token_seed, BridgeMessage, RootState, ScheduleRange, TokenState,
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

/// The schedule range a wallet names for a claim of `leaf` (the widest its token state allows).
pub fn range_for(token: &TokenState, leaf: u64) -> ScheduleRange {
    if token.schedules.is_empty() {
        return ScheduleRange { index: 0, from_leaf: 0, until_leaf: u64::MAX };
    }
    let j = token.schedules.iter().rposition(|s| s.from_leaf <= leaf).unwrap_or(0);
    let until = token.schedules.get(j + 1).map_or(token.known_until, |s| s.from_leaf);
    ScheduleRange { index: j as u64, from_leaf: token.schedules[j].from_leaf, until_leaf: until }
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
    pub range: ScheduleRange,
    pub nullifier: Hash,
    pub mint_fee: u128,
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
    let range = range_for(token, d.leaf_index);
    let amount = bw::u128_of(&d.amount)
        .ok_or_else(|| Refusal::program(code::NOT_REGISTERED, "checked before sending: the amount is above 2^128"))?;
    let Some(k) = token.entry.index_of(amount) else {
        return Err(Refusal::program(
            code::NOT_REGISTERED,
            "checked before sending: the amount is not one of the token's denominations",
        ));
    };
    if let Err(code) = bw::verify_claim(inputs.root, &witness, &range) {
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
    let (mint_fee, _) =
        token.fees_for(amount, &range).map_err(|code| Refusal::program(code, "checked before sending"))?;
    let relay_fee = token.entry.relay_fees.get(k).copied().unwrap_or(0);
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
        BridgeMessage::ClaimPrivately { config_id: ctx.cid, leaf_count: n, witness, range, relayer }
    } else {
        mentions.push(Mention::Pda { actor: root, seed: root_seed(&ctx.cid, n) });
        BridgeMessage::Claim { config_id: ctx.cid, leaf_count: n, witness, range, relayer }
    };
    mentions.push(Mention::Pda {
        actor: token_actor(ctx.program, &ctx.cid, &d.token),
        seed: token_seed(&ctx.cid, &d.token),
    });
    mentions.push(Mention::Pda { actor: config_actor(ctx.program, &ctx.cid), seed: config_seed(&ctx.cid) });
    mentions.push(Mention::Pda { actor: n_actor, seed: nullifier_seed(&ctx.cid, ctx.chain_id, &ctx.vault, &owned.nullifier) });
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
    Ok(ClaimPlan {
        what,
        mentions,
        root_index: 0,
        message,
        leaf_count: n,
        path,
        range,
        nullifier: owned.nullifier,
        mint_fee,
        relay_fee,
    })
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
}
