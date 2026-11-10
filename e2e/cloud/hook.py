#!/usr/bin/env python3
"""The driver's hooks inside the cloud runner (e2e/cloud/app.py writes the wrappers that call this).

    hook.py gpu <inputs dir> <proof> <job> <out dir>   FERRATA_GPU_CMD: gpu_prove on one L4; leaves its
                                                       files (prove.json, receipt-succinct.bin, run.json,
                                                       prover.log, ...) in <out dir>
    hook.py wrap <input.json> <proof.json>             FERRATA_MWRAP: groth16_wrap, RISC Zero's prover image
    hook.py ping                                       the app answers from inside the container

A GPU job that waits longer than FERRATA_GPU_WAIT_S (default 1800 s) for its result is cancelled and
queued again, at most FERRATA_GPU_ATTEMPTS times (default 3): a job once waited 42 minutes for an L4
that a second request got within five.
"""
import io
import json
import os
import sys
import tarfile
import time

import modal

APP_NAME = "ferrata-sepolia"


def say(msg: str) -> None:
    print(f"[{time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}] {msg}", flush=True)


def gpu(inputs: str, proof: str, job: str, out: str) -> int:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tf:
        tf.add(inputs, arcname=".")
    fn = modal.Function.from_name(APP_NAME, "gpu_prove")
    wait_s = int(os.environ.get("FERRATA_GPU_WAIT_S", "1800"))
    attempts = int(os.environ.get("FERRATA_GPU_ATTEMPTS", "3"))
    for attempt in range(1, attempts + 1):
        call = fn.spawn(buf.getvalue(), proof, job, 600)
        say(f"{job}: {proof} queued on gpu_prove, call {call.object_id} (attempt {attempt} of {attempts})")
        try:
            res = call.get(timeout=wait_s)
        except TimeoutError:
            say(f"no result after {wait_s} s: cancelling call {call.object_id}")
            call.cancel()
            continue
        os.makedirs(out, exist_ok=True)
        for name, data in res["files"].items():
            with open(os.path.join(out, name), "wb") as f:
                f.write(data)
        rec = res["record"]
        p = rec.get("prove") or {}
        say(f"RESULT {proof}: ok={rec['ok']} gpu={rec['gpu']} prover_wall_s={rec['prover_wall_s']:.1f}"
            f" total_s={p.get('total_s')} segments={p.get('segments')} image_ids_equal={rec['image_ids_equal']}"
            f" inputs_equal_manifest={rec['input_hashes_equal_manifest']} task={rec.get('task')}")
        return 0 if res["ok"] else 1
    say(f"no GPU result in {attempts} attempts")
    return 1


def wrap(input_json: str, proof_json: str) -> int:
    fn = modal.Function.from_name(APP_NAME, "groth16_wrap")
    with open(input_json, "rb") as f:
        out = fn.remote(f.read())
    sys.stderr.write(out["log"].decode("utf-8", "replace")[-3000:] + "\n")
    print(f"[wrap] exit {out['exit_code']}, {out['seconds']:.0f} s, {out['cpu_s']:.0f} CPU-s", file=sys.stderr)
    if out["exit_code"] != 0 or not out["proof_json"]:
        return 1
    with open(proof_json, "wb") as f:
        f.write(out["proof_json"])
    return 0


def main() -> int:
    args = sys.argv[1:]
    if args[:1] == ["gpu"] and len(args) == 5:
        return gpu(*args[1:])
    if args[:1] == ["wrap"] and len(args) == 3:
        return wrap(*args[1:])
    if args == ["ping"]:
        print(json.dumps(modal.Function.from_name(APP_NAME, "ping").remote()))
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
