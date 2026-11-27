//! The configuration files this crate ships parse, and the `updates` stream's two length
//! conventions both split.

use std::path::PathBuf;

use ferrata_lez_wire::instance_id;
use ferrata_updater::{
    chain::{instance_config, root, split_chunks},
    config::{url_host, ClientMatch, File, SourceKind},
    source::{client_family, parse_header_answer, parse_version_answer},
    utc,
};

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
    assert!(example
        .chains
        .iter()
        .all(|c| c.sources[0].kind == SourceKind::LightClient && c.sources[1].kind == SourceKind::Headers));
    assert_eq!(example.daemon.ledger, "sequencer");
    assert_eq!(example.sequencer.as_ref().unwrap().rpc, "http://127.0.0.1:3040");
    assert!(example.policy.finish_own_jobs);
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
        ("defer_window = 1", "defer_window = 8\njoin_wait_rounds = 11", "join_wait_rounds below defer_window + 4"),
        ("defer_window = 1", "defer_window = 1\njoin_wait_rounds = 1000001", "join_wait_rounds without bound"),
        (
            "dir = \"../../lab/results/record/net/mainnet\"\n\n  [[chain.source]]",
            "url = \"http://x\"\n  dir = \"y\"\n\n  [[chain.source]]",
            "url and dir",
        ),
        ("[keys]", "[keys]\nunknown_field = 1", "unknown field"),
        ("\nledger = \"harness\"", "\nledger = \"sequencer\"", "sequencer without [sequencer]"),
        (
            "\nledger = \"harness\"",
            "\nledger = \"sequencer\"\n[sequencer]\nrpc = \"http://10.0.0.1:3040\"\nprogram = \"x\"",
            "a sequencer that is not on this machine",
        ),
        ("\nledger = \"harness\"", "\nledger = \"chain\"", "unknown ledger"),
        ("adoption_parts = 2", "adoption_parts = 2\nlayout = \"bellatrix\"", "unknown layout"),
        (
            "name = \"record-a\"\n  dir",
            "name = \"record-a\"\n  kind = \"headers\"\n  dir",
            "a headers source without url",
        ),
        (
            "name = \"record-a\"\n  dir = \"../../lab/results/record/net/mainnet\"",
            "name = \"record-a\"\n  via = \"beacon_api_module\"",
            "beacon_api_module without chain_id",
        ),
        (
            "name = \"record-a\"\n  dir = \"../../lab/results/record/net/mainnet\"",
            "name = \"record-a\"\n  via = \"eth_rpc_module\"\n  chain_id = 1",
            "an unknown via",
        ),
        (
            "name = \"record-a\"\n  dir",
            "name = \"record-a\"\n  via = \"beacon_api_module\"\n  chain_id = 1\n  dir",
            "via and dir",
        ),
    ] {
        let text = base.replacen(from, to, 1);
        assert_ne!(text, base, "{why}: the substitution applies");
        assert!(File::parse(&text).is_err(), "{why} must be refused");
    }
}

#[test]
fn a_beacon_api_module_source_parses() {
    let base = std::fs::read_to_string(here("dev/dev.toml")).unwrap();
    let text = base.replacen(
        "name = \"record-a\"\n  dir = \"../../lab/results/record/net/mainnet\"",
        "name = \"record-a\"\n  via = \"beacon_api_module\"\n  chain_id = 1\n  provider = \"lodestar\"",
        1,
    );
    assert_ne!(text, base);
    let f = File::parse(&text).unwrap();
    let s = &f.chains[0].sources[0];
    assert_eq!(
        (s.via.as_deref(), s.chain_id, s.provider.as_deref()),
        (Some("beacon_api_module"), Some(1), Some("lodestar"))
    );
}

