#!/usr/bin/env bash
# The tests of modules/scripts/release-publish.sh (and of jwk_did.py and export.sh's reproducibility), with
# no network: a stub `gh` (RELEASE_PUBLISH_GH) that keeps "GitHub" as bare repositories under the scratch
# directory, git's https://github.com/ URLs rewritten to them (url.<base>.insteadOf through
# GIT_CONFIG_COUNT) and every transport but file:// refused (GIT_ALLOW_PROTOCOL=file), and a throwaway
# Ed25519 key (made by openssl when it has Ed25519, else RFC 8032's first test key). Two modules
# (attestor, monitor: their staging needs nothing outside the repository) stand for the six.
#
# Cases: usage errors; bad keys (not JSON, no d, x not d's public key, short d), refused before
# anything and without ever printing d; --dry-run with --yes; the default mode (prints, runs no gh);
# MODULES from the environment honoured without --yes and refused with it; a dry run into a relative
# directory (absolute file:// URLs, HEAD on main, the catalog clones with its submodules, the generated
# logos-repo.json and README), run again into the same directory, and refused when the directory holds
# a <name>.git no dry run made; --yes refused before anything is created when gh is missing, not logged
# in, ORG not an organization, a repository holding another main, another tag (on the second module, so
# that a refusal turned into a warning would create the first), or one git cannot read; --yes on a fresh
# organization (every step in order, the secret from the key file, the release trigger not run, the key
# nowhere on disk); a rerun with SOURCE_DATE_EPOCH and FERRATA_COMMIT inherited (ignored: nothing
# created, nothing moved); a failure at the secret step, then the rerun that finishes; no TMPDIR at all;
# two exports of a module a second apart, one commit.
#
#   bash modules/scripts/test-release-publish.sh [SCRATCH-DIR]   (default: a new directory under TMPDIR)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
S=${1:-$(mktemp -d "${TMPDIR:-/tmp}/test-release-publish.XXXXXX")}
mkdir -p "$S"
S=$(cd "$S" && pwd)
rm -rf "$S/github" "$S/bin" "$S/keys" "$S/runs" "$S/systmp" "$S/exports" "$S/other" "$S/clone" "$S/reldry" "$S/otherdry"
mkdir -p "$S/github" "$S/bin" "$S/keys" "$S/runs" "$S/systmp" "$S/exports"
P=$here/release-publish.sh
MODS="ferrata_attestor_module ferrata_monitor_module"
ORG=example-org
CAT=ferrata-modules
if command -v sha256sum > /dev/null; then SHA256=(sha256sum); else SHA256=(shasum -a 256); fi

npass=0; nfail=0
ok() { echo "ok   $*"; npass=$((npass + 1)); }
no() { echo "FAIL $*"; nfail=$((nfail + 1)); }
check() { local what=$1; shift; if "$@" > /dev/null 2>&1; then ok "$what"; else no "$what"; fi; }

# ── The stub gh: GitHub as bare repositories under $STUB_GH_ROOT/<org>/<repo>.git; every call logged ──
cat > "$S/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -u
root=$STUB_GH_ROOT
echo "gh $*" >> "$root/gh.log"
if [ -n "${STUB_GH_FAIL_ON:-}" ]; then
  case "$*" in "$STUB_GH_FAIL_ON"*) echo "stub gh: failing as told: $*" >&2; exit 1 ;; esac
