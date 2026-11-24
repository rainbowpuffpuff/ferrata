#!/usr/bin/env python3
"""The wallet end to end on OUR public test network, at its real /wallet/ URL (sending there was approved by the lead
for this run; never Sepolia or mainnet). One faucet claim per run (the faucet allows three per IP per day).

    python3 live_e2e.py https://HOST/wallet/ OUT_DIR

Creates a wallet in the page, takes the faucet's coins (with the account's public key), sends to a second account of
the same phrase by its receive code (a never-used account: its key goes as admission evidence), sends back by id,
reads Activity, locks, unlocks, reloads, imports the phrase in a second browser, and checks each step against the
network's RPC directly. The record names accounts and hashes by role and never names the host."""
import hashlib
import json
import os
import re
import sys
import time
import urllib.parse
import urllib.request

from playwright.sync_api import sync_playwright

URL = sys.argv[1].rstrip("/") + "/"
OUT = sys.argv[2]
U = urllib.parse.urlparse(URL)
ORIGIN = f"{U.scheme}://{U.netloc}"
RPC = ORIGIN + "/rpc"
PASS = "live test network only"
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
UNIT = 10**9
checks, numbers, labels, requests_seen = [], {}, {}, []


def b58(b):
    n = int.from_bytes(b, "big"); s = ""
    while n:
        n, r = divmod(n, 58); s = B58[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s


def account_id_of(pk_hex):
    return b58(hashlib.sha256(b"/LEE/v0.3/AccountId/Public/".ljust(32, b"\0") + bytes.fromhex(pk_hex)).digest())


def label(text):
    def sub(m):
        v = m.group(0)
        if v not in labels:
            labels[v] = f"<{'hash' if re.fullmatch('(0x)?[0-9a-f]{64}', v) else 'id'}{len(labels) + 1}>"
        return labels[v]
    text = text.replace(ORIGIN, "<host>").replace(U.netloc, "<host>")
    return re.sub(r"(0x)?\b[0-9a-f]{64}\b|\b[1-9A-HJ-NP-Za-km-z]{43,44}\b", sub, text)


def check(name, ok, detail=""):
    checks.append({"check": name, "ok": bool(ok), "detail": label(str(detail))[:300]})
    print(("PASS " if ok else "FAIL ") + name + (" -- " + label(str(detail))[:200] if detail else ""), flush=True)
    return ok


def rpc(method, params=None):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or []}).encode()
    req = urllib.request.Request(RPC, data=body, headers={"Content-Type": "application/json", "User-Agent": "ferrata-wallet-live-test"})
    with urllib.request.urlopen(req, timeout=20) as r:
        j = json.loads(r.read())
    if "error" in j:
        raise RuntimeError(f"{method}: {j['error']}")
    return j["result"]


def bal(i): return int(rpc("getAccountBalance", [i]))
def nonce(i): return int(rpc("getAccountsNonces", [[i]])[0])
def text(p, sel): return p.locator(sel).inner_text().strip()


def send(page, to, amount):
    page.goto(URL + "#send")
    page.fill("#send-to", to)
    page.fill("#send-amount", amount)
    page.click("#send-review")
    page.wait_for_function("() => !document.getElementById('send-confirm').hidden || document.getElementById('send-err').textContent", timeout=30000)


