#!/usr/bin/env python3
"""Re-reads the stand-in return leg's verdicts from an Ethereum JSON-RPC endpoint against a run's records
(lez-v03-return/README.md; e2e/testnet-return.sh calls it). Standard library only; reads only (eth_call,
eth_getCode, eth_getTransactionReceipt, eth_getTransactionCount); sends nothing.

  verify.py refusals <run> --rpc URL --phase pre-submit|pre-final|post-release
        the calls that must be refused (and the few that must be accepted) at that point of a run, each as an
        eth_call; appends what the node answered to <run>/refusals.json
  verify.py check <run> --rpc URL [--lez-rpc URL] [--fork]
        every recorded transaction, the deployment (the stand-in, its key set, RISC Zero's verifier behind it, the
        vault's immutables), the deposits, the signed checkpoint, both release proofs through RISC Zero's verifier, the
        payments and the permanent refusals. --fork: the endpoint is the rehearsal's local fork.

Words used below: REAL = checked by RISC Zero's Groth16 verifier or by the unchanged vault; STAND-IN = accepted on the
signatures of keys the run's operator holds.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import standin as s  # noqa: E402

ZERO = "0x" + "00" * 20
LEAF_TAG = s.sha256(b"ferrata/vault/leaf/v2")
TREE_EMPTY = bytes(32)


class Rpc:
    def __init__(self, url: str):
        self.url = url

    def __call__(self, method: str, *params):
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": list(params)}).encode()
        req = urllib.request.Request(self.url, data=body, headers={"Content-Type": "application/json",
                                                                   "User-Agent": "curl/8"})
        last = None
        for attempt in range(5):
            try:
                with urllib.request.urlopen(req, timeout=60) as reply:
                    out = json.load(reply)
                if "error" in out:
                    raise RuntimeError(json.dumps(out["error"])[:300])
                return out["result"]
            except OSError as e:
                last = e
                time.sleep(2 + 3 * attempt)
        raise RuntimeError(f"{method}: {last}")

    def call(self, to: str, data: bytes, sender: str = ZERO) -> tuple:
        """(True, return data) or (False, revert data)."""
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "eth_call",
                           "params": [{"from": sender, "to": to, "data": s.hx(data), "gas": hex(8_000_000)},
                                      "latest"]}).encode()
        req = urllib.request.Request(self.url, data=body, headers={"Content-Type": "application/json",
                                                                   "User-Agent": "curl/8"})
        last = None
        for attempt in range(5):
            try:
                with urllib.request.urlopen(req, timeout=60) as reply:
                    out = json.load(reply)
                break
            except OSError as e:
                last = e
                time.sleep(2 + 3 * attempt)
        else:
            raise RuntimeError(f"eth_call: {last}")
        if "error" in out:
            data = out["error"].get("data")
            if isinstance(data, dict):  # some nodes nest it
                data = data.get("data")
            if not isinstance(data, str) or not data.startswith("0x"):
                msg = out["error"].get("message", "")
                m = re.search(r"0x[0-9a-fA-F]{8,}", msg)
                data = m.group(0) if m else "0x"
            return False, bytes.fromhex(data[2:])
        return True, bytes.fromhex(out["result"][2:])


def selector(signature: str) -> bytes:
    return s.keccak256(signature.encode())[:4]


def w(value) -> bytes:
    if isinstance(value, bool):
        return s.word(int(value))
    if isinstance(value, int):
        return s.word(value)
    raw = s.unhex(value) if isinstance(value, str) else value
    return bytes(32 - len(raw)) + raw


def enc_bytes(data: bytes) -> bytes:
    return s.word(len(data)) + data + bytes(-len(data) % 32)


CP_FIELDS = ["nMin", "tMin", "parentState", "newState", "burnRoot", "burnCount", "importedLeafCount", "sunset",
             "echoNonce", "tipId", "tipHash", "n", "t"]
CP_TYPE = "(uint32,uint32,bytes32,bytes32,bytes32,uint64,uint64,bool,bytes32,uint64,bytes32,uint32,uint32)"
REL_TYPE = "(address,uint8,address,address,uint256,bytes32,bytes32)"


def submit_data(c: dict, seal: bytes) -> bytes:
    return (selector(f"submitCandidate({CP_TYPE},bytes)") + b"".join(w(c[k]) for k in CP_FIELDS)
            + s.word(14 * 32) + enc_bytes(seal))


def finalize_data(c: dict) -> bytes:
    return selector(f"finalizeAttested({CP_TYPE})") + b"".join(w(c[k]) for k in CP_FIELDS)


def release_data(r: dict, seal: bytes, burn_root: str | None = None) -> bytes:
    fields = [r["token"], int(r["denomIndex"]), r["recipient"], r["relayer"], int(str(r["relayFee"]), 0),
              burn_root or r["burnRoot"], r["nullifier"]]
    return selector(f"release({REL_TYPE},bytes)") + b"".join(w(f) for f in fields) + s.word(8 * 32) + enc_bytes(seal)


def verify_data(seal: bytes, image_id: bytes, digest: bytes) -> bytes:
    return selector("verify(bytes,bytes32,bytes32)") + s.word(96) + image_id + digest + enc_bytes(seal)


def release_journal(chain_id: int, vault: str, config_id: str, r: dict, amount: int) -> bytes:
    return b"".join([s.RELEASE_DOMAIN, w(chain_id), w(vault), w(config_id), w(r["burnRoot"]), w(r["nullifier"]),
                     w(r["token"]), w(amount), w(r["recipient"]), w(r["relayer"]), w(int(str(r["relayFee"]), 0))])


def load(run: str, name: str):
    with open(os.path.join(run, name)) as f:
        return json.load(f)


def have(run: str, name: str) -> bool:
    return os.path.isfile(os.path.join(run, name))


class Run:
    def __init__(self, run: str):
        self.dir = run
        self.plan = load(run, "plan.json")
        self.chain_id = int(self.plan["chainId"])
        self.vault = self.plan["vault"]
        self.stand_in = self.plan["standIn"]["address"]
        self.risc_zero = self.plan["standIn"]["riscZero"]
        self.groth16 = self.plan["standIn"]["groth16Verifier"]
        self.config_id = self.plan["lezConfigId"]
        self.release_id = s.unhex(self.plan["releaseImageId"], 32)
        self.crown = load(run, "crown.json") if have(run, "crown.json") else {}
        self.candidate = load(run, "checkpoint/candidate.json") if have(run, "checkpoint/candidate.json") else None

    def releases(self) -> list:
        return [r for r in self.crown.get("releases", []) if r.get("seal")]


# ---- refusals ------------------------------------------------------------------------------------------------------

def expect(rpc: Rpc, out: list, name: str, to: str, data: bytes, want: str | None, why: str) -> None:
    """`want`: an error signature the call must revert with, "revert" for any revert, None for success."""
    ok, ret = rpc.call(to, data)
    if want is None:
        good, got = ok, "accepted" if ok else "reverted " + s.hx(ret[:4])
    elif want == "revert":
        good, got = not ok, "accepted" if ok else "reverted " + s.hx(ret[:4])
    else:
        good = (not ok) and ret[:4] == selector(want)
        got = "accepted" if ok else "reverted " + s.hx(ret[:4])
    block = int(rpc("eth_blockNumber"), 16)
    out.append({"name": name, "to": to, "expected": want or "accepted", "got": got, "ok": good, "block": block,
                "why": why})
    print(f"{'PASS' if good else 'FAIL'} {name}: expected {want or 'accepted'}, {got} (block {block})", flush=True)


def permanent_refusals(rpc: Rpc, r: Run, out: list) -> None:
    """What stays true whenever it is asked, once the run is done."""
    c, seal = r.candidate["checkpoint"], s.unhex(r.candidate["seal"])
    digest = s.unhex(r.candidate["journalDigest"], 32)
    expect(rpc, out, "stand-in: the recorded attestation (a threshold of the key set)", r.stand_in,
           verify_data(seal, s.STAND_IN_IMAGE_ID, digest), None, "STAND-IN: these signatures are why the root counts")
    expect(rpc, out, "stand-in: one signature (below the threshold)", r.stand_in,
           verify_data(seal[:65], s.STAND_IN_IMAGE_ID, digest), "BelowThreshold()", "a root signed below threshold")
    expect(rpc, out, "stand-in: one signature twice (a duplicated signer)", r.stand_in,
           verify_data(seal[:65] * 2, s.STAND_IN_IMAGE_ID, digest), "SignersNotAscending()", "a duplicated signer")
    expect(rpc, out, "stand-in: the attestation for another journal", r.stand_in,
           verify_data(seal, s.STAND_IN_IMAGE_ID, s.sha256(b"another journal")), "NotASigner()",
           "the signatures are over one journal digest")
    expect(rpc, out, "stand-in: the attestation presented as a checkpoint_b proof", r.stand_in,
           verify_data(seal, s.unhex("f8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e"), digest),
           "revert", "every image id but the stand-in's goes to RISC Zero's verifier")
    amount = 10 ** 15
    for i, rel in enumerate(r.releases()):
        rseal = s.unhex(rel["seal"])
        rdigest = s.sha256(release_journal(r.chain_id, r.vault, r.config_id, rel, amount))
        for label, to in (("through the stand-in", r.stand_in), ("RISC Zero's emergency stop", r.risc_zero),
                          ("RISC Zero's Groth16 verifier", r.groth16)):
            expect(rpc, out, f"release {i}: its proof verifies, {label}", to,
                   verify_data(rseal, r.release_id, rdigest), None, "REAL: a zero-knowledge proof of `release`")
        expect(rpc, out, f"release {i}: its proof for another journal", r.stand_in,
               verify_data(rseal, r.release_id, s.sha256(b"another release journal")), "VerificationFailed()",
               "REAL: the proof is of one journal")
        expect(rpc, out, f"release {i}: the stand-in signatures as its proof", r.stand_in,
               verify_data(seal, r.release_id, rdigest), "revert", "signatures are not a release proof")
        expect(rpc, out, f"release {i}: under a root nobody signed", r.vault,
               release_data(rel, rseal, s.hx(s.sha256(b"ferrata testnet-return: a burn root nobody signed"))),
               "BurnRootNotAccepted()", "a release under an unsigned root")


def cmd_refusals(a) -> int:
    rpc, r, out = Rpc(a.rpc), Run(a.run), []
    c, seal = r.candidate["checkpoint"], s.unhex(r.candidate["seal"])
    if a.phase == "pre-submit":
        expect(rpc, out, "vault: the candidate with one signature", r.vault, submit_data(c, seal[:65]),
               "BelowThreshold()", "a root signed below threshold")
        expect(rpc, out, "vault: the candidate with one signature twice", r.vault, submit_data(c, seal[:65] * 2),
               "SignersNotAscending()", "a duplicated signer")
        other = dict(c, burnRoot=s.hx(s.sha256(b"ferrata testnet-return: a burn root nobody signed")))
        expect(rpc, out, "vault: the signatures with another burn root", r.vault, submit_data(other, seal),
               "NotASigner()", "the signatures are over one journal")
        expect(rpc, out, "vault: the candidate as signed", r.vault, submit_data(c, seal), None,
               "STAND-IN: a threshold of the key set signed this journal")
        for i, rel in enumerate(r.releases()):
            expect(rpc, out, f"vault: release {i} before its root is submitted", r.vault,
                   release_data(rel, s.unhex(rel["seal"])), "BurnRootNotAccepted()",
                   "a real release proof under a root that is not accepted")
    elif a.phase == "pre-final":
        expect(rpc, out, "vault: finalisation before the wait is over", r.vault, finalize_data(c), "WaitNotOver()",
               "the candidate's public wait")
        for i, rel in enumerate(r.releases()):
            expect(rpc, out, f"vault: release {i} while its root is only a candidate", r.vault,
                   release_data(rel, s.unhex(rel["seal"])), "BurnRootNotAccepted()",
                   "a real release proof under a root that is signed but not final")
    else:
        for i, rel in enumerate(r.releases()):
            expect(rpc, out, f"vault: release {i} again (a replay)", r.vault,
                   release_data(rel, s.unhex(rel["seal"])), "NullifierAlreadyUsed()", "a release replayed")
        permanent_refusals(rpc, r, out)
    path = os.path.join(a.run, "refusals.json")
    record = load(a.run, "refusals.json") if os.path.isfile(path) else {}
    record[a.phase] = out
    s.write(path, record)
    return 0 if all(x["ok"] for x in out) else 1


# ---- the whole run -------------------------------------------------------------------------------------------------

RESULTS = []


def check(name: str):
    def deco(fn):
        try:
            detail = fn()
            RESULTS.append((True, name, detail or ""))
        except Exception as e:  # every failure is a FAIL line; the other checks still run
            RESULTS.append((False, name, f"{type(e).__name__}: {str(e)[:400]}"))
        ok, _, d = RESULTS[-1]
        print(f"{'PASS' if ok else 'FAIL'} {name}{': ' + d if d else ''}", flush=True)
        return fn
    return deco


def need(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


def rlp_address(sender: str, nonce: int) -> str:
    n = b"" if nonce == 0 else nonce.to_bytes((nonce.bit_length() + 7) // 8, "big")
    item = bytes([0x80]) if nonce == 0 else (n if nonce < 0x80 else bytes([0x80 + len(n)]) + n)
    body = bytes([0x94]) + s.unhex(sender, 20) + item
    return s.hx(s.keccak256(bytes([0xC0 + len(body)]) + body)[12:])


def image_id_of_release() -> str:
    for line in open(os.path.join(REPO, "docs", "image-ids.md")):
        if "`release`" in line and line.startswith("| release"):
            return re.search(r"0x[0-9a-f]{64}", line).group(0)
    raise AssertionError("docs/image-ids.md has no release row")


def cmd_check(a) -> int:
    rpc, r = Rpc(a.rpc), Run(a.run)
    plan, crown = r.plan, r.crown
    manifest = load(a.run, "manifest.json")
    deploy = load(a.run, "deploy.json")
    deployer = plan["deployer"]

    def view(to: str, signature: str, *args) -> bytes:
        ok, ret = rpc.call(to, selector(signature) + b"".join(w(x) for x in args))
        need(ok, f"{signature} reverted")
        return ret

    def word_of(to: str, signature: str, *args, i: int = 0) -> bytes:
        return view(to, signature, *args)[32 * i:32 * i + 32]

    receipts = {}

    @check("the endpoint is the run's chain")
    def _():
        got = int(rpc("eth_chainId"), 16)
        need(got == r.chain_id, f"chain id {got}, the run is on {r.chain_id}")
        return f"chain id {got}" + (" (the rehearsal's local fork)" if a.fork else "")

    @check("every recorded transaction is on chain as recorded")
    def _():
        count = gas = 0
        for stage, rows in manifest["stages"].items():
            for row in rows:
                rc = rpc("eth_getTransactionReceipt", row["hash"])
                need(rc is not None, f"{stage}: {row['hash']} has no receipt")
                need(rc["status"] == "0x1", f"{stage}: {row['hash']} failed")
                need(rc["from"].lower() == deployer.lower(), f"{stage}: not sent by the deployer")
                for key in ("blockNumber", "gasUsed"):
                    need(int(rc[key], 16) == int(row[key], 16), f"{stage}: {key}")
                need((rc.get("to") or "").lower() == (row.get("to") or "").lower(), f"{stage}: to")
                receipts.setdefault(stage, []).append(rc)
                count, gas = count + 1, gas + int(rc["gasUsed"], 16)
        return f"{count} transactions, {gas:,} gas"

    @check("the deployment's addresses are the deployer's CREATE addresses")
    def _():
        first = int(plan["firstNonce"])
        need(rlp_address(deployer, first) == r.stand_in.lower(), "stand-in")
        need(deploy["standIn"].lower() == r.stand_in.lower() and deploy["vault"].lower() == r.vault.lower(), "record")
        offset = 1 if plan.get("registryReused") else 2
        need(rlp_address(deployer, first + offset) == r.vault.lower(), "vault")
        for addr in (r.stand_in, r.vault, deploy["registry"], r.risc_zero, r.groth16):
            need(rpc("eth_getCode", addr, "latest") != "0x", f"{addr} has no code")
        return f"stand-in {r.stand_in} (nonce {first}), vault {r.vault} (nonce {first + offset})"

    @check("STAND-IN: the verifier is the stand-in, with the recorded key set, in front of RISC Zero's stop")
    def _():
        need(word_of(r.stand_in, "STAND_IN_IMAGE_ID()") == s.STAND_IN_IMAGE_ID, "stand-in id")
        need(s.STAND_IN_IMAGE_ID == s.sha256(b"ferrata/stand-in/checkpoint-attestation/v1"), "label")
        need(word_of(r.stand_in, "riscZero()")[12:] == s.unhex(r.risc_zero, 20), "riscZero")
        need(int.from_bytes(word_of(r.stand_in, "threshold()"), "big") == int(plan["standIn"]["threshold"]), "threshold")
        got = view(r.stand_in, "signers()")
        n = int.from_bytes(got[32:64], "big")
        signers = [s.hx(got[64 + 32 * i + 12:96 + 32 * i]) for i in range(n)]
        need(signers == [x.lower() for x in plan["standIn"]["signers"]], f"signers {signers}")
        need(word_of(r.risc_zero, "verifier()")[12:] == s.unhex(r.groth16, 20), "the stop wraps another verifier")
        need(int.from_bytes(word_of(r.risc_zero, "paused()"), "big") == 0, "RISC Zero's stop is tripped")
        need(int.from_bytes(word_of(r.stand_in, "paused()"), "big") == 0, "paused()")
        code = bytes.fromhex(rpc("eth_getCode", r.stand_in, "latest")[2:])
        if plan["standIn"].get("codeHash"):
            need(s.hx(s.keccak256(code)) == plan["standIn"]["codeHash"], "the stand-in's code is not the recorded one")
        return f"{plan['standIn']['threshold']} of {n} keys {signers}; RISC Zero's stop {r.risc_zero}"

    @check("the vault's immutables: the stand-in, the stand-in id, the REAL release program, rule (B)")
    def _():
        need(word_of(r.vault, "verifier()")[12:] == s.unhex(r.stand_in, 20), "verifier")
        need(word_of(r.vault, "checkpointImageId()") == s.STAND_IN_IMAGE_ID, "checkpointImageId")
        need(word_of(r.vault, "releaseImageId()") == r.release_id, "releaseImageId")
        need(s.hx(r.release_id) == image_id_of_release(), "the release id is not docs/image-ids.md's")
        need(word_of(r.vault, "lezConfigId()") == s.unhex(r.config_id, 32), "lezConfigId")
        need(word_of(r.vault, "storedLezConfigId()") == s.unhex(r.config_id, 32), "slot 2")
        need(int.from_bytes(word_of(r.vault, "mode()"), "big") == 1, "not rule (B)")
        need(int.from_bytes(word_of(r.vault, "nMin()"), "big") == len(plan["standIn"]["signers"]), "nMin")
        need(int.from_bytes(word_of(r.vault, "tMin()"), "big") == int(plan["standIn"]["threshold"]), "tMin")
        need(int.from_bytes(word_of(r.vault, "delta()"), "big") == int(plan["delta"]), "delta")
        need(int.from_bytes(word_of(r.vault, "turnSeconds()"), "big") == 0, "turnSeconds")
        need(int.from_bytes(word_of(r.vault, "revokeWindow()"), "big") == int(plan["revokeWindow"]), "revokeWindow")
        need(int.from_bytes(word_of(r.vault, "releaseDelayFor(address,uint256)", ZERO, 0), "big")
             == int(plan["releaseDelay"]), "release delay")
        return f"checkpointImageId {s.hx(s.STAND_IN_IMAGE_ID)} (no program's id), releaseImageId {s.hx(r.release_id)}"

    @check("the vault's genesisState is the state at the recorded LEZ block with the stand-in key set")
    def _():
        tip = plan["lez"]["genesisTip"]
        keys = s.signer_words(plan["standIn"]["signers"])
        state = s.lez_state(int(tip["id"]), s.unhex(tip["hash"], 32), s.key_set_hash(keys), bytes(32), 0,
                            [bytes(32)] * 32, 0, False)
        need(s.hx(state) == plan["genesisState"], "plan.json's genesisState")
        need(word_of(r.vault, "genesisState()") == state, "the vault's genesisState")
        return f"LEZ block {tip['id']} {tip['hash']} ({tip['status']} on the public node when planned)"

    @check("the deposits: two leaves, the recorded commitments, the vault's root recomputed")
    def _():
        topic = s.hx(s.keccak256(b"Deposit(address,uint64,uint64,address,uint256,bytes32)"))
        leaves = []
        for d in crown["deposits"]:
            rc = receipts[f"deposit-{d['j']}"][0]
            logs = [lg for lg in rc["logs"] if lg["address"].lower() == r.vault.lower() and lg["topics"][0] == topic]
            need(len(logs) == 1, f"deposit {d['j']}: no Deposit event")
            data = bytes.fromhex(logs[0]["data"][2:])
            index, leaf_index = int.from_bytes(data[0:32], "big"), int.from_bytes(data[32:64], "big")
            amount, commitment = data[64:96], data[96:128]
            need(index == d["j"] and s.hx(commitment) == d["commitment"], f"deposit {d['j']}: commitment")
            need(int.from_bytes(amount, "big") == 10 ** 15, "denomination")
            leaves.append(s.sha256(b"\x00" + LEAF_TAG + leaf_index.to_bytes(8, "big") + s.unhex(deployer, 20)
                                   + bytes(20) + amount + commitment))
        count = int.from_bytes(word_of(r.vault, "nextLeafIndex()"), "big")
        need(count >= len(leaves), "nextLeafIndex")
        if count == len(leaves):
            need(word_of(r.vault, "depositRoot()") == s.tree_root(leaves), "depositRoot")
        return f"{len(leaves)} deposits of 0.001 ETH (one public, one private destination), root recomputed"

    @check("STAND-IN: the checkpoint's journal, signed by a threshold of the key set, is what the vault finalised")
    def _():
        cand = r.candidate
        c = cand["checkpoint"]
        journal = s.checkpoint_journal(c)
        need(s.hx(journal) == cand["journal"] == crown["candidate"]["journal"], "journal")
        need(c["vault"].lower() == r.vault.lower() and c["chainId"] == r.chain_id, "the journal's vault and chain")
        need(c["lezConfigId"] == r.config_id, "lezConfigId")
        got = s.seal_signers(dict(cand, verifier=r.stand_in))
        allowed = {s.unhex(x, 20) for x in plan["standIn"]["signers"]}
        need(all(g in allowed for g in got) and got == sorted(set(got)), "signers")
        need(len(got) >= int(plan["standIn"]["threshold"]), "below threshold")
        need(cand["seal"] == crown["candidate"]["seal"], "seal")
        burns = load(a.run, "checkpoint/burns.json")
        leaves = [s.burn_leaf(s.unhex(r.config_id, 32), k, s.unhex(b["token"], 20), s.unhex(b["amount"], 32),
                              s.unhex(b["c_out"], 32), s.unhex(b["r"], 32)) for k, b in enumerate(burns["burns"])]
        need(s.hx(s.tree_root(leaves)) == c["burnRoot"] and len(leaves) == c["burnCount"], "burn root of the burns")
        keys = s.signer_words(plan["standIn"]["signers"])
        state = s.lez_state(c["tipId"], s.unhex(c["tipHash"], 32), s.key_set_hash(keys), bytes(32), c["burnCount"],
                            s.tree_frontier(leaves), c["importedLeafCount"], False)
        need(s.hx(state) == c["newState"], "newState")
        topic = s.hx(s.keccak256(
            b"CheckpointFinalized(uint256,bytes32,bytes32,bytes32,uint64,uint64,bool,uint64,bytes32,uint32,uint32)"))
        logs = [lg for lg in receipts["finalizeAttested"][0]["logs"] if lg["topics"][0] == topic]
        need(len(logs) == 1 and logs[0]["topics"][2] == c["newState"], "CheckpointFinalized")
        need(logs[0]["data"][2 + 64:2 + 128] == c["burnRoot"][2:], "the finalised burn root")
        ok, ret = rpc.call(r.vault, selector("burnRootStatus(bytes32)") + w(c["burnRoot"]))
        need(ok and int.from_bytes(ret[:32], "big") == 1, "the burn root is not accepted")
        final = cand["source"].get("final_on_node")
        return (f"burn root {c['burnRoot']} over {c['burnCount']} burns at LEZ block {c['tipId']}; signed by "
                f"{[s.hx(g) for g in got]}; source: {cand['source']['kind']}"
                + ("" if final is None else f", tip final on the node: {final}"))

    @check("REAL: both release proofs verify in RISC Zero's Groth16 verifier, and the vault paid them")
    def _():
        topic = s.hx(s.keccak256(b"Release(bytes32,address,address,uint256,address,uint256,bytes32)"))
        rels = r.releases()
        need(len(rels) == 2, f"{len(rels)} releases with a seal")
        paid = []
        for i, rel in enumerate(rels):
            seal = s.unhex(rel["seal"])
            need(len(seal) == 260, "a Groth16 seal is 260 bytes")
            digest = s.sha256(release_journal(r.chain_id, r.vault, r.config_id, rel, 10 ** 15))
            for to in (r.groth16, r.risc_zero, r.stand_in):
                ok, _ret = rpc.call(to, verify_data(seal, r.release_id, digest))
                need(ok, f"release {i}: the proof does not verify at {to}")
            need(rel["burnRoot"] == r.candidate["checkpoint"]["burnRoot"], "the release's root")
            ok, ret = rpc.call(r.vault, selector("nullifierUsed(bytes32)") + w(rel["nullifier"]))
            need(ok and int.from_bytes(ret, "big") == 1, f"release {i}: nullifier not used")
            logs = [lg for lg in receipts[f"release-{i}"][0]["logs"] if lg["topics"][0] == topic]
            need(len(logs) == 1 and logs[0]["topics"][1] == rel["nullifier"], f"release {i}: Release event")
            need("0x" + logs[0]["topics"][3][-40:] == rel["recipient"].lower(), f"release {i}: recipient")
            paid.append(int(logs[0]["data"][2:66], 16))
        return f"2 proofs of image {s.hx(r.release_id)}; released {[p / 1e18 for p in paid]} ETH"

    @check("the refusals that stay true: replays, an unsigned root, below threshold, signatures as a proof")
    def _():
        out = []
        for i, rel in enumerate(r.releases()):
            expect(rpc, out, f"vault: release {i} again (a replay)", r.vault,
                   release_data(rel, s.unhex(rel["seal"])), "NullifierAlreadyUsed()", "a release replayed")
        permanent_refusals(rpc, r, out)
        need(all(x["ok"] for x in out), "; ".join(x["name"] for x in out if not x["ok"]))
        return f"{len(out)} calls answered as expected"

    if a.lez_rpc:
        @check("the LEZ tip of the checkpoint on the public LEZ node, now")
        def _():
            lez = s.Lez(a.lez_rpc)
            c = r.candidate["checkpoint"]
            b = s.checked_block(lez, c["tipId"])
            need(s.hx(b["hash"]) == c["tipHash"], "the node now has another block at this id")
            return f"block {b['id']} {s.hx(b['hash'])}: hash recomputed from its contents, status {b['status']}"

    passed = sum(1 for ok, _, _ in RESULTS if ok)
    print(f"{passed} of {len(RESULTS)} checks passed")
    return 0 if passed == len(RESULTS) else 1


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("refusals"); p.add_argument("run"); p.add_argument("--rpc", required=True)
    p.add_argument("--phase", required=True, choices=["pre-submit", "pre-final", "post-release"])
    p = sub.add_parser("check"); p.add_argument("run"); p.add_argument("--rpc", required=True)
    p.add_argument("--lez-rpc"); p.add_argument("--fork", action="store_true")
    a = ap.parse_args()
    sys.exit(cmd_refusals(a) if a.cmd == "refusals" else cmd_check(a))


if __name__ == "__main__":
    main()
