#!/usr/bin/env python3
"""Serve the web depositor to this machine only, so that wallet extensions inject into it.

    python3 -I web/depositor/serve.py [--port 8645]

then open http://127.0.0.1:8645/ in the browser that has your wallet extension. Browser wallets inject
`window.ethereum` into http and https pages; into a page opened from file:// only when the user has turned on the
extension's "Allow access to file URLs" (off by default; turning it on also lets the extension read local files).

What it does, and nothing else:
* reads web/depositor/index.html once, at start, prints its SHA-256 (compare it with the one your release states), and
  serves exactly those bytes at / and /index.html; anything else is 404;
* listens on 127.0.0.1 only and answers only requests whose Host is 127.0.0.1 or localhost on its port (a page of
  another site that resolves its own name to 127.0.0.1 gets 421);
* sends the page's Content-Security-Policy as a header too, with what a <meta> tag cannot carry: frame-ancestors
  'none' (no site may frame the page) and the inline script's own hash as the only script allowed; also
  X-Frame-Options DENY, X-Content-Type-Options nosniff, Referrer-Policy no-referrer and Cache-Control no-store.
Python 3 standard library only; no file is written.
"""
import argparse
import base64
import hashlib
import http.server
import re
import sys
from pathlib import Path

PAGE = Path(__file__).resolve().with_name("index.html")
HOST = "127.0.0.1"


def script_source(html: bytes) -> bytes:
    """The page's one inline script, exactly as the browser hashes it."""
    m = re.search(rb'<script id="ferrata-depositor">(.*?)</script>', html, re.S)
    if not m:
        raise SystemExit("index.html has no <script id=\"ferrata-depositor\">")
    return m.group(1)


def script_hash(html: bytes) -> str:
    return "'sha256-" + base64.b64encode(hashlib.sha256(script_source(html)).digest()).decode() + "'"


def headers_for(html: bytes) -> dict:
    csp = (
        "default-src 'none'; script-src " + script_hash(html) + "; style-src 'unsafe-inline'; base-uri 'none'; "
        "form-action 'none'; frame-ancestors 'none'"
    )
    return {
        "Content-Type": "text/html; charset=utf-8",
        "Content-Security-Policy": csp,
        "X-Frame-Options": "DENY",
        "X-Content-Type-Options": "nosniff",
        "Referrer-Policy": "no-referrer",
        "Cache-Control": "no-store",
    }


def handler(body: bytes, headers: dict, port: int):
    allowed_hosts = {f"{HOST}:{port}", f"localhost:{port}"}

    class Handler(http.server.BaseHTTPRequestHandler):
        server_version = "ferrata-depositor"
        sys_version = ""

        def _answer(self, with_body: bool):
            if self.headers.get("Host", "") not in allowed_hosts:
                self.send_error(421, "this server answers for 127.0.0.1 only")
                return
            if self.path not in ("/", "/index.html"):
                self.send_error(404)
                return
            self.send_response(200)
            for k, v in headers.items():
                self.send_header(k, v)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            if with_body:
                self.wfile.write(body)

        def do_GET(self):
            self._answer(True)

        def do_HEAD(self):
            self._answer(False)

    return Handler


def main() -> int:
    ap = argparse.ArgumentParser(description="Serve web/depositor/index.html on 127.0.0.1 only.")
    ap.add_argument("--port", type=int, default=8645)
    args = ap.parse_args()
    body = PAGE.read_bytes()
    headers = headers_for(body)
    server = http.server.HTTPServer((HOST, args.port), handler(body, headers, args.port))
    print(f"serving {PAGE.name} (sha256 {hashlib.sha256(body).hexdigest()}, {len(body)} bytes)", flush=True)
    print(f"open http://{HOST}:{args.port}/ in the browser that has your wallet; Ctrl-C stops the server", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
