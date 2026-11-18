//! `residue-host measure-handoff`: a committee handoff (a period update) in one execution, the next
//! committee adopted by its summary (`residue_handoff::adopt_summary`, `bench/RESIDUE.md` §10), in
//! the bench guest `guests/residue-agg-accel` (executor only, r0vm 3.0.5). Mainnet update_1 as served
//! (510 of 512, the real committee) against the store after update_0, and the same update signed by
//! the bench's committee (§5's method) with 512 and with 342 signers (every third member absent, and
//! every absent index odd: the costliest placement found, `results/residue/agg/placements.md`); the
//! residue-witness check (also folded) with one multiproof (blst's batch sum; also the chain of
//! additions); for comparison the same update with the core's 512-key adoption in the same
//! execution, and at 510 the final exponentiation with the core's composition. Every new store is
//! compared with the native core's (all its steps, the 512-key adoption included) and with
//! `residue_handoff::one_step` run natively. Three refusals in the guest. Experimental and
//! unaudited; sound only under the light client's honest-supermajority assumption. Outputs:
//! `results/residue/handoff/`.

use std::{fmt::Write as _, fs};

use ferrata_core::witness::{CompactStore, CompactUpdate};
use residue_agg::{NonSigners, Sum};

use super::{agg, bench_dir, get, native, report, Case};

/// In each of the 128 groups of four members (4g .. 4g+3): member `4g + one` absent, and in the
/// groups of `two_in` also member `4g + other` (as `tests/placements.rs`).
fn groups(one: usize, other: usize, two_in: std::ops::Range<usize>) -> Vec<bool> {
    let mut bits = vec![true; 512];
    for g in 0..128 {
        bits[4 * g + one] = false;
        if two_in.contains(&g) {
            bits[4 * g + other] = false;
        }
    }
    bits
}

const PATHS: [&str; 3] = ["final exponentiation", "witness", "witness folded"];
const COMPOSITIONS: [&str; 3] = ["core (a branch per non-signer)", "multiproof + batch sum", "multiproof + chain sum"];

