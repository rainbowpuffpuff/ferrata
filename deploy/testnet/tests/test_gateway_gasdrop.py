#!/usr/bin/env python3
"""The gateway's gas-drop routes (RFP-022 U.5, Priv.7): the published terms, the guest, the inbox and its bounds.

    python3 -I deploy/testnet/tests/test_gateway_gasdrop.py

Loads host/bin/ferrata-gateway as a module with GASDROP_* pointing into a temporary directory, serves it on a free
loopback port and checks: the terms (404 only when unset; 503 when unreadable or paused; 200 with the relay fee's
recipient), the guest's bytes; a job is verified (GASDROP_CHECK, here a stand-in checker that accepts the receipts
marked valid, as `ferrata-gasdrop check-job` accepts the real ones) BEFORE it counts against any cap: fake receipts are
refused 400 and leave the inbox, the hourly and the per-network caps untouched (review F2); no checker, a checker that
times out: 503, nothing counted; then a verified job accepted (0640, named by its N), the same N again (409), malformed
jobs (400), an oversized body (413), the inbox full (429), the per-IP, per-network and hourly caps (429), the per-IP
check budget (429), and the gateway's log naming neither the posting address nor N (review F5). Standard library only;
no network beyond loopback.
"""
import contextlib
import hashlib
import http.server
import importlib.machinery
import importlib.util
import io
import json
import os
import pathlib
import socket
import stat
import sys
import tempfile
import threading
import urllib.error
import urllib.request

HERE = pathlib.Path(__file__).resolve().parent
GW = pathlib.Path(os.environ.get("GATEWAY_UNDER_TEST") or HERE.parent / "host" / "bin" / "ferrata-gateway")
RELAYER = bytes(range(1, 33))
OTHER = bytes(32)
VAULT = "0x" + "55" * 20
IMAGE = "0x" + "89" * 32

# The stand-in checker: what `ferrata-gasdrop check-job` prints, for receipts marked valid; refuses the rest.
CHECKER = r'''
import json, sys, time
args = sys.argv[1:]
open(sys.argv[0] + ".args", "w").write(json.dumps(args))
job = json.loads(sys.stdin.read())
inner = job["receipt"].get("inner")
n = bytes(job["receipt"]["journal"]["bytes"][116:148]).hex()
if inner == {"Test": "slow"}:
    time.sleep(5)
if inner == {"Test": "other-n"}:
    n = "00" * 32
if inner in ({"Test": "valid"}, {"Test": "other-n"}):
    print(json.dumps({"ok": True, "nullifier": "0x" + n, "root": "0x" + "00" * 32}))
    sys.exit(0)
print(json.dumps({"ok": False, "reason": "refused: the receipt does not verify (not a proof)"}))
sys.exit(1)
'''


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
        with urllib.request.urlopen(req, timeout=20) as r:
            return r.status, dict(r.headers), r.read()
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), e.read()


def journal(n, relayer=RELAYER, domain=None):
    """The gas-drop journal: DROP_DOMAIN(32) root(32) chainId(32) vault(20) N(32) oneTime(32) relayer(32)."""
    d = domain or hashlib.sha256(b"ferrata/gas-drop/v1").digest()
    return list(d + bytes(32) + bytes(31) + b"\x01" + bytes(20) + n + bytes([7]) * 32 + relayer)


def job(n, inner=None, **kw):
    return {"kind": "gas-drop",
            "receipt": {"inner": inner or {"Test": "valid"}, "journal": {"bytes": journal(n, **kw)}, "metadata": {}},
            "oneTimeNpk": "0x" + "ab" * 32, "oneTimeVpk": "0x02" + "cd" * 600}


