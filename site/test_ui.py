#!/usr/bin/env python3
"""The build page in a real browser: the requirements overview, a requirement's sheet, deep links, the full table.

Serves deploy/testnet/web/build/ on 127.0.0.1 (a random port) and drives Chromium with Playwright; every request to
another origin is aborted, so it reads nothing from the network. Checks:
  - the overview fits one desktop screen (1366 x 768) and its 137 tiles are keyboard-reachable links;
  - a click, Enter or Space on a tile opens that requirement's sheet (the RFP's words, status and reason, tests,
    milestone), sets #req-RFP-…, and Escape closes it, returning focus to the tile;
  - a deep link #req-RFP-021-F.12 opens its sheet on load; the sheet's previous/next buttons and arrow keys step;
  - "Show the full table" shows the table with its filters (?view=table) and "Back to the overview" returns;
    ?status=waits opens the table filtered; #rfp-021-s-8 opens the table at that row;
  - the legend highlights one status; without JavaScript the full table shows (assets/nojs.css);
  - no script error on any of these pages.

Usage: python3 site/test_ui.py  (exit 0 pass, 1 fail, 77 when Playwright with Chromium is not installed)
"""

import functools
import http.server
import pathlib
import sys
import threading

ROOT = pathlib.Path(__file__).resolve().parent.parent
BUNDLE = ROOT / "deploy" / "testnet" / "web" / "build"


def serve():
    class Quiet(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *args):
            pass

    handler = functools.partial(Quiet, directory=str(BUNDLE))
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd


def main():
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        print("test_ui.py: skipped (needs Playwright: pip install playwright; playwright install chromium)")
        return 77
    httpd = serve()
    base = f"http://127.0.0.1:{httpd.server_address[1]}/"
    problems, errors = [], []

    def check(cond, what):
        if not cond:
            problems.append(what)

    with sync_playwright() as p:
        try:
            browser = p.chromium.launch()
        except Exception as e:
            print(f"test_ui.py: skipped (Chromium for Playwright is not installed: {str(e).splitlines()[0][:120]})")
            httpd.shutdown()
            return 77

        def page(js=True, width=1366, height=768):
            ctx = browser.new_context(viewport={"width": width, "height": height}, java_script_enabled=js)
            pg = ctx.new_page()
            pg.route("**/*", lambda r: r.continue_() if r.request.url.startswith(base) else r.abort())
            pg.on("pageerror", lambda e: errors.append(str(e)))
            return pg

        pg = page()
        pg.goto(base + "#requirements", wait_until="load")
        pg.wait_for_timeout(300)
        box = pg.locator("#overview").bounding_box()
        check(box and box["height"] <= 768 - 56, f"the overview is {box and round(box['height'])} px high, more than one screen")
        tiles = pg.locator("a.tile")
        check(tiles.count() == 137, f"{tiles.count()} tiles")
        check(pg.locator("#req-full").is_hidden(), "the full table shows before it is asked for")
        first = tiles.first
        first.click()
        sheet = pg.locator("#req-dialog")
        check(sheet.is_visible(), "a click on a tile opens no sheet")
        check(pg.evaluate("location.hash") == "#req-RFP-021-F.1", f"hash after a click: {pg.evaluate('location.hash')}")
        check(pg.locator("#rq-title").inner_text() == "RFP-021 F.1", "the sheet's title")
        body = pg.locator("#rq-body").text_content()
        for part in ("In the RFP’s words", "Status and why", "Shown by", "Milestone", "Run the Ethereum light client"):
            check(part in body, f"the sheet lacks {part!r}")
        pg.keyboard.press("ArrowRight")
        check(pg.locator("#rq-title").inner_text() == "RFP-021 F.2", "ArrowRight does not step to F.2")
        pg.locator('#req-dialog [data-step="-1"]').click()
        check(pg.locator("#rq-title").inner_text() == "RFP-021 F.1", "the previous button does not step back")
        pg.keyboard.press("Escape")
        check(sheet.is_hidden(), "Escape does not close the sheet")
        check(pg.evaluate("document.activeElement.getAttribute('data-req')") == "RFP-021-F.1", "focus does not return to the tile")
        check(pg.evaluate("location.hash") == "#requirements", "the hash is not restored on close")
        pg.keyboard.press("Space")
        check(sheet.is_visible(), "Space on a tile opens no sheet")
        pg.keyboard.press("Escape")
        pg.keyboard.press("Enter")
        check(sheet.is_visible(), "Enter on a tile opens no sheet")
        pg.keyboard.press("Escape")

        pg.locator(".ov-legend button.lg[data-hl=notyet]").click()
        check(pg.locator("#overview").get_attribute("data-hl") == "notyet", "the legend does not highlight")
        pg.locator(".ov-legend button.lg[data-hl=notyet]").click()

        pg.locator("#view-toggle").click()
        check(pg.locator("#req-full").is_visible() and pg.locator("#filters").is_visible(), "the full table does not show")
        check("view=table" in pg.url, "?view=table is not set")
        check(pg.locator("#ov-grid").is_hidden(), "the overview stays beside the table")
        check(pg.locator("#req-count").inner_text() == "137 of 137 requirements", "the table's count")
        check(pg.locator("#view-toggle").inner_text() == "Back to the overview", "the toggle's label")
        pg.locator("#view-toggle").click()
        check(pg.locator("#ov-grid").is_visible() and pg.locator("#req-full").is_hidden(), "back to the overview fails")

        pg2 = page()
        pg2.goto(base + "#req-RFP-021-F.12", wait_until="load")
        pg2.wait_for_timeout(300)
        check(pg2.locator("#req-dialog").is_visible() and pg2.locator("#rq-title").inner_text() == "RFP-021 F.12",
              "the deep link #req-RFP-021-F.12 opens no sheet")
        pg2.goto(base + "?status=waits", wait_until="load")
        pg2.wait_for_timeout(200)
        check(pg2.locator("#req-count").inner_text() == "5 of 137 requirements", "?status=waits does not filter the table")
        pg2.goto(base + "#rfp-021-s-8", wait_until="load")
        pg2.wait_for_timeout(300)
        check(pg2.locator("#rfp-021-s-8").is_visible(), "#rfp-021-s-8 does not open the table at its row")

        pg4 = page()
        pg4.goto(base + "#%E0%A4%A", wait_until="load")
        pg4.wait_for_timeout(300)
        check(pg4.locator("button.copy:visible").count() > 0, "a malformed hash stops the script (no copy button shown)")

        pg3 = page(js=False)
        pg3.goto(base, wait_until="load")
        check(pg3.locator("#req-table").is_visible(), "without JavaScript the full table is hidden")
        check(pg3.locator("a.tile").count() == 137, "without JavaScript the tiles are missing")

        mob = page(width=390, height=844)
        mob.goto(base + "#req-RFP-022-Sec.1", wait_until="load")
        mob.wait_for_timeout(300)
        sb = mob.locator("#req-dialog").bounding_box()
        check(sb and sb["width"] >= 380, "on a phone the sheet is not full width")
        browser.close()
    httpd.shutdown()
    problems += [f"script error: {e}" for e in errors]
    for prob in problems:
        print(f"FAIL {prob}", file=sys.stderr)
    print(f"test_ui.py: {'ok' if not problems else str(len(problems)) + ' problem(s)'}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
