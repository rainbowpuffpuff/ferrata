#!/usr/bin/env bash
# The Basecamp release: Ferrata's Logos packages (.lgx) for one platform, built from a clean checkout of a
# commit, the way macos.yml builds darwin-arm64 and release-local.sh builds linux-amd64 (export.sh, then
# `nix build .#lgx-portable` in the exported module, then `lgx verify` with the pinned logos-package), plus the
# mini-app (`ferrata_bridge_ui`, a QML view, built from its own flake). Nothing is signed or sent here.
#
#   modules/scripts/basecamp-release.sh build SRC OUT
#   modules/scripts/basecamp-release.sh keygen KEYDIR          a new release key (never overwrites one)
#   modules/scripts/basecamp-release.sh sign REL KEYDIR        sign every REL/<platform>/*.lgx, write SHA256SUMS
#   modules/scripts/basecamp-release.sh verify REL             what a user runs on a downloaded release
#
# SRC: a clean git checkout of the commit to release, with upstream/ beside it (scripts/fetch-upstream.sh).
# OUT: OUT/<platform>/<module>-<version>.lgx, OUT/build.json (each package's size, sha256, rootHash, the export's
# commit and tree, seconds), OUT/log/. Platform: linux-amd64 on x86-64 Linux, darwin-arm64 on an Apple-silicon
# Mac. MODULES names a subset; the default is every module the platform builds (ferrata_updater_module vendors
# x86-64 Linux builds of the circuits and of rapidsnark, so it is linux-amd64 only).
#
# Reproducible: the exports have fixed authors and dates (export.sh), Nix builds every input from pinned
# sources, and lgx writes its archives with fixed owners and times, so the same commit gives the same bytes;
# build twice on two machines and compare build.json's sha256 values. Portable to macOS (bash 3.2, BSD tools).
set -euo pipefail
LGX_PACKAGE=github:logos-co/logos-package/4bdd9041ebfbd06c2b22134afdcd27fce8a43f3c   # macos.yml's and release-local.sh's
here=$(cd "$(dirname "$0")" && pwd)
usage() { echo "usage: modules/scripts/basecamp-release.sh build SRC OUT | keygen KEYDIR | sign REL KEYDIR | verify REL" >&2; exit 2; }
if command -v sha256sum > /dev/null; then sha() { sha256sum "$1" | cut -d' ' -f1; }; else sha() { shasum -a 256 "$1" | cut -d' ' -f1; }; fi
size() { wc -c < "$1" | tr -d ' '; }
SIGNER=ferrata-release

# One Ed25519 key, in the forms ssh-keygen, lgx and minisign take (release_key.py); the private files stay in KEYDIR,
# unencrypted (no passphrase: signing runs unattended), so KEYDIR must stay on the signing machine and in its backups only.
keygen() {
  local kd=$1
  mkdir -p "$kd"
  chmod 700 "$kd"
  [ ! -e "$kd/$SIGNER" ] || { echo "basecamp-release.sh: $kd/$SIGNER exists; a release key is never overwritten" >&2; exit 1; }
  ssh-keygen -q -t ed25519 -N '' -C "$SIGNER" -f "$kd/$SIGNER"
  python3 -I "$here/release_key.py" "$kd/$SIGNER" > "$kd/key.json"
  python3 -I "$here/jwk_did.py" "$kd/$SIGNER.jwk" > "$kd/$SIGNER.did"
  ssh-keygen -l -E sha256 -f "$kd/$SIGNER.pub" > "$kd/fingerprint.txt"
  echo "basecamp-release.sh: release key $(cut -d' ' -f2 "$kd/fingerprint.txt"), $(cat "$kd/$SIGNER.did" | cut -c1-40)..."
}

# The public half, as a user receives it: REL/keys/.
publish_key() {
  local kd=$1 rel=$2
  mkdir -p "$rel/keys"
  cp "$kd/$SIGNER.pub" "$kd/$SIGNER.did" "$kd/fingerprint.txt" "$rel/keys/"
  cp "$kd/$SIGNER.minisign.pub" "$rel/keys/minisign.pub"
  printf '%s namespaces="file" %s\n' "$SIGNER" "$(cut -d' ' -f1,2 "$kd/$SIGNER.pub")" > "$rel/keys/allowed_signers"
}

