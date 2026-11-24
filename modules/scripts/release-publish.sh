#!/usr/bin/env bash
# Put everything in place so that publishing the module catalog is one owner step: check the inputs,
# generate the catalog's logos-repo.json (trustedSigners from the key the owner provides, indexUrl
# from ORG/CATALOG_REPO), export every module as its own repository (export.sh), assemble the catalog
# repository (the catalog files at its root, each module a git submodule), and then, with --yes,
# create the repositories, push them and their tags, and set the signing secret. It stops there:
# the browser settings (immutable releases, the tag ruleset, organization 2FA) must be on before the
# first release, so the release trigger is printed as the owner's last command, not run.
# The lambda-prize release-integrity mapping is modules/catalog/RELEASE.md.
#
#   modules/scripts/release-publish.sh ORG CATALOG_REPO SIGNING_KEY_FILE [--modules 'M ...'] [--dry-run DIR | --yes]
#
#   (no flag)      prepare everything locally and print the plan; nothing leaves this machine
#   --dry-run DIR  the same, then push everything to local bare repositories under DIR (file://
#                  remotes, the catalog's submodules pointing at them); no network command runs; a
#                  DIR/<name>.git that an earlier dry run made is replaced, any other is refused
#   --yes          check first, read-only, that the remote side can take every step (gh logged in,
#                  ORG an organization, every existing repository readable by git and holding no
#                  different main or version tag), and only then create the repositories, push, tag
#                  and set the secret; every step is idempotent, so a rerun after a failure resumes
#   --modules 'M ...'  only these modules (default: all of them); with --yes the only way to name a subset
#
# ORG/CATALOG_REPO: the GitHub organization and the catalog repository to publish under
#   (e.g. ferrata-co ferrata-modules). SIGNING_KEY_FILE: the Ed25519 private key whose public half
#   becomes the catalog's declared signer, in the format logos-modules-release-action's inline signing
#   reads (a JWK - {"crv":"Ed25519","d":<seed>,"kty":"OKP","x":<pubkey>}, what `lgx keygen` writes and
#   what the action's release.yml stores as the LOGOS_SIGNING_KEY secret). The key is checked (x must
#   be the public key of d), read at run time to derive the DID and, with --yes, fed to
#   `gh secret set` on its standard input; it is never copied, never printed, never written under any
#   repository. Everything else the script writes goes to a workspace made with mktemp (under TMPDIR
#   when it is set). MODULES (environment) also names a subset, without --yes only: with --yes a MODULES
#   set in the environment is refused, so an inherited one cannot narrow a publication unseen.
# What is published is the source commit (HEAD of this repository), dated with that commit's date: an
#   inherited SOURCE_DATE_EPOCH or FERRATA_COMMIT is ignored, so the module commits are the ones anyone
#   exports again from that commit (releases/RELEASE-INTEGRITY.template.md); --yes needs a git checkout.
# Needs bash (3.2 or later), git, jq, python3; gh only with --yes (RELEASE_PUBLISH_GH names another gh
# binary: the tests' stub, modules/scripts/test-release-publish.sh).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
MODULES_DIR=$repo/modules
CATALOG_DIR=$repo/modules/catalog
gh=${RELEASE_PUBLISH_GH:-gh}

usage() {
  sed -n '/^#   modules\/scripts\/release-publish\.sh ORG/,/^#   --modules/p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}
bad() { echo "release-publish.sh: $*" >&2; exit 2; }     # bad input: nothing was done
fail() { echo "release-publish.sh: $*" >&2; exit 1; }    # a step failed

[ $# -ge 3 ] || usage
org=$1; catalog_repo=$2; key_file=$3; shift 3
dry_dir=; yes=; modules_arg=; modules_given=
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) [ $# -ge 2 ] && [ -n "$2" ] || usage; dry_dir=$2; shift 2 ;;
    --modules) [ $# -ge 2 ] && [ -n "$2" ] || usage; modules_arg=$2; modules_given=1; shift 2 ;;
    --yes) yes=1; shift ;;
    -h|--help) usage ;;
    *) echo "release-publish.sh: unknown argument '$1'" >&2; usage ;;
  esac
