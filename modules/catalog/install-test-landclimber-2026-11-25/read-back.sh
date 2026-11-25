#!/usr/bin/env bash
# after-release.sh ORG CATALOG_REPO   (catalog-kit's after-release.sh, not in the repository, adapted to the update)
#
# Read-only, as the original: unauthenticated HTTPS GETs (curl), `git ls-remote` / `git clone`, and `gh api -X GET`.
# Nothing is written to GitHub. Sections 1-5 are the original's checks, with these changes for a catalog whose modules
# come from two publications (24 Nov: five modules at 98375f82; 25 Nov: ferrata_bridge_module 0.1.1 and the new
# ferrata_bridge_ui at f9623cfd):
#   - each module's pointer is compared with its own publication's offline dry run (UPDATE_MODULES: DRY_UPDATE, the
#     update's `release-publish.sh --update --dry-run`; the others: DRY, the first publication's dry run);
#   - the source commit is read from STAGED.json where the export has one, and from the module commit's message
#     ("exported from the Ferrata repository at <sha>"), which a QML-only ui module's export carries alone; it must be
#     SRC_UPDATE for UPDATE_MODULES and SRC_BASE for the others, both on public main of rainbowpuffpuff/ferrata
#     (GitHub's compare API: "identical" or "ahead"), replacing "one source commit" and "the kit's worktree HEAD";
#   - OLD_TAGS (releases superseded by a newer version, still offered): present, immutable, signed by the DID, rootHash
#     equal to OLD_ROOTS, in the index;
#   - the index: N_PACKAGES packages and N_VERSIONS versions;
#   - the release runs (RUNS): conclusion, workflow path, every build leg, the action's pin in _release-module.yml at
#     the run's commit, the submodules at the run's commit and at each release tag equal to main's;
#   - section 6 writes checks.txt and values.json (no box script: modules/catalog/install-test-plus/box-install.sh is
#     the repository's).
# Exit: 0 when every check passed, 1 when one failed, 2 on bad usage. "unknown" never fails the run.
set -uo pipefail
[ $# = 2 ] || { echo "usage: after-release.sh ORG CATALOG_REPO" >&2; exit 2; }
org=$1 repo=$2
case "$org" in ''|-*|*[!A-Za-z0-9-]*) echo "bad ORG" >&2; exit 2 ;; esac
case "$repo" in ''|-*|.*|*[!A-Za-z0-9_.-]*) echo "bad CATALOG_REPO" >&2; exit 2 ;; esac
for t in curl git jq python3 sha256sum gh; do command -v "$t" > /dev/null || { echo "$t is required" >&2; exit 2; }; done

