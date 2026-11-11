//! Compiles the bridge guest's own source (`lez/guests/bridge/src/main.rs`) into this crate, so
//! the fuzz targets drive the program's `handle` exactly as written. `include!` does not accept
//! inner doc comments, so the copy turns the file's leading `//!` lines into `//` lines; nothing
//! else changes.
use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let guest = manifest.join("../../lez/guests/bridge/src/main.rs");
    println!("cargo:rerun-if-changed={}", guest.display());
    let source = fs::read_to_string(&guest).expect("the bridge guest's source");
    let copy: String = source
        .lines()
        .map(|line| match line.strip_prefix("//!") {
            Some(rest) => format!("//{rest}\n"),
            None => format!("{line}\n"),
        })
        .collect();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bridge_guest.rs");
    fs::write(out, copy).unwrap();
}
