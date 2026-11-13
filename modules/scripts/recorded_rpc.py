#!/usr/bin/env python3
"""A JSON-RPC server that answers from recorded responses: the Ethereum side of the modules'
end-to-end test, standing in for a node (`eth_rpc_module` in Logos Core).

    python3 -I modules/scripts/recorded_rpc.py FIXTURE_DIR PORT [LOG_FILE]

FIXTURE_DIR holds `fixture.json` (its `chain_id`), `block_<n>.json` (`eth_getBlockByNumber`
responses) and `receipts_<n>.json` (`eth_getBlockReceipts` responses), as the attestation fixture
`crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10` does. Answers eth_chainId,
eth_blockNumber (the highest recorded block), eth_getBlockByNumber and eth_getBlockReceipts for
recorded blocks, and eth_getProof from the recorded `proof_<what>_<n>.json` responses (cut to the keys
asked for); anything else is a JSON-RPC error. Listens on 127.0.0.1 only. Each request is
logged (method, params, bytes answered) to LOG_FILE when given.
"""

import json
import pathlib
import re
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def result_of(path):
    data = json.loads(path.read_text())
    return data["result"] if isinstance(data, dict) and "jsonrpc" in data else data


def main():
    fixture = pathlib.Path(sys.argv[1])
    port = int(sys.argv[2])
    log = open(sys.argv[3], "a") if len(sys.argv) > 3 else None
    chain_id = json.loads((fixture / "fixture.json").read_text())["chain_id"]
    blocks = {}
    for p in fixture.glob("block_*.json"):
        m = re.fullmatch(r"block_(\d+)\.json", p.name)
        if m:
            blocks[int(m.group(1))] = p
    receipts = {int(re.fullmatch(r"receipts_(\d+)\.json", p.name).group(1)): p for p in fixture.glob("receipts_*.json")}
    # eth_getProof responses: proof_<what>_<n>.json, by block.
    proofs = {}
    for p in fixture.glob("proof_*_*.json"):
        m = re.fullmatch(r"proof_\w+?_(\d+)\.json", p.name)
        if m:
            proofs.setdefault(int(m.group(1)), []).append(p)

    def get_proof(address, keys, n):
        """A recorded eth_getProof at block n of `address` holding every key asked for, cut to those keys."""
        want = [int(k, 16) for k in keys]
        for p in proofs.get(n, []):
            r = result_of(p)
            if r.get("address", "").lower() != address.lower():
                continue
            have = {int(e["key"], 16): e for e in r.get("storageProof", [])}
            if all(k in have for k in want):
                return dict(r, storageProof=[have[k] for k in want])
        return None

    def number(tag):
        if tag == "latest" or tag == "finalized":
            return max(blocks)
        return int(tag, 16)

    def answer(method, params):
        if method == "eth_chainId":
            return hex(chain_id)
        if method == "eth_blockNumber":
            return hex(max(blocks))
        if method == "eth_getBlockByNumber":
            n = number(params[0])
            return result_of(blocks[n]) if n in blocks else None
        if method == "eth_getBlockReceipts":
            n = number(params[0])
            return result_of(receipts[n]) if n in receipts else None
        if method == "eth_getProof":
            r = get_proof(params[0], params[1], number(params[2]))
            if r is None:
                raise KeyError(method)
            return r
        raise KeyError(method)

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def do_POST(self):
            body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            req = None
            try:
                req = json.loads(body)
                try:
                    out = {"jsonrpc": "2.0", "id": req.get("id"), "result": answer(req["method"], req.get("params", []))}
                except KeyError:
                    out = {"jsonrpc": "2.0", "id": req.get("id"), "error": {"code": -32601, "message": "method not recorded"}}
            except ValueError:
                out = {"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": "parse error"}}
            data = json.dumps(out).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(data)
            if log:
                log.write(json.dumps({"method": req.get("method") if isinstance(req, dict) else None,
                                      "params": req.get("params") if isinstance(req, dict) else None,
                                      "bytes": len(data),
                                      # Which client asked: the module's own (Accept: application/json) or
                                      # eth_rpc_module's reqwest client (accept: */*), in logos-io.sh's runs.
                                      "accept": self.headers.get("Accept"),
                                      "user_agent": self.headers.get("User-Agent")}) + "\n")
                log.flush()

        def log_message(self, *args):
            pass

    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()


if __name__ == "__main__":
    main()
