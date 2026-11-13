#!/usr/bin/env bash
# ferrata_bridge_module end to end against the mini-app's devnet (cli/examples/miniapp_devnet.rs: the
# v5 vault on anvil, chain id 1, local only, its verifier the vault tests' mock; the zone in the LEZ
# harness with the bridge program, RISC Zero dev mode unless FERRATA_DEVNET_PROOFS=real). The module
# builds every LEZ call and the release from the chains' state given in its requests, with the
# `ferrata` tool's own code (ferrata-wallet); the devnet reads the chains and submits what it built.
#
#   import_root     the vault's root (2 deposits) from a block the devnet records, dry-run, submitted;
#                   a wrong leaf count refused by the dry run and by the program with the same code
#   registry        token(cid, ETH), config(cid), root(cid, 2) decoded; a claim's range and fees
#   claim (private) both deposits claimed privately (ClaimPrivately, proven and settled privately);
#                   a second claim of the first refused (ALREADY_CLAIMED) from the nullifier's state
#   release         two private burns, a checkpoint over the first (its delay passed) and one over both
#                   (in its delay): the wallet's rule picks the older root for burn 0, refuses burn 1
#                   (ReleaseDelayNotElapsed) until the newer opens, then picks the newer
#   prove_release   the release guest run on the module's input through r0vm (journal = native)
#   vault           both releases sent to the vault (the mock accepts the exact journals); `status`
#                   shows both burns released
#
#   modules/scripts/bridge_e2e.sh OUT_DIR                      the module's methods called natively
#   CALL=logosctl LOGOSCTL=BIN SESSION=DIR modules/scripts/bridge_e2e.sh OUT_DIR
#                                                              through a running Logos Core daemon
#   CALL=logosctl ETH_VIA=eth_rpc_module ... bridge_e2e.sh OUT_DIR
#                                                              also import_root reading the vault's
#                                                              eth_getProof itself through Logos Core's
#                                                              eth_rpc_module (chain 1 = the devnet's anvil)
# At the end, in every mode: a third deposit, anvil's newest block recorded as the instance's finalized
# header (the devnet's lc.finalize, as relayer-e2e.sh does), and import_root with the proofs given and
# with the module reading them from anvil at that block over plain HTTP (import_read_plain) and, with
# ETH_VIA=eth_rpc_module, through eth_rpc_module (import_read_eth); each read reply compared with the
# given one (logos-io.tsv). (The import above is under the devnet's synthetic block, whose proofs anvil
# cannot answer.)
# Needs: RELEASE_ELF (the release guest's binary: bridge/, `cargo build --release -p methods`), the call
# example (native) or logosctl, Foundry (anvil, forge) and vault/lib, cargo, curl, jq, python3.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd)
rm -rf "$out/req" "$out/rep" "$out/devnet"
mkdir -p "$out/req" "$out/rep" "$out/devnet"
CALL=${CALL:-native}
ETH_VIA=${ETH_VIA:-http}
PORT=${PORT:-$((18700 + $$ % 500))}
ANVIL_PORT=${ANVIL_PORT:-$((19300 + $$ % 500))}
: "${RELEASE_ELF:?RELEASE_ELF: the release guest binary (bridge/target/riscv-guest/.../release.bin)}"
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1 RISC0_DEV_MODE=${RISC0_DEV_MODE:-1}
if [ -z "${RISC0_HOME:-}" ] && [ -d "$repo/../risc0-home-lez" ]; then export RISC0_HOME="$(cd "$repo/../risc0-home-lez" && pwd)"; fi

call() {
  local t0 t1
  t0=$(date +%s.%N)
  case "$CALL" in
    native)
      "$repo/modules/ferrata_bridge_module/rust-lib/target/release/examples/call" "$1" "$out/req/$2.json" > "$out/rep/$2.json"
      ;;
    logosctl)
      "$LOGOSCTL" --config-dir "$SESSION" call ferrata_bridge_module "$1" "@$out/req/$2.json" --json < /dev/null > "$out/rep/$2.raw" 2>&1
      jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' "$out/rep/$2.raw" > "$out/rep/$2.json"
      ;;
  esac
  t1=$(date +%s.%N)
  printf '%s\t%s\t%s\n' "$2" "$1" "$(awk -v a="$t0" -v b="$t1" 'BEGIN { printf "%.2f", b - a }')" >> "$out/timings.tsv"
}
# dev NAME METHOD [PARAMS_JSON]: the devnet's reply's result in rep/dev_NAME.json (a failure stops the run)
dev() {
  local body
  body=$(jq -cn --arg m "$2" --argjson p "${3:-{\}}" '{method: $m, params: $p}')
  curl -s -d "$body" "http://127.0.0.1:$PORT/rpc" > "$out/rep/dev_$1.raw"
  if ! jq -e '.ok == true' "$out/rep/dev_$1.raw" > /dev/null; then
    echo "devnet $2 failed: $(head -c 600 "$out/rep/dev_$1.raw")" >&2
    exit 1
  fi
  jq -c '.result' "$out/rep/dev_$1.raw" > "$out/rep/dev_$1.json"
}
req() { cat > "$out/req/$1.json"; }
rep() { cat "$out/rep/$1.json"; }
printf 'name\tmethod\tseconds\n' > "$out/timings.tsv"
echo "call mode: $CALL; Ethereum read via: $ETH_VIA" > "$out/mode.txt"

