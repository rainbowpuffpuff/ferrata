#!/usr/bin/env python3
"""Read the Ferrata LEZ test network and check it yourself. Python 3.8+, standard library only, read-only.

    python3 ferrata_check.py https://<site>/          # the site serves /config.json and /rpc

1. The head block: getLastBlockId, getBlock; its hash recomputed from its bytes (SHA-256 over LEZ's encoding).
2. Each light-client instance in the site's config.json: its finalized Ethereum beacon header, read from the
   instance's account (getAccountView) and hashed to its block root (SSZ hash_tree_root).
3. That root compared with a public beacon API's root for the same slot (the providers in config.json).
Exit 0 if every check holds, 1 if one fails, 2 if something could not be read.
"""
import base64
import hashlib
import json
import struct
import sys
import urllib.request
from urllib.parse import urljoin

B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
UA = {"User-Agent": "ferrata-check/1", "Content-Type": "application/json", "Accept": "application/json"}


def b58encode(b):
    n = int.from_bytes(b, "big")
    s = ""
    while n:
        n, r = divmod(n, 58)
        s = B58[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s


def b58decode(s):
    n = 0
    for c in s:
        n = n * 58 + B58.index(c)
    body = n.to_bytes((n.bit_length() + 7) // 8, "big") if n else b""
    return b"\0" * (len(s) - len(s.lstrip("1"))) + body


def http_json(url, body=None):
    req = urllib.request.Request(url, data=json.dumps(body).encode() if body is not None else None, headers=UA)
    with urllib.request.urlopen(req, timeout=20) as r:
        return json.loads(r.read())


def rpc(url, method, params=()):
    r = http_json(url, {"jsonrpc": "2.0", "id": 1, "method": method, "params": list(params)})
    if "error" in r:
        raise RuntimeError(f"{method}: {r['error']}")
    return r["result"]


def pad32(s):
    return s.encode().ljust(32, b"\0")


def block_hash(raw):
    """LEZ v0.4: SHA-256("/LEE/v0.3/Message/Block/" padded to 32 || id || prev || timestamp || body);
    the header is id(8) prev(32) hash(32) timestamp(8), the body follows, the last byte is the Bedrock status."""
    return hashlib.sha256(pad32("/LEE/v0.3/Message/Block/") + raw[0:40] + raw[72:80] + raw[80:-1]).digest()


def pda(program, tag, *parts, domain="ferrata-lez/v1/"):
    seed = hashlib.sha256(domain.encode() + tag.encode() + b"".join(parts)).digest()
    return hashlib.sha256(pad32("/LEE/v0.2/AccountId/PDA/") + program + seed).digest()


def beacon_root(ssz):
    """hash_tree_root of a BeaconBlockHeader given as its 112 SSZ bytes."""
    leaves = [ssz[0:8].ljust(32, b"\0"), ssz[8:16].ljust(32, b"\0"), ssz[16:48], ssz[48:80], ssz[80:112]] + [b"\0" * 32] * 3
    while len(leaves) > 1:
        leaves = [hashlib.sha256(leaves[i] + leaves[i + 1]).digest() for i in range(0, len(leaves), 2)]
    return leaves[0]


PRESET_CHAIN = {"mainnet": 1, "sepolia": 11155111, "holesky": 17000, "hoodi": 560048}


def account_bytes(s):
    t = s[7:] if s.startswith("Public/") else s
    h = t[2:] if t[:2] in ("0x", "0X") else t
    if len(h) == 64 and all(c in "0123456789abcdefABCDEF" for c in h):
        return bytes.fromhex(h)
    return b58decode(t)


def instances(site, cfg):
    """The site's config.json instances, else those of the deployment's programs.json."""
    if cfg.get("instances"):
        return cfg["instances"]
    try:
        dep = http_json(urljoin(site, "programs.json"))
    except Exception:  # noqa: BLE001 - no deployment published yet
        return []
    out = []
    for name, i in (dep.get("instances") or {}).items():
        if i.get("program") and i.get("instance_id"):
            out.append({"label": name.capitalize(), "program": b58encode(account_bytes(i["program"])), "instance_id": i["instance_id"][2:] if i["instance_id"].startswith("0x") else i["instance_id"],
                        "account": b58encode(account_bytes(i["instance_account"])) if i.get("instance_account") else None})
    return out


def main():
    site = (sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:18750/").rstrip("/") + "/"
    cfg = http_json(urljoin(site, "config.json"))
    rpc_url = urljoin(site, cfg.get("rpc") or "rpc")
    ok = True

    head = rpc(rpc_url, "getLastBlockId")
    raw = base64.b64decode(rpc(rpc_url, "getBlock", [head]))
    bid, = struct.unpack_from("<Q", raw, 0)
    ts, = struct.unpack_from("<Q", raw, 72)
    ntx, = struct.unpack_from("<I", raw, 80)
    status = ["Pending", "Safe", "Finalized"][raw[-1]]
    same = block_hash(raw) == raw[40:72]
    ok &= same
    print(f"head block #{bid}: {ntx} transaction(s), {status} on Bedrock, timestamp {ts} ms; "
          f"hash {raw[40:72].hex()[:16]}… recomputed: {'equal' if same else 'DIFFERENT'}")

    for inst in instances(site, cfg):
        program = b58decode(inst["program"])
        account = b58decode(inst["account"]) if inst.get("account") else pda(program, "instance", bytes.fromhex(inst["instance_id"]))
        view = rpc(rpc_url, "getAccountView", [{"account_id": b58encode(account), "program_account_id": inst["program"]}])
        state = bytes((view or {}).get("data", {}).get("actor_states", {}).get(inst["program"], []))
        if not state:
            print(f"{inst['label']}: no state at {b58encode(account)}")
            ok = False
            continue
        # InstanceState (borsh): id[32] config[202] active u8 store Vec<u8> applied u64 root_id[32] parent_id[32]
        chain_id, = struct.unpack_from("<Q", state, 32)
        genesis_time, seconds_per_slot = struct.unpack_from("<QQ", state, 32 + 176)
        (n,) = struct.unpack_from("<I", state, 32 + 202 + 1)
        store = state[32 + 202 + 1 + 4: 32 + 202 + 1 + 4 + n]
        # CompactStore: fork u8, then the finalized BeaconBlockHeader (112 bytes of SSZ)
        header = store[1:113]
        slot, = struct.unpack_from("<Q", header, 0)
        root = beacon_root(header)
        print(f"{inst['label']}: chain {chain_id}, finalized slot {slot} (Ethereum time {genesis_time + slot * seconds_per_slot}), "
              f"block root 0x{root.hex()}")
        for api in cfg.get("beacon_apis", {}).get(str(chain_id), []):
            url = urljoin(site, api).rstrip("/") + f"/eth/v1/beacon/headers/{slot}"
            try:
                theirs = http_json(url)["data"]["root"][2:]
                same = theirs == root.hex()
                ok &= same
                print(f"  {urllib.request.urlparse(url).netloc}: {'equal' if same else 'DIFFERENT: 0x' + theirs}")
            except Exception as e:  # noqa: BLE001 - a provider that does not answer is reported, not fatal
                print(f"  {url}: could not read ({e})")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    try:
        main()
    except Exception as e:  # noqa: BLE001
        print(f"error: {e}", file=sys.stderr)
        sys.exit(2)