here=$(cd "$(dirname "$0")" && pwd)
top=$(cd "$here/../../../.." && pwd)      # the directory above the repository
dry=${DRY:?DRY: the first publication's release-publish.sh --dry-run directory}
dry_update=${DRY_UPDATE:?DRY_UPDATE: the update's release-publish.sh --update --dry-run directory}
update_modules=" ${UPDATE_MODULES:-ferrata_bridge_module ferrata_bridge_ui} "
src_update=${SRC_UPDATE:-f9623cfd90ae0af852d2dc7000e90821e8381f06}
src_base=${SRC_BASE:-98375f82c754f2fa4a56e20418687a89b321228f}
old_tags=${OLD_TAGS:-ferrata_bridge_module-v0.1.0}
old_roots=${OLD_ROOTS:-0e46fbecf4c39caa9880bac11bc53c9abab4a4c703b9af9fa34db6a4f166fdd8}
runs=${RUNS:-45543214624 45543219299}
action_pin=${ACTION_PIN:-bbdc52871dc4305c8007923e1b08738ad2dd6b50}
n_packages=${N_PACKAGES:-7} n_versions=${N_VERSIONS:-8}
tool=${RELEASE_TOOL:-$top/upstream/logos-modules-release-tool}
out=${OUT:-$here/after/$org-$repo}
export TMPDIR=$here/tmp
mkdir -p "$out" "$TMPDIR"
log=$out/checks.txt
: > "$log"

UA=ferrata-read-back
get() { curl -fsSL --retry 2 -A "$UA" "$@"; }
api() { gh api -X GET "$1" 2> /dev/null; }
fails=0 passes=0 unknowns=0
ok() { passes=$((passes + 1)); echo "ok      $*" | tee -a "$log"; }
bad() { fails=$((fails + 1)); echo "FAILED  $*" | tee -a "$log"; }
unk() { unknowns=$((unknowns + 1)); echo "unknown $*" | tee -a "$log"; }
check() { local what=$1; shift; if "$@"; then ok "$what"; else bad "$what"; fi; }
eq() { [ -n "$1" ] && [ "$1" = "$2" ]; }
on_public_main() { local s; s=$(api "repos/rainbowpuffpuff/ferrata/compare/$1...main" | jq -r .status 2> /dev/null); [ "$s" = identical ] || [ "$s" = ahead ]; }

echo "after-release.sh: $org/$repo, $(date -u +%Y-%m-%dT%H:%M:%SZ)" | tee -a "$log"

# ── 1. The catalog's identity ────────────────────────────────────────────────────────────────
repo_url="https://raw.githubusercontent.com/$org/$repo/refs/heads/main/logos-repo.json"
index_url_want="https://raw.githubusercontent.com/$org/$repo/refs/heads/main/index/index.json"
get "$repo_url" > "$out/logos-repo.json" || { bad "logos-repo.json at $repo_url"; exit 1; }
if [ -f "$tool/index.py" ]; then
  check "index.py validate-repo logos-repo.json (logos-modules-release-tool $(git -C "$tool" rev-parse --short HEAD 2> /dev/null))" python3 -I "$tool/index.py" validate-repo "$out/logos-repo.json"
else unk "validate-repo: no index.py"; fi
did=$(jq -r '((.trustedSigners // [])[0] // {}) | if type == "object" then (.did // empty) else . end' "$out/logos-repo.json")
index_url=$(jq -r '.indexUrl // empty' "$out/logos-repo.json")
check "indexUrl is $index_url_want" eq "$index_url" "$index_url_want"
check "one signer DID declared (did:jwk:)" eq "$(jq -r '(.trustedSigners // []) | length' "$out/logos-repo.json"):${did%%:jwk:*}" "1:did"
if [ -n "${EXPECTED_DID:-}" ]; then check "signer DID equals EXPECTED_DID (the 24 Nov manifest's)" eq "$did" "$EXPECTED_DID"; fi

# ── 2. The catalog's commit and its submodules ───────────────────────────────────────────────
cat_git="https://github.com/$org/$repo.git"
cat_sha=$(GIT_TERMINAL_PROMPT=0 git ls-remote "$cat_git" refs/heads/main | awk '{print $1}')
check "catalog main readable ($cat_sha)" test -n "$cat_sha"
rm -rf "$out/catalog"
GIT_TERMINAL_PROMPT=0 git -c core.hooksPath=/dev/null clone -q "$cat_git" "$out/catalog" 2> /dev/null || bad "git clone $cat_git"
check "clone's HEAD is main" eq "$(git -C "$out/catalog" rev-parse HEAD)" "$cat_sha"
check "logos-repo.json in the clone equals the raw URL's" cmp -s "$out/logos-repo.json" <(git -C "$out/catalog" show HEAD:logos-repo.json)

modules=()
while read -r _ path; do [ -n "$path" ] && modules+=("$(basename "$path")"); done \
  < <(git -C "$out/catalog" config -f .gitmodules --get-regexp 'submodule\..*\.path$')
check "catalog has ${#modules[@]} submodules (want $n_packages)" test ${#modules[@]} = "$n_packages"
declare -A ver ptr murl mname srcc
for m in "${modules[@]}"; do
  p=submodules/$m
  ptr[$m]=$(git -C "$out/catalog" ls-tree HEAD "$p" | awk '{print $3}')
  murl[$m]=$(git -C "$out/catalog" config -f .gitmodules "submodule.$p.url")
  check "$m: .gitmodules URL is https://github.com/$org/$m.git" eq "${murl[$m]}" "https://github.com/$org/$m.git"
  main=$(GIT_TERMINAL_PROMPT=0 git ls-remote "${murl[$m]}" refs/heads/main | awk '{print $1}')
  check "$m: catalog pointer ${ptr[$m]:0:12} = module repo main" eq "$main" "${ptr[$m]}"
  raw="https://raw.githubusercontent.com/$org/$m/${ptr[$m]}"
  get "$raw/metadata.json" > "$out/metadata-$m.json" 2> /dev/null || : > "$out/metadata-$m.json"
  ver[$m]=$(jq -r '.version // empty' "$out/metadata-$m.json")
  mname[$m]=$(jq -r '.name // empty' "$out/metadata-$m.json"); mname[$m]=${mname[$m]:-$m}
  check "$m: metadata.json name is the directory's (${mname[$m]})" eq "${mname[$m]}" "$m"
  check "$m: metadata.json version (${ver[$m]:-none})" test -n "${ver[$m]}"
  tag_commit=$(GIT_TERMINAL_PROMPT=0 git ls-remote "${murl[$m]}" "refs/tags/v${ver[$m]}^{}" | awk '{print $1}')
  check "$m: tag v${ver[$m]} points at the pointer" eq "$tag_commit" "${ptr[$m]}"
  case "$update_modules" in *" $m "*) d=$dry_update; want_src=$src_update; pub="the update's" ;; *) d=$dry; want_src=$src_base; pub="the first publication's" ;; esac
  if [ -d "$d/$m.git" ]; then
    check "$m: same commit as $pub offline dry run" eq "$(git -C "$d/$m.git" rev-parse refs/heads/main 2> /dev/null)" "${ptr[$m]}"
  else unk "$m: no dry-run repository $d/$m.git"; fi
  sc=
  for sd in staged attestor monitor vendor; do
    sc=$(get "$raw/rust-lib/$sd/STAGED.json" 2> /dev/null | jq -r '.commit // empty' 2> /dev/null) && [ -n "$sc" ] && break
  done
  msg_src=$(api "repos/$org/$m/commits/${ptr[$m]}" | jq -r '.commit.message' | grep -o -E 'exported from the Ferrata repository at [0-9a-f]{40}' | awk '{print $NF}')
  if [ -n "$sc" ] && [ -n "$msg_src" ]; then check "$m: STAGED.json commit = the module commit message's ($sc)" eq "$sc" "$msg_src"; fi
  srcc[$m]=${sc:-$msg_src}
  where="STAGED.json"; [ -n "$sc" ] || where="the module commit message"
  if [ -n "${srcc[$m]}" ]; then ok "$m: export records source commit ${srcc[$m]} ($where)"; else bad "$m: no source commit recorded"; fi
  check "$m: source commit is $pub ($want_src)" eq "${srcc[$m]}" "$want_src"
done
check "source commit $src_update on public main of rainbowpuffpuff/ferrata" on_public_main "$src_update"
check "source commit $src_base on public main of rainbowpuffpuff/ferrata" on_public_main "$src_base"

# ── 3. The releases ──────────────────────────────────────────────────────────────────────────
mkdir -p "$out/releases"
declare -A rel_url root sha lgxname variants missing signer immut rtype rtag
release_checks() {   # release_checks KEY TAG: KEY indexes the arrays (a module, or a superseded tag)
  local k=$1 tag=$2 d="$out/releases/$2"
  mkdir -p "$d"
  if ! api "repos/$org/$repo/releases/tags/$tag" > "$d/release.json" || [ ! -s "$d/release.json" ]; then bad "$tag: release not found"; return; fi
  rel_url[$k]=$(jq -r .html_url "$d/release.json")
  check "$tag: published (not a draft, not a pre-release)" eq "$(jq -r '"\(.draft) \(.prerelease)"' "$d/release.json")" "false false"
  immut[$k]=$(jq -r 'if has("immutable") then (.immutable | tostring) else "unknown" end' "$d/release.json")
  case "${immut[$k]}" in true) ok "$tag: Immutable" ;; false) bad "$tag: not immutable" ;; *) unk "$tag: no immutable flag in the API" ;; esac
  check "$tag: two assets, the .lgx and sidecar.json" eq "$(jq -r '[.assets[].name] | sort | join(" ")' "$d/release.json")" "$(jq -r '[.assets[].name] | map(select(endswith(".lgx") or . == "sidecar.json")) | sort | join(" ")' "$d/release.json")"
  if get "https://github.com/$org/$repo/releases/download/$tag/sidecar.json" > "$d/sidecar.json"; then ok "$tag: sidecar.json"; else bad "$tag: sidecar.json"; return; fi
  root[$k]=$(jq -r '.rootHash // empty' "$d/sidecar.json")
  sha[$k]=$(jq -r '.sha256 // empty' "$d/sidecar.json")
  variants[$k]=$(jq -r '(.builtVariants // []) | join(",")' "$d/sidecar.json")
  missing[$k]=$(jq -r '(.missingVariants // []) | join(",")' "$d/sidecar.json")
  signer[$k]=$(jq -r '.signature.did // empty' "$d/sidecar.json")
  rtype[$k]=$(jq -r '.manifest.type // empty' "$d/sidecar.json")
  rtag[$k]=$(git -C "$out/catalog" rev-list -n 1 "refs/tags/$tag" 2> /dev/null || GIT_TERMINAL_PROMPT=0 git ls-remote "$cat_git" "refs/tags/$tag" | awk '{print $1}')
  check "$tag: sidecar rootHash = manifest hashes.root" eq "${root[$k]}" "$(jq -r '.manifest.hashes.root // empty' "$d/sidecar.json")"
  check "$tag: sidecar publisherRef = the tag" eq "$(jq -r '.publisherRef // empty' "$d/sidecar.json")" "$tag"
  check "$tag: signed by the catalog's DID" eq "${signer[$k]}" "$did"
  lgxname[$k]=$(jq -r '[.assets[] | select(.name | endswith(".lgx")) | .name][0] // empty' "$d/release.json")
  if [ -n "${lgxname[$k]}" ] && get -o "$d/${lgxname[$k]}" "https://github.com/$org/$repo/releases/download/$tag/${lgxname[$k]}"; then
    check "$tag: sha256 of ${lgxname[$k]} = sidecar" eq "$(sha256sum "$d/${lgxname[$k]}" | cut -d' ' -f1)" "${sha[$k]}"
    check "$tag: size of ${lgxname[$k]} = sidecar" eq "$(stat -c %s "$d/${lgxname[$k]}")" "$(jq -r '.size // empty' "$d/sidecar.json")"
  else bad "$tag: .lgx asset"; fi
}
for m in "${modules[@]}"; do
  release_checks "$m" "${mname[$m]}-v${ver[$m]}"
  tag="${mname[$m]}-v${ver[$m]}"
  case "$m" in
    ferrata_updater_module) check "$tag: builtVariants include linux-amd64 (${variants[$m]})" grep -q linux-amd64 <<< "${variants[$m]}" ;;
    *) [ "${variants[$m]}" = "darwin-arm64,linux-amd64,linux-arm64" ] && ok "$tag: all three variants" \
         || unk "$tag: builtVariants ${variants[$m]} (missing ${missing[$m]:-none}: a partial release)" ;;
  esac
  case "$update_modules" in *" $m "*)
    check "$tag: release tag's commit ${rtag[$m]:0:12} has main's submodules and workflows" \
      git -C "$out/catalog" diff --quiet "${rtag[$m]}" HEAD -- submodules .gitmodules .github logos-repo.json ;;
  esac
