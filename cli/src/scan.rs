//! Recovery from the seed alone (022 F.9, R.5; proposal §B1): the wallet downloads the vault's
//! whole deposit list and the zone's public history once, and matches locally. No provider and no
//! node is asked about any single deposit or burn, so none learns which are the wallet's.

use std::collections::HashSet;

use anyhow::Result;
use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{self, OwnedDeposit},
    outbound::{recover_burns, OwnedBurn, PublishedBurn},
    Hash,
};

use crate::{
    eth::DepositList,
    lez::{Context, History, BURN_GAP, REFUSED},
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
    /// The burn index of another burn of this seed with the same tag: one `NOut` for both, so at most one of the two is
    /// ever released. Only a program that applied two notes with one tag shows it (the program before 22 November
    /// 2026, with a tool that reused a counter: review H-1; ADR 0032).
    pub twin: Option<u64>,
}

/// This seed's burns among the zone's published notes, with their state; and the next burn counter. From the seed alone
/// ([`burns_past`] with no journal).
pub fn burns(seed: &Seed, ctx: &Context, history: &History, released: &HashSet<Hash>) -> (Vec<BurnRow>, u64) {
    burns_past(seed, ctx, history, released, 0)
}

/// This seed's burns among the zone's applied notes, with their state; and the next burn counter (after every counter
/// used, applied or refused, and below `reserved`).
///
/// A counter is used once its tag is on the zone, in an applied burn or in a refused one: the tool never gives a
/// refused burn's counter to another burn (its memo is public in its block), so a run of refused burns leaves a run of
/// counters with no applied burn, and the scan, which ends after [`BURN_GAP`] unused counters, counts refused tags as
/// used to cross it (review M1 of 22 November 2026: 20 refused burns hid the next applied one). `reserved` is the
/// lowest counter the burn journal never handed out ([`crate::burns::Counters::next`]; 0 without one): every counter
/// below it was reserved, whether or not the zone shows its tag (a burn reserved and never sent), so the scan does not
/// end before [`BURN_GAP`] counters past it.
pub fn burns_past(
    seed: &Seed,
    ctx: &Context,
    history: &History,
    released: &HashSet<Hash>,
    reserved: u64,
) -> (Vec<BurnRow>, u64) {
    let keys = seed.bridge_keys();
    let notes: Vec<PublishedBurn> = history.burns.iter().chain(&history.refused_burns).copied().collect();
    // `recover_burns` ends `gap` counters past the last used one it finds; every counter below `reserved` is used.
    let gap = BURN_GAP.saturating_add(reserved);
    let scan = recover_burns(&keys, &ctx.cid, &chain_id_u256(ctx.chain_id), &ctx.vault, &notes, gap);
    // A refused note burnt nothing: only applied notes are burns.
    let owned: Vec<OwnedBurn> = scan.owned.into_iter().filter(|o| o.burn.burn_index != REFUSED).collect();
    let twin = |o: &OwnedBurn| {
        owned.iter().find(|x| x.i == o.i && x.burn.burn_index != o.burn.burn_index).map(|x| x.burn.burn_index)
    };
    let rows = owned
        .iter()
        .map(|owned| BurnRow { released: released.contains(&owned.nullifier), twin: twin(owned), owned: owned.clone() })
        .collect();
    (rows, scan.next_i.max(reserved))
}

#[cfg(test)]
mod tests {
    use alloy::primitives::Address;
    use ferrata_bridge_core::{
        encoding::u256_from_u128,
        outbound::{burn_tag, BurnNote, ReleaseTarget},
    };
    use lee::AccountId;

    use super::*;
    use crate::{burns, lez};

