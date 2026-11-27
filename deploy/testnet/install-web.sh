#!/usr/bin/env bash
# Put the site together on the host (root): the build page at /, the explorer bundle's pages beside it, the wallet at
# /wallet/, this host's own pages; fill the bundles' config with the network's URLs and deployed ids; Caddy.
#
#   install-web.sh WEB_TGZ    a tarball of build/, explorer/, wallet/ (and msg/, names/): the static sites built
#                             elsewhere in the repository (README: Artifacts); missing parts are left out
#
# The public names come from /etc/ferrata/site.env (FERRATA_DOMAIN=..., optional) and the instance's Elastic IP.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
TGZ="${1:?web tarball}"
WWW=/srv/ferrata/www
STAGE=/opt/ferrata/web-stage
[ -f /etc/ferrata/site.env ] && . /etc/ferrata/site.env
tok="$(curl -sS -X PUT -H 'X-aws-ec2-metadata-token-ttl-seconds: 60' http://169.254.169.254/latest/api/token)"
ip="$(curl -sS -H "X-aws-ec2-metadata-token: $tok" http://169.254.169.254/latest/meta-data/public-ipv4)"
SSLIP="${ip//./-}.sslip.io"
PRIMARY="${FERRATA_DOMAIN:-$SSLIP}"
SITES="$SSLIP"; [ -n "${FERRATA_DOMAIN:-}" ] && SITES="$FERRATA_DOMAIN, $SSLIP"
RANGE_LIMIT=50   # the gateway's GATEWAY_MAX_RANGE: the explorer pages getBlockRange by it

rm -rf "$STAGE"; mkdir -p "$STAGE"; tar -xzf "$TGZ" -C "$STAGE"
# The wallet's and the messaging module's config as this run finds them, the host's hand edits included (deposits
# closed with lezReady false), copied before step 3 replaces them with the bundle's raw ones: web-config.py keeps their
# values first, where the bundle leaves them empty.
install -d -m 755 "$STAGE/live-configs"
for f in wallet msg; do [ ! -f "$WWW/$f/config.json" ] || cp "$WWW/$f/config.json" "$STAGE/live-configs/$f-config.json"; done
# The same two as served after the last run that succeeded, kept outside the stage and written below only once step 5
# succeeds: they fill only what the live copies lack or leave null, the raw configs a run stopped half way leaves in
# place. The first run copies them.
SERVED=/var/lib/ferrata/web-served
install -d -m 755 "$SERVED"
for f in wallet msg; do [ -f "$SERVED/$f-config.json" ] || [ ! -f "$WWW/$f/config.json" ] || cp "$WWW/$f/config.json" "$SERVED/$f-config.json"; done
install -d -m 755 "$WWW" "$WWW/wallet"
install -d -o ferrata -g ferrata -m 755 "$WWW/status"
# 1. the build page owns /index.html, /evidence.*, /build.json, /assets/{site.css,site.js,logo.svg,nojs.css}, /img/ and
#    /testers/ (the testers' start page) and /llms.txt: everything build.json's "files" lists
cp -r "$STAGE/build/." "$WWW/"
# 2. the explorer bundle: its landing becomes /network.html, the rest keeps its paths (/explorer/, /status/index.html,
#    /agents/, /registry/, /assets/*.js, /idl/, /config.json, /AGENTS.md)
if [ -d "$STAGE/explorer" ]; then
  cp "$STAGE/explorer/index.html" "$WWW/network.html"
  ( cd "$STAGE/explorer" && find . -type f ! -path ./index.html ! -path './test/*' ! -name package.json -print0 \
      | xargs -0 -I{} install -D -m 644 {} "$WWW/{}" )
fi
# 3. the wallet (only its served files)
if [ -d "$STAGE/wallet" ]; then
  rm -rf "$WWW/wallet"; install -d -m 755 "$WWW/wallet"
  ( cd "$STAGE/wallet" && find . -type f ! -path './test/*' ! -name README.md ! -name build.sh ! -name 'package*.json' \
      ! -name entry.js -print0 | xargs -0 -I{} install -D -m 644 {} "$WWW/wallet/{}" )
fi
# 3b. the messaging app (the messages app's deploy/testnet/web/msg): /msg/, beside /wallet/ whose lez.js it imports
if [ -d "$STAGE/msg" ]; then
  rm -rf "$WWW/msg"; install -d -m 755 "$WWW/msg"
  ( cd "$STAGE/msg" && find . -type f ! -path './test/*' ! -name README.md ! -name 'package*.json' -print0 \
      | xargs -0 -I{} install -D -m 644 {} "$WWW/msg/{}" )
fi
# 3c. the names resolver (names/js) at /names/, for the wallet and agents
if [ -d "$STAGE/names/js" ]; then
  rm -rf "$WWW/names"; install -d -m 755 "$WWW/names"
  ( cd "$STAGE/names/js" && find . -type f ! -path './test/*' \( -name '*.js' -o -name '*.mjs' \) -print0 \
      | xargs -0 -I{} install -D -m 644 {} "$WWW/names/{}" )
fi
# 4. this host's own pages: the network health page (status.json), style
install -m 644 "$HERE/host/www/status.html" "$WWW/health.html"; install -m 644 "$HERE/host/www/health.js" "$WWW/health.js"
install -m 644 "$HERE/host/www/style.css" "$HERE/host/www/robots.txt" "$WWW/"
install -d -m 755 "$WWW/idl"
# The test network runs ferrata_bridge_b1: its frozen interface is the one the explorer decodes with (idl/deployed/).
install -m 644 "$REPO"/idl/ferrata_attestation.idl.json "$WWW/idl/"
install -m 644 "$REPO"/idl/deployed/ferrata_bridge_b1.idl.json "$WWW/idl/ferrata_bridge.idl.json"
[ -f /var/lib/ferrata/status/programs.json ] && install -m 644 /var/lib/ferrata/status/programs.json "$WWW/programs.json"
[ -f "$WWW/programs.json" ] || echo '{"status": "being deployed"}' > "$WWW/programs.json"
cp "$WWW/programs.json" "$WWW/status/programs.json"
# 5. the bundles' config: this network's URLs and, once deployed, its program and instance ids (messaging and names:
#    their second versions where deployed); /AGENTS.md with the program streams' sections and this host's limits
python3 -I "$HERE/web-config.py" "$WWW" "$PRIMARY" "$HERE" "$RANGE_LIMIT" "$STAGE" "$SERVED"
for f in wallet msg; do
  [ ! -f "$WWW/$f/config.json" ] || { cp "$WWW/$f/config.json" "$SERVED/.$f-config.json" && mv "$SERVED/.$f-config.json" "$SERVED/$f-config.json"; }
done
# 6. Caddy
sed -e "s/{{SITES}}/$SITES/" -e "s/{{HOST}}/$SSLIP/g" "$HERE/host/Caddyfile" > /etc/caddy/Caddyfile.new
caddy validate --config /etc/caddy/Caddyfile.new --adapter caddyfile > /dev/null 2>&1 || { caddy validate --config /etc/caddy/Caddyfile.new --adapter caddyfile; exit 1; }
mv /etc/caddy/Caddyfile.new /etc/caddy/Caddyfile
systemctl restart caddy   # admin API off: reload would fail
echo "== site at https://$PRIMARY/ (also https://$SSLIP/)"
