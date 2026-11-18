//! The second build of the bridge program (`guests/bridge-next`, not deployed), built by risc0-build and embedded:
//! `FERRATA_BRIDGE_NEXT_ELF` / `FERRATA_BRIDGE_NEXT_ID`. The harness runs its suites against it with the feature
//! `bridge-next` (`lez/results/bridge-next.md`).
include!(concat!(env!("OUT_DIR"), "/methods.rs"));