def main():
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="gw-gasdrop-", dir=os.environ.get("TEST_TMPDIR")))
    inbox = tmp / "inbox"
    inbox.mkdir()
    terms = tmp / "terms.json"
    guest = tmp / "gasdrop.bin"
    guest.write_bytes(b"\x7fELF" + bytes(1000))
    checker = tmp / "checker.py"
    checker.write_text(CHECKER)
    port = free_port()
    os.environ.update({
        "GATEWAY_LISTEN": f"127.0.0.1:{port}", "GATEWAY_UPSTREAM": "http://127.0.0.1:9",
        "GASDROP_INFO": "", "GASDROP_INBOX": str(inbox), "GASDROP_GUEST": str(guest), "GASDROP_MAX_BODY": "20000",
        "GASDROP_MAX_PENDING": "3", "GASDROP_PER_IP_PER_DAY": "2", "GASDROP_PER_NET_PER_DAY": "3",
        "GASDROP_GLOBAL_PER_HOUR": "4", "GASDROP_GET_RATE": "0.001", "GASDROP_GET_BURST": "50",
        "GASDROP_VERIFY_RATE": "0.001", "GASDROP_VERIFY_BURST": "6", "GASDROP_CHECK": "", "GASDROP_CHECK_TIMEOUT": "2",
        "GATEWAY_PER_IP_INFLIGHT": "6", "FAUCET_STATE": str(tmp / "faucet"),
    })
    loader = importlib.machinery.SourceFileLoader("gateway", str(GW))
    spec = importlib.util.spec_from_loader("gateway", loader)
    gw = importlib.util.module_from_spec(spec)
    loader.exec_module(gw)
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", port), gw.Handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    base = f"http://127.0.0.1:{port}/relayer/gas-drop"
    ok = 0
    ip = iter(f"203.0.{i // 200}.{i % 200 + 1}" for i in range(10000))  # fresh addresses, each its own check budget

    def check(cond, what):
        nonlocal ok
        if not cond:
            raise SystemExit(f"FAIL: {what}")
        ok += 1
        print(f"ok {ok}: {what}")

    def pending():
        return sorted(p.name for p in inbox.iterdir())

    # ---- the terms: 404 only when none are published ---------------------------------------------------------------
    c, _, _ = call(base)
    check(c == 404, "no GASDROP_INFO: no relayer published (404: the tool then pays from the user's own account)")
    c, _, _ = call(base, job(b"\x01" * 32))
    check(c == 404 and not pending(), "no GASDROP_INFO: a job is refused, nothing written")
    terms.write_text("{not json")
    gw.CFG["GASDROP_INFO"] = str(terms)
    check(call(base)[0] == 503, "terms set but unreadable: 503, not 404 (the tool stops rather than pay from the user's account)")
    check(call(base, job(b"\x01" * 32))[0] == 503 and not pending(), "terms unreadable: a job is refused 503, nothing written")
    t = {"relayer": "0x" + RELAYER.hex(), "imageId": IMAGE, "amount": 200000000, "covers": ["claim", "burn"],
         "chainId": 11155111, "vault": VAULT}
    terms.write_text(json.dumps({**t, "vault": "0x12"}))
    check(call(base)[0] == 503, "terms with a malformed vault: 503")
    terms.write_text(json.dumps(t))
    c, h, b = call(base)
    info = json.loads(b)
    check(c == 200 and info["relayer"] == t["relayer"] and info["amount"] == "200000000" and info["inbox"] == "/relayer/gas-drop"
          and info["guest"] == "/relayer/gas-drop/guest" and info["covers"] == ["claim", "burn"] and info["pending"] == 0
          and info["accepting"] is True, "the terms: relayer, amount as a string, covers, relative inbox and guest, accepting")
    check(info["relayFeeTo"] == t["relayer"] and "relay fee" in info["how"] and info["chainId"] == 11155111 and info["vault"] == VAULT,
          "the terms name who is paid the relay fee (the relayer the claim names), the chain and the vault")
    check(h.get("Access-Control-Allow-Origin") == "*", "CORS on the terms (the web wallet reads them)")
    c, h, b = call(base + "/guest")
    check(c == 200 and b == guest.read_bytes() and h.get("Content-Type") == "application/octet-stream", "the guest's bytes")

    # ---- verification before any cap ----------------------------------------------------------------------------------
    c, _, b = call(base, job(bytes([0x0a]) * 32), {"X-Real-IP": next(ip)})
    check(c == 503 and "checker" in json.loads(b)["error"] and not pending(),
          "no receipt checker configured: 503, nothing written or counted (fails closed)")
    gw.CFG["GASDROP_CHECK"] = f"{sys.executable} -I {checker}"
    for k in range(6):
        c, _, b = call(base, job(bytes([0x20 + k]) * 32, inner={"Fake": {}}), {"X-Real-IP": f"198.51.100.{100 + k}"})
        check(c == 400 and "does not check" in json.loads(b)["error"], f"fake receipt {k + 1} of 6 (one /24, the hourly cap is 4): refused 400")
    check(not pending() and len(gw.DROP_LIMITS.seen) == 0, "the six fake receipts left the inbox and every cap untouched")
    args = json.loads(pathlib.Path(str(checker) + ".args").read_text())
    check(args[:1] == ["check-job"] and args[args.index("--image-id") + 1] == IMAGE and args[args.index("--relayer") + 1] == t["relayer"]
          and args[args.index("--chain-id") + 1] == "11155111" and args[args.index("--vault") + 1] == VAULT
          and "--accept-dev-mode-receipts" not in args, "the checker is asked for the published image id, relayer, chain and vault")
    c, _, b = call(base, job(bytes([0x0b]) * 32, inner={"Test": "slow"}), {"X-Real-IP": next(ip)})
    check(c == 503 and "did not answer" in json.loads(b)["error"] and not pending(), "a checker past its timeout: 503, nothing counted")
    c, _, b = call(base, job(bytes([0x0c]) * 32, inner={"Test": "other-n"}), {"X-Real-IP": next(ip)})
    check(c == 400 and not pending(), "a checker's N other than the journal's: 400")
    budget = "198.51.100.250"
    codes = [call(base, job(bytes([0x30 + k]) * 32, inner={"Fake": {}}), {"X-Real-IP": budget})[0] for k in range(7)]
    check(codes == [400] * 6 + [429], f"one address's check budget (6) then 429, before any check ({codes})")
    check(not pending() and len(gw.DROP_LIMITS.seen) == 0, "still nothing counted")

    # ---- a verified job and the caps ------------------------------------------------------------------------------------
    n1 = bytes([0x11]) * 32
    log = io.StringIO()
    with contextlib.redirect_stdout(log):
        c, _, b = call(base, job(n1), {"X-Real-IP": "198.51.100.7"})
    files = sorted(inbox.iterdir())
    check(c == 202 and json.loads(b)["job"] == f"gas-drop-{n1.hex()}.json" and [f.name for f in files] == [f"gas-drop-{n1.hex()}.json"],
          "a verified job accepted (202) into the inbox, named by its N")
    check("198.51.100.7" not in log.getvalue() and n1.hex() not in log.getvalue() and "accepted" in log.getvalue(),
          f"the gateway's log line names neither the address nor N ({log.getvalue().strip()!r})")
    check(stat.S_IMODE(files[0].stat().st_mode) == 0o640, "the job is mode 0640 (the relayer reads it through the group)")
    check(json.loads(files[0].read_text()) == job(n1), "the job is written as posted")
    c, _, _ = call(base, job(n1), {"X-Real-IP": "198.51.100.8"})
    check(c == 409 and len(pending()) == 1, "the same N again while the inbox holds it: 409, nothing written")

    bad = job(bytes([0x12]) * 32)
    bad["extra"] = 1
    check(call(base, bad, {"X-Real-IP": next(ip)})[0] == 400, "a job with another key: 400")
    bad = job(bytes([0x12]) * 32)
    bad["oneTimeNpk"] = "0xAB"
    check(call(base, bad, {"X-Real-IP": next(ip)})[0] == 400, "a malformed one-time key: 400")
    check(call(base, job(bytes([0x12]) * 32, relayer=OTHER), {"X-Real-IP": next(ip)})[0] == 400, "a receipt bound to another relayer: 400")
    check(call(base, job(bytes([0x12]) * 32, domain=bytes(32)), {"X-Real-IP": next(ip)})[0] == 400, "a journal of another statement: 400")
    bad = job(bytes([0x12]) * 32)
    bad["receipt"]["journal"]["bytes"] = bad["receipt"]["journal"]["bytes"][:200]
    check(call(base, bad, {"X-Real-IP": next(ip)})[0] == 400, "a journal of the wrong length: 400")
    check(call(base, raw=b"{" + b" " * 30000 + b"}", headers={"X-Real-IP": next(ip)})[0] == 413, "a body over GASDROP_MAX_BODY: 413")
    check(call(base, raw=b"not json", headers={"X-Real-IP": next(ip)})[0] == 400, "not JSON: 400")

    c, _, b = call(base, job(bytes([0x13]) * 32), {"X-Real-IP": "198.51.100.7"})
    check(c == 202, "a second verified job from the same address")
    c, _, b = call(base, job(bytes([0x14]) * 32), {"X-Real-IP": "198.51.100.7"})
    check(c == 429 and "address" in json.loads(b)["error"], "a third from that address the same day: 429 (per IP)")
    c, _, b = call(base, job(bytes([0x15]) * 32), {"X-Real-IP": "198.51.100.9"})
    check(c == 202 and len(pending()) == 3, "another address of the network: accepted (three pending)")
    c, _, b = call(base, job(bytes([0x16]) * 32), {"X-Real-IP": "192.0.2.5"})
    check(c == 429 and "full" in json.loads(b)["error"], "the inbox at GASDROP_MAX_PENDING: 429")
    check(json.loads(call(base)[2])["accepting"] is False, "the terms say the inbox takes no jobs (the tool stops before proving)")
    (inbox / pending()[0]).unlink()  # the relayer takes one
    c, _, b = call(base, job(bytes([0x17]) * 32), {"X-Real-IP": "198.51.100.10"})
    check(c == 429 and "network" in json.loads(b)["error"], "a fourth job from one /24 the same day: 429 (per network)")
    c, _, b = call(base, job(bytes([0x18]) * 32), {"X-Real-IP": "192.0.2.5"})
    check(c == 202, "an address of another network: accepted")
    (inbox / pending()[0]).unlink()
    c, _, b = call(base, job(bytes([0x19]) * 32), {"X-Real-IP": "192.0.2.77"})
    check(c == 429 and "hour" in json.loads(b)["error"], "the fifth verified job in an hour, every caller together: 429")

    check(gw.check_drop_job(job(n1), RELAYER) == (n1.hex(), None), "check_drop_job returns N")
    terms.write_text(json.dumps({**t, "paused": "the drop account is being refilled"}))
    c, _, b = call(base)
    check(c == 503 and "refilled" in json.loads(b)["error"], "paused: the terms answer 503 with the reason (the tool stops)")
    check(call(base, job(bytes([0x1a]) * 32))[0] == 503, "paused: a job is refused with 503")
    print(f"all {ok} checks passed")


if __name__ == "__main__":
    main()
