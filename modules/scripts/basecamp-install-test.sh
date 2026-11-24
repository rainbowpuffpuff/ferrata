#!/usr/bin/env bash
# The Basecamp release's install test: a headless Logos Core (logosctl) with signature_policy require and the release
# key as its only trusted signer. First what it must refuse, each for its own reason (an unsigned package, one with a
# bit flipped, one signed by another key under the same signer name); then every linux-amd64 package of the release
# installed from its signed file, each core module loaded and its `version` called; then calls on data: the
# specification's first deposit vector (bridge module), and, with FERRATA_TESTNET_RPC set, the test network's Sepolia
# light-client instance read from its RPC by this script and decoded by the attestation module, and the deposit call
# for the vault the network's wallet configuration names (built, not sent). Nothing is sent to any network.
#
#   modules/scripts/basecamp-install-test.sh REL UNSIGNED WORK
#
# REL: the signed release (REL/linux-amd64/*.lgx, REL/keys/ferrata-release.did). UNSIGNED: basecamp-release.sh build's
# OUT/linux-amd64 (the same packages before signing). WORK: a new directory; the record goes to WORK/record (steps.tsv:
# name, expected, exit code, milliseconds, pass, output; out/<step>.txt; req/; the network data read; roots.env, the
# absolute paths that basecamp_install_record.py replaces). Needs lgx (logos-package) and jq on PATH, python3, curl.
# LOGOSCTL: a logosctl to use (default: logos-logoscore-cli 66d098d's portable bundle, built with Nix into WORK).
# LOGOS_TMPDIR: a directory with a short path for Logos Core's sockets (default WORK/s; 62 bytes at most).
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
[ $# = 3 ] || { echo "usage: modules/scripts/basecamp-install-test.sh REL UNSIGNED WORK" >&2; exit 2; }
rel=$(cd "$1" && pwd) && unsigned=$(cd "$2" && pwd) || exit 2
mkdir -p "$3" && work=$(cd "$3" && pwd)
[ -z "$(ls -A "$work")" ] || { echo "basecamp-install-test.sh: $work is not empty" >&2; exit 2; }
CLI=github:logos-co/logos-logoscore-cli/66d098d3af1367cf831b69c13a5aa8345edeb682
MODULES="ferrata_attest_module ferrata_bridge_module ferrata_relayer_module ferrata_updater_module ferrata_attestor_module ferrata_monitor_module"
rec=$work/record
mkdir -p "$rec/out" "$rec/req" "$work/home"
sock=${LOGOS_TMPDIR:-$work/s}
[ ${#sock} -le 62 ] || { echo "basecamp-install-test.sh: LOGOS_TMPDIR=$sock is longer than 62 bytes (Logos Core's sockets)" >&2; exit 2; }
mkdir -p "$sock"
export TMPDIR=$sock LOGOS_TMPDIR=$sock
if [ -z "${LOGOSCTL:-}" ]; then
  if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
  nix build "$CLI#ctl-bundle-dir" -o "$work/logos-portable" > "$work/logosctl-build.log" 2>&1 \
    || { tail -n 20 "$work/logosctl-build.log" >&2; exit 1; }
  LOGOSCTL=$work/logos-portable/bin/logosctl
fi
printf 'WORK=%s\nREL=%s\nUNSIGNED=%s\nLOGOSCTL=%s\n' "$work" "$rel" "$unsigned" "$(readlink -f "$LOGOSCTL")" > "$rec/roots.env"
{ echo "logosctl $("$LOGOSCTL" --version 2>&1 | head -n 1)"; uname -srm; nix --version 2>/dev/null || echo "nix: none"; } > "$rec/versions.txt"
ses=$work/session
did=$(cat "$rel/keys/ferrata-release.did")
ctl() { HOME="$work/home" "$LOGOSCTL" --config-dir "$ses" "$@"; }
# step NAME EXPECT CMD...: EXPECT is "ok" (exit 0) or "refused: TEXT" (a non-zero exit whose output contains TEXT).
step() {
  local name=$1 expect=$2 s e rc pass; shift 2
  s=$(date +%s%3N)
  "$@" > "$rec/out/$name.txt" 2>&1; rc=$?
  e=$(date +%s%3N)
  case "$expect" in
    ok) [ $rc = 0 ] && pass=1 || pass=0 ;;
    "refused: "*) [ $rc != 0 ] && grep -q -F -- "${expect#refused: }" "$rec/out/$name.txt" && pass=1 || pass=0 ;;
  esac
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$expect" "$rc" "$(( e - s ))" "$pass" \
    "$(tr '\n\t' '  ' < "$rec/out/$name.txt" | cut -c1-400)" >> "$rec/steps.tsv"
  echo "$([ $pass = 1 ] && echo PASS || echo FAIL) $name (exit $rc, $(( e - s )) ms)"
}
: > "$rec/steps.tsv"
printf 'signature_policy: require\n' > "$rec/node.yaml"
step config-set ok ctl daemon config set "$rec/node.yaml"
step daemon-start ok ctl daemon start --detach
step key-add ok ctl key add ferrata --did "$did" --display-name "Ferrata release (test network)"
step config-show ok ctl daemon config show
step key-ls ok ctl key ls

# What must be refused: an unsigned package; the signed one with one bit of its plugin flipped (every other member,
# its order and metadata as signed); the unsigned one signed by a fresh key under the release key's signer name.
lgx keygen --name other --output-dir "$work/other-key" > /dev/null
cp "$unsigned/ferrata_attest_module-0.1.0.lgx" "$work/other-signed.lgx" && chmod u+w "$work/other-signed.lgx"
lgx sign "$work/other-signed.lgx" --key "$work/other-key/other.jwk" --name "Ferrata release (test network)" > /dev/null
python3 -I - "$rel/linux-amd64/ferrata_attest_module-0.1.0.lgx" "$work/tampered.lgx" <<'PY'
import io, sys, tarfile
src, dst = sys.argv[1], sys.argv[2]
done = False
with tarfile.open(src, "r:gz") as tin, tarfile.open(dst, "w:gz", format=tin.format) as tout:
    for m in tin.getmembers():
        data = tin.extractfile(m).read() if m.isfile() else None
        if data is not None and not done and m.name.endswith(".so"):
            b = bytearray(data); b[4096] ^= 1; data = bytes(b); done = True
        tout.addfile(m, io.BytesIO(data) if data is not None else None)
if not done:
    sys.exit("no plugin in the package")
PY
step refuse-unsigned "refused: Package is unsigned" ctl package install "$unsigned/ferrata_attest_module-0.1.0.lgx" -y
step refuse-tampered "refused: Content hash mismatch" ctl package install "$work/tampered.lgx" -y
step refuse-other-signer "refused: Package signed by untrusted key" ctl package install "$work/other-signed.lgx" -y
step package-ls-after-refusals ok ctl package ls

for m in $MODULES ferrata_bridge_ui; do
  step "install-$m" ok ctl package install "$rel/linux-amd64/$m-0.1.0.lgx" -y
done
for m in $MODULES; do
  step "load-$m" ok ctl module load "$m"
  step "version-$m" ok ctl call "$m" version --json --timeout 60000
done
step package-ls ok ctl package ls

# The bridge module on the specification's first deposit vector: its commitment must come back.
python3 -I - "$here/../../bridge/vectors/inbound-v2.json" "$rec/req" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))["cases"][0]
keys = ("bridge_seed", "evm_chain_id", "vault", "depositor", "j", "kind", "dest", "token", "leaf_index", "amount")
json.dump({k: c[k] for k in keys}, open(sys.argv[2] + "/deposit_vector.json", "w"))
json.dump({"expect_commitment": c["C"]}, open(sys.argv[2] + "/deposit_vector.expect.json", "w"))
PY
step deposit-vector ok ctl call ferrata_bridge_module deposit "@$rec/req/deposit_vector.json" --json --timeout 60000

