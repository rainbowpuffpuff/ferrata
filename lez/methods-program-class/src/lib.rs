//! The second build of the attestation program, built by risc0-build and embedded:
//! `FERRATA_LEZ_CLASS_ELF` / `FERRATA_LEZ_CLASS_ID` (`guests/program-class`, not deployed).
include!(concat!(env!("OUT_DIR"), "/methods.rs"));