# The devnet: the vault built, the backend built and started.
(cd "$repo/vault" && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0; } && forge build --quiet)
(cd "$repo/cli" && cargo build --example miniapp_devnet 2>&1 | tail -n 2)
FERRATA_DEVNET_DIR="$out/devnet" FERRATA_DEVNET_PORT=$PORT FERRATA_DEVNET_ANVIL_PORT=$ANVIL_PORT \
  "$repo/cli/target/debug/examples/miniapp_devnet" > "$out/devnet.log" 2>&1 &
devnet=$!
trap 'curl -s -d "{\"method\":\"shutdown\",\"params\":{}}" "http://127.0.0.1:$PORT/rpc" > /dev/null 2>&1 || true; sleep 1; kill $devnet 2>/dev/null || true' EXIT
for _ in $(seq 1 480); do curl -sf "http://127.0.0.1:$PORT/" > /dev/null && break; sleep 0.5; done
dev info info

# Two deposits to the devnet user's private account /0 (the tool's deposit, as the mini-app sends it).
dev deposit0 deposit '{"kind":"private","denomination":0,"account":0}'
dev deposit1 deposit '{"kind":"private","denomination":0,"account":0}'

# --- import_root ----------------------------------------------------------------------------------
dev import_block module.import_block
dev inputs_import module.inputs
jq -n --argjson i "$(rep dev_inputs_import)" --argjson b "$(rep dev_import_block)" \
  '{program: $i.program, config_id: $i.config_id, config_state: $i.config_state, record_slot: $b.record_slot,
    record_state: $b.record_state, get_proof: $b.get_proof, header: $b.header}' | req import_root
call import_root import_root
# A wrong leaf count: the dry run refuses, then the program, with the same code.
jq --argjson b "$(rep dev_import_block)" '. + {leaf_count: ($b.leaf_count + 1)}' "$out/req/import_root.json" | req import_root_bad
call import_root import_root_bad
dev submit_import_bad module.submit "$(jq -c '{message, accounts, root_index, what}' "$out/rep/import_root_bad.json")"
dev submit_import module.submit "$(jq -c '{message, accounts, root_index, what}' "$out/rep/import_root.json")"
dev import_fees module.import_fees "$(jq -c '{block}' "$out/rep/dev_import_block.json")"

# --- registry -------------------------------------------------------------------------------------
dev inputs_registry module.inputs
jq -n --argjson i "$(rep dev_inputs_registry)" --argjson d "$(rep dev_deposit0)" \
  '{token_state: $i.token_state, config_state: $i.config_state, root_state: $i.root_state, bridge_header: $i.program,
    leaf_index: $d.leaf_index, amount: $d.amount}' | req registry
call registry registry

# --- claim (private) ------------------------------------------------------------------------------
claim_request() { # NAME DEPOSIT_REPLY INPUTS_REPLY
  jq -n --argjson i "$(rep "$3")" --argjson d "$(rep "$2")" \
    '{bridge_seed: $i.bridge_seed, chain_id: $i.chain_id, vault: $i.vault, program: $i.program, config_id: $i.config_id,
      depositor: $i.depositor, j: $d.j, leaf_index: $d.leaf_index, token: ("0x" + ("00" * 20)), amount: $d.amount,
      kind: "private", dest: $i.private_accounts[$d.account], private: true,
      leaves: $i.leaves, imported: $i.imported, root_state: $i.root_state, token_state: $i.token_state,
      nullifier_state: ($i.nullifier_state // null)}' | req "$1"
}
for k in 0 1; do
  dev inputs_claim$k module.inputs
  claim_request claim${k}_pre dev_deposit$k dev_inputs_claim$k
  call claim claim${k}_pre
  # The nullifier's account as the zone holds it (empty: unspent), then the claim with it.
  dev inputs_claim${k}_n module.inputs "$(jq -c '{nullifier}' "$out/rep/claim${k}_pre.json")"
  claim_request claim$k dev_deposit$k dev_inputs_claim${k}_n
  call claim claim$k
  dev submit_claim$k module.submit "$(jq -c '{message, accounts, root_index, what}' "$out/rep/claim$k.json")"
done
dev inputs_claimed module.inputs "$(jq -c '{nullifier}' "$out/rep/claim0.json")"
claim_request claim0_again dev_deposit0 dev_inputs_claimed
call claim claim0_again
dev scan_after_claims scan

# --- burns, two checkpoints, the release rule -----------------------------------------------------
amount=$(jq -r .amount "$out/rep/dev_deposit0.json")
dev burn0 burn "{\"private\":true,\"account\":0,\"amount\":\"$amount\",\"recipient\":\"0x00000000000000000000000000000000000b0b01\"}"
dev checkpoint_a checkpoint '{"pass_delay":true}'
dev burn1 burn "{\"private\":true,\"account\":0,\"amount\":\"$amount\",\"recipient\":\"0x00000000000000000000000000000000000b0b02\"}"
dev checkpoint_b checkpoint '{"pass_delay":false}'
release_request() { # NAME BURN_REPLY ROOTS_REPLY INPUTS_REPLY
  jq -n --argjson i "$(rep "$4")" --argjson r "$(rep "$3")" --argjson b "$(rep "$2")" \
    '{bridge_seed: $i.bridge_seed, chain_id: $i.chain_id, vault: $i.vault, config_id: $i.config_id, i: $b.i,
      burns: $i.burns, roots: $r.roots, now: $r.now}' | req "$1"
}
dev roots_early module.vault_roots
dev inputs_release module.inputs
release_request release0 dev_burn0 dev_roots_early dev_inputs_release
call release release0
release_request release1_early dev_burn1 dev_roots_early dev_inputs_release
call release release1_early
jq --arg elf "$RELEASE_ELF" '. + {program: $elf}' "$out/req/release0.json" | req prove_release0
call prove_release prove_release0
dev vault_release0 module.vault_release "$(jq -c '{release: .}' "$out/rep/release0.json")"
dev wait advance_time '{"seconds":200}'
dev roots_late module.vault_roots
release_request release1 dev_burn1 dev_roots_late dev_inputs_release
call release release1
dev vault_release1 module.vault_release "$(jq -c '{release: .}' "$out/rep/release1.json")"