# The test network's data, read here (the modules make no network call in these steps).
if [ -n "${FERRATA_TESTNET_RPC:-}" ]; then
  base=${FERRATA_TESTNET_RPC%/rpc}
  date -u +%Y-%m-%dT%H:%M:%SZ > "$rec/live-read-utc.txt"
  curl -fsS -m 30 "$base/programs.json" > "$rec/live-programs.json"
  curl -fsS -m 30 "$base/wallet/config.json" > "$rec/live-wallet-config.json"
  acct=$(jq -r '.instances.sepolia.instance_account' "$rec/live-programs.json")
  prog=$(jq -r '.instances.sepolia.program' "$rec/live-programs.json")
  curl -fsS -m 30 -X POST -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getAccount\",\"params\":[\"$acct\"]}" "$FERRATA_TESTNET_RPC" > "$rec/live-account.json"
  curl -fsS -m 30 "$base/status/status.json" > "$rec/live-status.json"
  python3 -I - "$rec/live-account.json" "$prog" "$rec/req/read_instance.json" <<'PY'
import json, sys
acc = json.load(open(sys.argv[1]))["result"]
data = json.loads(acc["data"]) if isinstance(acc["data"], str) else acc["data"]
json.dump({"state": "0x" + bytes(data["actor_states"][sys.argv[2]]).hex()}, open(sys.argv[3], "w"))
PY
  step live-read-instance ok ctl call ferrata_attest_module read_instance "@$rec/req/read_instance.json" --json --timeout 60000
  python3 -I - "$here/../../bridge/vectors/inbound-v2.json" "$rec/live-wallet-config.json" "$rec/req/deposit_live_vault.json" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))["cases"][0]
b = json.load(open(sys.argv[2]))["bridge"]
req = {k: c[k] for k in ("bridge_seed", "depositor", "j", "kind", "dest")}
req.update({"chain_id": b["evmChainId"], "vault": b["vault"], "token": "0x" + "00" * 20})
json.dump(req, open(sys.argv[3], "w"))
PY
  step deposit-live-vault ok ctl call ferrata_bridge_module deposit "@$rec/req/deposit_live_vault.json" --json --timeout 60000
fi
step daemon-stop ok ctl daemon stop
total=$(wc -l < "$rec/steps.tsv")
passed=$(awk -F'\t' '$5 == 1' "$rec/steps.tsv" | wc -l)
echo "basecamp-install-test.sh: $passed of $total steps as expected; record in $rec"
[ "$passed" = "$total" ]
