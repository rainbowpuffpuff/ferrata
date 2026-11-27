#!/usr/bin/env python3
"""test/serve.py --site: the host's layout for local runs (the build page's bundle at /, the wallet at /wallet/), and
nothing outside the bundle.

    python3 -I deploy/testnet/web/wallet/test/test_serve_site.py

Starts serve.py on a free loopback port with a scratch bundle beside this file, sends raw request paths (no client-side
normalisation) and checks each answer. Standard library only.
"""
import http.client
import os
import socket
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class ServeSite(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(dir=HERE)
        root = Path(cls.tmp.name)
        cls.site = root / "site"
        (cls.site / "testers").mkdir(parents=True)
        (cls.site / "index.html").write_text("<p>build page</p>")
        (cls.site / "testers" / "index.html").write_text("<p>testers</p>")
        (cls.site / "build.json").write_text("{}")
        (cls.site / "notes.sh").write_text("echo not a page type")
        (root / "secret.html").write_text("<p>outside the bundle</p>")
        (cls.site / "link.html").symlink_to(root / "secret.html")
        cls.port = free_port()
        cls.proc = subprocess.Popen([sys.executable, "-I", str(HERE / "serve.py"), "--port", str(cls.port), "--rpc",
                                     "http://127.0.0.1:1", "--site", str(cls.site)],
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            try:
                socket.create_connection(("127.0.0.1", cls.port), timeout=0.2).close()
                break
            except OSError:
                time.sleep(0.05)

    @classmethod
    def tearDownClass(cls):
        cls.proc.terminate()
        cls.proc.wait(timeout=10)
        cls.tmp.cleanup()

    def get(self, path):
        c = http.client.HTTPConnection("127.0.0.1", self.port, timeout=5)
        c.putrequest("GET", path, skip_accept_encoding=True)   # the path exactly as given
        c.endheaders()
        r = c.getresponse()
        body = r.read()
        c.close()
        return r.status, dict(r.getheaders()), body

    def test_the_bundle_at_the_root(self):
        self.assertEqual(self.get("/")[2], b"<p>build page</p>")
        self.assertEqual(self.get("/testers/")[2], b"<p>testers</p>")
        status, headers, _ = self.get("/build.json")
        self.assertEqual((status, headers["Content-Type"]), (200, "application/json"))
        self.assertNotIn("Content-Security-Policy", self.get("/")[1])   # the pages carry their own, in a meta element

    def test_the_wallet_under_wallet(self):
        status, headers, body = self.get("/wallet/")
        self.assertEqual(status, 200)
        self.assertIn(b"<html", body.lower())
        self.assertIn("default-src 'none'", headers["Content-Security-Policy"])
        self.assertEqual(self.get("/wallet/app.js")[0], 200)
        status, headers, _ = self.get("/wallet")
        self.assertEqual((status, headers.get("Location")), (308, "/wallet/"))

    def test_nothing_outside_the_bundle(self):
        for path in ("/../secret.html", "/testers/../../secret.html", "/link.html", "/notes.sh", "/missing.html",
                     "/%2e%2e/secret.html", "/wallet/../../secret.html"):
            status, _, body = self.get(path)
            self.assertEqual(status, 404, path)
            self.assertNotIn(b"outside the bundle", body, path)


if __name__ == "__main__":
    unittest.main()