fi
case "${1:-}" in
  auth) if [ "${STUB_GH_AUTH:-ok}" = ok ]; then exit 0; fi; echo "You are not logged into any GitHub hosts." >&2; exit 1 ;;
  api)
    case "$2" in orgs/*) ;; *) echo "stub gh: unexpected api call $2" >&2; exit 1 ;; esac
    if [ "${STUB_GH_ORG_KIND:-org}" = org ]; then echo "${2#orgs/}"; exit 0; fi
    echo 'gh: Not Found (HTTP 404)' >&2; exit 1 ;;
  repo)
    case "$2" in
      view) if [ -d "$root/$3.git" ]; then echo "{\"name\":\"${3#*/}\"}"; exit 0; fi
            echo "GraphQL: Could not resolve to a Repository with the name '$3'." >&2; exit 1 ;;
      create) if [ -d "$root/$3.git" ]; then echo "stub gh: $3 already exists" >&2; exit 1; fi
              mkdir -p "$(dirname "$root/$3.git")"
              git init -q --bare "$root/$3.git" && git -C "$root/$3.git" symbolic-ref HEAD refs/heads/main
              exit 0 ;;
    esac ;;
  secret)
    # gh secret set NAME --repo ORG/REPO, the value on standard input: only its sha256 is kept
    if [ "$2" = set ] && [ "$4" = --repo ]; then
      mkdir -p "$root/secrets/$5"
      if command -v sha256sum > /dev/null; then sha256sum; else shasum -a 256; fi | cut -d' ' -f1 > "$root/secrets/$5/$3.sha256"
      exit 0
    fi ;;
  workflow) exit 0 ;;
