#!/usr/bin/env bash
# The stand-in prover behind the prover client in the robust run (hook-robust.sh, R.06), run by
# deploy/testnet/tests/standin_modal/server.py for each spawned call: what gpu-prover/modal_app.py's prove_bytes does on
# a GPU, without a proof: checkpoint_b run by RISC Zero's executor over the call's input (ferrata-e2e exec: its journal
# must equal the native one), then the run's mock verifier told to accept that journal (as common.sh's stand-in prover
# does); the answer carries the mock's seal. From the server: PROGRAM INPUT META NAME; from the hook: BIN RPC MOCK
# DEPLOYER CHECKPOINT_B_ID STANDIN_DELAY (seconds the "GPU" takes before the executor runs).
set -euo pipefail
d="$(dirname "$META")"
jq '.kind = (if .kind == "checkpoint-b" then "checkpoint" else .kind end)' "$META" > "$d/meta-exec.json"
sleep "${STANDIN_DELAY:-0}"
"$BIN/ferrata-e2e" exec --program "$PROGRAM" --input "$INPUT" --meta "$d/meta-exec.json" > "$d/exec.txt" 2>&1 \
  || { tail -n 5 "$d/exec.txt" >&2; exit 1; }
j="$(jq -r .journal "$META")"; j="${j#0x}"
dig="0x$(python3 -I -c 'import hashlib, sys; print(hashlib.sha256(bytes.fromhex(sys.argv[1])).hexdigest())' "$j")"
cast send "$MOCK" "accept(bytes32,bytes32)" "$CHECKPOINT_B_ID" "$dig" --rpc-url "$RPC" --unlocked --from "$DEPLOYER" > "$d/accept.txt" 2>&1 \
  || { tail -n 5 "$d/accept.txt" >&2; exit 1; }
printf '{"ok": true, "journal_hex": "%s", "seal_hex": "5ea15ea1", "exit_code": 0, "exec": %s}\n' "$j" "$(tail -n 1 "$d/exec.txt")"
