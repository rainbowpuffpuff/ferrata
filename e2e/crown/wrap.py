#!/usr/bin/env python3
"""e2e/crown/wrap.py <input.json> <proof.json> (crown run, stages 7 and 9): the Groth16 step (RISC Zero's prover image)
through e2e/crown/groth16_app.py's groth16_wrap (app ferrata-groth16: the image of RISC Zero 3.1, v2026-10-20.1), on the
profile FERRATA_WRAP_PROFILE (required). FERRATA_WRAP_APP names another deployed app with a groth16_wrap function
(ferrata-build: the build box's, the image of RISC Zero 3.0.x, for receipts of 3.0.5 only). Appends to
spike/modal-build/ledger.tsv in mwrap's format."""
import json, os, sys, time
# spike/<worktree>/e2e/crown/wrap.py -> spike/
SPIKE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
profile = os.environ.get("FERRATA_WRAP_PROFILE") or sys.exit("set FERRATA_WRAP_PROFILE to your Modal profile")
os.environ["MODAL_PROFILE"] = profile
os.environ["TMPDIR"] = os.path.join(SPIKE, "tmp", "modal-build")
os.makedirs(os.environ["TMPDIR"], exist_ok=True)
import modal
app_name = os.environ.get("FERRATA_WRAP_APP", "ferrata-groth16")
out = modal.Function.from_name(app_name, "groth16_wrap").remote(open(sys.argv[1], "rb").read())
sys.stderr.write(out["log"].decode("utf-8", "replace")[-1500:] + "\n")
cost = max(out["cpu_s"], 2.0 * out["seconds"]) * 0.0000131 + 16.0 * out["seconds"] * 0.00000222 + 0.01
with open(os.path.join(SPIKE, "modal-build", "ledger.tsv"), "a") as f:
    f.write("\t".join([time.strftime("%Y-%m-%dT%H:%M:%S"), profile, "groth16-wrap", f"{out['seconds']:.0f}",
                       f"{out['cpu_s']:.0f}", str(out["exit_code"]), f"{cost:.4f}", os.path.basename(sys.argv[1])]) + "\n")
print(f"[wrap] {profile} {app_name} {out.get('image', '')}: exit {out['exit_code']}, {out['seconds']:.0f} s, {out['cpu_s']:.0f} CPU-s, about {cost:.2f} USD", file=sys.stderr)
if out["exit_code"] != 0 or not out["proof_json"]:
    sys.exit(1)
open(sys.argv[2], "wb").write(out["proof_json"])
