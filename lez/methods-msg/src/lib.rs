//! The messages programs, built by risc0-build and embedded: `FERRATA_MSG_ELF` / `FERRATA_MSG_ID` (`guests/msg`, v1, live
//! on the test network since 23 November 2026) and `FERRATA_MSG_V2_ELF` / `FERRATA_MSG_V2_ID` (`guests/msg-v2`).
include!(concat!(env!("OUT_DIR"), "/methods.rs"));
