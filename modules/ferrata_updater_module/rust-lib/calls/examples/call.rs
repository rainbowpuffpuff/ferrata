//! Run one method natively and print its reply: `cargo run --example call -- METHOD [REQUEST_FILE]`.
//! `scripts/check.sh` compares these replies with the module's replies through Logos Core.
//! `daemon REQUEST_FILE` starts the daemon with that request (give it `rounds`), waits for it to end
//! and prints its status.

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
    use ferrata_updater_calls as c;
    let reply = match method.as_str() {
        "version" => c::version(),
        "check_config" => c::check_config(&request),
        "run" => c::run(&request),
        "daemon" => match c::Daemon::start(&request) {
            Ok(mut d) => {
                while d.running() {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                d.stop()
            }
            Err(reply) => reply,
        },
        _ => {
            eprintln!("usage: call version|check_config|run [REQUEST_FILE] (or daemon REQUEST_FILE)");
            std::process::exit(2)
        }
    };
    println!("{reply}");
}
