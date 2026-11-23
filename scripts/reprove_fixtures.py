#!/usr/bin/env python3
"""Prove the vault's real-proof fixtures again under RISC Zero 3.1 from their recorded inputs
(docs/design/risc0-3.1-migration.md, phase 1). Receipts of risc0 3.0.5 do not verify under verifier 3.1.0, so every
committed seal is made again; the journals do not change (the guests' sources did not), and each new receipt is held to
the journal its run recorded.

    python3 -I scripts/reprove_fixtures.py programs WORK
        the 3.1 guest programs (R0BF .bin) of checkpoint/target and bridge/target into WORK/programs, with their ids
    python3 -I scripts/reprove_fixtures.py survey WORK INPUT...
        executes each recorded input (INPUT.input beside its meta INPUT.json) with every program of its kind and
        writes WORK/survey.jsonl: which program reproduces the recorded journal (r0vm's executor, no proof)
    python3 -I scripts/reprove_fixtures.py prove WORK JOBS.json [ID...]
        per job: the succinct proof (CPU, r0vm), its verification, identity_p254 and the Groth16 prover's input.json
    python3 -I scripts/reprove_fixtures.py wrap WORK JOBS.json [ID...]
        per job: RISC Zero's Groth16 prover image on Modal (e2e/crown/wrap.py; FERRATA_WRAP_PROFILE) -> proof.json
    python3 -I scripts/reprove_fixtures.py groth16 WORK JOBS.json [ID...]
        per job: the Groth16 receipt from proof.json, verified against the program's id and the recorded journal
    python3 -I scripts/reprove_fixtures.py apply WORK JOBS.json
        writes each new seal and image id into its fixture (vault/test/real/fixtures/*.json) and WORK/records/

JOBS.json: {"jobs": [{"id", "program", "input", "meta", "fixture", "seal": <JSON path>, "image": <JSON path>}]}, paths
relative to the repository; a JSON path is dot-separated keys and [i] indices ("releases[0].seal"). ferrata-e2e
(bridge/, built on risc0-zkvm 3.1.0) does every step but the wrap; r0vm 3.1.0 is RISC0_SERVER_PATH or the first
r0vm on PATH, and must say 3.1.0.
"""
import hashlib
import json
import os
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
E2E = os.path.join(ROOT, "bridge", "target", "release", "ferrata-e2e")
KINDS = {"release": ["release"], "checkpoint": ["checkpoint", "checkpoint_0_3_5"],
         "checkpoint-b": ["checkpoint_b", "checkpoint_b_0_3_5"]}
GUESTS = ("checkpoint", "checkpoint_b", "checkpoint_0_3_5", "checkpoint_b_0_3_5", "segment", "release", "claim",
          "gasdrop", "micro")


def die(msg):
    print(f"reprove_fixtures: {msg}", file=sys.stderr)
    sys.exit(1)


def run(args, log=None):
    p = subprocess.run(args, capture_output=True, text=True)
    if log:
        with open(log, "a") as f:
            f.write(f"$ {' '.join(args)}\n{p.stdout}{p.stderr}\nexit {p.returncode}\n")
    return p


def last_json(text):
    for line in reversed(text.strip().splitlines()):
        if line.startswith("{"):
            return json.loads(line)
    return None


def r0vm_version():
    exe = os.environ.get("RISC0_SERVER_PATH") or "r0vm"
    p = subprocess.run([exe, "--version"], capture_output=True, text=True)
    return p.stdout.strip()


def programs(work):
    out = os.path.join(work, "programs")
    os.makedirs(out, exist_ok=True)
    found = {}
    for ws in ("checkpoint", "bridge"):
        base = os.path.join(ROOT, ws, "target", "riscv-guest")
        for dirpath, _, files in os.walk(base):
            if "riscv32im-risc0-zkvm-elf/release" not in dirpath:
                continue
            for f in files:
                name = f[:-4]
                if f.endswith(".bin") and name in GUESTS:
                    found.setdefault(name, []).append(os.path.join(dirpath, f))
    ids = []
    for name in sorted(found):
        paths = sorted(found[name], key=os.path.getmtime)
        digests = {hashlib.sha256(open(p, "rb").read()).hexdigest() for p in paths}
        if len(digests) != 1:
            die(f"{name}: {len(digests)} different builds in the target directories; build in a fresh one")
        dst = os.path.join(out, f"{name}.bin")
        with open(paths[-1], "rb") as a, open(dst, "wb") as b:
            b.write(a.read())
        rec = last_json(run([E2E, "image-id", dst]).stdout)
        rec["program"] = f"{name}.bin"
        ids.append(rec)
    with open(os.path.join(out, "ids.jsonl"), "w") as f:
        for r in ids:
            f.write(json.dumps(r) + "\n")
    for r in ids:
        print(r["program"], r["image_id"])


