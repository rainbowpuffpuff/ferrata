#!/usr/bin/env bash
# ferrata_updater_module doing the updater's job on the chain its tests run on: LEZ v0.4.0-rc1's state
# machine in process (`ledger = "harness"`, RISC0_DEV_MODE=1, the attestation program as docs/image-ids.md
# lists it), fed the lab's recorded mainnet and Sepolia light-client data by two sources each
# (lez/lab/results/record/net/). From the checkpoints to the newest recorded updates: the instances
# initialised, every update verified by the program's own steps before it is paid for, the public job
# driven to the end; the finalized roots must be the lab's (lez/lab/README.md, the session of 11 Nov
# 14:17 UTC: mainnet slot 15,631,808 `530599ec…2328`, Sepolia 11,556,000 `d31b7d34…f3cc`), with no
# transaction refused.
#
#   modules/scripts/updater-e2e.sh OUT_DIR                  natively: `run` until up to date
#   CALL=logosctl LOGOSCTL=BIN SESSION=DIR modules/scripts/updater-e2e.sh OUT_DIR
#                                                           through a running Logos Core with the module
#                                                           loaded: `start`, `status` until up to date,
#                                                           `metrics`, `stop`
# Writes OUT_DIR/summary.md, req/, rep/. Exit 1 on a failed check.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd)
rm -rf "$out/req" "$out/rep" "$out/state"
mkdir -p "$out/req" "$out/rep" "$out/state"
CALL=${CALL:-native}
M=ferrata_updater_module
export RISC0_DEV_MODE=1
summary="$out/summary.md"
fails=0
MAINNET_ROOT=530599ec91a6ce70948d70d1066bb35a2a0cb999ad8e2cb6229452dcb7102328
SEPOLIA_ROOT=d31b7d34c2e6b6d8c3430dcdc763375d0ba9a9b201554164326133367aa3f3cc
PROGRAM_ID=$(grep 'attestation program (`lez/guests/program`)' "$repo/docs/image-ids.md" | grep -oE '0x[0-9a-f]{64}' | head -n 1)
echo "# ferrata_updater_module on the in-process LEZ chain with the lab's records ($CALL, $(date -u +%FT%TZ))" > "$summary"
echo >> "$summary"
printf 'name\tmethod\tseconds\n' > "$out/timings.tsv"

call() { # METHOD NAME: request req/NAME.json (none for version/status/metrics/stop), reply rep/NAME.json
  local t0 req=() rc=0
  t0=$(date +%s.%N)
  [ -f "$out/req/$2.json" ] && req=("@$out/req/$2.json")
  case "$CALL" in
    native)
      "$repo/modules/$M/rust-lib/target/release/examples/call" "$1" ${req:+"$out/req/$2.json"} > "$out/rep/$2.json" ;;
    logosctl)
      "$LOGOSCTL" --config-dir "$SESSION" --timeout 3600000 call "$M" "$1" "${req[@]}" --json < /dev/null > "$out/rep/$2.raw" 2>&1 || rc=$?
      jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' \
        "$out/rep/$2.raw" > "$out/rep/$2.json" 2>/dev/null || echo "{\"verdict\":\"call-failed\",\"exit\":$rc}" > "$out/rep/$2.json" ;;
  esac
  printf '%s\t%s\t%s\n' "$2" "$1" "$(awk -v a="$t0" -v b="$(date +%s.%N)" 'BEGIN { printf "%.2f", b - a }')" >> "$out/timings.tsv"
}
req() { cat > "$out/req/$1.json"; }
check() { if eval "$2"; then echo "- PASS $1" >> "$summary"; else echo "- FAIL $1" >> "$summary"; fails=$((fails + 1)); fi; }

# The configuration, with the state directory of this run (the module's dev/recorded.toml otherwise).
sed -e "s#^state_dir = .*#state_dir = \"$out/state\"#" \
    -e "s#\"\\.\\./\\.\\./\\.\\./#\"$repo/#g" "$repo/modules/$M/dev/recorded.toml" > "$out/recorded.toml"
sed -e 's#^ledger = "harness"#ledger = "sequencer"#' "$out/recorded.toml" > "$out/sequencer.toml"

call version version
check "version: the attestation program the module carries has the id of docs/image-ids.md ($PROGRAM_ID), declared and computed from its bytes" \
  '[ "$(jq -r .program.declared_id "$out/rep/version.json")" = "$PROGRAM_ID" ] && [ "$(jq -r .program.computed_id "$out/rep/version.json")" = "$PROGRAM_ID" ]'