def steps():
    with sync_playwright() as p:
        browser = p.chromium.launch()
        ctx = browser.new_context(viewport={"width": 1280, "height": 900})
        page = ctx.new_page()
        console = []
        page.on("request", lambda r: requests_seen.append(r.url))
        page.on("console", lambda m: console.append(f"{m.type}: {m.text}"))
        page.on("pageerror", lambda e: console.append(f"pageerror: {e}"))
        resp = page.goto(URL)
        csp = resp.headers.get("content-security-policy", "")
        check("the network serves the wallet with its CSP (own origin only, no framing)", "connect-src 'self'" in csp and "frame-ancestors 'none'" in csp, csp[:120])
        page.wait_for_function("() => /block \\d+/.test(document.getElementById('net-status').textContent)", timeout=30000)
        check("the page reads the network's head", True, text(page, "#net-status"))
        page.click("#go-create")
        words = [w.strip() for w in page.locator("#mnemonic-words li").all_text_contents()]
        page.check("#saved-words")
        page.fill("#create-pass", PASS)
        page.fill("#create-pass2", PASS)
        page.click("#create-go")
        page.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        id1 = text(page, "#home-id")
        check("a new wallet in the page: 24 words, its first account", len(words) == 24 and re.fullmatch("[1-9A-HJ-NP-Za-km-z]{43,44}", id1), id1)
        check("the account is new on the network", rpc("getAccount", [id1]) is None)

        # Faucet (the network's own, with the public key: the account is new)
        terms = json.loads(urllib.request.urlopen(urllib.request.Request(ORIGIN + "/faucet", headers={"User-Agent": "ferrata-wallet-live-test"}), timeout=20).read())
        amount = int(terms.get("amount", 0))
        t0 = time.time()
        page.click("#faucet")
        page.wait_for_function("() => /Received|failed|not arrived|HTTP|rror|Faucet/.test(document.getElementById('faucet-msg').textContent)", timeout=240000)
        numbers["faucet_seconds"] = round(time.time() - t0, 1)
        check("the faucet button funds the new account", "Received" in text(page, "#faucet-msg"), text(page, "#faucet-msg"))
        check("the RPC agrees: the faucet's amount", bal(id1) == amount, f"{bal(id1)} vs {amount}")

        # A second account of the same phrase; send to it by receive code (first credit: key as evidence)
        page.click("#add-account")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent !== a", arg=id1, timeout=15000)
        page.goto(URL + "#receive")
        code2, id2, pk2 = text(page, "#recv-code"), text(page, "#recv-id"), text(page, "#recv-pk")
        check("account 2: own id, receive code with its key", id2 != id1 and code2 == f"lez:{id2}?pk={pk2}" and account_id_of(pk2) == id2)
        page.select_option("#account-select", "0")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent === a", arg=id1, timeout=15000)
        send(page, code2, "1.25")
        check("the review is shown before signing (first credit flagged)", page.locator("#send-confirm").is_visible() and "First credit" in text(page, "#send-summary"), (text(page, "#send-summary") or text(page, "#send-err"))[:200])
        t0 = time.time()
        page.click("#send-go")
        page.wait_for_function("() => /In block|Not in a block|rror|reverted/.test(document.getElementById('send-result').textContent)", timeout=300000)
        numbers["send1_seconds_to_block"] = round(time.time() - t0, 1)
        res = text(page, "#send-result")
        check("the page reports the transfer applied in a block", "applied" in res, res)
        check("the RPC agrees: account 2 holds 1.25 LGO", bal(id2) == 1_250_000_000, bal(id2))
        fee = amount - 1_250_000_000 - bal(id1)
        numbers["send1_fee_atomic"] = fee
        check("the RPC agrees: account 1 paid 1.25 LGO and a fee; nonce 1", nonce(id1) == 1 and 0 < fee < 50_000_000, fee)
        h = re.findall(r"\b[0-9a-f]{64}\b", res)
        check("the hash the page shows is the network's transaction", bool(h) and rpc("getTransaction", [h[-1]]) is not None, h[-1:] if h else res)

        page.select_option("#account-select", "1")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent === a", arg=id2, timeout=15000)
        b1 = bal(id1)
        send(page, id1, "0.5")
        page.click("#send-go")
        page.wait_for_function("() => /In block|Not in a block|rror|reverted/.test(document.getElementById('send-result').textContent)", timeout=300000)
        check("account 2 sends back by id; applied", "applied" in text(page, "#send-result"), text(page, "#send-result"))
        check("the RPC agrees: +0.5 LGO exactly", bal(id1) == b1 + 500_000_000, bal(id1) - b1)

        page.select_option("#account-select", "0")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent === a", arg=id1, timeout=15000)
        page.goto(URL + "#activity")
        page.wait_for_function("() => /read|Could not/.test(document.getElementById('act-status').textContent)", timeout=180000)
        page.wait_for_timeout(500)
        items = "\n".join(page.locator("#act-list li[data-testid=act-item]").all_inner_texts())
        check("Activity lists the faucet's credit, the send and the return, none reverted",
              "−1.25 LGO" in items and "+0.5 LGO" in items and f"+{amount // UNIT} LGO" in items and "reverted" not in items, items[:300])

        page.goto(URL + "#home")
        page.click("#lock")
        page.fill("#unlock-pass", PASS)
        page.click("#unlock-go")
        page.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        page.reload()
        page.wait_for_selector("#v-unlock:not([hidden])", timeout=15000)
        page.fill("#unlock-pass", PASS)
        page.click("#unlock-go")
        page.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        check("lock, unlock and a reload keep both accounts", page.locator("#account-select option").count() == 2 and text(page, "#home-id") == id1)

        ctx2 = browser.new_context(viewport={"width": 390, "height": 844}, is_mobile=True, has_touch=True)
        p2 = ctx2.new_page()
        p2.on("request", lambda r: requests_seen.append(r.url))
        p2.goto(URL)
        p2.click("#go-import")
        p2.fill("#import-phrase", " ".join(words))
        p2.fill("#import-pass", PASS)
        p2.click("#import-go")
        p2.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        p2.wait_for_function("() => document.getElementById('balance').textContent !== '—'", timeout=30000)
        check("the phrase imported on a phone-sized screen opens the same account and balance",
              text(p2, "#home-id") == id1 and p2.locator("#tabs").bounding_box()["y"] > 700, text(p2, "#balance"))
        p2.screenshot(path=os.path.join(OUT, "phone-home.png"))
        ctx2.close()

        others = sorted({u for u in requests_seen if not u.startswith(ORIGIN + "/")})
        check("every request the page made went to the network's own origin", not others, others)
        check("no uncaught script error", not [c for c in console if c.startswith("pageerror")], [c for c in console if c.startswith("pageerror")])
        browser.close()


def main():
    os.makedirs(OUT, exist_ok=True)
    try:
        steps()
    except Exception as e:  # noqa: BLE001
        check("the run completed without an unexpected error", False, repr(e)[:300])
    rec = {"when_utc": time.strftime("%Y-%m-%dT%H:%MZ", time.gmtime()), "passed": sum(c["ok"] for c in checks),
           "failed": sum(not c["ok"] for c in checks), "numbers": numbers, "checks": checks}
    with open(os.path.join(OUT, "record.json"), "w") as f:
        json.dump(rec, f, indent=1)
    print(f"{rec['passed']} passed, {rec['failed']} failed; {numbers}")
    return 0 if rec["failed"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
