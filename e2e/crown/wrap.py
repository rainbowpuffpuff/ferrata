#!/usr/bin/env python3
"""e2e/crown/wrap.py <input.json> <proof.json> (crown run, stages 7 and 9): the Groth16 step (RISC Zero's prover image) through the build box app's
groth16_wrap, as spike/modal-build/mwrap does, on the profile FERRATA_WRAP_PROFILE (required; mwrap
hard-codes config.json's). Appends to spike/modal-build/ledger.tsv in mwrap's format."""
import json, os, sys, time
# spike/<worktree>/e2e/crown/wrap.py -> spike/
SPIKE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
profile = os.environ.get("FERRATA_WRAP_PROFILE") or sys.exit("set FERRATA_WRAP_PROFILE to your Modal profile")
os.environ["MODAL_PROFILE"] = profile
os.environ["TMPDIR"] = os.path.join(SPIKE, "tmp", "modal-build")
os.makedirs(os.environ["TMPDIR"], exist_ok=True)
import modal
out = modal.Function.from_name("ferrata-build", "groth16_wrap").remote(open(sys.argv[1], "rb").read())
sys.stderr.write(out["log"].decode("utf-8", "replace")[-1500:] + "\n")
cost = max(out["cpu_s"], 2.0 * out["seconds"]) * 0.0000131 + 16.0 * out["seconds"] * 0.00000222 + 0.01
with open(os.path.join(SPIKE, "modal-build", "ledger.tsv"), "a") as f:
    f.write("\t".join([time.strftime("%Y-%m-%dT%H:%M:%S"), profile, "groth16-wrap", f"{out['seconds']:.0f}",
                       f"{out['cpu_s']:.0f}", str(out["exit_code"]), f"{cost:.4f}", os.path.basename(sys.argv[1])]) + "\n")
print(f"[wrap] {profile}: exit {out['exit_code']}, {out['seconds']:.0f} s, {out['cpu_s']:.0f} CPU-s, about {cost:.2f} USD", file=sys.stderr)
if out["exit_code"] != 0 or not out["proof_json"]:
    sys.exit(1)
open(sys.argv[2], "wb").write(out["proof_json"])
