//! The recorded-statement program and its reader, built by risc0-build and embedded:
//! `FERRATA_STATEMENTS_ELF` / `FERRATA_STATEMENTS_ID` (`guests/statements`) and `FERRATA_STATEMENTS_READER_ELF` /
//! `FERRATA_STATEMENTS_READER_ID` (`guests/statements-reader`). Neither is deployed anywhere.
include!(concat!(env!("OUT_DIR"), "/methods.rs"));
