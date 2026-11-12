#!/usr/bin/env python3
"""One capture of the crown run (e2e/crown/README.md, stages 4 and 5): what LEZ session B reads of the vault at a
block that a Sepolia light-client finality update finalizes, with the vault v5's ETH registry and fee slots.

  capture.py --vault ADDR --from-block N --updates-from P [--expect-leaves n] [--min-block B] [--driver BIN] OUT
  capture.py slots [--driver BIN]
  capture.py check DIR --vault ADDR      every proven slot of a capture (also capture-anvil.py's) against its header

The vault slots proven (eth_getProof at the finalized block; lez/scripts/fetch-attest-fixture.py --network sepolia):
  0 depositRoot(), 1 nextLeafIndex(), 2 storedLezConfigId()   (checked against the getters at the same block)
  sha256("ferrata/vault/registry/v1" || ETH)                  the ETH registry entry's hash   (merged bridge: Register)
  sha256("ferrata/vault/fees/v1" || ETH || u64be 0), ... 1   fee schedules 0 and 1 of ETH    (Register, ImportFees)
ETH is the zero address (20 zero bytes). These are bridge-wire `register_slots(ETH)[..3]` (lez/bridge-wire lib.rs:
registry_slot, fee_slot) and vault/src/FerrataRegistry.sol's `_store` slots; with --driver the list is compared with
`crown-driver capture-slots` (the LEZ side's own list) before anything is fetched.

OUT must not exist; the capture is written to OUT.part and renamed when it holds a block >= --min-block and, with
--expect-leaves, exactly that many vault leaves (deposits.count and slot 1). Exit 0: captured; 3: not yet (the finality
update does not cover --min-block yet, or another leaf count: run again after the next update, about 6.4 minutes);
1: an error. OUT.json (beside OUT, which stays exactly as fetch-attest-fixture.py wrote it) records the slots, their
values and the checks.
Read-only: public endpoints, generic User-Agent (fetch-attest-fixture.py), nothing sent.
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
FETCH = os.path.join(REPO, "lez", "scripts", "fetch-attest-fixture.py")
ETH = bytes(20)
GETTERS = [(0, "depositRoot()"), (1, "nextLeafIndex()"), (2, "storedLezConfigId()")]


def word(n: int) -> str:
    return "0x" + n.to_bytes(32, "big").hex()


def registry_slot(token: bytes) -> str:
    return "0x" + hashlib.sha256(b"ferrata/vault/registry/v1" + token).hexdigest()


def fee_slot(token: bytes, index: int) -> str:
    return "0x" + hashlib.sha256(b"ferrata/vault/fees/v1" + token + index.to_bytes(8, "big")).hexdigest()


def crown_slots() -> list:
    """[(slot word, name, getter or None)] in the order of crown-driver capture-slots."""
    rows = [(word(n), f"slot {n}", g) for n, g in GETTERS]
    rows += [(registry_slot(ETH), "registry entry of ETH", None), (fee_slot(ETH, 0), "fee schedule 0 of ETH", None),
             (fee_slot(ETH, 1), "fee schedule 1 of ETH", None)]
    return rows


def storage_spec(vault: str) -> str:
    items = [f"{n}={g}" for n, g in GETTERS] + [s for s, _, g in crown_slots() if g is None]
    return f"{vault}:" + ",".join(items)


def check_driver(driver: str) -> None:
    """The LEZ side's own slot list must be ours (same words, same order)."""
    out = subprocess.run([driver, "capture-slots"], capture_output=True, text=True, timeout=60)
    if out.returncode != 0:
        raise SystemExit(f"{driver} capture-slots failed: {out.stderr.strip()[-300:]}")
    theirs = [s["slot"].lower() for s in json.loads(out.stdout)["slots"]]
    ours = [s for s, _, _ in crown_slots()]
    if theirs != ours:
        raise SystemExit(f"crown-driver capture-slots {theirs} != capture.py {ours}")


def verify_capture(d: str, vault: str) -> dict:
    """header.rlp hashes to block.json's hash; the vault's account and every slot of its eth_getProof answer
    verify against that header's state root; the crown slots are all there with the fixture's values."""
    sys.path.insert(0, HERE)
    import mpt
    fx = json.load(open(os.path.join(d, "fixture.json")))
    def answer(name):  # the files keep the whole JSON-RPC answer
        x = json.load(open(os.path.join(d, name)))
        return x.get("result", x) if isinstance(x, dict) else x
    block = answer("block.json")
    header = open(os.path.join(d, "header.rlp"), "rb").read()
    n = int(block["number"], 16)
    if fx.get("block") is not None and int(fx["block"]) != n:
        raise ValueError(f"fixture.json's block {fx['block']} is not block.json's {n}")
    own = "0x" + mpt.keccak256(header).hex()
    if fx.get("state_root_patched"):
        # lez/crown/capture-anvil.py (fork rehearsals): anvil reports a zero state root, so the stand-in header carries
        # the proofs' trie root instead and hashes to fixture.json's block_hash; every other field is anvil's block's.
        fields = mpt.rlp_decode(header)
        if own != fx["block_hash"].lower() or "0x" + fields[3].hex() != fx["state_root"].lower():
            raise ValueError("the stand-in header is not fixture.json's block_hash / state_root")
        if "0x" + fields[0].hex() != block["parentHash"].lower() or int.from_bytes(fields[8], "big") != n \
                or int.from_bytes(fields[11], "big") != int(block["timestamp"], 16):
            raise ValueError("the stand-in header's parent, number or time is not anvil's block's")
    else:
        if own != block["hash"].lower():
            raise ValueError("header.rlp does not hash to block.json's hash")
        if fx.get("block_hash") and fx["block_hash"].lower().removeprefix("0x") != block["hash"].lower().removeprefix("0x"):
            raise ValueError("fixture.json's block hash is not block.json's")
    names = [f for f in os.listdir(d) if f.lower() == f"proof_{vault}_{n}.json"]
    if not names:
        raise ValueError(f"no proof_{vault}_{n}.json")
    proof = answer(names[0])
    got = mpt.check_account_and_slots(mpt.header_state_root(header), proof)
    rows = {r["slot"].lower(): int(r["value"], 16) for e in fx.get("storage", []) if e["address"].lower() == vault
            for r in e["slots"]}
    for s, name, _ in crown_slots():
        if s not in got["slots"] or got["slots"][s] != rows.get(s):
            raise ValueError(f"{name} ({s}): proven {got['slots'].get(s)}, fixture {rows.get(s)}")
    return {"block": n, "hash": own, "standIn": bool(fx.get("state_root_patched")), "stateRoot": "0x" + mpt.header_state_root(header).hex(),
            "codeHash": got["codeHash"], "slots_proven": len(got["slots"])}


