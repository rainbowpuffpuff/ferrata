#!/usr/bin/env bash
# End to end through the modules: logs of a finalized mainnet block attested on LEZ.
#
#   ferrata_attest_module.fetch_log_proof   reads the block and its receipts over JSON-RPC from
#                                           recorded_rpc.py (the recorded mainnet block 26,384,851)
#   ferrata_attest_module.dry_run_attest    against the record the program holds (read from the chain)
#   ferrata_attest_module.build_attest_tx   the program's Attest message and its account
#   lez/harness module_e2e attest           decodes the bytes with the program's types and executes
#                                           them in LEZ v0.4.0-rc1's state machine (Stage 4 chain)
#   ferrata_attest_module.read_attestation  the program's event against the dry run's statement ids
#
# and the bridge module's messages (Burn, Claim from the specification's vectors) decoded with the
# bridge program's types. Two refusals: a tampered receipt proof (the program's exit code must be the
# code the dry run predicted) and a request naming another instance (WRONG_INSTANCE, 211, a check
# only the program makes).
#
#   modules/scripts/e2e.sh OUT_DIR                     the modules' methods called natively
#   CALL=logosctl LOGOSCTL=BIN SESSION=DIR modules/scripts/e2e.sh OUT_DIR
#                                                      through a running Logos Core daemon in which
#                                                      both modules are loaded
# Needs: the call examples (native) or logosctl, cargo (the harness), python3, jq.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd)
rm -rf "$out/req" "$out/rep"
mkdir -p "$out/req" "$out/rep"
CALL=${CALL:-native}
PORT=${PORT:-18545}
FIXTURE="$repo/crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10"
VECTORS="$repo/bridge/vectors"
export RISC0_HOME=${RISC0_HOME:-$repo/../risc0-home-lez} RISC0_DEV_MODE=1

# call MODULE METHOD NAME: request $out/req/NAME.json, reply $out/rep/NAME.json
call() {
  local t0 t1
  t0=$(date +%s.%N)
  case "$CALL" in
    native)
      "$repo/modules/$1/rust-lib/target/release/examples/call" "$2" "$out/req/$3.json" > "$out/rep/$3.json"
      ;;
    logosctl)
      "$LOGOSCTL" --config-dir "$SESSION" call "$1" "$2" "@$out/req/$3.json" --json < /dev/null > "$out/rep/$3.raw" 2>&1
      jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' \
        "$out/rep/$3.raw" > "$out/rep/$3.json"
      ;;
  esac
  t1=$(date +%s.%N)
  printf '%s\t%s\t%s\t%s\n' "$3" "$1" "$2" "$(awk -v a="$t0" -v b="$t1" 'BEGIN { printf "%.2f", b - a }')" >> "$out/timings.tsv"
}
harness() { (cd "$repo/lez" && cargo run --profile test -q -p harness --bin module_e2e -- "$@"); }
req() { cat > "$out/req/$1.json"; }
rep() { cat "$out/rep/$1.json"; }

printf 'name\tmodule\tmethod\tseconds\n' > "$out/timings.tsv"
echo "call mode: $CALL" > "$out/mode.txt"

python3 -I "$here/recorded_rpc.py" "$FIXTURE" "$PORT" "$out/rpc.log" &
rpc_pid=$!
trap 'kill $rpc_pid 2>/dev/null || true' EXIT
: > "$out/rpc.log"
for _ in $(seq 1 100); do (echo > "/dev/tcp/127.0.0.1/$PORT") 2>/dev/null && break; sleep 0.1; done

harness info > "$out/rep/info.json"
info() { jq -c "$1" "$out/rep/info.json"; }
block=$(info .block)

# One log case: NAME and the fetch selector (JSON object members).
log_case() {
  local name=$1 select=$2
  jq -n --arg url "http://127.0.0.1:$PORT" --argjson block "$block" "{rpc_url: \$url, block: \$block} + $select" | req "fetch_$name"
  call ferrata_attest_module fetch_log_proof "fetch_$name"
  jq -n --argjson i "$(rep info)" --argjson f "$(rep "fetch_$name")" \
    '{chain_id: $i.chain_id, record: $i.record, log: $f.log}' | req "dry_$name"
  call ferrata_attest_module dry_run_attest "dry_$name"
  jq -n --argjson i "$(rep info)" --argjson f "$(rep "fetch_$name")" \
    '{program_id: $i.program_id, instance_id: $i.instance_id, chain_id: $i.chain_id, slot: $i.finalized_slot, log: $f.log}' | req "tx_$name"
  call ferrata_attest_module build_attest_tx "tx_$name"
  harness attest "$(jq -r .message "$out/rep/tx_$name.json")" "$(jq -r .account "$out/rep/tx_$name.json")" > "$out/rep/exec_$name.json"
  jq -n --argjson i "$(rep info)" --argjson d "$(rep "dry_$name")" --argjson e "$(rep "exec_$name")" \
    '{selector: "0x4652542d41545431", event_data: $e.event_data, expected_ids: $d.ids, expected_instance_id: $i.instance_id}' | req "read_$name"
  call ferrata_attest_module read_attestation "read_$name"
}

