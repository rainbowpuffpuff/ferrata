#!/usr/bin/env bash
# e2e/crown/proofs.sh: stages 7 (checkpoint) and 9 (releases) of the crown run (e2e/crown/README.md) over one run
# directory. Each step is one command; "box" steps need the built tools (cargo build --release -p realblocks --bin
# realblocks-crown in checkpoint/, -p ferrata-e2e in bridge/), "laptop" steps the modal CLI.
#
#   proofs.sh <run> input                 box: realblocks-crown over the LEZ side's files -> checkpoint/, executor check
#   proofs.sh <run> stage-gpu             any: checkpoint/gpu-inputs (R0BF programs, the fixture, MANIFEST.json)
#   proofs.sh <run> gpu [name]            laptop: one L4 proof (gpu-prover spawn on ws-3), waits, fetches -> checkpoint/gpu/
#   proofs.sh <run> releases              box: crown-release (checkpoint/burns.json + burn-notes.json), executor, CPU proofs
#   proofs.sh <run> p254 <proof>          box: identity_p254 -> work/<proof>/input.json (proof: checkpoint | release-<name>)
#   proofs.sh <run> groth16-prove <proof> laptop: RISC Zero's Groth16 prover image via the box app (wrap.py) -> proof.json
#   proofs.sh <run> groth16 <proof>       box: the Groth16 receipt from proof.json, verified -> seal.hex, journal.hex
#   proofs.sh <run> fragment              any: proofs.json = crown.json's candidate {journal, seal} and releases[] with seals
#   proofs.sh <run> verify                box: the input and journal re-derived from the recorded files; every receipt verified
#
# Run directory (the Volume's layout; spike interface files lez.md and proofs.md):
#   identity.json                                  {chainId, vault, configId, genesisState} (stage 2)
#   lez/session-a/{session-a.json, channel_signing_key, deployment-settings.yaml}
#   lez/session-b/{session-b.json, burn-notes.json, node/}   (node/: crown-extract.sh at the end of session B)
#   programs/ (or $FERRATA_PROGRAMS)               R0BF checkpoint_b.bin checkpoint.bin segment.bin release.bin, ids.jsonl
#                                                  (ferrata-e2e program-binary of the crown build's user ELFs)
# Overrides: REALBLOCKS_CROWN, FERRATA_E2E (tool paths), BURNS / NOTES (crown-release inputs; rehearsals only),
# FERRATA_GPU_PROFILE (default ws-3), FERRATA_TMPDIR (modal's scratch on the laptop; default <run>/tmp).
set -euo pipefail
run="$(realpath -m "${1:?run dir}")"; step="${2:?step}"
here="$(cd "$(dirname "$0")" && pwd)"; repo="$(cd "$here/../.." && pwd)"
RC="${REALBLOCKS_CROWN:-$repo/checkpoint/target/release/realblocks-crown}"
E="${FERRATA_E2E:-$repo/bridge/target/release/ferrata-e2e}"
P="${FERRATA_PROGRAMS:-$run/programs}"
A="$run/lez/session-a"; B="$run/lez/session-b"; C="$run/checkpoint"; REL="$run/releases"
GPU_PROFILE="${FERRATA_GPU_PROFILE:-ws-3}"

id_of() { jq -r --arg p "/$1.bin" 'select(.program | endswith($p)) | .image_id' "$P/ids.jsonl"; }
name_of() { basename "$(ls "$C"/crown-*.input | head -n 1)" .input; }
image_of() { case "$1" in checkpoint) id_of checkpoint_b ;; release-*) id_of release ;; *) exit 2 ;; esac; }
meta_of() { case "$1" in checkpoint) echo "$C/$(name_of).json" ;; release-*) echo "$REL/$1.json" ;; esac; }
succinct_of() { case "$1" in checkpoint) echo "$C/gpu/$(name_of)/receipt-succinct.bin" ;; release-*) echo "$REL/receipts/$1/receipt-succinct.bin" ;; esac; }
receipts_of() { case "$1" in checkpoint) echo "$C/receipts" ;; release-*) echo "$REL/receipts/$1" ;; esac; }
hexfile() { tr -d ' \n' < "$1" | sed 's/^0x//'; }
input_args() {
  local gs; gs="$(jq -r '.genesisState // empty' "$run/identity.json")"
  printf '%s\0' --node "$B/node" --channel-key "$A/channel_signing_key" --genesis "$A/deployment-settings.yaml" \
    --session-a "$A/session-a.json" --session-b "$B/session-b.json" --vault "$(jq -r .vault "$run/identity.json")" \
    --config-id "$(jq -r .configId "$run/identity.json")"
  [ -n "$gs" ] && printf '%s\0' --genesis-state "$gs"
  return 0
}
modal_env() { export TMPDIR="${FERRATA_TMPDIR:-$run/tmp}"; mkdir -p "$TMPDIR"; }

case "$step" in
input)
  mkdir -p "$C"
  mapfile -d '' args < <(input_args)
  "$RC" "${args[@]}" --out "$C" | tee "$C/realblocks-crown.txt"
  n="$(name_of)"
  "$E" exec --program "$P/checkpoint_b.bin" --input "$C/$n.input" --meta "$C/$n.json" | tee "$C/exec.json"
  jq -e '.journal_equals_native' "$C/exec.json" > /dev/null
  ;;
stage-gpu)
  bash "$here/stage-gpu.sh" "$C" "$(name_of)" "$P" "$C/gpu-inputs"
  ;;
