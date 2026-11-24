#!/usr/bin/env python3
"""The /android/ page in Chromium (Playwright), read-only: its facts match apk.json, its live checks pass (Digital Asset
Links, the newest blocks' hashes and links, the wallet's security headers), the in-browser APK check and the check of
every wallet file pass, and the APK link downloads the published file.

    python3 -I test_page.py --base https://HOST --out DIR
"""
import argparse
import hashlib
import json
import os
import sys
import time

from playwright.sync_api import sync_playwright


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    checks, errors = [], []

    def check(name, ok, detail=""):
        checks.append({"name": name, "ok": bool(ok), "detail": detail})
        print(("PASS " if ok else "FAIL ") + name + (f": {detail}" if detail else ""), flush=True)

    with sync_playwright() as p:
        b = p.chromium.launch()
        ctx = b.new_context(base_url=a.base, viewport={"width": 412, "height": 915}, device_scale_factor=2.625, is_mobile=True, accept_downloads=True)
        page = ctx.new_page()
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.goto("/android/")
        apk = json.loads(page.evaluate("fetch('apk.json').then((r) => r.text())"))
        page.wait_for_selector("#facts dd", timeout=20000)
        facts = page.text_content("#facts")
        check("facts shown from apk.json", apk["sha256"] in facts and apk["certSha256"] in facts, f"{apk['file']} {apk['size']} bytes")
        end = time.time() + 60
        while time.time() < end and page.locator("#checks li.pending").count():
            time.sleep(0.5)
        items = page.locator("#checks li").all()
        for li in items:
            check("live check: " + li.locator("b").text_content(), "ok" in (li.get_attribute("class") or ""), li.locator(".detail").text_content().strip()[:200])
        page.click("#check-apk")
        end = time.time() + 120
        while time.time() < end and not page.text_content("#check-apk-out").startswith(("✓", "✗", "Could not")):
            time.sleep(0.5)
        check("the APK checked in the browser", page.text_content("#check-apk-out").startswith("✓"), page.text_content("#check-apk-out")[:160])
        page.click("#check-files")
        end = time.time() + 120
        last = None
        while time.time() < end:
            last = page.locator("#checks li").last
            if (last.get_attribute("class") or "") in ("ok", "bad"):
                break
            time.sleep(0.5)
        check("every wallet file against the service worker's list", last.get_attribute("class") == "ok", last.locator(".detail").text_content().strip()[:160])
        with page.expect_download(timeout=120000) as dl:
            page.click("#dl")
        path = os.path.join(a.out, dl.value.suggested_filename)
        dl.value.save_as(path)
        data = open(path, "rb").read()
        check("the download link saves the published APK under its name", hashlib.sha256(data).hexdigest() == apk["sha256"] and dl.value.suggested_filename == apk["file"],
              f"{dl.value.suggested_filename}, {len(data)} bytes")
        os.remove(path)
        page.screenshot(path=os.path.join(a.out, "android-page.png"), full_page=True)
        check("no script errors", not errors, " | ".join(errors[:3]))
        b.close()
    n = sum(c["ok"] for c in checks)
    json.dump({"passed": n, "total": len(checks), "checks": checks}, open(os.path.join(a.out, "record.json"), "w"), indent=1)
    print(f"{n} of {len(checks)}")
    sys.exit(0 if n == len(checks) else 1)


if __name__ == "__main__":
    main()
