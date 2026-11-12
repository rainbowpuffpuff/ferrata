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
    use ferrata_monitor_calls as c;
    let reply = match method.as_str() {
        "version" => c::version(),
        "check" => c::check(&request),
        "node_view" => c::node_view(&request),
        "run" => c::run(&request),
        _ => {
            eprintln!("usage: call version|check|node_view|run [REQUEST_FILE]");
            std::process::exit(2)
        }
    };
    println!("{reply}");
}