def ids_of(work):
    return {json.loads(l)["program"][:-4]: json.loads(l)["image_id"]
            for l in open(os.path.join(work, "programs", "ids.jsonl"))}


def survey(work, inputs):
    rows = []
    for inp in inputs:
        base = inp[:-6] if inp.endswith(".input") else inp
        meta = json.load(open(base + ".json"))
        row = {"input": os.path.relpath(base + ".input", ROOT), "kind": meta.get("kind"), "matches": []}
        for prog in KINDS.get(meta.get("kind"), []):
            p = run([E2E, "exec", "--program", os.path.join(work, "programs", prog + ".bin"), "--input", base + ".input",
                     "--meta", base + ".json"])
            rec = last_json(p.stdout) if p.returncode == 0 else None
            if rec:
                row["matches"].append({"program": prog, "user_cycles": rec["user_cycles"], "segments": rec["segments"]})
            else:
                row.setdefault("refusals", {})[prog] = (p.stderr.strip().splitlines() or ["?"])[-1][:200]
        rows.append(row)
        print(json.dumps(row))
    with open(os.path.join(work, "survey.jsonl"), "w") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")


def load_jobs(path, only):
    jobs = json.load(open(path))["jobs"]
    if only:
        jobs = [j for j in jobs if j["id"] in only]
        if not jobs:
            die("no job of that id")
    return jobs


def jdir(work, j):
    d = os.path.join(work, "jobs", j["id"])
    os.makedirs(d, exist_ok=True)
    return d


def prove_one(work, j, ids):
    d = jdir(work, j)
    log = os.path.join(d, "steps.log")
    prog = os.path.join(work, "programs", j["program"] + ".bin")
    meta = os.path.join(ROOT, j["meta"])
    if not os.path.exists(os.path.join(d, "receipt-succinct.bin")):
        p = run([E2E, "prove", "--program", prog, "--input", os.path.join(ROOT, j["input"]), "--meta", meta,
                 "--out-dir", d], log)
        if p.returncode:
            return f"{j['id']}: prove failed ({p.stderr.strip()[-300:]})"
    if not os.path.exists(os.path.join(d, "input.json")):
        p = run([E2E, "p254", "--receipt", os.path.join(d, "receipt-succinct.bin"), "--image-id", ids[j["program"]],
                 "--out-dir", d], log)
        if p.returncode:
            return f"{j['id']}: p254 failed ({p.stderr.strip()[-300:]})"
    pr = json.load(open(os.path.join(d, "prove.json")))
    p2 = json.load(open(os.path.join(d, "p254.json")))
    return f"{j['id']}: proven, {pr['user_cycles']} user cycles, {pr['segments']} segments, {pr['wall_s']:.1f} s; " \
           f"p254 {p2['wall_s']:.1f} s"


def prove(work, jobs):
    ids = ids_of(work)
    v = r0vm_version()
    if "3.1.0" not in v:
        die(f"r0vm says {v!r}, not 3.1.0")
    for j in jobs:  # one at a time: each proof uses every core
        print(prove_one(work, j, ids), flush=True)


def wrap(work, jobs):
    wrapper = os.path.join(ROOT, "e2e", "crown", "wrap.py")

    def one(j):
        d = jdir(work, j)
        if os.path.exists(os.path.join(d, "proof.json")):
            return f"{j['id']}: proof.json present"
        if not os.path.exists(os.path.join(d, "input.json")):
            return f"{j['id']}: no input.json (prove first)"
        p = run([sys.executable, "-I", wrapper, os.path.join(d, "input.json"), os.path.join(d, "proof.json")],
                os.path.join(d, "wrap.log"))
        tail = (p.stderr.strip().splitlines() or ["?"])[-1]
        return f"{j['id']}: wrap exit {p.returncode}: {tail}"

    with ThreadPoolExecutor(max_workers=4) as pool:
        for line in pool.map(one, jobs):
            print(line, flush=True)


