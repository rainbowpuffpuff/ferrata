#!/usr/bin/env python3
"""scripts/verify.py under test: its registry is honest, its command line behaves, and every check family fails
when its evidence is altered. Python 3.11+ standard library only, offline (the online run-record check runs against
a stand-in RPC on 127.0.0.1 that serves recorded blocks of our test network).

    python3 -I scripts/test_verify.py [-v] [-k PATTERN]

Each tamper test runs the real check (verifylib.core.run_check) on a copy of the repository in which one piece of
evidence is altered: the copy links every untouched path to this checkout and copies only what it changes, under
$RUNNER_TEMP or .scratch/ (never /tmp unless TMPDIR says so).
"""
from __future__ import annotations

import base64
import contextlib
import http.server
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import catalog, lez, runrecord  # noqa: E402
from verifylib.core import REGISTRY, Ctx, run_check  # noqa: E402
from verifylib.rows import status_rows  # noqa: E402

ROWS = catalog.load(ROOT)
SCRATCH = Path(os.environ.get("RUNNER_TEMP") or ROOT / ".scratch")


def overlay(edits: dict, copy=("scripts",)) -> Path:
    """A copy of the repository: each top-level entry a symlink to this checkout, except the directories on the way
    to an edited file (real directories of symlinks) and `copy` (copied whole: scripts that find the repository from
    their own path then find the copy). `edits` maps a path to a function of its bytes, or None to delete it."""
    SCRATCH.mkdir(parents=True, exist_ok=True)
    tmp = Path(tempfile.mkdtemp(prefix="test-verify-", dir=SCRATCH))

    def materialise(rel: Path) -> Path:
        """Make tmp/rel a real directory whose entries link to ROOT/rel's (recursively along `rel`)."""
        cur_t, cur_r = tmp, ROOT
        for part in rel.parts:
            cur_t, cur_r = cur_t / part, cur_r / part
            if cur_t.is_symlink():
                cur_t.unlink()
            if not cur_t.exists():
                cur_t.mkdir()
                for e in cur_r.iterdir():
                    (cur_t / e.name).symlink_to(e)
        return cur_t

    for e in ROOT.iterdir():
        if e.name in (".git", ".scratch"):
            continue
        (tmp / e.name).symlink_to(e)
    (tmp / ".scratch").mkdir()
    for c in copy:
        materialise(Path(c).parent) if Path(c).parent != Path(".") else None
        t = tmp / c
        if t.is_symlink():
            t.unlink()
        shutil.copytree(ROOT / c, t, symlinks=False, ignore=shutil.ignore_patterns("__pycache__"))
    for rel, fn in edits.items():
        rel = Path(rel)
        d = materialise(rel.parent) if rel.parent != Path(".") else tmp
        f = d / rel.name
        data = (ROOT / rel).read_bytes()
        if f.is_symlink():
            f.unlink()
        if fn is not None:
            f.write_bytes(fn(data))
    return tmp


def sub(old: str, new: str, count: int = 1):
    """An edit replacing the first `count` occurrences of `old`, which must be there."""
    def fn(b: bytes) -> bytes:
        s = b.decode()
        if old not in s:
            raise AssertionError(f"{old!r} not in the file to alter")
        return s.replace(old, new, count).encode()
    return fn


def run(cid: str, root: Path, **kw):
    ctx = Ctx(root=root, cache=root / ".scratch" / "cache", git_dir=ROOT if root != ROOT else None, **kw)
    return run_check(REGISTRY[cid], ctx)


class TamperCase(unittest.TestCase):
    def assertPasses(self, cid, root=ROOT, **kw):
        r = run(cid, root, **kw)
        self.assertEqual(r.status, "PASS", f"{cid} on {root}: {r.status}\n" + "\n".join(r.lines))
        return r

    def assertFails(self, cid, root, contains=None, **kw):
        r = run(cid, root, **kw)
        self.assertEqual(r.status, "FAIL", f"{cid} should fail on the altered copy: {r.status}\n" + "\n".join(r.lines))
        if contains:
            self.assertTrue(any(contains in ln for ln in r.lines), f"{cid}: no line names {contains!r}:\n" +
                            "\n".join(r.lines))
        return r

    def tearDown(self):
        for p in SCRATCH.glob("test-verify-*"):
            shutil.rmtree(p, ignore_errors=True)


# ---- the registry -------------------------------------------------------------------------------------------------