sign_release() {
  local rel kd kr f did tc
  rel=$(cd "$1" && pwd)
  kd=$(cd "$2" && pwd)
  publish_key "$kd" "$rel"
  did=$(cat "$kd/$SIGNER.did")
  kr="$rel/.keyring" && rm -rf "$kr" && mkdir -p "$kr"
  lgx keyring add ferrata "$did" --display-name "Ferrata release (test network)" --dir "$kr" > /dev/null
  mkdir -p "$rel/log"
  : > "$rel/log/sign.txt"
  for f in "$rel"/*/*.lgx; do
    rm -f "$f.sig" "$f.minisig"
    lgx sign "$f" --key "$kd/$SIGNER.jwk" --name "Ferrata release (test network)" >> "$rel/log/sign.txt" 2>&1
    lgx_signed_by_release "$f" "$kr" 2>> "$rel/log/sign.txt"
    ssh-keygen -Y sign -q -f "$kd/$SIGNER" -n file "$f"
    ssh-keygen -Y verify -f "$rel/keys/allowed_signers" -I "$SIGNER" -n file -s "$f.sig" < "$f" >> "$rel/log/sign.txt" 2>&1
    if command -v minisign > /dev/null; then
      tc="$(basename "$f") sha256 $(sha "$f")"
      minisign -S -s "$kd/$SIGNER.minisign" -m "$f" -x "$f.minisig" -t "$tc" > /dev/null
      minisign -V -q -p "$rel/keys/minisign.pub" -m "$f" -x "$f.minisig" >> "$rel/log/sign.txt" 2>&1
    fi
    echo "signed ${f#$rel/}" >> "$rel/log/sign.txt"
  done
  rm -rf "$kr"
  (cd "$rel" && for f in */*.lgx; do printf '%s  %s\n' "$(sha "$f")" "$f"; done) > "$rel/SHA256SUMS"
  rm -f "$rel/SHA256SUMS.sig" "$rel/SHA256SUMS.minisig"
  ssh-keygen -Y sign -q -f "$kd/$SIGNER" -n file "$rel/SHA256SUMS"
  if command -v minisign > /dev/null; then
    minisign -S -s "$kd/$SIGNER.minisign" -m "$rel/SHA256SUMS" -x "$rel/SHA256SUMS.minisig" -t "Ferrata Basecamp release SHA256SUMS" > /dev/null
  fi
  echo "basecamp-release.sh: signed $(grep -c '^signed ' "$rel/log/sign.txt") packages in $rel"
}

# What a user runs on a downloaded release (REL with SHA256SUMS, keys/ and the packages it lists). Every check stops the
# script at its first failure (exit 1, "FAILED: …" on stderr); "verified" is printed only when all of them passed.
# Checks: the ssh-keygen signature on SHA256SUMS, every package against SHA256SUMS, each package's ssh-keygen
# signature; with minisign installed, SHA256SUMS's and each package's minisign signature; with lgx installed, each
# package's own signature (`lgx verify` exits 0 for an unsigned package and for an unknown signer, so its output must
# say "Signature is valid" and "Signer is trusted: ferrata", the release key being the only key of the keyring used).
# VERIFY_TOOLS (default "ssh minisign lgx") limits the optional checks, for tests on synthetic packages.
lgx_signed_by_release() { # PKG KEYRING_DIR: lgx's own check, its words required (its exit code alone proves nothing)
  local out
  out=$(lgx verify "$1" --keyring-dir "$2" 2>&1) || { echo "$out" >&2; return 1; }
  case "$out" in *"Signature is valid"*) ;; *) echo "$out" >&2; return 1 ;; esac
  case "$out" in *"Signer is trusted: ferrata"*) ;; *) echo "$out" >&2; return 1 ;; esac
}
verify_release() {
  local rel=$1 f kr n=0 tools=${VERIFY_TOOLS:-ssh minisign lgx}
  fail() { echo "FAILED: $*" >&2; exit 1; }
  cd "$rel" || fail "no directory $rel"
  echo "1. the signature on SHA256SUMS (ssh-keygen, key ferrata-release):"
  ssh-keygen -Y verify -f keys/allowed_signers -I "$SIGNER" -n file -s SHA256SUMS.sig < SHA256SUMS || fail "the signature on SHA256SUMS"
  while read -r _ f; do n=$((n + 1)); done < SHA256SUMS
  [ "$n" -gt 0 ] || fail "SHA256SUMS lists no package"
  echo "2. every package against SHA256SUMS:"
  if command -v sha256sum > /dev/null; then sha256sum -c SHA256SUMS || fail "a package does not match SHA256SUMS"
  else shasum -a 256 -c SHA256SUMS || fail "a package does not match SHA256SUMS"; fi
  echo "3. each package's own signature (ssh-keygen):"
  while read -r _ f; do
    ssh-keygen -Y verify -f keys/allowed_signers -I "$SIGNER" -n file -s "$f.sig" < "$f" > /dev/null 2>&1 || fail "$f.sig"
    echo "$f: Good"
  done < SHA256SUMS
  case " $tools " in *" minisign "*)
    if command -v minisign > /dev/null; then
      echo "4. minisign, SHA256SUMS and each package:"
      minisign -V -q -p keys/minisign.pub -m SHA256SUMS -x SHA256SUMS.minisig || fail "SHA256SUMS.minisig"
      while read -r _ f; do
        minisign -V -q -p keys/minisign.pub -m "$f" -x "$f.minisig" || fail "$f.minisig"
        echo "$f: Good"
      done < SHA256SUMS
    else echo "4. minisign: not installed, not checked"; fi ;;
  *) echo "4. minisign: left out (VERIFY_TOOLS)" ;;
  esac
  case " $tools " in *" lgx "*)
    if command -v lgx > /dev/null; then
      echo "5. lgx verify (the signature in the package; the release key the keyring's only key):"
      kr=.verify-keyring && rm -rf "$kr" && mkdir "$kr"
      lgx keyring add ferrata "$(cat "keys/$SIGNER.did")" --dir "$kr" > /dev/null || fail "lgx keyring add"
      while read -r _ f; do
        lgx_signed_by_release "$f" "$kr" || { rm -rf "$kr"; fail "$f: lgx does not say it is signed by the release key"; }
        echo "$f: signature valid, signer trusted (the release key)"
      done < SHA256SUMS
      rm -rf "$kr"
    else echo "5. lgx: not installed, not checked"; fi ;;
  *) echo "5. lgx: left out (VERIFY_TOOLS)" ;;
  esac
  echo "verified: $n packages"
}

