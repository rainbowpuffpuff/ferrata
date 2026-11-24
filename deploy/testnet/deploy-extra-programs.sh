#!/usr/bin/env bash
# Deploy further programs with LEZ's own wallet CLI (`wallet program-loader deploy`, LEZ v0.4.0-rc3), immutable, each
# at fresh header and segment accounts, paid by the host's operator account (its key never leaves the host, never
# printed). Records the program (header) account ids in /var/lib/ferrata/status/extra-programs.json; run
# deploy-programs.sh publish afterwards to merge them into programs.json. As root on the host:
#
#   deploy-extra-programs.sh NAME=FILE:IMAGE_ID ...      e.g. ferrata_msg=/opt/ferrata/programs/ferrata_msg.bin:0x04ae…
#
# A name already recorded is skipped (programs are immutable; a second deployment would be another program).
set -euo pipefail
DATA=/var/lib/ferrata
OUT="$DATA/status/extra-programs.json"
HOME_W="$DATA/keys/cli-wallet"
export PATH=/opt/ferrata/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin
LOG=/var/log/ferrata/deploy-extra-programs.log
install -d -o ferrata -g ferrata -m 700 "$HOME_W"
if [ ! -f "$HOME_W/password" ]; then
  ( umask 077; head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' > "$HOME_W/password" ); chown ferrata:ferrata "$HOME_W/password"
fi
cli() { runuser -u ferrata -- env LEE_WALLET_HOME_DIR="$HOME_W" LEE_WALLET_PASSWORD="$(cat "$HOME_W/password")" \
  XDG_CACHE_HOME=/opt/ferrata/cache RISC0_SERVER_PATH=/opt/ferrata/bin/r0vm /opt/ferrata/bin/wallet "$@"; }
if [ ! -f "$HOME_W/wallet_config.json" ]; then
  sed 's#"sequencer_addr": *"[^"]*"#"sequencer_addr": "http://127.0.0.1:3040"#' \
    /opt/ferrata/src/upstream/lez-v0.4.0-rc3/lez/wallet/configs/debug/wallet_config.json > "$HOME_W/wallet_config.json"
  chown ferrata:ferrata "$HOME_W/wallet_config.json"
fi
cli account list > /dev/null 2>&1 || true
# The operator's account, imported once from its key file (read here, not printed). LEZ's wallet takes the key only as
# `account import public --private-key HEX` (no file, environment or stdin form in v0.4.0-rc3), so the key is in that
# process's command line, readable in /proc/<pid>/cmdline by local users for the instant it runs; on this host every
# local user is one of the network's own services. Done once per host (operator.id), never again.
if [ ! -f "$HOME_W/operator.id" ]; then
  op="$(cli account import public --private-key "$(tr -d ' \n' < "$DATA/keys/operator/payer.key")" 2>&1 \
        | grep -o -E 'Public/[1-9A-HJ-NP-Za-km-z]{32,44}' | head -1)"
  [ -n "$op" ] || { echo "could not import the operator account"; exit 1; }
  echo "$op" > "$HOME_W/operator.id"
fi
OP="$(cat "$HOME_W/operator.id")"
[ -f "$OUT" ] || echo '{}' > "$OUT"
new_public() { cli account new public 2>&1 | grep -o -E 'Public/[1-9A-HJ-NP-Za-km-z]{32,44}' | head -1; }
for spec in "$@"; do
  name="${spec%%=*}"; rest="${spec#*=}"; file="${rest%%:*}"; image="${rest#*:}"
  if python3 -I -c 'import json,sys; sys.exit(0 if sys.argv[2] in json.load(open(sys.argv[1])) else 1)' "$OUT" "$name"; then
    echo "$name: deployed already"; continue
  fi
  got="$(r0vm --elf "$file" --id | tail -n 1)"
  [ "0x${got#0x}" = "$image" ] || { echo "$name: image id $got is not $image"; exit 1; }
  size="$(stat -c %s "$file")"; n=$(( (size + 98303) / 98304 ))
  H="$(new_public)"; SEGS=(); for _ in $(seq 1 "$n"); do SEGS+=("$(new_public)"); done
  t0=$(date +%s)
  # The loader chunks the user ELF inside the binary, smaller than the file: LEZ's CLI says how many segments it wants.
  if ! cli program-loader deploy --elf "$file" --header "$H" --segments "${SEGS[@]}" --immutable --payer "$OP" >> "$LOG" 2>&1; then
    want="$(tail -n 5 "$LOG" | sed -n 's/.*splits into \([0-9]*\) segment.*/\1/p' | tail -n 1)"
    [ -n "$want" ] || { tail -n 20 "$LOG"; echo "$name: deploy failed"; exit 1; }
    while [ "${#SEGS[@]}" -lt "$want" ]; do SEGS+=("$(new_public)"); done
    SEGS=("${SEGS[@]:0:$want}"); n="$want"
    cli program-loader deploy --elf "$file" --header "$H" --segments "${SEGS[@]}" --immutable --payer "$OP" >> "$LOG" 2>&1 \
      || { tail -n 20 "$LOG"; echo "$name: deploy failed"; exit 1; }
  fi
  python3 -I - "$OUT" "$name" "${H#Public/}" "$image" "$(sha256sum "$file" | cut -d' ' -f1)" "$n" "$(( $(date +%s) - t0 ))" <<'EOF'
import json, sys
out, name, header, image, sha, n, secs = sys.argv[1:8]
d = json.load(open(out))
d[name] = {"program_account": header, "image_id": image, "binary_sha256": sha, "segments": int(n), "immutable": True,
           "deployed_with": "LEZ v0.4.0-rc3 wallet program-loader deploy", "seconds": int(secs)}
json.dump(d, open(out, "w"), indent=1)
EOF
  chown ferrata:ferrata "$OUT"
  echo "$name: program account ${H#Public/} ($n segments + header, $(( $(date +%s) - t0 )) s)"
done