class Registry(unittest.TestCase):
    def test_rows_exist(self):
        keys = {r.key for r in ROWS}
        for c in REGISTRY.values():
            for row in c.rows:
                self.assertIn(row, keys, f"{c.id} names {row}, which docs/requirements-status.md does not have")

    def test_rows_cite_the_evidence(self):
        """A check backs a row only where the row's own Evidence cell cites one of its EVIDENCE rows or files."""
        by_key = {r.key: r for r in ROWS}
        bad = []
        for c in REGISTRY.values():
            if c.family == "test":
                continue  # the row-to-test map is its own source (scripts/ci/row-tests.toml)
            for row in c.rows:
                r = by_key[row]
                if not (set(r.evidence) & set(c.evidence) or any(i in r.evidence_text for i in c.inputs)):
                    bad.append(f"{c.id} -> {row} (check evidence {c.evidence}, row cites {r.evidence})")
        self.assertEqual(bad, [], "checks naming rows that do not cite them:\n" + "\n".join(bad))

    def test_every_check_is_described(self):
        for c in REGISTRY.values():
            self.assertTrue(c.title and c.what and c.against, f"{c.id}: title, what and against")
            self.assertIn(c.mode, ("offline", "online"))

    def test_online_checks_send_nothing(self):
        """The LEZ client refuses anything but reads; the Ethereum reads of checks_online allow four methods."""
        rpc = lez.Rpc("http://127.0.0.1:9/rpc")
        with self.assertRaises(lez.RpcError):
            rpc.call("sendTransaction", ["AA=="])
        from verifylib import checks_online
        with self.assertRaises(ValueError):
            checks_online._eth("http://127.0.0.1:9", "eth_sendRawTransaction", ["0x00"])

    def test_every_shown_row_has_a_check(self):
        cov = catalog.coverage(ROOT, ROWS)
        missing = [k for k, v in cov.items() if v["status"] == "shown" and not v["soft"]
                   and not (v["quick_offline"] or v["online"] or v["tests"])]
        self.assertEqual(missing, [], "shown hard rows with no check")


# ---- the command line ---------------------------------------------------------------------------------------------

def cli(*args, root=ROOT):
    p = subprocess.run([sys.executable, "-I", str(root / "scripts/verify.py"), *args], capture_output=True, text=True,
                       cwd=root, timeout=900)
    return p.returncode, p.stdout + p.stderr


class CommandLine(TamperCase):
    def test_list(self):
        rc, out = cli("--list")
        self.assertEqual(rc, 0)
        self.assertIn("run-record  [run-record, offline, quick]", out)

    def test_row_selector(self):
        for sel in ("RFP-021:F.14", "021/F.14", "F.14@021", "RFP-021 F.14"):
            rc, out = cli(sel, "--list")
            self.assertEqual(rc, 0, out)
            self.assertIn("lab-roots", out, sel)

    def test_ambiguous_and_unknown(self):
        rc, out = cli("F.4")
        self.assertNotEqual(rc, 0)
        self.assertIn("give the RFP", out)
        rc, out = cli("no-such-check")
        self.assertNotEqual(rc, 0)

    def test_exit_status(self):
        rc, out = cli("acc-leaf-vectors")
        self.assertEqual(rc, 0, out)
        t = overlay({"vectors/acc-leaf.json": sub('"', "'", 1)})
        rc, out = cli("acc-leaf-vectors", root=t)
        self.assertEqual(rc, 1, out)
        self.assertIn("FAIL  acc-leaf-vectors", out)

    def test_coverage(self):
        rc, out = cli("--coverage")
        self.assertEqual(rc, 0)
        self.assertIn("RFP-021 hard rows", out)


# ---- tamper tests, family by family ---------------------------------------------------------------------------------

RR = runrecord.RECORD_DIR


