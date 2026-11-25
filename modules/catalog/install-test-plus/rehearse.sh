#!/usr/bin/env bash
# The catalog update's rehearsal, before the owner's release (Linux x86-64 with Nix; the Modal build box):
#   build     what logos-modules-release-action v1.8 does for the linux-amd64 variant of ferrata_bridge_module and
#             ferrata_bridge_ui (modules/scripts/release-local.sh: export.sh, `nix build .#lgx-portable` in the exported
#             repository, lgx verify, lgx manifest, the sidecar, index.py), its small outputs to ../dry-run-plus/
#   sign      the action's inline signing step on both packages (`lgx sign --key release --keys-dir`, then lgx verify)
#             with a throwaway key made here, their sidecars as the action writes them; the bridge module's built
#             contract (its package's assets/lidl/) compared with the mini-app's copy (lidl/, and in its package); the
#             published ferrata_bridge_module 0.1.0 downloaded from landclimber/ferrata-modules, so that the catalog
#             offers two versions
#   catalog   the three packages served over HTTPS on 127.0.0.1 as a catalog laid out as the published one (logos-repo.json
#             with the template's identity and the throwaway DID, index.json by index.py v1.3 with its icons)
#   install   box-install.sh against it (headless Logos Core, signatures required), its record to RECORD/
#   basecamp  basecamp-open.sh on the two signed packages (Basecamp's portable standalone mode, the mini-app's probe),
#             its record to RECORD/basecamp/
#
#   modules/catalog/install-test-plus/rehearse.sh WORK RECORD [PART...]      (default: every part, in that order)
# WORK: a scratch directory (outside the repository). RECORD: the record's directory. On a copy of the repository
# without git metadata (the build box), set FERRATA_COMMIT and SOURCE_DATE_EPOCH to the source commit's (export.sh).
# The portable Basecamp loads OpenGL, xkbcommon, fontconfig, harfbuzz, fribidi and ALSA from the host (on Ubuntu:
# libgl1 libegl1 libopengl0 libglx-mesa0 libxkbcommon0 libfontconfig1 libharfbuzz0b libfribidi0 libasound2t64 ...).
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
[ $# -ge 2 ] || { echo "usage: rehearse.sh WORK RECORD [PART...]" >&2; exit 2; }
mkdir -p "$1" "$2"; B=$(cd "$1" && pwd); R=$(cd "$2" && pwd); shift 2
parts=${*:-build sign catalog install basecamp}
has() { case " $parts " in *" $1 "*) return 0 ;; esac; return 1; }
MODS="ferrata_bridge_module ferrata_bridge_ui"
PORT=18547
PUBLISHED=https://github.com/landclimber/ferrata-modules/releases/download/ferrata_bridge_module-v0.1.0
cd "$repo"
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
echo "rehearse.sh: parts: $parts; $(nix --version 2>&1)"

if has build; then
  t0=$(date +%s)
  MODULES="$MODS" DRY_RUN_DIR="$repo/modules/catalog/dry-run-plus" bash modules/scripts/release-local.sh "$B/rel" > "$B/release-local.log" 2>&1
  echo "release-local.sh: exit $? in $(( $(date +%s) - t0 )) s ($(tail -n 1 "$B/release-local.log"))"
fi
LGX=$B/rel/lgx-tool/bin/lgx
export PATH="$B/rel/lgx-tool/bin:$PATH"   # index.py runs lgx

