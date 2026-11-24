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
install -d -m 755 "$WWW" "$WWW/wallet"
install -d -o ferrata -g ferrata -m 755 "$WWW/status"
# 1. the build page owns /index.html, /evidence.*, /build.json, /assets/{site.css,site.js,logo.svg,nojs.css}, /img/ and
#    /testers/ (the testers' start page): everything build.json's "files" lists
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
install -m 644 "$REPO"/idl/ferrata_attestation.idl.json "$REPO"/idl/ferrata_bridge.idl.json "$WWW/idl/"
[ -f /var/lib/ferrata/status/programs.json ] && install -m 644 /var/lib/ferrata/status/programs.json "$WWW/programs.json"
[ -f "$WWW/programs.json" ] || echo '{"status": "being deployed"}' > "$WWW/programs.json"
cp "$WWW/programs.json" "$WWW/status/programs.json"
# 5. the bundles' config: this network's URLs and, once deployed, its program and instance ids
FERRATA_WEB_STAGE="$STAGE" FERRATA_HERE="$HERE" FERRATA_RANGE_LIMIT="$RANGE_LIMIT" python3 -I - "$WWW" "$PRIMARY" <<'EOF'
import json, os, re, sys
www, primary = sys.argv[1:3]
range_limit = int(os.environ["FERRATA_RANGE_LIMIT"])
site = f"https://{primary}"
progs = json.load(open(os.path.join(www, "programs.json")))
p = os.path.join(www, "config.json")
if os.path.exists(p):
    c = json.load(open(p))
    c.update({"public_rpc": f"{site}/rpc", "site": site, "wallet": "/wallet/", "faucet": "/faucet",
              "faucet_api": "POST /faucet {\"account\": \"<base58 public account id>\", \"public_key\": \"<64 hex, for an account never used>\"}",
              "status_api": "/status/status.json", "range_limit": range_limit})
    c.setdefault("bedrock", {}).update({"anchored": True, "node_version": "0.3.5-rc.1",
              "channel_id": "0101010101010101010101010101010101010101010101010101010101010101"})
    accounts = {k: (v or {}).get("program_account") for k, v in (progs.get("programs") or {}).items()}
    key_of = {"ferrata_attestation": "attestation", "ferrata_bridge_b1": "bridge", "ferrata_consumer": "consumer",
              "ferrata_holdings": "holdings", "ferrata_msg": "msg"}
    for prog in c.get("programs", []):
        for name, acct in accounts.items():
            if key_of.get(name) == prog.get("key") and acct:
                prog["account"] = acct
    inst = (progs.get("instances") or {}).get("sepolia")
    if inst and inst.get("instance_id"):
        c["instances"] = [{"label": "Sepolia", "program": accounts.get("ferrata_attestation"), "instance_id": inst.get("instance_id"),
                           "account": inst.get("instance_account"), "chain_id": 11155111}]
    json.dump(c, open(p, "w"), indent=2)
m = os.path.join(www, "msg", "config.json")
msgp = (progs.get("programs") or {}).get("ferrata_msg", {}).get("program_account")
if os.path.exists(m) and msgp:
    c = json.load(open(m)); c["program"] = msgp; json.dump(c, open(m, "w"), indent=2)
a = os.path.join(www, "AGENTS.md")
if os.path.exists(a):
    t = open(a).read(); t2 = t.replace("{{HOST}}", primary)
    t2 = re.sub(r"at most \d+ blocks per call", f"at most {range_limit} blocks per call", t2)
    # The program streams' own AGENTS sections (messaging, names) and this host's limits, appended once each.
    stage = os.environ.get("FERRATA_WEB_STAGE", "")
    for sp in (os.path.join(stage, "msg/AGENTS-section.md"), os.path.join(stage, "names/AGENTS.md"),
               os.path.join(os.environ["FERRATA_HERE"], "host/www/AGENTS-limits.md")):
        if stage and os.path.exists(sp):
            body = open(sp).read().strip().replace("{{HOST}}", primary)
            first = next((l for l in body.splitlines() if l.strip()), "")
            if first and first not in t2:
                t2 = t2.rstrip() + "\n\n" + body + "\n"
    if t2 != t: open(a, "w").write(t2)
w = os.path.join(www, "wallet", "config.json")
if os.path.exists(w):
    c = json.load(open(w))
    c.update({"network": "Ferrata LEZ testnet (rc3)", "rpc": "/rpc", "faucet": "/faucet", "explorer": "/explorer/"})
    a = (progs.get("programs") or {}).get("ferrata_attestation", {}).get("program_account")
    inst = (progs.get("instances") or {}).get("sepolia")
    if a:
        c["attestation"] = {"program": a, "instances": ([{"id": "0x" + inst["instance_id"].removeprefix("0x"), "chainId": 11155111,
                            "name": "Sepolia"}] if inst and inst.get("instance_id") else [])}
    b = (progs.get("programs") or {}).get("ferrata_bridge_b1", {}).get("program_account")
    if b:
        c.setdefault("bridge", {})["program"] = b
    # The encrypted inbox and names panels turn on when their program is set.
    for key, prog in (("msg", "ferrata_msg"), ("names", "ferrata_names")):
        acct = (progs.get("programs") or {}).get(prog, {}).get("program_account")
        if acct:
            c.setdefault(key, {})["program"] = acct
    json.dump(c, open(w, "w"), indent=2)
EOF
# 6. Caddy
sed -e "s/{{SITES}}/$SITES/" -e "s/{{HOST}}/$SSLIP/g" "$HERE/host/Caddyfile" > /etc/caddy/Caddyfile.new
caddy validate --config /etc/caddy/Caddyfile.new --adapter caddyfile > /dev/null 2>&1 || { caddy validate --config /etc/caddy/Caddyfile.new --adapter caddyfile; exit 1; }
mv /etc/caddy/Caddyfile.new /etc/caddy/Caddyfile
systemctl restart caddy   # admin API off: reload would fail
echo "== site at https://$PRIMARY/ (also https://$SSLIP/)"
