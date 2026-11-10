//! Fidelity of `checkpoint-core` to the real formats, and the bench fixtures.
//!
//! - [`lez_build`]: LEZ v0.4.0-rc1 chains and transactions built with LEZ's crates.
//! - [`bedrock`]: inscriptions and channel configuration changes built with
//!   `logos-blockchain-core` at the revision LEZ pins.
//! - [`dump`]: the real blocks in LEZ's committed sequencer database dump.
//! - [`scenario`]: deployments, inscribed chains and checkpoint inputs.

pub mod bedrock;
pub mod dump;
pub mod lez_build;
pub mod scenario;
