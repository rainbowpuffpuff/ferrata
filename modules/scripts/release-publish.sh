#!/usr/bin/env bash
# Put everything in place so that publishing the module catalog is one owner step: check the inputs,
# generate the catalog's logos-repo.json (trustedSigners from the key the owner provides, indexUrl
# from ORG/CATALOG_REPO), export every module as its own repository (export.sh), assemble the catalog
# repository (the catalog files at its root, each module a git submodule), and then, with --yes,
# create the repositories, push them and their tags, and set the signing secret. It stops there:
# the browser settings (immutable releases, the tag ruleset, organization 2FA) must be on before the
# first release, so the release trigger is printed as the owner's last command, not run.
# With --update it changes a catalog already published instead: the named modules added or moved to
# this commit's exports, on top of the live repositories' main branches (fast-forward only).
# The lambda-prize release-integrity mapping is modules/catalog/RELEASE.md.
#
#   modules/scripts/release-publish.sh ORG CATALOG_REPO SIGNING_KEY_FILE [--modules 'M ...'] [--update] [--dry-run DIR | --yes]
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
#   --update       the catalog ORG/CATALOG_REPO is published already: add the named modules (--modules, required)
#                  to it or move them to this commit's exports. Each module's new main is its export's tree
#                  committed on top of the live main, the message naming the source commit (none when the tree
#                  is the same; a new repository when there is none); a module whose content changed while its
#                  version did not, or whose
#                  version tag or catalog release exists for other content, is refused (a published version
#                  is never changed: bump it). The catalog's new main is one commit on top of the live one:
#                  the submodules' entries and pointers, their release-<module>.yml, the README regenerated.
#                  Nothing else of the catalog changes (logos-repo.json, the shared workflows: differences
#                  from this repository's template are printed, not carried), no secret is set, no force-push
#                  is made. SIGNING_KEY_FILE may be '-': the catalog's declared signer is read from its live
#                  logos-repo.json (no private key needed); a key file's DID must be one it declares. The live
#                  repositories are read (git ls-remote and fetch: read-only) in every mode, and a dry run
#                  starts its bare repositories from them, so that its commits are the ones --yes pushes.
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
# The modules: every modules/<name> with a release workflow in the catalog's template
#   (modules/catalog/.github/workflows/release-<name>.yml): the six core modules and the mini-app
#   (ferrata_bridge_ui, a ui module the same action builds).
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
WORKFLOWS_DIR=$CATALOG_DIR/.github/workflows
gh=${RELEASE_PUBLISH_GH:-gh}

usage() {
  sed -n '/^#   modules\/scripts\/release-publish\.sh ORG/,/^#   --modules/p' "$0" | sed 's/^# \{0,1\}//' >&2
  echo "  --update       change a published catalog (see the script's header)" >&2
  exit 2
}
bad() { echo "release-publish.sh: $*" >&2; exit 2; }     # bad input: nothing was done
fail() { echo "release-publish.sh: $*" >&2; exit 1; }    # a step failed

[ $# -ge 3 ] || usage
org=$1; catalog_repo=$2; key_file=$3; shift 3
dry_dir=; yes=; modules_arg=; modules_given=; update=
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) [ $# -ge 2 ] && [ -n "$2" ] || usage; dry_dir=$2; shift 2 ;;
    --modules) [ $# -ge 2 ] && [ -n "$2" ] || usage; modules_arg=$2; modules_given=1; shift 2 ;;
    --update) update=1; shift ;;
    --yes) yes=1; shift ;;
    -h|--help) usage ;;
    *) echo "release-publish.sh: unknown argument '$1'" >&2; usage ;;
  esac
done
case "$org" in ''|-*|*[!A-Za-z0-9-]*) bad "bad ORG '$org' (a GitHub organization name: letters, digits, '-')" ;; esac
case "$catalog_repo" in ''|-*|.*|*[!A-Za-z0-9_.-]*) bad "bad CATALOG_REPO '$catalog_repo'" ;; esac
if [ "$key_file" = - ]; then
  [ -n "$update" ] || bad "SIGNING_KEY_FILE '-' (no key) is for --update only: a first publication declares the key's DID and sets the secret"
