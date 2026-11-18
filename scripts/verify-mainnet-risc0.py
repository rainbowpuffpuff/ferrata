#!/usr/bin/env python3
"""Ferrata's real Groth16 seals checked by RISC Zero's own verifier on Ethereum mainnet: read-only, nothing is sent.

    python3 -I scripts/verify-mainnet-risc0.py [--rpc URL] [--block N] [--out DIR --tag NAME] [--list]

Every Groth16 seal the tree records (crown runs, receipt directories, the vault's real fixtures, the bridge's release
guest runs) is paired with the image id and the journal its own records give, and checked with eth_call against RISC
Zero's deployment on Ethereum mainnet as risc0-ethereum v3.0.0 records it
(e2e/runs/mainnet-risc0-verify/upstream/deployment.toml, a verbatim copy, lines 1-56): the router's
verify(seal, imageId, sha256(journal)) and the Groth16 verifier's own verify must return; the router must revert for
the seal with its last byte changed, for the image id with its last byte changed and for the digest of the journal
with its last byte changed. The journal digest is sha256 of the journal bytes, as the vault computes it
(vault/src/FerrataVault.sol, scripts/verify-sepolia.py).

The only JSON-RPC methods this script can send are eth_chainId, eth_blockNumber, eth_getCode and eth_call; any other
raises before anything is sent. The endpoint must answer chain id 1. Every call is made at one block (--block, or the
endpoint's latest when the run starts). Python 3.11+ standard library only.

    PASS       the chain agrees with the records
    FAIL       it does not (exit status 1)
    NOJOURNAL  a seal recorded without its journal: verify() cannot be called for it (listed, not a failure)
    SKIP       a recorded seal that is not a Groth16 v3.0.0 seal (a stand-in's), listed

Exit status 2: the endpoint is not Ethereum mainnet or stopped answering.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.parse
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RESULTS = "e2e/runs/mainnet-risc0-verify"
DEPLOYMENT_TOML = RESULTS + "/upstream/deployment.toml"  # risc0-ethereum v3.0.0, contracts/deployment.toml
CONTROL_ID_SOL = RESULTS + "/upstream/ControlID.sol"  # risc0-ethereum v3.0.0, contracts/src/groth16/ControlID.sol
DEFAULT_RPC = "https://ethereum-rpc.publicnode.com"
CHAIN_ID = 1
VERSION = "3.0.0"  # the risc0-ethereum release the vault pins (vault/README.md)
ALLOWED = frozenset({"eth_chainId", "eth_blockNumber", "eth_getCode", "eth_call"})
USER_AGENT = "verify-mainnet-risc0/1"

# ---------------------------------------------------------------------------------------------------------------
# Keccak-256 (function selectors only; hashlib's sha3_256 pads differently).
# ---------------------------------------------------------------------------------------------------------------
_RC = [0x0000000000000001, 0x0000000000008082, 0x800000000000808A, 0x8000000080008000, 0x000000000000808B,
       0x0000000080000001, 0x8000000080008081, 0x8000000000008009, 0x000000000000008A, 0x0000000000000088,
       0x0000000080008009, 0x000000008000000A, 0x000000008000808B, 0x800000000000008B, 0x8000000000008089,
       0x8000000000008003, 0x8000000000008002, 0x8000000000000080, 0x000000000000800A, 0x800000008000000A,
       0x8000000080008081, 0x8000000000008080, 0x0000000080000001, 0x8000000080008008]
_ROT = [[0, 36, 3, 41, 18], [1, 44, 10, 45, 2], [62, 6, 43, 15, 61], [28, 55, 25, 21, 56], [27, 20, 39, 8, 14]]
_M64 = (1 << 64) - 1


def _rol(x: int, n: int) -> int:
    return ((x << n) | (x >> (64 - n))) & _M64 if n else x


def _keccak_f(a: list) -> list:
    for rc in _RC:
        c = [a[x][0] ^ a[x][1] ^ a[x][2] ^ a[x][3] ^ a[x][4] for x in range(5)]
        d = [c[(x - 1) % 5] ^ _rol(c[(x + 1) % 5], 1) for x in range(5)]
        a = [[a[x][y] ^ d[x] for y in range(5)] for x in range(5)]
        b = [[0] * 5 for _ in range(5)]
        for x in range(5):
            for y in range(5):
                b[y][(2 * x + 3 * y) % 5] = _rol(a[x][y], _ROT[x][y])
        a = [[b[x][y] ^ ((~b[(x + 1) % 5][y]) & b[(x + 2) % 5][y]) for y in range(5)] for x in range(5)]
        a[0][0] ^= rc
    return a


def keccak256(data: bytes) -> bytes:
    rate = 136
    msg = bytearray(data) + b"\x01" + b"\x00" * ((rate - (len(data) + 1) % rate) % rate)
    msg[-1] |= 0x80
    a = [[0] * 5 for _ in range(5)]
    for off in range(0, len(msg), rate):
        block = msg[off:off + rate]
        for i in range(rate // 8):
            a[i % 5][i // 5] ^= int.from_bytes(block[8 * i:8 * i + 8], "little")
        a = _keccak_f(a)
    return b"".join(a[i % 5][i // 5].to_bytes(8, "little") for i in range(4))


def sel(sig: str) -> bytes:
    return keccak256(sig.encode())[:4]


def hx(x: bytes) -> str:
    return "0x" + x.hex()


def unhex(s: str) -> bytes:
    return bytes.fromhex(s.strip().lower().removeprefix("0x"))


ERRORS = {sel(s).hex(): s for s in ("VerificationFailed()", "SelectorMismatch(bytes4,bytes4)", "SelectorUnknown(bytes4)",
                                    "SelectorRemoved(bytes4)", "EnforcedPause()")}


# ---------------------------------------------------------------------------------------------------------------
# JSON-RPC: four read-only methods, nothing else.
# ---------------------------------------------------------------------------------------------------------------
class EndpointError(Exception):
    pass


class Reverted(Exception):
    def __init__(self, message: str, data: str | None):
        super().__init__(message)
        self.message, self.data = message, data

    def name(self) -> str:
        d = (self.data or "").lower().removeprefix("0x")
        return ERRORS.get(d[:8], f"revert data 0x{d[:8]}…" if d else "no revert data")


class Rpc:
    def __init__(self, url: str, block: int | None, pace: float):
        self.url, self.block, self.pace, self.log = url, block, pace, []

    def __call__(self, method: str, *params):
        if method not in ALLOWED:  # before anything is sent
            raise PermissionError(f"{method} is not one of {sorted(ALLOWED)}")
        body = json.dumps({"jsonrpc": "2.0", "id": len(self.log) + 1, "method": method, "params": list(params)}).encode()
        rec = {"n": len(self.log) + 1, "method": method, "params": list(params)}
        for attempt in range(7):
            time.sleep(self.pace if attempt == 0 else min(2.0 * attempt, 10.0))
            req = urllib.request.Request(self.url, data=body, headers={"Content-Type": "application/json",
                                                                       "User-Agent": USER_AGENT})
            try:
                with urllib.request.urlopen(req, timeout=60) as r:
                    out = json.load(r)
            except urllib.error.HTTPError as e:  # some endpoints answer a JSON-RPC error with an HTTP error status
                try:
                    out = json.load(e)
                except (ValueError, OSError):
                    out = None
                if not isinstance(out, dict) or "error" not in out:
                    err = f"HTTP {e.code}"
                    continue
            except (urllib.error.URLError, TimeoutError, ConnectionError, json.JSONDecodeError) as e:
                err = f"{type(e).__name__}: {e}"
                continue
            if "error" in out:
                e = out["error"] or {}
                msg, data = str(e.get("message", "")), e.get("data")
                if isinstance(data, dict):  # some clients nest it
                    data = data.get("data") or data.get("result")
                if method == "eth_call" and (e.get("code") == 3 or "revert" in (msg + " " + str(data or "")).lower()):
                    rec["revert"] = {"code": e.get("code"), "message": msg, "data": data}
                    self.log.append(rec)
                    raise Reverted(msg, data if isinstance(data, str) and data.startswith("0x") else None)
                err = f"{method}: {e}"  # rate limit, missing state, ...: retried, never taken for a revert
                continue
            rec["result"] = out.get("result")
            self.log.append(rec)
            return out.get("result")
        rec["endpoint_error"] = err
        self.log.append(rec)
        raise EndpointError(err)

    def at(self) -> str:
        return hex(self.block)

    def call(self, to: str, data: bytes) -> bytes:
        return unhex(self("eth_call", {"to": to, "data": hx(data)}, self.at()))

    def code(self, addr: str) -> bytes:
        return unhex(self("eth_getCode", addr, self.at()))


def enc_verify(seal: bytes, image: bytes, digest: bytes) -> bytes:
    """IRiscZeroVerifier.verify(bytes seal, bytes32 imageId, bytes32 journalDigest)."""
    pad = (32 - len(seal) % 32) % 32
    return (sel("verify(bytes,bytes32,bytes32)") + (0x60).to_bytes(32, "big") + image + digest
            + len(seal).to_bytes(32, "big") + seal + b"\x00" * pad)


def addr_word(w: bytes) -> str:
    return "0x" + w[12:32].hex()


def abi_string(ret: bytes) -> str:
    off = int.from_bytes(ret[:32], "big")
    n = int.from_bytes(ret[off:off + 32], "big")
    return ret[off + 32:off + 32 + n].decode()


def flip_last(b: bytes) -> bytes:
    return b[:-1] + bytes([b[-1] ^ 0x01])


# ---------------------------------------------------------------------------------------------------------------
# The records: RISC Zero's deployment (verbatim upstream copies) and every seal the tree records.
# ---------------------------------------------------------------------------------------------------------------
def deployment(root: str) -> dict:
    with open(os.path.join(root, DEPLOYMENT_TOML), "rb") as f:
        raw = f.read()
    chain = tomllib.loads(raw.decode())["chains"]["ethereum-mainnet"]
    if chain["id"] != CHAIN_ID:
        raise SystemExit(f"{DEPLOYMENT_TOML}: ethereum-mainnet id {chain['id']}")
    groth = [v for v in chain["verifiers"] if v["name"] == "RiscZeroGroth16Verifier"]
    mine = [v for v in groth if v["version"] == VERSION]
    if len(mine) != 1:
        raise SystemExit(f"{DEPLOYMENT_TOML}: {len(mine)} RiscZeroGroth16Verifier entries of version {VERSION}")
    lines = raw.decode().split("\n")

    def line_of(value: str) -> int:
        return next(i + 1 for i, s in enumerate(lines) if value in s)

    src = open(os.path.join(root, CONTROL_ID_SOL)).read()
    cr = re.search(r'CONTROL_ROOT = hex"([0-9a-f]{64})"', src).group(1)
    bn = re.search(r'BN254_CONTROL_ID = hex"([0-9a-f]{64})"', src).group(1)
    return {"sha256": hashlib.sha256(raw).hexdigest(), "chain": chain, "groth16": groth, "v3": mine[0],
            "line": line_of, "control_root": bytes.fromhex(cr), "bn254_control_id": bytes.fromhex(bn),
            "control_id_sha256": hashlib.sha256(src.encode()).hexdigest()}


def tracked_files(root: str) -> list:
    try:
        out = subprocess.run(["git", "-C", root, "ls-files", "-z"], capture_output=True, check=True).stdout
        files = [f for f in out.decode().split("\0") if f]
        if files:
            return files
    except (OSError, subprocess.CalledProcessError):
        pass
    skip = {".git", "target", "node_modules", "out", "cache", "lib"}
    files = []
    for d, dirs, names in os.walk(root):
        dirs[:] = sorted(x for x in dirs if x not in skip)
        files += [os.path.relpath(os.path.join(d, n), root) for n in sorted(names)]
    return files


def load_json(root: str, rel: str):
    p = os.path.join(root, rel)
    if not os.path.isfile(p):
        return None
    try:
        with open(p) as f:
            return json.load(f)
    except (ValueError, UnicodeDecodeError):
        return None


def read_hex(root: str, rel: str) -> bytes | None:
    p = os.path.join(root, rel)
    return unhex(open(p).read()) if os.path.isfile(p) else None


class Catalog:
    """Candidates (seal, image id, journal) from the records, each with where every part came from."""

    def __init__(self, root: str, selector: bytes):
        self.root, self.selector, self.items, self.skipped, self.notes = root, selector, [], [], []
        self.files = tracked_files(root)
        self.scan_re = re.compile(r"(?<![0-9a-fA-F])(?:0x)?(" + selector.hex() + r"[0-9a-fA-F]{512})(?![0-9a-fA-F])")

    def add(self, seal, image, journal, src: str, label: str):
        if isinstance(seal, str):
            seal = unhex(seal)
        if isinstance(image, str):
            image = unhex(image)
        if isinstance(journal, str):
            journal = unhex(journal)
        if seal is None:
            return
        if len(seal) != 260 or not seal.startswith(self.selector):  # 4-byte selector + a, b, c of the Groth16 proof
            self.skipped.append({"label": label, "head": hx(seal[:4]), "bytes": len(seal), "src": src})
            return
        self.items.append({"seal": seal, "image": image, "journal": journal, "src": src, "label": label})

    def collect(self) -> None:
        root, files = self.root, self.files
        # 1. Crown runs: crown.json's candidate and releases; image ids from params.json (the mode deploy.json names);
        #    a release's journal from releases/release-<name>.json.
        for f in files:
            if not re.fullmatch(r"e2e/runs/[^/]+/crown\.json", f):
                continue
            d = os.path.dirname(f)
            crown, params, dep = load_json(root, f), load_json(root, d + "/params.json") or {}, \
                load_json(root, d + "/deploy.json") or {}
            mode = dep.get("mode") or "B"
            cand = crown.get("candidate") or {}
            if cand.get("seal"):
                key = "checkpointImageId" + mode
                if key in params or not unhex(cand["seal"]).startswith(self.selector):
                    self.add(cand["seal"], params.get(key), cand.get("journal"),
                             f"{f} candidate.seal/journal; {d}/params.json {key} (deploy.json mode {mode})",
                             f"{d} checkpoint")
            for i, r in enumerate(crown.get("releases") or []):
                name = r.get("name")
                meta = load_json(root, f"{d}/releases/release-{name}.json") if name else None
                if r.get("seal") and "releaseImageId" in params:
                    self.add(r["seal"], params["releaseImageId"], (meta or {}).get("journal"),
                             f"{f} releases[{name or i}].seal; {d}/params.json releaseImageId; "
                             + (f"{d}/releases/release-{name}.json journal" if meta else "no journal record"),
                             f"{d} release {name or i}")
        # 2. Receipt directories: seal.hex beside journal.hex; the image id from the run's own record.
        for f in files:
            if not f.endswith("/seal.hex"):
                continue
            d = os.path.dirname(f)
            p, base = os.path.dirname(d), os.path.basename(d)
            image, isrc = None, None
            for j in (d + "/groth16.json", d + "/prove.json", d + "/run.json", p + "/prove.json", p + "/run.json"):
                v = load_json(root, j)
                if isinstance(v, dict) and v.get("image_id"):
                    image, isrc = v["image_id"], j + " image_id"
                    break
            scen = load_json(root, p + "/scenario.json")
            if image is None and isinstance(scen, dict) and any(c.get("case") == base for c in scen.get("cases", [])):
                image, isrc = scen["release_image_id"], p + "/scenario.json release_image_id (case " + base + ")"
            readme = os.path.join(root, p, "README.md")
            if image is None and os.path.isfile(readme):
                m = re.search(r"checkpoint image\s*`(0x[0-9a-f]{64})`", open(readme).read())
                if m:
                    image, isrc = m.group(1), p + "/README.md (\"checkpoint image\")"
            proofs = load_json(root, os.path.dirname(p) + "/proofs.json")
            if image is None and base.startswith("release-") and isinstance(proofs, dict) and proofs.get("releaseImageId"):
                image, isrc = proofs["releaseImageId"], os.path.dirname(p) + "/proofs.json releaseImageId"
            g = load_json(root, d + "/groth16.json") or {}
            journal = read_hex(root, d + "/journal.hex")
            if journal is not None and g.get("journal_sha256"):
                if hashlib.sha256(journal).hexdigest() != g["journal_sha256"].lower().removeprefix("0x"):
                    self.notes.append(f"{d}: sha256(journal.hex) differs from groth16.json journal_sha256")
            if image:
                self.add(read_hex(root, f), image, journal,
                         f"{f}; {isrc}; " + (d + "/journal.hex" if journal is not None else "no journal record"), d)
        # 3. JSON records that carry seal, journal and image id together (the vault's real fixtures, the bridge's
        #    fixtures, the fork runs' fixture.json, proofs.json).
        for f in files:
            if not f.endswith(".json") or f.endswith("/crown.json"):
                continue
            v = load_json(root, f)
            if not isinstance(v, dict):
                continue
            ck = next((k for k in ("imageId", "checkpointImageId", "checkpointImageIdB") if v.get(k)), None)
            k = "imageId" if v.get("imageId") else "releaseImageId" if v.get("releaseImageId") and not ck else None
            if v.get("seal") and v.get("journal") and k:
                self.add(v["seal"], v[k], v["journal"], f"{f} seal/journal/{k}", f)
            if v.get("releaseSeal") and v.get("releaseJournal") and v.get("releaseImageId"):
                self.add(v["releaseSeal"], v["releaseImageId"], v["releaseJournal"],
                         f"{f} releaseSeal/releaseJournal/releaseImageId", f + " release")
            for part in ("candidate", "finalize"):
                c = v.get(part)
                if isinstance(c, dict) and c.get("seal") and c.get("journal") and ck and ck != "imageId":
                    self.add(c["seal"], v[ck], c["journal"], f"{f} {part}.seal/journal; {ck}", f"{f} {part}")
            rels = v.get("releases")
            if isinstance(rels, list) and v.get("releaseImageId"):
                for i, r in enumerate(rels):
                    if isinstance(r, dict) and r.get("seal"):
                        self.add(r["seal"], v["releaseImageId"], r.get("journal"),
                                 f"{f} releases[{i}].seal" + ("/journal" if r.get("journal") else " (no journal)")
                                 + "; releaseImageId", f"{f} release {r.get('name', i)}")
        # 4. The privacy suite's release proofs: seal and image id, no journal committed.
        v = load_json(root, "docs/privacy/real/proofs.json")
        if isinstance(v, dict):
            for i, r in enumerate(v.get("releases") or []):
                if r.get("seal") and (r.get("succinct") or {}).get("image_id"):
                    self.add(r["seal"], r["succinct"]["image_id"], None,
                             f"docs/privacy/real/proofs.json releases[{i}].seal, succinct.image_id (no journal)",
                             f"docs/privacy/real {r.get('name', i)}")

    def scan(self) -> dict:
        """Every distinct v3.0.0 seal in the tree's text files, with the files it appears in."""
        seen = {}
        for f in self.files:
            p = os.path.join(self.root, f)
            try:
                if os.path.getsize(p) > 20_000_000:
                    continue
                t = open(p, encoding="utf-8").read()
            except (OSError, UnicodeDecodeError):
                continue
            for m in self.scan_re.finditer(t):
                seen.setdefault(m.group(1).lower(), []).append(f)
        return {k: sorted(set(v)) for k, v in seen.items()}

    def build(self):
        self.collect()
        found = self.scan()
        by_seal = {}
        for it in self.items:
            by_seal.setdefault(it["seal"], []).append(it)
        entries, nojournal = [], []
        for seal, its in by_seal.items():
            pairs = {}
            for it in its:
                if it["image"] is None or it["journal"] is None:
                    continue
                key = (it["image"], hashlib.sha256(it["journal"]).digest())
                pairs.setdefault(key, {"journal": it["journal"], "src": [], "labels": []})
                pairs[key]["src"].append(it["src"])
                pairs[key]["labels"].append(it["label"])
            if not pairs:
                nojournal.append({"seal": seal, "src": sorted({it["src"] for it in its}),
                                  "label": its[0]["label"], "image": its[0]["image"]})
            for (image, digest), p in pairs.items():
                entries.append({"seal": seal, "image": image, "digest": digest, "journal": p["journal"],
                                "src": sorted(set(p["src"])), "label": order_label(p["labels"]),
                                "files": found.get(seal.hex(), [])})
        unmapped = {s: fs for s, fs in found.items() if bytes.fromhex(s) not in by_seal}
        entries.sort(key=lambda e: (priority(e["label"]), e["label"]))
        nojournal.sort(key=lambda e: e["label"])
        skipped = {(x["label"], x["head"]): x for x in self.skipped if x["head"] != hx(self.selector)
                   or x["bytes"] != 260}
        return entries, nojournal, unmapped, found, sorted(skipped.values(), key=lambda x: x["label"]), self.notes