done
check "ferrata_bridge_ui: manifest type ui_qml" eq "${rtype[ferrata_bridge_ui]:-}" "ui_qml"
check "ferrata_bridge_module: manifest type core" eq "${rtype[ferrata_bridge_module]:-}" "core"
i=0; read -r -a olds <<< "$old_tags"; read -r -a oldr <<< "$old_roots"
for t in "${olds[@]}"; do
  release_checks "old:$t" "$t"
  check "$t (superseded, still offered): rootHash as the 24 Nov manifest (${oldr[$i]:0:12})" eq "${root[old:$t]:-}" "${oldr[$i]}"
  i=$((i + 1))
done

# ── 4. The index (index/index.json on main) ──────────────────────────────────────────────────
if get "$index_url_want" > "$out/index.json"; then
  ok "index.json at indexUrl"
  check "index: $n_packages packages" eq "$(jq '.packages | length' "$out/index.json")" "$n_packages"
  check "index: $n_versions versions" eq "$(jq '[.packages[].versions[]] | length' "$out/index.json")" "$n_versions"
  check "index.json at indexUrl = the clone's index/index.json" cmp -s "$out/index.json" <(git -C "$out/catalog" show HEAD:index/index.json)
  idx_entry() { jq -c --arg t "$1" '.packages[] | .versions[] | select(.publisherRef == $t)' "$out/index.json" | head -n 1; }
  for k in "${modules[@]}" "${olds[@]/#/old:}"; do
    case "$k" in old:*) tag=${k#old:} ;; *) tag="${mname[$k]}-v${ver[$k]}" ;; esac
    e=$(idx_entry "$tag"); check "index lists $tag" test -n "$e"; [ -n "$e" ] || continue
    check "index $tag rootHash = sidecar" eq "$(jq -r .rootHash <<< "$e")" "${root[$k]:-}"
    check "index $tag sha256 = sidecar" eq "$(jq -r .sha256 <<< "$e")" "${sha[$k]:-}"
  done
  check "index repositoryName is the catalog's name" eq "$(jq -r '.repositoryName // empty' "$out/index.json")" "$(jq -r '.name // empty' "$out/logos-repo.json")"
  ic=$(jq -r '[.packages[] | select(.name == "ferrata_bridge_ui") | (.icon.path // .versions[0].icon.path // empty)][0] // empty' "$out/index.json")
  if [ -n "$ic" ]; then check "ferrata_bridge_ui's icon in the catalog ($ic)" git -C "$out/catalog" cat-file -e "HEAD:index/$ic"; else unk "ferrata_bridge_ui: no icon path in the index"; fi
else bad "index.json at $index_url_want"; fi

# ── 5. The settings (read-only) ──────────────────────────────────────────────────────────────
for n in "$repo" "${modules[@]}"; do
  r=$(api "repos/$org/$n/immutable-releases" | jq -r 'if type == "object" and has("enabled") then (.enabled | tostring) else empty end' 2> /dev/null)
  case "$r" in true) ok "$org/$n: immutable releases enabled" ;; false) bad "$org/$n: immutable releases off" ;; *) unk "$org/$n: immutable-releases not readable" ;; esac
