#!/usr/bin/env bash
# Install and start the testnet's services on the provisioned host (provision.sh install; root). Idempotent: keys,
# the node's state, the sequencer's home and the faucet's ledger are made once and never replaced.
#
#   FERRATA_HOSTNAME  the public name (default: the Elastic IP's a-b-c-d.sslip.io, from the instance metadata)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OPT=/opt/ferrata
DATA=/var/lib/ferrata
LEZ="$OPT/src/upstream/lez-v0.4.0-rc3"
LOG=/var/log/ferrata
WWW=/srv/ferrata/www
# The node's funded wallet key that LEZ's local stack gives the sequencer (bedrock/node-config.yaml `funding_pk`).
FUNDING_PK=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26
as_ferrata() { runuser -u ferrata -- "$@"; }

if [ -z "${FERRATA_HOSTNAME:-}" ]; then
  tok="$(curl -sS -X PUT -H 'X-aws-ec2-metadata-token-ttl-seconds: 60' http://169.254.169.254/latest/api/token)"
  ip="$(curl -sS -H "X-aws-ec2-metadata-token: $tok" http://169.254.169.254/latest/meta-data/public-ipv4)"
  FERRATA_HOSTNAME="${ip//./-}.sslip.io"
fi
echo "== hostname $FERRATA_HOSTNAME"

install -d -o ferrata -g ferrata -m 750 "$DATA"/{node,sequencer,indexer,keys,day1,updater,monitor,status}
install -d -o ferrata -g ferrata -m 750 "$LOG"
# Every log a unit appends to, owned by the user logrotate rotates them as (`su ferrata ferrata`, host/logrotate):
# systemd opens an `append:` file as root and would create it root-owned, which logrotate as ferrata cannot
# truncate (copytruncate), so it would never be rotated. Mode 644: the CloudWatch agent reads them.
LOGS="node sequencer sequencer-run indexer explorer gateway updater monitor status topup bedrock-split
      bridge-attestor bridge-import bridge-monitor bridge-prover bridge-relayer bridge-lez-init bridge-lez-import gasdrop
      bridge-checkpoint"
