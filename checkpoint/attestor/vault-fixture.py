"""Writes a vault fixture from the devnet run (attestor/runs/devnet-n3) and the Groth16 wrap of one of its proofs
(artifacts/<name>/groth16, written by bridge/'s ferrata-e2e groth16), and copies the wrap's small outputs to
attestor/runs/devnet-n3/<proof dir>/. Usage, from checkpoint/:
    python3 -I attestor/vault-fixture.py [<name> <fixture file name> <proof dir>]
(default: rbm-lab-n3-t2 rule-b-multi.json proof)"""
import hashlib, json, pathlib, shutil, sys

name, fixture, proof_dir = (sys.argv[1:4] if len(sys.argv) == 4 else ["rbm-lab-n3-t2", "rule-b-multi.json", "proof"])
here = pathlib.Path.cwd()
run = here / "attestor/runs/devnet-n3"
art = here / "artifacts" / name
meta = json.loads((run / f"{name}.json").read_text())
journal = (art / "groth16/journal.hex").read_text().strip().removeprefix("0x")
assert journal == meta["journal"], "the wrapped journal is not the native one"
sha = lambda p: "0x" + hashlib.sha256(p.read_bytes()).hexdigest()
DESCRIPTIONS = {"rbm-lab-rot": "Rule (B) checkpoint with t of n and a key rotation: the lab's real LEZ blocks 186..=202 "
    "(LEZ v0.4.0-rc1, one sequencer); stand-in parent key set {the lab sequencer's key (producer of T = 202), k1, k2} "
    "with a stand-in posting timeframe of 20, rotated in the proof to {lab, k1, k3} by a configuration transaction "
    "signed by k1 and k2 (2 of 3); nMin 3, tMin 2; two attestations under the new set collected by ferrata_attestor; "
    "guest checkpoint_b proven on the build box's CPU, wrapped to Groth16. checkpoint/attestor/RESULTS.md"}
fx = {
    "description": "Rule (B) checkpoint with t of n: the lab's real LEZ blocks 186..=202 (LEZ v0.4.0-rc1, one sequencer) "
                   "under a stand-in key set of three (the lab sequencer's key, the real producer of T = 202, and two "
                   "seeded sequencer keys), nMin 3, tMin 2; two attestations collected by ferrata_attestor (the lab's "
                   "attestor with its real certificate, and attestor 1; sequencer 2 silent); guest checkpoint_b proven on "
                   "the build box's CPU, wrapped to Groth16 (identity_p254 on the box, RISC Zero's Groth16 prover image "
                   "on Modal). checkpoint/attestor/RESULTS.md",
    "imageId": "0x82dec3b8c242297e236d6862f74c4b29428e961bb539c84058fd0922b188e464",
    "journal": "0x" + journal,
    "journalSha256": "0x" + hashlib.sha256(bytes.fromhex(journal)).hexdigest(),
    "seal": "0x" + (art / "groth16/seal.hex").read_text().strip().removeprefix("0x"),
    "succinctReceiptSha256": sha(art / "receipt-succinct.bin"),
    "groth16ReceiptSha256": sha(art / "groth16/receipt-groth16.bin"),
    "keys": ["0x" + k for k in meta["keys"]],
    "attestors": ["0x" + a["channel_key"] for a in meta["attestations"]],
    "postingTimeframe": meta["posting_timeframe"],
    "configTipHash": "0x" + meta["config_tip_hash"],
    "parentTipId": meta["parent_tip_id"],
    "parentTipHash": "0x" + meta["parent_tip_hash"],
    "tipId": meta["tip"],
    "tipHash": "0x" + meta["tip_hash"],
    "n": meta["n"],
    "t": meta["t"],
    "blocks": meta["blocks"],
    "transactions": meta["transactions"],
}
if name in DESCRIPTIONS:
    fx["description"] = DESCRIPTIONS[name]
if name == "rbm-lab-rot":
    # The set after the rotation: the one the attestations were signed over (att-rot-*.json's key_set).
    fx["rotatedKeys"] = ["0x" + k for k in json.loads((run / "att-rot-0.json").read_text())["key_set"]]
out = here / "../vault/test/real/fixtures" / fixture
out.write_text(json.dumps(fx, indent=2) + "\n")
proof = run / proof_dir
proof.mkdir(exist_ok=True)
for f in ["groth16/seal.hex", "groth16/journal.hex", "groth16/receipt-groth16.bin", "wrap/p254.json", "wrap/proof.json"]:
    shutil.copy(art / f, proof / pathlib.Path(f).name)
(proof / "receipt-succinct.sha256").write_text(fx["succinctReceiptSha256"].removeprefix("0x") + "  receipt-succinct.bin\n")
print("seal bytes", len(bytes.fromhex(fx["seal"][2:])), "journal sha256", fx["journalSha256"])