def groth16(work, jobs):
    ids = ids_of(work)
    for j in jobs:
        d = jdir(work, j)
        log = os.path.join(d, "steps.log")
        p = run([E2E, "groth16", "--receipt", os.path.join(d, "receipt-succinct.bin"), "--image-id", ids[j["program"]],
                 "--proof", os.path.join(d, "proof.json"), "--out-dir", d], log)
        if p.returncode:
            print(f"{j['id']}: groth16 failed ({p.stderr.strip()[-300:]})")
            continue
        p = run([E2E, "verify", "--receipt", os.path.join(d, "receipt-groth16.bin"), "--image-id", ids[j["program"]],
                 "--meta", os.path.join(ROOT, j["meta"])], log)
        g = json.load(open(os.path.join(d, "groth16.json")))
        print(f"{j['id']}: groth16 selector {g['selector']}, verify exit {p.returncode}")


def path_parts(path):
    return [int(x) if x.isdigit() else x for x in re.findall(r"[^.\[\]]+", path)]


def put(obj, path, value):
    parts = path_parts(path)
    for k in parts[:-1]:
        obj = obj[k]
    old = obj[parts[-1]]
    obj[parts[-1]] = value
    return old


def get(obj, path):
    for k in path_parts(path):
        obj = obj[k]
    return obj


def apply(work, jobs):
    ids = ids_of(work)
    records = os.path.join(work, "records")
    os.makedirs(records, exist_ok=True)
    fixtures = {}
    for j in jobs:
        d = jdir(work, j)
        g = json.load(open(os.path.join(d, "groth16.json")))
        if not g.get("verifies") or g["image_id"] != ids[j["program"]]:
            die(f"{j['id']}: no verified Groth16 receipt of {j['program']}")
        seal = "0x" + open(os.path.join(d, "seal.hex")).read().strip().removeprefix("0x")
        journal = open(os.path.join(d, "journal.hex")).read().strip().removeprefix("0x")
        if journal != json.load(open(os.path.join(ROOT, j["meta"])))["journal"].removeprefix("0x"):
            die(f"{j['id']}: the receipt's journal is not the recorded one")
        fx = fixtures.setdefault(j["fixture"], json.load(open(os.path.join(ROOT, j["fixture"]))))
        old_seal = put(fx, j["seal"], seal)
        old_id = None
        if j.get("image"):
            prev = get(fx, j["image"])
            if prev not in (ids[j["program"]],) and not str(prev).startswith("0x"):
                die(f"{j['id']}: {j['image']} is not an id")
            old_id = put(fx, j["image"], ids[j["program"]])
        pr = json.load(open(os.path.join(d, "prove.json")))
        p2 = json.load(open(os.path.join(d, "p254.json")))
        rec = {"id": j["id"], "fixture": j["fixture"], "seal_path": j["seal"], "program": j["program"],
               "image_id": ids[j["program"]], "previous_image_id": old_id, "previous_selector": old_seal[:10],
               "selector": seal[:10], "journal_sha256": g["journal_sha256"], "input": j["input"],
               "input_sha256": pr["input_sha256"], "risc0_zkvm": pr["risc0_zkvm"], "user_cycles": pr["user_cycles"],
               "segments": pr["segments"], "prove_wall_s": round(pr["wall_s"], 1), "threads": pr["threads"],
               "p254_wall_s": round(p2["wall_s"], 1), "groth16_receipt_bytes": g["receipt_bytes"]}
        with open(os.path.join(records, j["id"] + ".json"), "w") as f:
            json.dump(rec, f, indent=1)
    for path, fx in fixtures.items():
        text = json.dumps(fx, indent=2) + "\n"
        with open(os.path.join(ROOT, path), "w") as f:
            f.write(text)
        print(f"wrote {path}")


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(2)
    cmd, work = sys.argv[1], os.path.abspath(sys.argv[2])
    os.makedirs(work, exist_ok=True)
    if cmd == "programs":
        programs(work)
    elif cmd == "survey":
        survey(work, sys.argv[3:])
    elif cmd in ("prove", "wrap", "groth16", "apply"):
        jobs = load_jobs(sys.argv[3], sys.argv[4:])
        {"prove": prove, "wrap": wrap, "groth16": groth16, "apply": apply}[cmd](work, jobs)
    else:
        die(f"unknown command {cmd}")


if __name__ == "__main__":
    main()
