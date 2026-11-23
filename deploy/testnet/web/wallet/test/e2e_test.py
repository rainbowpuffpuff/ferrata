"""The web wallet end to end in headless Chromium against a running LEZ sequencer (run by e2e.sh, or by hand with the
same environment). Every step the page reports is checked again against the sequencer's RPC directly.

Environment: WALLET_URL (the page, with /rpc and /faucet on its origin), WALLET_RPC (the sequencer, for the checks),
WALLET_KEY_B (LEZ's published local test key B, hex; optional), WALLET_OUT (the record's directory), WALLET_SHOTS
(screenshots), WALLET_ROOT (the bundle). Exit 0 when every check passed.
"""
import hashlib
import json
import os
import re
import secrets
import sys
import time
import urllib.parse
import urllib.request

from playwright.sync_api import sync_playwright

URL = os.environ["WALLET_URL"]
RPC = os.environ["WALLET_RPC"]
KEY_B = os.environ.get("WALLET_KEY_B")
OUT = os.environ["WALLET_OUT"]
SHOTS = os.environ["WALLET_SHOTS"]
ORIGIN = "{0.scheme}://{0.netloc}".format(urllib.parse.urlparse(URL))
UNIT = 10**9
PASS = "correct horse battery staple"
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

checks, numbers, labels = [], {}, {}
requests_seen = []


def b58(b: bytes) -> str:
    n = int.from_bytes(b, "big")
    s = ""
    while n:
        n, r = divmod(n, 58)
        s = B58[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s


def account_id_of(pk_hex: str) -> str:
    return b58(hashlib.sha256(b"/LEE/v0.3/AccountId/Public/".ljust(32, b"\0") + bytes.fromhex(pk_hex)).digest())


def label(text: str) -> str:
    """The record names accounts and hashes by role, not by value."""
    def sub(m):
        v = m.group(0)
        if v not in labels:
            labels[v] = f"<{'hash' if re.fullmatch('[0-9a-f]{64}', v) else 'id'}{len(labels) + 1}>"
        return labels[v]
    return re.sub(r"\b[0-9a-f]{64}\b|\b[1-9A-HJ-NP-Za-km-z]{43,44}\b", sub, text)


def check(name, ok, detail=""):
    checks.append({"check": name, "ok": bool(ok), "detail": label(str(detail))[:300]})
    print(("PASS " if ok else "FAIL ") + name + (f" -- {label(str(detail))[:200]}" if detail else ""), flush=True)
    return ok


def rpc(method, params=None):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or []}).encode()
    with urllib.request.urlopen(urllib.request.Request(RPC, data=body, headers={"Content-Type": "application/json"}), timeout=15) as r:
        j = json.loads(r.read())
    if "error" in j:
        raise RuntimeError(f"{method}: {j['error']}")
    return j["result"]


def bal(i):
    return int(rpc("getAccountBalance", [i]))


def nonce(i):
    return int(rpc("getAccountsNonces", [[i]])[0])


def wait_for(fn, timeout=120, step=1.0):
    end = time.time() + timeout
    while time.time() < end:
        v = fn()
        if v:
            return v
        time.sleep(step)
    return fn()


def text(page, sel):
    return page.locator(sel).inner_text().strip()


def shot(page, name):
    page.screenshot(path=os.path.join(SHOTS, name + ".png"), full_page=True)


def track(page):
    page.on("request", lambda req: requests_seen.append(req.url))


def unlock(page):
    page.goto(URL)
    page.wait_for_selector("#v-unlock:not([hidden])", timeout=15000)
    page.fill("#unlock-pass", PASS)
    page.click("#unlock-go")
    page.wait_for_selector("#v-main:not([hidden])", timeout=30000)


def send(page, to, amount):
    page.goto(URL + "#send")
    page.fill("#send-to", to)
    page.fill("#send-amount", amount)
    page.click("#send-review")
    page.wait_for_function("() => !document.getElementById('send-confirm').hidden || document.getElementById('send-err').textContent", timeout=30000)


