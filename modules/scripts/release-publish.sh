#!/usr/bin/env bash
# Put everything in place so that publishing the module catalog is one owner step: check the inputs,
# generate the catalog's logos-repo.json (trustedSigners from the key the owner provides, indexUrl
# from ORG/CATALOG_REPO), export every module as its own repository (export.sh), assemble the catalog
# repository (the catalog files at its root, each module a git submodule), and print the exact gh and
# git commands the owner runs: create the repositories, set the signing secret, push, tag, trigger
# the release workflows. Nothing is published from here: the commands run only with --yes, and with
# --dry-run DIR everything is created as local bare git repositories under DIR (file:// remotes) and
# no network command runs at all. The lambda-prize release-integrity mapping is modules/catalog/RELEASE.md.
#
#   modules/scripts/release-publish.sh ORG CATALOG_REPO SIGNING_KEY_FILE [--dry-run DIR] [--yes]
#
# ORG/CATALOG_REPO: the GitHub organization and the catalog repository to publish under
#   (e.g. ferrata-co ferrata-modules). SIGNING_KEY_FILE: the Ed25519 private key whose public half
#   becomes the catalog's trusted signer, in the format logos-modules-release-action's inline signing
#   reads (a JWK — {"crv":"Ed25519","d":<seed>,"kty":"OKP","x":<pubkey>}, what `lgx keygen` writes and
#   what the action's release.yml stores as the LOGOS_SIGNING_KEY secret). The private key is read at
#   run time to derive the DID and (with --yes) to feed `gh secret set`; it is never copied, never
#   written under any repository. Everything else the script writes lives in TMPDIR.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
MODULES_DIR=$repo/modules
CATALOG_DIR=$repo/modules/catalog

org=$1; catalog_repo=$2; key_file=$3; shift 3
dry_dir=; yes=
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) dry_dir=$2; shift 2 ;;
    --yes) yes=1; shift ;;
    *) echo "release-publish.sh: unknown argument '$1'" >&2; exit 2 ;;
  esac
done
[ -n "${TMPDIR:-}" ] || { echo "release-publish.sh: set TMPDIR (a scratch directory); refusing to fall back to /tmp" >&2; exit 2; }
case "$org" in ''|*[!A-Za-z0-9_.-]*|*/*) echo "release-publish.sh: bad ORG '$org'" >&2; exit 2 ;; esac
case "$catalog_repo" in ''|*[!A-Za-z0-9_.-]*|*/*) echo "release-publish.sh: bad CATALOG_REPO '$catalog_repo'" >&2; exit 2 ;; esac
[ -f "$key_file" ] || { echo "release-publish.sh: no signing-key file '$key_file'" >&2; exit 2; }
[ -n "$dry_dir" ] && [ -n "$yes" ] && { echo "release-publish.sh: --dry-run and --yes are mutually exclusive" >&2; exit 2; }
command -v jq >/dev/null || { echo "release-publish.sh: jq is required" >&2; exit 2; }
command -v python3 >/dev/null || { echo "release-publish.sh: python3 is required" >&2; exit 2; }

# The key: read once, check it is the JWK the action's inline signing reads, derive the signer's
# did:jwk from its public half (lgx's construction: did:jwk:base64url({"crv","kty","x"}), keys
# alphabetical, no padding). Only the public DID leaves this check.
did=$(python3 -I -c '
import base64, json, sys
p = sys.argv[1]
try:
    jwk = json.load(open(p))
except Exception as e:
    sys.exit(f"release-publish.sh: {p} is not JSON: {e}")
def b64u_len(s):
    try:
        r = base64.urlsafe_b64decode(s + "=" * (-len(s) % 4))
    except Exception:
        sys.exit(f"release-publish.sh: {p}: not base64url: {s!r}")
    return len(r)
for f in ("crv", "d", "kty", "x"):
    if f not in jwk:
        sys.exit(f"release-publish.sh: {p}: JWK has no {f!r} field (expected an Ed25519 secret key as lgx keygen writes: crv, d, kty, x)")
if jwk["kty"] != "OKP" or jwk["crv"] != "Ed25519":
    sys.exit(f"release-publish.sh: {p}: not an Ed25519 OKP JWK (kty={jwk.get('kty')!r}, crv={jwk.get('crv')!r})")
if b64u_len(jwk["d"]) != 32:
    sys.exit(f"release-publish.sh: {p}: d is not a 32-byte Ed25519 seed")
if b64u_len(jwk["x"]) != 32:
    sys.exit(f"release-publish.sh: {p}: x is not a 32-byte Ed25519 public key")
pub = {"crv": "Ed25519", "kty": "OKP", "x": jwk["x"]}
print("did:jwk:" + base64.urlsafe_b64encode(json.dumps(pub, separators=(",", ":"), sort_keys=True).encode()).rstrip(b"=").decode())
' "$key_file") || exit 1
case "$did" in did:jwk:*) ;; *) echo "release-publish.sh: deriving the DID failed" >&2; exit 1 ;; esac