done
case "$org" in ''|-*|*[!A-Za-z0-9-]*) bad "bad ORG '$org' (a GitHub organization name: letters, digits, '-')" ;; esac
case "$catalog_repo" in ''|-*|.*|*[!A-Za-z0-9_.-]*) bad "bad CATALOG_REPO '$catalog_repo'" ;; esac
[ -f "$key_file" ] && [ -r "$key_file" ] || bad "no readable signing-key file '$key_file'"
if [ -n "$dry_dir" ] && [ -n "$yes" ]; then bad "--dry-run and --yes are mutually exclusive"; fi
for tool in git jq python3; do command -v "$tool" > /dev/null || bad "$tool is required"; done
key_abs=$(cd "$(dirname "$key_file")" && pwd)/$(basename "$key_file")

# The key: read once, check it is the JWK the action's inline signing reads and that its public half x
# belongs to its seed d (otherwise the declared signer would not be the key that signs), then derive the
# signer's did:jwk from x (lgx's construction: did:jwk:base64url({"crv","kty","x"}), keys alphabetical,
# no padding). Only the public DID leaves this check; no message ever shows d.
did=$(python3 -I "$here/jwk_did.py" "$key_abs") || exit 2
case "$did" in did:jwk:*) ;; *) bad "deriving the DID failed" ;; esac