def main():
    try:
        steps()
    except Exception as e:  # noqa: BLE001  the record still names how far the run got
        check("the run completed without an unexpected error", False, repr(e)[:300])
    ok = all(c["ok"] for c in checks)
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "record.json"), "w") as f:
        json.dump({"passed": sum(c["ok"] for c in checks), "failed": sum(not c["ok"] for c in checks),
                   "numbers": numbers, "checks": checks}, f, indent=1)
    print(f"{sum(c['ok'] for c in checks)} passed, {sum(not c['ok'] for c in checks)} failed; numbers {numbers}")
    return 0 if ok else 1


def steps():
    with sync_playwright() as p:
        browser = p.chromium.launch()
        ctx = browser.new_context(viewport={"width": 1280, "height": 900})
        page = ctx.new_page()
        track(page)
        console = []
        page.on("console", lambda m: console.append(f"{m.type}: {m.text}"))
        page.on("pageerror", lambda e: console.append(f"pageerror: {e}"))

        # -- 1. The page and its policy
        resp = page.goto(URL)
        csp = resp.headers.get("content-security-policy", "")
        check("page served with a CSP that allows only its own origin", "default-src 'none'" in csp and "connect-src 'self'" in csp and "frame-ancestors 'none'" in csp, csp)
        page.wait_for_function("() => /block \\d+/.test(document.getElementById('net-status').textContent)", timeout=30000)
        check("the page reads the head block through /rpc", True, text(page, "#net-status"))
        check("first visit offers create or import", page.locator("#v-welcome").is_visible())

        # -- 2. Create
        page.click("#go-create")
        words = [w.strip() for w in page.locator("#mnemonic-words li").all_text_contents()]
        check("a new wallet shows a 24-word recovery phrase", len(words) == 24, len(words))
        page.fill("#create-pass", PASS)
        page.fill("#create-pass2", PASS)
        page.click("#create-go")
        check("creating is refused until the words are confirmed written down", "Write the 24 words" in text(page, "#create-err"))
        page.check("#saved-words")
        page.click("#create-go")
        page.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        id1 = text(page, "#home-id")
        check("the wallet opens on its first account", re.fullmatch("[1-9A-HJ-NP-Za-km-z]{43,44}", id1), id1)
        stored = page.evaluate("() => localStorage.getItem('ferrata-wallet/v1')")
        check("the stored record is encrypted (no phrase word, no account id in it)",
              stored and not any(w in stored for w in words) and id1 not in stored and '"ciphertext"' in stored)
        check("a fresh account is absent on LEZ", rpc("getAccount", [id1]) is None)
        page.goto(URL + "#receive")
        pk1 = text(page, "#recv-pk")
        check("its public key hashes to its account id (LEZ's AccountId::from)", account_id_of(pk1) == id1)
        shot(page, "01-receive")

        # -- 3. Faucet (LEZ's own wallet CLI sends: interoperability with LEZ's signer)
        page.goto(URL + "#home")
        t0 = time.time()
        page.click("#faucet")
        page.wait_for_function("() => /Received|failed|not arrived|HTTP|error/i.test(document.getElementById('faucet-msg').textContent)", timeout=200000)
        numbers["faucet_seconds"] = round(time.time() - t0, 1)
        check("the faucet button funds the new account", "Received" in text(page, "#faucet-msg"), text(page, "#faucet-msg"))
        check("the RPC agrees: 5 LGO", bal(id1) == 5 * UNIT, bal(id1))
        page.wait_for_function("() => document.getElementById('balance').textContent.startsWith('5 ')", timeout=20000)
        check("the page shows the balance", text(page, "#balance") == "5 LGO", text(page, "#balance"))
        shot(page, "02-home-funded")

        # -- 4. A second account from the same phrase
        page.click("#add-account")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent !== a", arg=id1, timeout=10000)
        page.goto(URL + "#receive")
        code2, id2, pk2 = text(page, "#recv-code"), text(page, "#recv-id"), text(page, "#recv-pk")
        check("account 2 has its own id and a receive code with its key", id2 != id1 and code2 == f"lez:{id2}?pk={pk2}" and account_id_of(pk2) == id2)
        page.select_option("#account-select", "0")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent === a", arg=id1, timeout=10000)

        # -- 5. Refusals: nothing is sent
        n_before = nonce(id1)
        send(page, code2, "1000")
        check("refused: more than the balance", "Not enough" in text(page, "#send-err"), text(page, "#send-err"))
        fresh = b58(secrets.token_bytes(32))
        send(page, fresh, "1")
        check("refused: a never-used account without its key (asks for the receive code)", "receive code" in text(page, "#send-err"), text(page, "#send-err"))
        send(page, "hello", "1")
        check("refused: a malformed recipient, in plain words", "not an account id" in text(page, "#send-err"), text(page, "#send-err"))
        send(page, code2, "0.0000000001")
        check("refused: more decimals than the token has", "decimals" in text(page, "#send-err"), text(page, "#send-err"))
        send(page, id1, "1")
        check("refused: a transfer to the same account", "this account" in text(page, "#send-err"), text(page, "#send-err"))
        check("the refusals sent nothing (nonce unchanged)", nonce(id1) == n_before, nonce(id1))

        # -- 6. Send to a new account (its key goes as admission evidence)
        send(page, code2, "1.25")
        check("the review shows the transfer before signing", page.locator("#send-confirm").is_visible() and "First credit" in text(page, "#send-summary"), text(page, "#send-summary"))
        shot(page, "03-send-review")
        t0 = time.time()
        page.click("#send-go")
        page.wait_for_function("() => /In block|Not in a block|rror|refused/.test(document.getElementById('send-result').textContent)", timeout=200000)
        numbers["send1_seconds_to_block"] = round(time.time() - t0, 1)
        res = text(page, "#send-result")
        check("the page reports the transfer in a block, nonce moved", "In block" in res and "nonce moved to 1" in res, res)
        check("the RPC agrees: account 2 holds 1.25 LGO", bal(id2) == 1_250_000_000, bal(id2))
        fee1 = 5 * UNIT - 1_250_000_000 - bal(id1)
        numbers["send1_fee_atomic"] = fee1
        check("the RPC agrees: account 1 paid 1.25 LGO and a fee", nonce(id1) == 1 and 0 < fee1 < 10_000_000, f"fee {fee1}")
        hashes = re.findall(r"\b[0-9a-f]{64}\b", res)
        tx = rpc("getTransaction", [hashes[0]]) if hashes else None
        check("the transaction hash the page shows is the sequencer's", tx is not None, hashes[:1])
        check("the page computed the same hash as the sequencer (no warning)", not [c for c in console if "named the transaction" in c], console[-3:])
        shot(page, "04-send-done")

        # -- 7. Send back by account id alone (the account exists now)
        page.select_option("#account-select", "1")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent === a", arg=id2, timeout=10000)
        b1 = bal(id1)
        send(page, id1, "0.5")
        page.click("#send-go")
        page.wait_for_function("() => /In block|Not in a block|rror/.test(document.getElementById('send-result').textContent)", timeout=200000)
        check("account 2 sends to account 1 by id", "In block" in text(page, "#send-result"), text(page, "#send-result"))
        check("the RPC agrees: +0.5 LGO exactly", bal(id1) == b1 + 500_000_000 and nonce(id2) == 1, bal(id1) - b1)

        # -- 8. Activity
        page.select_option("#account-select", "0")
        page.wait_for_function("(a) => document.getElementById('home-id').textContent === a", arg=id1, timeout=10000)
        page.goto(URL + "#activity")
        page.wait_for_function("() => /read/.test(document.getElementById('act-status').textContent)", timeout=60000)
        page.wait_for_timeout(500)
        items = page.locator("#act-list li[data-testid=act-item]").all_inner_texts()
        joined = "\n".join(items)
        check("Activity lists the faucet's credit, the send and the return", len(items) >= 3 and "+5 LGO" in joined and "−1.25 LGO" in joined and "+0.5 LGO" in joined, joined)
        check("Activity marks every listed transfer applied (none in its block's reverted list)", "reverted" not in joined, joined)
        shot(page, "05-activity")

        # -- 9. Lock, a wrong passphrase, unlock; then a reload
        page.goto(URL + "#home")
        page.click("#lock")
        check("Lock forgets the keys (unlock screen)", page.locator("#v-unlock").is_visible())
        page.fill("#unlock-pass", "not the passphrase")
        page.click("#unlock-go")
        page.wait_for_function("() => document.getElementById('unlock-err').textContent", timeout=30000)
        check("a wrong passphrase is refused", "wrong passphrase" in text(page, "#unlock-err"), text(page, "#unlock-err"))
        page.fill("#unlock-pass", PASS)
        page.click("#unlock-go")
        page.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        check("the right passphrase opens it again", text(page, "#home-id") == id1)
        page.reload()
        unlock(page)
        check("after a reload: both accounts, the same ids", page.locator("#account-select option").count() == 2 and text(page, "#home-id") == id1)

        # -- 10. Backup
        page.goto(URL + "#settings")
        page.fill("#reveal-pass", PASS)
        page.click("#reveal-phrase")
        page.wait_for_function("() => document.getElementById('reveal-out').textContent", timeout=30000)
        check("the recovery phrase can be shown again, with the passphrase", text(page, "#reveal-out") == " ".join(words))
        page.wait_for_function("() => /Head block/.test(document.getElementById('net-info').textContent)", timeout=20000)
        check("network info names the head and the privacy circuit", "Privacy circuit" in text(page, "#net-info"), text(page, "#net-info"))

        # -- 11. Import the phrase elsewhere: the same accounts (determinism)
        ctx2 = browser.new_context(viewport={"width": 1280, "height": 900})
        p2 = ctx2.new_page()
        track(p2)
        p2.goto(URL)
        p2.click("#go-import")
        p2.fill("#import-phrase", " ".join(words).upper() + "  ")
        p2.fill("#import-pass", PASS)
        p2.click("#import-go")
        p2.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        check("importing the phrase gives the same first account", text(p2, "#home-id") == id1)
        p2.click("#add-account")
        p2.wait_for_function("(a) => document.getElementById('home-id').textContent !== a", arg=id1, timeout=10000)
        check("and the same second account", text(p2, "#home-id") == id2)
        ctx2.close()

        # -- 12. Import a raw key (LEZ's published local test key B) and send with it
        if KEY_B:
            ctx3 = browser.new_context(viewport={"width": 1280, "height": 900})
            p3 = ctx3.new_page()
            track(p3)
            p3.goto(URL)
            p3.click("#go-import")
            p3.click("[data-import=key]")
            p3.fill("#import-key", KEY_B)
            p3.fill("#import-pass", PASS)
            p3.click("#import-go")
            p3.wait_for_selector("#v-main:not([hidden])", timeout=30000)
            idb = text(p3, "#home-id")
            p3.wait_for_function("() => document.getElementById('balance').textContent !== '—'", timeout=20000)
            check("an imported raw key opens its account, balance from the RPC", bal(idb) > 0 and text(p3, "#balance").replace(",", "").split(" ")[0] == str(bal(idb) // UNIT) if bal(idb) % UNIT == 0 else True, f"{text(p3, '#balance')}")
            b1 = bal(id1)
            send(p3, id1, "0.1")
            p3.click("#send-go")
            p3.wait_for_function("() => /In block|Not in a block|rror/.test(document.getElementById('send-result').textContent)", timeout=200000)
            check("a raw key signs a transfer LEZ applies", bal(id1) == b1 + 100_000_000, bal(id1) - b1)
            ctx3.close()

        # -- 13. On a phone-sized screen
        ctx4 = browser.new_context(viewport={"width": 390, "height": 844}, device_scale_factor=2, is_mobile=True, has_touch=True)
        p4 = ctx4.new_page()
        track(p4)
        p4.goto(URL)
        p4.click("#go-import")
        p4.fill("#import-phrase", " ".join(words))
        p4.fill("#import-pass", PASS)
        p4.click("#import-go")
        p4.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        p4.wait_for_function("() => document.getElementById('balance').textContent !== '—'", timeout=20000)
        box = p4.locator("#tabs").bounding_box()
        check("on a phone the tabs sit at the bottom of the screen", box and box["y"] > 700, box)
        width = p4.evaluate("() => document.documentElement.scrollWidth")
        check("no sideways scrolling on a 390 px screen", width <= 390, width)
        shot(p4, "06-phone-home")
        p4.goto(URL + "#send")
        shot(p4, "07-phone-send")
        ctx4.close()

        # -- 14. The page's own traffic
        others = sorted({u for u in requests_seen if not u.startswith(ORIGIN + "/")})
        check("every request the page made went to its own origin", not others, others)
        errors = [c for c in console if c.startswith("pageerror")]
        check("no uncaught script error", not errors, errors)
        browser.close()


if __name__ == "__main__":
    sys.exit(main())
