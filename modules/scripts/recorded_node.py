"""A Logos Blockchain node's HTTP API, answered from saved answers (no node runs): what
ferrata_attestor reads from its node, as realblocks/extract.sh and checkpoint/attestor/zone.sh save it;
and, on the same port, a LEZ node's JSON-RPC `getBlock` stood in from the same answers.

  GET /cryptarchia/info                          info.json (or info-start.json)
  GET /channel/<id>                              channel.json, for the channel given (default the
                                                 LEZ devnet channel 0101...01); 404 for another
  GET /cryptarchia/blocks?slot_from=A&slot_to=B  the saved blocks whose header slot is in [A, B]
  POST {"method": "getBlock", "params": [ID]}    the LEZ block ID as inscribed on the channel in the saved
                                                 blocks (the first inscription of that id), its status byte
                                                 set to Finalized, base64 as LEZ serves borsh(Block); null
                                                 for an id never inscribed

  python3 -I recorded_node.py ANSWERS_DIR PORT LOG [CHANNEL_HEX]
  python3 -I recorded_node.py --save-lez ANSWERS_DIR OUT_DIR [CHANNEL_HEX]
      writes the same getBlock answers as OUT_DIR/lez/getBlock-<id>.json, the layout ferrata_attestor
      reads with `attest --lez-answers` (the module's `lez_dir`)

The LEZ stand-in assumes what the attestor exists to check (that every inscribed block applied, as
`ferrata_attestor devnet-lez-standin` does): it is for replays of recorded honest runs, whose zones
saved no LEZ node's answers, never for a live attestor.

Binds 127.0.0.1 only; one line per request in LOG.
"""

import base64
import http.server
import json
import os
import sys
import urllib.parse

OP_CHANNEL_INSCRIBE = 0x11
FINALIZED = 2


def load_blocks(answers):
    blocks = []
    for name in os.listdir(os.path.join(answers, "blocks")):
        with open(os.path.join(answers, "blocks", name)) as f:
            blocks.extend(json.load(f))
    return blocks


def lez_blocks(blocks, channel):
    """LEZ block id -> the bytes LEZ would serve: the first inscription of that id on the channel, in slot
    order, its last byte (bedrock_status, outside the block hash) set to Finalized."""
    out = {}
    for b in sorted(blocks, key=lambda b: b["header"]["slot"]):
        for tx in b.get("transactions", []):
            for op in tx.get("mantle_tx", {}).get("ops", []):
                p = op.get("payload", {})
                if op.get("opcode") != OP_CHANNEL_INSCRIBE or str(p.get("channel_id", "")).lower() != channel:
                    continue
                try:
                    data = bytearray.fromhex(p["inscription"])
                except (KeyError, ValueError):
                    continue
                if len(data) < 9:
                    continue
                data[-1] = FINALIZED
                out.setdefault(int.from_bytes(data[:8], "little"), bytes(data))
    return out


def get_block_reply(lez, req_id, block_id):
    b = lez.get(block_id)
    return {"jsonrpc": "2.0", "id": req_id, "result": base64.b64encode(b).decode() if b is not None else None}


def save_lez(answers, out, channel):
    lez = lez_blocks(load_blocks(answers), channel)
    os.makedirs(os.path.join(out, "lez"), exist_ok=True)
    for block_id in lez:
        with open(os.path.join(out, "lez", f"getBlock-{block_id}.json"), "w") as f:
            json.dump(get_block_reply(lez, 1, block_id), f)
    print(f"recorded_node.py: {len(lez)} inscribed LEZ blocks saved as a LEZ node's final blocks under {out}/lez")


def serve(answers, port, log, channel):
    info_path = os.path.join(answers, "info.json")
    if not os.path.exists(info_path):
        info_path = os.path.join(answers, "info-start.json")
    info = open(info_path, "rb").read()
    channel_doc = open(os.path.join(answers, "channel.json"), "rb").read()
    blocks = load_blocks(answers)
    lez = lez_blocks(blocks, channel)

    class Handler(http.server.BaseHTTPRequestHandler):
        def reply(self, code, body, kind="application/json", line=None):
            self.send_response(code)
            self.send_header("Content-Type", kind)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            with open(log, "a") as f:
                f.write(f"{line or self.path} {code} {len(body)}\n")

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

        def do_POST(self):
            n = int(self.headers.get("Content-Length") or 0)
            try:
                req = json.loads(self.rfile.read(n) or b"null")
                method, params, req_id = req["method"], req.get("params") or [], req.get("id")
            except (ValueError, KeyError, TypeError):
                return self.reply(400, b"not a JSON-RPC request\n", "text/plain", "POST (unreadable)")
            if method != "getBlock" or len(params) != 1 or not isinstance(params[0], int):
                body = {"jsonrpc": "2.0", "id": req_id, "error": {"code": -32601, "message": f"not served: {method}"}}
                return self.reply(200, json.dumps(body).encode(), line=f"POST {method}")
            body = json.dumps(get_block_reply(lez, req_id, params[0])).encode()
            return self.reply(200, body, line=f"POST getBlock {params[0]}")

        def log_message(self, *args):
            pass

    http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()


if __name__ == "__main__":
    if sys.argv[1] == "--save-lez":
        save_lez(sys.argv[2], sys.argv[3], (sys.argv[4] if len(sys.argv) > 4 else "01" * 32).lower())
    else:
        serve(sys.argv[1], int(sys.argv[2]), sys.argv[3], (sys.argv[4] if len(sys.argv) > 4 else "01" * 32).lower())
