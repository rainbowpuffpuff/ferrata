"""A Logos Blockchain node's HTTP API, answered from saved answers (no node runs): what
ferrata_attestor reads from its node, as realblocks/extract.sh and checkpoint/attestor/zone.sh save it.

  GET /cryptarchia/info                          info.json (or info-start.json)
  GET /channel/<id>                              channel.json, for the channel given (default the
                                                 LEZ devnet channel 0101...01); 404 for another
  GET /cryptarchia/blocks?slot_from=A&slot_to=B  the saved blocks whose header slot is in [A, B]

  python3 -I recorded_node.py ANSWERS_DIR PORT LOG [CHANNEL_HEX]

Binds 127.0.0.1 only; one line per request in LOG.
"""

import http.server
import json
import os
import sys
import urllib.parse

answers, port, log = sys.argv[1], int(sys.argv[2]), sys.argv[3]
channel = (sys.argv[4] if len(sys.argv) > 4 else "01" * 32).lower()
info_path = os.path.join(answers, "info.json")
if not os.path.exists(info_path):
    info_path = os.path.join(answers, "info-start.json")
info = open(info_path, "rb").read()
channel_doc = open(os.path.join(answers, "channel.json"), "rb").read()
blocks = []
for name in os.listdir(os.path.join(answers, "blocks")):
    with open(os.path.join(answers, "blocks", name)) as f:
        blocks.extend(json.load(f))


class Handler(http.server.BaseHTTPRequestHandler):
    def reply(self, code, body, kind="application/json"):
        self.send_response(code)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
        with open(log, "a") as f:
            f.write(f"{self.path} {code} {len(body)}\n")

    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        if u.path == "/cryptarchia/info":
            return self.reply(200, info)
        if u.path.startswith("/channel/"):
            if u.path[len("/channel/"):].lower() != channel:
                return self.reply(404, b"unknown channel\n", "text/plain")
            return self.reply(200, channel_doc)
        if u.path == "/cryptarchia/blocks":
            q = urllib.parse.parse_qs(u.query)
            lo, hi = int(q["slot_from"][0]), int(q["slot_to"][0])
            sel = [b for b in blocks if lo <= b["header"]["slot"] <= hi]
            return self.reply(200, json.dumps(sel, separators=(",", ":")).encode())
        return self.reply(404, b"not found\n", "text/plain")

    def log_message(self, *args):
        pass


http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
