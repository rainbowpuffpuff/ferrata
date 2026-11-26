#!/usr/bin/env python3
"""ferrata-zone-keys configs (deploy/testnet/MULTIKEY.md): the configurations of a zone grown from one sequencer to
1 + N on one host. Each is the first sequencer's but for its home, its gossip address (fixed loopback ports, each the
others' bootstrap peer: one zone, every sequencer reaching every other) and its metrics port; the genesis, the node,
the channel and the fee reserve are the first's, byte for byte. N outside 1..8 is refused and nothing is written.
Run: python3 -I deploy/testnet/tests/test_zone_keys.py"""
import json
import pathlib
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE.parent / "host" / "bin" / "ferrata-zone-keys"
FAILED = []


def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + (f": {detail}" if detail and not ok else ""))
    if not ok:
        FAILED.append(name)


FIRST = {
    "home": "/var/lib/ferrata/sequencer",
    "block_create_timeout": "10s",
    "bedrock_config": {"node_url": "http://127.0.0.1:18080", "channel_id": "01" * 32, "priority_fee_percent": 900},
    "gossip": {"listen_addr": "/ip4/127.0.0.1/udp/0/quic-v1", "bootstrap_peers": []},
    "metrics_listen_address": "127.0.0.1:9000",
    "genesis": {"actions": [{"supply_account": {"recipient": {"identified": {"key": "aa" * 32}}, "balance": 10**15}}]},
}

with tempfile.TemporaryDirectory(dir=HERE) as d:
    d = pathlib.Path(d)
    src = d / "first.json"
    src.write_text(json.dumps(FIRST))
    out = d / "out"
    r = subprocess.run(["bash", str(SCRIPT), "configs", "3", "--src", str(src), "--out", str(out)], capture_output=True, text=True)
    check("configs 3 runs", r.returncode == 0, r.stderr[-300:])
    files = sorted(p.name for p in out.glob("*.json"))
    check("four configurations, sequencers 0..3", files == [f"sequencer_config-{i}.json" for i in range(4)], repr(files))
    cs = [json.loads((out / f"sequencer_config-{i}.json").read_text()) for i in range(4)]
    addrs = [c["gossip"]["listen_addr"] for c in cs]
    check("each listens on its own fixed loopback port", addrs == [f"/ip4/127.0.0.1/udp/{19100 + i}/quic-v1" for i in range(4)], repr(addrs))
    check("each one's bootstrap peers are exactly the others",
          all(sorted(c["gossip"]["bootstrap_peers"]) == sorted(a for j, a in enumerate(addrs) if j != i) for i, c in enumerate(cs)))
    check("homes: the first's unchanged, the others sequencer-<i>",
          [c["home"] for c in cs] == ["/var/lib/ferrata/sequencer"] + [f"/var/lib/ferrata/sequencer-{i}" for i in (1, 2, 3)])
    check("metrics on 9000 + i", [c["metrics_listen_address"] for c in cs] == [f"127.0.0.1:{9000 + i}" for i in range(4)])
    same = all({k: v for k, v in c.items() if k not in ("home", "gossip", "metrics_listen_address")}
               == {k: v for k, v in FIRST.items() if k not in ("home", "gossip", "metrics_listen_address")} for c in cs)
    check("the genesis, the node, the channel and the fee reserve are the first's, unchanged", same)
    for bad in ("0", "9", "x", ""):
        o2 = d / f"bad-{bad or 'empty'}"
        r = subprocess.run(["bash", str(SCRIPT), "configs", bad, "--src", str(src), "--out", str(o2)], capture_output=True, text=True)
        check(f"N = {bad!r} refused, nothing written", r.returncode != 0 and not o2.exists(), r.stderr[-200:])
print(f"{'FAILED ' + str(len(FAILED)) if FAILED else 'all passed'}")
raise SystemExit(1 if FAILED else 0)
