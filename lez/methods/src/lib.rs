//! The guests, built by risc0-build and embedded: `FERRATA_LEZ_ELF` / `FERRATA_LEZ_ID` (the
//! program) and `FERRATA_CONSUMER_ELF` / `FERRATA_CONSUMER_ID` (the Stage 3 consumer).
include!(concat!(env!("OUT_DIR"), "/methods.rs"));