if has sign; then
  rm -rf "$B/key" "$B/www" && mkdir -p "$B/key" "$B/www"
  "$LGX" keygen --name release --output-dir "$B/key" > "$B/keygen.log" 2>&1
  python3 -I modules/scripts/jwk_did.py "$B/key/release.jwk" > "$B/did.txt" || exit 1
  echo "throwaway DID: $(cat "$B/did.txt")"
  for m in $MODS; do
    v=$(jq -r .version "modules/$m/metadata.json"); tag=$m-v$v; f=$B/www/$tag/$m-$v.lgx
    mkdir -p "$B/www/$tag" && cp "$B/rel/dist/$tag/$m-$v.lgx" "$f" && chmod u+w "$f"
    "$LGX" sign "$f" --key release --keys-dir "$B/key" > "$B/sign-$m.log" 2>&1 && "$LGX" verify "$f" >> "$B/sign-$m.log" 2>&1
    echo "$m $v: lgx sign and lgx verify exit $?"
    manifest=$("$LGX" manifest "$f" --json); sig=$("$LGX" signature "$f")
    jq -n --arg releaseTag "$tag" --arg releasedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
       --arg sha "$(sha256sum "$f" | awk '{print $1}')" --argjson size "$(stat -c %s "$f")" \
       --arg rootHash "$(jq -r '.hashes.root // ""' <<<"$manifest")" --argjson manifest "$manifest" --arg signature "$sig" \
       '{publisherRef: $releaseTag, releasedAt: $releasedAt, sha256: $sha, size: $size, rootHash: $rootHash,
         builtVariants: ["linux-amd64"], missingVariants: [], manifest: $manifest}
        + (if ($signature | length) > 0 then {signature: ($signature | fromjson)} else {} end)' > "$B/www/$tag/sidecar.json"
    echo "  type $(jq -r .manifest.type "$B/www/$tag/sidecar.json"), rootHash $(jq -r .rootHash "$B/www/$tag/sidecar.json"), size $(jq -r .size "$B/www/$tag/sidecar.json"), signer $(jq -r '.signature.did // "none"' "$B/www/$tag/sidecar.json" | cut -c1-24)..."
    tar -tzf "$f" > "$B/files-$m.txt"
    p=$(grep -E '(^|/)ferrata_bridge_module\.lidl$' "$B/files-$m.txt" | head -n 1)
    if [ -n "$p" ] && tar -xzf "$f" -O "$p" | cmp -s - modules/ferrata_bridge_ui/lidl/ferrata_bridge_module.lidl; then
      echo "  contract $p: the same as modules/ferrata_bridge_ui/lidl/ferrata_bridge_module.lidl"
    else echo "  contract ${p:-none}: DIFFERS from modules/ferrata_bridge_ui/lidl/ferrata_bridge_module.lidl"; fi
  done
  mkdir -p "$B/www/ferrata_bridge_module-v0.1.0"
  for a in ferrata_bridge_module-0.1.0.lgx sidecar.json; do
    curl -fsSL --retry 3 -o "$B/www/ferrata_bridge_module-v0.1.0/$a" "$PUBLISHED/$a" || echo "  download $a failed"
  done
  echo "published ferrata_bridge_module 0.1.0: sha256 $(sha256sum "$B/www/ferrata_bridge_module-v0.1.0/ferrata_bridge_module-0.1.0.lgx" | cut -c1-64), its sidecar's $(jq -r .sha256 "$B/www/ferrata_bridge_module-v0.1.0/sidecar.json")"
fi

if has catalog || has install; then
  openssl req -x509 -newkey rsa:2048 -nodes -keyout "$B/key.pem" -out "$B/cert.pem" -days 2 \
    -subj "/CN=127.0.0.1" -addext "subjectAltName=IP:127.0.0.1" > "$B/openssl.log" 2>&1
  cat /etc/ssl/certs/ca-certificates.crt "$B/cert.pem" > "$B/ca-bundle.pem"
  export SSL_CERT_FILE=$B/ca-bundle.pem
  python3 -I -c '
import functools, http.server, ssl, sys
port, d, cert, key = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
h = functools.partial(http.server.SimpleHTTPRequestHandler, directory=d)
s = http.server.ThreadingHTTPServer(("127.0.0.1", port), h)
c = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); c.load_cert_chain(cert, key)
s.socket = c.wrap_socket(s.socket, server_side=True)
s.serve_forever()' "$PORT" "$B/www" "$B/cert.pem" "$B/key.pem" > "$B/http.log" 2>&1 &
  srv=$!
  trap 'kill $srv 2> /dev/null' EXIT
  for _ in $(seq 1 100); do (echo > "/dev/tcp/127.0.0.1/$PORT") 2> /dev/null && break; sleep 0.1; done
