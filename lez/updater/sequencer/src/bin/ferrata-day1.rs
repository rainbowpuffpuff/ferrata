//! The day-one deployment's transactions on a LEZ v0.4 network: `src/day1.rs`; driven by `lez/testnet04/day1.sh`.
fn main() -> anyhow::Result<()> {
    ferrata_updater_sequencer::day1::main()
}
