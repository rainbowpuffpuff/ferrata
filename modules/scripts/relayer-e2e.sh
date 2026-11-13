#!/usr/bin/env bash
# ferrata-relayer doing its job on the devnets its tests use, run as the binary (the reference) or as
# ferrata_relayer_module in a Logos Core (relayer-module.sh in the binary's place, the same arguments and lines):
#
#  fork   cli/tests/relayer_fork.rs: an anvil fork of Sepolia at the block before crown-live-1's submitCandidate
#         (nothing is sent to Sepolia) with that run's real proofs: the rule (B) checkpoint from the inbox, a release
#         deferred until its root is final, finalizeAttested after W, the releases (one wrapped from the holder's
#         succinct receipt), the relay fee paid by a proof naming the relayer, fees withdrawn, a misbehaving relayer
#         and a release paying another relayer refused, status, a clean stop on SIGTERM
#  roots  root imports on the mini-app's devnet (cli/examples/miniapp_devnet.rs: the v5 vault on anvil, chain id 1;
#         the zone in the LEZ harness with the bridge program, RISC Zero dev mode), the relayer with --import-roots
#         and --lez-devnet; the light client's finality update is the devnet's stand-in (lc.finalize: anvil's newest
#         block recorded at header(id, slot), instance(id) moved to it), the proofs are anvil's own eth_getProof:
#           no light client on the zone yet: waiting; a record of 2 deposits: ImportRoot n = 2 sent and applied,
#           its message byte for byte ferrata_bridge_module's import_root for the same record and proofs (when the
#           bridge module's call example is built); up to date: nothing sent; a third deposit not yet recorded:
#           waiting; a record whose state root the proofs do not match: refused by the dry run with the code the
#           bridge module's import_root gives, nothing sent; the next honest record: n = 3; then the daemon running
#           (--poll-secs 1): a fourth deposit and its record imported while it runs, and a clean stop on SIGTERM
#
#   modules/scripts/relayer-e2e.sh OUT_DIR                                    the ferrata-relayer binary
#   CALL=logosctl LOGOSCTL=BIN LGX=DIR modules/scripts/relayer-e2e.sh OUT_DIR  the module from the package in DIR
#       (*.lgx), in a Logos Core of its own: started without RISC0_DEV_MODE (the relayer wraps a real succinct
#       receipt) and with FERRATA_WRAP_STUB_PROOF (the wrap stub the daemon runs reads it)
#   CALL=logosctl LOGOSCTL=BIN SESSION=DIR PARTS=roots modules/scripts/relayer-e2e.sh OUT_DIR
#       the module already loaded in that running Logos Core (catalog-install.sh's, installed from the catalog)
# PARTS="fork roots" (default both). FERRATA_FORK_URL as relayer_fork.rs. BRIDGE_CALL: ferrata_bridge_module's call
# example (default the one scripts/check.sh builds; skipped when absent). Needs cargo, Foundry (anvil, forge) and
# vault/lib, r0vm (the wrap's identity_p254), curl, jq; the fork part reads public Sepolia state.
# Writes OUT_DIR/summary.md (PASS/FAIL lines), OUT_DIR/roots.json, OUT_DIR/fork-results.json and the logs.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd)
CALL=${CALL:-native}
PARTS=${PARTS:-"fork roots"}
PORT=${PORT:-$((18700 + $$ % 500))}
ANVIL_PORT=${ANVIL_PORT:-$((19300 + $$ % 500))}
BRIDGE_CALL=${BRIDGE_CALL:-$repo/modules/ferrata_bridge_module/rust-lib/target/release/examples/call}
export PATH="$HOME/.foundry/bin:$HOME/.risc0/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
if [ -z "${RISC0_HOME:-}" ] && [ -d "$repo/../risc0-home-lez" ]; then export RISC0_HOME="$(cd "$repo/../risc0-home-lez" && pwd)"; fi
rm -rf "$out/rounds" "$out/rep" "$out/devnet" "$out/relayer-state" "$out/inbox" "$out/fork-work" "$out/session" "$out/home"
mkdir -p "$out/rounds" "$out/rep" "$out/devnet" "$out/inbox"
summary="$out/summary.md"
printf '# relayer-e2e (%s)\n\n' "$CALL" > "$summary"
pass=0
fail=0
ok() { echo "- PASS $*" | tee -a "$summary"; pass=$((pass + 1)); }
bad() { echo "- FAIL $*" | tee -a "$summary"; fail=$((fail + 1)); }
check() { # NAME CONDITION_COMMAND...
  local name=$1
  shift
  if "$@"; then ok "$name"; else bad "$name"; fi
}