for f in $LOGS; do [ -e "$LOG/$f.log" ] || install -o ferrata -g ferrata -m 644 /dev/null "$LOG/$f.log"; done
chown ferrata:ferrata "$LOG"/*.log
install -d -m 755 /etc/ferrata "$WWW"
install -d -o ferrata -g ferrata -m 755 "$WWW/status"

# ---- binaries and scripts -------------------------------------------------------------------------------------------
install -m 755 "$HERE"/host/bin/ferrata-* "$OPT/bin/"   # (not a __pycache__ a local test run may leave)
if [ -f "$OPT/bin/explorer_service" ]; then
  install -d -m 755 "$OPT/explorer"
  install -m 755 "$OPT/bin/explorer_service" "$OPT/explorer/explorer_service"
  rm -rf "$OPT/explorer/site"; cp -r "$OPT/explorer-site" "$OPT/explorer/site"
  install -m 644 "$LEZ/lez/explorer_service/Cargo.toml" "$OPT/explorer/Cargo.toml"
fi

# ---- keys: made here, mode 600, never printed ----------------------------------------------------------------------
for k in faucet operator; do
  if [ ! -f "$DATA/keys/$k/payer.key" ]; then
    as_ferrata "$OPT/bin/ferrata-day1" keygen --keys "$DATA/keys/$k" > /dev/null
  fi
done
# The faucet's key and ledger belong to the gateway's own user ; the other services cannot read them.
id ferrata-gw >/dev/null 2>&1 || useradd --system --home-dir /nonexistent --no-create-home --shell /usr/sbin/nologin ferrata-gw
install -d -o ferrata-gw -g ferrata-gw -m 700 "$DATA/faucet"
chown -R ferrata-gw:ferrata-gw "$DATA/keys/faucet" "$DATA/faucet"; chmod 700 "$DATA/keys/faucet"
pubkey() { python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["payer_public_key"])' "$DATA/keys/$1/accounts.json"; }
account() { python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["payer"])' "$DATA/keys/$1/accounts.json"; }

# ---- the Logos Blockchain node: LEZ's local configuration, listening on loopback only ------------------------------
if [ ! -f "$DATA/node/node-config.yaml" ]; then
  install -o ferrata -g ferrata -m 640 "$LEZ/bedrock/node-config.yaml" "$LEZ/bedrock/deployment-settings.yaml" "$DATA/node/"
  sed -i -e 's#listen_address: 0.0.0.0:18080#listen_address: 127.0.0.1:18080#' \
         -e 's#^    swarm:$#    swarm:\n      host: 127.0.0.1#' "$DATA/node/node-config.yaml"
fi

# ---- the sequencer: LEZ's local.json, with this network's genesis and fee reserve ---------------------------------
python3 -I - "$LEZ/lez/sequencer/service/networks/local.json" /etc/ferrata/sequencer_config.json \
  "$(pubkey faucet)" "$(pubkey operator)" <<'EOF'
import json, sys
src, dst, faucet_pk, operator_pk = sys.argv[1:5]
c = json.load(open(src))
c["home"] = "/var/lib/ferrata/sequencer"
c["block_create_timeout"] = "10s"           # LEZ's testnet.json value; divides the node's 240 s epoch
c["bedrock_config"]["node_url"] = "http://127.0.0.1:18080"
# The reserve every inscription is funded with above the node's price (LEZ default 12 %). The node raises its storage
# price at an epoch's start (up to 7x seen on rc3); an inscription whose reserve is below the new price is never mined
# and the zone stalls for ~20 minutes (LEZ issue 1109). 900 % covers a tenfold step.
c["bedrock_config"]["priority_fee_percent"] = 900
c["gossip"] = {"listen_addr": "/ip4/127.0.0.1/udp/0/quic-v1", "bootstrap_peers": []}
c["metrics_listen_address"] = "127.0.0.1:9000"
# Genesis: this network's own accounts (keys made on the host), not LEZ's published local test keys.
c["genesis"]["actions"] = [
    {"supply_account": {"recipient": {"identified": {"key": faucet_pk}}, "balance": 1_000_000_000_000_000}},
    {"supply_account": {"recipient": {"identified": {"key": operator_pk}}, "balance": 1_000_000_000_000_000}},
]
json.dump(c, open(dst, "w"), indent=2)
open(dst, "a").write("\n")
EOF
chmod 644 /etc/ferrata/sequencer_config.json
if [ ! -f "$DATA/sequencer/sequencer_config.json" ]; then
  as_ferrata "$OPT/bin/sequencer_service" setup "$DATA/sequencer" --config /etc/ferrata/sequencer_config.json \
    --funding-public-key "$FUNDING_PK" --create-channel > "$LOG/sequencer-setup.log" 2>&1
fi

# ---- indexer --------------------------------------------------------------------------------------------------------
cat > /etc/ferrata/indexer_config.json <<'EOF'
{
  "consensus_info_polling_interval": "1s",
  "bedrock_config": { "addr": "http://127.0.0.1:18080" },
  "channel_id": "0101010101010101010101010101010101010101010101010101010101010101",
  "allow_chain_reset": false,
  "event_filter": "archival"
}
EOF

# ---- gateway --------------------------------------------------------------------------------------------------------
[ -f /etc/ferrata/gateway.env ] || : > /etc/ferrata/gateway.env

# ---- site and Caddy: the build page, the explorer and wallet bundles, this host's pages (install-web.sh) --------
if [ -f /opt/ferrata/web.tar.gz ]; then
  bash "$HERE/install-web.sh" /opt/ferrata/web.tar.gz
else
  tmpd="$(mktemp -d /opt/ferrata/web-empty.XXXX)"; mkdir -p "$tmpd/build"
  printf '%s\n' '<!doctype html><meta charset="utf-8"><title>Ferrata LEZ testnet</title><p>The network is starting. <a href="/health">Health</a></p>' > "$tmpd/build/index.html"
  tar -czf "$tmpd.tgz" -C "$tmpd" build && bash "$HERE/install-web.sh" "$tmpd.tgz"; rm -rf "$tmpd" "$tmpd.tgz"
fi
install -m 644 "$HERE/host/logrotate" /etc/logrotate.d/ferrata
install -m 644 "$HERE/host/cloudwatch-agent.json" /opt/aws/amazon-cloudwatch-agent/etc/amazon-cloudwatch-agent.json
/opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-ctl -a fetch-config -m ec2 -s \
  -c file:/opt/aws/amazon-cloudwatch-agent/etc/amazon-cloudwatch-agent.json > /dev/null

# ---- units ----------------------------------------------------------------------------------------------------------
install -m 644 "$HERE"/host/systemd/* /etc/systemd/system/
systemctl daemon-reload
# The node's API for its own users only, before the node first starts (host/bin/ferrata-node-api-guard).
systemctl enable --now ferrata-node-api-guard.service
systemctl enable --now ferrata-node.service
for _ in $(seq 1 120); do curl -fsS -m 2 http://127.0.0.1:18080/cryptarchia/info > /dev/null 2>&1 && break; sleep 1; done
# A fresh chain: fund the sequencer's Bedrock key once from the SDP note and wait until that note is the leader's aged
# stake again, before the sequencer spends anything (ferrata-bedrock-split; never spending the stake later).
if [ ! -f "$DATA/sequencer/.bedrock-split-done" ]; then
  as_ferrata python3 -I "$OPT/bin/ferrata-bedrock-split" | tee -a "$LOG/bedrock-split.log"
  [ "${PIPESTATUS[0]}" = 0 ] && touch "$DATA/sequencer/.bedrock-split-done"
fi
systemctl enable --now ferrata-sequencer.service ferrata-topup.timer ferrata-status.timer
# A fresh chain: LEZ's two published test accounts (funded by rc3's pre-genesis state, keys in LEZ's source) are swept
# into the faucet before the public gateway opens (ferrata-sweep-published). ferrata-status alarms if they
# hold funds again.
if [ ! -f "$DATA/sequencer/.published-swept" ]; then
  for _ in $(seq 1 120); do
    h="$(curl -fsS -m 3 http://127.0.0.1:3040 -H 'content-type: application/json' \
          -d '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p')"
    [ -n "$h" ] && [ "$h" -ge 2 ] && break; sleep 5
  done
  if bash "$OPT/bin/ferrata-sweep-published" | tee -a "$LOG/sweep.log"; then touch "$DATA/sequencer/.published-swept"; fi
fi
systemctl enable --now ferrata-gateway.service
if [ -x "$OPT/bin/indexer_service" ]; then systemctl enable --now ferrata-indexer.service; fi
if [ -x "$OPT/explorer/explorer_service" ]; then systemctl enable --now ferrata-explorer.service; fi
# Caddy's admin API is off (no local process can reconfigure it): a restart, not a reload, applies a new configuration.
systemctl enable caddy; systemctl restart caddy
echo "== installed"
