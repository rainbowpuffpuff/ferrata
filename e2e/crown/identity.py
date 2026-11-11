#!/usr/bin/env python3
"""The crown run's identity values that Python can compute exactly (standard library only).

  identity.py genesis-state --tip-id N --tip-hash 0x.. --keys 0x..[,0x..] [--posting-timeframe 0] [--config-tip-hash 0x..]
      the vault's genesisState: the checkpoint state S at the parent tip with no burns, no imports and no sunset
      (checkpoint-core state_digest, attestor-core key_set_hash; vault/test/real/RealBlocks.t.sol `_state`)
  identity.py commitment --chain-id 11155111 --vault 0x.. --depositor 0x.. --j 0 --kind public|private --dest 0x..
      the deposit commitment C (bridge core encoding.rs: s = HMAC(kAuth, "ferrata/s/v1" || chainId:u256 || vault ||
      depositor || j:u64), kAuth = HMAC(bridgeSeed, "ferrata/auth/v1"), C = sha256("ferrata/C/v1" || s || kind || dest);
      bridgeSeed = sha256("e2e: user bridge seed (test value)"), the test seed of bridge/e2e and the inbound plan)
  identity.py selftest
      genesis-state reproduces the parent state proven in vault/test/real/fixtures/real-blocks.json
"""
import argparse
import hashlib
import hmac
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))


def b32(x: str) -> bytes:
    v = bytes.fromhex(x.lower().removeprefix("0x"))
    if len(v) != 32:
        raise SystemExit(f"not 32 bytes: {x}")
    return v


def genesis_state(tip_id: int, tip_hash: bytes, keys: list, timeframe: int, config_tip_hash: bytes) -> bytes:
    key_set = hashlib.sha256(b"ferrata/keyset/v2" + len(keys).to_bytes(4, "big") + timeframe.to_bytes(4, "big")
                             + b"".join(keys)).digest()
    frontier = hashlib.sha256(b"ferrata/burn-frontier/v1" + bytes(32 * 32)).digest()
    return hashlib.sha256(b"ferrata/lez-state/v1" + tip_id.to_bytes(8, "big") + tip_hash + key_set + config_tip_hash
                          + (0).to_bytes(8, "big") + frontier + (0).to_bytes(8, "big") + b"\x00").digest()


def commitment(chain_id: int, vault: bytes, depositor: bytes, j: int, kind: int, dest: bytes) -> bytes:
    seed = hashlib.sha256(b"e2e: user bridge seed (test value)").digest()
    k_auth = hmac.new(seed, b"ferrata/auth/v1", hashlib.sha256).digest()
    s = hmac.new(k_auth, b"ferrata/s/v1" + chain_id.to_bytes(32, "big") + vault + depositor + j.to_bytes(8, "big"),
                 hashlib.sha256).digest()
    return hashlib.sha256(b"ferrata/C/v1" + s + bytes([kind]) + dest).digest()


def addr(x: str) -> bytes:
    v = bytes.fromhex(x.lower().removeprefix("0x"))
    if len(v) != 20:
        raise SystemExit(f"not an address: {x}")
    return v


def main() -> None:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    g = sub.add_parser("genesis-state")
    g.add_argument("--tip-id", type=int, required=True)
    g.add_argument("--tip-hash", required=True)
    g.add_argument("--keys", required=True)
    g.add_argument("--posting-timeframe", type=int, default=0)
    g.add_argument("--config-tip-hash", default="0x" + "00" * 32)
    c = sub.add_parser("commitment")
    c.add_argument("--chain-id", type=int, required=True)
    c.add_argument("--vault", required=True)
    c.add_argument("--depositor", required=True)
    c.add_argument("--j", type=int, required=True)
    c.add_argument("--kind", choices=["public", "private"], required=True)
    c.add_argument("--dest", required=True)
    sub.add_parser("selftest")
    a = ap.parse_args()
    if a.cmd == "genesis-state":
        keys = [b32(k) for k in a.keys.split(",")]
        print("0x" + genesis_state(a.tip_id, b32(a.tip_hash), keys, a.posting_timeframe, b32(a.config_tip_hash)).hex())
    elif a.cmd == "commitment":
        kind = 2 if a.kind == "public" else 1  # bridge core DestKind (the inbound plan's kind_byte)
        print("0x" + commitment(a.chain_id, addr(a.vault), addr(a.depositor), a.j, kind, b32(a.dest)).hex())
    else:
        f = json.load(open(os.path.join(REPO, "vault/test/real/fixtures/real-blocks.json")))
        journal = bytes.fromhex(f["journal"].removeprefix("0x"))
        proven_parent = journal[6 * 32:7 * 32]  # domain, chainId, vault, configId, nMin, tMin, parentState, ...
        got = genesis_state(int(f["parentTipId"]), b32(f["parentTipHash"]), [b32(f["labChannelKey"])],
                            int(f["postingTimeframe"]), b32(f["configTipHash"]))
        ok = got == proven_parent
        print(f"genesis-state of the real-blocks parent: {'0x' + got.hex()} {'= proven' if ok else '!= ' + proven_parent.hex()}")
        sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
