#!/usr/bin/env bash
# On the Modal build box (needs Nix): install every module from the PUBLISHED catalog into a headless Logos Core
# (logosctl 66d098d, portable), signature_policy require, the catalog's DID the only key in its keyring.
# First what must be refused, each for its own reason (as modules/scripts/basecamp-install-test.sh): an install from
# the catalog before the key is trusted; then, the key trusted, a published package with its manifest.sig removed
# (unsigned), the same package re-signed by a fresh key under the release key's signer name (another signer), and the
# published package with one bit of its linux-amd64 plugin flipped (tampered). Then each module installed from the
# catalog (the URL users add), its installed rootHash compared with the release sidecar's, loaded, `version` called.
# lgx (logos-package 4bdd904) verifies every downloaded .lgx against a keyring holding only the DID.
#   box-install.sh ORG CATALOG_REPO DID TAG/ASSET/ROOTHASH...
# Record: box/ beside this script (steps.tsv: name, expected, exit code, ms, pass, output; steps/<step>.txt; log.txt;
# versions.txt; daemon.log), absolute paths replaced by "spike".
set -uo pipefail
org=$1 repo=$2 did=$3; shift 3
here=$(cd "$(dirname "$0")" && pwd)
spike=$(cd "$here/../../../.." && pwd)
W="$here/box"; rm -rf "$W"; mkdir -p "$W/home" "$W/steps" "$W/pkgs"
S="$spike/tmp/ck-s"; rm -rf "$S"; mkdir -p "$S"      # Logos Core's socket dir: at most 62 bytes
export TMPDIR="$S" LOGOS_TMPDIR="$S"
if [ -z "${SSL_CERT_FILE:-}" ] && [ -f /etc/ssl/certs/ca-certificates.crt ]; then export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; fi
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
log="$W/log.txt"; : > "$log"
command -v nix > /dev/null || { echo "box-install.sh: no nix on this box (add a setup file)" | tee -a "$log"; exit 1; }
nixb() { local a; for a in 1 2 3 4 5 6; do [ -d /homeless-shelter ] && rm -rf /homeless-shelter; nix build "$@" && return 0; sleep 1; done; return 1; }
t0=$(date +%s)
nixb github:logos-co/logos-logoscore-cli/66d098d3af1367cf831b69c13a5aa8345edeb682#ctl-bundle-dir -o "$W/logos-portable" > "$W/nix-logosctl.log" 2>&1 || { echo "logosctl build failed" | tee -a "$log"; exit 1; }
nixb github:logos-co/logos-package/4bdd9041ebfbd06c2b22134afdcd27fce8a43f3c#lgx -o "$W/lgx-tool" > "$W/nix-lgx.log" 2>&1 || { echo "lgx build failed" | tee -a "$log"; exit 1; }
echo "nix builds: $(( $(date +%s) - t0 )) s" >> "$log"
ctl_bin="$W/logos-portable/bin/logosctl"
lgx="$W/lgx-tool/bin/lgx"
{ echo "logosctl $("$ctl_bin" --version 2>&1 | head -n 1) (logos-logoscore-cli 66d098d3af1367cf831b69c13a5aa8345edeb682, ctl-bundle-dir)"
  echo "lgx $("$lgx" --version 2>&1 | head -n 1) (logos-package 4bdd9041ebfbd06c2b22134afdcd27fce8a43f3c)"
  uname -srm; nix --version; echo "SSL_CERT_FILE=${SSL_CERT_FILE:-unset}"; echo "RISC0_DEV_MODE=1 (the daemon's environment)"
  date -u +%Y-%m-%dT%H:%M:%SZ; } > "$W/versions.txt" 2>&1
ctl() { HOME="$W/home" RISC0_DEV_MODE=1 "$ctl_bin" --config-dir "$W/session" "$@"; }
# step NAME EXPECT CMD...: EXPECT is "ok" (exit 0) or "refused: TEXT" (a non-zero exit whose output contains TEXT).
step() {
  local name=$1 expect=$2 s e rc pass; shift 2
  s=$(date +%s%3N)
  "$@" > "$W/steps/$name.txt" 2>&1; rc=$?
  e=$(date +%s%3N)
  case "$expect" in
    ok) [ $rc = 0 ] && pass=1 || pass=0 ;;
    "refused: "*) [ $rc != 0 ] && grep -q -F -- "${expect#refused: }" "$W/steps/$name.txt" && pass=1 || pass=0 ;;
  esac
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$expect" "$rc" "$(( e - s ))" "$pass" \
    "$(tr '\n\t' '  ' < "$W/steps/$name.txt" | cut -c1-400)" >> "$W/steps.tsv"
  echo "$([ $pass = 1 ] && echo PASS || echo FAIL) $name (exit $rc, $(( e - s )) ms)" | tee -a "$log"
}
: > "$W/steps.tsv"
no_ferrata_installed() { local o; o=$(ctl package ls) || return 1; echo "$o"; jq -e 'map(select(.name | startswith("ferrata_"))) | length == 0' <<< "$o"; }
root_of() {   # root_of MODULE ROOT: the installed package's hashes.root equals ROOT
  local o r; o=$(ctl package ls) || return 1
  r=$(jq -r --arg n "$1" '.[] | select(.name == $n) | .hashes.root // empty' <<< "$o" 2> /dev/null | head -n 1)
  echo "installed $1 rootHash: ${r:-none}; release sidecar: $2"; [ -n "$r" ] && [ "$r" = "$2" ]
}
fetch() { curl -fsSL --retry 3 -A catalog-live-box -o "$2" "$1" && echo "$(sha256sum "$2" | cut -d' ' -f1)  $(basename "$2")"; }