FIRST = ["e2e/runs/crown-live-3", "e2e/runs/zone-bridge-rc3", "e2e/runs/testnet-return-fork-2", "bridge/artifacts"]


def priority(label: str) -> int:
    return next((i for i, p in enumerate(FIRST) if label.startswith(p)), len(FIRST))


def order_label(labels: list) -> str:
    return sorted(set(labels), key=lambda s: (priority(s), not s.startswith("e2e/runs/"), len(s), s))[0]


# ---------------------------------------------------------------------------------------------------------------
def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--rpc", default=DEFAULT_RPC, help=f"an Ethereum mainnet JSON-RPC endpoint (default {DEFAULT_RPC})")
    ap.add_argument("--block", type=int, help="the block every call is made at (default: the endpoint's latest)")
    ap.add_argument("--out", help=f"write verify-TAG.txt, calls-TAG.jsonl and summary-TAG.json here (e.g. {RESULTS})")
    ap.add_argument("--tag", help="the files' name (default: the endpoint's host)")
    ap.add_argument("--pace", type=float, default=0.05, help="seconds between requests (default 0.05)")
    ap.add_argument("--list", action="store_true", help="list the seals and their records; send nothing")
    a = ap.parse_args()

    dep = deployment(ROOT)
    v3, chain = dep["v3"], dep["chain"]
    selector = unhex(v3["selector"])
    entries, nojournal, unmapped, found, skipped, notes = Catalog(ROOT, selector).build()
    lines = []

    def out(s: str) -> None:
        print(s, flush=True)
        lines.append(s)

    if a.list:
        for e in entries:
            out(f"{e['label']}\n    seal {hx(e['seal'][:12])}…  image {hx(e['image'])}  journal sha256 {hx(e['digest'])}"
                + "".join(f"\n    from {s}" for s in e["src"]))
        for e in nojournal:
            out(f"NOJOURNAL {e['label']}: " + "; ".join(e["src"]))
        for x in skipped:
            out(f"SKIP {x['label']}: seal starts {x['head']}, {x['bytes']} bytes: not a Groth16 {VERSION} seal ({x['src']})")
        for n in notes:
            out(f"NOTE {n}")
        out(f"{len(found)} distinct seals with selector {v3['selector']} in the tree; {len(entries)} (seal, image id, journal)"
            f" to check; {len(nojournal)} without a journal; {len(unmapped)} not mapped to a record")
        return 0

    host = urllib.parse.urlparse(a.rpc).hostname or "rpc"
    tag = a.tag or host
    rpc = Rpc(a.rpc, None, a.pace)
    fails, passes, summary = [], 0, {"endpoint": a.rpc, "seals": []}
    try:
        cid = int(rpc("eth_chainId"), 16)
        if cid != CHAIN_ID:
            out(f"FAIL  {a.rpc} answers chain id {cid}, not {CHAIN_ID} (Ethereum mainnet)")
            return 2
        latest = int(rpc("eth_blockNumber"), 16)
        rpc.block = a.block if a.block is not None else latest
        started = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        summary.update(chainId=cid, block=rpc.block, latestAtStart=latest, startedUtc=started)
        out(f"endpoint {a.rpc} at {started}: chain id {cid}; every call at block {rpc.block} (the endpoint's latest at "
            f"the start: {latest}); methods sent: eth_chainId, eth_blockNumber, eth_getCode, eth_call only")

        def check(ok: bool, text: str) -> None:
            nonlocal passes
            out(("PASS  " if ok else "FAIL  ") + text)
            if ok:
                passes += 1
            else:
                fails.append(text)

        L = dep["line"]
        router, estop, verifier = chain["router"], v3["estop"], v3["verifier"]
        out(f"record: {DEPLOYMENT_TOML} (risc0-ethereum v3.0.0 contracts/deployment.toml, sha256 {dep['sha256']}): "
            f"router {router} (line {L(router)}), RiscZeroGroth16Verifier {VERSION} selector {v3['selector']} verifier "
            f"{verifier} estop {estop} (lines {L(v3['selector'])}-{L(estop)})")
        # Code at every address the record names.
        for name, ad in (("router", router), ("v3.0.0 emergency stop", estop), ("v3.0.0 Groth16 verifier", verifier),
                         ("timelock controller", chain["timelock-controller"])):
            c = rpc.code(ad)
            check(len(c) > 0, f"code at the {name} {ad}: {len(c)} bytes, keccak256 {hx(keccak256(c))}")
            summary.setdefault("code", {})[ad] = {"bytes": len(c), "keccak256": hx(keccak256(c))}
        # The router's table for every Groth16 version the record lists, and each emergency stop.
        table = []
        for g in dep["groth16"]:
            try:
                got = addr_word(rpc.call(router, sel("getVerifier(bytes4)") + unhex(g["selector"]) + bytes(28)))
            except Reverted as r:
                got = "reverts " + r.name()
            paused = None
            if got.lower() == g["estop"].lower():
                paused = int.from_bytes(rpc.call(g["estop"], sel("paused()")), "big") == 1
            want_paused = bool(g.get("stopped", False))
            ok = got.lower() == g["estop"].lower() and paused == want_paused
            table.append({"version": g["version"], "selector": g["selector"], "estop": g["estop"],
                          "getVerifier": got, "paused": paused, "stoppedInRecord": want_paused, "asRecorded": ok})
            text = (f"router.getVerifier({g['selector']}) (Groth16 {g['version']}, record lines "
                    f"{L(g['selector'])}-{L(g['estop'])}: estop {g['estop']}, stopped = {want_paused}): {got}"
                    + (f", paused() = {paused}" if paused is not None else ""))
            if g is v3:  # the version our seals carry: a check; the others are context (the router may remove them)
                check(ok, text)
            else:
                out(("INFO  " if ok else "INFO  differs from the v3.0.0 record: ") + text)
        summary["routerTable"] = table
        owner = addr_word(rpc.call(router, sel("owner()")))
        check(owner.lower() == chain["timelock-controller"].lower(),
              f"router.owner() = {owner} = the record's timelock-controller (line {L(chain['timelock-controller'])}, "
              f"delay {chain['timelock-delay']} s)")
        ev = addr_word(rpc.call(estop, sel("verifier()")))
        eo = addr_word(rpc.call(estop, sel("owner()")))
        check(ev.lower() == verifier.lower() and eo.lower() == chain["admin"].lower(),
              f"estop {estop}: verifier() = {ev} (the v3.0.0 Groth16 verifier), owner() = {eo} (the record's admin, "
              f"line {L(chain['admin'])})")
        s_ = rpc.call(verifier, sel("SELECTOR()"))[:4]
        ver = abi_string(rpc.call(verifier, sel("VERSION()")))
        c0 = rpc.call(verifier, sel("CONTROL_ROOT_0()"))[:16]
        c1 = rpc.call(verifier, sel("CONTROL_ROOT_1()"))[:16]
        bn = rpc.call(verifier, sel("BN254_CONTROL_ID()"))
        rev = int.from_bytes(dep["control_root"], "little").to_bytes(32, "big")  # splitDigest: reverse, then halves
        want0, want1 = rev[16:], rev[:16]
        check(s_ == selector and ver == VERSION and c0 == want0 and c1 == want1 and bn == dep["bn254_control_id"],
              f"verifier {verifier}: SELECTOR() = {hx(s_)}, VERSION() = \"{ver}\", CONTROL_ROOT_0/1 = {hx(c0)}/{hx(c1)} "
              f"(= v3.0.0's ControlID.CONTROL_ROOT split as the constructor does), BN254_CONTROL_ID = {hx(bn)} "
              f"({CONTROL_ID_SOL}, sha256 {dep['control_id_sha256']})")
        summary["verifierGetters"] = {"SELECTOR": hx(s_), "VERSION": ver, "CONTROL_ROOT_0": hx(c0),
                                      "CONTROL_ROOT_1": hx(c1), "BN254_CONTROL_ID": hx(bn)}

        # Every recorded seal.
        def outcome(to: str, data: bytes) -> str:
            try:
                r = rpc.call(to, data)
                return "returns" if r == b"" else "returns 0x" + r.hex()
            except Reverted as e:
                return "reverts " + e.name()

        for e in entries:
            seal, image, digest, journal = e["seal"], e["image"], e["digest"], e["journal"]
            res = {"router": outcome(router, enc_verify(seal, image, digest)),
                   "verifier": outcome(verifier, enc_verify(seal, image, digest)),
                   "seal_changed": outcome(router, enc_verify(flip_last(seal), image, digest)),
                   "image_changed": outcome(router, enc_verify(seal, flip_last(image), digest)),
                   "journal_changed": outcome(router, enc_verify(seal, image, hashlib.sha256(flip_last(journal)).digest()))}
            ok = (res["router"] == res["verifier"] == "returns"
                  and all(res[k].startswith("reverts") for k in ("seal_changed", "image_changed", "journal_changed")))
            summary["seals"].append({"label": e["label"], "sealHead": hx(seal[:12]),
                                     "sealSha256": hx(hashlib.sha256(seal).digest()), "imageId": hx(image),
                                     "journalSha256": hx(digest), "journalBytes": len(journal), "records": e["src"],
                                     "filesWithThisSeal": len(e["files"]), **res, "pass": ok})
            check(ok, f"{e['label']}: seal {hx(seal[:10])}…, image {hx(image)}, journal sha256 {hx(digest)} "
                      f"({len(journal)} B): router {res['router']}, verifier {res['verifier']}; seal changed: "
                      f"{res['seal_changed']}; image id changed: {res['image_changed']}; journal changed: "
                      f"{res['journal_changed']}")
        for e in nojournal:
            out(f"NOJOURNAL  {e['label']}: seal {hx(e['seal'][:10])}…, image {hx(e['image']) if e['image'] else '?'}: "
                f"recorded without its journal ({'; '.join(e['src'])})")
        summary["noJournal"] = [{"label": e["label"], "sealHead": hx(e["seal"][:12]), "records": e["src"]}
                                for e in nojournal]
        for x in skipped:
            out(f"SKIP  {x['label']}: the recorded seal starts {x['head']} ({x['bytes']} bytes), not a Groth16 {VERSION} "
                f"seal ({x['src']})")
        summary["skipped"] = skipped
        for n in notes:
            check(False, n)
        for s, fs in unmapped.items():
            check(False, f"seal 0x{s[:20]}… in {', '.join(fs)} is not mapped to an image id by any record rule")
        out(f"summary: block {rpc.block}; {len(found)} distinct Groth16 {VERSION} seals in the tree; "
            f"{sum(1 for x in summary['seals'] if x['pass'])} of {len(entries)} (seal, image id, journal) verify through "
            f"RISC Zero's mainnet router and verifier with every changed input refused; {len(nojournal)} recorded without "
            f"a journal; {passes} PASS, {len(fails)} FAIL; {len(rpc.log)} JSON-RPC requests")
    except EndpointError as e:
        out(f"FAIL  the endpoint stopped answering: {e}")
        fails.append("endpoint")
        return 2
    finally:
        summary.update(passes=passes, fails=len(fails), requests=len(rpc.log))
        if a.out:
            os.makedirs(a.out, exist_ok=True)
            with open(os.path.join(a.out, f"verify-{tag}.txt"), "w") as f:
                f.write("\n".join(lines) + "\n")
            with open(os.path.join(a.out, f"calls-{tag}.jsonl"), "w") as f:
                for r in rpc.log:
                    f.write(json.dumps(r, separators=(",", ":")) + "\n")
            with open(os.path.join(a.out, f"summary-{tag}.json"), "w") as f:
                json.dump(summary, f, indent=1)
                f.write("\n")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