done
rs=$(api "repos/$org/$repo/rulesets?includes_parents=true" || true)
tag_rs=$(jq -r '[.[]? | select(.target == "tag" and .enforcement == "active") | .id] | join(" ")' <<< "${rs:-[]}" 2> /dev/null)
if [ -n "$tag_rs" ]; then
  for id in $tag_rs; do api "repos/$org/$repo/rulesets/$id" > "$out/ruleset-$id.json" || true; done
  if jq -s -e 'map(select((.conditions.ref_name.include // []) | index("~ALL") or index("refs/tags/*"))) | map([.rules[].type]) | any(index("update") and index("deletion"))' "$out"/ruleset-*.json > /dev/null 2>&1; then
    ok "$org/$repo: an active tag ruleset on all tags restricting updates and deletions"
  else bad "$org/$repo: no active tag ruleset on all tags with update + deletion restricted"; fi
else unk "$org/$repo: rulesets not readable"; fi
tfa=$(api "orgs/$org" | jq -r 'if has("two_factor_requirement_enabled") and .two_factor_requirement_enabled != null then (.two_factor_requirement_enabled | tostring) else empty end' 2> /dev/null)
case "$tfa" in true) ok "$org: 2FA required" ;; false) bad "$org: 2FA not required" ;; *) unk "$org: 2FA requirement not readable" ;; esac

