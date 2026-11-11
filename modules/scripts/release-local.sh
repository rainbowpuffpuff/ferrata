#!/usr/bin/env bash
# What logos-modules-release-action's release.yml and rebuild-index.yml do, for the linux-amd64
# variant, run locally for both modules and publishing nothing:
#   export each module as its own repository (export.sh); `nix build .#lgx-portable`; `lgx verify`;
#   `lgx manifest --json`; the release sidecar (the action's fields); then the catalog's index.json
#   built by logos-modules-release-tool's index.py (the action's index builder) from the packages'
#   URLs, served by a local HTTP server.
# One variant: the action then skips `lgx merge` (it needs two). Nothing is signed (the base
# catalog's default, signing_mode: none).
#
#   modules/scripts/release-local.sh OUT_DIR      results also copied to modules/catalog/dry-run/
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
LGX_PACKAGE=github:logos-co/logos-package/4bdd9041ebfbd06c2b22134afdcd27fce8a43f3c   # 30 Oct 2026
TOOL_REPO=https://github.com/logos-co/logos-modules-release-tool
TOOL_REF=v1.3                                                                  # rebuild-index.yml's
PORT=${PORT:-18546}
MODULES="ferrata_attest_module ferrata_bridge_module"
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
mkdir -p "$1"
out=$(cd "$1" && pwd)
rm -rf "$out/src" "$out/dist" "$out/log"
mkdir -p "$out/src" "$out/dist" "$out/log"

# nix build with check.sh's retries on the build box's two container errors.
nixb() {
  local attempt log="$out/log/nix-attempt.log"
  for attempt in 1 2 3 4 5 6 7 8 9 10 11 12; do
    if [ "$(id -u)" = 0 ] && [ -d /homeless-shelter ]; then rm -rf /homeless-shelter; fi
    if nix build "$@" > >(tee "$log") 2>&1; then return 0; fi
    sleep 1
    grep -q -E 'reading a line: Input/output error|unexpected EOF reading a line|/homeless-shelter. exists' "$log" || return 1
    echo "release-local.sh: nix build stopped on a build-box error (attempt $attempt); retrying"
  done
  return 1
}

nixb "$LGX_PACKAGE#lgx" -o "$out/lgx-tool" > "$out/log/lgx-tool.log" 2>&1
export PATH="$out/lgx-tool/bin:$PATH"
lgx --version > "$out/log/lgx-version.txt" 2>&1 || lgx --help | head -3 > "$out/log/lgx-version.txt" 2>&1 || true

for m in $MODULES; do
  "$here/export.sh" "$m" "$out/src/$m" | tee "$out/log/export-$m.txt"
  version=$(jq -r .version "$out/src/$m/metadata.json")
  tag="$m-v$version"
  t0=$(date +%s)
  (cd "$out/src/$m" && nixb .#lgx-portable --print-out-paths --no-link > "$out/log/out-$m.txt") > "$out/log/build-$m.log" 2>&1
  echo "build $m $(( $(date +%s) - t0 )) s" >> "$out/log/times.txt"
  # nixb's output mixes Nix's progress with the printed out path: take the store path line.
  built=$(find "$(grep -E '^/nix/store/[^ ]+$' "$out/log/out-$m.txt" | tail -n 1)" -maxdepth 2 -name '*.lgx' | head -n 1)
  mkdir -p "$out/dist/$tag"
  lgx_file="$out/dist/$tag/$m-$version.lgx"
  cp "$built" "$lgx_file"
  chmod u+w "$lgx_file"
  lgx verify "$lgx_file" > "$out/log/verify-$m.txt" 2>&1
  manifest=$(lgx manifest "$lgx_file" --json)
  mname=$(jq -r .name <<<"$manifest")
  mver=$(jq -r .version <<<"$manifest")
  [ "$mname" = "$m" ] && [ "$mver" = "$version" ] || { echo "release-local.sh: manifest says $mname@$mver" >&2; exit 1; }
  sig=$(lgx signature "$lgx_file" || true)
  jq -n --arg releaseTag "$tag" --arg releasedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
     --arg sha "$(sha256sum "$lgx_file" | awk '{print $1}')" --argjson size "$(stat -c %s "$lgx_file")" \
     --arg rootHash "$(jq -r '.hashes.root // ""' <<<"$manifest")" --argjson manifest "$manifest" \
     --arg signature "$sig" \
     '{publisherRef: $releaseTag, releasedAt: $releasedAt, sha256: $sha, size: $size, rootHash: $rootHash,
       builtVariants: ["linux-amd64"], missingVariants: [], manifest: $manifest}
      + (if ($signature | length) > 0 then {signature: ($signature | fromjson)} else {} end)' > "$out/dist/$tag/sidecar.json"
done

# The index, as rebuild-index.yml builds it: index.py over every release's .lgx URL.
git clone -q --depth 1 --branch "$TOOL_REF" "$TOOL_REPO" "$out/_tool"
git -C "$out/_tool" rev-parse HEAD > "$out/log/tool-rev.txt"
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$out/dist" > "$out/log/http.log" 2>&1 &
http_pid=$!
trap 'kill $http_pid 2>/dev/null || true' EXIT
for _ in $(seq 1 100); do (echo > "/dev/tcp/127.0.0.1/$PORT") 2>/dev/null && break; sleep 0.1; done
find "$out/dist" -name '*.lgx' | sort | sed "s#^$out/dist#http://127.0.0.1:$PORT#" > "$out/urls.txt"
(cd "$out" && python3 -I "$out/_tool/index.py" build "$out/urls.txt" -o "$out/index.json" --icons-dir "") > "$out/log/index.log" 2>&1
jq '{ schemaVersion, repositoryName, packageCount: (.packages | length) }' "$out/index.json" | tee "$out/log/index-summary.json"

# The small results into the repository (no package binaries).
dry="$repo/modules/catalog/dry-run"
rm -rf "$dry"
mkdir -p "$dry"
cp "$out/index.json" "$out/urls.txt" "$dry/"
cp -r "$out/log" "$dry/log"
rm -f "$dry/log/nix-attempt.log"
for m in $MODULES; do
  tag="$m-v$(jq -r .version "$out/src/$m/metadata.json")"
  cp "$out/dist/$tag/sidecar.json" "$dry/$tag.sidecar.json"
done
echo "release-local.sh: done; $(jq '.packages | length' "$out/index.json") packages in index.json"
