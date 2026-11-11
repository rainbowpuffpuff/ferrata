//! `ferrata-core`: the consensus-critical core of Ferrata (Logos RFP-021).
//!
//! An Ethereum sync-committee light client, Altair through Gloas, transcribed from the
//! light-client specs at `consensus-specs` v1.7.0-beta.4 and checked against that release's
//! official test vectors. `no_std` + `alloc`, so the same crate can run in a LEZ (RISC Zero)
//! guest, a Logos module, a CLI dry run and the tests.
//!
//! * [`validate::validate_light_client_update`]: the spec's update validation (pure).
//! * [`store::LightClientStore`]: the spec's full store, used to replay the test vectors.
//! * [`policy::FinalizedStore`]: Ferrata's on-chain policy, which accepts only finality
//!   updates signed by a two-thirds supermajority and never forces a header.
//! * [`bls::BlsBackend`]: BLS12-381 behind a trait, with `blst` and zkcrypto backends.
//! * [`ws`]: the weak-subjectivity period and the bound a store is advanced within.

#![no_std]
#![deny(unsafe_code)]
#![warn(missing_docs)]
#![allow(missing_docs)] // spike: field-level docs are deferred

extern crate alloc;

// Execution layer: RLP, Merkle-Patricia proofs, execution objects and attestations.
pub mod accumulator;
pub mod attest;
pub mod exec;
pub mod keccak;
pub mod mpt;
pub mod rlp;

pub mod bls;
pub mod error;
pub mod fork;
pub mod hash;
pub mod merkle;
pub mod policy;
mod ssz;
#[cfg(feature = "blst")]
pub mod steps;
pub mod store;
pub mod types;
pub mod upgrade;
pub mod validate;
pub mod witness;
pub mod ws;

pub use error::Error;

/// A 32-byte SSZ root or chunk.
pub type Root = [u8; 32];
