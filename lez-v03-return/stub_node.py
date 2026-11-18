#!/usr/bin/env python3
"""A local STUB of a LEZ v0.3.1 node, for the fork rehearsal of the return leg (e2e/testnet-return.sh fork).

It serves REAL blocks of the public LEZ testnet, recorded on 18 November 2026 (fixtures/testnet-block-*.json:
83000 and 83001 Finalized, 83002 and 83003 Pending), and a bridge state PLACED here from a burns.json, encoded as
lez-v03-bridge's core crate encodes `burns(cid)` and `root(cid, n)`. No block it serves holds those burns: the state is
a stand-in, so that the attestor's checks (the accumulator against the burns, the tip Finalized, the blocks between
the tip and the read) run on real blocks and a placed state. Listens on 127.0.0.1 only; answers getLastBlockId,
getBlock and getAccount, and refuses every other method. Standard library only.

  stub_node.py program                       the rehearsal's labelled bridge program id (base58)
  stub_node.py bridge-state --identity ID.json --burns BURNS.json --program P --imported N --deposit-root 0x..
        --out STATE.json                     the bridge's accounts, as the stub serves them
  stub_node.py serve --port P --blocks DIR --state STATE.json [--head N]
"""
from __future__ import annotations

import argparse
import http.server
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import standin as s  # noqa: E402

PROGRAM_LABEL = b"ferrata testnet-return rehearsal: STAND-IN bridge program id (served by a stub LEZ node)"


def rehearsal_program() -> str:
    return s.b58encode(s.sha256(PROGRAM_LABEL))


def bridge_accounts(program: bytes, config_id: bytes, chain_id: int, vault: bytes, leaves: list, imported: int,
                    deposit_root: bytes) -> dict:
    """{account (base58): data} of lez-v03-bridge's `burns(cid)` (BridgeState::Burns, variant 5: config_id, count u64,
    frontier Vec of 32 nodes) and `root(cid, n)` (BridgeState::Root, variant 1: config_id, evm_chain_id u64, vault,
    leaf_count u64, root, record_slot u64, block_number u64; the last two are 0 here: no record stands behind it)."""
    burns = (bytes([5]) + config_id + len(leaves).to_bytes(8, "little") + (32).to_bytes(4, "little")
             + b"".join(s.tree_frontier(leaves)))
    root = (bytes([1]) + config_id + chain_id.to_bytes(8, "little") + vault + imported.to_bytes(8, "little")
            + deposit_root + bytes(16))
    return {s.b58encode(s.bridge_pda(program, b"burns", config_id)): burns,
            s.b58encode(s.bridge_pda(program, b"root", config_id, imported.to_bytes(8, "little"))): root}


def cmd_bridge_state(a) -> None:
    ident = s.identity(a.identity)
    burns = json.load(open(a.burns))
    leaves = [s.burn_leaf(ident["configId"], k, s.unhex(b["token"], 20), s.unhex(b["amount"], 32),
                          s.unhex(b["c_out"], 32), s.unhex(b["r"], 32)) for k, b in enumerate(burns["burns"])]
    program = s.b58decode(s.account_id(a.program))
    accounts = bridge_accounts(program, ident["configId"], ident["chainId"], ident["vault"], leaves, a.imported,
                               s.unhex(a.deposit_root, 32))
    s.write(a.out, {"what": "STAND-IN: the bridge state a local stub LEZ node serves in the fork rehearsal, placed "
                            "from burns.json (no LEZ block holds these burns)", "program": s.b58encode(program),
                    "accounts": {acc: {s.b58encode(program): data.hex()} for acc, data in accounts.items()}})
    print(json.dumps({"accounts": sorted(accounts), "burn_count": len(leaves)}))


def cmd_serve(a) -> None:
    blocks = {}
    for name in os.listdir(a.blocks):
        if name.startswith("testnet-block-") and name.endswith(".json"):
            blocks[int(name[len("testnet-block-"):-len(".json")])] = json.load(open(os.path.join(a.blocks, name)))["result"]
    head = a.head if a.head is not None else max(blocks)

    def answer(method: str, params: list):
        if method == "getLastBlockId":
            return head
        if method == "getBlock":
            return blocks.get(int(params[0])) if int(params[0]) <= head else None
        if method == "getAccount":
            state = json.load(open(a.state)) if os.path.exists(a.state) else {"accounts": {}}
            shards = state["accounts"].get(params[0], {})
            return {"nonce": 0, "data": {"shards": {p: list(bytes.fromhex(d)) for p, d in shards.items()}}}
        raise ValueError(f"the stub answers reads only, not {method}")

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            req = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            try:
                out = {"jsonrpc": "2.0", "id": req.get("id"), "result": answer(req["method"], req.get("params", []))}
            except (ValueError, KeyError, IndexError) as e:
                out = {"jsonrpc": "2.0", "id": req.get("id"), "error": {"code": -32601, "message": str(e)}}
            body = json.dumps(out).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, fmt, *args):
            sys.stderr.write("stub LEZ node: " + fmt % args + "\n")

    server = http.server.ThreadingHTTPServer(("127.0.0.1", a.port), Handler)
    print(json.dumps({"stub": f"http://127.0.0.1:{server.server_address[1]}", "head": head,
                      "blocks": sorted(blocks)}), flush=True)
    server.serve_forever()


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("program")
    p = sub.add_parser("bridge-state"); p.add_argument("--identity", required=True)
    p.add_argument("--burns", required=True); p.add_argument("--program", required=True)
    p.add_argument("--imported", type=int, required=True); p.add_argument("--deposit-root", required=True)
    p.add_argument("--out", required=True)
    p = sub.add_parser("serve"); p.add_argument("--port", type=int, required=True)
    p.add_argument("--blocks", required=True); p.add_argument("--state", required=True); p.add_argument("--head", type=int)
    a = ap.parse_args()
    if a.cmd == "program":
        print(rehearsal_program())
    else:
        {"bridge-state": cmd_bridge_state, "serve": cmd_serve}[a.cmd](a)


if __name__ == "__main__":
    main()
