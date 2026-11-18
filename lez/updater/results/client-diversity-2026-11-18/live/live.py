#!/usr/bin/env python3
"""The updater with `policy.min_distinct_clients` against live public beacon APIs, on LEZ's state machine in process
(`ledger = "harness"`, RISC0_DEV_MODE=1): nothing is sent to any network; the beacon APIs are only read (GET, User-Agent
"ferrata-updater").

  python3 -I live.py OUT_DIR --bin PATH/ferrata-updater --name NAME --preset mainnet|sepolia [--layout gloas]
      --lc NAME=URL [--lc ...] [--headers NAME=URL] --k K [--match bytes|finalized-header] [--minutes M] [--poll S]
      [--port P] --key PAYER_KEY_FILE

The checkpoint: the finalized header of the previous sync-committee period's update (as the first light-client source
serves it), checked to be served as a bootstrap by every light-client source. Writes OUT_DIR/updater.toml,
daemon.log, health.jsonl, metrics-last.txt, summary.md.
"""
import argparse, hashlib, json, os, signal, subprocess, sys, time, urllib.error, urllib.request

UA = "ferrata-updater"


def get(url, accept="application/json", timeout=30):
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": accept})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, b""
    except Exception as e:  # noqa: BLE001
        return None, repr(e).encode()


def header_root(h):
    leaves = [h[0:8] + bytes(24), h[8:16] + bytes(24), h[16:48], h[48:80], h[80:112]] + [bytes(32)] * 3
    while len(leaves) > 1:
        leaves = [hashlib.sha256(leaves[i] + leaves[i + 1]).digest() for i in range(0, len(leaves), 2)]
    return leaves[0]


def finalized_of_update(b):
    """(slot, root) of a LightClientUpdate's finalized header (Gloas: fixed layout; Electra/Fulu: offsets)."""
    gloas_header = 112 + 32 + 32 * 11
    if len(b) == 2 * gloas_header + 24624 + 32 * 11 + 32 * 9 + 168:
        at = gloas_header + 24624 + 32 * 11
        fin = b[at:at + 112]
    else:
        of = int.from_bytes(b[4 + 24624 + 192:4 + 24624 + 196], "little")
        fin = b[of:of + 112]
    return int.from_bytes(fin[:8], "little"), header_root(fin)


def chunks(raw):
    for counts_digest in (True, False):
        out, at, ok = [], 0, True
        while at < len(raw):
            n = int.from_bytes(raw[at:at + 8], "little")
            size = n - 4 if counts_digest else n
            if size <= 0 or at + 12 + size > len(raw):
                ok = False
                break
            out.append(raw[at + 12:at + 12 + size])
            at += 12 + size
        if ok:
            return out
    return []


