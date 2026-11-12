#!/usr/bin/env bash
# One release proven for real through Logos Core: a fresh logosctl daemon started WITHOUT
# RISC0_DEV_MODE, ferrata_bridge_module installed and loaded, `prove_release` called with a release
# request (default: the one bridge_e2e.sh made for burn 0); the module runs the release guest through
# r0vm (a succinct proof on the host's CPU) and verifies the receipt against the image id.
#
#   modules/scripts/real_release_proof.sh LOGOSCTL BRIDGE_LGX_DIR RELEASE_ELF OUT_DIR [REQUEST]
#
# Needs: the dev logosctl and the bridge module's dev package (check.sh builds both), r0vm 3.0.5
# (RISC0_SERVER_PATH or on PATH), jq; TMPDIR short enough for the daemon's socket (a Unix socket path
# is at most 107 bytes). Writes OUT_DIR/{request.json,reply.raw,reply.json,receipt-succinct.json,timing.txt}.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
LOGOSCTL=$1 LGX=$2 ELF=$3
mkdir -p "$4"
out=$(cd "$4" && pwd)
REQUEST=${5:-$repo/modules/results/complete-2026-11-12/bridge-e2e-native/req/prove_release0.json}
spike=$(cd "$repo/.." && pwd)
S="$out/session"
rm -rf "$S" "$out/home"
mkdir -p "$out/home"
ctl() { "$LOGOSCTL" --config-dir "$S" "$@"; }
env -u RISC0_DEV_MODE HOME="$out/home" "$LOGOSCTL" --config-dir "$S" daemon start --detach > "$out/daemon.txt" 2>&1
# At exit: the daemon stopped, the last lines of its log kept, its session and home removed (they hold
# the installed package and the daemon's state, not results).
finish() {
  ctl daemon stop >> "$out/daemon.txt" 2>&1 || true
  tail -n 20 "$S"/logs/daemon_*.log > "$out/daemon-log-tail.txt" 2>/dev/null || true
  sed -i "s|$spike/|<spike>/|g" "$out/daemon-log-tail.txt" 2>/dev/null || true
  rm -rf "$S" "$out/home"
}
trap finish EXIT
ctl install "$LGX"/*.lgx -y >> "$out/daemon.txt" 2>&1
ctl module load ferrata_bridge_module >> "$out/daemon.txt" 2>&1
# The request names the release guest by path: this machine's.
jq --arg elf "$ELF" --arg out "$out/receipt-succinct.json" \
  '.program = $elf | . + {expected_image_id: "0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9", receipt_out: $out}' \
  "$REQUEST" > "$out/request.json"
t0=$(date +%s.%N)
ctl call ferrata_bridge_module prove_release "@$out/request.json" --json < /dev/null > "$out/reply.raw" 2>&1
t1=$(date +%s.%N)
jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' "$out/reply.raw" > "$out/reply.json"
{
  echo "logosctl call seconds $(awk -v a="$t0" -v b="$t1" 'BEGIN { printf "%.1f", b - a }')"
  echo "nproc $(nproc)"
  echo "RISC0_DEV_MODE in the daemon: unset"
} > "$out/timing.txt"
[ -f "$out/receipt-succinct.json" ] && { sha256sum "$out/receipt-succinct.json" | cut -c1-64; stat -c %s "$out/receipt-succinct.json"; } > "$out/receipt-succinct.sha256"
find "$out" -maxdepth 1 -type f \( -name '*.json' -o -name '*.raw' -o -name '*.txt' \) ! -name 'receipt-succinct.json' -exec sed -i "s|$spike/|<spike>/|g" {} +
jq -e '.verdict == "proven" and .dev_mode == false and .image_id_matches == true' "$out/reply.json"