case "${1:-}" in
  keygen) [ $# = 2 ] || usage; keygen "$2"; exit 0 ;;
  sign) [ $# = 3 ] || usage; sign_release "$2" "$3"; exit 0 ;;
  verify) [ $# = 2 ] || usage; verify_release "$2"; exit 0 ;;
  build) [ $# = 3 ] || usage ;;
  *) usage ;;
esac
src=$(cd "$2" && pwd)
mkdir -p "$3"
out=$(cd "$3" && pwd)
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform=linux-amd64 ;;
  Darwin-arm64) platform=darwin-arm64 ;;
  *) echo "basecamp-release.sh: no Logos variant for $(uname -s)-$(uname -m)" >&2; exit 2 ;;
esac
default_modules="ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module ferrata_relayer_module"
[ "$platform" = linux-amd64 ] && default_modules="$default_modules ferrata_updater_module"
MODULES=${MODULES:-"$default_modules ferrata_bridge_ui"}
# The checkout must be exactly its commit: export.sh records HEAD, and the mini-app's flake reads the directory.
commit=$(git -C "$src" rev-parse --verify HEAD)
if [ -n "$(git -C "$src" status --porcelain --untracked-files=all)" ]; then
  echo "basecamp-release.sh: $src is not clean (git status); build from a fresh checkout" >&2; exit 1
fi
[ -d "$(dirname "$src")/upstream" ] || { echo "basecamp-release.sh: no upstream/ beside $src (scripts/fetch-upstream.sh)" >&2; exit 1; }
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; else ulimit -s 65536 2> /dev/null || ulimit -s hard; fi
rm -rf "$out/src" "$out/$platform" "$out/log" "$out/build.json"
mkdir -p "$out/src" "$out/$platform" "$out/log"

# nix build, retried only on the two failures of the Modal build box (a gVisor container) that check.sh names.
nixb() {
  local attempt log="$out/log/nix-attempt.log"
  for attempt in 1 2 3 4 5 6 7 8 9 10 11 12; do
    if [ "$(id -u)" = 0 ] && [ -d /homeless-shelter ]; then rm -rf /homeless-shelter; fi
    if nix build "$@" > "$log" 2>&1; then cat "$log"; return 0; fi
    grep -q -E 'reading a line: Input/output error|unexpected EOF reading a line|/homeless-shelter. exists' "$log" \
      || { cat "$log"; return 1; }
    echo "basecamp-release.sh: nix build stopped on a build-box error (attempt $attempt); retrying" >&2
    sleep 1
  done
  cat "$log"; return 1
}

nixb "$LGX_PACKAGE#lgx" -o "$out/lgx-tool" > "$out/log/lgx-tool.log" 2>&1
export PATH="$out/lgx-tool/bin:$PATH"
{ nix --version; uname -srm; echo "source $commit"; echo "lgx $LGX_PACKAGE"; } > "$out/log/versions.txt"