/// An execution source (`kind = "execution"`): a JSON-RPC `url`, or `via = "eth_rpc_module"` with `chain_id`; refused
/// as a recorded `dir`, through `beacon_api_module`, with a `provider`, or as the only kind of source a chain has.
#[test]
fn an_execution_source_parses_and_its_misuses_are_refused() {
    let base = std::fs::read_to_string(here("dev/dev.toml")).unwrap();
    let add = |extra: &str| format!("{base}\n  [[chain.source]]\n  name = \"el\"\n  kind = \"execution\"\n{extra}\n");
    let f = File::parse(&add("  url = \"https://rpc.sepolia.ethpandaops.io\"")).unwrap();
    let el = &f.chains[0].sources[2];
    assert_eq!((el.kind, el.url.as_deref()), (SourceKind::Execution, Some("https://rpc.sepolia.ethpandaops.io")));
    let f = File::parse(&add("  via = \"eth_rpc_module\"\n  chain_id = 1")).unwrap();
    let el = &f.chains[0].sources[2];
    assert_eq!((el.kind, el.via.as_deref(), el.chain_id), (SourceKind::Execution, Some("eth_rpc_module"), Some(1)));
    for (extra, why) in [
        ("  dir = \"../../lab/results/record/net/mainnet\"", "a recorded dir"),
        ("  via = \"beacon_api_module\"\n  chain_id = 1", "through beacon_api_module"),
        ("  via = \"eth_rpc_module\"", "eth_rpc_module without chain_id"),
        ("  url = \"https://x\"\n  provider = \"lodestar\"", "a provider"),
        ("  url = \"https://x\"\n  via = \"eth_rpc_module\"\n  chain_id = 1", "url and via"),
    ] {
        assert!(File::parse(&add(extra)).is_err(), "an execution source with {why} must be refused");
    }
    // No light-client source at all: the two recorded sources made a headers and an execution source.
    let none = add("  url = \"https://x\"")
        .replacen("name = \"record-a\"\n  dir = \"../../lab/results/record/net/mainnet\"", "name = \"record-a\"\n  url = \"https://h\"\n  kind = \"headers\"", 1)
        .replacen("name = \"record-b\"\n  dir = \"../../lab/results/record/net/mainnet\"", "name = \"record-b\"\n  url = \"https://i\"\n  kind = \"headers\"", 1);
    let e = File::parse(&none).unwrap_err();
    assert!(format!("{e:#}").contains("no light-client source"), "{e:#}");
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

/// The configuration of `ferrata-updater.example.toml`'s Sepolia chain (the 12 Nov 2026 checkpoint, layout gloas) is
/// the one crown-driver's `sepolia_base` initialises from the same fixture: its instance id is the instance of
/// `e2e/runs/crown-sessiona-sepolia-1` (`lez/session-a/export/instances.tsv`), so the updater continues that instance.
#[test]
fn sepolia_gloas_instance_is_the_crown_instance() {
    let example = File::load(&here("ferrata-updater.example.toml")).unwrap();
    let c = &example.chains[0];
    let config = instance_config(&c.preset, c.adoption_parts, c.layout.as_deref()).unwrap();
    assert_eq!(config.store_fork, 4);
    let id = instance_id(&config, &root(&c.checkpoint).unwrap());
    assert_eq!(hex::encode(id), "a5fb2591db162d5a1a8dd56c83c18323a84cb80764fdef585ac0afbb1b9af2b5");
    // The preset's own layout (Electra) is another instance.
    let electra = instance_config(&c.preset, c.adoption_parts, None).unwrap();
    assert_ne!(instance_id(&electra, &root(&c.checkpoint).unwrap()), id);
}

#[test]
fn header_answers_and_utc() {
    let body = r#"{"execution_optimistic":false,"finalized":true,"data":{"root":"0x3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c","canonical":true,"header":{"message":{"slot":"11562112","proposer_index":"520"}}}}"#;
    let want = root("3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c").unwrap();
    assert_eq!(parse_header_answer(body, 11_562_112).unwrap(), Some(want));
    assert_eq!(parse_header_answer(body, 11_562_113).unwrap(), None, "another slot");
    assert_eq!(
        parse_header_answer(&body.replace("\"canonical\":true", "\"canonical\":false"), 11_562_112).unwrap(),
        None
    );
    assert!(parse_header_answer("not json", 1).is_err());
    assert_eq!(utc(1_794_481_349), "2026-11-12T11:02:29Z");
    assert_eq!(utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(utc(1_709_208_000), "2024-02-29T12:00:00Z");
}

/// `policy.min_distinct_clients`: off by default; 1 is refused; at least that many sources must be able to count (a node
/// reports its client; a recorded dir needs `client`; a `headers` source counts only with `client_match =
/// "finalized-header"`); `client` is for a recorded dir only.
#[test]
fn client_diversity_configuration() {
    let base = std::fs::read_to_string(here("dev/dev.toml")).unwrap();
    let rec_a = "name = \"record-a\"\n  dir = \"../../lab/results/record/net/mainnet\"";
    let rec_b = "name = \"record-b\"\n  dir = \"../../lab/results/record/net/mainnet\"";
    let with = |policy: &str, a: &str, b: &str| {
        base.replacen("max_periods_per_request = 16", &format!("max_periods_per_request = 16\n{policy}"), 1)
            .replacen(rec_a, a, 1)
            .replacen(rec_b, b, 1)
    };
    let nimbus = format!("{rec_a}\n  client = \"Nimbus/v26.10.2-5628de-stateofus\"");
    let lodestar = format!("{rec_b}\n  client = \"Lodestar/v1.51.0/7923280\"");
    let url_lc = "name = \"record-b\"\n  url = \"https://lodestar-mainnet.chainsafe.io\"";
    let url_headers =
        "name = \"record-b\"\n  url = \"https://ethereum-beacon-api.publicnode.com\"\n  kind = \"headers\"";
    let f = File::parse(&base).unwrap();
    assert_eq!((f.policy.min_distinct_clients, f.policy.client_match), (0, ClientMatch::Bytes));
    for (policy, a, b) in [
        ("min_distinct_clients = 2", nimbus.as_str(), lodestar.as_str()),
        ("min_distinct_clients = 2", nimbus.as_str(), url_lc),
        ("min_distinct_clients = 2\nclient_match = \"finalized-header\"", nimbus.as_str(), url_headers),
    ] {
        let f = File::parse(&with(policy, a, b)).unwrap_or_else(|e| panic!("{policy}: {e:#}"));
        assert_eq!(f.policy.min_distinct_clients, 2);
    }
    for (url, host) in [
        ("https://lodestar-mainnet.chainsafe.io", "lodestar-mainnet.chainsafe.io"),
        ("http://User@Testing.Mainnet.Beacon-API.nimbus.team:5052/eth?x=1", "testing.mainnet.beacon-api.nimbus.team"),
        ("http://[::1]:5052", "::1"),
        ("127.0.0.1:9000/x", "127.0.0.1"),
    ] {
        assert_eq!(url_host(url), host, "{url}");
    }
    let f = File::parse(&with("min_distinct_clients = 2", &nimbus, &lodestar)).unwrap();
    assert_eq!(f.chains[0].sources[1].client.as_deref(), Some("Lodestar/v1.51.0/7923280"));
    for (policy, a, b, why) in [
        ("min_distinct_clients = 1", nimbus.as_str(), lodestar.as_str(), "1 asks nothing"),
        ("min_distinct_clients = 2", rec_a, lodestar.as_str(), "a recorded dir without a client cannot count"),
        ("min_distinct_clients = 3", nimbus.as_str(), lodestar.as_str(), "three clients from two sources"),
        ("min_distinct_clients = 2", nimbus.as_str(), url_headers, "a headers source serves no bytes to compare"),
        (
            "",
            nimbus.as_str(),
            "name = \"record-b\"\n  url = \"https://x\"\n  client = \"Nimbus/v1\"",
            "client on a url source",
        ),
        ("client_match = \"header\"", rec_a, rec_b, "an unknown client_match"),
        (
            "min_distinct_clients = 2",
            "name = \"record-a\"\n  url = \"https://lodestar-mainnet.chainsafe.io/\"",
            url_lc,
            "one host twice",
        ),
    ] {
        let text = with(policy, a, b);
        assert_ne!(text, base, "{why}: the substitution applies");
        assert!(File::parse(&text).is_err(), "{why} must be refused");
    }
}

/// `/eth/v1/node/version` answers recorded from public beacon APIs on 18 November 2026 (`fixtures/node-version/`), the
/// client family read from each, and a provider's hostile or empty answer.
#[test]
fn node_version_answers() {
    for (client, family) in [("nimbus", "nimbus"), ("lodestar", "lodestar"), ("lighthouse", "lighthouse")] {
        let body = std::fs::read_to_string(here(&format!("tests/fixtures/node-version/{client}.json"))).unwrap();
        let v = parse_version_answer(&body).unwrap();
        assert_eq!(client_family(&v).as_deref(), Some(family), "{v}");
        println!("{client}: {v}");
    }
    assert_eq!(client_family("teku/v25.11.0/linux-x86_64").as_deref(), Some("teku"));
    let v = parse_version_answer("{\"data\":{\"version\":\"Evil\\\"} 1\\n# x/1\"}}").unwrap();
    assert!(v.chars().all(|c| c.is_ascii_graphic() && c != '"' && c != '\\'), "{v}");
    assert_eq!(client_family(&v).as_deref(), Some("evil_}_1_#_x"));
    assert!(parse_version_answer("{\"data\":{\"version\":\"\"}}").is_err());
    assert!(parse_version_answer("{\"data\":{\"version\":\"/v1\"}}").is_err());
    assert!(parse_version_answer("{\"data\":{}}").is_err());
    assert!(parse_version_answer("<html>").is_err());
}