# The modules: every modules/<name>/metadata.json (--modules, or MODULES without --yes, names a subset,
# e.g. when one module's export needs an artifact this machine does not have: ferrata_updater_module's
# stage wants lez/methods' built guests - `cd lez && cargo build --locked -p methods`, RISC0_HOME as
# lez/RESULTS.md says; the published catalog takes all six).
all_modules=()
for d in "$MODULES_DIR"/*_module; do
  if [ -f "$d/metadata.json" ]; then all_modules+=("$(basename "$d")"); fi
done
modules=()
if [ -n "$modules_given" ]; then
  for m in $modules_arg; do modules+=("$m"); done
elif [ -n "${MODULES:-}" ]; then
  if [ -n "$yes" ]; then
    bad "MODULES is set in the environment ('$MODULES'); with --yes a subset is named on the command line (--modules '...'), or unset MODULES to publish all ${#all_modules[@]}; nothing was done"
  fi
  for m in $MODULES; do modules+=("$m"); done
else
  if [ ${#all_modules[@]} -gt 0 ]; then modules=("${all_modules[@]}"); fi
fi
[ ${#modules[@]} -gt 0 ] || bad "no modules found under $MODULES_DIR"
for m in "${modules[@]}"; do
  case "$m" in ''|-*|.*|*[!A-Za-z0-9_.-]*) bad "bad module name '$m'" ;; esac
  [ -f "$MODULES_DIR/$m/metadata.json" ] || bad "no module $m under $MODULES_DIR"
  [ "$(jq -r .name "$MODULES_DIR/$m/metadata.json")" = "$m" ] || bad "modules/$m/metadata.json names another module"
  case "$(jq -r .version "$MODULES_DIR/$m/metadata.json")" in ''|null|*[!A-Za-z0-9.+-]*) bad "modules/$m/metadata.json: bad version" ;; esac
  [ "$m" != "$catalog_repo" ] || bad "CATALOG_REPO must differ from every module name"
done
version_of() { jq -r .version "$MODULES_DIR/$1/metadata.json"; }
names=("${modules[@]}" "$catalog_repo")

# A dry run replaces only bare repositories an earlier dry run made (they carry release-publish.dryRun in
# their configuration); any other DIR/<name>.git is refused before anything is done.
made_by_dry_run() { [ "$(git config --file "$1/config" --get release-publish.dryRun 2> /dev/null)" = true ]; }
if [ -n "$dry_dir" ]; then
  mkdir -p "$dry_dir"
  dry_dir=$(cd "$dry_dir" && pwd)
  for n in "${names[@]}"; do
    if { [ -e "$dry_dir/$n.git" ] || [ -L "$dry_dir/$n.git" ]; } && ! made_by_dry_run "$dry_dir/$n.git"; then
      bad "$dry_dir/$n.git exists and was not made by a dry run of this script; nothing was done (choose another DIR, or remove it yourself)"
    fi
  done
fi

# The source: this repository's HEAD and its date (see the header: inherited SOURCE_DATE_EPOCH and
# FERRATA_COMMIT ignored).
src_commit=$(git -C "$repo" rev-parse --verify -q HEAD 2> /dev/null || true)
if [ -n "$yes" ] && [ -z "$src_commit" ]; then
  bad "$repo is not a git checkout: --yes publishes a source commit, which the module exports record; nothing was done"
fi
src_epoch=$(git -C "$repo" log -1 --format=%ct HEAD 2> /dev/null || echo 0)
ignored=
if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then ignored="$ignored SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH"; fi
if [ -n "${FERRATA_COMMIT:-}" ]; then ignored="$ignored FERRATA_COMMIT=$FERRATA_COMMIT"; fi
unset SOURCE_DATE_EPOCH FERRATA_COMMIT

# The workspace: everything the script writes (exported repositories, the assembled catalog) goes here,
# outside every repository. New on each run; the private key is never written into it.
ws=$(mktemp -d "${TMPDIR:-/tmp}/release-publish.XXXXXX")
mkdir -p "$ws/src"

# The commits this script makes (the catalog's, the module tags) carry the source commit's date, as
# export.sh's do: the same source gives the same commits, so a rerun pushes nothing new.
export GIT_AUTHOR_DATE="$src_epoch +0000" GIT_COMMITTER_DATE="$src_epoch +0000"
export GIT_AUTHOR_NAME=ferrata-release-publish GIT_AUTHOR_EMAIL=publish@localhost
export GIT_COMMITTER_NAME=ferrata-release-publish GIT_COMMITTER_EMAIL=publish@localhost

echo "release-publish.sh: publishing $org/$catalog_repo"
echo "  modules (${#modules[@]} of ${#all_modules[@]}): ${modules[*]}"
echo "  source commit: ${src_commit:-unknown (no git checkout)}, date $src_epoch${ignored:+ (from the environment, ignored:$ignored)}"
echo "  signer DID: $did"
echo "  workspace: $ws"
if [ -n "$dry_dir" ]; then echo "  DRY RUN: every repository a local bare repository under $dry_dir; no network command runs"; fi
if [ -n "$yes" ]; then echo "  --yes: checks the remote side first, then creates, pushes, tags and sets the secret"; fi

# ── 1. Export each module as its own repository ────────────────────────────────────────────────
for m in "${modules[@]}"; do
  echo "  export $m"
  SOURCE_DATE_EPOCH=$src_epoch FERRATA_COMMIT=${src_commit:-unknown} "$here/export.sh" "$m" "$ws/src/$m" > /dev/null \
    || fail "export.sh $m failed"
  git -C "$ws/src/$m" branch -M main
  git -C "$ws/src/$m" tag -a "v$(version_of "$m")" -m "$m $(version_of "$m"), exported from the Ferrata repository"
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
\`indexUrl\`, the signer the releases are signed by in \`trustedSigners\`), the modules as git
submodules under \`submodules/\`, and the release workflows that build, sign, verify and publish each one with
[\`logos-co/logos-modules-release-action\`](https://github.com/logos-co/logos-modules-release-action)
(pinned to a commit). How it is published, and each lambda-prize release-integrity rule it meets:
\`modules/catalog/RELEASE.md\` in the source repository (https://github.com/rainbowpuffpuff/ferrata).

The modules are prototype, unaudited, not for production.

## Trust the signing key first

Logos clients do not trust a catalog's \`trustedSigners\` by themselves: a package's signature counts
only for a key in the installer's own keyring, and by default (\`signature_policy: warn\`) an unsigned
package, or one signed by a key the keyring lacks, installs with a warning. So, before installing:

1. Check the signer DID below against the one in the release-integrity manifest of the source
   repository (\`releases/\` of https://github.com/rainbowpuffpuff/ferrata), not only against this page:

       $did

2. Add it to the keyring and make signatures required (Logos Core's \`logosctl\`, per session):

       logosctl key add ferrata --did $did --display-name "Ferrata module releases"
       # in the daemon configuration (node.yaml), then \`logosctl daemon config set ./node.yaml\`
       # and restart the daemon (the policy takes effect at the next start):
       signature_policy: require

   With \`lgpm\`: \`lgx keyring add ferrata $did\`, then install with \`--require-signatures\`.
   Basecamp's package manager keeps its own keyring: add the key there too.

## Install

    logosctl catalog add https://raw.githubusercontent.com/$org/$catalog_repo/refs/heads/main/logos-repo.json
    logosctl catalog refresh
    logosctl search ferrata
    logosctl install ferrata_attest_module -y

The same URL works in the package-manager UI and with \`lgpd\`.
EOF

# ── 3. Assemble the catalog repository: each module a submodule pointing at its repository ─────
url_of() {   # the URL a repository is published at (in a dry run, its local bare repository)
  if [ -n "$dry_dir" ]; then echo "file://$dry_dir/$1.git"; else echo "https://github.com/$org/$1.git"; fi
}
git init -q "$ws/catalog"
git -C "$ws/catalog" symbolic-ref HEAD refs/heads/main
for m in "${modules[@]}"; do
  # Cloned from the exported repository beside it (a plain local path, so git's file-transport guard
  # needs protocol.file.allow=always on this one command: every path involved is one this script
  # created in its workspace), then the URL the published catalog points at recorded in .gitmodules.
  git -C "$ws/catalog" -c protocol.file.allow=always submodule add -q "$ws/src/$m" "submodules/$m"
  git -C "$ws/catalog" config -f .gitmodules "submodule.submodules/$m.url" "$(url_of "$m")"
  git -C "$ws/catalog/submodules/$m" remote set-url origin "$(url_of "$m")"
done
git -C "$ws/catalog" add -A
git -C "$ws/catalog" commit -q \
  -m "Ferrata module catalog: ${#modules[@]} modules, signing on, action pinned (assembled by modules/scripts/release-publish.sh)"
for m in "${modules[@]}"; do
  [ "$(git -C "$ws/catalog" rev-parse "HEAD:submodules/$m")" = "$(git -C "$ws/src/$m" rev-parse HEAD)" ] \
    || fail "the catalog's pointer for $m is not the exported commit"
done

# ── 4. The remote side ─────────────────────────────────────────────────────────────────────────
show() { printf '  $'; printf ' %q' "$@"; printf '\n'; }
gh_step() {   # a gh command of the owner step: run with --yes, printed otherwise
  show gh "$@"
  if [ -n "$yes" ]; then "$gh" "$@"; fi
}
wsdir_of() { if [ "$1" = "$catalog_repo" ]; then echo "$ws/catalog"; else echo "$ws/src/$1"; fi; }

if [ -n "$yes" ]; then
  # Read-only checks, before anything is created: a failure here leaves the remote side untouched.
  echo; echo "Checks before anything is created:"
  command -v "$gh" > /dev/null || fail "gh (GitHub CLI) is required with --yes; nothing was created"
  "$gh" auth status > /dev/null 2>&1 || fail "gh is not logged in (gh auth login); nothing was created"
  [ "$("$gh" api "orgs/$org" --jq .login 2> /dev/null | tr 'A-Z' 'a-z')" = "$(echo "$org" | tr 'A-Z' 'a-z')" ] \
    || fail "$org is not a GitHub organization this login can see (the catalog belongs under an organization, RELEASE.md rule 7); nothing was created"
  echo "  gh logged in; $org is an organization; git must push to github.com as the same login (gh auth setup-git)"
  for n in "${names[@]}"; do
    d=$(wsdir_of "$n")
    if "$gh" repo view "$org/$n" --json name > /dev/null 2>&1; then
      # What it holds, read by git (as the pushes will be): a repository git cannot read stops it here.
      if ! GIT_TERMINAL_PROMPT=0 git ls-remote "$(url_of "$n")" > "$ws/refs-$n" 2> "$ws/refs-$n.err"; then
        fail "$org/$n exists but git cannot read it ($(tail -n 1 "$ws/refs-$n.err")); nothing was created (git must reach github.com as the gh login: gh auth setup-git)"
      fi
      have=$(awk '$2 == "refs/heads/main" {print $1}' "$ws/refs-$n")
      want=$(git -C "$d" rev-parse HEAD)
      if [ -n "$have" ] && [ "$have" != "$want" ]; then
        fail "$org/$n exists and its main is $have, not this export's $want; nothing was created (publish into new repositories, or remove the old ones yourself)"
      fi
      if [ "$n" != "$catalog_repo" ]; then
        t="v$(version_of "$n")"
        have=$(awk -v ref="refs/tags/$t" '$2 == ref {print $1}' "$ws/refs-$n")
        want=$(git -C "$d" rev-parse "refs/tags/$t")
        if [ -n "$have" ] && [ "$have" != "$want" ]; then
          fail "$org/$n already has a tag $t at $have, not this export's $want; nothing was created (a published version is never moved: bump the module's version)"
        fi
      fi
      echo "  $org/$n exists, holding nothing that differs from this export (resumed)"
    else
      echo "  $org/$n will be created"
    fi
  done
fi

if [ -n "$yes" ]; then
  echo; echo "Creating, pushing and tagging (each step idempotent: a rerun resumes):"
elif [ -n "$dry_dir" ]; then
  echo; echo "Into local bare repositories under $dry_dir (what --yes does on GitHub; the gh commands are printed, not run):"
else
  echo; echo "The commands --yes runs (nothing runs now):"
fi
for n in "${names[@]}"; do
  d=$(wsdir_of "$n")
  url=$(url_of "$n")
  if [ "$n" = "$catalog_repo" ]; then desc="Ferrata's Logos module catalog (prototype, unaudited)"
  else desc="Ferrata $n - Logos Core module (prototype, unaudited)"; fi
  if [ -n "$dry_dir" ]; then
    if [ -e "$dry_dir/$n.git" ] || [ -L "$dry_dir/$n.git" ]; then   # an earlier dry run's (checked above, and here): replaced
      made_by_dry_run "$dry_dir/$n.git" || fail "$dry_dir/$n.git is not a dry run's repository; left as it is"
      rm -rf "$dry_dir/$n.git"
    fi
    git init -q --bare "$dry_dir/$n.git"
    git -C "$dry_dir/$n.git" symbolic-ref HEAD refs/heads/main
    git -C "$dry_dir/$n.git" config release-publish.dryRun true
  fi
  if [ -n "$yes" ] && "$gh" repo view "$org/$n" --json name > /dev/null 2>&1; then
    echo "  $org/$n exists"
  else
    gh_step repo create "$org/$n" --public --description "$desc" || fail "creating $org/$n failed; rerun to resume"
  fi
  show git -C "$d" push "$url" refs/heads/main:refs/heads/main
  if [ -n "$yes" ] || [ -n "$dry_dir" ]; then
    git -C "$d" push -q "$url" refs/heads/main:refs/heads/main || fail "pushing $n's main failed; rerun to resume"
  fi
  if [ "$n" != "$catalog_repo" ]; then
    t="v$(version_of "$n")"
    show git -C "$d" push "$url" "refs/tags/$t"
    if [ -n "$yes" ] || [ -n "$dry_dir" ]; then
      git -C "$d" push -q "$url" "refs/tags/$t" || fail "pushing $n's tag $t failed; rerun to resume"
    fi
  fi
done
printf '  $ gh secret set LOGOS_SIGNING_KEY --repo %q < %q   # the key file, read once from standard input\n' "$org/$catalog_repo" "$key_abs"
if [ -n "$yes" ]; then
  "$gh" secret set LOGOS_SIGNING_KEY --repo "$org/$catalog_repo" < "$key_abs" || fail "setting the LOGOS_SIGNING_KEY secret failed; rerun to resume"
fi

if [ -n "$dry_dir" ]; then
  echo; echo "Bare repositories under $dry_dir:"
  for n in "${names[@]}"; do
    d=$(wsdir_of "$n")
    if [ "$n" = "$catalog_repo" ]; then
      echo "  $n.git  main $(git -C "$d" rev-parse HEAD) (the assembled catalog)"
    else
      t="v$(version_of "$n")"
      echo "  $n.git  main $(git -C "$d" rev-parse HEAD), tag $t $(git -C "$d" rev-parse "refs/tags/$t")"
    fi
  done
fi

# ── 5. What only the owner does, in this order ────────────────────────────────────────────────
echo
if [ -n "$yes" ]; then
  echo "Done on GitHub: the repositories, their main branches and version tags, the LOGOS_SIGNING_KEY secret."
else
  echo "After those commands:"
fi
echo "The owner's remaining steps, in this order (the release must come last):"
echo "  1. The rolling index release, created empty while releases are still mutable: rebuild-index.yml"
echo "     replaces its index.json after every module release, which an immutable release refuses: it must"
echo "     exist before immutability is turned on):"
show gh release create index --repo "$org/$catalog_repo" --title "Catalog index (auto-updated)" \
  --notes "Auto-generated index of every per-module release in this repository." --latest=false
echo "  2. [in the browser] in $org/$catalog_repo and each module repository: turn on immutable releases;"
echo "     add a ruleset protecting refs/tags/* (the release tags <module>-v<version>; updates and deletions"
echo "     restricted, creations allowed: the workflows create them); require 2FA for the $org organization."
echo "     Tokens stay least-privilege: the workflows use the built-in GITHUB_TOKEN with contents alone"
echo "     (modules/catalog/RELEASE.md, rules 2 and 6)."
echo "  3. Only then, the release: builds, signs, verifies and publishes every module, then the index:"
show gh workflow run release-all.yml --repo "$org/$catalog_repo"
echo "  4. Fill releases/RELEASE-INTEGRITY.template.md (each value's command is in it) with the release"
echo "     URLs, the source commits, the signer DID and the rootHashes the sidecars give."
echo
echo "release-publish.sh: done.${dry_dir:+ Dry run: repositories under $dry_dir.}"
