//! Recovery from the seed alone (022 F.9, R.5; proposal §B1): the wallet downloads the vault's
//! whole deposit list and the zone's public history once, and matches locally. No provider and no
//! node is asked about any single deposit or burn, so none learns which are the wallet's.

use std::collections::HashSet;

use anyhow::Result;
use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{self, OwnedDeposit},
    outbound::{recover_burns, OwnedBurn},
    Hash,
};

use crate::{
    eth::DepositList,
    lez::{Context, History, BURN_GAP},
    seed::Seed,
};

/// Where a deposit of this wallet stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepositState {
    /// Its nullifier is spent on the zone.
    Claimed,
    /// The vault refunded it (after a sunset).
    Refunded,
    /// Claimable now: an imported root covers it (the newest such root's leaf count).
    Claimable { root_leaf_count: u64 },
    /// Not yet covered by an imported root.
    NotImported,
}

#[derive(Clone, Debug)]
pub struct DepositRow {
    pub owned: OwnedDeposit,
    pub state: DepositState,
}

/// This seed's deposits among `list`, with their state.
pub fn deposits(
    seed: &Seed,
    ctx: &Context,
    list: &DepositList,
    gap: u32,
    history: &History,
    refunded: &HashSet<u64>,
) -> Result<Vec<DepositRow>> {
    let keys = seed.bridge_keys();
    let dests = seed.destinations(gap)?;
    let owned =
        inbound::recover(&keys.auth, &chain_id_u256(ctx.chain_id), &ctx.vault, &list.deposits, gap, |kind, a| {
            dests.of(kind, a)
        });
    Ok(owned
        .into_iter()
        .map(|owned| {
            let state = if crate::lez::is_claimed(ctx, history, &owned.nullifier) {
                DepositState::Claimed
            } else if refunded.contains(&owned.deposit.leaf_index) {
                DepositState::Refunded
            } else if let Some(&n) = history.roots.iter().rev().find(|&&n| n > owned.deposit.leaf_index) {
                DepositState::Claimable { root_leaf_count: n }
            } else {
                DepositState::NotImported
            };
            DepositRow { owned, state }
        })
        .collect())
}

/// Where a burn of this wallet stands.
#[derive(Clone, Debug)]
pub struct BurnRow {
    pub owned: OwnedBurn,
    /// Its `NOut` is used on the vault.
    pub released: bool,
}

/// This seed's burns among the zone's published notes, with their state; and the next burn counter.
pub fn burns(seed: &Seed, ctx: &Context, history: &History, released: &HashSet<Hash>) -> (Vec<BurnRow>, u64) {
    let keys = seed.bridge_keys();
    let scan = recover_burns(&keys, &ctx.cid, &chain_id_u256(ctx.chain_id), &ctx.vault, &history.burns, BURN_GAP);
    let rows =
        scan.owned.into_iter().map(|owned| BurnRow { released: released.contains(&owned.nullifier), owned }).collect();
    (rows, scan.next_i)
}
