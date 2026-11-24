#!/usr/bin/env python3
"""The gateway's gas-drop routes (RFP-022 U.5, Priv.7): the published terms, the guest, the inbox and its bounds.

    python3 -I deploy/testnet/tests/test_gateway_gasdrop.py

Loads host/bin/ferrata-gateway as a module with GASDROP_* pointing into a temporary directory, serves it on a free
loopback port and checks: the terms (404 unset, 200 set, 503 paused), the guest's bytes, a job accepted into the inbox
(file mode 0640, the name carrying N), the same N again (409), malformed jobs (400: shape, journal, another relayer),
an oversized body (413), the inbox full (429), the per-IP, per-network and hourly caps (429), and X-Real-IP honoured
from loopback only. Standard library only; no network beyond loopback.
"""
import hashlib
import http.server
import importlib.machinery
import importlib.util
import json
import os
import pathlib
import socket
import stat
import tempfile
import threading
import urllib.error
import urllib.request

HERE = pathlib.Path(__file__).resolve().parent
GW = pathlib.Path(os.environ.get("GATEWAY_UNDER_TEST") or HERE.parent / "host" / "bin" / "ferrata-gateway")
RELAYER = bytes(range(1, 33))
OTHER = bytes(32)


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def call(url, body=None, headers=None, raw=None):
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json", **(headers or {})})
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status, dict(r.headers), r.read()
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), e.read()


def journal(n, relayer=RELAYER, domain=None):
    """The gas-drop journal: DROP_DOMAIN(32) root(32) chainId(32) vault(20) N(32) oneTime(32) relayer(32)."""
    d = domain or hashlib.sha256(b"ferrata/gas-drop/v1").digest()
    return list(d + bytes(32) + bytes(31) + b"\x01" + bytes(20) + n + bytes([7]) * 32 + relayer)


def job(n, **kw):
    return {"kind": "gas-drop", "receipt": {"inner": {"Fake": {}}, "journal": {"bytes": journal(n, **kw)}, "metadata": {}},
            "oneTimeNpk": "0x" + "ab" * 32, "oneTimeVpk": "0x02" + "cd" * 32}


