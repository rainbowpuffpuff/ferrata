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
        "version" => ferrata_bridge_calls::version(),
        "deposit" => ferrata_bridge_calls::deposit(&request),
        "claim" => ferrata_bridge_calls::claim(&request),
        "burn" => ferrata_bridge_calls::burn(&request),
        "release" => ferrata_bridge_calls::release(&request),
        "recover" => ferrata_bridge_calls::recover(&request),
        "status" => ferrata_bridge_calls::status(&request),
        "prove_release" => ferrata_bridge_calls::prove_release(&request),
        "import_root" => ferrata_bridge_calls::import_root(&request),
        "registry" => ferrata_bridge_calls::registry(&request),
        "admin" => ferrata_bridge_calls::admin(&request),
        _ => {
            eprintln!("usage: call version|deposit|claim|burn|release|recover|status|prove_release|import_root|registry|admin [REQUEST_FILE]");
            std::process::exit(2)
        }
    };
    println!("{reply}");
}