def pick_checkpoint(lcs):
    base = lcs[0][1]
    st, b = get(base + "/eth/v1/beacon/light_client/finality_update", "application/octet-stream")
    if st != 200:
        sys.exit(f"{base}: finality_update HTTP {st}")
    # Attested slot: Gloas fixed layout starts with the attested header; Electra has an offset.
    att = int.from_bytes(b[:8], "little") if len(b) == 2 * (112 + 32 + 352) + 288 + 168 else int.from_bytes(b[int.from_bytes(b[:4], "little"):][:8], "little")
    period = att // 8192
    for p in (period - 1, period - 2):
        st, raw = get(base + f"/eth/v1/beacon/light_client/updates?start_period={p}&count=1", "application/octet-stream")
        if st != 200 or not chunks(raw):
            continue
        slot, root = finalized_of_update(chunks(raw)[0])
        if all(get(u + f"/eth/v1/beacon/light_client/bootstrap/0x{root.hex()}", "application/octet-stream")[0] == 200 for _, u in lcs):
            return p, slot, root.hex()
    sys.exit("no checkpoint served as a bootstrap by every light-client source")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--bin", required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--preset", required=True)
    ap.add_argument("--layout")
    ap.add_argument("--lc", action="append", required=True)
    ap.add_argument("--headers", action="append", default=[])
    ap.add_argument("--k", type=int, required=True)
    ap.add_argument("--match", default="bytes")
    ap.add_argument("--minutes", type=float, default=12)
    ap.add_argument("--poll", type=int, default=12)
    ap.add_argument("--port", type=int, default=9470)
    ap.add_argument("--key", required=True)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    lcs = [tuple(x.split("=", 1)) for x in a.lc]
    hds = [tuple(x.split("=", 1)) for x in a.headers]
    period, slot, root = pick_checkpoint(lcs)
    sources = "".join(f'\n  [[chain.source]]\n  name = "{n}"\n  url = "{u}"\n' for n, u in lcs)
    sources += "".join(f'\n  [[chain.source]]\n  name = "{n}"\n  url = "{u}"\n  kind = "headers"\n' for n, u in hds)
    toml = f'''# Written by live.py: the checkpoint is the finalized header of period {period}'s update (slot {slot}).
[daemon]
name = "{a.name}"
state_dir = "state"
poll_secs = {a.poll}
listen = "127.0.0.1:{a.port}"
ledger = "harness"

[keys]
payer_key_file = "{os.path.abspath(a.key)}"

[policy]
defer_window = 1
max_lag_slots = 64
min_agreeing = 1
retry_base_ms = 500
retry_max_ms = 30000
max_attempts = 5
demote_rounds = 50
max_periods_per_request = 16
min_distinct_clients = {a.k}
client_match = "{a.match}"

[[chain]]
name = "{a.preset}"
preset = "{a.preset}"
checkpoint = "{root}"
adoption_parts = 2
{f'layout = "{a.layout}"' if a.layout else ''}
{sources}'''
    cfg = os.path.join(a.out, "updater.toml")
    with open(cfg, "w") as f:
        f.write(toml)
    env = dict(os.environ, RISC0_DEV_MODE="1")
    log = open(os.path.join(a.out, "daemon.log"), "w")
    t0 = time.time()
    proc = subprocess.Popen([a.bin, "--config", cfg], stdout=log, stderr=subprocess.STDOUT, env=env)
    health = open(os.path.join(a.out, "health.jsonl"), "w")
    metrics = b""
    while time.time() - t0 < a.minutes * 60 and proc.poll() is None:
        time.sleep(30)
        st, h = get(f"http://127.0.0.1:{a.port}/health")
        health.write(json.dumps({"t": round(time.time() - t0), "http": st, "body": json.loads(h) if st in (200, 503) and h[:1] in (b"{", b"[") else h[:200].decode("utf-8", "replace")}) + "\n")
        health.flush()
        st, m = get(f"http://127.0.0.1:{a.port}/metrics", "text/plain")
        if st == 200:
            metrics = m
    proc.send_signal(signal.SIGTERM)
    try:
        code = proc.wait(timeout=600)
    except subprocess.TimeoutExpired:
        proc.kill()
        code = "killed"
    with open(os.path.join(a.out, "metrics-last.txt"), "wb") as f:
        f.write(metrics)
    lines = open(os.path.join(a.out, "daemon.log"), errors="replace").read().splitlines()
    sent = [l for l in lines if " applied (exit" in l or " refused (exit" in l or " rejected (exit" in l]
    jobs = [l for l in lines if "served or confirmed by" in l]
    notsent = [l for l in lines if "not sent" in l]
    dis = [l for l in lines if "sources disagree" in l]
    md = [f"# {a.name}: live, {a.preset}, min_distinct_clients = {a.k}, client_match = {a.match}", "",
          f"Started {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(t0))}, {a.minutes} min, poll {a.poll} s; daemon exit {code}.",
          f"Checkpoint: finalized header of period {period}'s update, slot {slot}, root 0x{root}.",
          "Sources: " + ", ".join(f"`{n}` {u}" for n, u in lcs) + ("; headers: " + ", ".join(f"`{n}` {u}" for n, u in hds) if hds else ""), "",
          f"- transactions: {len(sent)} ({sum(1 for l in sent if ' applied (exit' in l)} applied, {sum(1 for l in sent if ' refused (exit' in l)} refused, {sum(1 for l in sent if ' rejected (exit' in l)} rejected)",
          f"- jobs opened (each update chosen): {len(jobs)}", f"- rounds an update was not sent for lack of clients: {len(notsent)}",
          f"- disagreements logged: {len(dis)}", "", "Jobs:", ""] + [f"    {l[l.find('job 0x'):][:400]}" for l in jobs] + \
         ["", "Disagreements (first 5):", ""] + [f"    {l[:600]}" for l in dis[:5]] + \
         ["", "Not sent (first 3, last 3):", ""] + [f"    {l[:400]}" for l in notsent[:3] + notsent[-3:]]
    with open(os.path.join(a.out, "summary.md"), "w") as f:
        f.write("\n".join(md) + "\n")
    print("\n".join(md))


if __name__ == "__main__":
    sys.exit(main())
