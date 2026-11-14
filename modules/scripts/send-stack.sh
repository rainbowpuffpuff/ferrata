#!/usr/bin/env bash
# Logos Core's Ethereum send stack built from its sources and loaded in a headless Logos Core: keystore_module,
# signer_manager_module, fee_module, eth_rpc_module and tx_sender_module, at the revisions tx_sender_module's lock pins,
# each with logos-module-builder 5f74cdb as 5f74cdb locks its own inputs (graft_builder_lock.py: the module's lock with
# the builder subtree of modules/ferrata_bridge_module/flake.lock; every other input as the module's lock pins it).
# Built with `--override-input logos-module-builder` instead, keystore_module and eth_rpc_module keep their own older
# logos-cpp-sdk (e1d6bf2, no `--normalize-lidl`) and stop in their install phase (modules/LOGOS-CORE-MODULES.md).
# A module's patches, modules/patches/<name>-<rev7>-*.patch, are applied to its source before the build:
# eth_rpc_module 6965f32's glue for the verified_proxy_module client builder 5f74cdb generates (without it, E0308 at
# rust-lib/src/glue.rs 212 and 247). verified_proxy_module and beacon_api_module (eth_rpc_module's verified route) are
# built with their own lock (builder a57dc7f), as modules/scripts/verified-proxy.sh builds them; logosctl 66d098d as
# modules/scripts/check.sh builds it.
#
#   modules/scripts/send-stack.sh build OUT      the dev .lgx in OUT/lgx-<name>/, logosctl in OUT/logos (Nix; OUT/logs,
#                                                OUT/steps.tsv); ONLY="a b" builds only these
#   LOGOSCTL=... modules/scripts/send-stack.sh run OUT
#                                                a headless Logos Core with them on an anvil chain (31337): loaded, the
#                                                callers' identities, eth_rpc_module's chain, tx_sender_module's prepare
#                                                of a transfer (OUT/calls.jsonl, OUT/report.md)
# Box only (Nix, a 1 GiB stack). Nothing leaves the machine but the sources' fetches.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
MODE=${1:?build|run}; OUT=$(mkdir -p "${2:?OUT}" && cd "$2" && pwd)
mkdir -p "$OUT/logs"
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
MODS="eth_rpc_module logos-evm-eth-rpc-module 6965f3252986eef02e9342198a0625fa725aad8d
fee_module logos-evm-fee-module f2065e787eebcd7fa49f6cf6ab1dc39457cba981
keystore_module logos-evm-keystore-module e83a899a1718ed52e3173dc85d06b15b9c68f41a
signer_manager_module logos-signer-manager-module bd5c84d21ced71246c4bf44d137aa1c597496fb0
tx_sender_module logos-evm-tx-sender-module f1754c3b13c7c580ee449116ba8982ca193e114d"
# Built with their own lock (no graft): the pair upstream tests together.
OWN_LOCK="beacon_api_module logos-evm-beacon-api-module aca138e5c211a370654700f4f8f2779ea5081c86
verified_proxy_module logos-verified-proxy-module fa984790c5c5d7b39f37ed42d7b66abfc9e72b67"
CLI=github:logos-co/logos-logoscore-cli/66d098d3af1367cf831b69c13a5aa8345edeb682

