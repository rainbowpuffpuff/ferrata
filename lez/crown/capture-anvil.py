#!/usr/bin/env python3
"""The execution half of a capture, from a LOCAL anvil fork (rehearsals only; STAND-IN).

  capture-anvil.py --rpc http://127.0.0.1:8545 --vault ADDR --from-block N --slot 0x.. [--slot ..] OUT

Writes into OUT (which must not exist) the files `crown-driver --chain stand-in --capture OUT` reads, in
the format of lez/scripts/fetch-attest-fixture.py --network sepolia: block.json (eth_getBlockByNumber of
the latest block), header.rlp (rebuilt from it and checked against its hash; anvil's fork mode writes a zero
state root, which is replaced by the root of the trie its eth_getProof answers from, and the hash is then the stand-in's), proof_<vault>_<n>.json
(eth_getProof of the vault and the slots), deposits_<n>.json (the vault's Deposit logs from N), and
fixture.json. There is no finality_update.ssz: the stand-in's committee (crown-driver) finalizes the
block. Python standard library only; only a loopback RPC is accepted.
"""
import argparse
import importlib.util
import json
import os
import sys
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location("fetch_mainnet_e2e", os.path.join(HERE, "..", "..", "scripts", "fetch-mainnet-e2e.py"))
e2e = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(e2e)

DEPOSIT_EVENT = "Deposit(address,uint64,uint64,address,uint256,bytes32)"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--rpc", required=True)
    ap.add_argument("--vault", required=True)
    ap.add_argument("--from-block", type=int, required=True)
    ap.add_argument("--slot", action="append", required=True)
    ap.add_argument("out")
    a = ap.parse_args()
    host = a.rpc.split("//", 1)[-1].split(":")[0].split("/")[0]
    if host not in ("127.0.0.1", "localhost"):
        raise SystemExit(f"refusing {host}: a local anvil only")
    os.makedirs(a.out)
    vault = a.vault.lower()

    def call(method, params, name=None):
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
        req = urllib.request.Request(a.rpc, body, {"content-type": "application/json"})
        raw = urllib.request.urlopen(req, timeout=60).read()
        got = json.loads(raw)
        if "error" in got:
            raise SystemExit(f"{method}: {got['error']}")
        if name:
            open(os.path.join(a.out, name), "wb").write(raw)
        return got["result"]

    head = call("eth_getBlockByNumber", ["latest", False])
    n = int(head["number"], 16)
    block = call("eth_getBlockByNumber", [hex(n), False], "block.json")
    words = ["0x" + int(s, 16).to_bytes(32, "big").hex() for s in a.slot]
    proof = call("eth_getProof", [vault, words, hex(n)], f"proof_{vault}_{n}.json")
    # anvil's fork mode writes a zero state root into the blocks it mines; its eth_getProof answers from its own
    # trie. The stand-in header carries that trie's root (keccak of the account proof's first node) instead.
    patched = int(block["stateRoot"], 16) == 0
    header = dict(block)
    if patched:
        header["stateRoot"] = "0x" + e2e.keccak256(bytes.fromhex(proof["accountProof"][0][2:])).hex()
    rlp = e2e.header_rlp(header)
    if not patched and "0x" + e2e.keccak256(rlp).hex() != block["hash"]:
        raise SystemExit(f"the rebuilt header of block {n} does not hash to {block['hash']}")
    open(os.path.join(a.out, "header.rlp"), "wb").write(rlp)
    topic = "0x" + e2e.keccak256(DEPOSIT_EVENT.encode()).hex()
    logs = call("eth_getLogs", [{"address": vault, "topics": [topic], "fromBlock": hex(a.from_block), "toBlock": hex(n)}])
    open(os.path.join(a.out, f"deposits_{n}.json"), "w").write(json.dumps(logs, indent=1) + "\n")
    slots = [{"slot": w, "value": "0x" + int(sp["value"], 16).to_bytes(32, "big").hex()} for w, sp in zip(words, proof["storageProof"])]
    manifest = {
        "network": "anvil fork of Sepolia (STAND-IN: not finalized by Sepolia's beacon chain)",
        "block": n,
        "block_hash": "0x" + e2e.keccak256(rlp).hex(),
        "anvil_block_hash": block["hash"],
        "state_root_patched": patched,
        "timestamp": int(block["timestamp"], 16),
        "state_root": header["stateRoot"],
        "storage": [{"address": vault, "slots": slots}],
        "deposits": {"vault": vault, "from_block": a.from_block, "event": DEPOSIT_EVENT, "topic": topic, "count": len(logs)},
        "updates": [],
    }
    open(os.path.join(a.out, "fixture.json"), "w").write(json.dumps(manifest, indent=2) + "\n")
    print(f"capture of block {n} {manifest['block_hash']} (state root patched: {patched}): {len(logs)} deposit(s), {len(words)} slot(s)")


if __name__ == "__main__":
    main()
