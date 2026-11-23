"""ferrata-groth16: the Groth16 step of a RISC Zero 3.1 wrap, RISC Zero's own prover image run as a Modal function.

risc0-groth16 3.0.6 (risc0-zkvm 3.1.0) runs `docker run --rm -v <dir>:/mnt risczero/risc0-groth16-prover:v2026-10-20.1`
(`src/prove/docker.rs`): the image reads /mnt/input.json (the identity_p254 seal as JSON, `ferrata-e2e p254` writes it)
and writes /mnt/proof.json (`ferrata-e2e groth16` turns it into the Groth16 receipt and the 260-byte seal). The same
image runs here as a plain function, without Docker: no wrap needs the laptop, and the image is x86-only anyway.
The image carries the trusted setup of RISC Zero 3.1 (GHSA-g6qg-9j8p-7j87); a proof made with the image of 3.0.x
(`v2025-04-03.1`) does not verify under verifier 3.1.0.

    MODAL_PROFILE=<your profile> modal deploy e2e/crown/groth16_app.py      # once per workspace
    FERRATA_WRAP_PROFILE=<your profile> e2e/crown/wrap.py input.json proof.json

Never `modal profile activate`: the profile is given per command. No secret is attached; nothing here signs or sends.
"""

from __future__ import annotations

import os
import subprocess
import time

import modal

APP_NAME = "ferrata-groth16"
# risc0-groth16 3.0.6, src/prove/docker.rs; pinned by its digest (Docker Hub's index of the tag, read 23 November 2026).
PROVER_IMAGE = "risczero/risc0-groth16-prover:v2026-10-20.1@sha256:e8c2068fb34bbc41447255aa980a89cbb7d6e5a80c0d66041264431f2a955e47"

image = modal.Image.from_registry(PROVER_IMAGE, add_python="3.12").entrypoint([])
app = modal.App(APP_NAME)


def _script() -> str:
    """The image's prover script: /app/prover.sh in the images of RISC Zero 3.0 and 3.1."""
    for p in ("/app/prover.sh", "/app/scripts/prover.sh"):
        if os.path.exists(p):
            return p
    raise FileNotFoundError("no prover script in the image: " + " ".join(sorted(os.listdir("/app"))[:40]))


@app.function(image=image, cpu=(2.0, 16.0), memory=(8192, 32768), timeout=1800, max_containers=4, retries=0,
              single_use_containers=True)
def groth16_wrap(input_json: bytes) -> dict:
    t0 = time.time()
    os.makedirs("/mnt", exist_ok=True)
    for stale in ("/mnt/proof.json", "/tmp/output.wtns"):
        if os.path.lexists(stale):
            os.remove(stale)
    with open("/mnt/input.json", "wb") as f:
        f.write(input_json)
    script = _script()
    p = subprocess.run(["bash", script], cwd=os.path.dirname(script), capture_output=True)
    proof = b""
    if os.path.exists("/mnt/proof.json"):
        with open("/mnt/proof.json", "rb") as f:
            proof = f.read()
    t = os.times()
    return {"exit_code": p.returncode, "proof_json": proof, "seconds": time.time() - t0, "image": PROVER_IMAGE,
            "cpu_s": t.children_user + t.children_system, "log": (p.stdout + p.stderr)[-6000:]}
