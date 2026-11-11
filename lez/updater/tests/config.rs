//! The configuration files this crate ships parse, and the `updates` stream's two length
//! conventions both split.

use std::path::PathBuf;

use ferrata_updater::{chain::split_chunks, config::File};

fn here(p: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(p)
}

#[test]
fn shipped_configurations_parse() {
    let example = File::load(&here("ferrata-updater.example.toml")).unwrap();
    assert_eq!(example.chains.len(), 2);
    assert_eq!(example.fees.gas_limit, 4_800_000);
    assert_eq!(example.policy.defer_window, 8);
    assert!(example.chains.iter().all(|c| c.sources.len() == 2 && c.sources.iter().all(|s| s.url.is_some())));
    let dev = File::load(&here("dev/dev.toml")).unwrap();
    assert_eq!(dev.daemon.ledger, "harness");
    assert_eq!(dev.policy.min_agreeing, 2);
}

#[test]
fn bad_configurations_are_refused() {
    let base = std::fs::read_to_string(here("dev/dev.toml")).unwrap();
    for (from, to, why) in [
        ("preset = \"mainnet\"", "preset = \"holesky\"", "unknown preset"),
        ("checkpoint = \"4baa", "checkpoint = \"zz4baa", "checkpoint not hex"),
        ("defer_window = 1", "defer_window = 0", "zero deferral window"),
        (
            "dir = \"../../lab/results/record/net/mainnet\"\n\n  [[chain.source]]",
            "url = \"http://x\"\n  dir = \"y\"\n\n  [[chain.source]]",
            "url and dir",
        ),
        ("[keys]", "[keys]\nunknown_field = 1", "unknown field"),
    ] {
        let text = base.replacen(from, to, 1);
        assert_ne!(text, base, "{why}: the substitution applies");
        assert!(File::parse(&text).is_err(), "{why} must be refused");
    }
}

#[test]
fn updates_stream_splits_with_either_length_convention() {
    let (a, b) = (vec![1u8; 300], vec![2u8; 500]);
    for counts_digest in [true, false] {
        let mut raw = Vec::new();
        for u in [&a, &b] {
            let n = if counts_digest { u.len() + 4 } else { u.len() };
            raw.extend_from_slice(&(n as u64).to_le_bytes());
            raw.extend_from_slice(&[0, 1, 2, 3]);
            raw.extend_from_slice(u);
        }
        assert_eq!(split_chunks(&raw), vec![a.clone(), b.clone()], "counts_digest {counts_digest}");
    }
    assert!(split_chunks(&[1, 2, 3]).is_empty());
}
