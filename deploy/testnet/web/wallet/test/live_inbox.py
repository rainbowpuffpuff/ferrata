#!/usr/bin/env python3
"""The encrypted inbox live on OUR test network at its real /wallet/ URL (sending approved by the lead): two browsers,
A and B, each with its own wallet. A takes the faucet and funds B; B publishes its messaging key; A writes; B reads the
message decrypted in its page; B switches to contacts-only and A's next message is refused before any fee; B adds A as
a contact and A's message arrives; B blocks A and A's next one is refused before any fee. The plaintext is checked to
be absent from B's mailbox as the RPC serves it. One faucet claim per run.

    python3 live_inbox.py https://HOST/wallet/ OUT_DIR"""
import json
import os
import re
import sys
import time
import urllib.parse
import urllib.request

from playwright.sync_api import sync_playwright

URL, OUT = sys.argv[1].rstrip("/") + "/", sys.argv[2]
U = urllib.parse.urlparse(URL)
RPC = f"{U.scheme}://{U.netloc}/rpc"
checks, numbers = [], {}


def check(name, ok, detail=""):
    d = re.sub(r"\b(0x)?[0-9a-f]{16,}\b|\b[1-9A-HJ-NP-Za-km-z]{43,44}\b", "<x>", str(detail)).replace(U.netloc, "<host>")[:300]
    checks.append({"check": name, "ok": bool(ok), "detail": d})
    print(("PASS " if ok else "FAIL ") + name + (" -- " + d[:200] if d else ""), flush=True)
    return ok


def rpc(method, params):
    req = urllib.request.Request(RPC, data=json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode(),
                                 headers={"Content-Type": "application/json", "User-Agent": "ferrata-wallet-live-test"})
    with urllib.request.urlopen(req, timeout=20) as r:
        return json.loads(r.read()).get("result")


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


def inbox_out(pg, pattern, timeout=240000):
    pg.wait_for_function(f"() => /{pattern}/.test(document.querySelector('[data-testid=inbox-out]').textContent)", timeout=timeout)
    return pg.locator("[data-testid=inbox-out]").inner_text()


def send_msg(pg, to, text):
    pg.goto(URL + "#inbox")
    pg.wait_for_function("() => !/^…$/.test(document.querySelector('[data-testid=inbox-status]').textContent)", timeout=30000)
    pg.fill("[data-testid=inbox-to]", to)
    pg.fill("[data-testid=inbox-text]", text)
    pg.evaluate("() => document.querySelector('[data-testid=inbox-out]').textContent = ''")
    pg.click("[data-testid=inbox-send]")
    return inbox_out(pg, "Send: done|Not sent|refused|rror|not in a block")


def steps():
    with sync_playwright() as p:
        b = p.chromium.launch()
        ctx_a, a = new_wallet(b, "live inbox A")
        ctx_b, bb = new_wallet(b, "live inbox B")
        id_a, id_b = a.locator("#home-id").inner_text().strip(), bb.locator("#home-id").inner_text().strip()
        bb.goto(URL + "#receive")
        code_b = bb.locator("#recv-code").inner_text().strip()
        a.click("#faucet")
        a.wait_for_function("() => /Received|failed|not arrived|rror|Faucet/.test(document.getElementById('faucet-msg').textContent)", timeout=240000)
        check("A is funded by the network's faucet", "Received" in a.locator("#faucet-msg").inner_text(), a.locator("#faucet-msg").inner_text())
        a.goto(URL + "#send")
        a.fill("#send-to", code_b)
        a.fill("#send-amount", "1")
        a.click("#send-review")
        a.wait_for_function("() => !document.getElementById('send-confirm').hidden || document.getElementById('send-err').textContent", timeout=30000)
        a.click("#send-go")
        a.wait_for_function("() => /In block|Not in a block|rror|reverted/.test(document.getElementById('send-result').textContent)", timeout=240000)
        check("A funds B (1 LGO, B's first credit)", "applied" in a.locator("#send-result").inner_text(), a.locator("#send-result").inner_text())

        bb.goto(URL + "#inbox")
        bb.wait_for_function("() => !/^…$/.test(document.querySelector('[data-testid=inbox-status]').textContent)", timeout=30000)
        t0 = time.time()
        bb.click("[data-testid=inbox-publish]")
        res = inbox_out(bb, "Publish key: done|refused|rror|not in a block")
        numbers["publish_seconds"] = round(time.time() - t0, 1)
        check("B publishes its messaging key", "done" in res, res)
        bb.wait_for_function("() => /fingerprint/.test(document.querySelector('[data-testid=inbox-status]').textContent)", timeout=30000)
        check("B's page shows the key's fingerprint", "fingerprint" in bb.locator("[data-testid=inbox-status]").inner_text(), bb.locator("[data-testid=inbox-status]").inner_text())

        secret = f"hello B, from A, live {int(time.time())}"
        t0 = time.time()
        res = send_msg(a, id_b, secret)
        numbers["send_seconds"] = round(time.time() - t0, 1)
        check("A sends an encrypted message to B", "Send: done" in res, res)
        bb.goto(URL + "#home"); bb.goto(URL + "#inbox")
        bb.wait_for_function(f"(t) => document.querySelector('[data-testid=inbox-list]').textContent.includes(t)", arg=secret, timeout=60000)
        check("B reads it, decrypted in its own page", True, "")
        acct = rpc("getAccount", [id_b])
        raw = json.dumps(acct)
        check("the RPC's view of B's mailbox does not contain the text", secret not in raw and secret.encode().hex() not in raw.replace(",", ""), len(raw))

        bb.select_option("[data-testid=inbox-policy]", "contactsOnly")
        res = inbox_out(bb, "Policy: done|refused|rror|not in a block")
        check("B accepts contacts only", "done" in res, res)
        n0 = int(rpc("getAccountsNonces", [[id_a]])[0])
        res = send_msg(a, id_b, "should not arrive")
        check("A's message to a contacts-only B is refused before any fee", "NOT_A_CONTACT" in res and int(rpc("getAccountsNonces", [[id_a]])[0]) == n0, res)

        bb.goto(URL + "#home"); bb.goto(URL + "#inbox")
        bb.wait_for_selector("[data-testid=inbox-contact]", timeout=30000)
        bb.locator("[data-testid=inbox-contact]").first.click()
        res = inbox_out(bb, "Add contact: done|refused|rror|not in a block")
        check("B adds A as a contact", "done" in res, res)
        res = send_msg(a, id_b, "second, as a contact")
        check("A's message as a contact arrives", "Send: done" in res, res)

        bb.goto(URL + "#home"); bb.goto(URL + "#inbox")
        bb.wait_for_function("() => document.querySelector('[data-testid=inbox-list]').textContent.includes('second, as a contact')", timeout=60000)
        bb.locator("[data-testid=inbox-block]").first.click()
        res = inbox_out(bb, "Block: done|refused|rror|not in a block")
        check("B blocks A", "done" in res, res)
        n0 = int(rpc("getAccountsNonces", [[id_a]])[0])
        res = send_msg(a, id_b, "blocked")
        check("A's message to B after the block is refused before any fee", "BLOCKED" in res and int(rpc("getAccountsNonces", [[id_a]])[0]) == n0, res)
        bb.screenshot(path=os.path.join(OUT, "inbox-b.png"), full_page=True)
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
