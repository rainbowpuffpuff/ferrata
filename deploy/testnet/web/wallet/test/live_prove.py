#!/usr/bin/env python3
"""Prove an Ethereum fact on OUR public test network from its real /wallet/ URL (sending there approved by the lead):
a new wallet funded by the faucet attests Sepolia USDC's storage slot 11 through the attestation program. The page's
Ethereum reads go through a stand-in browser wallet (EIP-1193) that forwards read-only calls to a public Sepolia
endpoint; no Ethereum transaction is sent.

    python3 live_prove.py https://HOST/wallet/ https://SEPOLIA-RPC OUT_DIR"""
import json
import os
import re
import sys
import time
import urllib.request

from playwright.sync_api import sync_playwright

URL, ETH, OUT = sys.argv[1].rstrip("/") + "/", sys.argv[2], sys.argv[3]
READS = {"eth_chainId", "eth_getBlockByHash", "eth_getBlockByNumber", "eth_getProof", "eth_blockNumber", "eth_requestAccounts", "eth_accounts"}
checks, numbers = [], {}


def check(name, ok, detail=""):
    d = re.sub(r"\b(0x)?[0-9a-f]{16,}\b|\b[1-9A-HJ-NP-Za-km-z]{43,44}\b", "<x>", str(detail))[:300]
    checks.append({"check": name, "ok": bool(ok), "detail": d})
    print(("PASS " if ok else "FAIL ") + name + (" -- " + d[:200] if d else ""), flush=True)


def eth_forward(method, params):
    if method not in READS:
        return {"error": {"code": 4200, "message": f"{method} is not allowed by this read-only stand-in"}}
    if method in ("eth_requestAccounts", "eth_accounts"):
        return {"result": []}
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(ETH, data=body, headers={"Content-Type": "application/json", "User-Agent": "ferrata-wallet-live-test"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read())


STAND_IN = """window.ethereum = { isStandIn: true, async request({ method, params }) {
  const r = await window.__ethRead(method, params || []);
  if (r.error) { const e = new Error(r.error.message); e.code = r.error.code; throw e; }
  return r.result; } };"""


def steps():
    with sync_playwright() as p:
        b = p.chromium.launch()
        ctx = b.new_context(viewport={"width": 1280, "height": 900})
        ctx.expose_function("__ethRead", eth_forward)
        ctx.add_init_script(STAND_IN)
        pg = ctx.new_page()
        pg.goto(URL)
        pg.click("#go-create")
        pg.check("#saved-words")
        pg.fill("#create-pass", "live prove test")
        pg.fill("#create-pass2", "live prove test")
        pg.click("#create-go")
        pg.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        pg.click("#faucet")
        pg.wait_for_function("() => /Received|failed|not arrived|rror|Faucet/.test(document.getElementById('faucet-msg').textContent)", timeout=240000)
        check("the faucet funds the attesting account", "Received" in pg.locator("#faucet-msg").inner_text(), pg.locator("#faucet-msg").inner_text())
        pg.goto(URL + "#prove")
        pg.wait_for_function("() => /Newest record|Could not/.test(document.querySelector('[data-testid=prove-record]').textContent)", timeout=60000)
        rec = pg.locator("[data-testid=prove-record]").inner_text()
        check("the Prove tab reads the instance's newest record from LEZ", "Newest record" in rec, rec)
        pg.fill("[data-testid=prove-address]", "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238")
        pg.fill("[data-testid=prove-slot]", "11")
        t0 = time.time()
        pg.click("[data-testid=prove-check]")
        pg.wait_for_function("() => /holds|rror|not|No |chain/.test(document.querySelector('[data-testid=prove-out]').textContent)", timeout=180000)
        out = pg.locator("[data-testid=prove-out]").inner_text()
        numbers["check_seconds"] = round(time.time() - t0, 1)
        check("the page fetched eth_getProof through the browser wallet and checked it against the record's block", "proof holds" in out, out + " | " + pg.locator("[data-testid=prove-facts]").inner_text()[:200])
        pg.screenshot(path=os.path.join(OUT, "prove-checked.png"), full_page=True)
        if "proof holds" not in out:
            return
        t0 = time.time()
        pg.click("[data-testid=prove-send]")
        pg.wait_for_function("() => /Attested|Refused|Not in a block|rror|needs/.test(document.querySelector('[data-testid=prove-out]').textContent)", timeout=300000)
        numbers["attest_seconds_to_block"] = round(time.time() - t0, 1)
        res = pg.locator("[data-testid=prove-out]").inner_text()
        check("the attestation program accepted the fact on our network (not in the block's reverted list)", res.startswith("Attested"), res)
        pg.screenshot(path=os.path.join(OUT, "prove-attested.png"), full_page=True)
        b.close()


def main():
    os.makedirs(OUT, exist_ok=True)
    try:
        steps()
    except Exception as e:  # noqa: BLE001
        check("the run completed without an unexpected error", False, repr(e)[:300])
    rec = {"when_utc": time.strftime("%Y-%m-%dT%H:%MZ", time.gmtime()), "passed": sum(c["ok"] for c in checks),
           "failed": sum(not c["ok"] for c in checks), "numbers": numbers, "checks": checks}
    json.dump(rec, open(os.path.join(OUT, "record.json"), "w"), indent=1)
    print(rec["passed"], "passed,", rec["failed"], "failed;", numbers)
    return 0 if rec["failed"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
