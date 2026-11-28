#!/usr/bin/env bash
# Deploy Ferrata's programs on the testnet with the day-one kit (lez/testnet04/day1.sh), on the host, as root:
#
#   deploy-programs.sh plan     day1.sh discover + plan against the local sequencer (fresh keys, made here, mode 600)
#   deploy-programs.sh fund     the operator's genesis account pays the plan's payer and daemon key (ferrata-day1 transfer)
#   deploy-programs.sh run      FERRATA_TESTNET_SEND=yes day1.sh run: (a) the attestation program 0x816d1fef…, (b) Init of
#                               a Sepolia instance, (c) the updater, (d) one attestation, (e) the bridge program at B1
#                               0x847a4336…; the kit leaves its updater detached (setsid), which a stop/start of the
#                               host does not bring back: run `handover` next, before any resize
#   deploy-programs.sh publish  programs.json (public ids only) for the site, AGENTS.md and the status page
#   deploy-programs.sh handover the kit's detached updater handed to systemd (ferrata-updater), attest-monitor started
#                               (ferrata-monitor, /etc/ferrata/attest.env), programs.json published, the site's config filled
#   deploy-programs.sh verify   day1.sh verify
#
# Everything the kit writes stays in /var/lib/ferrata/day1 (keys mode 600, never printed).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
. "$HERE/lez-tag.sh"
DATA=/var/lib/ferrata
URL=http://127.0.0.1:3040
export FERRATA_DAY1_BIN=/opt/ferrata/bin FERRATA_DAY1_PROGRAMS=/opt/ferrata/programs/day1
export FERRATA_DAY1_STATE=$DATA/day1/state FERRATA_DAY1_KEYS=$DATA/day1/keys FERRATA_DAY1_BRIDGE=1
export PATH=/opt/ferrata/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin XDG_CACHE_HOME=/opt/ferrata/cache RISC0_SERVER_PATH=/opt/ferrata/bin/r0vm
LOG=/var/log/ferrata/deploy-programs.log
as_ferrata() { runuser -u ferrata -- env PATH="$PATH" XDG_CACHE_HOME="$XDG_CACHE_HOME" RISC0_SERVER_PATH="$RISC0_SERVER_PATH" \
  FERRATA_DAY1_BIN="$FERRATA_DAY1_BIN" FERRATA_DAY1_PROGRAMS="$FERRATA_DAY1_PROGRAMS" FERRATA_DAY1_STATE="$FERRATA_DAY1_STATE" \
  FERRATA_DAY1_KEYS="$FERRATA_DAY1_KEYS" FERRATA_DAY1_BRIDGE=1 HOME="$DATA" "$@"; }
D="$REPO/lez/testnet04/day1.sh"

programs_dir() { # the kit's names: ferrata_lez.bin, ferrata_bridge.bin (the B1 build, sha256 checked by day1.sh)
  install -d -m 755 "$FERRATA_DAY1_PROGRAMS"
  install -m 644 /opt/ferrata/programs/ferrata_lez.bin "$FERRATA_DAY1_PROGRAMS/ferrata_lez.bin"
  install -m 644 /opt/ferrata/programs/ferrata_bridge_b1.bin "$FERRATA_DAY1_PROGRAMS/ferrata_bridge.bin"
  install -d -o ferrata -g ferrata -m 750 "$DATA/day1"
  # The kit reads its repository; the host's copy is root's: give ferrata a readable copy of what it runs.
  chmod -R o+rX "$REPO/lez/testnet04" "$REPO/lez-v03/scripts" 2>/dev/null || true
}