# The relayer: the binary, or the module through relayer-module.sh in a Logos Core of its own.
(cd "$repo/cli" && cargo build --bin ferrata-relayer --example miniapp_devnet 2>&1 | tail -n 2)
if [ "$CALL" = logosctl ] && [ -z "${LGX:-}" ] && [ -n "${SESSION:-}" ]; then
  : "${LOGOSCTL:?}"
  export SESSION LOGOSCTL
  "$LOGOSCTL" --config-dir "$SESSION" call ferrata_relayer_module version --json < /dev/null > "$out/version.raw" 2>&1
  jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' "$out/version.raw" > "$out/version.json"
  check "the module loaded in the given Logos Core, version() $(jq -c '{module, version, lez_wallet}' "$out/version.json" 2>/dev/null)" \
    jq -e '.module == "ferrata_relayer_module"' "$out/version.json"
  R="$here/relayer-module.sh"
elif [ "$CALL" = logosctl ]; then
  : "${LOGOSCTL:?}" "${LGX:?LGX: the directory holding the relayer module package, or SESSION}"
  own_core=1
  LOGOS_TMPDIR=${LOGOS_TMPDIR:-${TMPDIR:-/tmp}}
  if [ $(( ${#LOGOS_TMPDIR} + 45 )) -gt 107 ]; then echo "relayer-e2e.sh: LOGOS_TMPDIR too long for Logos Core's sockets" >&2; exit 2; fi
  export TMPDIR="$LOGOS_TMPDIR"
  export SESSION="$out/session" LOGOSCTL
  mkdir -p "$out/home" "$SESSION"
  env -u RISC0_DEV_MODE HOME="$out/home" FERRATA_WRAP_STUB_PROOF="$repo/e2e/runs/relayer-fork/paid/proof.json" \
    "$LOGOSCTL" --config-dir "$SESSION" daemon start --detach > "$out/core-start.log" 2>&1
  trap '"$LOGOSCTL" --config-dir "$SESSION" daemon stop > /dev/null 2>&1 || true' EXIT
  "$LOGOSCTL" --config-dir "$SESSION" install "$LGX"/*.lgx -y > "$out/core-install.log" 2>&1
  "$LOGOSCTL" --config-dir "$SESSION" module load ferrata_relayer_module > "$out/core-load.log" 2>&1
  "$LOGOSCTL" --config-dir "$SESSION" call ferrata_relayer_module version --json < /dev/null > "$out/version.raw" 2>&1
  jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' "$out/version.raw" > "$out/version.json"
  check "the module loaded in a Logos Core of its own, version() $(jq -c '{module, version, lez_wallet}' "$out/version.json" 2>/dev/null)" \
    jq -e '.module == "ferrata_relayer_module"' "$out/version.json"
  R="$here/relayer-module.sh"
else
  R="$repo/cli/target/debug/ferrata-relayer"
fi
echo "relayer: $R" | tee -a "$summary"

# ---- fork: cli/tests/relayer_fork.rs -----------------------------------------------------------------------
if case " $PARTS " in *" fork "*) true ;; *) false ;; esac; then
  t0=$(date +%s)
  fenv=(FERRATA_RELAYER_FORK=1 FERRATA_RELAYER_WORK="$out/fork-work" FERRATA_RELAYER_RESULTS="$out/fork-results.json")
  if [ "$CALL" = logosctl ]; then fenv+=(FERRATA_RELAYER_CMD="$R"); fi
  if (cd "$repo/cli" && env -u RISC0_DEV_MODE "${fenv[@]}" cargo test --test relayer_fork -- --nocapture) > "$out/fork.log" 2>&1; then
    ok "fork: relayer_fork.rs on an anvil fork of Sepolia with crown-live-1's proofs ($(( $(date +%s) - t0 )) s): $(grep -c '^daemon {' "$out/fork.log") daemon lines; $(jq -r '[.status.checkpoints, .cleanStop.reason] | join(" checkpoint(s), stop: ")' "$out/fork-results.json" 2>/dev/null)"
  else
    bad "fork: relayer_fork.rs ($(( $(date +%s) - t0 )) s): $(grep -E 'Error|panicked|FAILED' "$out/fork.log" | head -n 3 | tr '\n' ' ')"
  fi
fi

# ---- roots: root imports on the mini-app's devnet ------------------------------------------------------------
if case " $PARTS " in *" roots "*) true ;; *) false ;; esac; then
  (cd "$repo/vault" && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0; } && forge build --quiet)
  RISC0_DEV_MODE=1 FERRATA_DEVNET_DIR="$out/devnet" FERRATA_DEVNET_PORT=$PORT FERRATA_DEVNET_ANVIL_PORT=$ANVIL_PORT \
    "$repo/cli/target/debug/examples/miniapp_devnet" > "$out/devnet.log" 2>&1 &
  devnet=$!
  stop_all() {
    curl -s -d '{"method":"shutdown","params":{}}' "http://127.0.0.1:$PORT/rpc" > /dev/null 2>&1 || true
    sleep 1
    kill "$devnet" 2>/dev/null || true
    if [ -n "${own_core:-}" ]; then "$LOGOSCTL" --config-dir "$SESSION" daemon stop > /dev/null 2>&1 || true; fi
  }
  trap stop_all EXIT
  for _ in $(seq 1 480); do curl -sf "http://127.0.0.1:$PORT/" > /dev/null && break; sleep 0.5; done
  # dev NAME METHOD [PARAMS_JSON]: the devnet's result in rep/NAME.json (a failure stops the part)
  dev() {
    local body
    body=$(jq -cn --arg m "$2" --argjson p "${3:-{\}}" '{method: $m, params: $p}')
    curl -s -d "$body" "http://127.0.0.1:$PORT/rpc" > "$out/rep/$1.raw"
    if ! jq -e '.ok == true' "$out/rep/$1.raw" > /dev/null; then
      bad "devnet $2: $(head -c 400 "$out/rep/$1.raw")"
      return 1
    fi
    jq -c '.result' "$out/rep/$1.raw" > "$out/rep/$1.json"
  }
  rep() { cat "$out/rep/$1.json"; }
  if dev info info && dev inputs0 module.inputs; then
    PROGRAM=$(jq -r .program "$out/rep/inputs0.json")
    VAULT=$(jq -r .vault "$out/rep/inputs0.json")
    A=(--eth-rpc "http://127.0.0.1:$ANVIL_PORT" --vault "$VAULT" --chain-id 1
       --keystore "$out/devnet/keystore-relayer.json" --password-file "$out/devnet/password.txt"
       --state "$out/relayer-state" --lez-devnet "http://127.0.0.1:$PORT/rpc" --bridge-account "$PROGRAM" --import-roots)
    # round NAME: one round of the relayer; its lines in rounds/NAME.jsonl
    round() {
      "$R" "${A[@]}" run --inbox "$out/inbox" --rounds 1 --poll-secs 0 > "$out/rounds/$1.jsonl" 2> "$out/rounds/$1.err"
      local rc=$?
      [ $rc = 0 ] || bad "round $1 exited $rc: $(head -c 300 "$out/rounds/$1.err")"
      return $rc
    }
    ev() { jq -c --arg e "$2" 'select(.event == $e)' "$out/rounds/$1.jsonl"; }   # ev ROUND EVENT
    applied() { dev "applied_$1" zone.applied "{\"program\":\"$PROGRAM\"}" && jq '.applied | length' "$out/rep/applied_$1.json"; }
    imports() { jq -c '.imported' "$out/rep/$1.json"; }

    dev deposit0 deposit '{"kind":"private","denomination":0,"account":0}'
    dev deposit1 deposit '{"kind":"private","denomination":0,"account":0}'
    n_before=$(applied before)
    round r1
    check "roots: no light client on the zone yet: waiting ($(ev r1 root-import-waiting | jq -r .reason | head -c 120))" \
      bash -c "[ \"\$(jq -s 'map(select(.event == \"root-import-waiting\" and .vaultCount == 2 and .imported == 0)) | length' '$out/rounds/r1.jsonl')\" = 1 ]"

    dev fin1 lc.finalize
    round r2
    dev inputs2 module.inputs
    n2=$(applied r2)
    check "roots: a record of 2 deposits (block $(jq -r .block "$out/rep/fin1.json"), slot $(jq -r .record_slot "$out/rep/fin1.json")): ImportRoot n = 2 applied ($(ev r2 root-imported | jq -c '{leafCount, recordSlot, block, tx}'))" \
      bash -c "[ \"\$(jq -s 'map(select(.event == \"root-imported\" and .leafCount == 2 and .applied == true)) | length' '$out/rounds/r2.jsonl')\" = 1 ] && jq -e '.imported == [2]' '$out/rep/inputs2.json' > /dev/null && [ $n2 = $((n_before + 1)) ]"
    # The same import from ferrata_bridge_module (import_root on the same record and proofs): the same message.
    if [ -x "$BRIDGE_CALL" ]; then
      jq -n --argjson i "$(rep inputs2)" --argjson f "$(rep fin1)" \
        '{program: $i.program, config_id: $i.config_id, config_state: $i.config_state, record_slot: $f.record_slot,
          record_state: $f.record_state, get_proof: $f.get_proof}' > "$out/rep/bridge_import_root.req.json"
      "$BRIDGE_CALL" import_root "$out/rep/bridge_import_root.req.json" > "$out/rep/bridge_import_root.json"
      relayer_msg=$(jq -r '.applied[-1].message' "$out/rep/applied_r2.json")
      check "roots: the relayer's ImportRoot is byte for byte ferrata_bridge_module's import_root for the same record ($(jq -r '.verdict + ", " + (.message_bytes | tostring) + " bytes"' "$out/rep/bridge_import_root.json"))" \
        bash -c "[ \"\$(jq -r .verdict '$out/rep/bridge_import_root.json')\" = importable ] && [ \"\$(jq -r .message '$out/rep/bridge_import_root.json')\" = '$relayer_msg' ]"
    else
      echo "skipped: the byte comparison with ferrata_bridge_module (no $BRIDGE_CALL)" | tee -a "$summary"
    fi

    round r3
    n3=$(applied r3)
    check "roots: up to date: no import line, nothing sent" \
      bash -c "[ \"\$(jq -s 'map(select(.event | startswith(\"root-import\"))) | length' '$out/rounds/r3.jsonl')\" = 0 ] && [ $n3 = $n2 ]"

    dev deposit2 deposit '{"kind":"private","denomination":0,"account":0}'
    round r4
    check "roots: a third deposit not yet recorded: waiting ($(ev r4 root-import-waiting | jq -r .reason | head -c 140))" \
      bash -c "[ \"\$(jq -s 'map(select(.event == \"root-import-waiting\" and .vaultCount == 3 and .imported == 2)) | length' '$out/rounds/r4.jsonl')\" = 1 ]"

    dev fin2 lc.finalize '{"tamper":true}'
    round r5
    n5=$(applied r5)
    reason=$(ev r5 root-import-refused | jq -r .reason)
    code=$(sed -n 's/.*exit code \([0-9]*\).*/\1/p' <<< "$reason" | head -n 1)
    bridge_code=""
    if [ -x "$BRIDGE_CALL" ]; then
      dev inputs5 module.inputs
      jq -n --argjson i "$(rep inputs5)" --argjson f "$(rep fin2)" \
        '{program: $i.program, config_id: $i.config_id, config_state: $i.config_state, record_slot: $f.record_slot,
          record_state: $f.record_state, get_proof: $f.get_proof}' > "$out/rep/bridge_import_root_tampered.req.json"
      "$BRIDGE_CALL" import_root "$out/rep/bridge_import_root_tampered.req.json" > "$out/rep/bridge_import_root_tampered.json"
      bridge_code=$(jq -r '.code // empty' "$out/rep/bridge_import_root_tampered.json")
    fi
    check "roots: a record the proofs do not match (state root changed): refused by the dry run, code $code$( [ -n "$bridge_code" ] && echo " (ferrata_bridge_module's import_root: $bridge_code $(jq -r .error "$out/rep/bridge_import_root_tampered.json"))"), nothing sent" \
      bash -c "[ -n '$code' ] && [ $n5 = $n3 ] && { [ -z '$bridge_code' ] || [ '$bridge_code' = '$code' ]; }"

    dev fin3 lc.finalize
    round r6
    dev inputs6 module.inputs
    check "roots: the next honest record: ImportRoot n = 3 applied ($(ev r6 root-imported | jq -c '{leafCount, previous, recordSlot}'))" \
      bash -c "[ \"\$(jq -s 'map(select(.event == \"root-imported\" and .leafCount == 3 and .previous == 2 and .applied == true)) | length' '$out/rounds/r6.jsonl')\" = 1 ] && jq -e '.imported == [2, 3]' '$out/rep/inputs6.json' > /dev/null"

    # The daemon running: a deposit and its record while it polls, then SIGTERM.
    "$R" "${A[@]}" run --inbox "$out/inbox" --poll-secs 1 > "$out/rounds/daemon.jsonl" 2> "$out/rounds/daemon.err" &
    d=$!
    for _ in $(seq 1 240); do grep -q '"event":"start"' "$out/rounds/daemon.jsonl" 2>/dev/null && break; sleep 0.5; done
    dev deposit3 deposit '{"kind":"private","denomination":0,"account":0}'
    dev fin4 lc.finalize
    for _ in $(seq 1 240); do
      jq -e 'select(.event == "root-imported" and .leafCount == 4)' "$out/rounds/daemon.jsonl" > /dev/null 2>&1 && break
      sleep 0.5
    done
    kill -TERM "$d"
    wait "$d"
    drc=$?
    dev inputs7 module.inputs
    check "roots: the daemon running imports n = 4 while it polls; clean stop on SIGTERM (exit $drc, $(jq -s -c 'map(select(.event == "stop"))[0] | {rounds, reason}' "$out/rounds/daemon.jsonl"))" \
      bash -c "[ $drc = 0 ] && [ \"\$(jq -s 'map(select(.event == \"root-imported\" and .leafCount == 4)) | length' '$out/rounds/daemon.jsonl')\" = 1 ] && [ \"\$(jq -s -r 'map(select(.event == \"stop\"))[0].reason' '$out/rounds/daemon.jsonl')\" = SIGTERM ] && jq -e '.imported == [2, 3, 4]' '$out/rep/inputs7.json' > /dev/null"

    # The record of this part.
    jq -n --arg call "$CALL" --argjson fin1 "$(rep fin1 | jq 'del(.get_proof, .record_state)')" \
      --argjson fin2 "$(rep fin2 | jq 'del(.get_proof, .record_state)')" --argjson fin3 "$(rep fin3 | jq 'del(.get_proof, .record_state)')" \
      --argjson fin4 "$(rep fin4 | jq 'del(.get_proof, .record_state)')" --argjson imported "$(imports inputs7)" \
      --slurpfile r1 "$out/rounds/r1.jsonl" --slurpfile r2 "$out/rounds/r2.jsonl" --slurpfile r3 "$out/rounds/r3.jsonl" \
      --slurpfile r4 "$out/rounds/r4.jsonl" --slurpfile r5 "$out/rounds/r5.jsonl" --slurpfile r6 "$out/rounds/r6.jsonl" \
      --slurpfile daemon "$out/rounds/daemon.jsonl" \
      '{call: $call, records: [$fin1, $fin2, $fin3, $fin4], imported: $imported,
        rounds: {r1: $r1, r2: $r2, r3: $r3, r4: $r4, r5: $r5, r6: $r6, daemon: $daemon}}' > "$out/roots.json"
  fi
  stop_all
  trap - EXIT
fi

printf '\n%s passed, %s failed\n' "$pass" "$fail" | tee -a "$summary"
[ "$fail" = 0 ]
