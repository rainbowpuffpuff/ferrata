#!/usr/bin/env python3
"""The wallet against the public test network, read only: serve the bundle locally with /rpc proxied to the network
(test/serve.py --rpc https://HOST/rpc, no faucet), then run this with the page URL and an output file. It creates a
wallet in the page, reads the head, a balance, blocks and the network info, and checks that a send is refused before
signing; it asks no faucet and sends no transaction."""
import json, re, sys, time
from playwright.sync_api import sync_playwright
URL = sys.argv[1]
out = {"checks": []}
def check(n, ok, d=""): out["checks"].append({"check": n, "ok": bool(ok), "detail": str(d)[:200]}); print(("PASS " if ok else "FAIL ") + n, "--", str(d)[:160])
with sync_playwright() as p:
    b = p.chromium.launch(); pg = b.new_page(); errs = []; reqs = []
    pg.on("pageerror", lambda e: errs.append(str(e))); pg.on("request", lambda r: reqs.append(r.url))
    pg.goto(URL); pg.wait_for_function("() => /block \\d+/.test(document.getElementById('net-status').textContent)", timeout=30000)
    head = int(re.findall(r"\d+", pg.locator("#net-status").inner_text())[0]); check("the page reads the public network's head through /rpc", head > 0, head)
    pg.click("#go-create"); pg.check("#saved-words"); pg.fill("#create-pass", "read only check"); pg.fill("#create-pass2", "read only check"); pg.click("#create-go")
    pg.wait_for_selector("#v-main:not([hidden])", timeout=30000)
    pg.wait_for_function("() => document.getElementById('balance').textContent !== '—'", timeout=30000)
    check("a new account's balance and nonce read from the public RPC", pg.locator("#balance").inner_text() == "0 LGO" and pg.locator("#home-nonce").inner_text() == "0", pg.locator("#balance").inner_text())
    t0 = time.time(); pg.goto(URL + "#activity"); pg.wait_for_function("() => /read|Could not/.test(document.getElementById('act-status').textContent)", timeout=180000)
    st = pg.locator("#act-status").inner_text(); check("Activity reads the public network's blocks (getBlockRange, spans of 50)", "read" in st and "Could not" not in st, f"{st} in {time.time()-t0:.1f} s")
    pg.goto(URL + "#settings"); pg.wait_for_function("() => /Head block/.test(document.getElementById('net-info').textContent)", timeout=30000)
    info = pg.locator("#net-info").inner_text(); check("network info: rc3's privacy circuit on the public network", "631f0c96" in info, re.sub(r"https?://\S+", "<url>", info.replace("\n", " | "))[:200])
    pg.goto(URL + "#send"); pg.fill("#send-to", "1" * 44); pg.fill("#send-amount", "1"); pg.click("#send-review"); pg.wait_for_timeout(2000)
    check("a send without funds is refused before anything is signed", pg.locator("#send-err").inner_text() != "", pg.locator("#send-err").inner_text())
    sends = [u for u in reqs if "faucet" in u]; check("no faucet request was made", not sends, sends)
    check("no uncaught script error", not errs, errs); b.close()
out["passed"] = sum(c["ok"] for c in out["checks"]); out["failed"] = sum(not c["ok"] for c in out["checks"])
json.dump(out, open(sys.argv[2], "w"), indent=1); print(out["passed"], "passed", out["failed"], "failed")