# A USDC transfer (transaction 0, its first log) and a WETH transfer in the block's largest receipt
# (306 logs), the second chosen by JSON-RPC's block-wide logIndex.
log_case transfer '{tx_index: 0, log_index: 0}'
log_case largest_receipt '{block_log_index: 7}'

# A storage slot: WETH's balance slot of the fixture's holder, with the eth_getProof response the
# fixture recorded for the block (the storage path of build_attest_tx: one account, one slot).
weth_slot=$(jq -r .weth_balance_slot "$FIXTURE/fixture.json")
jq -n --argjson i "$(rep info)" --slurpfile p "$FIXTURE/proof_weth_$block.json" --arg slot "$weth_slot" \
  '{chain_id: $i.chain_id, record: $i.record, storage: {get_proof: ($p[0].result // $p[0]), slot: $slot}}' | req dry_storage
call ferrata_attest_module dry_run_attest dry_storage
jq -n --argjson i "$(rep info)" --slurpfile p "$FIXTURE/proof_weth_$block.json" --arg slot "$weth_slot" \
  '{program_id: $i.program_id, instance_id: $i.instance_id, chain_id: $i.chain_id, slot: $i.finalized_slot,
    storage: {get_proof: ($p[0].result // $p[0]), slot: $slot}}' | req tx_storage
call ferrata_attest_module build_attest_tx tx_storage
harness attest "$(jq -r .message "$out/rep/tx_storage.json")" "$(jq -r .account "$out/rep/tx_storage.json")" > "$out/rep/exec_storage.json"
jq -n --argjson i "$(rep info)" --argjson d "$(rep dry_storage)" --argjson e "$(rep exec_storage)" \
  '{selector: "0x4652542d41545431", event_data: $e.event_data, expected_ids: $d.ids, expected_instance_id: $i.instance_id}' | req read_storage
call ferrata_attest_module read_attestation read_storage

# Refusal 1: the transfer's proof with the last byte of its receipt node changed.
jq '.log.receipt_proof[-1] |= (.[0:-2] + (if .[-2:] == "00" then "01" else "00" end))' "$out/rep/fetch_transfer.json" > "$out/rep/fetch_tampered.json"
jq -n --argjson i "$(rep info)" --argjson f "$(rep fetch_tampered)" '{chain_id: $i.chain_id, record: $i.record, log: $f.log}' | req dry_tampered
call ferrata_attest_module dry_run_attest dry_tampered
jq -n --argjson i "$(rep info)" --argjson f "$(rep fetch_tampered)" \
  '{program_id: $i.program_id, instance_id: $i.instance_id, chain_id: $i.chain_id, slot: $i.finalized_slot, log: $f.log}' | req tx_tampered
call ferrata_attest_module build_attest_tx tx_tampered
harness attest "$(jq -r .message "$out/rep/tx_tampered.json")" "$(jq -r .account "$out/rep/tx_tampered.json")" > "$out/rep/exec_tampered.json"

# Refusal 2: a request naming another instance, sent to the real instance's header account.
jq -n --argjson i "$(rep info)" --argjson f "$(rep fetch_transfer)" \
  '{program_id: $i.program_id, instance_id: ("0x" + ("11" * 32)), chain_id: $i.chain_id, slot: $i.finalized_slot, log: $f.log}' | req tx_wrong_instance
call ferrata_attest_module build_attest_tx tx_wrong_instance
harness attest "$(jq -r .message "$out/rep/tx_wrong_instance.json")" "$(jq -r .header_account "$out/rep/info.json")" > "$out/rep/exec_wrong_instance.json"

# The bridge module's messages, decoded by the bridge program's types: a burn (outbound vector
# case 0) and a plaintext claim (the inbound vector's claim, a public destination).
jq '.cases[0] | {bridge_seed, config_id, i, evm_chain_id, vault, recipient, token, amount, burn_index}' "$VECTORS/outbound-v2.json" | req bridge_burn
call ferrata_bridge_module burn bridge_burn
harness bridge "$(jq -r .message "$out/rep/bridge_burn.json")" > "$out/rep/decode_burn.json"
jq '.claim | {bridge_seed, evm_chain_id, vault, depositor, j, leaf_index, token, amount, kind, dest, leaves, root,
              config_id: ("0x" + ("c1" * 32)), relayer: ("0x" + ("e2" * 32)), range: {index: 0, from_leaf: 0, until_leaf: 1000}}' \
  "$VECTORS/inbound-v2.json" | req bridge_claim
call ferrata_bridge_module claim bridge_claim
harness bridge "$(jq -r .message "$out/rep/bridge_claim.json")" > "$out/rep/decode_claim.json"

python3 -I "$here/e2e_check.py" "$out" "$VECTORS"