esac
echo "stub gh: unexpected call: $*" >&2
exit 1
EOF
chmod +x "$S/bin/gh"
# mktemp for the run without TMPDIR: the system default (/tmp) mapped into the scratch directory, so that
# the test sees which base the script chose without writing outside its own directory.
real_mktemp=$(command -v mktemp)
cat > "$S/bin/mktemp" <<EOF
#!/usr/bin/env bash
args=()
for a in "\$@"; do case "\$a" in /tmp/*) args+=("$S/systmp/\${a#/tmp/}") ;; *) args+=("\$a") ;; esac; done
exec "$real_mktemp" "\${args[@]}"
EOF
chmod +x "$S/bin/mktemp"

# The stubs first on PATH as well, so that nothing here can reach a real gh.
export PATH="$S/bin:$PATH"
export STUB_GH_ROOT=$S/github
export RELEASE_PUBLISH_GH=$S/bin/gh
export GIT_ALLOW_PROTOCOL=file
export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0="url.file://$S/github/.insteadOf" GIT_CONFIG_VALUE_0=https://github.com/
export TMPDIR=$S/runs
unset MODULES SOURCE_DATE_EPOCH FERRATA_COMMIT
: > "$S/github/gh.log"

# ── Keys ─────────────────────────────────────────────────────────────────────────────────────────
b64u() { base64 | tr '+/' '-_' | tr -d '=\n'; }
key_from_pem() { # PEM -> JWK, the recipe of modules/catalog/RELEASE.md
  local d x
  d=$(openssl pkey -in "$1" -outform DER | tail -c 32 | b64u)
  x=$(openssl pkey -in "$1" -pubout -outform DER | tail -c 32 | b64u)
  printf '{"crv":"Ed25519","d":"%s","kty":"OKP","x":"%s"}\n' "$d" "$x"
}
if openssl genpkey -algorithm ed25519 -out "$S/keys/a.pem" > /dev/null 2>&1 \
   && openssl genpkey -algorithm ed25519 -out "$S/keys/b.pem" > /dev/null 2>&1; then
  key_from_pem "$S/keys/a.pem" > "$S/keys/key.jwk"
  key_from_pem "$S/keys/b.pem" > "$S/keys/other.jwk"
  echo "keys: two throwaway Ed25519 keys from openssl"
else
  # RFC 8032 section 7.1, TEST 1 and TEST 2 (public test keys, throwaway by definition).
  hexb64u() { python3 -I -c 'import base64, sys; print(base64.urlsafe_b64encode(bytes.fromhex(sys.argv[1])).rstrip(b"=").decode())' "$1"; }
  printf '{"crv":"Ed25519","d":"%s","kty":"OKP","x":"%s"}\n' \
    "$(hexb64u 9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60)" \
    "$(hexb64u d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a)" > "$S/keys/key.jwk"
  printf '{"crv":"Ed25519","d":"%s","kty":"OKP","x":"%s"}\n' \
    "$(hexb64u 4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb)" \
    "$(hexb64u 3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c)" > "$S/keys/other.jwk"
  echo "keys: RFC 8032 test keys (this openssl has no Ed25519)"
fi
D=$(jq -r .d "$S/keys/key.jwk")
jq -c --arg x "$(jq -r .x "$S/keys/other.jwk")" '.x = $x' "$S/keys/key.jwk" > "$S/keys/mismatch.jwk"
jq -c 'del(.d)' "$S/keys/key.jwk" > "$S/keys/no-d.jwk"
jq -c '.d = "AAAA"' "$S/keys/key.jwk" > "$S/keys/short-d.jwk"
echo 'not json' > "$S/keys/not-json.jwk"
DID=$(python3 -I "$here/jwk_did.py" "$S/keys/key.jwk")
check "jwk_did.py: the key's DID is did:jwk of its public half" \
  test "$DID" = "did:jwk:$(printf '{"crv":"Ed25519","kty":"OKP","x":"%s"}' "$(jq -r .x "$S/keys/key.jwk")" | b64u)"

run_bare() { # NAME ARGS...: run release-publish.sh, output to runs/NAME.out, exit status to runs/NAME.rc
  local name=$1; shift
  local rc=0
  bash "$P" "$@" > "$S/runs/$name.out" 2>&1 || rc=$?
  echo "$rc" > "$S/runs/$name.rc"
}
run() { # NAME ARGS...: the same with --modules "$MODS" (the two modules standing for the six)
  local name=$1; shift
  run_bare "$name" "$@" --modules "$MODS"
}
rc() { cat "$S/runs/$1.rc"; }
out_has() { grep -q -F -- "$2" "$S/runs/$1.out"; }
creates() { grep -c '^gh repo create' "$S/github/gh.log" || true; }
no_d_in() { ! grep -r -q -F -- "$D" "$@"; }

# ── Usage and input errors: exit 2, nothing done ───────────────────────────────────────────────
run noargs
check "no arguments: exit 2 with the usage" test "$(rc noargs)" = 2
check "no arguments: the usage names the flags" out_has noargs "--dry-run DIR"
run unknown "$ORG" "$CAT" "$S/keys/key.jwk" --frobnicate
check "unknown argument: exit 2" test "$(rc unknown)" = 2
run both "$ORG" "$CAT" "$S/keys/key.jwk" --dry-run "$S/x" --yes
check "--dry-run with --yes: exit 2" test "$(rc both)" = 2
run badorg "bad/org" "$CAT" "$S/keys/key.jwk"
check "ORG with a slash: exit 2" test "$(rc badorg)" = 2
for k in not-json no-d mismatch short-d; do
  run "key-$k" "$ORG" "$CAT" "$S/keys/$k.jwk" --yes
  check "key $k: exit 2" test "$(rc "key-$k")" = 2
  check "key $k: d never printed" no_d_in "$S/runs/key-$k.out"
done
check "key mismatch: names x and d" out_has key-mismatch "x is not the public key of d"
check "bad keys: no gh call at all" test ! -s "$S/github/gh.log"
check "bad keys: no workspace made" test -z "$(ls "$S/runs" | grep release-publish || true)"

# ── The default mode: prints the plan, runs no gh, pushes nothing ──────────────────────────────
run plan "$ORG" "$CAT" "$S/keys/key.jwk"
check "default mode: exit 0" test "$(rc plan)" = 0
check "default mode: no gh call" test ! -s "$S/github/gh.log"
check "default mode: prints the release trigger as the owner's last step" out_has plan "gh workflow run release-all.yml"

# ── MODULES from the environment: a subset without --yes; refused with --yes ──────────────────────
MODULES=ferrata_monitor_module run_bare envplan "$ORG" "$CAT" "$S/keys/key.jwk"
check "MODULES without --yes: exit 0" test "$(rc envplan)" = 0
check "MODULES without --yes: the subset it names" out_has envplan "modules (1 of "
MODULES=ferrata_monitor_module run_bare envyes "$ORG" "$CAT" "$S/keys/key.jwk" --yes
check "MODULES with --yes: refused, exit 2" test "$(rc envyes)" = 2
check "MODULES with --yes: says to name the subset with --modules" out_has envyes "MODULES is set in the environment"
check "MODULES with --yes: no gh call at all" test ! -s "$S/github/gh.log"

# ── The dry run, into a relative directory ─────────────────────────────────────────────────────
(cd "$S" && run dry "$ORG" "$CAT" "$S/keys/key.jwk" --dry-run reldry)
DRY=$S/reldry
check "dry run: exit 0" test "$(rc dry)" = 0
check "dry run: no gh call" test ! -s "$S/github/gh.log"
for n in $MODS "$CAT"; do
  check "dry run: $n.git's HEAD is main" test "$(git -C "$DRY/$n.git" symbolic-ref HEAD)" = refs/heads/main
done
git clone -q "file://$DRY/$CAT.git" "$S/clone" 2> /dev/null || true
check "dry run: the catalog clones with its files checked out" test -f "$S/clone/logos-repo.json"
check "dry run: .gitmodules URLs are absolute file:// URLs" \
  test "$(git -C "$S/clone" config -f .gitmodules --get-regexp 'url$' | awk '{print $2}' | grep -c "^file://$DRY/")" = 2
git -C "$S/clone" -c protocol.file.allow=always submodule -q update --init 2> /dev/null || true
check "dry run: the submodules check out at the exported commits" test -f "$S/clone/submodules/ferrata_monitor_module/metadata.json"
check "dry run: logos-repo.json declares the key's DID" test "$(jq -r '.trustedSigners[0].did' "$S/clone/logos-repo.json")" = "$DID"
check "dry run: logos-repo.json's indexUrl" \
  test "$(jq -r .indexUrl "$S/clone/logos-repo.json")" = "https://raw.githubusercontent.com/$ORG/$CAT/refs/heads/main/index/index.json"
check "dry run: the owner's steps create no index release (the index is index/index.json on main)" \
  sh -c '! grep -q -F "gh release create" "$1"' sh "$S/runs/dry.out"
check "dry run: the catalog's rebuild-index.yml builds lgx at a pinned logos-package commit" \
  sh -c 'grep -q -E "nix build github:logos-co/logos-package/[0-9a-f]{40}#lgx " "$1" && ! grep -q -F "logos-package#" "$1"' \
  sh "$S/clone/.github/workflows/rebuild-index.yml"
check "dry run: README says to add the key to the keyring" grep -q -F "logosctl key add ferrata --did $DID" "$S/clone/README.md"
check "dry run: README says to require signatures" grep -q -F "signature_policy: require" "$S/clone/README.md"
check "dry run: module tags pushed" test -n "$(git -C "$DRY/ferrata_monitor_module.git" tag -l 'v*')"
WS_DRY=$(sed -n 's/^  workspace: //p' "$S/runs/dry.out")
check "dry run: the key nowhere in the workspace or the repositories" no_d_in "$WS_DRY" "$DRY"
(cd "$S" && run dryagain "$ORG" "$CAT" "$S/keys/key.jwk" --dry-run reldry)
check "dry run again into the same directory: exit 0, its own repositories replaced" test "$(rc dryagain)" = 0
# A directory holding a <name>.git that no dry run made: refused before anything, left as it was.
mkdir -p "$S/otherdry/ferrata_monitor_module.git" && echo keep > "$S/otherdry/ferrata_monitor_module.git/keep"
run dryforeign "$ORG" "$CAT" "$S/keys/key.jwk" --dry-run "$S/otherdry"
check "dry run into a foreign <name>.git: refused, exit 2" test "$(rc dryforeign)" = 2
check "dry run into a foreign <name>.git: says so" out_has dryforeign "was not made by a dry run of this script"
check "dry run into a foreign <name>.git: left as it was" test -f "$S/otherdry/ferrata_monitor_module.git/keep"
check "dry run into a foreign <name>.git: nothing else made" test "$(ls "$S/otherdry")" = ferrata_monitor_module.git

# ── --yes refused before anything is created ───────────────────────────────────────────────────
RELEASE_PUBLISH_GH=$S/no-such-gh run nogh "$ORG" "$CAT" "$S/keys/key.jwk" --yes
check "--yes without gh: exit 1" test "$(rc nogh)" = 1
STUB_GH_AUTH=no run noauth "$ORG" "$CAT" "$S/keys/key.jwk" --yes
check "--yes not logged in: exit 1" test "$(rc noauth)" = 1
STUB_GH_ORG_KIND=user run notorg "$ORG" "$CAT" "$S/keys/key.jwk" --yes
check "--yes when ORG is not an organization: exit 1" test "$(rc notorg)" = 1
# A module repository that already holds another main.
git init -q --bare "$S/github/$ORG/ferrata_monitor_module.git"
git init -q "$S/other" && git -C "$S/other" -c user.name=t -c user.email=t@localhost commit -q --allow-empty -m other
git -C "$S/other" push -q "file://$S/github/$ORG/ferrata_monitor_module.git" HEAD:refs/heads/main
run othermain "$ORG" "$CAT" "$S/keys/key.jwk" --yes
check "--yes when a repository holds another main: exit 1" test "$(rc othermain)" = 1
check "--yes when a repository holds another main: says so" out_has othermain "exists and its main is"
rm -rf "$S/github/$ORG/ferrata_monitor_module.git"
# A module repository whose version tag is another object: the SECOND module (attestor comes first), so
# that the refusal is what stops it, not the tag push that would fail after the first create.
git init -q --bare "$S/github/$ORG/ferrata_monitor_module.git"
V=$(jq -r .version "$repo/modules/ferrata_monitor_module/metadata.json")
git -C "$S/other" -c user.name=t -c user.email=t@localhost tag -a "v$V" -m other
git -C "$S/other" push -q "file://$S/github/$ORG/ferrata_monitor_module.git" "refs/tags/v$V"
run othertag "$ORG" "$CAT" "$S/keys/key.jwk" --yes
check "--yes when a version tag exists elsewhere: exit 1" test "$(rc othertag)" = 1
check "--yes when a version tag exists elsewhere: says so" out_has othertag "already has a tag v$V"
check "--yes when a version tag exists elsewhere: nothing created" test "$(creates)" = 0
rm -rf "$S/github/$ORG/ferrata_monitor_module.git"
# A repository that gh sees and git cannot read.
mkdir -p "$S/github/$ORG/ferrata_monitor_module.git"
run unreadable "$ORG" "$CAT" "$S/keys/key.jwk" --yes
check "--yes when git cannot read an existing repository: exit 1" test "$(rc unreadable)" = 1
check "--yes when git cannot read an existing repository: says so" out_has unreadable "exists but git cannot read it"
check "--yes when git cannot read an existing repository: nothing created" test "$(creates)" = 0
rm -rf "$S/github/$ORG/ferrata_monitor_module.git"
check "--yes refusals: no repository created" test "$(creates)" = 0
check "--yes refusals: no secret set" test ! -d "$S/github/secrets"

# ── --yes on a fresh organization ──────────────────────────────────────────────────────────────
: > "$S/github/gh.log"
run yes1 "$ORG" "$CAT" "$S/keys/key.jwk" --yes
check "--yes: exit 0" test "$(rc yes1)" = 0
check "--yes: three repositories created" test "$(creates)" = 3
check "--yes: checks (auth, org) come before the first create" \
  test "$(grep -n -m1 '^gh repo create' "$S/github/gh.log" | cut -d: -f1)" -gt "$(grep -n -m1 '^gh api orgs/' "$S/github/gh.log" | cut -d: -f1)"
check "--yes: the release trigger is not run" test "$(grep -c '^gh workflow' "$S/github/gh.log" || true)" = 0
check "--yes: the secret is the key file, read from standard input" \
  test "$(cat "$S/github/secrets/$ORG/$CAT/LOGOS_SIGNING_KEY.sha256")" = "$("${SHA256[@]}" < "$S/keys/key.jwk" | cut -d' ' -f1)"
WS1=$(sed -n 's/^  workspace: //p' "$S/runs/yes1.out")
for m in $MODS; do
  check "--yes: $m's main is the export" \
    test "$(git -C "$S/github/$ORG/$m.git" rev-parse refs/heads/main)" = "$(git -C "$WS1/src/$m" rev-parse HEAD)"
  check "--yes: $m's version tag pushed" git -C "$S/github/$ORG/$m.git" rev-parse "refs/tags/v$(jq -r .version "$repo/modules/$m/metadata.json")"
done
check "--yes: the catalog's main is the assembled catalog" \
  test "$(git -C "$S/github/$ORG/$CAT.git" rev-parse refs/heads/main)" = "$(git -C "$WS1/catalog" rev-parse HEAD)"
check "--yes: the catalog's submodules point at https://github.com/$ORG" \
  test "$(git -C "$WS1/catalog" config -f .gitmodules --get-regexp 'url$' | awk '{print $2}' | grep -c "^https://github.com/$ORG/")" = 2
check "--yes: the browser steps come before the trigger in the output" \
  test "$(grep -n -m1 'turn on immutable releases' "$S/runs/yes1.out" | cut -d: -f1)" -lt "$(grep -n -m1 'gh workflow run' "$S/runs/yes1.out" | cut -d: -f1)"
check "--yes: the key nowhere in the workspace or the repositories" no_d_in "$WS1" "$S/github"

# ── A rerun: nothing created, nothing moved ────────────────────────────────────────────────────
refs_now() { for n in $MODS "$CAT"; do git -C "$S/github/$ORG/$n.git" for-each-ref --format='%(refname) %(objectname)' 2> /dev/null || echo "$n missing"; done; }
before=$(refs_now)
: > "$S/github/gh.log"
SOURCE_DATE_EPOCH=315532800 FERRATA_COMMIT=0000000000000000000000000000000000000000 run yes2 "$ORG" "$CAT" "$S/keys/key.jwk" --yes
after=$(refs_now)
check "rerun: exit 0" test "$(rc yes2)" = 0
check "rerun: no repository created" test "$(creates)" = 0
check "rerun: every ref as before (exports reproducible)" test "$before" = "$after"
check "rerun: an inherited SOURCE_DATE_EPOCH and FERRATA_COMMIT ignored, and said so" \
  out_has yes2 "(from the environment, ignored: SOURCE_DATE_EPOCH=315532800 FERRATA_COMMIT=0000000000000000000000000000000000000000)"

# ── A failure at the secret step, then the rerun that finishes ─────────────────────────────────
rm -rf "$S/github/$ORG" "$S/github/secrets"
: > "$S/github/gh.log"
STUB_GH_FAIL_ON="secret set" run failsecret "$ORG" "$CAT" "$S/keys/key.jwk" --yes
check "secret step fails: exit 1" test "$(rc failsecret)" = 1
check "secret step fails: says a rerun resumes" out_has failsecret "rerun to resume"
check "secret step fails: no secret" test ! -d "$S/github/secrets"
: > "$S/github/gh.log"
run resume "$ORG" "$CAT" "$S/keys/key.jwk" --yes
check "resume: exit 0" test "$(rc resume)" = 0
check "resume: no repository created again" test "$(creates)" = 0
check "resume: the secret set" test -f "$S/github/secrets/$ORG/$CAT/LOGOS_SIGNING_KEY.sha256"

# ── Without TMPDIR: the system's temporary directory (mapped here), no refusal ─────────────────
(unset TMPDIR; run notmpdir "$ORG" "$CAT" "$S/keys/key.jwk")
check "no TMPDIR: exit 0" test "$(rc notmpdir)" = 0
check "no TMPDIR: the workspace in the system default" test -n "$(sed -n "s#^  workspace: $S/systmp/release-publish\.#&#p" "$S/runs/notmpdir.out")"

# ── export.sh: the same source gives the same commit ───────────────────────────────────────────
bash "$here/export.sh" ferrata_monitor_module "$S/exports/a" > /dev/null
sleep 1   # a time-stamped export would now differ
bash "$here/export.sh" ferrata_monitor_module "$S/exports/b" > /dev/null
check "export.sh: two exports a second apart, one commit" test "$(git -C "$S/exports/a" rev-parse HEAD)" = "$(git -C "$S/exports/b" rev-parse HEAD)"

echo
echo "test-release-publish.sh: $npass passed, $nfail failed (bash $BASH_VERSION; scratch $S)"
[ "$nfail" = 0 ]