jq -n --arg c "$out/recorded.toml" '{config_file: $c}' | req config
call check_config config
check "check_config: two chains (mainnet, sepolia), two sources each, ledger harness" \
  '[ "$(jq -c "[.verdict, .ledger, [.chains[].name], [.chains[].sources | length]]" "$out/rep/config.json")" = "[\"ok\",\"harness\",[\"mainnet\",\"sepolia\"],[2,2]]" ]'
jq -n --arg c "$out/sequencer.toml" '{config_file: $c, rounds: 1}' | req sequencer
if [ "$CALL" = logosctl ]; then call start sequencer; else call run sequencer; fi
check "the sequencer backend is refused, as by the binary (it is not built)" \
  '[ "$(jq -r .verdict "$out/rep/sequencer.json")" = error ] && jq -r .error "$out/rep/sequencer.json" | grep -q "sequencer backend is not in this build"'

t0=$(date +%s)
if [ "$CALL" = logosctl ]; then
  jq -n --arg c "$out/recorded.toml" '{config_file: $c}' | req start
  call start start
  check "start: the daemon runs in the module" '[ "$(jq -r .verdict "$out/rep/start.json")" = started ]'
  polls=0
  while [ $(( $(date +%s) - t0 )) -lt 3600 ]; do
    sleep 5
    polls=$((polls + 1))
    call status status
    jq -c '{t: (now | floor), round: .snapshot.round, up: .snapshot.up_to_date,
            i: [.snapshot.instances[]? | {chain, finalized_slot, applied}]}' "$out/rep/status.json" >> "$out/progress.jsonl"
    [ "$(jq -r .snapshot.up_to_date "$out/rep/status.json")" = true ] && break
    [ "$(jq -r .running "$out/rep/status.json")" = true ] || break
  done
  call metrics metrics
  call stop stop
  jq '.snapshot + {verdict, metrics: null}' "$out/rep/status.json" > "$out/rep/final.json"
  jq -r .metrics "$out/rep/metrics.json" > "$out/metrics.txt"
  check "status: polled $polls times while the daemon ran; then stop ended it" '[ "$(jq -r .running "$out/rep/stop.json")" = false ]'
else
  jq -n --arg c "$out/recorded.toml" '{config_file: $c, rounds: 2000, until_up_to_date: true}' | req run
  call run run
  cp "$out/rep/run.json" "$out/rep/final.json"
  jq -r .metrics "$out/rep/run.json" > "$out/metrics.txt"
fi
seconds=$(( $(date +%s) - t0 ))
f="$out/rep/final.json"
inst() { jq -r --arg c "$1" ".instances[] | select(.chain == \$c) | $2" "$f"; }
check "up to date after $(jq .round "$f") rounds ($seconds s)" '[ "$(jq -r .up_to_date "$f")" = true ]'
check "mainnet: finalized slot 15,631,808, root = the lab's ($MAINNET_ROOT)" \
  '[ "$(inst mainnet .finalized_slot)" = 15631808 ] && [ "$(inst mainnet .finalized_root)" = "$MAINNET_ROOT" ]'
check "sepolia: finalized slot 11,556,000, root = the lab's ($SEPOLIA_ROOT)" \
  '[ "$(inst sepolia .finalized_slot)" = 11556000 ] && [ "$(inst sepolia .finalized_root)" = "$SEPOLIA_ROOT" ]'
check "health: both instances ok" '[ "$(jq -c "[.health[].status]" "$f")" = "[\"ok\",\"ok\"]" ]'
applied=$(awk '/^ferrata_updater_tx_total\{/ && /result="applied"/ {s += $NF} END {print s + 0}' "$out/metrics.txt")
refused=$(awk '/^ferrata_updater_tx_total\{/ && /result="refused"/ {s += $NF} END {print s + 0}' "$out/metrics.txt")
check "metrics: $applied transactions applied, none refused" '[ "$applied" -gt 0 ] && [ "$refused" = 0 ]'
{
  echo
  echo "Jobs applied (InstanceState.applied): mainnet $(inst mainnet .applied), sepolia $(inst sepolia .applied); rounds $(jq .round "$f"); $seconds s."
  echo
  echo "Checks failed: $fails"
} >> "$summary"
cat "$summary"
[ "$fails" -eq 0 ]
