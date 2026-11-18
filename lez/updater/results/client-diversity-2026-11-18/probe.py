#!/usr/bin/env python3
"""Which consensus client each public beacon API reports, and whether endpoints running different clients serve
byte-identical light-client data (the check `policy.min_distinct_clients` makes before the updater sends anything).

Read-only GETs with the User-Agent "ferrata-updater" and nothing that identifies the caller:
  /eth/v1/node/version                                   (JSON: the client, as the node reports it)
  /eth/v1/beacon/light_client/finality_update            (SSZ, asked of every endpoint at once, in rounds)
  /eth/v1/beacon/light_client/updates?start_period&count (SSZ, the last periods)

  python3 -I probe.py OUT_DIR --net NAME=URL[,URL...] [--net ...] [--rounds N] [--interval S] [--periods K]

Writes OUT_DIR/probe.jsonl (every answer, decoded), OUT_DIR/summary.md, and per network the SSZ of the first
round's finality updates and of the period updates, and every endpoint's version answer (OUT_DIR/answers/).
"""
import argparse, concurrent.futures as cf, hashlib, json, os, sys, time, urllib.error, urllib.request

UA = "ferrata-updater"
SLOTS_PER_PERIOD = 8192
# Light-client layouts (consensus-specs): Gloas headers are fixed-size, Electra/Fulu ones carry an offset.
GLOAS_HEADER = 112 + 32 + 32 * 11          # beacon, execution_block_hash, execution_branch (gindex 2856)
GLOAS_FINALITY = 2 * GLOAS_HEADER + 32 * 9 + 160 + 8          # finality_branch gindex 735
GLOAS_UPDATE = 2 * GLOAS_HEADER + 24624 + 32 * 11 + 32 * 9 + 160 + 8   # next_sync_committee gindex 2946


def get(url, accept, timeout=30):
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": accept})
    t0 = time.time()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, dict(r.headers), r.read(), t0
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers or {}), e.read() if e.fp else b"", t0
    except Exception as e:  # noqa: BLE001 - recorded, not raised
        return None, {}, repr(e).encode(), t0


def u64(b):
    return int.from_bytes(b[:8], "little")


def u32(b):
    return int.from_bytes(b[:4], "little")


def header_root(h):
    """hash_tree_root(BeaconBlockHeader) of the first 112 bytes of a LightClientHeader."""
    leaves = [h[0:8] + bytes(24), h[8:16] + bytes(24), h[16:48], h[48:80], h[80:112]] + [bytes(32)] * 3
    while len(leaves) > 1:
        leaves = [hashlib.sha256(leaves[i] + leaves[i + 1]).digest() for i in range(0, len(leaves), 2)]
    return leaves[0].hex()


def info(att, fin, sig_slot, bits, raw):
    return {
        "sha256": hashlib.sha256(raw).hexdigest(),
        "bytes": len(raw),
        "attested_slot": u64(att),
        "attested_root": header_root(att),
        "finalized_slot": u64(fin),
        "finalized_root": header_root(fin),
        "signature_slot": sig_slot,
        "signers": sum(bin(x).count("1") for x in bits),
    }


def decode_finality(b):
    if b[:1] == b"{":
        raise ValueError("JSON, not SSZ")
    if len(b) == GLOAS_FINALITY:
        h = GLOAS_HEADER
        return info(b[:h], b[h:2 * h], u64(b[-8:]), b[-168:-104], b) | {"layout": "gloas"}
    oa, of = u32(b[0:4]), u32(b[4:8])
    return info(b[oa:of], b[of:], u64(b[oa - 8:oa]), b[oa - 168:oa - 104], b) | {"layout": "electra"}


def decode_update(b):
    if b[:1] == b"{":
        raise ValueError("JSON, not SSZ")
    if len(b) == GLOAS_UPDATE:
        h = GLOAS_HEADER
        fin_at = h + 24624 + 32 * 11
        return info(b[:h], b[fin_at:fin_at + h], u64(b[-8:]), b[-168:-104], b) | {"layout": "gloas"}
    oa = u32(b[0:4])
    of = u32(b[4 + 24624 + 192:4 + 24624 + 196])
    return info(b[oa:of], b[of:], u64(b[oa - 8:oa]), b[oa - 168:oa - 104], b) | {"layout": "electra"}