def main():
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="gw-gasdrop-", dir=os.environ.get("TEST_TMPDIR")))
    inbox = tmp / "inbox"
    inbox.mkdir()
    terms = tmp / "terms.json"
    guest = tmp / "gasdrop.bin"
    guest.write_bytes(b"\x7fELF" + bytes(1000))
    port = free_port()
    os.environ.update({
        "GATEWAY_LISTEN": f"127.0.0.1:{port}", "GATEWAY_UPSTREAM": "http://127.0.0.1:9",
        "GASDROP_INFO": "", "GASDROP_INBOX": str(inbox), "GASDROP_GUEST": str(guest), "GASDROP_MAX_BODY": "20000",
        "GASDROP_MAX_PENDING": "3", "GASDROP_PER_IP_PER_DAY": "2", "GASDROP_PER_NET_PER_DAY": "3",
        "GASDROP_GLOBAL_PER_HOUR": "4", "GASDROP_GET_RATE": "0.001", "GASDROP_GET_BURST": "50",
        "FAUCET_STATE": str(tmp / "faucet"),
    })
    loader = importlib.machinery.SourceFileLoader("gateway", str(GW))
    spec = importlib.util.spec_from_loader("gateway", loader)
    gw = importlib.util.module_from_spec(spec)
    loader.exec_module(gw)
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", port), gw.Handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    base = f"http://127.0.0.1:{port}/relayer/gas-drop"
    ok = 0

    def check(cond, what):
        nonlocal ok
        if not cond:
            raise SystemExit(f"FAIL: {what}")
        ok += 1
        print(f"ok {ok}: {what}")

    c, _, _ = call(base)
    check(c == 404, "no GASDROP_INFO: no relayer published (404, the tool then pays from the user's own account)")
    c, _, _ = call(base, job(b"\x01" * 32))
    check(c == 404 and not list(inbox.iterdir()), "no GASDROP_INFO: a job is refused, nothing written")

    t = {"relayer": "0x" + RELAYER.hex(), "imageId": "0x" + "89" * 32, "amount": 200000000, "covers": ["claim", "burn"]}
    terms.write_text(json.dumps(t))
    gw.CFG["GASDROP_INFO"] = str(terms)
    c, h, b = call(base)
    info = json.loads(b)
    check(c == 200 and info["relayer"] == t["relayer"] and info["amount"] == "200000000" and info["inbox"] == "/relayer/gas-drop"
          and info["guest"] == "/relayer/gas-drop/guest" and info["covers"] == ["claim", "burn"] and info["pending"] == 0,
          "the terms: relayer, amount as a string, covers, relative inbox and guest, nothing pending")
    check(h.get("Access-Control-Allow-Origin") == "*", "CORS on the terms (the web wallet reads them)")
    c, h, b = call(base + "/guest")
    check(c == 200 and b == guest.read_bytes() and h.get("Content-Type") == "application/octet-stream", "the guest's bytes")

    n1 = bytes([0x11]) * 32
    c, _, b = call(base, job(n1), {"X-Real-IP": "198.51.100.7"})
    files = sorted(inbox.iterdir())
    check(c == 202 and json.loads(b)["job"] == f"gas-drop-{n1.hex()}.json" and [f.name for f in files] == [f"gas-drop-{n1.hex()}.json"],
          "a job accepted (202) into the inbox, named by its N")
    check(stat.S_IMODE(files[0].stat().st_mode) == 0o640, "the job is mode 0640 (the relayer reads it through the group)")
    check(json.loads(files[0].read_text()) == job(n1), "the job is written as posted")
    c, _, _ = call(base, job(n1), {"X-Real-IP": "198.51.100.8"})
    check(c == 409 and len(list(inbox.iterdir())) == 1, "the same N again while the inbox holds it: 409, nothing written")

    bad = job(bytes([0x12]) * 32)
    bad["extra"] = 1
    check(call(base, bad)[0] == 400, "a job with another key: 400")
    bad = job(bytes([0x12]) * 32)
    bad["oneTimeNpk"] = "0xAB"
    check(call(base, bad)[0] == 400, "a malformed one-time key: 400")
    check(call(base, job(bytes([0x12]) * 32, relayer=OTHER))[0] == 400, "a receipt bound to another relayer: 400")
    check(call(base, job(bytes([0x12]) * 32, domain=bytes(32)))[0] == 400, "a journal of another statement: 400")
    bad = job(bytes([0x12]) * 32)
    bad["receipt"]["journal"]["bytes"] = bad["receipt"]["journal"]["bytes"][:200]
    check(call(base, bad)[0] == 400, "a journal of the wrong length: 400")
    check(call(base, raw=b"{" + b" " * 30000 + b"}")[0] == 413, "a body over GASDROP_MAX_BODY: 413")
    check(call(base, raw=b"not json")[0] == 400, "not JSON: 400")

    c, _, b = call(base, job(bytes([0x13]) * 32), {"X-Real-IP": "198.51.100.7"})
    check(c == 202, "a second job from the same address")
    c, _, b = call(base, job(bytes([0x14]) * 32), {"X-Real-IP": "198.51.100.7"})
    check(c == 429 and "address" in json.loads(b)["error"], "a third from that address the same day: 429 (per IP)")
    c, _, b = call(base, job(bytes([0x15]) * 32), {"X-Real-IP": "198.51.100.9"})
    check(c == 202 and len(list(inbox.iterdir())) == 3, "another address of the network: accepted (three pending)")
    c, _, b = call(base, job(bytes([0x16]) * 32), {"X-Real-IP": "203.0.113.5"})
    check(c == 429 and "full" in json.loads(b)["error"], "the inbox at GASDROP_MAX_PENDING: 429")
    next(inbox.iterdir()).unlink()  # the relayer takes one
    c, _, b = call(base, job(bytes([0x17]) * 32), {"X-Real-IP": "198.51.100.10"})
    check(c == 429 and "network" in json.loads(b)["error"], "a fourth job from one /24 the same day: 429 (per network)")
    c, _, b = call(base, job(bytes([0x18]) * 32), {"X-Real-IP": "203.0.113.5"})
    check(c == 202, "an address of another network: accepted")
    next(inbox.iterdir()).unlink()
    c, _, b = call(base, job(bytes([0x19]) * 32), {"X-Real-IP": "192.0.2.1"})
    check(c == 429 and "hour" in json.loads(b)["error"], "the fifth job in an hour, every caller together: 429")

    check(gw.check_drop_job(job(n1), RELAYER) == (n1.hex(), None), "check_drop_job returns N")
    t["paused"] = "the drop account is being refilled"
    terms.write_text(json.dumps(t))
    c, _, b = call(base)
    check(c == 503 and "refilled" in json.loads(b)["error"], "paused: the terms answer 503 with the reason (the tool stops)")
    check(call(base, job(bytes([0x1a]) * 32))[0] == 503, "paused: a job is refused with 503")
    print(f"all {ok} checks passed")


if __name__ == "__main__":
    main()
