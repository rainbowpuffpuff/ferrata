#!/usr/bin/env python3
"""One first visit of the wallet (a new profile) whose user agent carries a tag, so that the host's access log can sum
every byte it sent for that visit: the page, the service worker's offline copy, the RPC calls. Read-only.

    python3 -I test/pwa_firstvisit.py --base https://HOST --tag TAG [--seconds 20]
"""
import argparse
import os
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pwa_e2e import UA, wait_js  # noqa: E402
from playwright.sync_api import sync_playwright  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--tag", required=True)
    ap.add_argument("--seconds", type=int, default=20)
    a = ap.parse_args()
    with sync_playwright() as p:
        ctx = p.chromium.launch_persistent_context(
            tempfile.mkdtemp(prefix="first-", dir=os.environ.get("TMPDIR")), channel="chromium", base_url=a.base.rstrip("/"),
            viewport={"width": 412, "height": 915}, device_scale_factor=2.625, is_mobile=True, has_touch=True, user_agent=f"{UA} {a.tag}")
        page = ctx.pages[0] if ctx.pages else ctx.new_page()
        page.goto("/wallet/")
        wait_js(page, "navigator.serviceWorker.controller !== null", 60000)
        n = page.evaluate("caches.keys().then(async (ks) => { const k = ks.find((x) => x.startsWith('ferrata-wallet-') && x !== 'ferrata-wallet-config'); return k ? (await (await caches.open(k)).keys()).length : 0; })")
        page.wait_for_timeout(a.seconds * 1000)
        print(f"offline copy: {n} files; tag {a.tag}", flush=True)
        ctx.close()


if __name__ == "__main__":
    main()
