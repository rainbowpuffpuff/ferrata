"""Writes vault/test/real/fixtures/real-blocks.json from the real-blocks run: the fixture's meta and attestor
record (checkpoint/realblocks/out), the lab's block table (blocks.tsv) and the Groth16 wrap
(gpu-prover/results/realblocks-l4/real-lab-8-202/). Usage: python3 -I vault-fixture.py <repo root>"""
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
out = root / "checkpoint/realblocks/out"
run = root / "gpu-prover/results/realblocks-l4/real-lab-8-202"
meta = json.loads((out / "real-lab-8-202.json").read_text())
att = json.loads((out / "attestor.json").read_text())
rows = {int(r.split("\t")[0]): r.split("\t") for r in (out / "blocks.tsv").read_text().splitlines()[1:]}
journal = (run / "groth16/journal.hex").read_text().strip().removeprefix("0x")
assert journal == meta["journal"], "the wrapped journal is not the native one"
sha = lambda p: "0x" + hashlib.sha256(p.read_bytes()).hexdigest()
parent = meta["first_block"] - 1
fx = {
    "description": "Rule (B) checkpoint over the lab's real LEZ blocks 8..=202 (lez/lab: LEZ v0.4.0-rc1 sequencer, one "
                   "Logos Blockchain validator, leader proofs in dev mode), as inscribed; tip 202 attested by a key the "
                   "lab's channel key certified (n = 1, t = 1); guest checkpoint_b proven on one NVIDIA L4 "
                   "(gpu-prover/results/realblocks-l4), wrapped to Groth16 (identity_p254 on the build box, RISC Zero's "
                   "Groth16 prover image through mwrap)",
    "imageId": "0xd359754364c10357103359276867f4fe58842c42cbb30a3ada5b241de6aa0d64",
    "journal": "0x" + journal,
    "journalSha256": "0x" + hashlib.sha256(bytes.fromhex(journal)).hexdigest(),
    "seal": "0x" + (run / "groth16/seal.hex").read_text().strip().removeprefix("0x"),
    "succinctReceiptSha256": sha(run / "receipt-succinct.bin"),
    "groth16ReceiptSha256": sha(run / "groth16/receipt-groth16.bin"),
    "labChannelKey": "0x" + att["channel_key"],
    "attestationKey": "0x" + att["attestation_key"],
    "postingTimeframe": att["posting_timeframe"],
    "configTipHash": "0x" + att["config_tip_hash"],
    "parentTipId": parent,
    "parentTipHash": "0x" + rows[parent][1],
    "tipId": att["tip_id"],
    "tipHash": "0x" + att["tip_hash"],
    "finalMsgId": "0x" + att["final_msg_id"],
    "certificate": "0x" + att["certificate"],
    "attestationSignature": "0x" + att["signature"],
    "blocks": meta["blocks"],
    "transactions": meta["transactions"],
}
(root / "vault/test/real/fixtures/real-blocks.json").write_text(json.dumps(fx, indent=2) + "\n")
print("seal bytes", len(bytes.fromhex(fx["seal"][2:])), "journal sha256", fx["journalSha256"])