def main() -> int:
    if sys.argv[1:2] == ["check"]:
        ap = argparse.ArgumentParser(prog="capture.py check")
        ap.add_argument("dir")
        ap.add_argument("--vault", required=True)
        a = ap.parse_args(sys.argv[2:])
        print(json.dumps(verify_capture(a.dir, a.vault.lower())))
        return 0
    if sys.argv[1:2] == ["slots"]:
        ap = argparse.ArgumentParser(prog="capture.py slots")
        ap.add_argument("--driver")
        a = ap.parse_args(sys.argv[2:])
        if a.driver:
            check_driver(a.driver)
        print(json.dumps({"slots": [{"slot": s, "what": n, "getter": g} for s, n, g in crown_slots()],
                          "storage": storage_spec("{vault}"), "driver_checked": bool(a.driver)}, indent=1))
        return 0
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--vault", required=True)
    ap.add_argument("--from-block", type=int, required=True, help="the vault's deployment block (its Deposit logs from)")
    ap.add_argument("--expect-leaves", type=int)
    ap.add_argument("--min-block", type=int, default=0, help="the capture's block must be at least this (the deposit's)")
    ap.add_argument("--updates-from", type=int, required=True,
                    help="first period update: the bundle fixture's last period + 1 (drive.py passes it)")
    ap.add_argument("--driver", help="crown-driver: compare its capture-slots first")
    ap.add_argument("out")
    a = ap.parse_args()
    vault = a.vault.lower()
    if len(vault) != 42 or not vault.startswith("0x"):
        raise SystemExit(f"not an address: {a.vault}")
    if os.path.exists(a.out):
        raise SystemExit(f"{a.out} exists")
    if a.driver:
        check_driver(a.driver)
    part = a.out + ".part"
    shutil.rmtree(part, ignore_errors=True)
    cmd = [sys.executable, "-I", FETCH, "--network", "sepolia", "--storage", storage_spec(vault),
           "--deposits", f"{vault}:{a.from_block}", "--updates-from", str(a.updates_from), part]
    print("capture: " + " ".join(cmd[2:]), flush=True)
    rc = subprocess.run(cmd).returncode
    if rc != 0 or not os.path.isfile(os.path.join(part, "SHA256SUMS")):
        shutil.rmtree(part, ignore_errors=True)
        print(f"capture: fetch-attest-fixture.py exit {rc}; nothing kept (run again after the next finality update)")
        return 3
    fx = json.load(open(os.path.join(part, "fixture.json")))
    rows = {r["slot"].lower(): r for e in fx.get("storage", []) if e["address"].lower() == vault for r in e["slots"]}
    missing = [n for s, n, _ in crown_slots() if s not in rows]
    if missing:
        shutil.rmtree(part, ignore_errors=True)
        raise SystemExit(f"the capture lacks {missing}")
    block, leaves = int(fx.get("block") or 0), (fx.get("deposits") or {}).get("count")
    slot1 = int(rows[word(1)]["value"], 16)
    checks = {"block >= min_block": block >= a.min_block,
              "deposits.count == slot 1": leaves == slot1,
              "leaves as expected": a.expect_leaves is None or (leaves == a.expect_leaves and slot1 == a.expect_leaves)}
    record = {"vault": vault, "block": block, "finalized_slot": fx.get("finalized_slot"), "min_block": a.min_block,
              "expect_leaves": a.expect_leaves, "leaves": leaves, "updates_from": a.updates_from,
              "slots": [{"slot": s, "what": n, "getter": g, "value": rows[s]["value"]} for s, n, g in crown_slots()],
              "checks": checks, "driver_checked": bool(a.driver)}
    if not all(checks.values()):
        shutil.rmtree(part, ignore_errors=True)
        print(f"capture: not yet: {json.dumps(record)}")
        return 3
    try:
        record["proofs"] = verify_capture(part, vault)
    except (ValueError, OSError, KeyError) as e:
        shutil.rmtree(part, ignore_errors=True)
        raise SystemExit(f"the capture's proofs do not verify: {e}")
    os.rename(part, a.out)
    with open(a.out.rstrip("/") + ".json", "w") as f:
        json.dump(record, f, indent=1)
    for r in record["slots"]:
        print(f"capture: block {block} {r['what']}: {r['value']}")
    print(f"capture: {a.out} (block {block}, leaves {leaves}; {record['proofs']['slots_proven']} slots proven against"
          f" state root {record['proofs']['stateRoot']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
