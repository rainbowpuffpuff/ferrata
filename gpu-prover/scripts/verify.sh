#!/usr/bin/env bash
# Verifies every downloaded receipt on this machine (no proving; milliseconds each): it must be a real
# proof (succinct or Groth16, not a dev-mode receipt), verify against the image id the local
# ferrata-checkpoint host reports (`host ids`), and carry the journal of the native run (the
# fixture's "journal" in ../checkpoint/fixtures/<name>.json). Also recomputes the image
# ids of the staged programs. Writes results/verify-local.jsonl.
#
# usage: scripts/verify.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
fc="$(cd "$here/../checkpoint" && pwd)"
unset RISC0_DEV_MODE
v="$here/bin/ferrata-modal-verify"
ids="$("$fc/target/release/host" ids)"
id_of() { awk -v k="$1" '$1==k {print $2}' <<<"$ids"; }
out="$here/results/verify-local.jsonl"
: > "$out"
{
  echo "{\"local_host_ids\": {\"checkpoint\": \"$(id_of checkpoint)\", \"segment\": \"$(id_of segment)\"}, \"verified_at\": \"$(date -Is)\", \"machine\": \"$(uname -srm)\"}"
  "$v" image-id "$here/inputs/programs/checkpoint.bin" "$here/inputs/programs/segment.bin"
  for r in "$here"/results/*/*/receipt-*.bin; do
    name="$(basename "$(dirname "$r")")"
    kind="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["kind"])' "$fc/fixtures/$name.json")"
    "$v" verify "$r" "$(id_of "$kind")" "$fc/fixtures/$name.json"
  done
} | tee "$out"
# Negative controls on the first checkpoint receipt: another image id and another fixture's journal
# must both be refused.
first="$(ls "$here"/results/*/cp-empty-1/receipt-succinct.bin 2>/dev/null | head -1)"
if [ -n "$first" ]; then
  if "$v" verify "$first" "$(id_of segment)" "$fc/fixtures/cp-empty-1.json" >/dev/null 2>&1; then
    echo "NEGATIVE CONTROL FAILED: accepted under the segment image id" >&2; exit 1; fi
  if "$v" verify "$first" "$(id_of checkpoint)" "$fc/fixtures/cp-empty-360.json" >/dev/null 2>&1; then
    echo "NEGATIVE CONTROL FAILED: accepted with cp-empty-360's journal" >&2; exit 1; fi
  echo "negative controls: $first refused under the segment image id and with cp-empty-360's journal"
fi
python3 -I - "$out" <<'EOF'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1])]
host = rows[0]["local_host_ids"]
ids = {r["program"].rsplit("/", 1)[-1].removesuffix(".bin"): r["image_id"] for r in rows if "program" in r}
assert ids == host, f"staged programs' image ids {ids} != local host ids {host}"
receipts = [r for r in rows if "receipt" in r]
assert receipts and all(r["verifies"] and r["journal_equals_native"] for r in receipts)
print(f"OK: staged programs hash to the local host's image ids; {len(receipts)} receipt(s) verify against them with the native journals")
EOF
