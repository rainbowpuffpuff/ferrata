#!/usr/bin/env bash
# Tests `basecamp-release.sh verify` (the check a user runs on a downloaded release) on synthetic releases: a good one
# passes and says "verified"; each faulty one stops with exit 1, prints "FAILED", and never prints "verified".
# Needs bash, python3, ssh-keygen and sha256sum or shasum; minisign's cases run when minisign is installed. lgx's own
# check is not exercised here (the synthetic packages are not .lgx files): VERIFY_TOOLS leaves it out.
#
#   modules/scripts/test-basecamp-release.sh WORK_DIR      (a new or empty directory; it is filled, not cleaned)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
[ $# = 1 ] || { echo "usage: modules/scripts/test-basecamp-release.sh WORK_DIR" >&2; exit 2; }
mkdir -p "$1"
work=$(cd "$1" && pwd)
[ -z "$(ls -A "$work")" ] || { echo "test-basecamp-release.sh: $work is not empty" >&2; exit 2; }
if command -v sha256sum > /dev/null; then sha() { sha256sum "$1" | cut -d' ' -f1; }; else sha() { shasum -a 256 "$1" | cut -d' ' -f1; }; fi
has_minisign=$(command -v minisign > /dev/null && echo 1 || echo 0)

# The release key and a second, unrelated key, as basecamp-release.sh keygen makes them.
bash "$here/basecamp-release.sh" keygen "$work/key" > /dev/null
bash "$here/basecamp-release.sh" keygen "$work/other" > /dev/null

# A release signed the way `sign` signs it (minus lgx): REL/<platform>/*.lgx with .sig (and .minisig), keys/,
# SHA256SUMS signed. $1: the directory; $2: the key directory that signs.
make_release() {
  local rel=$1 kd=$2 f
  mkdir -p "$rel/linux-amd64" "$rel/darwin-arm64" "$rel/keys"
  head -c 4096 /dev/urandom > "$rel/linux-amd64/a-0.1.0.lgx"
  head -c 2048 /dev/urandom > "$rel/darwin-arm64/b-0.1.0.lgx"
  cp "$work/key/ferrata-release.pub" "$work/key/ferrata-release.did" "$rel/keys/"
  cp "$work/key/ferrata-release.minisign.pub" "$rel/keys/minisign.pub"
  printf 'ferrata-release namespaces="file" %s\n' "$(cut -d' ' -f1,2 "$work/key/ferrata-release.pub")" > "$rel/keys/allowed_signers"
  for f in "$rel"/*/*.lgx; do
    ssh-keygen -Y sign -q -f "$kd/ferrata-release" -n file "$f"
    if [ "$has_minisign" = 1 ]; then minisign -S -s "$kd/ferrata-release.minisign" -m "$f" -x "$f.minisig" > /dev/null; fi
  done
  (cd "$rel" && for f in */*.lgx; do printf '%s  %s\n' "$(sha "$f")" "$f"; done) > "$rel/SHA256SUMS"
  resign_sums "$rel" "$kd"
}
resign_sums() {
  rm -f "$1/SHA256SUMS.sig" "$1/SHA256SUMS.minisig"
  ssh-keygen -Y sign -q -f "$2/ferrata-release" -n file "$1/SHA256SUMS"
  if [ "$has_minisign" = 1 ]; then minisign -S -s "$2/ferrata-release.minisign" -m "$1/SHA256SUMS" -x "$1/SHA256SUMS.minisig" > /dev/null; fi
}

fails=0 n=0
# expect NAME pass|fail: run verify on $work/NAME, check its exit code and words.
expect() {
  local name=$1 want=$2 rc=0 out
  n=$((n + 1))
  out=$(VERIFY_TOOLS="ssh minisign" bash "$here/basecamp-release.sh" verify "$work/$name" 2>&1) || rc=$?
  if [ "$want" = pass ] && [ $rc = 0 ] && printf '%s' "$out" | grep -q '^verified: 2 packages$'; then echo "ok   $name: verified"
  elif [ "$want" = fail ] && [ $rc != 0 ] && printf '%s' "$out" | grep -q '^FAILED' && ! printf '%s' "$out" | grep -q '^verified'; then
    echo "ok   $name: refused ($(printf '%s' "$out" | grep '^FAILED' | head -n 1))"
  else echo "FAIL $name: exit $rc, wanted $want"; printf '%s\n' "$out" | tail -n 5; fails=$((fails + 1)); fi
}

make_release "$work/good" "$work/key"; expect good pass

make_release "$work/swapped-sig" "$work/key"
cp "$work/swapped-sig/darwin-arm64/b-0.1.0.lgx.sig" "$work/swapped-sig/linux-amd64/a-0.1.0.lgx.sig"
expect swapped-sig fail

make_release "$work/missing-sig" "$work/key"; rm "$work/missing-sig/linux-amd64/a-0.1.0.lgx.sig"; expect missing-sig fail

make_release "$work/other-key-sig" "$work/key"
rm "$work/other-key-sig/linux-amd64/a-0.1.0.lgx.sig"
ssh-keygen -Y sign -q -f "$work/other/ferrata-release" -n file "$work/other-key-sig/linux-amd64/a-0.1.0.lgx"
expect other-key-sig fail

make_release "$work/changed-package" "$work/key"
printf 'x' | dd of="$work/changed-package/linux-amd64/a-0.1.0.lgx" bs=1 seek=100 conv=notrunc 2> /dev/null
expect changed-package fail

make_release "$work/missing-package" "$work/key"; rm "$work/missing-package/darwin-arm64/b-0.1.0.lgx"; expect missing-package fail

make_release "$work/sums-other-key" "$work/key"; resign_sums "$work/sums-other-key" "$work/other"; expect sums-other-key fail

make_release "$work/sums-edited" "$work/key"
sed 's/^\(.\)/0/' "$work/sums-edited/SHA256SUMS" > "$work/sums-edited/S" && mv "$work/sums-edited/S" "$work/sums-edited/SHA256SUMS"
expect sums-edited fail

make_release "$work/empty-sums" "$work/key"; : > "$work/empty-sums/SHA256SUMS"; resign_sums "$work/empty-sums" "$work/key"
expect empty-sums fail

if [ "$has_minisign" = 1 ]; then
  make_release "$work/swapped-minisig" "$work/key"
  cp "$work/swapped-minisig/darwin-arm64/b-0.1.0.lgx.minisig" "$work/swapped-minisig/linux-amd64/a-0.1.0.lgx.minisig"
  expect swapped-minisig fail
fi

echo "test-basecamp-release.sh: $((n - fails)) of $n as expected (minisign cases: $([ "$has_minisign" = 1 ] && echo run || echo "not run, no minisign"))"
[ "$fails" = 0 ]
