#!/usr/bin/env python3
"""Screenshots of the test network's own apps, as served, for the build page: the wallet, the explorer, the status page.

Run it once the test network is live, against its host; it writes site/shots/<name>.png and site/shots/CAPTURED.txt
(the URL, the time, the viewport and the browser of each picture) and prints the [[shot]] entries to add to
site/content.toml. Every picture is of the real app reading the real network at that time; the captions it suggests
say so. It requests nothing but the host given (other origins are blocked while it renders).

Usage: python3 site/capture.py https://HOST [--out site/shots]
Needs Playwright with Chromium (pip install playwright; playwright install chromium); not the build's dependency.
"""

import datetime
import pathlib
import sys
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parent.parent

PAGES = [
    ("wallet", "/wallet/", "The test network's web wallet, as served"),
    ("explorer", "/explorer/", "The test network's explorer: the newest blocks, as served"),
    ("explorer-programs", "/explorer/#/program", "The explorer's programs view: Ferrata's programs on the test network"),
    ("status", "/status", "The test network's status page"),
]


def main(argv):
    if not argv or argv[0].startswith("-"):
        print(__doc__, file=sys.stderr)
        return 2
    base = argv[0].rstrip("/")
    host = urllib.parse.urlparse(base).hostname
    out = pathlib.Path(argv[argv.index("--out") + 1]) if "--out" in argv else ROOT / "site" / "shots"
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        print("capture.py: needs Playwright (pip install playwright; playwright install chromium)", file=sys.stderr)
        return 2
    out.mkdir(parents=True, exist_ok=True)
    lines, entries = [], []
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 1280, "height": 860})

        def only_host(route):
            target = urllib.parse.urlparse(route.request.url).hostname or ""
            return route.continue_() if target == host or target.endswith("." + host) else route.abort()

        page.route("**/*", only_host)
        for name, path, caption in PAGES:
            url = base + path
            page.goto(url, wait_until="networkidle")
            page.wait_for_timeout(2500)
            dest = out / f"{name}.png"
            page.screenshot(path=str(dest))
            when = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
            lines.append(f"{name}.png\t{path}\t{when}\t1280x860\tChromium {browser.version}")
            rel = dest.relative_to(ROOT).as_posix() if dest.is_relative_to(ROOT) else dest.as_posix()
            entries.append(f'[[shot]]\ngallery = "testnet"\npath = "{rel}"\ncaption = "{caption}, {when[:16].replace("T", " ")} UTC"\n'
                           f'record = "site/shots/CAPTURED.txt"\n')
        browser.close()
    # The host is not recorded: a committed file must not name the server it was taken from.
    (out / "CAPTURED.txt").write_text("file\tpath on the test network's host\tutc\tviewport\tbrowser\n" + "\n".join(lines) + "\n",
                                      encoding="utf-8")
    print("\n".join(entries))
    print(f"capture.py: {len(lines)} pictures in {out}; commit them and add the entries above to site/content.toml")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