build() {
  [ -f "$OUT/steps.tsv" ] || printf 'module\trevision\tresult\tseconds\n' > "$OUT/steps.tsv"
  while read -r name gh rev; do
    local src="$OUT/src/$name" t0 r
    # ONLY="a b": build only these.
    case " ${ONLY:-$name} " in *" $name "*) ;; *) continue ;; esac
    # A builder that writes $HOME leaves /homeless-shelter behind, and every later build refuses to start.
    rm -rf /homeless-shelter
    t0=$(date +%s)
    rm -rf "$src" && git clone -q "https://github.com/logos-co/$gh" "$src" && git -C "$src" checkout -q "$rev" \
      && python3 -I "$here/graft_builder_lock.py" "$src/flake.lock" "$repo/modules/ferrata_bridge_module/flake.lock" > "$src/flake.lock.new" \
      2> "$OUT/logs/graft-$name.log" && mv "$src/flake.lock.new" "$src/flake.lock" || { echo "$name: source or graft failed"; continue; }
    local patch
    for patch in "$repo/modules/patches/$name-${rev:0:7}-"*.patch; do
      [ -f "$patch" ] || continue
      git -C "$src" apply --verbose "$patch" >> "$OUT/logs/graft-$name.log" 2>&1 \
        || { echo "$name: $(basename "$patch") does not apply"; continue 2; }
      echo "$name: applied $(basename "$patch")"
    done
    # A derivation that writes $HOME leaves /homeless-shelter behind and the next derivation of the same build
    # refuses to start ("home directory '/homeless-shelter' exists"); finished derivations stay in the store, so the
    # build is retried from where it stopped (as logos-core-io's runs did, up to 12 times).
    local attempt
    for attempt in $(seq 1 12); do
      rm -rf /homeless-shelter
      if nix build "path:$src#lgx" -o "$OUT/lgx-$name" -L > "$OUT/logs/build-$name.log" 2>&1; then r="ok (attempt $attempt)"; break; fi
      r="FAILED($?, attempt $attempt)"
      grep -q "homeless-shelter' exists" "$OUT/logs/build-$name.log" || break
    done
    printf '%s\t%s\t%s\t%s\n' "$name" "${rev:0:12}" "$r" "$(( $(date +%s) - t0 ))" | tee -a "$OUT/steps.tsv"
    grep -a -E 'Plugin file does not exist|^error|error:|error\[' "$OUT/logs/build-$name.log" | head -n 8
  done <<< "$MODS"
  local attempt
  while read -r name gh rev; do
    case " ${ONLY:-$name} " in *" $name "*) ;; *) continue ;; esac
    t0=$(date +%s)
    for attempt in $(seq 1 12); do
      rm -rf /homeless-shelter
      if nix build "github:logos-co/$gh/$rev#lgx" -o "$OUT/lgx-$name" -L > "$OUT/logs/build-$name.log" 2>&1; then r="ok (attempt $attempt, own lock)"; break; fi
      r="FAILED($?, attempt $attempt)"
      grep -q "homeless-shelter' exists" "$OUT/logs/build-$name.log" || break
    done
    printf '%s\t%s\t%s\t%s\n' "$name" "${rev:0:12}" "$r" "$(( $(date +%s) - t0 ))" | tee -a "$OUT/steps.tsv"
  done <<< "$OWN_LOCK"
  case " ${ONLY:-logosctl} " in
    *" logosctl "*)
      t0=$(date +%s)
      for attempt in $(seq 1 12); do
        rm -rf /homeless-shelter
        if nix build "$CLI#ctl" -o "$OUT/logos" -L > "$OUT/logs/build-logosctl.log" 2>&1; then r="ok (attempt $attempt)"; break; fi
        r="FAILED($?, attempt $attempt)"
        grep -q "homeless-shelter' exists" "$OUT/logs/build-logosctl.log" || break
      done
      printf '%s\t%s\t%s\t%s\n' logosctl 66d098d3af13 "$r" "$(( $(date +%s) - t0 ))" | tee -a "$OUT/steps.tsv" ;;
  esac
  ( cd "$OUT" && sha256sum lgx-*/*.lgx ) > "$OUT/lgx-sha256.txt" 2>&1
}

run() {
  : "${LOGOSCTL:?the logosctl to use}"
  local LT="${LOGOS_TMPDIR:-/var/tmp/ss}" S="$OUT/session" anvil
  mkdir -p "$LT"; rm -rf "$S" "$OUT/home"; mkdir -p "$OUT/home"; : > "$OUT/calls.jsonl"
  anvil --port 18545 --chain-id 31337 --silent > "$OUT/anvil.log" 2>&1 & anvil=$!
  for _ in $(seq 60); do cast chain-id --rpc-url http://127.0.0.1:18545 > /dev/null 2>&1 && break; sleep 1; done
  ctl() { TMPDIR="$LT" "$LOGOSCTL" --config-dir "$S" "$@"; }
  # c MODULE METHOD ARGS...: one call through Logos Core, recorded.
  c() {
    local m="$1" f="$2" out rc; shift 2
    out="$(ctl --timeout 120000 call "$m" "$f" "$@" --json < /dev/null 2>&1)"; rc=$?
    jq -c -n --arg m "$m" --arg f "$f" --args '$ARGS.positional' "$@" > "$OUT/.args" 2>/dev/null || echo '[]' > "$OUT/.args"
    jq -c -n --arg m "$m" --arg f "$f" --slurpfile a "$OUT/.args" --arg o "$out" --argjson rc "$rc" \
      '{module: $m, method: $f, args: $a[0], exit: $rc, reply: ($o | fromjson? // $o)}' >> "$OUT/calls.jsonl"
    echo "$m.$f -> exit $rc: ${out:0:300}"
  }
  HOME="$OUT/home" TMPDIR="$LT" "$LOGOSCTL" --config-dir "$S" daemon start --detach > "$OUT/daemon.txt" 2>&1
  {
    while read -r name _ _; do ctl install "$OUT/lgx-$name"/*.lgx -y 2>&1 | tail -n 2; done <<< "$MODS"
    ctl module load tx_sender_module 2>&1 | tail -n 3
    ctl module list --json > "$OUT/modules-loaded.json" 2>&1; jq -c '.' "$OUT/modules-loaded.json" 2>/dev/null | head -c 1500; echo
    c eth_rpc_module set_chain_config 31337 '{"endpoint":"http://127.0.0.1:18545","name":"anvil","testnet":true}'
    c eth_rpc_module block_number 31337
    c keystore_module caller_identity
    c signer_manager_module signers
    c tx_sender_module prepare '{"chainId":31337,"from":"0x70997970C51812dc3A010C7d01b50e0d17dc79C8","calls":[{"to":"0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC","value":"1"}]}'
  } > "$OUT/run.log" 2>&1
  cp "$S"/logs/*.log "$OUT/" 2>/dev/null || true
  ctl daemon stop > /dev/null 2>&1; kill $anvil 2>/dev/null
  cat "$OUT/run.log"
}

case "$MODE" in build) build ;; run) run ;; *) sed -n 2,14p "$0"; exit 2 ;; esac
