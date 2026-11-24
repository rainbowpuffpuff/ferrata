#!/usr/bin/env python3
"""The deployed programs of the Ferrata LEZ test network against scripts/reproduce/deployed-programs.json.

    python3 -I scripts/reproduce/check_network.py chain   [--list FILE] [--network URL]
        reads the network's /programs.json and, over its JSON-RPC, each program's loader header (the image id LEZ runs
        for that program account, and whether the header is immutable); compares both with the list. No build needed.
    python3 -I scripts/reproduce/check_network.py compare --out DIR [--list FILE] [--network URL]
        the same, plus the rebuilt ids that scripts/reproduce/deployed.sh build wrote to DIR/<source>.tsv;
        writes DIR/result.json. Without --network only the rebuild is compared.

One line per program; exit 1 on any difference. Standard library only; read-only calls.
"""
import argparse
import json
import sys
import time
import urllib.request
from pathlib import Path

LOADER = bytes([0xFE] * 32)  # LEZ's program loader (its builtin id)
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
UA = {"User-Agent": "ferrata-check-network/1", "Content-Type": "application/json"}


def b58enc(b: bytes) -> str:
    n = int.from_bytes(b, "big")
    s = ""
    while n:
        n, r = divmod(n, 58)
        s = B58[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s


def get_json(url: str, body=None):
    req = urllib.request.Request(url, data=None if body is None else json.dumps(body).encode(), headers=UA)
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read())


def header(network: str, account: str):
    """(image id hex, immutable) from the loader's state of `account`, or raises."""
    res = get_json(f"{network}/rpc", {"jsonrpc": "2.0", "id": 1, "method": "getAccountView",
                                       "params": [{"account_id": account, "program_account_id": b58enc(LOADER)}]})
    if "error" in res:
        raise RuntimeError(res["error"])
    states = ((res.get("result") or {}).get("data") or {}).get("actor_states") or {}
    data = states.get(b58enc(LOADER))
    if data is None:
        raise RuntimeError("no loader state at that account")
    data = bytes(data)
    if len(data) != 66 or data[0] != 0:
        raise RuntimeError(f"not a loader header ({len(data)} bytes, variant {data[0] if data else None})")
    return "0x" + data[1:33].hex(), bool(data[65])


def rebuilt(out: Path) -> dict:
    """{(source, guest): row} from DIR/<source>.tsv."""
    rows = {}
    for f in sorted(out.glob("*.tsv")):
        lines = f.read_text().splitlines()
        if not lines:
            continue
        cols = lines[0].split("\t")
        for line in lines[1:]:
            r = dict(zip(cols, line.split("\t")))
            idh = r.get("image_id", "").lower()
            r["image_id"] = idh if idh.startswith("0x") else "0x" + idh
            rows[(f.stem, r.get("name", ""))] = r
    return rows


def evaluate(p: dict, compare: bool, built_row, network: bool, live_entry, read_header):
    """One program's verdict: (record, [rebuilt, published, on chain] cells, ok). ok needs, where checked: the rebuilt
    image id and binary sha256 equal to the list's and r0vm's id of the binary agreeing ("= id", as guest_ids.py writes
    it); the network's /programs.json entry equal to the list's; the loader header on chain naming the list's image id
    and immutable (a mutable header can be pointed at another binary tomorrow)."""
    r = {"name": p["name"], "recorded": p["image_id"], "binary_sha256": p["binary_sha256"]}
    cells, ok = [], True
    if compare:
        if built_row is None:
            cells.append("missing"); ok = False
        else:
            r["rebuilt"] = built_row["image_id"]; r["rebuilt_sha256"] = built_row.get("sha256_bin")
            r["r0vm"] = built_row.get("r0vm_id")
            same = (built_row["image_id"] == p["image_id"] and built_row.get("sha256_bin") == p["binary_sha256"]
                    and built_row.get("r0vm_id") == "= id")
            cells.append("equal" if same else ("DIFFERS" if built_row["image_id"] != p["image_id"] or
                                               built_row.get("sha256_bin") != p["binary_sha256"] else "r0vm DIFFERS"))
            ok &= same
    else:
        cells.append("-")
    if network:
        if live_entry is None:
            cells += ["absent", "-"]; ok = False
        else:
            pub = (live_entry.get("image_id", "").lower() == p["image_id"]
                   and live_entry.get("binary_sha256", "").lower() == p["binary_sha256"])
            cells.append("equal" if pub else "DIFFERS"); ok &= pub
            r["account"] = live_entry.get("program_account")
            try:
                img, imm = read_header(live_entry["program_account"])
                r["chain_image_id"], r["immutable"] = img, imm
                on = img == p["image_id"]
                cells.append(("equal" if on else "DIFFERS") + (", immutable" if imm else ", MUTABLE"))
                ok &= on and imm
            except Exception as e:  # noqa: BLE001
                cells.append(f"unread: {e}"[:40]); ok = False
    else:
        cells += ["-", "-"]
    r["ok"] = ok
    return r, cells, ok


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["chain", "compare"])
    ap.add_argument("--list", default=str(Path(__file__).with_name("deployed-programs.json")))
    ap.add_argument("--out")
    ap.add_argument("--network")
    a = ap.parse_args()
    lst = json.load(open(a.list))
    network = (a.network or (lst["network"] if a.mode == "chain" else "")).rstrip("/")
    built = rebuilt(Path(a.out)) if a.mode == "compare" and a.out else {}
    if a.mode == "compare" and not built:
        print(f"no rebuilt ids in {a.out} (run scripts/reproduce/deployed.sh build --out DIR first)")
        return 1
    live = {}
    if network:
        try:
            live = get_json(f"{network}/programs.json").get("programs", {})
        except Exception as e:  # noqa: BLE001
            print(f"cannot read {network}/programs.json: {e}")
            return 1
    bad, results = 0, []
    print(f"checked {time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime())}; list {Path(a.list).name}; network {network or '(none)'}")
    print(f"{'program':28} {'recorded id':20} {'rebuilt':10} {'published':10} {'on chain':16} verdict")
    for p in lst["programs"]:
        b = built.get((p["source"], p["guest"])) if a.mode == "compare" else None
        lp = live.get(p["name"]) if network else None
        reader = (lambda acct: header(network, acct)) if network else None
        r, cells, ok = evaluate(p, a.mode == "compare", b, bool(network), lp, reader)
        bad += not ok
        results.append(r)
        print(f"{p['name']:28} {p['image_id'][:18]:20} {cells[0]:10} {cells[1]:10} {cells[2]:16} {'ok' if ok else 'FAIL'}")
    extra = sorted(set(live) - {p["name"] for p in lst["programs"]})
    for name in extra:  # deployed but never rebuilt: the claim "every deployed program reproduced" would be false
        print(f"{name:28} {'(not in the list)':20} {'-':10} {'-':10} {'-':16} FAIL: not reproduced, add it to the list")
    if a.out:
        Path(a.out).mkdir(parents=True, exist_ok=True)
        (Path(a.out) / "result.json").write_text(json.dumps({"network": network, "programs": results,
                                                            "not_in_list": extra}, indent=1) + "\n")
    print(f"{len(results) - bad} of {len(results)} programs ok" + (f"; {len(extra)} deployed but not in the list" if extra else ""))
    return 1 if bad or extra else 0


if __name__ == "__main__":
    sys.exit(main())
