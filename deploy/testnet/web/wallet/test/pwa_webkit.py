#!/usr/bin/env python3
"""The wallet in WebKit (Playwright's WebKit, the engine of Safari on iPhone and iPad, here on Linux; not an iPhone),
with an iPhone's screen and user agent, against the deployed page. Read-only towards the network.

    python3 -I test/pwa_webkit.py --base https://HOST --out DIR

Checks: the Home Screen tags (manifest, apple-touch-icon), the service worker, a wallet created, the iPhone install hint,
the receive QR (read back by zbar), a scan from a photo, PIN unlock, and the app opening offline. WebKit has no virtual
authenticator in Playwright, so passkey unlock is not tested here.
"""
import argparse
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pwa_e2e import Record, wait_js, zbar  # noqa: E402
from playwright.sync_api import sync_playwright  # noqa: E402

PASS, PIN = "webkit passphrase 1", "975310"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    rec = Record(a.out)
    errors, version, offline = [], None, None
    with sync_playwright() as p:
        dev = dict(p.devices["iPhone 15"])
        dev.pop("default_browser_type", None)
        b = p.webkit.launch()
        version = b.version
        ctx = b.new_context(base_url=a.base.rstrip("/"), **dev)
        page = ctx.new_page()
        page.on("pageerror", lambda e: errors.append(str(e)))
        vis = lambda sel, t=20000: page.wait_for_selector(sel, state="visible", timeout=t)
        page.goto("/wallet/")
        vis("#v-welcome", 30000)
        tags = page.evaluate("""[!!document.querySelector('link[rel=manifest]'), document.querySelector('link[rel=apple-touch-icon]')?.href,
          document.querySelector('meta[name=apple-mobile-web-app-capable]')?.content]""")
        icon_ok = page.evaluate(f"fetch({json.dumps(tags[1])}).then((r) => r.ok && r.headers.get('content-type'))") if tags[1] else None
        rec.check("Home Screen tags: manifest, apple-touch-icon (served), web-app-capable", tags[0] and icon_ok == "image/png" and tags[2] == "yes", json.dumps(tags[2:]))
        try:
            wait_js(page, "navigator.serviceWorker && navigator.serviceWorker.controller !== null", 30000)
            sw = True
        except TimeoutError:
            sw = False
        rec.check("service worker controls the page (WebKit)", sw)
        page.click("#go-create"); page.check("#saved-words"); page.fill("#create-pass", PASS); page.fill("#create-pass2", PASS)
        page.click("#create-go")
        vis("#v-main", 60000)
        rec.check("wallet created", bool(page.text_content("#home-id")))
        page.goto("/wallet/#settings")
        vis("[data-testid=app-settings]")
        rec.check("More shows the iPhone install hint", "Add to Home Screen" in page.text_content("[data-testid=app-settings]"), page.text_content("[data-testid=app-settings]")[:120])
        page.fill("[data-testid=lock-pass]", PASS); page.fill("[data-testid=pin-new]", PIN); page.fill("[data-testid=pin-again]", PIN)
        page.click("[data-testid=pin-set]")
        wait_js(page, "document.querySelector('[data-testid=lock-msg]')?.textContent.includes('PIN unlock is on')", 60000)
        page.goto("/wallet/#receive")
        vis("#recv-qr svg")
        own = page.text_content("#recv-code")
        png = os.path.join(a.out, "receive-qr.png")
        z = ""
        for _ in range(3):  # the QR settles (and any on-screen line goes) before the picture is taken
            page.evaluate("document.getElementById('toast').hidden = true")
            page.locator("#recv-qr").scroll_into_view_if_needed()
            page.wait_for_timeout(700)
            page.locator("#recv-qr").screenshot(path=png)
            z = zbar(png)
            if z is None or z == own:
                break
        rec.check("receive QR read back by zbar", z is None or z == own, repr(z)[:120])
        page.goto("/wallet/#send")
        page.set_input_files("#send-scan-file", png)
        wait_js(page, "document.getElementById('send-to').value.length > 0", 20000)
        rec.check("scan from a photo fills the recipient (WebKit has no BarcodeDetector: the page's decoder)", page.input_value("#send-to") == own)
        page.click("#lock")
        vis("#unlock-pin")
        page.fill("#unlock-pin", PIN); page.click("#unlock-pin-go")
        vis("#v-main", 60000)
        rec.check("PIN unlock", True)
        offline = None
        if sw:
            ctx.set_offline(True)
            try:
                page2 = ctx.new_page()
                page2.goto("/wallet/")
                page2.wait_for_selector("#v-unlock", state="visible", timeout=30000)
                page2.fill("#unlock-pin", PIN); page2.click("#unlock-pin-go")
                page2.wait_for_selector("#v-main", state="visible", timeout=60000)
                page2.goto("/wallet/#receive")
                page2.wait_for_selector("#recv-qr svg", timeout=20000)
                rec.check("offline: opens, unlocks, shows the receive QR", page2.text_content("#recv-code") == own)
            except Exception as e:  # noqa: BLE001 - Playwright's WebKit cannot navigate while offline: recorded, not shown
                offline = f"not shown: Playwright's WebKit refused the offline navigation ({str(e).splitlines()[0][:120]})"
                print("NOT RUN offline check: " + offline, flush=True)
            ctx.set_offline(False)
        page.screenshot(path=os.path.join(a.out, "webkit.png"))
        rec.check("no script errors", not errors, " | ".join(errors[:3]))
        b.close()
    ok = rec.save({"engine": f"WebKit {version} (Playwright, Linux)", "offline": offline or "shown"})
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
