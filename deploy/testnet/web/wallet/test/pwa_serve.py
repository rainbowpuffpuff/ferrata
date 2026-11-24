#!/usr/bin/env python3
"""The wallet as the test network's host serves it, for the PWA's tests: the bundle at /wallet/ with the host's headers
(deploy/testnet/host/Caddyfile: the wallet's Content-Security-Policy, the hidden paths, no-cache on the page and its
config), /rpc and /faucet passed to an upstream (a local sequencer's gateway, or the public test network for read-only
runs), and optionally the Android page at /android/ and /.well-known/.

    python3 -I pwa_serve.py --port 18660 [--upstream https://HOST] [--config FILE] [--android DIR]

GET /__bump makes sw.js differ by one comment line (a new build, for the update test); GET /__tamper toggles an sw.js
whose list names another app.js than the one served (the service worker must refuse that build); GET /__config?json=TEXT
serves TEXT as config.json until GET /__config. Standard library only.
"""
import argparse
import os
import re
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
HIDDEN = {"README.md", "vendor/build.sh", "vendor/package.json", "vendor/package-lock.json", "vendor/entry.js"}
TYPES = {".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8",
         ".json": "application/json", ".png": "image/png", ".svg": "image/svg+xml", ".apk": "application/vnd.android.package-archive",
         ".txt": "text/plain; charset=utf-8"}
CSP = ("default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; "
       "manifest-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'")
SITE_CSP = "script-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"
STATE = {"bump": 0, "tamper": False, "config": None}


class Handler(BaseHTTPRequestHandler):
    upstream = None
    config = None
    android = None
    root = ROOT

    def log_message(self, fmt, *args):
        pass

    def reply(self, code, body: bytes, ctype="text/plain; charset=utf-8", headers=None):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header("X-Frame-Options", "DENY")
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def file(self, base, rel, csp, nocache=False):
        path = os.path.realpath(os.path.join(base, rel))
        if not path.startswith(os.path.realpath(base) + os.sep) or not os.path.isfile(path):
            return self.reply(404, b"not found")
        with open(path, "rb") as f:
            body = f.read()
        if rel == "sw.js" and STATE["bump"]:
            body += f"\n// bump {STATE['bump']}\n".encode()
        if rel == "sw.js" and STATE["tamper"]:
            # A new build whose list names another app.js than the one served (half-deployed, or swapped).
            body = re.sub(rb"const BUILD = '[0-9a-f]+';", b"const BUILD = 'tampered0000000" + str(STATE["bump"] % 10).encode() + b"';", body)
            body = re.sub(rb"('app\.js': ')[0-9a-f]{64}'", rb"\g<1>" + b"0" * 64 + b"'", body)
        headers = {"Content-Security-Policy": csp}
        if nocache:
            headers["Cache-Control"] = "no-cache"
        return self.reply(200, body, TYPES.get(os.path.splitext(rel)[1], "application/octet-stream"), headers)

    def do_HEAD(self):
        self.do_GET()

    def do_GET(self):
        p = self.path.split("?", 1)[0].split("#", 1)[0]
        if p == "/__bump":
            STATE["bump"] += 1
            return self.reply(200, str(STATE["bump"]).encode())
        if p == "/__tamper":
            STATE["tamper"] = not STATE["tamper"]
            return self.reply(200, str(STATE["tamper"]).encode())
        if p == "/__config":  # ?json=<text>: serve that as config.json until /__config without it
            q = urllib.parse.parse_qs(self.path.split("?", 1)[1]) if "?" in self.path else {}
            STATE["config"] = q["json"][0].encode() if "json" in q else None
            return self.reply(200, b"ok")
        if p == "/wallet":
            return self.reply(308, b"", headers={"Location": "/wallet/"})
        if p.startswith("/wallet/"):
            rel = p[len("/wallet/"):] or "index.html"
            if rel.startswith("test/") or rel in HIDDEN:
                return self.reply(404, b"not found")
            nocache = rel in ("index.html", "config.json")
            if rel == "config.json" and STATE["config"] is not None:
                return self.reply(200, STATE["config"], "application/json", {"Content-Security-Policy": CSP, "Cache-Control": "no-cache"})
            if rel == "config.json" and self.config:
                with open(self.config, "rb") as f:
                    return self.reply(200, f.read(), "application/json", {"Content-Security-Policy": CSP, "Cache-Control": "no-cache"})
            return self.file(self.root, rel, CSP, nocache)
        if self.android and (p.startswith("/android/") or p.startswith("/.well-known/")):
            rel = p.lstrip("/")
            if rel.endswith("/"):
                rel += "index.html"
            return self.file(self.android, rel, SITE_CSP)
        return self.reply(404, b"not found")

    def do_POST(self):
        p = self.path.split("?", 1)[0]
        if p not in ("/rpc", "/faucet") or not self.upstream:
            return self.reply(404, b"not found")
        body = self.rfile.read(int(self.headers.get("Content-Length") or 0))
        req = urllib.request.Request(self.upstream + p, data=body, method="POST",
                                     headers={"Content-Type": "application/json", "User-Agent": "ferrata-pwa-test"})
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                out, code, ctype = r.read(), r.status, r.headers.get("Content-Type", "application/json")
                retry = r.headers.get("Retry-After")
        except urllib.error.HTTPError as e:
            out, code, ctype, retry = e.read(), e.code, e.headers.get("Content-Type", "application/json"), e.headers.get("Retry-After")
        except Exception as e:  # noqa: BLE001 - the page sees a gateway error, as from the host
            out, code, ctype, retry = str(e).encode(), 502, "text/plain", None
        return self.reply(code, out, ctype, {"Retry-After": retry} if retry else None)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=18660)
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--upstream", help="https://HOST: /rpc and /faucet go there")
    ap.add_argument("--config", help="serve this file as /wallet/config.json")
    ap.add_argument("--android", help="serve /android/ and /.well-known/ from this directory")
    ap.add_argument("--root", help="serve this bundle instead of this one (an older build, for comparisons)")
    a = ap.parse_args()
    Handler.upstream = a.upstream.rstrip("/") if a.upstream else None
    Handler.config = a.config
    Handler.android = a.android
    Handler.root = a.root or ROOT
    ThreadingHTTPServer((a.host, a.port), Handler).serve_forever()


if __name__ == "__main__":
    main()
