#!/usr/bin/env bash
# On the Modal build box (needs Nix): install ferrata_bridge_module 0.1.1 and the mini-app ferrata_bridge_ui from the
# catalog into a headless Logos Core (logosctl 66d098d, portable), signature_policy require, the catalog's DID the only
# key in its keyring. The six core modules' install from the published catalog is ../install-test-landclimber-2026-11-24/
# (this script is its box-install.sh with these changes): the refusal inputs are made from ferrata_bridge_module's
# package; a core module is loaded and its `version` must name the release's version; a ui module is not loaded (a
# headless Logos Core has no view to show it in) but must be installed as a ui package (`package ls --type ui`); and
# `package show` must list every version the catalog offers, the newest installed.
# First what must be refused, each for its own reason: an install from the catalog before the key is trusted; then,
# the key trusted, a package with its manifest.sig removed (unsigned), the same package re-signed by a fresh key under
# the release key's signer name (another signer), and the package with one bit of its linux-amd64 plugin flipped
# (tampered). Then each package installed by name from the catalog (the URL users add), its installed rootHash compared
# with the release sidecar's. lgx (logos-package 4bdd904) verifies every downloaded .lgx against a keyring holding only
# the DID.
#
#   box-install.sh ORG CATALOG_REPO DID TAG/ASSET/ROOTHASH...
#     e.g. ... ferrata_bridge_module-v0.1.1/ferrata_bridge_module-0.1.1.lgx/<root> ferrata_bridge_ui-v0.1.0/ferrata_bridge_ui-0.1.0.lgx/<root>
#   The published catalog by default (the URLs users add). For a rehearsal against a catalog served elsewhere:
#     CATALOG_URL  the logos-repo.json URL (default https://raw.githubusercontent.com/ORG/CATALOG_REPO/refs/heads/main/logos-repo.json)
#     PKG_BASE     where TAG/ASSET is downloaded from (default https://github.com/ORG/CATALOG_REPO/releases/download)
#     REC          the record's directory (default box/ beside this script)
#   SSL_CERT_FILE names the CA bundle (default the system's).
# Record: REC (steps.tsv: name, expected, exit code, ms, pass, output; steps/<step>.txt; log.txt; versions.txt;
# daemon.log), absolute paths shortened (this repository as <repo>, the directory above it as <top>).
set -uo pipefail
org=$1 repo=$2 did=$3; shift 3
here=$(cd "$(dirname "$0")" && pwd)
repodir=$(cd "$here/../../.." && pwd); top=$(cd "$repodir/.." && pwd)
CATALOG_URL=${CATALOG_URL:-https://raw.githubusercontent.com/$org/$repo/refs/heads/main/logos-repo.json}
PKG_BASE=${PKG_BASE:-https://github.com/$org/$repo/releases/download}
W=${REC:-$here/box}; rm -rf "$W"; mkdir -p "$W/home" "$W/steps" "$W/pkgs"
S="$top/tmp/cp-s"; rm -rf "$S"; mkdir -p "$S"      # Logos Core's socket dir: at most 62 bytes
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
  echo "catalog: $CATALOG_URL"; echo "packages: $PKG_BASE"
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
installed_as() {   # installed_as MODULE VERSION TYPE: `package ls --type TYPE` lists MODULE at VERSION
  local o; o=$(ctl package ls --type "$3") || return 1
  echo "$o"; jq -e --arg n "$1" --arg v "$2" 'map(select(.name == $n and .version == $v)) | length == 1' <<< "$o"
}
versions_offered() {   # versions_offered MODULE VERSION: `package show` names the versions the catalog offers, VERSION among them
  local o; o=$(ctl package show "$1") || return 1
  echo "$o"; grep -q -F "$2" <<< "$o"
}
version_is() {   # version_is MODULE VERSION: its `version` call's reply (JSON in the call's result) has version VERSION
  local o v; o=$(ctl call "$1" version --json --timeout 60000) || return 1
  v=$(jq -r '.result | fromjson | .version' <<< "$o" 2> /dev/null)
  echo "$o"; echo "version: ${v:-none}"; [ "$v" = "$2" ]
}
fetch() { curl -fsSL --retry 3 -A ferrata-install-test -o "$2" "$1" && echo "$(sha256sum "$2" | cut -d' ' -f1)  $(basename "$2")"; }

# The packages, downloaded and verified with lgx against the DID alone.
"$lgx" keyring add ferrata "$did" --dir "$W/keyring" >> "$log" 2>&1
mods=() roots=() vers=() types=()
for a in "$@"; do
  tag=${a%%/*}; rest=${a#*/}; asset=${rest%%/*}; rh=${rest#*/}; m=${tag%-v*}
  mods+=("$m"); roots+=("$rh"); vers+=("${tag##*-v}")
  step "download-$m" ok fetch "$PKG_BASE/$tag/$asset" "$W/pkgs/$asset"
  step "lgx-verify-$m" ok "$lgx" verify "$W/pkgs/$asset" --keyring-dir "$W/keyring"
  t=$("$lgx" manifest "$W/pkgs/$asset" --json 2> /dev/null | jq -r '.type // .metadata.type // "core"'); types+=("$t")
  echo "$m: manifest type $t" >> "$log"
done

# The refusal inputs, made from ferrata_bridge_module's package (it has a linux-amd64 plugin to flip a bit of).
src=$(ls "$W"/pkgs/ferrata_bridge_module-*.lgx | head -n 1)
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
step catalog-add ok ctl catalog add "$CATALOG_URL"
step catalog-source ok ctl catalog source http
step catalog-refresh ok ctl catalog refresh
step catalog-ls ok ctl catalog ls
step search ok ctl search ferrata

# Refused: the catalog's package while the keyring is empty (the catalog's trustedSigners are not trusted by themselves).
step refuse-catalog-before-key "refused: Package signed by untrusted key" ctl install ferrata_bridge_module -y
step key-add ok ctl key add ferrata --did "$did" --display-name "Ferrata module releases"
step key-ls ok ctl key ls
step refuse-unsigned "refused: Package is unsigned" ctl package install "$W/unsigned.lgx" -y
step refuse-other-signer "refused: Package signed by untrusted key" ctl package install "$W/other-signed.lgx" -y
step refuse-tampered "refused: Content hash mismatch" ctl package install "$W/tampered.lgx" -y
step nothing-installed-after-refusals ok no_ferrata_installed

i=0
for m in "${mods[@]}"; do
  step "show-$m" ok versions_offered "$m" "${vers[$i]}"
  step "install-$m" ok ctl install "$m" -y
  step "roothash-$m" ok root_of "$m" "${roots[$i]}"
  i=$((i + 1))
done
i=0
for m in "${mods[@]}"; do
  case "${types[$i]}" in
    ui*) step "installed-as-ui-$m" ok installed_as "$m" "${vers[$i]}" ui ;;
    *) step "installed-as-core-$m" ok installed_as "$m" "${vers[$i]}" core
       step "load-$m" ok ctl module load "$m"
       step "version-$m" ok version_is "$m" "${vers[$i]}" ;;
  esac
  i=$((i + 1))
done
step package-ls ok ctl package ls
cp "$W/session/logs/daemon.log" "$W/daemon.log" 2> /dev/null || true
step daemon-stop ok ctl daemon stop
trap - EXIT
total=$(wc -l < "$W/steps.tsv"); passed=$(awk -F'\t' '$5 == 1' "$W/steps.tsv" | wc -l)
echo "box-install.sh: $passed of $total steps as expected" | tee -a "$log"
rm -rf "$W/logos-portable" "$W/lgx-tool" "$W/home" "$W/session" "$W/pkgs" "$W"/*.lgx "$W/other-key" "$W/keyring" "$S"
for f in "$W"/nix-*.log; do [ -s "$f" ] || rm -f "$f"; done
grep -rlF "$top" "$W" 2> /dev/null | while read -r f; do sed -i -e "s#$repodir#<repo>#g" -e "s#$top#<top>#g" "$f"; done
[ "$passed" = "$total" ]
