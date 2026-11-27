"""The Bridge tab's deposit with the real MetaMask extension (run by metamask.sh). MetaMask is loaded unpacked into
Chromium (Playwright persistent context, new headless mode), imports anvil's published test mnemonic, and every
MetaMask screen is handled through MetaMask's own UI: the connection request, the network the page asks it to add and
switch to, and the deposit's confirmation. Each screen is logged (its data-testids and buttons) and photographed.

Environment: MM_EXT (the unpacked release), MM_VERSION, WALLET_URL, WALLET_ANVIL, WALLET_VAULT, WALLET_OUT,
WALLET_SHOTS, WALLET_PROFILE (a fresh browser profile directory)."""
import hashlib
import hmac
import json
import os
import re
import sys
import time
import urllib.request

from playwright.sync_api import sync_playwright

EXT, URL, ANVIL, VAULT = os.environ["MM_EXT"], os.environ["WALLET_URL"], os.environ["WALLET_ANVIL"], os.environ["WALLET_VAULT"]
OUT, SHOTS, PROFILE = os.environ["WALLET_OUT"], os.environ["WALLET_SHOTS"], os.environ["WALLET_PROFILE"]
MNEMONIC = "test test test test test test test test test test test junk"  # anvil's published test mnemonic
MM_PASS = "local-test-only-1"
LEZ_PASS = "metamask test passphrase"
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
checks, numbers = [], {}
LOG = open(os.path.join(SHOTS, "driver.log"), "w")
shot_n = [0]


def log(*a):
    print(*a, file=LOG, flush=True)


def check(name, ok, detail=""):
    checks.append({"check": name, "ok": bool(ok), "detail": str(detail)[:300]})
    print(("PASS " if ok else "FAIL ") + name + (f" -- {str(detail)[:200]}" if detail else ""), flush=True)
    return ok


