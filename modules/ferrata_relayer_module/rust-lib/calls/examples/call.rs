//! Run one method natively and print its reply: `cargo run --example call -- METHOD [REQUEST_FILE]`.
//! `scripts/check.sh` compares these replies with the module's replies through Logos Core;
//! `scripts/relayer-module.sh` with `CALL=native` runs the daemon through it.

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
    use ferrata_relayer_calls as c;
    let reply = match method.as_str() {
        "version" => c::version(),
        "run" => c::run(&request),
        _ => {
            eprintln!("usage: call version|run [REQUEST_FILE]");
            std::process::exit(2)
        }
    };
    println!("{reply}");
}