def split_chunks(raw):
    """The `updates` endpoint's SSZ stream: 8-byte length (counting the 4-byte fork digest or not), digest, payload."""
    for counts_digest in (True, False):
        out, at, ok = [], 0, True
        while at < len(raw):
            if at + 12 > len(raw):
                ok = False
                break
            n = int.from_bytes(raw[at:at + 8], "little")
            size = n - 4 if counts_digest else n
            if size <= 0 or at + 12 + size > len(raw):
                ok = False
                break
            out.append((raw[at + 8:at + 12].hex(), raw[at + 12:at + 12 + size]))
            at += 12 + size
        if ok:
            return out
    return []


def family(version):
    return version.split("/")[0].strip().lower() if version else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--net", action="append", required=True, help="NAME=URL[,URL...]")
    ap.add_argument("--rounds", type=int, default=10)
    ap.add_argument("--interval", type=float, default=12.0)
    ap.add_argument("--periods", type=int, default=3)
    a = ap.parse_args()
    os.makedirs(os.path.join(a.out, "answers"), exist_ok=True)
    log = open(os.path.join(a.out, "probe.jsonl"), "w")

    def rec(**kw):
        log.write(json.dumps(kw, sort_keys=True) + "\n")
        log.flush()
        return kw

    nets = {}
    for spec in a.net:
        name, urls = spec.split("=", 1)
        nets[name] = [u.rstrip("/") for u in urls.split(",") if u]
    md = ["# Client diversity of public light-client data: probe", "",
          f"Started {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}; {a.rounds} rounds, {a.interval} s apart; "
          f"User-Agent `{UA}`; read-only GETs.", ""]
    for net, urls in nets.items():
        md += [f"## {net}", "", "| endpoint | `/eth/v1/node/version` | client | `finality_update` | layout |", "|---|---|---|---|---|"]
        clients = {}
        for u in urls:
            st, hd, body, t0 = get(u + "/eth/v1/node/version", "application/json")
            ver = None
            if st == 200:
                try:
                    ver = json.loads(body)["data"]["version"]
                except Exception:  # noqa: BLE001
                    ver = None
            slug = u.split("//", 1)[-1].replace("/", "_").replace(":", "_")
            with open(os.path.join(a.out, "answers", f"{net}-{slug}-node-version.json"), "wb") as f:
                f.write(body)
            st2, hd2, b2, _ = get(u + "/eth/v1/beacon/light_client/finality_update", "application/octet-stream")
            fu = None
            if st2 == 200:
                try:
                    fu = decode_finality(b2)
                except Exception as e:  # noqa: BLE001
                    fu = {"error": repr(e)}
            clients[u] = family(ver)
            rec(net=net, endpoint=u, what="version", status=st, version=ver, client=family(ver), t=t0)
            rec(net=net, endpoint=u, what="finality_update_first", status=st2, consensus_version=hd2.get("Eth-Consensus-Version") or hd2.get("eth-consensus-version"), decoded=fu,
                error=None if st2 == 200 else b2[:200].decode("utf-8", "replace"))
            md.append(f"| `{u}` | {st}: `{ver}` | {family(ver)} | {st2}{' (' + str(fu.get('bytes')) + ' bytes)' if isinstance(fu, dict) and 'bytes' in fu else ''} | {fu.get('layout') if isinstance(fu, dict) else ''} |")
        # Rounds: every endpoint asked at once.
        md += ["", "Finality updates asked of every endpoint at the same moment:", "",
               "| round | " + " | ".join(f"`{u.split('//')[-1]}` ({clients[u]})" for u in urls) + " | verdict |",
               "|---|" + "---|" * len(urls) + "---|"]
        tally = {"identical": 0, "different moment": 0, "DISAGREE": 0, "incomplete": 0}
        first_saved = False
        for r in range(a.rounds):
            with cf.ThreadPoolExecutor(len(urls)) as ex:
                res = list(ex.map(lambda u: get(u + "/eth/v1/beacon/light_client/finality_update", "application/octet-stream"), urls))
            row, decoded = [], {}
            for u, (st, hd, b, t0) in zip(urls, res):
                if st == 200:
                    try:
                        d = decode_finality(b)
                    except Exception as e:  # noqa: BLE001
                        d = {"error": repr(e)}
                    decoded[u] = (d, b)
                    rec(net=net, endpoint=u, what="finality_update", round=r, status=st, t=t0, decoded=d)
                    row.append(f"{d.get('sha256', '?')[:12]} att {d.get('attested_slot')} sig {d.get('signature_slot')} fin {d.get('finalized_slot')} ({d.get('signers')})")
                    if not first_saved:
                        slug = u.split("//", 1)[-1].replace("/", "_").replace(":", "_")
                        with open(os.path.join(a.out, "answers", f"{net}-{slug}-finality-round{r}.ssz"), "wb") as f:
                            f.write(b)
                else:
                    rec(net=net, endpoint=u, what="finality_update", round=r, status=st, t=t0, error=b[:200].decode("utf-8", "replace"))
                    row.append(f"HTTP {st}")
            first_saved = first_saved or len(decoded) > 0
            ok = [d for d, _ in decoded.values() if "sha256" in d]
            if len(ok) < 2:
                verdict = "incomplete"
            elif len({d["sha256"] for d in ok}) == 1:
                verdict = "identical"
            elif len({(d["attested_root"], d["signature_slot"]) for d in ok}) == len({d["sha256"] for d in ok}):
                verdict = "different moment"
            else:
                verdict = "DISAGREE"
            tally[verdict] += 1
            rec(net=net, what="round", round=r, verdict=verdict)
            md.append(f"| {r} | " + " | ".join(row) + f" | {verdict} |")
            if r + 1 < a.rounds:
                time.sleep(a.interval)
        md += ["", "Rounds: " + ", ".join(f"{k} {v}" for k, v in tally.items()), ""]
        # Period updates.
        last = [json.loads(l) for l in open(os.path.join(a.out, "probe.jsonl")) if '"finality_update"' in l and f'"net": "{net}"' in l and '"decoded"' in l]
        head = max((x["decoded"]["attested_slot"] for x in last if "attested_slot" in x["decoded"]), default=None)
        if head is not None:
            hp = head // SLOTS_PER_PERIOD
            start = hp - a.periods
            md += [f"Period updates `updates?start_period={start}&count={a.periods + 1}` (head period {hp}; periods below it are finished):", "",
                   "| period | " + " | ".join(f"`{u.split('//')[-1]}`" for u in urls) + " | verdict |", "|---|" + "---|" * len(urls) + "---|"]
            per = {}
            for u in urls:
                st, hd, b, t0 = get(u + f"/eth/v1/beacon/light_client/updates?start_period={start}&count={a.periods + 1}", "application/octet-stream")
                if st != 200:
                    rec(net=net, endpoint=u, what="updates", status=st, error=b[:200].decode("utf-8", "replace"))
                    continue
                for digest, payload in split_chunks(b):
                    try:
                        d = decode_update(payload)
                    except Exception as e:  # noqa: BLE001
                        d = {"error": repr(e)}
                    p = d.get("attested_slot", -1) // SLOTS_PER_PERIOD if "attested_slot" in d else None
                    per.setdefault(p, {})[u] = d
                    rec(net=net, endpoint=u, what="update", period=p, fork_digest=digest, decoded=d)
                    slug = u.split("//", 1)[-1].replace("/", "_").replace(":", "_")
                    with open(os.path.join(a.out, "answers", f"{net}-{slug}-period{p}.ssz"), "wb") as f:
                        f.write(payload)
            for p in sorted(x for x in per if x is not None):
                ds = per[p]
                cells = [f"{ds[u]['sha256'][:12]} att {ds[u]['attested_slot']} sig {ds[u]['signature_slot']} ({ds[u]['signers']})" if u in ds and 'sha256' in ds[u] else "—" for u in urls]
                shas = {ds[u]["sha256"] for u in ds if "sha256" in ds[u]}
                verdict = ("identical" if len(shas) == 1 and len(ds) > 1 else "one endpoint only" if len(ds) <= 1 else
                           ("differ (unfinished period)" if p >= hp else "DIFFER (finished period)"))
                rec(net=net, what="period", period=p, verdict=verdict)
                md.append(f"| {p} | " + " | ".join(cells) + f" | {verdict} |")
            md.append("")
    md.append(f"Ended {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}.")
    with open(os.path.join(a.out, "summary.md"), "w") as f:
        f.write("\n".join(md) + "\n")
    print("\n".join(md))


if __name__ == "__main__":
    sys.exit(main())