# --- status: both burns released ------------------------------------------------------------------
dev inputs_status module.inputs
jq -n --argjson r0 "$(rep release0)" --argjson r1 "$(rep release1)" --argjson rl "$(rep dev_roots_late)" --argjson i "$(rep dev_inputs_status)" \
  '{deposits: [], imported_leaf_count: ($i.imported | max), spent_nullifiers: [],
    burns: [{burn_index: $r0.burn_index, nullifier: $r0.nullifier}, {burn_index: $r1.burn_index, nullifier: $r1.nullifier}],
    checkpointed_burn_count: ($rl.roots | map(.count) | max), released_nullifiers: ($i.released_nullifiers // [])}' | req status
call status status

# --- import_root reading the vault's proofs (plain HTTP; eth_rpc_module) ---------------------------
dev deposit2 deposit '{"kind":"private","denomination":0,"account":0}'
dev lc_finalize lc.finalize
dev inputs_read module.inputs
jq -n --argjson i "$(rep dev_inputs_read)" --argjson f "$(rep dev_lc_finalize)" \
  '{program: $i.program, config_id: $i.config_id, config_state: $i.config_state, record_slot: $f.record_slot,
    record_state: $f.record_state, get_proof: $f.get_proof}' | req import_given
call import_root import_given
printf 'name\tvia\troutes\tequal_to_given\n' > "$out/logos-io.tsv"
same() {
  local eq
  eq=$(jq -n --slurpfile a "$out/rep/$1.json" --slurpfile b "$out/rep/import_given.json" \
    '($a[0] | del(.eth)) == $b[0] and ($a[0] | has("eth"))')
  printf '%s\t%s\t%s\t%s\n' "$1" "$(jq -r '.eth.via // "none"' "$out/rep/$1.json")" \
    "$(jq -c '.eth.routes // []' "$out/rep/$1.json")" "$eq" >> "$out/logos-io.tsv"
}
jq --argjson f "$(rep dev_lc_finalize)" --arg url "http://127.0.0.1:$ANVIL_PORT" \
  'del(.get_proof) + {block: $f.block, rpc_url: $url}' "$out/req/import_given.json" | req import_read_plain
call import_root import_read_plain
same import_read_plain
if [ "$ETH_VIA" = eth_rpc_module ]; then
  "$LOGOSCTL" --config-dir "$SESSION" call eth_rpc_module set_chain_config 1 \
    "{\"endpoint\":\"http://127.0.0.1:$ANVIL_PORT\",\"name\":\"devnet anvil\"}" --json < /dev/null > "$out/rep/eth_rpc_chain.raw" 2>&1
  jq --argjson f "$(rep dev_lc_finalize)" 'del(.get_proof) + {block: $f.block, eth: {via: "eth_rpc_module", chain_id: 1}}' \
    "$out/req/import_given.json" | req import_read_eth
  call import_root import_read_eth
  same import_read_eth
fi

# The devnet's working directory holds its keystores (test keys): keep only its call log.
curl -s -d '{"method":"shutdown","params":{}}' "http://127.0.0.1:$PORT/rpc" > /dev/null 2>&1 || true
sleep 1
cp "$out/devnet/calls.jsonl" "$out/devnet-calls.jsonl" 2>/dev/null || true
rm -rf "$out/devnet"
# No machine paths in the results (the release guest's path, the devnet's log): the directory beside
# the repository becomes <spike>.
spike=$(cd "$repo/.." && pwd)
find "$out" -type f \( -name '*.json' -o -name '*.raw' -o -name '*.log' -o -name '*.txt' \) -exec sed -i "s|$spike/|<spike>/|g" {} +
python3 -I "$here/bridge_e2e_check.py" "$out"
