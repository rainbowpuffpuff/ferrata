#!/usr/bin/env bash
# After attestor/devnet.sh (run from checkpoint/ on the build box): the executor over the collected and
# the refused inputs, a succinct proof of rbm-lab-n3-t2 on the CPU, and the identity_p254 step of its
# Groth16 wrap (bridge/'s ferrata-e2e; the Groth16 step runs in RISC Zero's prover image, groth16_wrap).
cd "$(dirname "$0")/.."
unset RISC0_DEV_MODE
echo "== executor: accepted inputs"
target/release/host bench rbm-lab-n3-t2 rbm-lab-n3-all 2>&1 | tail -n 6
echo "== executor: inputs the guest must refuse"
for f in too-few none uncertified other-vault other-chain outsider duplicate; do
  echo "-- rbm-refuse-$f"; target/release/host bench rbm-refuse-$f 2>&1 | grep -E -o "checkpoint refused: [A-Za-z]+[^\"]{0,40}|Error: .{0,160}" | head -2
done
echo "== succinct proof (CPU, $(nproc) threads)"
target/release/host prove rbm-lab-n3-t2 2>&1 | tail -n 4
echo "== identity_p254 (first step of the Groth16 wrap)"
../bridge/target/release/ferrata-e2e p254 --receipt artifacts/rbm-lab-n3-t2/receipt-succinct.bin \
  --image-id 0x82dec3b8c242297e236d6862f74c4b29428e961bb539c84058fd0922b188e464 --out-dir artifacts/rbm-lab-n3-t2/wrap 2>&1 | tail -n 3
ls -la artifacts/rbm-lab-n3-t2 artifacts/rbm-lab-n3-t2/wrap
sha256sum artifacts/rbm-lab-n3-t2/receipt-succinct.bin