# The modules: every modules/<name>/metadata.json (release-local.sh's MODULES names a subset, e.g. when
# one module's export needs an artifact this machine does not have: ferrata_updater_module's stage
# wants lez/methods' built guests — `cd lez && cargo build --locked -p methods`, RISC0_HOME as
# lez/RESULTS.md says; the published catalog takes all six).
modules=()
if [ -n "${MODULES:-}" ]; then
  for m in $MODULES; do
    [ -f "$MODULES_DIR/$m/metadata.json" ] || { echo "release-publish.sh: no module $m under $MODULES_DIR" >&2; exit 2; }
    [ "$(jq -r .name "$MODULES_DIR/$m/metadata.json")" = "$m" ] || { echo "release-publish.sh: modules/$m/metadata.json names $(jq -r .name "$MODULES_DIR/$m/metadata.json")" >&2; exit 1; }
    modules+=("$m")
  done
else
  for d in "$MODULES_DIR"/*_module; do
    m=$(basename "$d")
    [ -f "$d/metadata.json" ] || continue
    [ "$(jq -r .name "$d/metadata.json")" = "$m" ] || { echo "release-publish.sh: $d/metadata.json names $(jq -r .name "$d/metadata.json")" >&2; exit 1; }
    modules+=("$m")
  done
fi
[ ${#modules[@]} -gt 0 ] || { echo "release-publish.sh: no modules found under $MODULES_DIR" >&2; exit 2; }

# The workspace; everything the script writes (exported repositories, the assembled catalog) goes
# here, outside every repository. Recreated on each run; the private key is never written into it.
ws=$(mktemp -d "${TMPDIR%/}/release-publish.XXXXXX")
mkdir -p "$ws/src"

echo "release-publish.sh: publishing $org/$catalog_repo"
echo "  modules (${#modules[@]}): ${modules[*]}"
echo "  signer DID: $did"
[ -n "$dry_dir" ] && echo "  DRY RUN: everything local under $dry_dir (bare repositories); no network command runs"
[ -n "$yes" ] && echo "  --yes: the printed commands below are run by this script"

# ── 1. Export each module as its own repository ────────────────────────────────────────────────
declare -A version
for m in "${modules[@]}"; do
  echo "  export $m"
  "$here/export.sh" "$m" "$ws/src/$m" > /dev/null
  git -C "$ws/src/$m" branch -M main
  version[$m]=$(jq -r .version "$MODULES_DIR/$m/metadata.json")
done

# ── 2. The generated logos-repo.json: identity kept, indexUrl and trustedSigners written ───────
index_url="https://github.com/$org/$catalog_repo/releases/download/index/index.json"
mkdir -p "$ws/catalog/.github/workflows"
cp "$CATALOG_DIR/.github/workflows/"*.yml "$ws/catalog/.github/workflows/"
jq --arg indexUrl "$index_url" --arg did "$did" \
   '.indexUrl = $indexUrl
  | .trustedSigners = [ { did: $did, name: "Ferrata module releases" } ]' \
   "$CATALOG_DIR/logos-repo.json" > "$ws/catalog/logos-repo.json"
python3 -I -m json.tool "$ws/catalog/logos-repo.json" > /dev/null   # generated file is valid JSON
cat > "$ws/catalog/README.md" <<EOF
# Ferrata modules

Ferrata's Logos module catalog: \`logos-repo.json\` (identity, the rolling \`index\` release in
\`indexUrl\`, the trusted signer in \`trustedSigners\`), the modules as git submodules under
\`submodules/\`, and the release workflows that build, sign, verify and publish each one with
[\`logos-co/logos-modules-release-action\`](https://github.com/logos-co/logos-modules-release-action)
(pinned to a commit). How it is published, and each lambda-prize release-integrity rule it meets:
\`RELEASE.md\` in the source repository (https://github.com/rainbowpuffpuff/ferrata).

Install (Logos Core's \`logosctl\`; the same URL works in the package-manager UI and with \`lgpd\`):

    logosctl catalog add https://raw.githubusercontent.com/$org/$catalog_repo/refs/heads/main/logos-repo.json
    logosctl catalog refresh
    logosctl search ferrata
    logosctl install ferrata_attest_module -y

The modules are prototype, unaudited, not for production.
EOF

# ── 3. Assemble the catalog repository: each module a submodule pointing at its repository ─────
git init -q "$ws/catalog"
git -C "$ws/catalog" checkout -q -b main 2>/dev/null || true
for m in "${modules[@]}"; do
  if [ -n "$dry_dir" ]; then
    url="file://$dry_dir/$m.git"          # functional offline: what the catalog would clone
  else
    url="https://github.com/$org/$m.git"  # what the published catalog points at
  fi
  # Clone from the exported repository beside it (a plain local path, so git's file-transport guard
  # needs protocol.file.allow=always on this one command: every path involved is one this script
  # created in TMPDIR), then record the URL the published catalog points at in .gitmodules.
  git -C "$ws/catalog" -c protocol.file.allow=always submodule add -q "$ws/src/$m" "submodules/$m"
  git -C "$ws/catalog" config -f .gitmodules "submodule.submodules/$m.url" "$url"
  git -C "$ws/catalog/submodules/$m" remote set-url origin "$url"
done
git -C "$ws/catalog" add -A
git -C "$ws/catalog" -c user.name=ferrata-release-publish -c user.email=publish@localhost commit -q \
  -m "Ferrata module catalog: ${#modules[@]} modules, signing on, action pinned (assembled by modules/scripts/release-publish.sh)"

# ── 4. Dry-run: push everything to local bare repositories, then report ────────────────────────
if [ -n "$dry_dir" ]; then
  mkdir -p "$dry_dir"
  dry_dir=$(cd "$dry_dir" && pwd)
  echo "  bare repositories under $dry_dir:"
  for m in "${modules[@]}"; do
    rm -rf "$dry_dir/$m.git"   # a previous dry run's repository of the same name: replaced
    git init -q --bare "$dry_dir/$m.git"
    git -C "$ws/src/$m" remote remove origin 2>/dev/null || true
    git -C "$ws/src/$m" remote add origin "file://$dry_dir/$m.git"
    git -C "$ws/src/$m" push -q origin main
    echo "    $m.git  commit $(git -C "$ws/src/$m" rev-parse HEAD) (main, v${version[$m]})"
  done
  rm -rf "$dry_dir/$catalog_repo.git"   # as above
  git init -q --bare "$dry_dir/$catalog_repo.git"
  git -C "$ws/catalog" remote add origin "file://$dry_dir/$catalog_repo.git"
  git -C "$ws/catalog" push -q origin main
  echo "    $catalog_repo.git  commit $(git -C "$ws/catalog" rev-parse HEAD) (main)"
fi

# ── 5. The commands the owner runs (repo creation, secret, push, tag, trigger); --yes runs them ─
run() {   # the automatable commands of the one owner step; printed always, run only with --yes
  echo "  \$ $*"
  if [ -n "$yes" ] && [ -z "$dry_dir" ]; then
    eval "$*"
  fi
}
man() {   # the settings with no gh command this offline setup could verify; never automated
  echo "  [owner, in the browser] $*"
}

echo; echo "The commands${dry_dir:+ (as emulated by the dry run; none of them runs)} the owner runs:"
echo "WS=$ws   # the exported repositories (\$WS/src/<module>) and the assembled catalog (\$WS/catalog)"
for m in "${modules[@]}"; do
  v=${version[$m]}
  run "gh repo create '$org/$m' --public --description 'Ferrata $m — Logos Core module (prototype, unaudited)'"
  run "git -C \"\$WS/src/$m\" remote add origin 'https://github.com/$org/$m.git'"
  run "git -C \"\$WS/src/$m\" push -u origin main"
  run "git -C \"\$WS/src/$m\" tag -a 'v$v' -m '$m $v, exported from the Ferrata repository'"
  run "git -C \"\$WS/src/$m\" push origin 'v$v'"
done
run "gh repo create '$org/$catalog_repo' --public --description \"Ferrata's Logos module catalog\""
run "git -C \"\$WS/catalog\" remote add origin 'https://github.com/$org/$catalog_repo.git'"
run "git -C \"\$WS/catalog\" push -u origin main"
run "gh secret set LOGOS_SIGNING_KEY --repo '$org/$catalog_repo' < '$key_file'   # read once; stored as the Actions secret the workflows pass as signing_key"
man "in $org/$catalog_repo and each $org/<module>: enable the immutable-releases repository setting; add a ruleset protecting refs/tags/* (the release tags <module>-v<version>); require 2FA for the $org organization; keep every token least-privilege (the workflows use the built-in GITHUB_TOKEN, contents alone — see their per-job permissions and modules/catalog/RELEASE.md)."
run "gh workflow run release-all.yml --repo '$org/$catalog_repo'   # builds, signs, verifies and releases every module; then rebuild-index rolls up index.json"

echo; echo "Then: fill releases/RELEASE-INTEGRITY.template.md (each value's command is in it) with the"
echo "release URLs, the source commit SHAs, the signer DID and the rootHashes the sidecars print."
echo; echo "release-publish.sh: done.${dry_dir:+ Dry run: repositories under $dry_dir.}${yes:+ Commands were run as printed above; the bracketed browser steps remain for the owner.}"