# The published packages, downloaded and verified with lgx against the DID alone.
"$lgx" keyring add ferrata "$did" --dir "$W/keyring" >> "$log" 2>&1
mods=() roots=()
for a in "$@"; do
  tag=${a%%/*}; rest=${a#*/}; asset=${rest%%/*}; rh=${rest#*/}; m=${tag%-v*}
  mods+=("$m"); roots+=("$rh")
  step "download-$m" ok fetch "https://github.com/$org/$repo/releases/download/$tag/$asset" "$W/pkgs/$asset"
  step "lgx-verify-$m" ok "$lgx" verify "$W/pkgs/$asset" --keyring-dir "$W/keyring"
done

# The refusal inputs, made from the published ferrata_attest_module package.
src=$(ls "$W"/pkgs/ferrata_attest_module-*.lgx | head -n 1)
python3 -I - "$src" "$W/unsigned.lgx" "$W/tampered.lgx" <<'PY'
import io, sys, tarfile
src, unsigned, tampered = sys.argv[1:4]
def repack(dst, drop_sig, flip):
    done = False
    with tarfile.open(src, "r:gz") as tin, tarfile.open(dst, "w:gz", format=tin.format) as tout:
        for m in tin.getmembers():
            if drop_sig and m.name.lstrip("./") == "manifest.sig":
                continue
            data = tin.extractfile(m).read() if m.isfile() else None
            if flip and data is not None and not done and m.name.startswith("variants/linux-amd64/") and m.name.endswith("_plugin.so"):
                b = bytearray(data); b[4096] ^= 1; data = bytes(b); done = True
            tout.addfile(m, io.BytesIO(data) if data is not None else None)
    if flip and not done:
        sys.exit("no linux-amd64 plugin in the package")
repack(unsigned, True, False)
repack(tampered, False, True)
PY
"$lgx" keygen --name other --output-dir "$W/other-key" > /dev/null 2>&1
cp "$W/unsigned.lgx" "$W/other-signed.lgx"
"$lgx" sign "$W/other-signed.lgx" --key "$W/other-key/other.jwk" --name "Ferrata module releases" >> "$log" 2>&1
step lgx-signature-unsigned ok sh -c '[ -s "$2" ] && s=$("$1" signature "$2") && [ -z "$s" ] && echo "unsigned: lgx signature prints nothing"' sh "$lgx" "$W/unsigned.lgx"
step lgx-signature-other-signer ok sh -c '[ -s "$2" ] && "$1" signature "$2" | jq -e --arg d "$3" "{did, name} | select(.did != \$d)"' sh "$lgx" "$W/other-signed.lgx" "$did"

printf 'signature_policy: require\n' > "$W/node.yaml"
step config-set ok ctl daemon config set "$W/node.yaml"
step daemon-start ok ctl daemon start --detach
trap 'ctl daemon stop > /dev/null 2>&1' EXIT
step config-show ok ctl daemon config show
default=$(ctl catalog ls 2> /dev/null | jq -r '.[] | select(.isDefault == true) | .url // empty' 2> /dev/null | head -n 1)
[ -n "$default" ] && step catalog-disable-default ok ctl catalog disable "$default"
step catalog-add ok ctl catalog add "https://raw.githubusercontent.com/$org/$repo/refs/heads/main/logos-repo.json"
step catalog-source ok ctl catalog source http
step catalog-refresh ok ctl catalog refresh
step catalog-ls ok ctl catalog ls
step search ok ctl search ferrata

# Refused: the catalog's package while the keyring is empty (the catalog's trustedSigners are not trusted by themselves).
step refuse-catalog-before-key "refused: Package signed by untrusted key" ctl install ferrata_attest_module -y
step key-add ok ctl key add ferrata --did "$did" --display-name "Ferrata module releases"
step key-ls ok ctl key ls
step refuse-unsigned "refused: Package is unsigned" ctl package install "$W/unsigned.lgx" -y
step refuse-other-signer "refused: Package signed by untrusted key" ctl package install "$W/other-signed.lgx" -y
step refuse-tampered "refused: Content hash mismatch" ctl package install "$W/tampered.lgx" -y
step nothing-installed-after-refusals ok no_ferrata_installed

i=0
for m in "${mods[@]}"; do
  step "install-$m" ok ctl install "$m" -y
  step "roothash-$m" ok root_of "$m" "${roots[$i]}"
  i=$((i + 1))
done
for m in "${mods[@]}"; do
  step "load-$m" ok ctl module load "$m"
  step "version-$m" ok ctl call "$m" version --json --timeout 60000
done
step package-ls ok ctl package ls
cp "$W/session/logs/daemon.log" "$W/daemon.log" 2> /dev/null || true
step daemon-stop ok ctl daemon stop
trap - EXIT
total=$(wc -l < "$W/steps.tsv"); passed=$(awk -F'\t' '$5 == 1' "$W/steps.tsv" | wc -l)
echo "box-install.sh: $passed of $total steps as expected" | tee -a "$log"
rm -rf "$W/logos-portable" "$W/lgx-tool" "$W/home" "$W/session" "$W/pkgs" "$W"/*.lgx "$W/other-key" "$W/keyring" "$S"
for f in "$W"/nix-*.log; do [ -s "$f" ] || rm -f "$f"; done
grep -rlF "$spike" "$W" 2> /dev/null | while read -r f; do sed -i "s#$spike#spike#g" "$f"; done
[ "$passed" = "$total" ]
