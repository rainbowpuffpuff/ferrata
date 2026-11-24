#!/usr/bin/env python3
"""Names live on OUR test network at its real /wallet/ URL (sending approved by the lead): A claims a fresh name with
the names program's commit-reveal (Commit, then Register once the reveal window opens, about four minutes later), Home shows
it; B sends to the name and the review shows the account it resolves to; the RPC confirms A was credited; B's Activity
names A. One faucet claim per run.

    python3 live_names.py https://HOST/wallet/ OUT_DIR"""
import json
import os
import re
import secrets
import string
import sys
import time
import urllib.parse
import urllib.request

from playwright.sync_api import sync_playwright

URL, OUT = sys.argv[1].rstrip("/") + "/", sys.argv[2]
U = urllib.parse.urlparse(URL)
RPC = f"{U.scheme}://{U.netloc}/rpc"
NAME = "w" + "".join(secrets.choice(string.ascii_lowercase + string.digits) for _ in range(9))
checks, numbers = [], {}


def check(name, ok, detail=""):
    d = re.sub(r"\b(0x)?[0-9a-f]{16,}\b|\b[1-9A-HJ-NP-Za-km-z]{43,44}\b", "<x>", str(detail)).replace(U.netloc, "<host>")[:300]
    checks.append({"check": name, "ok": bool(ok), "detail": d})
    print(("PASS " if ok else "FAIL ") + name + (" -- " + d[:200] if d else ""), flush=True)
    return ok


def bal(i):
    req = urllib.request.Request(RPC, data=json.dumps({"jsonrpc": "2.0", "id": 1, "method": "getAccountBalance", "params": [i]}).encode(),
                                 headers={"Content-Type": "application/json", "User-Agent": "ferrata-wallet-live-test"})
    with urllib.request.urlopen(req, timeout=20) as r:
        return int(json.loads(r.read())["result"])


def new_wallet(browser, pw):
    ctx = browser.new_context(viewport={"width": 1280, "height": 900})
    pg = ctx.new_page()
    pg.goto(URL)
    pg.click("#go-create")
    pg.check("#saved-words")
    pg.fill("#create-pass", pw)
    pg.fill("#create-pass2", pw)
    pg.click("#create-go")
    pg.wait_for_selector("#v-main:not([hidden])", timeout=30000)
    return ctx, pg


def send(pg, to, amount):
    pg.goto(URL + "#send")
    pg.fill("#send-to", to)
    pg.fill("#send-amount", amount)
    pg.click("#send-review")
    pg.wait_for_function("() => !document.getElementById('send-confirm').hidden || document.getElementById('send-err').textContent", timeout=60000)
    summary = pg.locator("#send-summary").inner_text()
    pg.click("#send-go")
    pg.wait_for_function("() => /In block|Not in a block|rror|reverted/.test(document.getElementById('send-result').textContent)", timeout=240000)
    return summary, pg.locator("#send-result").inner_text()


def steps():
    with sync_playwright() as p:
        b = p.chromium.launch()
        ctx_a, a = new_wallet(b, "live names A")
        ctx_b, bb = new_wallet(b, "live names B")
        id_a = a.locator("#home-id").inner_text().strip()
        bb.goto(URL + "#receive")
        code_b = bb.locator("#recv-code").inner_text().strip()
        a.goto(URL + "#home")
        a.click("#faucet")
        a.wait_for_function("() => /Received|failed|not arrived|rror|Faucet/.test(document.getElementById('faucet-msg').textContent)", timeout=240000)
        check("A is funded by the faucet", "Received" in a.locator("#faucet-msg").inner_text(), a.locator("#faucet-msg").inner_text())
        check("Home offers a name (the network publishes the names program)", a.locator("#names-box").is_visible())
        a.fill("#name-input", NAME)
        a.click("#name-claim")
        a.wait_for_function("() => /Committed|refused|rror|needs/.test(document.getElementById('name-msg').textContent)", timeout=240000)
        msg = a.locator("#name-msg").inner_text()
        check("Claim, step 1: the commitment is in a block (the name stays hidden)", "Committed" in msg, msg)
        t0 = time.time()
        a.wait_for_function("() => /^Reveal [a-z0-9-]+$/.test(document.getElementById('name-claim').textContent)", timeout=600000)
        numbers["reveal_wait_seconds"] = round(time.time() - t0, 1)
        a.click("#name-claim")
        a.wait_for_function("() => /is yours|refused|rror|passed/.test(document.getElementById('name-msg').textContent)", timeout=240000)
        msg = a.locator("#name-msg").inner_text()
        check("Claim, step 2: the reveal registers the name", "is yours" in msg, msg)
        a.goto(URL + "#home")
        a.wait_for_function("(n) => document.getElementById('home-name').textContent === n + '.lez'", arg=NAME, timeout=60000)
        check("Home shows the account's name (reverse, checked against the record)", True, a.locator("#home-name").inner_text())

        summary, res = send(a, code_b, "1")
        check("A funds B", "applied" in res, res)
        before = bal(id_a)
        summary, res = send(bb, NAME, "0.25")
        check("B sends to the name; the review shows the account it resolves to", NAME in summary and id_a in summary, summary[:200])
        check("the transfer to the name is applied and A is credited", "applied" in res and bal(id_a) == before + 250_000_000, res)
        bb.goto(URL + "#activity")
        bb.wait_for_function("(n) => document.getElementById('act-list').textContent.includes(n + '.lez')", arg=NAME, timeout=120000)
        check("B's Activity names the recipient", True)
        bb.screenshot(path=os.path.join(OUT, "names-activity.png"), full_page=True)
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
