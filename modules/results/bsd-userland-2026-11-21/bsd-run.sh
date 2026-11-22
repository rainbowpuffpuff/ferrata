#!/bin/sh
# Runs inside the FreeBSD-userland container: bash 3.2 first on PATH, no sha256sum (as macOS).
set -u
rm -f /usr/sbin/sha256sum /usr/sbin/sha224sum /usr/sbin/sha384sum /usr/sbin/sha512sum /usr/sbin/sha1sum /usr/sbin/md5sum /usr/bin/sha256sum 2>/dev/null
export PATH=/opt/bash32/bin:$PATH TMPDIR=/work/tmp
mkdir -p /work/tmp /work/out/bsd
cd /work/repo
git config --global --add safe.directory '*'
echo "== tools"; bash --version | head -1; tar --version | head -1; command -v sha256sum || echo "sha256sum: absent"; command -v shasum
echo "== bash 3.2 -n on the macOS scripts"
for f in scripts/fetch-vectors.sh scripts/fetch-upstream.sh modules/scripts/export.sh modules/scripts/stage.sh modules/scripts/stage-relayer.sh modules/scripts/stage-updater.sh modules/scripts/release-publish.sh modules/scripts/test-release-publish.sh; do
  if bash -n "$f"; then echo "  ok $f"; else echo "  SYNTAX $f"; fi
done
echo "== test-release-publish.sh under bash 3.2"
bash modules/scripts/test-release-publish.sh /work/tmp/trp > /work/out/trp-bsd.log 2>&1; echo "  exit $?: $(tail -n 1 /work/out/trp-bsd.log)"
grep '^FAIL' /work/out/trp-bsd.log | head -20
echo "== exports"
for m in ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module ferrata_relayer_module; do
  if bash modules/scripts/export.sh "$m" "/work/out/bsd/$m" > "/work/out/export-bsd-$m.log" 2>&1; then
    echo "  $m tree $(git -C /work/out/bsd/$m rev-parse 'HEAD^{tree}') commit $(git -C /work/out/bsd/$m rev-parse HEAD) files $(git -C /work/out/bsd/$m ls-files | wc -l | tr -d ' ')"
  else echo "  $m EXPORT FAILED: $(tail -n 3 /work/out/export-bsd-$m.log)"; fi
done
echo "== the vector extraction of fetch-vectors.sh on a tarball shaped like minimal.tar.gz"
mkdir -p /work/tmp/vec && cd /work/tmp/vec && rm -rf x && mkdir x
tar -tzf /work/out/minimal-like.tar.gz | grep -E '^tests/minimal/[^/]+/light_client/(sync|update_ranking)/.*[^/]$' > list
tar -xzf /work/out/minimal-like.tar.gz -C x -T list
(cd x && find . -type f | LC_ALL=C sort) > /work/out/vec-bsd.txt; echo "  $(wc -l < /work/out/vec-bsd.txt | tr -d ' ') files"
echo "== done"
