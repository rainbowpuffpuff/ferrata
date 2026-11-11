#!/usr/bin/env python3
"""Re-checks a crown run (e2e/crown/README.md) from public chain data and the recorded devnet blocks.

  verify.py <run dir> --lez DIR [--channel-key FILE --e2e ferrata-e2e --realblocks realblocks-crown]

<run dir> is e2e/runs/<run> (identity.json, params.json, crown.json, manifest.json, deploy.json, run.json, captures/,
checkpoint/, releases/); --lez holds session-a/ and session-b/ (the run directory's lez/); --channel-key the devnet's
channel key (in the cloud only). The chain is read through SEPOLIA_RPC_URL (read-only: eth_call, logs, receipts;
during a fork rehearsal it is the fork). With --e2e and --realblocks (the cloud runner: CPU work) the checkpoint's
input is rebuilt from the node's recorded blocks and must be the proven one byte for byte, and the guest is executed
on it again. Prints one line per check, PASS or FAIL; exit 0 only if every check passed.

Every value is computed here independently where Python can: configId from session A's fields (the 261-byte
preimage of lez/bridge-wire BridgeConfig::id), genesisState (identity.py), the deposit commitments (identity.py), the
vault's CREATE address, the journal's fields, the seals through RISC Zero's verifier on chain (eth_call), the
captures' storage proofs against their headers (mpt.py) and those headers against the chain's own blocks.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.dont_write_bytecode = True
sys.path.insert(0, HERE)
import mpt  # noqa: E402

_spec = importlib.util.spec_from_file_location("crown_identity", os.path.join(HERE, "identity.py"))
ident = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ident)

CHAIN_ID = 11155111
CHECKPOINT_DOMAIN = "0xbdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7"
# The vault-side programs a run must have used: crown-live-1 and its rehearsal crown-rehearsal-3 ran programs built at the
# maintainer's checkout path (before guest builds were path-independent); later runs the final programs
# (e2e/crown/README.md's first table, docs/image-ids.md), as cloud/drive.py pins them.
RUN_IDS = {run: {"checkpoint_b": "0xa475e8f9aaa41d0ae3bf9a20ae7d937ebc637a961808765f0ff175158760d3b2",
                 "release": "0x17257f91bca70abf7a5476b498d687e95b6fb4d59e75b6bca1d46877ac9bf5cd"}
           for run in ("crown-live-1", "crown-rehearsal-3")}
FINAL_IDS = {"checkpoint_b": "0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e",
             "release": "0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9"}
IDS = FINAL_IDS  # set per run in main()
RESULTS = []


def check(name: str):
    def deco(fn):
        try:
            detail = fn()
            RESULTS.append((True, name, detail or ""))
        except Exception as e:  # every failure is a FAIL line, the other checks still run
            RESULTS.append((False, name, f"{type(e).__name__}: {str(e)[:400]}"))
        ok, _, d = RESULTS[-1]
        print(f"{'PASS' if ok else 'FAIL'} {name}{': ' + d if d else ''}", flush=True)
        return fn
    return deco


def need(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


def cast(*a: str) -> str:
    p = subprocess.run(["cast", *a], capture_output=True, text=True, timeout=180,
                       env={**os.environ, "ETH_RPC_URL": os.environ["SEPOLIA_RPC_URL"]})
    if p.returncode != 0:
        raise RuntimeError(f"cast {a[0]}: {p.stderr.strip()[-300:]}")
    return p.stdout.strip()


def cast_ok(*a: str) -> bool:
    return subprocess.run(["cast", *a], capture_output=True, text=True, timeout=180,
                          env={**os.environ, "ETH_RPC_URL": os.environ["SEPOLIA_RPC_URL"]}).returncode == 0


def first_int(s: str) -> int:
    return int(s.split()[0], 0)


def b(x: str) -> bytes:
    return bytes.fromhex(x.lower().removeprefix("0x"))


def h(x: bytes) -> str:
    return "0x" + x.hex()


def jl(p: str):
    with open(p) as f:
        return json.load(f)


def sha256f(p: str) -> str:
    with open(p, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("run")
    ap.add_argument("--lez", required=True)
    ap.add_argument("--channel-key", help="the devnet's channel key (the runner's; never fetched)")
    ap.add_argument("--e2e")
    ap.add_argument("--realblocks")
    a = ap.parse_args()
    global IDS
    IDS = RUN_IDS.get(os.path.basename(a.run.rstrip("/")), FINAL_IDS)
    R, L = a.run, a.lez
    rj, idn, crown = jl(f"{R}/run.json"), jl(f"{R}/identity.json"), jl(f"{R}/crown.json")
    sa, sb = jl(f"{L}/session-a/session-a.json"), jl(f"{L}/session-b/session-b.json")
    notes = jl(f"{L}/session-b/burn-notes.json")
    vault, deployer = idn["vault"], idn["deployer"]
    call = lambda sig, *x: cast("call", vault, sig, *x)
    print(f"crown run {os.path.basename(R.rstrip('/'))} ({rj['mode']}{': FORK, the chain is anvil' if rj['mode'] == 'fork' else ''}),"
          f" vault {vault}, deployer {deployer}")
    words = lambda hexs: [h(b(hexs)[i:i + 32]) for i in range(0, len(b(hexs)), 32)]
    jw = words(crown["candidate"]["journal"])

    @check("chain")
    def _():
        need(int(cast("chain-id")) == CHAIN_ID, "not Sepolia's chain id")
        return f"chain id {CHAIN_ID}, head {cast('block-number')}"

    @check("configId from session A's fields")
    def _():
        pre = (b"ferrata/config/v1" + b(sa["bedrockGenesisId"]) + b(sa["channel"]["id"]) + b(sa["bridge"]["header"])
               + b(sa["bridge"]["imageId"]) + int(sa["evmChainId"]).to_bytes(32, "big") + b(vault) + b(sa["attestationRootId"])
               + b(sa["salt"]))
        need(len(pre) == 261, f"preimage of {len(pre)} bytes")
        cid = h(hashlib.sha256(pre).digest())
        need(cid == idn["configId"].lower(), f"recomputed {cid}, identity {idn['configId']}")
        on = call("storedLezConfigId()(bytes32)")
        need(on.lower() == cid, f"vault's storedLezConfigId {on}")
        return f"{cid} = the vault's storedLezConfigId"

    @check("bedrockGenesisId = sha256(deployment settings)")
    def _():
        got = "0x" + sha256f(f"{L}/session-a/deployment-settings.yaml")
        need(got == sa["bedrockGenesisId"].lower(), f"{got} != {sa['bedrockGenesisId']}")
        return got

    @check("genesisState from T0 and the channel")
    def _():
        ch = sa["channel"]
        gs = h(ident.genesis_state(int(sa["tip"]["id"]), b(sa["tip"]["hash"]), [b(k) for k in ch["keys"]],
                                   int(ch["postingTimeframe"]), b(ch["configTipHash"])))
        on = call("genesisState()(bytes32)")
        need(gs == idn["genesisState"].lower() == on.lower(), f"recomputed {gs}, identity {idn['genesisState']}, vault {on}")
        return f"{gs} (T0 = {sa['tip']['id']}) = the vault's"

    @check("vault: code, CREATE address, rule (B)")
    def _():
        need(cast("code", vault) not in ("0x", ""), "no code")
        addr = cast("compute-address", deployer, "--nonce", str(int(idn["firstNonce"]) + 2)).split()[-1]
        need(addr.lower() == vault.lower(), f"CREATE(deployer, {idn['firstNonce']} + 2) = {addr}")
        need(call("mode()(uint8)") == "1", "mode is not B")
        return f"CREATE(deployer, nonce {int(idn['firstNonce']) + 2}), mode B"

    @check("every recorded transaction succeeded, sent by the deployer")
    def _():
        m = jl(f"{R}/manifest.json")["stages"]
        n = 0
        for stage, rows in m.items():
            for t in rows:
                rc = json.loads(cast("receipt", t["hash"], "--json"))
                need(rc["status"] == "0x1", f"{stage} {t['hash']} status {rc['status']}")
                need(rc["from"].lower() == deployer.lower(), f"{stage} {t['hash']} from {rc['from']}")
                need(int(rc["blockNumber"], 16) == int(t["blockNumber"], 16), f"{stage} block")
                n += 1
        return f"{n} transactions of {len(m)} stages ({', '.join(m)})"

    @check("deposits: two logs, commitments recomputed")
    def _():
        logs = json.loads(cast("logs", "--json", "--from-block", str(rj.get("deploy_from_block") or 0), "--address", vault,
                               "Deposit(address indexed,uint64,uint64,address indexed,uint256,bytes32)"))
        need(len(logs) == 2, f"{len(logs)} Deposit logs")
        out = []
        for lg, d in zip(logs, crown["deposits"]):
            w = words(lg["data"])
            j, leaf, amount, c = int(w[0], 16), int(w[1], 16), int(w[2], 16), w[3]
            need("0x" + lg["topics"][1][-40:].lower() == deployer.lower(), "depositor")
            kind = 2 if d["kind"] == "public" else 1
            want = h(ident.commitment(CHAIN_ID, b(vault), b(deployer), j, kind, b(d["dest"])))
            need(c == want == d["commitment"].lower(), f"deposit {j}: log {c}, recomputed {want}")
            out.append(f"j{j} leaf {leaf} {amount} wei {d['kind']}")
        return "; ".join(out)

    @check("captures: headers are the chain's blocks; every slot proven")
    def _():
        out = []
        for n in (1, 2):
            d = f"{R}/captures/capture-{n}"
            hdr = open(f"{d}/header.rlp", "rb").read()
            blk = json.load(open(f"{d}/block.json"))
            blk = blk.get("result", blk)
            num = int(blk["number"], 16)
            if rj["mode"] == "live":
                chain_hash = cast("block", str(num), "--field", "hash")
                need(chain_hash.lower() == h(mpt.keccak256(hdr)), f"capture {n}: header is not block {num} of the chain")
            pf = [f for f in os.listdir(d) if f.lower() == f"proof_{vault.lower()}_{num}.json"]
            need(len(pf) == 1, f"capture {n}: no proof file of the vault")
            proof = json.load(open(f"{d}/{pf[0]}"))
            got = mpt.check_account_and_slots(mpt.header_state_root(hdr), proof.get("result", proof))["slots"]
            need(got.get("0x" + "00" * 31 + "01") == n, f"capture {n}: slot 1 = {got.get('0x' + '00' * 31 + '01')}")
            need(h(got.get("0x" + "00" * 31 + "02", 0).to_bytes(32, "big")) == idn["configId"].lower(), f"capture {n}: slot 2")
            reg = "0x" + hashlib.sha256(b"ferrata/vault/registry/v1" + bytes(20)).hexdigest()
            need(got.get(reg, 0) != 0, f"capture {n}: the ETH registry slot is empty")
            out.append(f"capture {n}: block {num}, {len(got)} slots")
        return "; ".join(out) + ("" if rj["mode"] == "live" else " (FORK: headers are the fork's, not checked against Sepolia)")

    @check("LEZ session records")
    def _():
        need(sb["tipT0"]["id"] == sa["tip"]["id"] and sb["tipT0"]["hash"].lower() == sa["tip"]["hash"].lower(), "tipT0 != T0")
        need(sb["blocks"]["from"] == sa["tip"]["id"] + 1 and sb["blocks"]["to"] == sb["tipT1"]["id"], f"blocks {sb['blocks']}")
        need(sb["configId"].lower() == idn["configId"].lower() and sb["vault"].lower() == vault.lower(), "configId/vault")
        need(len(notes) == 2 and sorted(x["kind"] for x in notes) == ["private", "public"], "burn notes")
        for x in notes:
            need(x["configId"].lower() == idn["configId"].lower() and x["vault"].lower() == vault.lower(), "note identity")
            need(x["recipient"].lower() == deployer.lower(), "note recipient")
        kinds = sorted(c.get("kind", "?") for c in sb.get("claims", []))
        return f"T0 {sa['tip']['id']} -> T1 {sb['tipT1']['id']}, {len(sb.get('steps', []))} steps, claims {kinds}, 2 burn notes"

    @check("checkpoint journal fields")
    def _():
        need(len(jw) == 17, f"{len(jw)} words")
        need(jw[0] == CHECKPOINT_DOMAIN and int(jw[1], 16) == CHAIN_ID, "domain or chain id")
        need(jw[2][-40:] == vault.lower()[2:] and jw[3] == idn["configId"].lower(), "vault or configId")
        need(int(jw[4], 16) == 1 and int(jw[5], 16) == 1, "nMin, tMin")
        need(jw[6] == idn["genesisState"].lower(), "parent state is not genesisState")
        need(int(jw[13], 16) == sb["tipT1"]["id"] and jw[14] == sb["tipT1"]["hash"].lower(), "tip is not T1")
        return f"parent = genesisState, tip T1 {int(jw[13], 16)}, burns {int(jw[9], 16)}, imported leaves {int(jw[10], 16)}"

    @check("checkpoint final on the vault, its seal verified by RISC Zero's verifier (eth_call)")
    def _():
        need(call("finalState()(bytes32)").lower() == jw[7], "finalState is not the candidate's new state")
        need(first_int(call("checkpointCount()(uint256)")) >= 1, "no checkpoint")
        dig = h(hashlib.sha256(b(crown["candidate"]["journal"])).digest())
        seal = crown["candidate"]["seal"]
        need(cast_ok("call", idn["groth16Verifier"], "verify(bytes,bytes32,bytes32)", seal, IDS["checkpoint_b"], dig),
             "the verifier rejects the seal")
        bad = seal[:-2] + ("00" if seal[-2:] != "00" else "01")
        need(not cast_ok("call", idn["groth16Verifier"], "verify(bytes,bytes32,bytes32)", bad, IDS["checkpoint_b"], dig),
             "the verifier accepts a changed seal (control)")
        acc = call("burnRootStatus(bytes32)(bool,uint256)", jw[8]).splitlines()[0]
        need(acc.strip() == "true", "the burn root is not accepted")
        return f"final state {jw[7]}, seal accepted (a changed seal refused), burn root {jw[8]} accepted"

    @check("releases: paid once, seals verified on chain, events")
    def _():
        out = []
        logs = json.loads(cast("logs", "--json", "--from-block", str(rj.get("deploy_from_block") or 0), "--address", vault,
                               "Release(bytes32 indexed,address indexed,address indexed,uint256,address,uint256,bytes32)"))
        by_null = {lg["topics"][1].lower(): lg for lg in logs}
        for i, r in enumerate(crown["releases"]):
            meta = jl(f"{R}/releases/release-{r['name']}.json")
            need(r["burnRoot"].lower() == jw[8], f"release {i}: burn root")
            need(call("nullifierUsed(bytes32)(bool)", r["nullifier"]) == "true", f"release {i}: nullifier not used")
            dig = h(hashlib.sha256(b(meta["journal"])).digest())
            need(cast_ok("call", idn["groth16Verifier"], "verify(bytes,bytes32,bytes32)", r["seal"], IDS["release"], dig),
                 f"release {i}: the verifier rejects the seal")
            lg = by_null.get(r["nullifier"].lower())
            need(lg is not None, f"release {i}: no Release log")
            need("0x" + lg["topics"][3][-40:] == r["recipient"].lower(), f"release {i}: recipient")
            out.append(f"{r['name']}: {int(words(lg['data'])[0], 16)} wei to {r['recipient']}")
        need(len(logs) == len(crown["releases"]) == 2, f"{len(logs)} Release logs")
        return "; ".join(out) + f"; vault balance now {cast('balance', vault)} wei"

    if a.realblocks and a.e2e and a.channel_key and os.path.isfile(a.channel_key):
        @check("checkpoint input rebuilt from the node's recorded blocks = the proven input; guest re-executed")
        def _():
            name = f"crown-{sa['tip']['id']}-{sb['tipT1']['id']}"
            with tempfile.TemporaryDirectory(dir=R) as t:
                p = subprocess.run([a.realblocks, "--node", f"{L}/session-b/node", "--channel-key", a.channel_key,
                                    "--genesis", f"{L}/session-a/deployment-settings.yaml", "--session-a", f"{L}/session-a/session-a.json",
                                    "--session-b", f"{L}/session-b/session-b.json", "--vault", vault, "--config-id", idn["configId"],
                                    "--genesis-state", idn["genesisState"], "--name", name, "--out", t], capture_output=True, text=True)
                need(p.returncode == 0, f"realblocks-crown: {p.stderr.strip()[-300:]}")
                new, old = sha256f(f"{t}/{name}.input"), sha256f(f"{R}/checkpoint/input/{name}.input")
                need(new == old, f"rebuilt input {new} != proven {old}")
                e = subprocess.run([a.e2e, "exec", "--program", f"{R}/programs/checkpoint_b.bin", "--input", f"{t}/{name}.input",
                                    "--meta", f"{t}/{name}.json"], capture_output=True, text=True)
                need(e.returncode == 0, f"exec: {e.stderr.strip()[-300:]}")
                need(jl(f"{t}/{name}.json")["journal"] == crown["candidate"]["journal"][2:], "the native journal is not the candidate's")
                return f"input sha256 {new[:16]}... = proven; executed again: {e.stdout.strip()[-160:]}"

        @check("Groth16 receipts verify locally (ferrata-e2e verify)")
        def _():
            name = f"crown-{sa['tip']['id']}-{sb['tipT1']['id']}"
            rows = [(f"{R}/checkpoint/receipt/receipt-groth16.bin", IDS["checkpoint_b"], f"{R}/checkpoint/input/{name}.json")]
            rows += [(f"{R}/releases/{r['name']}/receipt/receipt-groth16.bin", IDS["release"], f"{R}/releases/release-{r['name']}.json")
                     for r in crown["releases"]]
            for rec, iid, meta in rows:
                p = subprocess.run([a.e2e, "verify", "--receipt", rec, "--image-id", iid, "--meta", meta], capture_output=True, text=True)
                need(p.returncode == 0, f"{os.path.relpath(rec, R)}: {p.stderr.strip()[-200:]}")
            return f"{len(rows)} receipts"
    else:
        print("SKIP the rebuild of the checkpoint input and the local receipt checks (they need --e2e, --realblocks and the"
              " channel key: the cloud runner's verify stage does them)")

    bad = [n for ok, n, _ in RESULTS if not ok]
    print(f"{len(RESULTS) - len(bad)} of {len(RESULTS)} checks passed" + (f"; FAILED: {', '.join(bad)}" if bad else ""))
    return 0 if not bad else 1


if __name__ == "__main__":
    sys.exit(main())
