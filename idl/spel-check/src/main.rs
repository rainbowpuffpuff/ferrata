//! SPEL's own code reads Ferrata's IDL: `spel-framework-core` 0.7.0 (logos-co/spel at c6f73ce)
//! deserializes `idl/ferrata_attestation.idl.json` and `idl/ferrata_bridge.idl.json` into its
//! `SpelIdl`, and decodes every committed vector (and, given a file, every recorded message) with
//! its own borsh decoder, requiring every byte to be consumed (`decode_account_data_try_all` over
//! an IDL whose `accounts` holds just the vector's type).
//!
//! `cargo run --locked -- [RECORDED.json]` (from `idl/spel-check`); exit status 1 on any failure.

use std::{fs, path::PathBuf, process::ExitCode};

use spel_framework_core::{
    decode::decode_account_data_try_all,
    idl::{IdlAccountType, SpelIdl},
};

fn exact(idl: &SpelIdl, type_name: &str, bytes: &[u8]) -> Result<serde_json::Value, String> {
    let def = idl
        .accounts
        .iter()
        .find(|a| a.name == type_name)
        .map(|a| a.type_.clone())
        .or_else(|| idl.types.iter().find(|t| t.name == type_name).cloned())
        .ok_or_else(|| format!("type {type_name} is not in the IDL"))?;
    let mut one = idl.clone();
    one.accounts = vec![IdlAccountType { name: type_name.to_string(), type_: def }];
    decode_account_data_try_all(bytes, &one).map(|(_, v)| v).ok_or_else(|| "SPEL's decoder refuses it".to_string())
}

fn check(idl: &SpelIdl, label: &str, path: &PathBuf) -> (usize, usize) {
    let file: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).expect("vectors")).expect("JSON");
    let (mut ok, mut n) = (0, 0);
    for v in file["vectors"].as_array().expect("vectors") {
        n += 1;
        let bytes = hex::decode(v["hex"].as_str().unwrap()).unwrap();
        match exact(idl, v["type"].as_str().unwrap(), &bytes) {
            Ok(json) => {
                ok += 1;
                if ok == 1 {
                    let text = json.to_string();
                    println!("  first: {} -> {}", v["name"].as_str().unwrap(), &text[..text.len().min(160)]);
                }
            }
            Err(e) => println!("  FAIL {}: {e}", v["name"].as_str().unwrap()),
        }
    }
    println!("  {label}: SPEL decoded {ok} of {n} exactly");
    (ok, n)
}

fn main() -> ExitCode {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let recorded = std::env::args().nth(1).map(PathBuf::from);
    let mut failed = 0;
    for name in ["ferrata_attestation", "ferrata_bridge"] {
        let text = fs::read_to_string(dir.join(format!("{name}.idl.json"))).expect("IDL");
        let idl: SpelIdl = match serde_json::from_str(&text) {
            Ok(idl) => idl,
            Err(e) => {
                println!("{name}: SPEL's SpelIdl refuses the IDL: {e}");
                failed += 1;
                continue;
            }
        };
        println!(
            "{name}: parsed as SPEL's SpelIdl: {} instructions, {} account types, {} types, {} errors, instruction_type {:?}",
            idl.instructions.len(),
            idl.accounts.len(),
            idl.types.len(),
            idl.errors.len(),
            idl.instruction_type
        );
        let (ok, n) = check(&idl, "committed vectors", &dir.join(format!("vectors/{name}.vectors.json")));
        failed += n - ok;
        if let (Some(path), "ferrata_attestation") = (&recorded, name) {
            let (ok, n) = check(&idl, "recorded messages", path);
            failed += n - ok;
        }
    }
    println!("SPEL check: {}", if failed == 0 { "passed".to_string() } else { format!("{failed} failure(s)") });
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