class RunRecordOffline(TamperCase):
    def test_unaltered(self):
        self.assertPasses("run-record")

    def test_summary_number(self):
        s = json.loads((ROOT / RR / "summary.json").read_text())
        inst = next(iter(s["instances"]))
        n = s["instances"][inst]["updates_applied"]
        t = overlay({f"{RR}/summary.json": sub(f'"updates_applied": {n}', f'"updates_applied": {n + 1}')})
        self.assertFails("run-record", t, "updates_applied")

    def test_a_time_moved(self):
        rows = (ROOT / RR / "transactions.tsv").read_text().splitlines()
        line = next(ln for ln in rows[1:] if "\tfinish\tapplied\t" in ln)
        t_old = line.split("\t")[1]
        t_new = t_old[:11] + ("23" if t_old[11:13] != "23" else "22") + t_old[13:]
        t = overlay({f"{RR}/transactions.tsv": sub(line, line.replace(t_old, t_new))})
        self.assertFails("run-record", t)

    def test_a_finish_removed(self):
        rows = (ROOT / RR / "transactions.tsv").read_text().splitlines()
        line = next(ln for ln in rows[1:] if "\tfinish\tapplied\t" in ln)
        t = overlay({f"{RR}/transactions.tsv": sub(line + "\n", "")})
        self.assertFails("run-record", t)

    def test_record_md(self):
        t = overlay({f"{RR}/RECORD.md": sub("0 over 3 h", "1 over 3 h")})
        self.assertFails("run-record", t, "RECORD.md")

    def test_an_interval_hidden(self):
        """Dropping an update shortens nothing silently: the intervals and counts recomputed differ."""
        rows = (ROOT / RR / "transactions.tsv").read_text().splitlines()
        fin = [ln for ln in rows[1:] if "\tfinish\tapplied\t" in ln]
        t = overlay({f"{RR}/transactions.tsv": sub(fin[len(fin) // 2] + "\n", "")})
        self.assertFails("run-record", t, "summary.json")


class StandInRpc:
    """A JSON-RPC stand-in on 127.0.0.1 serving recorded blocks: getLastBlockId, getBlockRange, getBlock."""

    def __init__(self, blocks: dict, files: dict | None = None):
        self.blocks = blocks
        self.files = files or {}  # GET path -> JSON
        self.txs = {}
        for b64 in blocks.values():
            for tx in lez.decode_block(base64.b64decode(b64))["transactions"]:
                self.txs[tx["hash"].hex()] = base64.b64encode(tx["raw"]).decode()
        outer = self

        class H(http.server.BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def do_GET(self):
                body = json.dumps(outer.files.get(self.path)).encode()
                self.send_response(200 if self.path in outer.files else 404)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):
                req = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                m, p = req["method"], req.get("params", [])
                if m == "getLastBlockId":
                    res = max(outer.blocks)
                elif m == "getBlockRange":
                    res = [outer.blocks[i] for i in range(p[0], p[1] + 1) if i in outer.blocks]
                elif m == "getBlock":
                    res = outer.blocks.get(p[0])
                elif m == "getTransaction":
                    res = outer.txs.get(p[0])
                else:
                    res = None
                body = json.dumps({"jsonrpc": "2.0", "id": req.get("id"), "result": res}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self):
        self.server.shutdown()


FIXTURE = ROOT / "scripts/verifylib/testdata/lez-rc3-blocks-25190-25260.json"


class RunRecordOnline(TamperCase):
    """run-record-online against a stand-in RPC serving 71 recorded blocks of the test network, on a record of
    those blocks made by the same code the maintainer's build uses."""

    def setUp(self):
        fx = json.loads(FIXTURE.read_text())
        self.blocks = {fx["first"] + i: b for i, b in enumerate(fx["blocks"])}
        chain = json.loads((ROOT / RR / "chain.json").read_text())
        program = lez.b58decode(chain["program_account"])
        lines, ch, probs = runrecord.scan((base64.b64decode(self.blocks[i]) for i in sorted(self.blocks)), program,
                                          runrecord.lc.Idl.load(ROOT / "idl/ferrata_attestation.idl.json"),
                                          first=fx["first"], instances=chain["instances"])
        self.assertEqual(probs, [])
        self.assertGreaterEqual(sum(1 for ln in lines if ln["step"] == "finish"), 2)
        ch.update(program_account=chain["program_account"], first_block=fx["first"])
        self.base = Path(tempfile.mkdtemp(prefix="test-verify-rr-", dir=SCRATCH))
        runrecord.write_tsv(lines, self.base / "transactions.tsv")
        (self.base / "chain.json").write_text(json.dumps(ch, indent=1, sort_keys=True) + "\n")
        self.rpc = StandInRpc(dict(self.blocks))

    def tearDown(self):
        self.rpc.close()
        shutil.rmtree(self.base, ignore_errors=True)
        super().tearDown()

    def copy(self, tsv=None, chain=None):
        edits = {}
        for name, fn in (("transactions.tsv", tsv), ("chain.json", chain)):
            data = (self.base / name).read_bytes()
            edits[f"{RR}/{name}"] = (lambda _d, _fn=fn, _data=data: (_fn(_data) if _fn else _data))
        return overlay(edits)

    def online(self, root):
        return run("run-record-online", root, lez_url=self.rpc.url)

    def test_unaltered(self):
        r = self.online(self.copy())
        self.assertEqual(r.status, "PASS", "\n".join(r.lines))

    def _first(self, step):
        return next(ln for ln in (self.base / "transactions.tsv").read_text().splitlines()[1:]
                    if f"\t{step}\t" in ln)

    def test_tx_hash(self):
        line = self._first("finish")
        tx = line.split("\t")[4]
        bad = tx[:-1] + ("0" if tx[-1] != "0" else "1")
        r = self.online(self.copy(tsv=sub(line, line.replace(tx, bad))))
        self.assertEqual(r.status, "FAIL")
        self.assertTrue(any("not in the chain" in ln for ln in r.lines), r.lines)

    def test_a_field(self):
        line = self._first("open")
        cols = line.split("\t")
        i = runrecord.COLUMNS.index("participants")
        cols[i] = str(int(cols[i]) + 1)
        r = self.online(self.copy(tsv=sub(line, "\t".join(cols))))
        self.assertEqual(r.status, "FAIL")
        self.assertTrue(any("differs in participants" in ln for ln in r.lines), r.lines)

    def test_a_line_missing(self):
        line = self._first("miller")
        r = self.online(self.copy(tsv=sub(line + "\n", "")))
        self.assertEqual(r.status, "FAIL")
        self.assertTrue(any("missing from the record" in ln for ln in r.lines), r.lines)

    def test_as_of_hash(self):
        ch = json.loads((self.base / "chain.json").read_text())
        h = ch["as_of"]["hash"]
        r = self.online(self.copy(chain=sub(h, h[::-1])))
        self.assertEqual(r.status, "FAIL")

    def test_a_block_byte(self):
        """The stand-in serves a block with one byte of a transaction changed: its hash no longer holds."""
        bid = int(self._first("open").split("\t")[0])
        raw = bytearray(base64.b64decode(self.rpc.blocks[bid]))
        raw[len(raw) // 2] ^= 1
        self.rpc.blocks[bid] = base64.b64encode(bytes(raw)).decode()
        r = self.online(self.copy())
        self.assertEqual(r.status, "FAIL", "\n".join(r.lines))


class LezRead(unittest.TestCase):
    """scripts/lez-read.py, the explorer's reads from a terminal, against the stand-in RPC and recorded blocks."""

    def setUp(self):
        fx = json.loads(FIXTURE.read_text())
        self.blocks = {fx["first"] + i: b for i, b in enumerate(fx["blocks"])}
        chain = json.loads((ROOT / RR / "chain.json").read_text())
        progs = {"programs": {"ferrata_attestation": {"program_account": chain["program_account"],
                                                      "image_id": "0x" + runrecord.ATTESTATION_IMAGE}}}
        self.rpc = StandInRpc(dict(self.blocks), {"/programs.json": progs})
        lines, _, _ = runrecord.scan((base64.b64decode(self.blocks[i]) for i in sorted(self.blocks)),
                                     lez.b58decode(chain["program_account"]),
                                     runrecord.lc.Idl.load(ROOT / "idl/ferrata_attestation.idl.json"),
                                     first=fx["first"], instances=chain["instances"])
        self.open = next(ln for ln in lines if ln["step"] == "open")

    def tearDown(self):
        self.rpc.close()

    def read(self, *args):
        p = subprocess.run([sys.executable, "-I", str(ROOT / "scripts/lez-read.py"), "--lez", self.rpc.url, "--json",
                            *args], capture_output=True, text=True, timeout=120)
        return p.returncode, (json.loads(p.stdout) if p.stdout.strip() else None), p.stderr

    def test_block_with_an_update(self):
        rc, out, err = self.read("block", str(self.open["block"]))
        self.assertEqual(rc, 0, err)
        self.assertTrue(out["hash_ok"])
        call = next(t["call"] for t in out["transactions"] if t["call"]["program_name"] == "ferrata_attestation")
        self.assertEqual(call["instruction"], "open")
        self.assertEqual(call["update"]["finalized_slot"], self.open["finalized_slot"])
        self.assertEqual(call["update"]["participants"], self.open["participants"])

    def test_transaction(self):
        rc, out, err = self.read("tx", self.open["tx"])
        self.assertEqual(rc, 0, err)
        self.assertTrue(out["hash_ok"])
        self.assertEqual(out["signers"], [self.open["sender"]])

    def test_a_block_that_does_not_hash(self):
        bid = self.open["block"]
        raw = bytearray(base64.b64decode(self.rpc.blocks[bid]))
        raw[85] ^= 1  # a byte of the body: the block's hash no longer covers it
        self.rpc.blocks[bid] = base64.b64encode(bytes(raw)).decode()
        rc, out, err = self.read("block", str(bid))
        self.assertEqual(rc, 1, (out, err))


class Records(TamperCase):
    def test_perf_tracking(self):
        self.assertPasses("perf-tracking")
        text = (ROOT / "lez/results/perf-tracking.md").read_text()
        num = re.search(r"\d{1,3},\d{3},\d{3}", text).group(0)
        t = overlay({"lez/results/perf-tracking.md": sub(num, num[:-1] + str((int(num[-1]) + 1) % 10))})
        self.assertFails("perf-tracking", t)

    def test_lab_roots(self):
        self.assertPasses("lab-roots")
        f = next((ROOT / "lez/lab/results/sessions").glob("*/fetch-sepolia.json"))
        rel = str(f.relative_to(ROOT))
        root = json.loads(f.read_text())["second_provider_finalized_root"]
        bad = root[:-1] + ("0" if root[-1] != "0" else "1")
        t = overlay({rel: sub(root, bad)})
        self.assertFails("lab-roots", t, "differs")

    def test_m0(self):
        self.assertPasses("m0")
        t = overlay({"lez/results/v0.4.0-rc3/gas-drop/summary.json": sub('"passed": true', '"passed": false')})
        self.assertFails("m0", t)


class Seals(TamperCase):
    """verifylib/groth16.py on recorded seals, and the seal checks on altered evidence."""

    @classmethod
    def setUpClass(cls):
        from verifylib import checks_seals, groth16
        cls.g = groth16
        cls.live = checks_seals._live_proofs(ROOT)

    def test_a_live_proof_verifies_and_alterations_fail(self):
        e = self.live[1]  # the release proof of the test network's live round trip (3.1.0)
        self.assertEqual(self.g.verify_seal(e["seal"], e["image"], e["digest"])[:2], (True, "3.1.0"))
        bad = bytearray(e["seal"])
        bad[100] ^= 1
        self.assertFalse(self.g.verify_seal(bytes(bad), e["image"], e["digest"])[0], "a seal byte")
        self.assertFalse(self.g.verify_seal(e["seal"], e["image"][:-1] + b"\x00", e["digest"])[0], "the image id")
        self.assertFalse(self.g.verify_seal(e["seal"], e["image"], bytes(32))[0], "the journal")
        other = bytes.fromhex(self.g.PARAMS["3.0.0"]["selector"]) + e["seal"][4:]
        self.assertFalse(self.g.verify_seal(other, e["image"], e["digest"])[0], "the other release's selector")
        self.assertFalse(self.g.verify_seal(e["seal"][:-1], e["image"], e["digest"])[0], "a short seal")
        big = e["seal"][:4] + self.g.P.to_bytes(32, "big") + e["seal"][36:]
        self.assertFalse(self.g.verify_seal(big, e["image"], e["digest"])[0], "a coordinate not below p")

    def test_the_check_fails_on_an_altered_proof(self):
        self.assertPasses("seals-v3.1")
        from verifylib.checks_seals import LIVE_PROOFS
        seal = json.loads((ROOT / LIVE_PROOFS).read_text())["release"]["seal"]
        flipped = seal[:-1] + ("0" if seal[-1] != "0" else "1")
        t = overlay({LIVE_PROOFS: sub(seal, flipped)})
        self.assertFails("seals-v3.1", t, "does not verify")


class Hashes(TamperCase):
    def test_site_manifest(self):
        self.assertPasses("site-manifest-git")
        m = json.loads((ROOT / "deploy/testnet/web/trust/bridge/verify/manifest.json").read_text())
        h = m["files"][3]["sha256"]
        t = overlay({"deploy/testnet/web/trust/bridge/verify/manifest.json": sub(h, h[::-1])})
        self.assertFails("site-manifest-git", t, m["files"][3]["path"])

    def test_image_ids(self):
        self.assertPasses("image-ids-agree")
        dep = json.loads((ROOT / "scripts/reproduce/deployed-programs.json").read_text())
        img = dep["programs"][0]["image_id"].lower()
        t = overlay({"docs/image-ids.md": sub(img[2:], img[2:][::-1], count=100)})
        self.assertFails("image-ids-agree", t, dep["programs"][0]["name"])

    def test_acc_leaf(self):
        self.assertPasses("acc-leaf-vectors")
        text = (ROOT / "vectors/acc-leaf.json").read_text()
        h = next(m.group(0) for m in re.finditer(r"[0-9a-f]{64}", text) if len(set(m.group(0))) > 4)
        t = overlay({"vectors/acc-leaf.json": sub(h, h[:-1] + ("0" if h[-1] != "0" else "1"))})
        self.assertFails("acc-leaf-vectors", t)


if __name__ == "__main__":
    unittest.main()
