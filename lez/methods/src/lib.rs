//! The guests, built by risc0-build and embedded: `FERRATA_LEZ_ELF` / `FERRATA_LEZ_ID` (the
//! program), `FERRATA_CONSUMER_ELF` / `FERRATA_CONSUMER_ID` (the reference consumer), LEZ's
//! `SCRIPTED_*` test program, and for the Keccak comparison `FERRATA_LEZ_KECCAK_SOFT_*` and
//! `FERRATA_CONSUMER_KECCAK_ACCEL_*`.
include!(concat!(env!("OUT_DIR"), "/methods.rs"));
