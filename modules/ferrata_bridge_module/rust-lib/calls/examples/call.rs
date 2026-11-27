//! Run one method natively and print its reply: `cargo run --example call -- METHOD [REQUEST_FILE]`.
//! `scripts/check.sh` compares these replies with the module's replies through Logos Core.
//! `call wallet FILE`: the mini-app's calls in one session, one `{"method", "params"}` per line of FILE, each reply
//! printed on a line of its own (`#` lines are comments). The network session is driven as the mini-app's view drives
//! it: a line may name a `client` (a view of its own, `as` a caller: by default `module:ferrata_bridge_ui`), whose
//! `open_network` seals `params.phrase` to the module's one-time key (no phrase: a new one, its words not printed) and
//! whose later calls carry their counter and MAC; a line with `"raw": true` is sent as it is, `as` its caller (another
//! module, or the view without the session's key). After a claim or burn, `"pending_id": "$previous"` names its
//! approval; after `confirm`, `"id": "$previous"` names its job. `"wait": true` on `confirm` waits for that job.

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
        "wallet" => {
            for reply in ferrata_bridge_calls::wallet_calls(&request) {
                println!("{reply}");
            }
            return;
        }
        _ => {
            eprintln!("usage: call version|deposit|claim|burn|release|recover|status|prove_release|import_root|registry|admin|wallet [REQUEST_FILE]");
            std::process::exit(2)
        }
    };
    println!("{reply}");
}
