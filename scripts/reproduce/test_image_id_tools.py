"""Tests of the image-id tooling's fail-closed checks: `python3 -I scripts/reproduce/test_image_id_tools.py`.

guest_ids.py: two methods.rs for one guest that disagree are a conflict (exit 1, the guest left out), not the first
one sorted; the r0vm column. compare_ids.py: a row counts only with "= id" in the r0vm column; every guest of
docs/image-ids.md's first table must be rebuilt; with --vault, a vault that cannot be read (RPC error, no endpoint,
empty answer, unknown mode) or an immutable that differs is a difference, never "6 of 6". The vault is a JSON-RPC stub
on 127.0.0.1. Standard library only; files go to a temporary directory (TMPDIR).
"""

import http.server
import importlib.util
import json
import os
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
GUEST_IDS = os.path.join(HERE, "guest_ids.py")
COMPARE = os.path.join(HERE, "compare_ids.py")
_spec = importlib.util.spec_from_file_location("verify_sepolia", os.path.join(HERE, "..", "verify-sepolia.py"))
vs = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(vs)

ID = {k: "0x" + (f"{i:02x}" * 32) for i, k in enumerate(
    ["attestation", "bridge", "checkpoint_b", "checkpoint", "segment", "release", "guest_a", "guest_b", "extra_c"], 1)}


def words_le(idhex: str) -> str:
    """risc0-build's methods.rs form of an id: eight u32 words, little-endian."""
    raw = bytes.fromhex(idhex.removeprefix("0x"))
    return ", ".join(str(int.from_bytes(raw[i:i + 4], "little")) for i in range(0, 32, 4))


def run(args, env=None):
    e = dict(os.environ)
    e.update(env or {})
    return subprocess.run([sys.executable, "-I", *args], capture_output=True, text=True, env=e, timeout=120)


class GuestIds(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        self.bin = os.path.join(self.root, "bin")
        os.makedirs(self.bin)

    def tearDown(self):
        self.tmp.cleanup()

    def methods(self, ws, hash_, name, idhex, binary, mtime):
        d = os.path.join(self.root, ws, "target", "release", "build", f"methods-{hash_}", "out")
        os.makedirs(d, exist_ok=True)
        p = os.path.join(d, "methods.rs")
        with open(p, "w") as f:
            f.write(f'pub const {name}_ID: [u32; 8] = [{words_le(idhex)}];\n'
                    f'pub const {name}_PATH: &str = "{binary}";\n')
        os.utime(p, (mtime, mtime))

    def binary(self, name, content):
        p = os.path.join(self.root, "guests", name + ".bin")
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "wb") as f:
            f.write(content)
        return p

    def fake_r0vm(self, idhex):
        """An r0vm on PATH that prints IDHEX for any binary (the real one computes it)."""
        p = os.path.join(self.bin, "r0vm")
        with open(p, "w") as f:
            f.write(f"#!/bin/sh\necho {idhex.removeprefix('0x')}\n")
        os.chmod(p, os.stat(p).st_mode | stat.S_IEXEC)
        return {"PATH": self.bin + os.pathsep + os.environ.get("PATH", "")}

    def guest_ids(self, env=None):
        out = os.path.join(self.root, "ids.tsv")
        r = run([GUEST_IDS, self.root, out], env)
        rows = [l.split("\t") for l in open(out).read().splitlines()[1:]] if os.path.exists(out) else []
        return r, rows

    def test_disagreeing_methods_rs_is_a_conflict(self):
        # The review's probe (L1): the older methods.rs sorts first and carries the recorded id; the newer one another.
        b = self.binary("release", b"new release binary")
        self.methods("bridge", "0000aaaa", "RELEASE", ID["release"], b, time.time() - 3600)
        self.methods("bridge", "ffffbbbb", "RELEASE", ID["guest_a"], b, time.time())
        r, rows = self.guest_ids(self.fake_r0vm(ID["guest_a"]))
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("CONFLICT bridge release", r.stderr)
        self.assertEqual([x for x in rows if x[1] == "release"], [])

    def test_agreeing_duplicates_give_one_row(self):
        b = self.binary("release", b"release binary")
        self.methods("bridge", "0000aaaa", "RELEASE", ID["release"], b, time.time() - 3600)
        self.methods("bridge", "ffffbbbb", "RELEASE", ID["release"], b, time.time())
        r, rows = self.guest_ids(self.fake_r0vm(ID["release"]))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(len(rows), 1)
        self.assertEqual((rows[0][1], rows[0][2], rows[0][6]), ("release", ID["release"], "= id"))

    def test_r0vm_column(self):
        b = self.binary("release", b"release binary")
        self.methods("bridge", "0000aaaa", "RELEASE", ID["release"], b, time.time())
        _, rows = self.guest_ids(self.fake_r0vm(ID["guest_b"]))
        self.assertEqual(rows[0][6], ID["guest_b"].removeprefix("0x"))   # r0vm's own id where it differs
        os.remove(os.path.join(self.bin, "r0vm"))
        _, rows = self.guest_ids({"PATH": self.bin})                    # no r0vm at all
        self.assertEqual(rows[0][6], "?")