else
  [ -f "$key_file" ] && [ -r "$key_file" ] || bad "no readable signing-key file '$key_file'"
fi
if [ -n "$dry_dir" ] && [ -n "$yes" ]; then bad "--dry-run and --yes are mutually exclusive"; fi
if [ -n "$update" ] && [ -z "$modules_given" ]; then bad "--update needs --modules 'M ...': the modules to add to the catalog or move"; fi
for tool in git jq python3; do command -v "$tool" > /dev/null || bad "$tool is required"; done

# The key: read once, check it is the JWK the action's inline signing reads and that its public half x
# belongs to its seed d (otherwise the declared signer would not be the key that signs), then derive the
# signer's did:jwk from x (lgx's construction: did:jwk:base64url({"crv","kty","x"}), keys alphabetical,
# no padding). Only the public DID leaves this check; no message ever shows d. With --update and '-',
# no key: the DID is the live catalog's (read below).
did=; key_abs=
if [ "$key_file" != - ]; then
  key_abs=$(cd "$(dirname "$key_file")" && pwd)/$(basename "$key_file")
  did=$(python3 -I "$here/jwk_did.py" "$key_abs") || exit 2
  case "$did" in did:jwk:*) ;; *) bad "deriving the DID failed" ;; esac
fi

# The modules: every one with a release workflow in the catalog's template (--modules, or MODULES without
# --yes, names a subset, e.g. when one module's export needs an artifact this machine does not have:
# ferrata_updater_module's stage wants lez/methods' built guests - `cd lez && cargo build --locked -p methods`,
# RISC0_HOME as lez/RESULTS.md says; the published catalog takes them all).
all_modules=()
for f in "$WORKFLOWS_DIR"/release-*.yml; do
  n=$(basename "$f" .yml); n=${n#release-}
  if [ "$n" != all ] && [ -f "$MODULES_DIR/$n/metadata.json" ]; then all_modules+=("$n"); fi
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
  [ -f "$WORKFLOWS_DIR/release-$m.yml" ] || bad "no release workflow for $m in modules/catalog/.github/workflows/ (release-$m.yml)"
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

if [ -n "$update" ]; then echo "release-publish.sh: updating $org/$catalog_repo"; else echo "release-publish.sh: publishing $org/$catalog_repo"; fi
echo "  modules (${#modules[@]} of ${#all_modules[@]}): ${modules[*]}"
echo "  source commit: ${src_commit:-unknown (no git checkout)}, date $src_epoch${ignored:+ (from the environment, ignored:$ignored)}"
if [ -n "$did" ]; then echo "  signer DID: $did"; else echo "  signer DID: the live catalog's (no key given)"; fi
echo "  workspace: $ws"
if [ -n "$dry_dir" ]; then
  if [ -n "$update" ]; then
    echo "  DRY RUN: every repository a local bare repository under $dry_dir, started from the live one (read-only git); nothing is pushed to GitHub"
  else
    echo "  DRY RUN: every repository a local bare repository under $dry_dir; no network command runs"
  fi
fi
if [ -n "$yes" ] && [ -n "$update" ]; then echo "  --yes: checks the remote side first, then creates, pushes and tags"
elif [ -n "$yes" ]; then echo "  --yes: checks the remote side first, then creates, pushes, tags and sets the secret"; fi

# ── 1. Export each module as its own repository ────────────────────────────────────────────────
# (With --update the version tag is made later, on the commit the module's main will hold.)
for m in "${modules[@]}"; do
  echo "  export $m"
  SOURCE_DATE_EPOCH=$src_epoch FERRATA_COMMIT=${src_commit:-unknown} "$here/export.sh" "$m" "$ws/src/$m" > /dev/null \
    || fail "export.sh $m failed"
  git -C "$ws/src/$m" branch -M main
  if [ -z "$update" ]; then
    git -C "$ws/src/$m" tag -a "v$(version_of "$m")" -m "$m $(version_of "$m"), exported from the Ferrata repository"
  fi
done

# The catalog's README (rule 5's install instructions), written into DIR with the signer's DID; the mini-app's
# install line when the catalog holds it.
write_readme() { # DIR DID WITH_UI(0|1)
  local ui_line=
  if [ "$3" = 1 ]; then
    ui_line='    logosctl install ferrata_bridge_ui -y        # the bridge'"'"'s mini-app (a ui module, for Basecamp)'
  fi
  cat > "$1/README.md" <<EOF
# Ferrata modules

Ferrata's Logos module catalog: \`logos-repo.json\` (identity, the index in \`indexUrl\`, the signer the
releases are signed by in \`trustedSigners\`), the modules as git submodules under \`submodules/\`, the
release workflows that build, sign, verify and publish each one with
[\`logos-co/logos-modules-release-action\`](https://github.com/logos-co/logos-modules-release-action)
(pinned to a commit), and \`index/index.json\`, the list of every published package that
\`rebuild-index.yml\` commits after each release (the packages themselves are in the immutable
\`<module>-v<version>\` releases). How it is published, and each lambda-prize release-integrity rule it meets:
\`modules/catalog/RELEASE.md\` in the source repository (https://github.com/rainbowpuffpuff/ferrata).

The modules are prototype, unaudited, not for production.

## Trust the signing key first

Logos clients do not trust a catalog's \`trustedSigners\` by themselves: a package's signature counts
only for a key in the installer's own keyring, and by default (\`signature_policy: warn\`) an unsigned
package, or one signed by a key the keyring lacks, installs with a warning. So, before installing:

1. Check the signer DID below against the one in the release-integrity manifest of the source
   repository (\`releases/\` of https://github.com/rainbowpuffpuff/ferrata), not only against this page:

       $2

2. Add it to the keyring and make signatures required (Logos Core's \`logosctl\`, per session):

       logosctl key add ferrata --did $2 --display-name "Ferrata module releases"
       # in the daemon configuration (node.yaml), then \`logosctl daemon config set ./node.yaml\`
       # and restart the daemon (the policy takes effect at the next start):
       signature_policy: require

   With \`lgpm\`: \`lgx keyring add ferrata $2\`, then install with \`--require-signatures\`.
   Basecamp's package manager keeps its own keyring: add the key there too.

## Install

    logosctl catalog add https://raw.githubusercontent.com/$org/$catalog_repo/refs/heads/main/logos-repo.json
    logosctl catalog refresh
    logosctl search ferrata
    logosctl install ferrata_attest_module -y
${ui_line:+$ui_line
}
The same URL works in the package-manager UI and with \`lgpd\`.
EOF
}

# The catalog's workflows: the shared ones (every file but the per-module release-<module>.yml) and the
# named modules' own.
copy_workflows() { # DEST_DIR MODULE...
  local dest=$1 f b; shift
  mkdir -p "$dest"
  for f in "$WORKFLOWS_DIR"/*.yml; do
    b=$(basename "$f")
    case "$b" in release-all.yml) cp "$f" "$dest/" ;; release-*.yml) ;; *) cp "$f" "$dest/" ;; esac
  done
  for b in "$@"; do cp "$WORKFLOWS_DIR/release-$b.yml" "$dest/"; done
}

url_of() {   # the URL a repository is read from and pushed to (in a dry run, its local bare repository)
  if [ -n "$dry_dir" ]; then echo "file://$dry_dir/$1.git"; else echo "https://github.com/$org/$1.git"; fi
}
live_url() { echo "https://github.com/$org/$1.git"; }   # where it is published (an update's .gitmodules)
show() { printf '  $'; printf ' %q' "$@"; printf '\n'; }
gh_step() {   # a gh command of the owner step: run with --yes, printed otherwise
  show gh "$@"
  if [ -n "$yes" ]; then "$gh" "$@"; fi
}
wsdir_of() { if [ "$1" = "$catalog_repo" ]; then echo "$ws/catalog"; else echo "$ws/src/$1"; fi; }
desc_of() {
  if [ "$1" = "$catalog_repo" ]; then echo "Ferrata's Logos module catalog (prototype, unaudited)"
  else case "$(jq -r .type "$MODULES_DIR/$1/metadata.json")" in
         ui*) echo "Ferrata $1 - Logos ui module, the mini-app (prototype, unaudited)" ;;
         *) echo "Ferrata $1 - Logos Core module (prototype, unaudited)" ;;
       esac; fi
}
new_bare() { # NAME: a dry run's bare repository, marked as made by a dry run (an earlier one's replaced)
  if [ -e "$dry_dir/$1.git" ] || [ -L "$dry_dir/$1.git" ]; then
    made_by_dry_run "$dry_dir/$1.git" || fail "$dry_dir/$1.git is not a dry run's repository; left as it is"
    rm -rf "$dry_dir/$1.git"
  fi
  git init -q --bare "$dry_dir/$1.git"
  git -C "$dry_dir/$1.git" symbolic-ref HEAD refs/heads/main
  git -C "$dry_dir/$1.git" config release-publish.dryRun true
}
gh_checks() { # the read-only checks of --yes that need gh (before anything is created)
  command -v "$gh" > /dev/null || fail "gh (GitHub CLI) is required with --yes; nothing was created"
  "$gh" auth status > /dev/null 2>&1 || fail "gh is not logged in (gh auth login); nothing was created"
  [ "$("$gh" api "orgs/$org" --jq .login 2> /dev/null | tr 'A-Z' 'a-z')" = "$(echo "$org" | tr 'A-Z' 'a-z')" ] \
    || fail "$org is not a GitHub organization this login can see (the catalog belongs under an organization, RELEASE.md rule 7); nothing was created"
  echo "  gh logged in; $org is an organization; git must push to github.com as the same login (gh auth setup-git)"
}

if [ -n "$update" ]; then
  # ══ --update: a published catalog, changed by fast-forwards ═══════════════════════════════════
  st=$ws/update
  mkdir -p "$st"
  # git for the commits and tags made here: the user's signing and hooks kept out (the same commits everywhere).
  gitp() { git -c commit.gpgSign=false -c tag.gpgSign=false -c core.hooksPath=/dev/null "$@"; }
  echo
  echo "The live state (read-only):"
  if [ -n "$yes" ]; then gh_checks; fi
  # A dry run's bare repositories start as the live ones: every branch and tag fetched from GitHub (git,
  # read-only); one that does not exist there is an empty repository, as --yes would create it. The
  # catalog's main refuses non-fast-forwards and deletions, as its ruleset does.
  if [ -n "$dry_dir" ]; then
    for n in "${names[@]}"; do
      new_bare "$n"
      if GIT_TERMINAL_PROMPT=0 git ls-remote "$(live_url "$n")" > "$st/live-$n" 2> "$st/live-$n.err"; then
        if [ -s "$st/live-$n" ]; then
          GIT_TERMINAL_PROMPT=0 git -C "$dry_dir/$n.git" fetch -q "$(live_url "$n")" '+refs/heads/*:refs/heads/*' '+refs/tags/*:refs/tags/*' \
            || fail "reading $(live_url "$n") failed"
        fi
        echo "  $n: started from $(live_url "$n") (main $(awk '$2 == "refs/heads/main" {print $1}' "$st/live-$n"))"
      else
        git -C "$dry_dir/$n.git" config release-publish.created true
        echo "  $n: not on GitHub; an empty repository stands for the one --yes creates"
      fi
      git -C "$dry_dir/$n.git" config receive.denyNonFastForwards true
      git -C "$dry_dir/$n.git" config receive.denyDeletes true
    done
  fi
  exists() { # NAME: does the repository exist (on GitHub; in a dry run, among the started ones)?
    if [ -n "$dry_dir" ]; then [ "$(git config --file "$dry_dir/$1.git/config" --get release-publish.created 2> /dev/null)" != true ]
    elif [ -n "$yes" ]; then "$gh" repo view "$org/$1" --json name > /dev/null 2>&1
    else GIT_TERMINAL_PROMPT=0 git ls-remote "$(live_url "$1")" > /dev/null 2>&1; fi
  }
  refs_of() { # NAME: its refs, read by git as the pushes will be (a repository git cannot read stops it here)
    GIT_TERMINAL_PROMPT=0 git ls-remote "$(url_of "$1")" > "$st/refs-$1" 2> "$st/refs-$1.err" \
      || fail "$org/$1 exists but git cannot read it ($(tail -n 1 "$st/refs-$1.err")); nothing was pushed (git must reach github.com as the gh login: gh auth setup-git)"
  }
  ref_in() { awk -v r="$2" '$2 == r {print $1}' "$st/refs-$1"; }                 # NAME REF
  peeled_in() { local p; p=$(ref_in "$1" "$2^{}"); if [ -n "$p" ]; then echo "$p"; else ref_in "$1" "$2"; fi; }

  # The catalog: it must exist, its main is the parent of the update.
  exists "$catalog_repo" || fail "$org/$catalog_repo does not exist: --update changes a published catalog (publish it first, without --update); nothing was pushed"
  refs_of "$catalog_repo"
  cat_live=$(ref_in "$catalog_repo" refs/heads/main)
  [ -n "$cat_live" ] || fail "$org/$catalog_repo has no main; nothing was pushed"
  git init -q --template= "$ws/catalog"
  GIT_TERMINAL_PROMPT=0 git -C "$ws/catalog" fetch -q "$(url_of "$catalog_repo")" "+refs/heads/main:refs/remotes/live/main" \
    || fail "fetching $org/$catalog_repo's main failed"
  gitp -C "$ws/catalog" checkout -q -B main refs/remotes/live/main
  [ "$(git -C "$ws/catalog" rev-parse HEAD)" = "$cat_live" ] || fail "$org/$catalog_repo's main moved while it was read; rerun"
  echo "  $org/$catalog_repo: main $cat_live"
  signers=$(jq -r '[.trustedSigners[]?.did] | join(" ")' "$ws/catalog/logos-repo.json" 2> /dev/null || true)
  if [ -n "$did" ]; then
    case " $signers " in *" $did "*) ;; *) fail "the key's DID is not a signer $org/$catalog_repo's logos-repo.json declares ($signers); nothing was pushed" ;; esac
  else
    [ -n "$signers" ] && [ "$(echo "$signers" | wc -w | tr -d ' ')" = 1 ] \
      || fail "$org/$catalog_repo's logos-repo.json declares $(echo "$signers" | wc -w | tr -d ' ') signers; give the key file whose DID the README names"
    did=$signers
    echo "  signer DID (the live logos-repo.json's): $did"
  fi

  # Each module: its new main, decided against the live one.
  : > "$st/plan"
  for m in "${modules[@]}"; do
    v=$(version_of "$m"); t="v$v"; d="$ws/src/$m"
    tree=$(git -C "$d" rev-parse 'HEAD^{tree}')
    live_main=; live_tag=; created=0
    if exists "$m"; then
      refs_of "$m"
      live_main=$(ref_in "$m" refs/heads/main)
      live_tag=$(peeled_in "$m" "refs/tags/$t")
      if [ -n "$live_main" ]; then
        GIT_TERMINAL_PROMPT=0 git -C "$d" fetch -q "$(url_of "$m")" "+refs/heads/main:refs/remotes/live/main" \
          || fail "fetching $org/$m's main failed"
      fi
    else
      created=1
    fi
    # A new commit holds the export's tree with export.sh's identity and date, its message naming the source commit
    # (the module's STAGED.json does too where it has one; the mini-app has none), on top of the live main if any.
    export_as() { # [PARENT]
      GIT_AUTHOR_NAME=ferrata-export GIT_AUTHOR_EMAIL=export@localhost GIT_COMMITTER_NAME=ferrata-export \
        GIT_COMMITTER_EMAIL=export@localhost gitp -C "$d" commit-tree "$tree" ${1:+-p "$1"} \
        -m "$m $v, exported from the Ferrata repository at ${src_commit:-unknown}"
    }
    if [ -z "$live_main" ]; then
      new=$(export_as)
      what="new: main $new"
    elif [ "$(git -C "$d" rev-parse "$live_main^{tree}")" = "$tree" ]; then
      new=$live_main
      what="unchanged: main $live_main holds this export's tree"
    else
      live_v=$(git -C "$d" show "$live_main:metadata.json" 2> /dev/null | jq -r .version 2> /dev/null || echo "")
      [ "$live_v" != "$v" ] \
        || fail "$org/$m's main ($live_main) holds version $v with other content than this export: a published version is never changed (bump modules/$m/metadata.json's version); nothing was pushed"
      new=$(export_as "$live_main")
      what="moved: main $live_main ($live_v) -> $new ($v, fast-forward)"
    fi
    if [ -n "$live_tag" ] && [ "$live_tag" != "$new" ]; then
      fail "$org/$m already has a tag $t at $live_tag, not $new; nothing was pushed (a published version is never moved: bump the module's version)"
    fi
    rel=$(peeled_in "$catalog_repo" "refs/tags/$m-$t")
    if [ -n "$rel" ] && [ "$new" != "$live_main" ]; then
      fail "$org/$catalog_repo has released $m-$t already, and $org/$m's main would change; nothing was pushed (bump modules/$m/metadata.json's version)"
    fi
    git -C "$d" update-ref refs/heads/main "$new"
    gitp -C "$d" reset -q --hard main
    if [ -z "$live_tag" ]; then
      gitp -C "$d" tag -f -a "$t" -m "$m $v, exported from the Ferrata repository" "$new" > /dev/null
    fi
    printf '%s %s %s %s %s %s\n' "$m" "$v" "$new" "${live_main:--}" "${live_tag:--}" "$created" >> "$st/plan"
    echo "  $m $v: $what${live_tag:+; tag $t present}${rel:+; released as $m-$t}"
  done

  # The catalog's commit: the submodules' entries and pointers, their workflows, the README.
  cd "$ws/catalog"
  for m in "${modules[@]}"; do
    new=$(awk -v m="$m" '$1 == m {print $3}' "$st/plan")
    if url=$(git config -f .gitmodules --get "submodule.submodules/$m.url" 2> /dev/null); then
      [ "$url" = "$(live_url "$m")" ] || fail "the catalog's .gitmodules names $url for $m, not $(live_url "$m"); nothing was pushed"
    else
      git config -f .gitmodules "submodule.submodules/$m.path" "submodules/$m"
      git config -f .gitmodules "submodule.submodules/$m.url" "$(live_url "$m")"
    fi
    git update-index --add --cacheinfo "160000,$new,submodules/$m"
    mkdir -p .github/workflows
    cp "$WORKFLOWS_DIR/release-$m.yml" .github/workflows/
  done
  with_ui=0
  if git config -f .gitmodules --get submodule.submodules/ferrata_bridge_ui.path > /dev/null 2>&1; then with_ui=1; fi
  write_readme "$ws/catalog" "$did" "$with_ui"
  git add .gitmodules .github/workflows README.md
  if git diff --cached --quiet; then
    cat_new=$cat_live
    echo "  $org/$catalog_repo: already holds this update (main $cat_live)"
  else
    msg="Ferrata module catalog:"
    while read -r m v new lm lt cr <&3; do
      if [ "$lm" = - ]; then msg="$msg $m $v added;"; elif [ "$new" != "$lm" ]; then msg="$msg $m $v;"; fi
    done 3< "$st/plan"
    gitp commit -q -m "${msg%;} (modules/scripts/release-publish.sh --update)"
    cat_new=$(git rev-parse HEAD)
    echo "  $org/$catalog_repo: main $cat_live -> $cat_new (fast-forward): $(git diff --name-only "$cat_live" "$cat_new" | tr '\n' ' ')"
  fi
  for m in "${modules[@]}"; do
    [ "$(git rev-parse "HEAD:submodules/$m")" = "$(git -C "$ws/src/$m" rev-parse main)" ] \
      || fail "the catalog's pointer for $m is not the module's new main"
  done
  # What this repository's template would write and the update does not carry (said, not changed).
  for f in "$WORKFLOWS_DIR"/*.yml; do
    b=$(basename "$f")
    case "$b" in release-all.yml) ;; release-*.yml) continue ;; esac
    cmp -s "$f" ".github/workflows/$b" || echo "  note: .github/workflows/$b differs from modules/catalog/'s; an update does not carry it"
  done
  if ! cmp -s <(jq -S 'del(.indexUrl, .trustedSigners)' logos-repo.json) <(jq -S 'del(.indexUrl, .trustedSigners)' "$CATALOG_DIR/logos-repo.json"); then
    echo "  note: logos-repo.json's identity differs from modules/catalog/'s; an update does not carry it"
  fi
  cd "$repo"

  # The writes, modules first (the catalog's pointers must resolve), each skipped when already done.
  echo
  if [ -n "$yes" ]; then echo "Creating and pushing (fast-forwards only; each step skipped when done, so a rerun resumes):"
  elif [ -n "$dry_dir" ]; then echo "Into the bare repositories under $dry_dir (what --yes does on GitHub; the gh commands are printed, not run):"
  else echo "The commands --yes runs (nothing runs now):"; fi
  while read -r m v new lm lt cr <&3; do
    d="$ws/src/$m"
    if [ "$cr" = 1 ]; then
      gh_step repo create "$org/$m" --public --description "$(desc_of "$m")" || fail "creating $org/$m failed; rerun to resume"
    fi
    if [ "$new" != "$lm" ]; then
      show git -C "$d" push "$(url_of "$m")" "$new:refs/heads/main"
      if [ -n "$yes" ] || [ -n "$dry_dir" ]; then
        git -C "$d" push -q "$(url_of "$m")" "$new:refs/heads/main" || fail "pushing $m's main failed; rerun to resume"
      fi
    fi
    if [ "$lt" = - ]; then
      show git -C "$d" push "$(url_of "$m")" "refs/tags/v$v"
      if [ -n "$yes" ] || [ -n "$dry_dir" ]; then
        git -C "$d" push -q "$(url_of "$m")" "refs/tags/v$v" || fail "pushing $m's tag v$v failed; rerun to resume"
      fi
    fi
  done 3< "$st/plan"
  if [ "$cat_new" != "$cat_live" ]; then
    show git -C "$ws/catalog" push "$(url_of "$catalog_repo")" "$cat_new:refs/heads/main"
    if [ -n "$yes" ] || [ -n "$dry_dir" ]; then
      git -C "$ws/catalog" push -q "$(url_of "$catalog_repo")" "$cat_new:refs/heads/main" \
        || fail "pushing the catalog's main failed (moved since it was read? rerun: the update is made again on the new main)"
    fi
  fi
  if [ -n "$dry_dir" ]; then
    echo; echo "Bare repositories under $dry_dir:"
    while read -r m v new lm lt cr <&3; do
      echo "  $m.git  main $(git -C "$dry_dir/$m.git" rev-parse refs/heads/main), tag v$v $(git -C "$dry_dir/$m.git" rev-parse "refs/tags/v$v")$([ "$cr" = 1 ] && echo " (created)")"
    done 3< "$st/plan"
    echo "  $catalog_repo.git  main $(git -C "$dry_dir/$catalog_repo.git" rev-parse refs/heads/main) (parent $cat_live)"
  fi
  echo
  if [ -n "$yes" ]; then echo "Done on GitHub: the repositories, their main branches and version tags, the catalog's main."
  else echo "After those commands:"; fi
  echo "The owner's remaining steps, in this order (the releases come last):"
  n_created=$(awk '$6 == 1' "$st/plan" | wc -l | tr -d ' ')
  if [ "$n_created" != 0 ]; then
    echo "  1. [settings] in each repository created here ($(awk '$6 == 1 {printf "%s%s", sep, $1; sep=", "}' "$st/plan")):"
    echo "     turn on immutable releases (the browser, or: gh api -X PUT repos/$org/<repository>/immutable-releases),"
    echo "     as on the catalog's other repositories (modules/catalog/RELEASE.md, rule 2)."
  else
    echo "  1. [settings] none: no repository was created."
  fi
  echo "  2. Only then, the releases: each builds, signs, verifies and publishes one module, then the index:"
  while read -r m v new lm lt cr <&3; do
    if [ -z "$(peeled_in "$catalog_repo" "refs/tags/$m-v$v")" ]; then show gh workflow run "release-$m.yml" --repo "$org/$catalog_repo"; fi
  done 3< "$st/plan"
  echo "  3. Add the new releases to releases/RELEASE-INTEGRITY.md (each value's command is in its template)."
  echo
  echo "release-publish.sh: done.${dry_dir:+ Dry run: repositories under $dry_dir.}"
  exit 0
fi

# ── 2. The generated logos-repo.json: identity kept, indexUrl and trustedSigners written ───────
# The index is index/index.json on the catalog's main, which the catalog's rebuild-index.yml commits
# after every release: a rolling release cannot hold it under immutable releases (RELEASE.md, rule 2).
index_url="https://raw.githubusercontent.com/$org/$catalog_repo/refs/heads/main/index/index.json"
copy_workflows "$ws/catalog/.github/workflows" "${modules[@]}"
jq --arg indexUrl "$index_url" --arg did "$did" \
   '.indexUrl = $indexUrl
  | .trustedSigners = [ { did: $did, name: "Ferrata module releases" } ]' \
   "$CATALOG_DIR/logos-repo.json" > "$ws/catalog/logos-repo.json"
python3 -I -m json.tool "$ws/catalog/logos-repo.json" > /dev/null   # generated file is valid JSON
with_ui=0
for m in "${modules[@]}"; do if [ "$m" = ferrata_bridge_ui ]; then with_ui=1; fi; done
write_readme "$ws/catalog" "$did" "$with_ui"

# ── 3. Assemble the catalog repository: each module a submodule pointing at its repository ─────
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
if [ -n "$yes" ]; then
  # Read-only checks, before anything is created: a failure here leaves the remote side untouched.
  echo; echo "Checks before anything is created:"
  gh_checks
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
        fail "$org/$n exists and its main is $have, not this export's $want; nothing was created (publish into new repositories, or remove the old ones yourself; a published catalog is changed with --update)"
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
  if [ -n "$dry_dir" ]; then new_bare "$n"; fi
  if [ -n "$yes" ] && "$gh" repo view "$org/$n" --json name > /dev/null 2>&1; then
    echo "  $org/$n exists"
  else
    gh_step repo create "$org/$n" --public --description "$(desc_of "$n")" || fail "creating $org/$n failed; rerun to resume"
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
echo "  1. [in the browser] in $org/$catalog_repo and each module repository: turn on immutable releases;"
echo "     add a ruleset protecting refs/tags/* (the release tags <module>-v<version>; updates and deletions"
echo "     restricted, creations allowed: the workflows create them) and one protecting main from deletion"
echo "     and force-pushes (not from pushes: rebuild-index.yml commits index/index.json to it); require"
echo "     2FA for the $org organization. No index release is created: the index lives on main."
echo "     Tokens stay least-privilege: the workflows use the built-in GITHUB_TOKEN with contents alone"
echo "     (modules/catalog/RELEASE.md, rules 2 and 6)."
echo "  2. Only then, the release: builds, signs, verifies and publishes every module, then the index:"
show gh workflow run release-all.yml --repo "$org/$catalog_repo"
echo "  3. Fill releases/RELEASE-INTEGRITY.template.md (each value's command is in it) with the release"
echo "     URLs, the source commits, the signer DID and the rootHashes the sidecars give."
echo
echo "release-publish.sh: done.${dry_dir:+ Dry run: repositories under $dry_dir.}"