pub fn measure() {
    let b = agg::base();
    let mut raw = String::new();
    let mut table = String::from(
        "| case | path | composition | adoption | S1 step 1 | S2+S3 (folded: one phase) | adoption | rest | whole update (cycle_count) | execution user cycles | of 10M | segments |\n|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    // (path, composition, adopted by the summary?)
    let main: Vec<(u32, u32, bool)> = vec![(1, 1, true), (2, 1, true), (1, 2, true), (1, 1, false)];
    let mut first = main.clone();
    first.extend([(2, 1, false), (0, 0, true)]);
    let fake = |label: &str, bits: Vec<bool>| Case {
        name: format!("period (handoff), update_1 signed by the bench's committee: {label}"),
        store: b.substituted.clone(),
        update: agg::signed(&b, &b.period, bits),
        committee: b.fake.committee.clone(),
    };
    let cases: Vec<(Case, Vec<(u32, u32, bool)>)> = vec![
        (
            Case {
                name: "period (handoff), mainnet update_1 as served: 510 of 512 (real committee)".into(),
                store: b.store.clone(),
                update: b.period.clone(),
                committee: b.real.clone(),
            },
            first,
        ),
        (fake("512 (all)", vec![true; 512]), main.clone()),
        (fake("342 (every third absent: the most hashes)", agg::every_third()), main.clone()),
        (
            fake(
                "342 (every absent index odd: 4g+3 in every group of four, 4g+1 in groups 0..41; the costliest placement found)",
                groups(3, 1, 0..42),
            ),
            main.clone(),
        ),
    ];
    let mut after_510 = None;
    for (case, runs) in &cases {
        assert_eq!(case.update.sync_aggregate.bits.len(), 512);
        let inp = native(case, &b.spec, &b.gvr);
        let multiproof = agg::agg_witness(case, &b.spec, &b.gvr);
        // Natively: the one-step path gives the core's store (the 512-key adoption included).
        let compact = CompactUpdate::of(&case.update);
        let signers = NonSigners::decode(&multiproof, 512).unwrap();
        for sum in [Sum::Batch, Sum::Chain] {
            let (next, outcome, job) = residue_handoff::one_step(
                &case.store,
                &case.store,
                &compact,
                &signers,
                case.update.signature_slot,
                &inp.c,
                &inp.w,
                &b.spec,
                &b.gvr,
                sum,
            )
            .unwrap_or_else(|e| panic!("{}: one step ({sum:?}): {e:?}", case.name));
            assert!(outcome.adopted && job.adoption.is_none(), "{}: a handoff, adopted by its summary", case.name);
            assert_eq!(next.encode(), inp.expected_store, "{}: one step ({sum:?}) = the core's steps", case.name);
        }
        after_510.get_or_insert_with(|| inp.expected_store.clone());
        for &(path, composition, summary) in runs {
            let (c, w, c_inv): (&[u8], &[u8], &[u8]) = match path {
                0 => (&[], &[], &[]),
                1 => (&inp.c, &inp.w, &[]),
                _ => (&inp.c, &inp.w, &inp.c_inv),
            };
            let signers: &[u8] = if composition == 0 { &inp.signers } else { &multiproof };
            let keys: &[u8] = if summary { &[] } else { &inp.keys };
            let run = agg::execute(
                agg::MODE_ONE_TX,
                &[path, composition],
                &[&inp.store, &inp.update, signers, keys, c, w, c_inv],
            );
            let adoption = if summary { "summary (no keys)" } else { "core: 512 keys" };
            let label =
                format!("{} / {} / {} / {adoption}", case.name, PATHS[path as usize], COMPOSITIONS[composition as usize]);
            assert_eq!(run.extra, inp.expected_store, "{label}: the guest's new store is the core's");
            report(&mut raw, &label, &run);
            let s1 = get(&run, "S1 ");
            let s23 = get(&run, "S2 ") + get(&run, "S3") + get(&run, "S23f ");
            let a = get(&run, "A0 ") + get(&run, "A1 ") + get(&run, "A2 ");
            let whole = get(&run, "T  ");
            let cycles = run.info.cycles();
            writeln!(
                table,
                "| {} | {} | {} | {adoption} | {s1} | {s23} | {a} | {} | {whole} | {cycles} | {:.1}% | {} |",
                case.name,
                PATHS[path as usize],
                COMPOSITIONS[composition as usize],
                whole - s1 - s23 - a,
                cycles as f64 / 100_000.0,
                run.info.segments.len()
            )
            .unwrap();
            println!("{label}: {cycles} user cycles");
        }
    }

    // Refusals in the guest (witness check, one multiproof, batch sum, summary adoption).
    let real = &cases[0].0;
    let inp = native(real, &b.spec, &b.gvr);
    let after = CompactStore::decode(&after_510.unwrap()).unwrap();
    let mut bad_branch = b.period.clone();
    bad_branch.next_sync_committee_branch[0][3] ^= 1;
    let below = agg::signed(&b, &b.period, (0..512).map(|i| i >= 171).collect());
    let refusals: [(&str, &CompactStore, &ferrata_core::types::LightClientUpdate, &ferrata_core::types::SyncCommittee); 3] = [
        ("a node of the next-committee branch with one bit flipped", &b.store, &bad_branch, &b.real),
        ("341 of 512 signers (the bench's committee)", &b.substituted, &below, &b.fake.committee),
        ("the same handoff again, on the store it produced", &after, &b.period, &b.real),
    ];
    for (label, store, update, committee) in refusals {
        let compact = CompactUpdate::of(update);
        let signers = NonSigners::build(committee, &update.sync_aggregate.bits).unwrap();
        let want = residue_agg::step1(store, &compact, &signers, update.signature_slot, &b.spec, &b.gvr, 1, Sum::Batch)
            .unwrap_err()
            .code();
        let run = agg::execute(
            agg::MODE_ONE_TX,
            &[1, 1],
            &[&store.encode(), &compact.encode(store.fork), &signers.encode(), &[], &inp.c, &inp.w, &[]],
        );
        assert_eq!(run.extra, vec![0xff, want as u8], "{label}: refused in the guest with the native code");
        report(&mut raw, &format!("REFUSAL (code {want}): {label}"), &run);
        println!("refused in the guest: {label}: code {want}");
    }

    let dir = bench_dir().join("results/residue/handoff");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("one-tx.txt"), &raw).unwrap();
    fs::write(dir.join("tables.md"), &table).unwrap();
    println!("{table}");
}
