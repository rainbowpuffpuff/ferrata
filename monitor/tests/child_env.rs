//! The watcher key given by `--key-env` is not inherited by any command the monitor runs (internal review of
//! 27 November 2026, docs/security/internal-review-2026-11-27.md, C-4): the monitor read the variable and left it set,
//! and every alert command (`--alert-cmd`) and node command (`--node cmd:…`) ran under `sh -c` with the environment
//! inherited, so any of them could read the key that cancels, pauses and revokes. A control variable still reaches
//! them. (A command running as the monitor's user can still read the monitor's `/proc/<pid>/environ`; the README says
//! to prefer `--key-file` or the keystore where that matters.)

use clap::Parser;
use ferrata_monitor::alert::{Alerts, Level};
use ferrata_monitor::cli::{signer, RunArgs};
use ferrata_monitor::node::NodeSource;

#[derive(Parser)]
struct Args {
    #[command(flatten)]
    run: RunArgs,
}

#[test]
fn the_watcher_key_reaches_no_alert_or_node_command() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("child-env");
    std::fs::create_dir_all(&dir).unwrap();
    let (alert_env, node_env) = (dir.join("alert.env"), dir.join("node.env"));
    let key = "0x1111111111111111111111111111111111111111111111111111111111111111";
    std::env::set_var("FERRATA_TEST_WATCHER_KEY", key);
    std::env::set_var("FERRATA_TEST_CONTROL", "still-here");
    let a = Args::parse_from([
        "monitor",
        "--rpc",
        "http://127.0.0.1:9",
        "--vault",
        "0x0000000000000000000000000000000000000001",
        "--node",
        "file:unused.json",
        "--state",
        "unused",
        "--key-env",
        "FERRATA_TEST_WATCHER_KEY",
    ])
    .run;
    assert!(signer(&a).unwrap().is_some(), "the key is read");

    let alerts =
        Alerts { log: None, cmd: Some(format!("env > {}", alert_env.display())), cmd_level: Some(Level::Info) };
    alerts.emit(Level::Critical, "test", 0, serde_json::json!({}));
    let node: NodeSource = format!("cmd:env > {}; exit 1", node_env.display()).parse().unwrap();
    assert!(node.read().is_err(), "the node command exits 1");

    for f in [&alert_env, &node_env] {
        let seen = std::fs::read_to_string(f).unwrap();
        assert!(seen.contains("FERRATA_TEST_CONTROL=still-here"), "{} ran with the environment", f.display());
        assert!(
            !seen.contains(&key[2..]) && !seen.contains("FERRATA_TEST_WATCHER_KEY"),
            "{} saw the watcher key",
            f.display()
        );
    }
}