class Fixture(unittest.TestCase):
    """compare_ids.py on a synthetic output directory, README and docs table (no tests of its own)."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        d = self.d = self.tmp.name
        os.makedirs(os.path.join(d, "out", "r0bf"))
        with open(os.path.join(d, "out", "ids.txt"), "w") as f:
            f.write(f"ferrata_lez.bin {'a' * 64} {ID['attestation'][2:]}\nferrata_bridge.bin {'b' * 64} {ID['bridge'][2:]}\n")
        with open(os.path.join(d, "out", "r0bf", "ids.jsonl"), "w") as f:
            for k in ("checkpoint_b", "checkpoint", "segment", "release"):
                f.write(json.dumps({"program": f"{k}.bin", "image_id": ID[k], "sha256": "c" * 64}) + "\n")
        with open(os.path.join(d, "README.md"), "w") as f:
            f.write("**The programs of `crown-test`**\n\n| Program | Image id |\n|---|---|\n")
            for label, k in [("attestation program", "attestation"), ("bridge program", "bridge"),
                             ("`checkpoint_b`", "checkpoint_b"), ("`checkpoint`", "checkpoint"),
                             ("`segment`", "segment"), ("release", "release")]:
                f.write(f"| {label} | `{ID[k]}` |\n")
        with open(os.path.join(d, "image-ids.md"), "w") as f:
            f.write("# Image ids\n\n| Program | Image id | sha256 |\n|---|---|---|\n"
                    f"| a guest: `guest_a` | `{ID['guest_a']}` | `x` |\n| b guest: `guest_b` | `{ID['guest_b']}` | `x` |\n"
                    f"\n## A second build, not built by `build`\n\n| Program | Image id |\n|---|---|\n"
                    f"| c: `extra_c` | `{ID['extra_c']}` |\n")

    def tearDown(self):
        self.tmp.cleanup()

    def tsv(self, rows):
        with open(os.path.join(self.d, "out", "ids.tsv"), "w") as f:
            f.write("workspace\tname\timage_id\tsha256_bin\tbytes_bin\tsha256_user_elf\tr0vm_id\n")
            for name, r0 in rows:
                f.write(f"lez\t{name}\t{ID[name]}\t{'d' * 64}\t10\t-\t{r0}\n")

    def compare(self, *extra, env=None):
        return run([COMPARE, "--out", os.path.join(self.d, "out"), "--readme", os.path.join(self.d, "README.md"),
                    "--table", os.path.join(self.d, "image-ids.md"), *extra], env)



class Compare(Fixture):
    def test_all_equal_passes(self):
        self.tsv([("guest_a", "= id"), ("guest_b", "= id")])
        r = self.compare()
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertIn("6 of 6 ids of the crown run's programs", r.stdout)
        self.assertIn("2 of 2 guests equal docs/image-ids.md; 0 of its first table's 2 missing", r.stdout)

    def test_r0vm_not_confirmed_is_a_difference(self):
        # The review's probe (L1): the id equals the table, but r0vm did not compute it ("?").
        self.tsv([("guest_a", "?"), ("guest_b", "= id")])
        r = self.compare()
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertRegex(r.stdout, r"DIFFER guest_a .*r0vm \?")

    def test_r0vm_other_id_is_a_difference(self):
        self.tsv([("guest_a", ID["extra_c"][2:]), ("guest_b", "= id")])
        self.assertEqual(self.compare().returncode, 1)

    def test_first_table_guest_missing(self):
        self.tsv([("guest_a", "= id")])
        r = self.compare()
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("MISSING guest_b", r.stdout)
        self.assertNotIn("MISSING extra_c", r.stdout)   # the later tables are other builds

    def test_no_ids_tsv(self):
        r = self.compare()
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("guests not compared", r.stdout)


class GuestsOnly(Fixture):
    """--guests (CI jobs that build some guests): no crown table or ids.txt needed, only the named guests compared."""

    def setUp(self):
        super().setUp()
        os.unlink(os.path.join(self.d, "out", "ids.txt"))              # a CI job builds no LEZ programs
        shutil.rmtree(os.path.join(self.d, "out", "r0bf"))

    def test_named_guests_equal(self):
        self.tsv([("guest_a", "= id")])
        r = self.compare("--guests", "guest_a")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("1 of 1 named guests equal docs/image-ids.md", r.stdout)

    def test_named_guest_missing_or_unconfirmed(self):
        self.tsv([("guest_a", "?")])
        r = self.compare("--guests", "guest_a,guest_b")
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertRegex(r.stdout, r"DIFFER guest_a .*r0vm \?")
        self.assertIn("MISSING guest_b", r.stdout)

    def test_no_vault_with_guests(self):
        self.tsv([("guest_a", "= id")])
        r = self.compare("--guests", "guest_a", "--vault", "0x" + "11" * 20)
        self.assertEqual(r.returncode, 2)


class StubRpc(http.server.BaseHTTPRequestHandler):
    """Answers eth_call by selector from the server's `answers` (a result string, or an error dict)."""

    def do_POST(self):
        req = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        out = {"jsonrpc": "2.0", "id": req.get("id")}
        if req.get("method") == "eth_call":
            sel = req["params"][0]["data"][2:10]
            a = self.server.answers.get(sel, {"code": 3, "message": "execution reverted"})
            out.update({"error": a} if isinstance(a, dict) else {"result": a})
        else:
            out["result"] = {"eth_chainId": "0xaa36a7", "eth_blockNumber": "0x100"}.get(req.get("method"), None)
        b = json.dumps(out).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def log_message(self, *a):
        pass