    /// BIP-39 test phrase (a published test value, never a wallet).
    const USER: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    /// Review M1 (22 November 2026), the reviewer's case through the tool's own code: counter 0 applied, then a run of
    /// burns refused on the zone (each uses its counter: the journal never gives it to another burn), then the counter
    /// the journal hands out next, applied. `ferrata release`, `recover` and `scan` (all `scan::burns`) find it after
    /// 20 and 25 refused counters (before the fix they found only counter 0 and said "nothing to release"), and the
    /// next counter is the journal's.
    #[test]
    fn burns_found_past_a_run_of_refused_counters() {
        let user = Seed::from_phrase(USER).unwrap();
        let keys = user.bridge_keys();
        let ctx = Context { program: AccountId::new([7; 32]), cid: [9; 32], chain_id: 1, vault: [0x55; 20] };
        let to = [0xe1; 20];
        let target = ReleaseTarget { chain_id: chain_id_u256(1), vault: ctx.vault, recipient: to };
        let amount = 1_000_000_000_000_000_000u128;
        let note = |i: u64| BurnNote::create(&keys, &ctx.cid, i, &target, [0; 20], u256_from_u128(amount));
        let tag = |i: u64| burn_tag(&keys.view, &ctx.cid, i);
        let note_of = |i: u64| lez::note_id(&note(i));
        for run in [1u64, 19, 20, 25, 60] {
            let mut h = History::default();
            h.burns.push(PublishedBurn { burn_index: 0, note: note(0) });
            for i in 1..=run {
                h.refused_burns.push(PublishedBurn { burn_index: REFUSED, note: note(i) });
            }
            // The journal's choice for the next burn, from the zone as `flow::burn` reads it.
            let seen = burns::Seen {
                applied: h.burns.iter().map(|b| (b.note.r, lez::note_id(&b.note))).collect(),
                refused: h.refused_burns.iter().map(|b| (b.note.r, lez::note_id(&b.note))).collect(),
                next: h.next_burn_counter(&keys, &ctx),
            };
            let ask =
                burns::Ask { from: "private /0".into(), token: Address::ZERO, amount, recipient: Address::from(to) };
            let i = burns::choose(&mut burns::Counters::default(), &seen, &ask, 0, &tag, &note_of).unwrap().counter.i();
            assert_eq!(i, run + 1, "after {run} refused counters");
            h.burns.push(PublishedBurn { burn_index: 1, note: note(i) });
            let (rows, next) = burns(&user, &ctx, &h, &HashSet::new());
            let found: Vec<(u64, u64)> = rows.iter().map(|r| (r.owned.i, r.owned.burn.burn_index)).collect();
            assert_eq!(found, vec![(0, 0), (i, 1)], "after {run} refused counters: the refused ones are not burns");
            assert_eq!(next, i + 1);
            assert!(rows.iter().all(|r| r.twin.is_none() && !r.released));
        }
    }

    /// A counter the journal reserved counts as used even where the zone shows no tag for it (a burn reserved and
    /// never sent; a refused burn the history read does not show): with the journal's next counter the scan finds a
    /// burn past more than [`BURN_GAP`] such counters; from the seed alone it cannot.
    #[test]
    fn burns_found_past_counters_the_journal_reserved() {
        let user = Seed::from_phrase(USER).unwrap();
        let keys = user.bridge_keys();
        let ctx = Context { program: AccountId::new([7; 32]), cid: [9; 32], chain_id: 1, vault: [0x55; 20] };
        let target = ReleaseTarget { chain_id: chain_id_u256(1), vault: ctx.vault, recipient: [0xe1; 20] };
        let note = |i: u64| BurnNote::create(&keys, &ctx.cid, i, &target, [0; 20], u256_from_u128(1));
        let mut h = History::default();
        h.burns.push(PublishedBurn { burn_index: 0, note: note(0) });
        h.burns.push(PublishedBurn { burn_index: 1, note: note(30) });
        let (alone, next_alone) = burns(&user, &ctx, &h, &HashSet::new());
        assert_eq!(alone.iter().map(|r| r.owned.i).collect::<Vec<_>>(), vec![0]);
        assert_eq!(next_alone, 1);
        let (rows, next) = burns_past(&user, &ctx, &h, &HashSet::new(), 31);
        assert_eq!(rows.iter().map(|r| r.owned.i).collect::<Vec<_>>(), vec![0, 30]);
        assert_eq!(next, 31);
        // Reserved past the last burn: the next counter is the journal's.
        assert_eq!(burns_past(&user, &ctx, &h, &HashSet::new(), 40).1, 40);
    }
}