fi

if has catalog; then
  jq --arg u "https://127.0.0.1:$PORT/index.json" --arg did "$(cat "$B/did.txt")" \
    '.indexUrl = $u | .trustedSigners = [{did: $did, name: "Ferrata module releases"}]' \
    modules/catalog/logos-repo.json > "$B/www/logos-repo.json"
  find "$B/www" -name '*.lgx' | sort | sed "s#^$B/www#https://127.0.0.1:$PORT#" > "$B/urls.txt"
  (cd "$B/www" && python3 -I "$B/rel/_tool/index.py" build "$B/urls.txt" -o "$B/www/index.json") > "$B/index.log" 2>&1
  echo "index.py build: exit $?, $(jq -c '[.packages[] | {name, versions: [.versions[].manifest.version], icon: (.icon.path // .versions[0].icon.path // null)}]' "$B/www/index.json" 2> /dev/null)"
  python3 -I "$B/rel/_tool/index.py" validate-repo "$B/www/logos-repo.json" > "$B/validate-repo.log" 2>&1
  echo "index.py validate-repo: exit $?"
fi

if has install; then
  args=()
  for m in $MODS; do
    v=$(jq -r .version "modules/$m/metadata.json"); tag=$m-v$v
    args+=("$tag/$m-$v.lgx/$(jq -r .rootHash "$B/www/$tag/sidecar.json")")
  done
  t0=$(date +%s)
  CATALOG_URL=https://127.0.0.1:$PORT/logos-repo.json PKG_BASE=https://127.0.0.1:$PORT REC=$R \
    bash modules/catalog/install-test-plus/box-install.sh local-rehearsal ferrata-modules "$(cat "$B/did.txt")" "${args[@]}" > "$B/install.log" 2>&1
  echo "box-install.sh: exit $? in $(( $(date +%s) - t0 )) s ($(tail -n 1 "$B/install.log"))"
fi

if has basecamp; then
  t0=$(date +%s)
  bash modules/catalog/install-test-plus/basecamp-open.sh "$R/basecamp" \
    "$B/www/ferrata_bridge_ui-v0.1.0/ferrata_bridge_ui-0.1.0.lgx" "$B/www/ferrata_bridge_module-v0.1.1/ferrata_bridge_module-0.1.1.lgx" > "$B/basecamp.log" 2>&1
  echo "basecamp-open.sh: exit $? in $(( $(date +%s) - t0 )) s ($(grep -E '[0-9]+ passed, [0-9]+ failed' "$R/basecamp/probe.log" | tail -n 1 | tr -d '\033'))"
fi
# The small outputs beside the record: the sidecars, the index, the logos-repo.json served, the package listings.
mkdir -p "$R/catalog"
(cd "$B/www" 2> /dev/null && find . -name '*.json' -exec cp --parents {} "$R/catalog/" \;)
cp "$B"/files-*.txt "$R/catalog/" 2> /dev/null
# No machine path in the records: the repository as <repo>, WORK as <work>, the directory above the repository as
# <top> (box-install.sh and basecamp-open.sh have already written theirs with <repo> and <top>).
top=$(cd "$repo/.." && pwd)
case "$B" in "$top"/*) wb="<top>${B#"$top"}" ;; *) wb=$B ;; esac
grep -rlF -e "$top" -e "$wb" "$R" "$repo/modules/catalog/dry-run-plus" 2> /dev/null | while read -r f; do
  sed -i -e "s#$repo#<repo>#g" -e "s#$B#<work>#g" -e "s#$wb#<work>#g" -e "s#$top#<top>#g" "$f"
done
echo "rehearse.sh: done"
