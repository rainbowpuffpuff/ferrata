//! Run one method natively and print its reply: `cargo run --example call -- METHOD [REQUEST_FILE]`.
//! `scripts/check.sh` compares these replies with the module's replies through Logos Core.

fn main() {
    let mut args = std::env::args().skip(1);
    let method = args.next().unwrap_or_default();
    let request = match args.next() {
        Some(path) => std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("call: cannot read {path}: {e}");
            std::process::exit(2)
        }),
        None => String::new(),
    };
    let reply = match method.as_str() {
        "version" => ferrata_attest_calls::version(),
        "dry_run_attest" => ferrata_attest_calls::dry_run_attest(&request),
        // FERRATA_HTTP_RECORD=FILE: every provider request and response of the call, as JSON (the tests' fixtures).
        "verify_checkpoint" => match std::env::var("FERRATA_HTTP_RECORD") {
            Ok(path) => {
                use ferrata_attest_calls::providers::{Curl, Recording};
                let timeout = serde_json::from_str::<serde_json::Value>(&request)
                    .ok()
                    .and_then(|r| r["timeout_secs"].as_u64())
                    .unwrap_or(20);
                let mut rec = Recording { inner: Curl { timeout_secs: timeout }, log: Vec::new() };
                let reply = ferrata_attest_calls::verify_checkpoint_with(&request, &mut rec);
                let log = serde_json::to_string_pretty(&rec.log).unwrap_or_default();
                std::fs::write(&path, log + "\n").unwrap_or_else(|e| eprintln!("call: cannot write {path}: {e}"));
                reply
            }
            Err(_) => ferrata_attest_calls::verify_checkpoint(&request),
        },
        "fetch_log_proof" => ferrata_attest_calls::fetch_log_proof(&request),
        "build_attest_tx" => ferrata_attest_calls::build_attest_tx(&request),
        "read_attestation" => ferrata_attest_calls::read_attestation(&request),
        "read_instance" => ferrata_attest_calls::read_instance(&request),
        "advance" => ferrata_attest_calls::advance(&request),
        _ => {
            eprintln!("usage: call version|dry_run_attest|verify_checkpoint|fetch_log_proof|build_attest_tx|read_attestation|read_instance|advance [REQUEST_FILE]");
            std::process::exit(2)
        }
    };
    println!("{reply}");
}