def word(v):
    return "0x" + (v.removeprefix("0x") if isinstance(v, str) else f"{v:064x}")


class CompareVault(Fixture):
    def setUp(self):
        super().setUp()
        self.tsv([("guest_a", "= id"), ("guest_b", "= id")])
        self.srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), StubRpc)
        self.srv.answers = {}
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()
        self.rpc = f"http://127.0.0.1:{self.srv.server_address[1]}"

    def tearDown(self):
        self.srv.shutdown()
        self.srv.server_close()
        super().tearDown()

    def vault(self, checkpoint, release, mode):
        self.srv.answers = {vs.selector("checkpointImageId()"): checkpoint, vs.selector("releaseImageId()"): release,
                            vs.selector("mode()"): mode}

    def compare_vault(self, rpc=None):
        return self.compare("--vault", "0x" + "de" * 20, "--rpc", rpc or self.rpc, env={"PUBNET_REQUEST_BUDGET_S": "2"})

    def test_reverting_vault_is_a_difference(self):
        # The review's probe (L2): every call reverts (a mistyped address); before the fix: "6 of 6", exit 0.
        r = self.compare_vault()
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("not read", r.stdout)
        self.assertNotIn("6 of 6", r.stdout)

    def test_empty_answer_is_a_difference(self):
        self.vault("0x", "0x", "0x")      # no contract at the address
        r = self.compare_vault()
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("empty return data", r.stdout)

    def test_unreachable_endpoint_is_a_difference(self):
        s = socket.socket()
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
        s.close()                       # nothing listens there
        r = self.compare_vault(f"http://127.0.0.1:{port}")
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("not read", r.stdout)
        self.assertNotIn("Traceback", r.stderr)

    def test_mode_b_vault_equal(self):
        self.vault(word(ID["checkpoint_b"]), word(ID["release"]), word(1))
        r = self.compare_vault()
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertIn("6 of 6 ids of the crown run's programs equal the recorded ones and the vault's immutables", r.stdout)

    def test_mode_a_vault_compares_rule_a(self):
        self.vault(word(ID["checkpoint"]), word(ID["release"]), word(0))
        self.assertEqual(self.compare_vault().returncode, 0)
        self.vault(word(ID["checkpoint_b"]), word(ID["release"]), word(0))   # rule (B)'s program in a mode-A vault
        r = self.compare_vault()
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertRegex(r.stdout, r"DIFFER checkpoint ")

    def test_release_immutable_differs(self):
        self.vault(word(ID["checkpoint_b"]), word(ID["segment"]), word(1))
        r = self.compare_vault()
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertRegex(r.stdout, r"DIFFER release ")

    def test_unknown_mode_is_a_difference(self):
        self.vault(word(ID["checkpoint_b"]), word(ID["release"]), word(7))
        r = self.compare_vault()
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("mode() is 7", r.stdout)


if __name__ == "__main__":
    unittest.main()