# ferrata_updater_module's export embeds LEZ's guests as risc0-build made them (stage-updater.sh): build them first.
# Guest builds do not depend on the build path (docs/image-ids.md), so the staged ids are the recorded ones
# (stage-updater.sh checks them). RISC0_HOME must name the RISC Zero home with guest Rust 1.91.1 (lez/RESULTS.md).
case " $MODULES " in *" ferrata_updater_module "*)
  if [ -z "$(find "$src/lez/target" -path '*/build/methods-*/out/methods.rs' 2> /dev/null | head -n 1)" ]; then
    [ -n "${RISC0_HOME:-}" ] || { echo "basecamp-release.sh: the updater needs lez/methods built; set RISC0_HOME (lez/RESULTS.md)" >&2; exit 1; }
    t0=$(date +%s)
    (cd "$src/lez" && cargo build --locked -p methods) > "$out/log/lez-methods.log" 2>&1 \
      || { echo "basecamp-release.sh: lez/methods failed to build (log/lez-methods.log)" >&2; tail -n 20 "$out/log/lez-methods.log" >&2; exit 1; }
    echo "basecamp-release.sh: lez/methods built in $(( $(date +%s) - t0 )) s"
  fi ;;
esac

entries=""
for m in $MODULES; do
  t0=$(date +%s)
  if [ "$m" = ferrata_bridge_ui ]; then
    # A ui_qml module: no export (nothing staged), its flake read from the clean checkout.
    flake="path:$src/modules/$m"
    meta="$src/modules/$m/metadata.json"
    export_commit="" export_tree=""
  else
    "$src/modules/scripts/export.sh" "$m" "$out/src/$m" > "$out/log/export-$m.txt"
    flake="$out/src/$m"
    meta="$out/src/$m/metadata.json"
    export_commit=$(git -C "$out/src/$m" rev-parse HEAD)
    export_tree=$(git -C "$out/src/$m" rev-parse 'HEAD^{tree}')
  fi
  (cd "$flake" 2> /dev/null || cd "$out"; nixb "$flake#lgx-portable" --print-out-paths --no-link) > "$out/log/build-$m.log" 2>&1 \
    || { echo "basecamp-release.sh: $m failed to build (log/build-$m.log):" >&2; tail -n 30 "$out/log/build-$m.log" >&2; exit 1; }
  store=$(grep -E '^/nix/store/[^ ]+$' "$out/log/build-$m.log" | tail -n 1)
  built=$(find "$store/" -maxdepth 2 -name '*.lgx' | head -n 1)
  [ -n "$built" ] || { echo "basecamp-release.sh: no .lgx in $store" >&2; exit 1; }
  version=$(jq -r .version "$meta")
  file="$platform/$m-$version.lgx"
  cp "$built" "$out/$file"
  chmod 644 "$out/$file"
  lgx verify "$out/$file" > "$out/log/verify-$m.txt" 2>&1
  manifest=$(lgx manifest "$out/$file" --json)
  [ "$(jq -r .name <<< "$manifest")" = "$m" ] || { echo "basecamp-release.sh: $file's manifest names another module" >&2; exit 1; }
  entries="$entries$(jq -cn --arg name "$m" --arg version "$version" --arg platform "$platform" --arg file "$file" \
    --argjson size "$(size "$out/$file")" --arg sha256 "$(sha "$out/$file")" --argjson manifest "$manifest" \
    --arg export_commit "$export_commit" --arg export_tree "$export_tree" --arg store "$store" \
    --argjson seconds "$(( $(date +%s) - t0 ))" \
    '{name: $name, version: $version, platform: $platform, file: $file, size: $size, sha256: $sha256,
      rootHash: ($manifest.hashes.root // ""), type: ($manifest.type // ""), variants: ($manifest.main // {} | keys),
      export_commit: $export_commit, export_tree: $export_tree, nix_out: $store, seconds: $seconds}')
"
  echo "basecamp-release.sh: $file $(size "$out/$file") bytes, sha256 $(sha "$out/$file"), $(( $(date +%s) - t0 )) s"
done
printf '%s' "$entries" | jq -s --arg commit "$commit" --arg platform "$platform" --arg lgx "$LGX_PACKAGE" \
  --arg nix "$(nix --version)" '{source_commit: $commit, platform: $platform, lgx_package: $lgx, nix: $nix, packages: .}' \
  > "$out/build.json"
(cd "$out/$platform" && for f in *.lgx; do printf '%s  %s\n' "$(sha "$f")" "$f"; done) > "$out/$platform/SHA256SUMS"
echo "basecamp-release.sh: $(jq '.packages | length' "$out/build.json") packages for $platform from $commit in $out"
