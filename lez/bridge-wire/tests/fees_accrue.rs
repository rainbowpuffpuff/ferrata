//! A deterministic randomized model of ADR 0048 fee collection, checked against [`FeesState::accrue`].
//!
//! The model grows a 256-bit vault total, takes records of it, and collects the newest or an older record with
//! varied limits. It checks every mint and refusal, conservation of the minted sum, monotonicity, full catch-up,
//! and totals around 2^128 and 2^255.

use ferrata_lez_bridge_wire::{code, u128_of, word_add_u128, word_of_u128, word_sub, FeesState, Word};

const SEQUENCES: usize = 320;
const STEPS: usize = 200;
const ZERO: Word = [0u8; 32];

/// Xorshift64*, with a different fixed seed for each sequence.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

struct Model {
    total: Word,
    records: Vec<Word>,
    state: FeesState,
    minted: Word,
    largest_record_used: Word,
}

impl Model {
    fn new(total: Word) -> Self {
        Model {
            total,
            records: vec![total],
            state: FeesState::new([7; 32], [2; 20]),
            minted: ZERO,
            largest_record_used: ZERO,
        }
    }

    fn grow(&mut self, amount: u128) {
        self.total = word_add_u128(&self.total, amount).expect("the model total stays below 2^256");
    }

    fn record(&mut self) {
        self.records.push(self.total);
    }

    fn collect(&mut self, sequence: usize, step: usize, record_index: usize, limit: u128) {
        let before = self.state;
        let total = self.records[record_index];
        self.largest_record_used = self.largest_record_used.max(total);
        let due = word_sub(&total, &before.collected);
        let refused = limit == 0 || due.as_ref().map_or(true, |d| *d == ZERO);
        let full_catch_up =
            record_index + 1 == self.records.len() && limit == u128::MAX && due.as_ref().and_then(u128_of).is_some();
        let result = before.accrue(&total, limit);

        if limit == 0 {
            assert_eq!(
                result,
                Err(code::BAD_MESSAGE),
                "sequence {sequence}, step {step}: a zero limit is refused before the record is considered"
            );
        } else {
            match due {
                None => assert_eq!(
                    result,
                    Err(code::IMPORT_MISMATCH),
                    "sequence {sequence}, step {step}: an older record is refused"
                ),
                Some(due) if due == ZERO => assert_eq!(
                    result,
                    Err(code::ALREADY_WRITTEN),
                    "sequence {sequence}, step {step}: an already collected record is refused"
                ),
                Some(due) => {
                    let expected_amount = u128_of(&due).map_or(limit, |d| d.min(limit));
                    let (amount, after) = result.unwrap_or_else(|error| {
                        panic!("sequence {sequence}, step {step}: collection refused with {error}")
                    });
                    assert_eq!(
                        amount, expected_amount,
                        "sequence {sequence}, step {step}: amount must be min(total - collected, limit)"
                    );
                    let expected_collected =
                        word_add_u128(&before.collected, amount).expect("a collection cannot overflow its total");
                    assert_eq!(
                        after.collected, expected_collected,
                        "sequence {sequence}, step {step}: collected must advance by exactly the minted amount"
                    );
                    assert!(after.collected >= before.collected, "sequence {sequence}, step {step}: count decreased");
                    assert!(after.collected <= total, "sequence {sequence}, step {step}: count exceeds its record");
                    assert_eq!(after.config_id, before.config_id, "collection changed config_id");
                    assert_eq!(after.token, before.token, "collection changed token");
                    self.minted = word_add_u128(&self.minted, amount).expect("the minted sum equals the 256-bit count");
                    self.state = after;
                }
            }
        }

        if refused {
            assert_eq!(self.state, before, "sequence {sequence}, step {step}: a refusal leaves state unchanged");
        }
        assert!(
            self.state.collected >= before.collected,
            "sequence {sequence}, step {step}: collected never decreases"
        );
        assert_eq!(
            self.minted, self.state.collected,
            "sequence {sequence}, step {step}: the sum of minted amounts equals collected"
        );
        assert!(
            self.state.collected <= self.largest_record_used,
            "sequence {sequence}, step {step}: collected exceeds the largest record used"
        );
        if full_catch_up {
            assert_eq!(
                self.state.collected, total,
                "sequence {sequence}, step {step}: an unlimited fitting collection catches up to the newest record"
            );
        }
    }

    /// Finish with unlimited collections of the newest record. A fitting amount due catches up in the last call.
    fn finish_at_newest_record(&mut self, sequence: usize) {
        self.record();
        let newest = self.records.len() - 1;
        for round in 0..3 {
            let due = word_sub(&self.total, &self.state.collected).expect("the newest record is not below collected");
            let fits = u128_of(&due).is_some();
            self.collect(sequence, STEPS + round, newest, u128::MAX);
            if fits {
                return;
            }
        }
        // A total around 2^255 takes around 2^127 u128-sized mints; leave those sequences partially collected.
    }
}

/// Start at zero or a small total, just below 2^128, or just above 2^255.
fn initial_total(sequence: usize, unit: u128, rng: &mut Rng) -> Word {
    match sequence % 6 {
        0 => ZERO,
        1 => word_of_u128((rng.next() as u128 + 1) * unit),
        2 | 4 => {
            let two_to_128 = word_add_u128(&word_of_u128(u128::MAX), 1).expect("2^128 fits");
            word_sub(&two_to_128, &word_of_u128(unit)).expect("the unit is below 2^128")
        }
        3 | 5 => {
            // 2^255 (the word's top bit) plus a small total: 200 growths of at most 2^64 units of 10^12 stay far below
            // 2^256.
            let mut total = word_of_u128(rng.next() as u128 * unit);
            total[0] = 0x80;
            total
        }
        _ => unreachable!(),
    }
}

fn collection_limit(rng: &mut Rng, unit: u128) -> u128 {
    match rng.below(20) {
        0..=4 => u128::MAX,
        5 => 0,
        6..=10 => (rng.below(4) as u128 + 1) * unit,
        _ => (rng.next() as u128 + 1) * unit,
    }
}

#[test]
fn accrue_matches_growing_vault_totals() {
    let units = [1, 1_000_000, 1_000_000_000_000];
    for sequence in 0..SEQUENCES {
        let seed = 0xfee5_0048_9e37_79b9u64 ^ (sequence as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let mut rng = Rng(if seed == 0 { 1 } else { seed });
        let unit = units[rng.below(units.len())];
        let crosses_128 = matches!(sequence % 6, 2 | 4);
        let above_255 = matches!(sequence % 6, 3 | 5);
        let mut model = Model::new(initial_total(sequence, unit, &mut rng));

        for step in 0..STEPS {
            // Force one growth and record so the 2^128 sequences actually cross the boundary.
            let forced_growth = step == 0 && crosses_128;
            if forced_growth || rng.below(100) < 45 {
                model.grow((rng.next() as u128 + 1) * unit);
                if forced_growth || rng.below(2) == 0 {
                    model.record();
                }
                if forced_growth {
                    assert!(u128_of(&model.total).is_none(), "sequence {sequence}: total did not cross 2^128");
                }
            } else {
                let record_index = if model.records.len() > 1 && rng.below(4) == 0 {
                    rng.below(model.records.len() - 1)
                } else {
                    model.records.len() - 1
                };
                model.collect(sequence, step, record_index, collection_limit(&mut rng, unit));
            }
        }
        model.finish_at_newest_record(sequence);
        if above_255 {
            assert!(model.total[0] & 0x80 != 0, "sequence {sequence}: the total is not above 2^255");
        }
    }
}