def eth(method, params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    with urllib.request.urlopen(urllib.request.Request(ANVIL, data=body, headers={"Content-Type": "application/json"}), timeout=20) as r:
        return json.loads(r.read()).get("result")


def b58decode(s):
    n = 0
    for ch in s:
        n = n * 58 + B58.index(ch)
    return n.to_bytes(32, "big")


def expected_c(words, chain_id, vault, depositor, j, kind, dest32):
    seed64 = hashlib.pbkdf2_hmac("sha512", " ".join(words).encode(), b"mnemonic", 2048)
    hs = lambda k, m: hmac.new(k, m, hashlib.sha256).digest()  # noqa: E731
    k_auth = hs(hs(seed64, b"ferrata/bridge-seed/v1"), b"ferrata/auth/v1")
    s = hs(k_auth, b"ferrata/s/v1" + chain_id.to_bytes(32, "big") + bytes.fromhex(vault[2:]) + bytes.fromhex(depositor[2:]) + j.to_bytes(8, "big"))
    return "0x" + hashlib.sha256(b"ferrata/C/v1" + s + bytes([kind]) + dest32).hexdigest()


def dump(page, tag):
    shot_n[0] += 1
    name = f"{shot_n[0]:02d}-{tag}"
    try:
        page.screenshot(path=os.path.join(SHOTS, name + ".png"))
        # Playwright's locators run outside the page's world: MetaMask's LavaMoat blocks page-world evaluation.
        ids = [e.get_attribute("data-testid") for e in page.locator("[data-testid]").all()[:30] if e.is_visible()]
        btns = [(e.inner_text() or "").strip()[:40] for e in page.locator("button, a, [role=button]").all()[:40] if e.is_visible()]
        log(f"== {name} {page.url}\n   testids: {ids}\n   buttons: {btns}")
    except Exception as e:  # noqa: BLE001
        log(f"== {name} (dump failed: {e})")
    return name


def visible(page, sel):
    try:
        loc = page.locator(sel).first
        return loc.count() > 0 and loc.is_visible()
    except Exception:  # noqa: BLE001
        return False


def click_first(page, selectors):
    for sel in selectors:
        try:
            loc = page.locator(sel).first
            if loc.count() and loc.is_visible() and loc.is_enabled():
                loc.click(timeout=5000)
                log(f"   clicked {sel}")
                return sel
        except Exception as e:  # noqa: BLE001
            log(f"   click {sel} failed: {e}")
    return None


TEXT_ONBOARD = ["I have an existing wallet", "Import an existing wallet", "Import using Secret Recovery Phrase",
                "Use Secret Recovery Phrase", "Import wallet", "I agree", "Agree", "Accept", "No thanks", "Maybe later",
                "Remind me later", "Continue", "Confirm Secret Recovery Phrase", "Confirm", "Next", "Done", "Got it",
                "Open wallet", "Skip", "Not now", "Close", "Let's go", "Get started", "Dismiss", "Ok", "OK"]


def click_text(page, texts):
    for t in texts:
        try:
            for loc in page.get_by_text(t, exact=True).all():
                if loc.is_visible() and loc.is_enabled():
                    loc.click(timeout=5000)
                    log(f"   clicked text {t!r}")
                    return t
        except Exception as e:  # noqa: BLE001
            log(f"   click text {t!r} failed: {e}")
    return None
IDS_ONBOARD = ["onboarding-import-wallet", "biometrics-maybe-later", "passkey-maybe-later", "metametrics-no-thanks", "onboarding-import-with-srp-button", "metametrics-no-thanks", "metametrics-i-agree",
               "import-srp-confirm", "create-password-import", "create-password-submit", "onboarding-complete-done",
               "pin-extension-next", "pin-extension-done", "download-app-continue", "onboarding-get-started-button",
               "terms-of-use-agree-button", "terms-of-use-accept-button", "popover-close", "not-now-button"]


def onboard(ctx, ext_id):
    home = f"chrome-extension://{ext_id}/home.html"
    page = next((p for p in ctx.pages if p.url.startswith(home)), None) or ctx.new_page()
    if not page.url.startswith(home):
        page.goto(home)
    for step in range(70):
        page.wait_for_timeout(1200)
        dump(page, f"onboard{step}")
        if any(visible(page, f'[data-testid="{t}"]') for t in ("account-menu-icon", "eth-overview-send", "account-options-menu-button", "app-header-logo")) \
                and "onboarding" not in page.url:
            return page
        # Past "Open wallet" the keyring exists and onboarding is recorded; headless, the page may stay on this route.
        if "/onboarding/completion" in page.url and getattr(onboard, "done_clicked", False):
            page.wait_for_timeout(3000)
            page.goto(home + "#/")
            page.wait_for_timeout(3000)
            dump(page, "home")
            if "onboarding" not in page.url:
                return page
        # Terms checkboxes on the welcome and password screens (never the MetaMetrics opt-in).
        for cb in ([] if "metametrics" in page.url else page.locator("input[type=checkbox]").all()):
            try:
                if cb.is_visible() and not cb.is_checked():
                    cb.check(timeout=3000, force=True)
                    log("   checked a checkbox")
            except Exception:  # noqa: BLE001
                pass
        # The recovery phrase: one box per word, or one text area.
        if visible(page, '[data-testid="import-srp__srp-word-0"]'):
            for i, w in enumerate(MNEMONIC.split()):
                page.fill(f'[data-testid="import-srp__srp-word-{i}"]', w)
            log("   filled the phrase word by word")
        elif ("srp" in page.url or "recovery" in page.url.lower() or "import" in page.url) and visible(page, "textarea"):
            ta = page.locator("textarea").first
            if not ta.input_value():
                ta.click()
                page.keyboard.type(MNEMONIC)
                log("   typed the phrase into the text area")
        # Passwords.
        pw = [x for x in page.locator("input[type=password]").all() if x.is_visible()]
        if pw and not pw[0].input_value():
            for x in pw:
                x.fill(MM_PASS)
            log(f"   filled {len(pw)} password fields")
        sel = click_first(page, [f'[data-testid="{t}"]' for t in IDS_ONBOARD]) or click_text(page, TEXT_ONBOARD)
        if sel and "onboarding-complete-done" in sel or sel == "Open wallet":
            onboard.done_clicked = True
        if not sel and step > 5:
            page.keyboard.press("Escape")
    raise RuntimeError("MetaMask's onboarding did not reach the wallet home")


APPROVE = ['[data-testid="confirm-btn"]', '[data-testid="confirmation-submit-button"]', '[data-testid="confirm-footer-button"]',
           '[data-testid="page-container-footer-next"]', 'button:text-is("Connect")', 'button:text-is("Confirm")',
           'button:text-is("Approve")', 'button:text-is("Switch network")', 'button:text-is("Got it")']


def approve_until(ctx, ext_id, done, timeout=120, tag="popup"):
    end = time.time() + timeout
    seen = set()
    notif = None
    while time.time() < end:
        if done():
            if notif:
                notif.close()
            return True
        # Headless, MetaMask's approval window does not open by itself: its pending requests are served at
        # notification.html, which the driver opens (and reloads when it shows nothing to approve).
        try:
            mode = os.environ.get("MM_MODE", "home")
            if mode == "home":
                # MetaMask's full-page UI routes to a pending request when it loads: reload a home tab.
                notif = next((pg for pg in ctx.pages if pg.url.startswith(f"chrome-extension://{ext_id}/home.html")), None) or ctx.new_page()
                if not any(visible(notif, sel) for sel in APPROVE):
                    notif.goto(f"chrome-extension://{ext_id}/home.html")
                    notif.wait_for_timeout(3000)
            else:
                if notif is None or notif.is_closed():
                    notif = ctx.new_page()
                if not any(visible(notif, sel) for sel in APPROVE):
                    notif.goto(f"chrome-extension://{ext_id}/{'popup' if mode == 'popup' else 'notification'}.html")
                    notif.wait_for_timeout(2500)
        except Exception as e:  # noqa: BLE001
            log(f"   notification page: {e}")
        urls = [pg.url for pg in ctx.pages]
        if tuple(urls) not in seen:
            seen.add(tuple(urls)); log(f"   pages: {urls}")
        for pg in list(ctx.pages):
            u = pg.url
            if u.startswith(f"chrome-extension://{ext_id}/") and not u.endswith("home.html#/") and "onboarding" not in u or (pg is notif and any(visible(pg, sel) for sel in APPROVE)):
                try:
                    pg.wait_for_timeout(800)
                    for cb in pg.locator("input[type=checkbox]").all():
                        if cb.is_visible() and not cb.is_checked():
                            cb.check(force=True)
                    dump(pg, tag)
                    click_first(pg, APPROVE) or click_text(pg, ["Connect", "Confirm", "Approve", "Switch network", "Got it", "Continue"])
                except Exception as e:  # noqa: BLE001
                    log(f"   popup handling: {e}")
        time.sleep(1)
    return done()


def steps():
    with sync_playwright() as p:
        # Headed under Xvfb when MM_HEADED=1 (MetaMask's approval windows do not render in headless mode).
        headed = os.environ.get("MM_HEADED") == "1" and bool(os.environ.get("DISPLAY"))
        log(f"   headed: {headed}")
        ctx = p.chromium.launch_persistent_context(PROFILE, channel="chromium", headless=not headed, viewport=None if headed else {"width": 1280, "height": 900},
                                                   args=[f"--disable-extensions-except={EXT}", f"--load-extension={EXT}", "--window-size=1280,1000"])
        ctx.set_default_timeout(20000)
        ctx.set_default_navigation_timeout(20000)
        sw = ctx.service_workers[0] if ctx.service_workers else ctx.wait_for_event("serviceworker", timeout=60000)
        ext_id = sw.url.split("/")[2]
        check("the real MetaMask extension is loaded (its service worker runs)", bool(ext_id), f"MetaMask {os.environ.get('MM_VERSION')}")
        time.sleep(3)
        mm = onboard(ctx, ext_id)
        check("MetaMask's own onboarding imported anvil's test phrase and reached the wallet", True, mm.url.split("#")[-1])

        page = ctx.new_page()
        page.goto(URL)
        page.click("#go-create")
        words = [w.strip() for w in page.locator("#mnemonic-words li").all_text_contents()]
        page.check("#saved-words")
        page.fill("#create-pass", LEZ_PASS)
        page.fill("#create-pass2", LEZ_PASS)
        page.click("#create-go")
        page.wait_for_selector("#v-main:not([hidden])", timeout=30000)
        lez_id = page.locator("#home-id").inner_text().strip()
        page.goto(URL + "#bridge")
        log(f"   window.ethereum in the page: {page.evaluate('() => !!window.ethereum')}, isMetaMask: {page.evaluate('() => !!(window.ethereum && window.ethereum.isMetaMask)')}")
        page.click("[data-testid=bridge-connect]")
        ok = approve_until(ctx, ext_id, lambda: "Connected" in page.locator("[data-testid=bridge-status]").inner_text()
                           or "chain" in page.locator("[data-testid=bridge-out]").inner_text(), tag="connect")
        dump(page, "page-connected")
        status = page.locator("[data-testid=bridge-status]").inner_text()
        check("MetaMask connected the page and switched to the vault's chain (added by the page's request)",
              ok and "Connected" in status and "chain 31337" in status, status + " " + page.locator("[data-testid=bridge-out]").inner_text())
        depositor = re.search(r"Connected (0x[0-9a-fA-F]{40})", status).group(1).lower()   # shown EIP-55 checksummed
        page.select_option("[data-testid=bridge-kind]", "public")
        page.click("[data-testid=bridge-review]")
        page.wait_for_function("() => /Commitment C/.test(document.querySelector('[data-testid=bridge-summary]').textContent) || /rror|No |chain/.test(document.querySelector('[data-testid=bridge-out]').textContent)", timeout=60000)
        m = re.search(r"Commitment C\s*(0x[0-9a-f]{64})", page.locator("[data-testid=bridge-summary]").inner_text())
        want = expected_c(words, 31337, VAULT.lower(), depositor, 0, 2, b58decode(lez_id))
        check("the review's C equals C recomputed from the LEZ phrase (the ferrata tool's derivation)", m and m.group(1) == want, (m.group(1) if m else None, want))
        dump(page, "page-review")
        page.click("[data-testid=bridge-go]")
        t0 = time.time()
        ok = approve_until(ctx, ext_id, lambda: re.search(r"Deposited|failed|rror|changed", page.locator("[data-testid=bridge-out]").inner_text()) is not None, timeout=180, tag="confirm")
        numbers["deposit_seconds"] = round(time.time() - t0, 1)
        out = page.locator("[data-testid=bridge-out]").inner_text()
        dump(page, "page-deposited")
        check("MetaMask signed the deposit and the page checked the mined Deposit event", ok and "Deposited: leaf 0" in out, out)
        txh = re.search(r"\((0x[0-9a-f]{64})\)", out)
        tx = eth("eth_getTransactionByHash", [txh.group(1)]) if txh else None
        check("the transaction is from MetaMask's account, to the vault, with depositETH's calldata and the total as value",
              tx and tx["from"].lower() == depositor and tx["to"].lower() == VAULT.lower() and tx["input"].endswith("0" * 64) and want[2:] in tx["input"], {k: tx.get(k) for k in ("from", "to", "value")} if tx else None)
        logs = eth("eth_getLogs", [{"address": VAULT, "fromBlock": "0x0", "toBlock": "latest"}]) or []
        check("the vault logged exactly that C for MetaMask's account", sum(1 for l in logs if l["topics"][1][-40:] == depositor[2:] and l["data"][-64:] == want[2:]) == 1, len(logs))
        ctx.close()


def main():
    import signal
    def watchdog(*_):
        raise TimeoutError("the run's 12-minute watchdog fired (MetaMask stopped answering)")
    signal.signal(signal.SIGALRM, watchdog)
    signal.alarm(720)
    try:
        steps()
    except Exception as e:  # noqa: BLE001
        check("the run completed without an unexpected error", False, repr(e)[:300])
    os.makedirs(OUT, exist_ok=True)
    rec = {"metamask": os.environ.get("MM_VERSION"), "passed": sum(c["ok"] for c in checks), "failed": sum(not c["ok"] for c in checks),
           "numbers": numbers, "checks": checks}
    json.dump(rec, open(os.path.join(OUT, "record.json"), "w"), indent=1)
    print(f"{rec['passed']} passed, {rec['failed']} failed")
    return 0 if rec["failed"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