gpu)
  modal_env; n="$(name_of)"; r="${3:-crown-$(date -u +%Y%m%dT%H%M%SZ)}"
  ( cd "$repo/gpu-prover" && FERRATA_INPUTS="$C/gpu-inputs" MODAL_PROFILE="$GPU_PROFILE" \
      modal run --detach modal_app.py::spawn --fixtures "$n" --run "$r" --limit-s 1800 )
  until MODAL_PROFILE="$GPU_PROFILE" modal volume ls ferrata-prover "results/$r/$n" 2>/dev/null | grep -q 'run.json'; do sleep 15; done
  mkdir -p "$C/gpu" && ( cd "$C/gpu" && MODAL_PROFILE="$GPU_PROFILE" modal volume get --force ferrata-prover "results/$r/$n" . > /dev/null )
  jq -c '{ok, gpu, image_ids_equal, prover_wall_s, container_s_until_done, finished_at_utc}' "$C/gpu/$n/run.json"
  jq -e '.ok' "$C/gpu/$n/run.json" > /dev/null
  ;;
releases)
  mkdir -p "$REL"
  "$E" crown-release --identity "$run/identity.json" --burns "${BURNS:-$C/burns.json}" --notes "${NOTES:-$B/burn-notes.json}" --out-dir "$REL"
  for f in "$REL"/release-*.input; do
    p="$(basename "$f" .input)"
    "$E" exec --program "$P/release.bin" --input "$f" --meta "$REL/$p.json" > "$REL/$p.exec.json"
    "$E" prove --program "$P/release.bin" --input "$f" --meta "$REL/$p.json" --out-dir "$REL/receipts/$p" | tee "$REL/$p.prove.json"
  done
  ;;
p254)
  p="${3:?proof}"; "$E" p254 --receipt "$(succinct_of "$p")" --image-id "$(image_of "$p")" --out-dir "$run/work/$p"
  ;;
groth16-prove)
  p="${3:?proof}"; modal_env; python3 "$here/wrap.py" "$run/work/$p/input.json" "$run/work/$p/proof.json"
  ;;
groth16)
  p="${3:?proof}"; d="$(receipts_of "$p")"; mkdir -p "$d"
  "$E" groth16 --receipt "$(succinct_of "$p")" --image-id "$(image_of "$p")" --proof "$run/work/$p/proof.json" --out-dir "$d"
  "$E" verify --receipt "$d/receipt-groth16.bin" --image-id "$(image_of "$p")" --meta "$(meta_of "$p")"
  ;;
fragment)
  n="$(name_of)"
  [ "$(hexfile "$C/receipts/journal.hex")" = "$(jq -r .journal "$C/$n.json")" ] || { echo "checkpoint journal != native" >&2; exit 1; }
  rel="[]"
  if [ -f "$REL/releases.json" ]; then
    rel="$(jq -c '.[]' "$REL/releases.json" | while read -r e; do
      p="release-$(jq -r .name <<< "$e")"
      [ "$(hexfile "$REL/receipts/$p/journal.hex")" = "$(jq -r .journal "$REL/$p.json")" ] || { echo "$p: journal" >&2; exit 1; }
      jq -c --arg s "0x$(hexfile "$REL/receipts/$p/seal.hex")" '.seal = $s' <<< "$e"
    done | jq -s -c .)"
  fi
  jq -n --arg j "0x$(hexfile "$C/receipts/journal.hex")" --arg s "0x$(hexfile "$C/receipts/seal.hex")" --argjson r "$rel" \
    --arg cid "$(id_of checkpoint_b)" --arg rid "$(id_of release)" \
    '{checkpointImageIdB: $cid, releaseImageId: $rid, candidate: {journal: $j, seal: $s}, releases: $r}' > "$run/proofs.json"
  jq -c '{candidate: (.candidate | {journal: (.journal | length), seal: (.seal | length)}), releases: (.releases | length)}' "$run/proofs.json"
  ;;
verify)
  t="$run/verify-tmp"; rm -rf "$t"; mkdir -p "$t"; n="$(name_of)"
  mapfile -d '' args < <(input_args)
  "$RC" "${args[@]}" --out "$t" > /dev/null
  cmp -s "$t/$n.input" "$C/$n.input" || { echo "verify: the re-derived input differs" >&2; exit 1; }
  [ "$(jq -r .journal "$t/$n.json")" = "$(jq -r .journal "$C/$n.json")" ] || { echo "verify: the journal differs" >&2; exit 1; }
  echo "checkpoint input and journal re-derived from the recorded node answers and session files: equal"
  "$E" verify --receipt "$(succinct_of checkpoint)" --image-id "$(image_of checkpoint)" --meta "$C/$n.json"
  "$E" verify --receipt "$C/receipts/receipt-groth16.bin" --image-id "$(image_of checkpoint)" --meta "$C/$n.json"
  for m in "$REL"/release-*.json; do
    [ -e "$m" ] || continue
    case "$m" in *.exec.json|*.prove.json) continue ;; esac
    p="$(basename "$m" .json)"
    "$E" verify --receipt "$REL/receipts/$p/receipt-succinct.bin" --image-id "$(image_of "$p")" --meta "$m"
    "$E" verify --receipt "$REL/receipts/$p/receipt-groth16.bin" --image-id "$(image_of "$p")" --meta "$m"
  done
  if [ -f "$run/proofs.json" ]; then
    [ "$(jq -r .candidate.seal "$run/proofs.json")" = "0x$(hexfile "$C/receipts/seal.hex")" ] || { echo "verify: proofs.json seal" >&2; exit 1; }
    echo "proofs.json: candidate seal = the verified receipt's"
  fi
  rm -rf "$t"
  ;;
*) echo "unknown step $step" >&2; exit 2 ;;
esac