# ── 5b. The release runs (logos-modules-release-action on GitHub's runners) ───────────────────
mkdir -p "$out/runs"
for id in $runs; do
  api "repos/$org/$repo/actions/runs/$id" > "$out/runs/$id.json"
  api "repos/$org/$repo/actions/runs/$id/jobs?per_page=50" > "$out/runs/$id-jobs.json"
  wf=$(jq -r .path "$out/runs/$id.json"); hs=$(jq -r .head_sha "$out/runs/$id.json")
  m=$(basename "$wf" .yml); m=${m#release-}
  check "run $id ($wf): completed, success" eq "$(jq -r '"\(.status) \(.conclusion)"' "$out/runs/$id.json")" "completed success"
  check "run $id: workflow_dispatch on main" eq "$(jq -r '"\(.event) \(.head_branch)"' "$out/runs/$id.json")" "workflow_dispatch main"
  check "run $id: the workflow of a module in the catalog ($m)" grep -q -x -F "$m" <(printf '%s\n' "${modules[@]}")
  check "run $id: its commit ${hs:0:12} has main's submodules and workflows" git -C "$out/catalog" diff --quiet "$hs" HEAD -- submodules .gitmodules .github
  check "run $id: $wf calls _release-module.yml with module_path submodules/$m" \
    sh -c 'git -C "$1" show "$2:$3" | grep -q -F "uses: ./.github/workflows/_release-module.yml" && git -C "$1" show "$2:$3" | grep -q -F "module_path: submodules/$4"' sh "$out/catalog" "$hs" "$wf" "$m"
  check "run $id: _release-module.yml at its commit pins logos-modules-release-action@$action_pin" \
    sh -c 'git -C "$1" show "$2:.github/workflows/_release-module.yml" | grep -q -E "uses: logos-co/logos-modules-release-action/\.github/workflows/release\.yml@$3"' sh "$out/catalog" "$hs" "$action_pin"
  for v in darwin-arm64 linux-amd64 linux-arm64; do
    check "run $id: build ($v) success" eq "$(jq -r --arg n "build ($v)" '[.jobs[] | select(.name | endswith($n)) | .conclusion][0] // empty' "$out/runs/$id-jobs.json")" "success"
  done
  check "run $id: release job success" eq "$(jq -r '[.jobs[] | select(.name | endswith("/ release")) | .conclusion][0] // empty' "$out/runs/$id-jobs.json")" "success"
  check "run $id: the sidecar's builtVariants are the legs that built" eq "${variants[$m]:-}" "darwin-arm64,linux-amd64,linux-arm64"
done

# ── 6. The values, for the manifest and the rows ─────────────────────────────────────────────
{
  jq -n --arg did "$did" --arg main "$cat_sha" --arg index_url "$index_url" '{did: $did, main: $main, indexUrl: $index_url, modules: {}}'
  for k in "${modules[@]}" "${olds[@]/#/old:}"; do
    case "$k" in old:*) tag=${k#old:}; mm=${tag%-v*}; vv=${tag##*-v} ;; *) tag="${mname[$k]}-v${ver[$k]}"; mm=$k; vv=${ver[$k]} ;; esac
    jq -n --arg k "$k" --arg tag "$tag" --arg m "$mm" --arg v "$vv" --arg url "${rel_url[$k]:-}" --arg ptr "${ptr[$mm]:-}" \
      --arg src "${srcc[$mm]:-}" --arg root "${root[$k]:-}" --arg sha "${sha[$k]:-}" --arg built "${variants[$k]:-}" \
      --arg miss "${missing[$k]:-}" --arg imm "${immut[$k]:-}" --arg type "${rtype[$k]:-}" --arg lgx "${lgxname[$k]:-}" --arg rtag "${rtag[$k]:-}" \
      '{($k): {tag: $tag, module: $m, version: $v, url: $url, pointer: $ptr, source: $src, rootHash: $root, sha256: $sha,
              built: $built, missing: $miss, immutable: $imm, type: $type, lgx: $lgx, tagCommit: $rtag}}'
  done
} | jq -s '.[0] + {modules: (.[1:] | add)}' > "$out/values.json"

echo
echo "passed $passes, failed $fails, not readable here $unknowns" | tee -a "$log"
echo "written: $out/{checks.txt,values.json}"
exit $(( fails > 0 ? 1 : 0 ))
