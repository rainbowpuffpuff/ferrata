//! The compact witness formats a prover hands the guest (`ferrata_core::witness`) and the
//! attested header record (`attest::HeaderRecord`), from arbitrary bytes.
//!
//! No decoder panics, and every value a decoder accepts encodes back to exactly the input
//! (one encoding per value: the guest's digests of these objects must not have two preimages
//! that decode alike).
#![no_main]
use ferrata_core::{
    attest,
    witness::{AdoptionProgress, CommitteeSummary, CompactBootstrap, CompactStore, CompactUpdate, HeaderRecord, SignerWitness},
};
use ferrata_fuzz::{committee_size, lc_fork, Input};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut i = Input(data);
    let fork = lc_fork(i.u8());
    let n = committee_size(i.u8());
    let b = i.rest();

    if let Ok(s) = CompactStore::decode(b) {
        assert_eq!(s.encode(), b, "CompactStore");
        let _ = (s.digest(), s.is_next_sync_committee_known());
        if let Some(next) = s.fork.next() {
            let _ = s.upgraded(next);
        }
    }
    if let Ok(c) = CompactBootstrap::decode(b, fork) {
        assert_eq!(c.encode(fork), b, "CompactBootstrap");
    }
    if let Ok(layout) = CompactUpdate::layout_of(b) {
        if let Ok(u) = CompactUpdate::decode(b, layout, n) {
            assert_eq!(u.encode(layout), b, "CompactUpdate");
            let _ = (u.digest(layout), u.is_finality_update(), u.is_sync_committee_update());
        }
    }
    if let Ok(u) = CompactUpdate::decode(b, fork, n) {
        assert_eq!(u.encode(fork), b, "CompactUpdate (given fork)");
    }
    if let Ok(w) = SignerWitness::decode(b, n) {
        assert_eq!(w.encode(), b, "SignerWitness");
    }
    if let Ok(p) = AdoptionProgress::decode(b) {
        assert_eq!(p.encode(), b, "AdoptionProgress");
        let _ = p.next_part();
    }
    if let Ok(c) = CommitteeSummary::decode(b) {
        assert_eq!(c.encode(), b, "CommitteeSummary");
        let _ = c.root();
    }
    if let Ok(r) = HeaderRecord::decode(b) {
        assert_eq!(r.encode(), b, "witness::HeaderRecord");
        let chain_id = u64::from(fork as u8);
        let _ = attest::HeaderRecord::from_recorded(&r, chain_id);
    }
    if let Ok(r) = attest::HeaderRecord::from_ssz(b) {
        assert_eq!(&r.to_ssz()[..], b, "attest::HeaderRecord");
    }
});