publish() {
  as_ferrata /opt/ferrata/bin/ferrata-day1 inspect --keys "$FERRATA_DAY1_KEYS" --state "$FERRATA_DAY1_STATE" --rpc "$URL" \
    --bin "$FERRATA_DAY1_PROGRAMS/ferrata_lez.bin" > "$FERRATA_DAY1_STATE/inspect.json" 2>> "$LOG" || true
  python3 -I - "$FERRATA_DAY1_KEYS" "$FERRATA_DAY1_STATE" "$DATA/status/programs.json" "$LEZ_TAG" <<'EOF'
import json, os, sys
keys, state, out, tag = sys.argv[1:5]
acc = json.load(open(os.path.join(keys, "accounts.json")))
d = {"network": "ferrata-lez-testnet", "lez": tag, "programs": {}, "instances": {}}
prog = acc.get("program") or {}
brid = acc.get("bridge") or {}
def pid(x):
    return x.get("header") or x.get("program") or x.get("account")
if prog:
    d["programs"]["ferrata_attestation"] = {"program_account": pid(prog), "image_id": "0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df",
        "binary_sha256": "ddfde69bea89a5b882f55266fcb4b7c887df7debc1c9d4972d8ebea5a72eaad9", "immutable": True,
        "note": "attestation program (RFP-021), docs/image-ids.md ferrata_lez"}
if brid:
    d["programs"]["ferrata_bridge_b1"] = {"program_account": pid(brid), "image_id": "0x847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515",
        "binary_sha256": "537cbe7a4c6900419b54e556b9741a9e37d86adf53f5e65683aa2b22c8e9a036", "immutable": True,
        "note": "bridge program at milestone B1 (RFP-022), docs/image-ids.md ferrata_bridge_b1; not configured with a vault"}
p = os.path.join(keys, "instance.json")
if os.path.exists(p):
    inst = json.load(open(p))
    d["instances"]["sepolia"] = {k: inst[k] for k in inst if k in ("instance_id", "program", "preset", "layout", "checkpoint", "checkpoint_slot", "chain_id", "ws_end_ms")}
    ins = os.path.join(state, "inspect.json")
    if os.path.exists(ins):
        i = json.load(open(ins)).get("instance") or {}
        d["instances"]["sepolia"]["instance_account"] = i.get("account")
    d["instances"]["sepolia"]["note"] = "follows Sepolia (ferrata-updater), watched by attest-monitor"
extra = os.path.join(os.path.dirname(out), "extra-programs.json")
if os.path.exists(extra):
    for name, v in json.load(open(extra)).items():
        d["programs"][name] = v
d["raw_accounts"] = {k: v for k, v in acc.items() if k in ("program", "bridge")}
os.makedirs(os.path.dirname(out), exist_ok=True)
json.dump(d, open(out, "w"), indent=1)
print(json.dumps(d, indent=1))
EOF
  chown ferrata:ferrata "$DATA/status/programs.json"
  install -m 644 "$DATA/status/programs.json" /srv/ferrata/www/programs.json
}

case "${1:-}" in
  plan)
    programs_dir
    as_ferrata "$D" discover "$URL" 2>&1 | tee -a "$LOG" | tail -n 25
    as_ferrata "$D" plan "$URL" 2>&1 | tee -a "$LOG" | tail -n 30 ;;
  fund)
    plan="$(find "$FERRATA_DAY1_STATE" -name plan.json | head -1)"
    [ -n "$plan" ] || { echo "no plan.json: run plan first"; exit 1; }
    for who in payer daemon; do
      to="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$plan" "$who")"
      # Fresh accounts: LEZ credits them only with their public key as admission evidence (--to-pk).
      pk="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]+"_public_key"])' "$FERRATA_DAY1_KEYS/accounts.json" "$who")"
      need="$(python3 -I -c 'import json,sys; print(int(json.load(open(sys.argv[1]))["need_"+sys.argv[2]]))' "$plan" "$who")"
      floor=100000000000; [ "$who" = daemon ] && floor=1000000000000   # 100 LGO; the daemon 1,000 LGO (about 60 days)
      amount=$(( need * 4 > floor ? need * 4 : floor ))
      as_ferrata /opt/ferrata/bin/ferrata-day1 transfer --from-key "$DATA/keys/operator/payer.key" --to "$to" --to-pk "$pk" \
        --amount "$amount" --rpc "$URL" --state "$DATA/keys/operator/tool" --wait 300 2>&1 | tee -a "$LOG"
    done ;;
  run)
    programs_dir
    # To the log file, not a pipe: day1.sh's detached daemon keeps its starter's stdout open, and a pipe reader would
    # wait for it until the daemon stops (seen on this host on 23 Nov 2026; the handover stops it).
    rc=0; FERRATA_TESTNET_SEND=yes as_ferrata env FERRATA_TESTNET_SEND=yes "$D" run "$URL" >> "$LOG" 2>&1 || rc=$?
    tail -n 40 "$LOG"; [ "$rc" = 0 ] || exit "$rc"
    publish ;;
  publish) publish ;;
  handover) # the kit's detached daemon -> ferrata-updater.service; attest-monitor on the instance; the site's ids
    as_ferrata "$D" daemon-stop "$URL" 2>&1 | tail -n 3 || true
    systemctl enable --now ferrata-updater.service
    python3 -I "$HERE/attest-env.py" "$FERRATA_DAY1_KEYS/instance.json" > /etc/ferrata/attest.env
    systemctl enable --now ferrata-monitor.service
    publish
    if [ -f /opt/ferrata/web.tar.gz ]; then bash "$HERE/install-web.sh" /opt/ferrata/web.tar.gz; fi
    systemctl is-active ferrata-updater ferrata-monitor ;;
  verify) as_ferrata "$D" verify "$URL" 2>&1 | tee -a "$LOG" | tail -n 40 ;;
  *) sed -n '2,/^set -euo/p' "$0" | sed '$d'; exit 2 ;;
esac
