#!/usr/bin/env python3
"""Renders the app's PNG icons from the SVG sources in icons/ with Chromium (Playwright), transparent where the SVG is.

    python3 -I test/make_icons.py      (on the build box; writes icons/*.png and prints their SHA-256)
"""
import hashlib
import os

from playwright.sync_api import sync_playwright

ICONS = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "icons")
# (source SVG, output PNG, size): "any" icons keep their rounded corners; maskable and Apple's are full-bleed squares
# (the system cuts its own shape); mono is the white mark alone (Android's themed icons, the notification badge).
OUT = [("icon.svg", "icon-192.png", 192), ("icon.svg", "icon-512.png", 512), ("maskable.svg", "maskable-512.png", 512),
       ("maskable.svg", "apple-touch-icon.png", 180), ("mono.svg", "mono-512.png", 512), ("mono.svg", "mono-96.png", 96)]


def main():
    with sync_playwright() as p:
        b = p.chromium.launch()
        for src, out, n in OUT:
            with open(os.path.join(ICONS, src)) as f:
                svg = f.read().replace("<svg ", f'<svg width="{n}" height="{n}" ', 1)
            page = b.new_page(viewport={"width": n, "height": n}, device_scale_factor=1)
            page.set_content(f'<html><body style="margin:0;background:transparent">{svg}</body></html>')
            page.screenshot(path=os.path.join(ICONS, out), omit_background=True, clip={"x": 0, "y": 0, "width": n, "height": n})
            page.close()
        b.close()
    for _, out, n in OUT:
        with open(os.path.join(ICONS, out), "rb") as f:
            print(hashlib.sha256(f.read()).hexdigest(), out, n)


if __name__ == "__main__":
    main()
