//! Exit codes: every refusal code has one owner. The program exits with its own codes and passes
//! on the core's and the step API's, so the blocks of `ferrata_lez_wire::CODE_BLOCKS` must not
//! overlap, and every code of the core, the step API, the program and the consumer must lie in a
//! block of its owner. A code added to any of them outside its block fails here.

use std::ops::RangeInclusive;

use ferrata_core::steps::StepError;
use ferrata_lez_wire::{code, consumer, CodeOwner, CODE_BLOCKS};

/// `ferrata_core::Error`'s variants and codes as declared in the core's source. The enum is
/// `#[non_exhaustive]`, so no other crate can enumerate it; reading the declaration means a
/// variant added to the core cannot escape this test.
fn core_codes_in_source() -> Vec<(String, u16)> {
    let source = include_str!("../../../crates/ferrata-core/src/error.rs");
    let body = source.split("pub enum Error {").nth(1).expect("the enum").split("\n}").next().unwrap();
    body.lines()
        .filter_map(|line| {
            let (name, value) = line.trim().split_once(" = ")?;
            name.chars().all(|c| c.is_ascii_alphanumeric()).then_some(())?;
            Some((name.to_string(), value.strip_suffix(',')?.parse().expect("a code")))
        })
        .collect()
}

/// The step API's own refusals.
const STEP_ERRORS: [StepError; 5] = [
    StepError::Witness,
    StepError::Phase,
    StepError::StoreChanged,
    StepError::AdoptionIncomplete,
    StepError::NoAdoption,
];

/// Does not compile once `StepError` gains a variant, so that `STEP_ERRORS` is extended.
#[allow(dead_code)]
fn every_step_error_is_listed(e: StepError) {
    match e {
        StepError::Core(_)
        | StepError::Witness
        | StepError::Phase
        | StepError::StoreChanged
        | StepError::AdoptionIncomplete
        | StepError::NoAdoption => {}
    }
}

/// Every refusal code, with its owner and name.
fn every_code() -> Vec<(CodeOwner, String, u16)> {
    let mut all = Vec::new();
    for (name, value) in core_codes_in_source() {
        all.push((CodeOwner::Core, format!("Error::{name}"), value));
    }
    for e in STEP_ERRORS {
        all.push((CodeOwner::StepApi, format!("StepError::{e:?}"), e.code()));
    }
    for (name, value) in code::ALL {
        all.push((CodeOwner::Program, format!("code::{name}"), u16::from(*value)));
    }
    for (name, value) in consumer::code::ALL {
        all.push((CodeOwner::Consumer, format!("consumer::code::{name}"), u16::from(*value)));
    }
    all
}

/// The first pair of blocks that share a code.
fn first_overlap(blocks: &[(CodeOwner, RangeInclusive<u8>)]) -> Option<(usize, usize)> {
    (0..blocks.len()).flat_map(|i| (i + 1..blocks.len()).map(move |j| (i, j))).find(|&(i, j)| {
        let (a, b) = (&blocks[i].1, &blocks[j].1);
        a.start() <= b.end() && b.start() <= a.end()
    })
}

#[test]
fn the_blocks_do_not_overlap() {
    assert_eq!(first_overlap(&CODE_BLOCKS), None, "{CODE_BLOCKS:?}");
    for (owner, block) in &CODE_BLOCKS {
        assert!(*block.start() >= 1 && block.start() <= block.end(), "{owner:?} {block:?}: 0 is success");
    }
    // The check itself: blocks sharing one code overlap.
    assert_eq!(first_overlap(&[(CodeOwner::Core, 100..=199), (CodeOwner::Program, 199..=212)]), Some((0, 1)));
}

#[test]
fn every_code_lies_in_a_block_of_its_owner() {
    let all = every_code();
    for (owner, name, value) in &all {
        let byte = u8::try_from(*value).unwrap_or_else(|_| panic!("{name} = {value} does not fit an exit code"));
        let block = CODE_BLOCKS.iter().find(|(_, b)| b.contains(&byte));
        assert_eq!(block.map(|(o, _)| *o), Some(*owner), "{name} = {value} is not in a block of {owner:?}");
    }
    let mut values: Vec<u16> = all.iter().map(|(_, _, v)| *v).collect();
    values.sort_unstable();
    values.dedup();
    assert_eq!(values.len(), all.len(), "a code is used twice");
    // The program's codes before the merge (100–112) are the core's execution-layer codes now.
    assert!(all.iter().any(|(o, _, v)| *o == CodeOwner::Core && (100..=112).contains(v)));
}

#[test]
fn the_tables_name_every_core_code() {
    // The list the tables name codes with (harness::exit_name) is the core's declaration.
    let listed: Vec<(String, u16)> = harness::CORE_ERRORS.iter().map(|e| (format!("{e:?}"), e.code())).collect();
    assert_eq!(listed, core_codes_in_source());
    for (owner, name, value) in every_code() {
        if owner != CodeOwner::Consumer {
            assert!(!harness::exit_name(u32::from(value)).starts_with("code "), "{name} has no name in the tables");
        }
    }
}
