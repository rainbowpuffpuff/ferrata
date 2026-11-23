#!/usr/bin/env bash
# The updater module on our public LEZ test network, and the off-chain components' resources in the same run
# (RFP-021 U.3 and P.6, RFP-022 P.6; record and figures: modules/results/live-testnet-*/README.md).
#
#   live-testnet.sh build OUT             on the build box: the guests as committed (lez/methods, LEZ v0.4.0-rc1: the
#                                         images the network runs), then a copy of the tree with LEZ v0.4.0-rc3 selected
#                                         (the network's tag; lez/scripts/lez-select.sh) for ferrata-day1, ferrata-updater
#                                         and ferrata_updater_module (dev package); ferrata_attest_module (dev package),
#                                         attest-monitor, the ferrata-attest CLI; logosctl 66d098d, beacon_api_module
#                                         aca138e, eth_rpc_module 6965f32 (as verified-proxy.sh builds them)
#   live-testnet.sh instance OUT          a second Sepolia instance of our own on the network's published attestation
#                                         program (not the host's): fresh keys, two faucet claims (public keys only),
#                                         a fresh finalized checkpoint (Lodestar's bootstrap, publicnode's header), Init
#   live-testnet.sh session OUT MINUTES   one headless Logos Core: ferrata_updater_module started on the instance, both
#                                         light-client sources through beacon_api_module, `ledger = "sequencer"` on the
#                                         network; ferrata_attest_module's calls with Ethereum reads through
#                                         eth_rpc_module; attest-monitor watching the instance; an attestation sent with
#                                         ferrata-day1; the ferrata-attest CLI's journey; every process sampled
#                                         (bench/perf/tree_sample.py), each daemon's traffic counted by its own relay
#                                         (bench/perf/http_relay.py); a restart through the module's stop and start; the
#                                         module's stop and Logos Core's stop at the end
#   live-testnet.sh build-bridge OUT      ferrata_monitor_module and ferrata_relayer_module (dev packages, the tree as
#                                         committed)
#   live-testnet.sh bridge-modules OUT [IDLE_MIN]   bench/perf/daemons-anvil-modules.sh with them (RFP-022 P.6)
#   live-testnet.sh collect OUT RECORD    the record (no keys, no wallet storage, the host's name and the box's paths
#                                         replaced) and its summary (modules/scripts/live_testnet.py)
#
# Env: FERRATA_TESTNET_RPC (the network's JSON-RPC URL; required by instance and session, never written into a record),
# FERRATA_TESTNET_SEND=yes (instance and session send transactions to that network, and only to it), LOGOS_TMPDIR (a
# short directory for Logos Core's sockets, 62 bytes at most; default /var/tmp/mls), ML_WORK (scratch outside OUT for
# the copy and the cargo targets; default /var/tmp/ml). Nothing is sent to Sepolia or any other chain: Ethereum is only
# read (public endpoints, no API key, the modules' own clients or curl with the generic User-Agent `curl/8`).
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
spike=$(cd "$repo/.." && pwd)
die() { echo "live-testnet.sh: $*" >&2; exit 1; }
cmd=${1:?build, instance, session or collect}
mkdir -p "${2:?OUT}"
out=$(cd "$2" && pwd)
case "$out/" in "$repo/"*) die "OUT must be outside the repository: $out" ;; esac
BUILDER=github:logos-co/logos-module-builder/5f74cdb3cecacde054c68d72fcc0d2cfe2195900
CLI=github:logos-co/logos-logoscore-cli/66d098d3af1367cf831b69c13a5aa8345edeb682
BEACON_REV=aca138e5c211a370654700f4f8f2779ea5081c86
ETH_RPC_REV=6965f3252986eef02e9342198a0625fa725aad8d
TAG=v0.4.0-rc3
PROGRAM_IMAGE=0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df   # docs/image-ids.md ferrata_lez
CHAIN=11155111
LODESTAR=https://lodestar-sepolia.chainsafe.io
NIMBUS=http://unstable.sepolia.beacon-api.nimbus.team
PN_BEACON=https://ethereum-sepolia-beacon-api.publicnode.com
EL=https://rpc.sepolia.ethpandaops.io    # keeps every proof (modules/results/verified-proxy-2026-11-14/)
VAULT=0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c   # FerrataVault v5 on Sepolia (docs/sepolia.md): its slots 0 and 1
UA=curl/8
SLOT0=0x0000000000000000000000000000000000000000000000000000000000000000
SLOT1=0x0000000000000000000000000000000000000000000000000000000000000001
WORK=${ML_WORK:-/var/tmp/ml}
LOGOS_TMPDIR=${LOGOS_TMPDIR:-/var/tmp/mls}
[ $(( ${#LOGOS_TMPDIR} + 45 )) -le 107 ] || die "LOGOS_TMPDIR=$LOGOS_TMPDIR is too long for Logos Core's sockets"
mkdir -p "$WORK" "$LOGOS_TMPDIR"
export GIT_TERMINAL_PROMPT=0 CARGO_TERM_COLOR=never
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
rep="$out/report-$cmd"
mkdir -p "$rep/logs"
[ -f "$rep/steps.md" ] || printf '| step | result | seconds |\n|---|---|---:|\n' > "$rep/steps.md"
failures=0
note() { printf '%s %s\n' "$(date -u +%H:%M:%S)" "$*" | tee -a "$rep/notes.log"; }
step() { # NAME CMD...: CMD's output to report/logs/NAME.log, a row in report/steps.md
  local name=$1 t0 rc
  shift
  t0=$(date +%s)
  note "step $name"
  if "$@" > "$rep/logs/$name.log" 2>&1; then rc=ok; else rc="FAILED (exit $?)"; failures=$((failures + 1)); fi
  printf '| %s | %s | %s |\n' "$name" "$rc" "$(( $(date +%s) - t0 ))" >> "$rep/steps.md"
  [ "$rc" = ok ] || tail -n 25 "$rep/logs/$name.log" | sed "s/^/  [$name] /"
  [ "$rc" = ok ]
}
# nix build, retried on the box's known failures (check.sh, verified-proxy.sh).
nixb() {
  local attempt log="$WORK/nix-build-attempt.log"
  for attempt in $(seq 1 20); do
    if [ "$(id -u)" = 0 ] && [ -d /homeless-shelter ]; then rm -rf /homeless-shelter; fi
    if nix build "$@" > >(tee "$log") 2>&1; then return 0; fi
    sleep 2
    grep -q -E 'reading a line: Input/output error|unexpected EOF reading a line|/homeless-shelter. exists|unable to download|HTTP error|timed out' "$log" || return 1
    echo "live-testnet.sh: nix build $* stopped on a box or network error (attempt $attempt); retrying"
  done
  return 1
}
b58hex() { # a LEZ account (base58) as the 32 bytes' hex, as attest-monitor takes it
  python3 -I -c 'import sys
A = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
n = 0
for ch in sys.argv[1]:
    n = n * 58 + A.index(ch)
print(n.to_bytes(32, "big").hex())' "$1"
}
C="$WORK/rc3"                 # the copy with LEZ $TAG selected: $C/ferrata (the tree), $C/upstream (-> spike/upstream)
BIN="$out/bin"                # the binaries the session runs
mkdir -p "$BIN"

# ---- build --------------------------------------------------------------------------------------------------------
guests() { (cd "$repo/lez" && RISC0_HOME="$spike/risc0-home-lez" cargo build --locked -p methods); }
methods_rs() {
  python3 -I -c 'import glob, os, sys
found = glob.glob(os.path.join(sys.argv[1], "**", "build", "methods-*", "out", "methods.rs"), recursive=True)
print(max(found, key=os.path.getmtime) if found else "")' "$repo/lez/target"
}
rc3_copy() {
  mkdir -p "$C/ferrata"
  rsync -a --delete --exclude='target/' --exclude='.git' --exclude='/modules/*/rust-lib/vendor/' \
    --exclude='/modules/*/rust-lib/staged/' --exclude='/modules/*/rust-lib/upstream/' --exclude='/modules/*/logos-rust-sdk-src' \
    --exclude='node_modules/' --exclude='/vault/lib/' "$repo/" "$C/ferrata/"
  ln -sfn "$spike/upstream" "$C/upstream"
  "$C/ferrata/lez/scripts/lez-select.sh" "$TAG"
  # What lez-select.sh does not rewrite and the updater module links: the harness's wire crates (rebate-wire,
  # statements-wire) and the module's calls crate, which names the staged LEZ tree by its tag (stage-updater.sh
  # checks it). Every remaining manifest of lez/ and of the module, in this copy only.
  grep -rl "upstream/lez-v0.4.0-rc1/" --include=Cargo.toml "$C/ferrata/lez" "$C/ferrata/modules/ferrata_updater_module" \
    | grep -v '/target/' | tee "$WORK/rc3-rewritten.txt" | xargs -r sed -i "s#upstream/lez-v0.4.0-rc1/#upstream/lez-$TAG/#g"
  echo "also pointed at $TAG: $(wc -l < "$WORK/rc3-rewritten.txt") manifests"; cat "$WORK/rc3-rewritten.txt"
  sed -i "s#on a running LEZ v0.4.0-rc1 sequencer#on a running LEZ $TAG sequencer#" "$C/ferrata/modules/ferrata_updater_module/metadata.json"
  "$C/ferrata/lez/scripts/lez-select.sh" --show
}
rc3_tools() {
  (cd "$C/ferrata/lez/updater/sequencer" && CARGO_TARGET_DIR="$WORK/target-rc3" cargo build --release --locked \
    --bin ferrata-day1 --bin ferrata-updater) || return 1
  cp "$WORK/target-rc3/release/ferrata-day1" "$WORK/target-rc3/release/ferrata-updater" "$BIN/"
}
attest_monitor() {
  (cd "$repo/monitor/attest" && CARGO_TARGET_DIR="$WORK/target-attest-monitor" cargo build --release --locked) || return 1
  cp "$WORK/target-attest-monitor/release/attest-monitor" "$BIN/"
}
# The CLI as lez/ATTEST-CLI.md builds it (debug: the harness's test-utils refuse a build without debug assertions).
attest_cli() {
  (cd "$repo/lez" && RISC0_HOME="$spike/risc0-home-lez" cargo build --locked -p harness --bin ferrata-attest) || return 1
  cp "$repo/lez/target/debug/ferrata-attest" "$BIN/"
}
stage_updater_rc3() {
  local m
  m=$(methods_rs)
  [ -n "$m" ] || { echo "no lez/methods build output"; return 1; }
  (cd "$C/ferrata" && METHODS_RS="$m" FERRATA_COMMIT="$(git -C "$repo" rev-parse --short HEAD) ($TAG copy)" \
    modules/scripts/stage.sh ferrata_updater_module) || return 1
  jq -e --arg id "$PROGRAM_IMAGE" '[.[]] | index($id) != null' "$C/ferrata/modules/ferrata_updater_module/rust-lib/vendor/ids.json" \
    || { echo "the staged guests do not hold the network's attestation program $PROGRAM_IMAGE"; return 1; }
  jq . "$C/ferrata/modules/ferrata_updater_module/rust-lib/vendor/STAGED.json"
}
sdk() { # MODULE DIR
  nixb "$BUILDER#rust-sdk-src" -o "$2/modules/$1/logos-rust-sdk-src"
}
# The updater module's lockfile for $TAG: the committed one (rust-lib/lockfiles/), else the rc1 lockfile moved to
# $TAG's graph by cargo (versions kept wherever they still fit), written back for the repository.
lock_updater_rc3() {
  local r="$C/ferrata/modules/ferrata_updater_module/rust-lib" saved="$repo/modules/ferrata_updater_module/rust-lib/lockfiles/$TAG.Cargo.lock"
  if [ -f "$saved" ]; then
    cp "$saved" "$r/Cargo.lock"
    (cd "$r" && cargo +1.98.1 metadata --locked --format-version 1 > /dev/null)
  else
    (cd "$r" && cargo +1.98.1 metadata --format-version 1 > /dev/null) || return 1
    mkdir -p "$(dirname "$saved")"
    cp "$r/Cargo.lock" "$saved"
    echo "wrote $saved"
  fi
}
prepare() { # MODULE SOURCE_TREE: the module's copy in a fresh repository, the staged crates added (check.sh)
  local m=$1 src=$2 b="$WORK/build-$1" c
  rm -rf "$b"
  mkdir -p "$b"
  tar -C "$src/modules/$m" --exclude=./out --exclude=./rust-lib/target --exclude=./logos-rust-sdk-src \
    --exclude='./result*' -cf - . | tar -C "$b" -xf -
  (
    cd "$b" || exit 1
    git init -q && git add -A
    for c in rust-lib/staged rust-lib/upstream rust-lib/attestor rust-lib/crates rust-lib/monitor rust-lib/checkpoint \
             rust-lib/vendor; do if [ -d "$c" ]; then git add -f "$c"; fi; done
    git add -f rust-lib/Cargo.lock
    if [ ! -f flake.lock ]; then nix flake lock && git add flake.lock; fi
  )
}
lgx() { (cd "$WORK/build-$1" && nixb ".#lgx" -o "$out/lgx-$1" -L); }
eth_rpc() {
  local s="$WORK/src-eth_rpc_module"
  rm -rf "$s"
  git clone -q https://github.com/logos-co/logos-evm-eth-rpc-module.git "$s" && git -C "$s" checkout -q "$ETH_RPC_REV" \
    && python3 -I "$here/eth_rpc_by_name.py" "$s" && git -C "$s" add -A \
    && nixb "git+file://$s#lgx" --override-input logos-module-builder "$BUILDER" -o "$out/lgx-eth_rpc_module"
}
save_nix() {
  local f=/vol/nix/modules-live.tar.zst
  [ -d /vol ] || { echo "no /vol: not the build box"; return 0; }
  mkdir -p /vol/nix
  tar -C / -cf - nix | zstd -T0 -3 -q -f -o "$f.part" && mv "$f.part" "$f" && ls -la "$f"
}
versions() {
  uname -srm; echo "cpus $(nproc)"; free -g | sed -n 2p; nix --version; cargo --version; rustc +1.98.1 --version
  git -C "$repo" rev-parse HEAD; git -C "$spike/upstream/lez-$TAG" rev-parse HEAD
}

do_build() {
  step versions versions
  step guests guests || return 1
  step rc3-copy rc3_copy || return 1
  step stage-updater-rc3 stage_updater_rc3 || return 1
  step sdk-updater sdk ferrata_updater_module "$C/ferrata" || return 1
  step lock-updater-rc3 lock_updater_rc3 || return 1
  step prepare-updater prepare ferrata_updater_module "$C/ferrata" || return 1
  # Two lanes: Nix builds one at a time (two at once race on the box's /homeless-shelter), cargo beside them.
  (
    step lgx-ferrata_updater_module lgx ferrata_updater_module
    step logosctl nixb "$CLI#ctl" -o "$out/logos"
    step lgx-beacon_api_module nixb "github:logos-co/logos-evm-beacon-api-module/$BEACON_REV#lgx" -o "$out/lgx-beacon_api_module"
    step lgx-eth_rpc_module eth_rpc
    step stage-attest "$here/stage.sh" ferrata_attest_module && step prepare-attest prepare ferrata_attest_module "$repo" \
      && step lgx-ferrata_attest_module lgx ferrata_attest_module
  ) &
  local nixlane=$!
  step rc3-tools rc3_tools
  step rc3-tools-test bash -c "cd '$C/ferrata/lez/updater/sequencer' && CARGO_TARGET_DIR='$WORK/target-rc3' cargo test --release --locked --lib"
  step attest-monitor attest_monitor
  step attest-cli attest_cli
  wait "$nixlane"
  {
    for f in "$out"/lgx-*/*.lgx; do [ -f "$f" ] && printf '%s  %s %s bytes\n' "$(sha256sum < "$f" | cut -c1-64)" "${f#"$out"/}" "$(stat -L -c %s "$f")"; done
    for f in "$BIN"/*; do printf '%s  bin/%s\n' "$(sha256sum < "$f" | cut -c1-64)" "$(basename "$f")"; done
    "$out/logos/bin/logosctl" --version 2>&1 | head -n 2
  } > "$rep/artifacts.txt" 2>&1
  if [ "${SAVE_NIX:-1}" = 1 ]; then step save-nix save_nix; fi
  failures=$(grep -c FAILED "$rep/steps.md")
}

# RFP-022's two Ethereum-side daemons as modules (the tree as committed, LEZ v0.4.0-rc1 for the relayer's LEZ side).
do_build_bridge() {
  step stage-bridge-modules "$here/stage.sh" ferrata_monitor_module ferrata_relayer_module || return 1
  step prepare-monitor prepare ferrata_monitor_module "$repo" || return 1
  step prepare-relayer prepare ferrata_relayer_module "$repo" || return 1
  step lgx-ferrata_monitor_module lgx ferrata_monitor_module
  step lgx-ferrata_relayer_module lgx ferrata_relayer_module
  failures=$(grep -c FAILED "$rep/steps.md")
}
do_bridge_modules() {
  LOGOS_TMPDIR=/var/tmp/mlb bash "$repo/bench/perf/daemons-anvil-modules.sh" "$out/daemons-anvil-modules" "${1:-10}" \
    "$out/logos/bin/logosctl" "$out/lgx-ferrata_monitor_module" "$out/lgx-ferrata_relayer_module"
}

# ---- instance -----------------------------------------------------------------------------------------------------
need_net() {
  : "${FERRATA_TESTNET_RPC:?the JSON-RPC URL of the network}"
  [ "${FERRATA_TESTNET_SEND:-}" = yes ] || die "set FERRATA_TESTNET_SEND=yes: transactions go to $FERRATA_TESTNET_RPC (and nowhere else)"
  export FERRATA_SEQUENCER_REMOTE="$FERRATA_TESTNET_RPC"
  ORIGIN=${FERRATA_TESTNET_RPC%/rpc}
}
day1() { "$BIN/ferrata-day1" "$@" --rpc "$FERRATA_TESTNET_RPC"; }
K="$out/keys"; S="$out/day1"; F="$out/fetch"; I="$out/instance"
claim() { # ACCOUNT PUBLIC_KEY NAME: one faucet claim (the account and its public key, nothing else)
  local body
  body=$(jq -n --arg a "$1" --arg k "$2" '{account: $a, public_key: $k}')
  curl -sS -m 180 -A "$UA" -H 'content-type: application/json' --data-binary "$body" "$ORIGIN/faucet" -o "$I/faucet-$3.json" -w '%{http_code}\n'
  jq -c '{ok, amount, credited, balance, error}' "$I/faucet-$3.json"
}
do_instance() {
  need_net
  mkdir -p "$I"
  curl -sS -m 60 -A "$UA" "$ORIGIN/programs.json" -o "$I/programs.json" || die "programs.json"
  local program image
  program=$(jq -r .programs.ferrata_attestation.program_account "$I/programs.json")
  image=$(jq -r .programs.ferrata_attestation.image_id "$I/programs.json")
  [ "$image" = "$PROGRAM_IMAGE" ] || die "the network's attestation program is $image, not $PROGRAM_IMAGE"
  note "the network's attestation program: $program ($image); the host's instance $(jq -r .instances.sepolia.instance_id "$I/programs.json" | cut -c1-8)… is left alone"
  if [ ! -f "$K/payer.key" ]; then step keygen day1 keygen --keys "$K" --program "$program" || return 1; fi
  jq '{payer, payer_public_key, daemon, daemon_public_key, program}' "$K/accounts.json" > "$I/accounts.json"
  for who in payer daemon; do
    if [ ! -f "$I/faucet-$who.json" ] || ! jq -e .ok "$I/faucet-$who.json" > /dev/null; then
      step "faucet-$who" claim "$(jq -r ".$who" "$K/accounts.json")" "$(jq -r ".${who}_public_key" "$K/accounts.json")" "$who"
    fi
  done
  if [ ! -f "$K/init/chain.json" ] && [ ! -f "$F/chain.json" ]; then
    step checkpoint python3 -I "$repo/lez-v03/scripts/fetch.py" checkpoint "$F" || return 1
  fi
  if [ ! -d "$K/tx/init" ]; then
    step sign-init day1 sign --keys "$K" --state "$S" --stage init --fetch "$F" || return 1
  fi
  step send-init /usr/bin/time -v "$BIN/ferrata-day1" send --rpc "$FERRATA_TESTNET_RPC" --keys "$K" --state "$S" --stage init || return 1
  step inspect day1 inspect --keys "$K" --state "$S"
  cp "$K/instance.json" "$I/instance.json"
  cp "$rep/logs/inspect.log" "$I/inspect-after-init.json"
  mkdir -p "$I/tx-init"
  cp "$K/tx/init/"*.json "$I/tx-init/" 2>/dev/null
  jq -c '{instance_id, checkpoint, checkpoint_slot, program}' "$I/instance.json"
}

# ---- session ------------------------------------------------------------------------------------------------------
do_session() {
  need_net
  local minutes=${1:-90} W="$out/session" SESSION="$LOGOS_TMPDIR/mls"
  local program id checkpoint t_start t_end
  program=$(jq -r .program "$I/instance.json"); id=$(jq -r .instance_id "$I/instance.json"); checkpoint=$(jq -r .checkpoint "$I/instance.json")
  rm -rf "$W" "$SESSION"
  mkdir -p "$W/rep" "$W/req" "$W/probes-u1" "$W/relay" "$W/attest" "$W/cli" "$W/monitor" "$W/logos-logs" "$SESSION"
  export TMPDIR="$LOGOS_TMPDIR"
  printf 'name\tmodule\tmethod\texit\tseconds\tstart\n' > "$W/calls.tsv"
  marks() { printf '%s\t%s\n' "$(date +%s.%N)" "$1" >> "$W/marks.tsv"; }
  printf 't\tmark\n' > "$W/marks.tsv"
  local pids=()
  cleanup() {
    "$out/logos/bin/logosctl" --config-dir "$SESSION" daemon stop > /dev/null 2>&1 || true
    for p in "${pids[@]}"; do kill -TERM "$p" 2>/dev/null || true; done
    wait 2>/dev/null || true
  }
  trap cleanup EXIT
  # Each daemon's traffic through its own relay.
  relay() { python3 -I "$repo/bench/perf/http_relay.py" --listen "$2" --to "$3" --stats "$W/relay/$1.json" --series "$W/relay/$1.jsonl" --every 30 & pids+=($!); }
  relay lez-updater 18701 "$FERRATA_TESTNET_RPC"
  relay lez-monitor 18702 "$FERRATA_TESTNET_RPC"
  relay lodestar-updater 18711 "$LODESTAR"
  relay nimbus-updater 18712 "$NIMBUS"
  relay lodestar-monitor 18721 "$LODESTAR"
  relay publicnode-monitor 18722 "$PN_BEACON"
  sleep 1
  ctl() { "$out/logos/bin/logosctl" --config-dir "$SESSION" "$@"; }
  call() { # MODULE METHOD NAME [ARGS...]
    local m=$1 f=$2 name=$3 t0 rc=0
    shift 3
    t0=$(date +%s.%N)
    ctl --timeout "${CALL_MS:-300000}" call "$m" "$f" "$@" --json < /dev/null > "$W/rep/$name.raw" 2>&1 || rc=$?
    jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' "$W/rep/$name.raw" \
      > "$W/rep/$name.json" 2>/dev/null || echo "{\"call_failed\":$rc}" > "$W/rep/$name.json"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$m" "$f" "$rc" "$(awk -v a="$t0" -v b="$(date +%s.%N)" 'BEGIN { printf "%.2f", b - a }')" "$t0" >> "$W/calls.tsv"
    return $rc
  }
  marks session-start
  note "session: $minutes minutes; Logos Core $("$out/logos/bin/logosctl" --version 2>/dev/null | head -n 1)"
  # The process sampler: Logos Core's daemon (found once it runs), each module's host process with its children,
  # attest-monitor, the relays.
  python3 -I "$repo/bench/perf/tree_sample.py" --out "$W/samples.csv" --every 5 \
    --find "updater-module=logos_host --name ferrata_updater_module " --find "attest-module=logos_host --name ferrata_attest_module " \
    --find "beacon_api_module=logos_host --name beacon_api_module " --find "eth_rpc_module=logos_host --name eth_rpc_module " \
    --find "attest-monitor=attest-monitor watch " --find "core=logosctl --config-dir $SESSION daemon" \
    --find "relays=python3 -I $repo/bench/perf/http_relay.py --listen 18701 " & pids+=($!)
  env HOME="$W/home" RISC0_SERVER_PATH="${RISC0_SERVER_PATH:-$HOME/.risc0/bin/r0vm}" \
    "$out/logos/bin/logosctl" --config-dir "$SESSION" daemon start --detach >> "$W/core-start.log" 2>&1
  for m in beacon_api_module eth_rpc_module ferrata_attest_module ferrata_updater_module; do
    ctl install "$out/lgx-$m"/*.lgx -y >> "$W/install.log" 2>&1 || note "install $m failed"
  done
  for m in beacon_api_module eth_rpc_module ferrata_attest_module ferrata_updater_module; do
    ctl module load "$m" >> "$W/install.log" 2>&1 || note "load $m failed"
  done
  ctl module list --json > "$W/rep/modules-loaded.json" 2>&1 || true
  ps -eo pid,ppid,rss,args | grep -F "$SESSION" | grep -v grep > "$W/processes.txt" || true
  call ferrata_updater_module version U-version
  call ferrata_attest_module version A-version
  # Consensus endpoints through beacon_api_module: its own defaults, and the two the updater asks first, each through
  # its counting relay (an operator's own providers, added with the module's add_provider).
  call beacon_api_module init_defaults bam-init_defaults
  call beacon_api_module add_provider bam-add-lodestar "$CHAIN" '{"endpoint":"http://127.0.0.1:18711","id":"lodestar-counted"}'
  call beacon_api_module add_provider bam-add-nimbus "$CHAIN" '{"endpoint":"http://127.0.0.1:18712","id":"nimbus-counted"}'
  call beacon_api_module list_providers bam-providers "$CHAIN"
  # The execution endpoint through eth_rpc_module (mode off: straight to the endpoint).
  call eth_rpc_module set_network_scope rpc-scope both
  call eth_rpc_module set_chain_config rpc-chain "$CHAIN" \
    "{\"endpoint\":\"$EL\",\"name\":\"sepolia\",\"testnet\":true,\"timeoutSecs\":60,\"verifiedTimeoutSecs\":240}"
  call eth_rpc_module set_verified_proxy_mode rpc-mode "$CHAIN" off
  call eth_rpc_module get_chain_config rpc-chain-config "$CHAIN"
  # The updater: the instance `instance` made, the network's program, both sources through beacon_api_module.
  cat > "$W/req/updater.toml" <<T
# live-testnet.sh session: ferrata_updater_module on the public test network (LEZ $TAG), our own Sepolia instance.
[daemon]
name = "modules-live"
state_dir = "$W/updater-state"
poll_secs = 24
listen = "127.0.0.1:9464"
ledger = "sequencer"

[sequencer]
rpc = "http://127.0.0.1:18701"
program = "$program"
wait_final = true

[keys]
payer_key_file = "$K/daemon.key"

[fees]
gas_limit = 4800000
max_fee_per_tx = 1000000000
budget = 6000000000

[policy]
defer_window = 1
max_lag_slots = 64
min_agreeing = 2
retry_base_ms = 500
retry_max_ms = 30000
max_attempts = 5
demote_rounds = 50
max_periods_per_request = 16
finish_own_jobs = true

[[chain]]
name = "sepolia"
preset = "sepolia"
layout = "gloas"
checkpoint = "$checkpoint"
adoption_parts = 2

  [[chain.source]]
  name = "beacon_api_module-lodestar"
  via = "beacon_api_module"
  chain_id = $CHAIN
  provider = "lodestar-counted"
  timeout_secs = 60

  [[chain.source]]
  name = "beacon_api_module-nimbus"
  via = "beacon_api_module"
  chain_id = $CHAIN
  provider = "nimbus-counted"
  timeout_secs = 60
T
  jq -n --arg c "$W/req/updater.toml" '{config_file: $c}' > "$W/req/updater-config.json"
  call ferrata_updater_module check_config U-check_config "@$W/req/updater-config.json"
  marks updater-start
  CALL_MS=600000 call ferrata_updater_module start U-start-1 "@$W/req/updater-config.json"
  # attest-monitor on the same instance: the instance through its relay, two providers through theirs.
  "$BIN/attest-monitor" watch --state lez:http://127.0.0.1:18702 --program "$(b58hex "$program")" \
    --instance "$id" --provider lodestar=http://127.0.0.1:18721 --provider publicnode=http://127.0.0.1:18722 --interval 60 \
    --alert-log "$W/monitor/alerts.jsonl" --status-file "$W/monitor/status.json" --state-file "$W/monitor/seen.json" \
    > "$W/monitor/monitor.log" 2>&1 & local mon=$!
  pids+=("$mon")
  marks monitor-start
  t_start=$(date +%s); t_end=$(( t_start + minutes * 60 ))
  local restarted=0 attested=0 probed=0 life=1 code restart_at=$(( t_start + minutes * 60 / 2 )) attest_at=$(( t_start + minutes * 60 * 6 / 10 ))
  while [ "$(date +%s)" -lt "$t_end" ]; do
    probed=$((probed + 1))
    # /health as check.py reads it (lez/updater/results/live-sepolia/check.py: probes*/health.jsonl), and the
    # module's status through Logos Core.
    rm -f "$W/h.json"
    code=$(curl -sS -m 10 -o "$W/h.json" -w '%{http_code}' http://127.0.0.1:9464/health 2>/dev/null) || true
    python3 -I -c 'import json, sys
t, life, code, f = sys.argv[1:5]
try:
    h = json.load(open(f))
except Exception:
    h = None
print(json.dumps({"t": t, "life": int(life), "http": int(code or 0), "health": h}, separators=(",", ":")))' \
      "$(date -u +%FT%TZ)" "$life" "${code:-0}" "$W/h.json" >> "$W/probes-u1/health.jsonl"
    call ferrata_updater_module status "status-$probed" > /dev/null 2>&1 || true
    jq -c --arg t "$(date -u +%FT%TZ)" --arg life "$life" '{t: $t, life: ($life | tonumber), status: .}' "$W/rep/status-$probed.json" \
      >> "$W/probes-u1/status.jsonl" 2>/dev/null
    rm -f "$W/rep/status-$probed.raw" "$W/rep/status-$probed.json"
    # Once the instance holds a record of its own past the checkpoint's: the attestation module's calls and one
    # attestation sent with ferrata-day1 (both on that record).
    if [ "$attested" = 0 ] && { fresh_finish "$W/updater-state/sepolia/journal.tsv" || [ "$(date +%s)" -ge "$attest_at" ]; }; then
      attest_phase "$W" "$program" "$id" "$checkpoint" && attested=1 || attested=2
    fi
    # The restart through the module, halfway: stop (the round in hand finishes), 15 s, start again.
    if [ "$restarted" = 0 ] && [ "$(date +%s)" -ge "$restart_at" ]; then
      marks restart-stop
      CALL_MS=600000 call ferrata_updater_module stop U-stop-1 || true
      sleep 15
      marks restart-start
      CALL_MS=600000 call ferrata_updater_module start U-start-2 "@$W/req/updater-config.json" || true
      restarted=1
      life=2
    fi
    sleep 60
  done
  marks session-end
  call ferrata_updater_module metrics U-metrics-final || true
  call ferrata_updater_module status U-status-final || true
  marks stop
  CALL_MS=600000 call ferrata_updater_module stop U-stop-final || true
  kill -TERM "$mon" 2>/dev/null; wait "$mon"; echo "attest-monitor exit $?" >> "$W/monitor/exit.txt"
  # The instance at the end, as the network holds it.
  "$BIN/attest-monitor" state --state "lez:$FERRATA_TESTNET_RPC" --program "$(b58hex "$program")" --instance "$id" > "$W/final-state.json" 2>&1
  ctl daemon stop >> "$W/core-stop.log" 2>&1; echo "logosctl daemon stop exit $?" >> "$W/core-stop.log"
  marks core-stopped
  cp -L "$SESSION/logs/"*.log "$W/logos-logs/" 2>/dev/null
  cp -L "$SESSION/logs/daemon.log" "$W/updater-module.log" 2>/dev/null
  cp "$W/updater-state/sepolia/journal.tsv" "$W/journal.tsv" 2>/dev/null
  cp "$W/updater-state/ledger/run.tsv" "$W/run.tsv" 2>/dev/null
  sleep 6
  cleanup
  trap - EXIT
  # The CLI's journey (lez/ATTEST-CLI.md), on its own: in-process chain, recorded Sepolia data, dev mode.
  marks cli-start
  step cli-journey bash -c "cd '$repo/lez' && RISC0_DEV_MODE=1 /usr/bin/time -v '$BIN/ferrata-attest' --follow-to 1408 \
    --message-out '$W/cli/attest-messages' config --then advance --period 1409 --then advance --period 1409 --send \
    --then advance --finality --send --then slot --then slot --send --then slot --expect-eq 0x01 --then slot \
    --expect-eq 0x01 --send --then log --then log --send --then status > '$W/cli/stdout.txt' 2> '$W/cli/stderr.txt'; \
    echo \"exit \$?\" > '$W/cli/exit.txt'"
  marks cli-end
  cp "$rep/logs/cli-journey.log" "$W/cli/step.log" 2>/dev/null
  # The updater's state (journal, the backend's manifest), without the follower's snapshot.
  find "$W/updater-state" -maxdepth 3 -type f -printf '%s %p\n' > "$W/updater-state-files.txt" 2>/dev/null
}
# The journal's last line is an applied Finish of the second job or later, at most three minutes old: the instance's
# newest record is fresh (Sepolia's execution endpoints still serve proofs for its block).
fresh_finish() {
  [ -f "$1" ] || return 1
  python3 -I -c 'import datetime, sys
rows = [l.split("\t") for l in open(sys.argv[1]).read().splitlines() if l.strip()]
done = [r for r in rows if len(r) > 10 and r[1] == "finish" and r[4] == "applied"]
if len(done) < 2 or rows[-1] is not done[-1]:
    sys.exit(1)
t = datetime.datetime.strptime(done[-1][10][:19], "%Y-%m-%dT%H:%M:%S").replace(tzinfo=datetime.timezone.utc)
sys.exit(0 if (datetime.datetime.now(datetime.timezone.utc) - t).total_seconds() < 180 else 1)' "$1"
}
# The attestation module's calls on the instance's newest record, then one attestation sent with ferrata-day1.
attest_phase() {
  local W=$1 program=$2 id=$3 checkpoint=$4 slot hash dir
  marks attest-start
  "$BIN/attest-monitor" state --state "lez:http://127.0.0.1:18702" --program "$(b58hex "$program")" --instance "$id" > "$W/attest/state.json" 2> "$W/attest/state.err" || return 1
  jq -n --slurpfile s "$W/attest/state.json" --arg c "$checkpoint" --arg id "$id" \
    '{state: $s[0].instance_state, checkpoint: $c, expected_instance_id: $id}' > "$W/req/read_instance.json"
  call ferrata_attest_module read_instance A-read_instance "@$W/req/read_instance.json" || return 1
  "$BIN/ferrata-day1" target --rpc "$FERRATA_TESTNET_RPC" --keys "$K" --state "$S" > "$W/attest/target.json" 2> "$W/attest/target.err" || return 1
  slot=$(jq -r .slot "$W/attest/target.json"); hash=$(jq -r .block_hash "$W/attest/target.json")
  note "attest: the instance's newest record: slot $slot, execution block $hash"
  python3 -I "$repo/lez-v03/scripts/fetch.py" proof "$F" "$slot" "$hash" "$VAULT" "0,1" > "$W/attest/fetch-proof.log" 2>&1 || return 1
  dir="$F/proofs/$slot"
  # The attestation signed at once (on this record; sent after the module's calls): a later update does not change it.
  step attest-sign /usr/bin/time -v "$BIN/ferrata-day1" sign --rpc "$FERRATA_TESTNET_RPC" --keys "$K" --state "$S" --stage attest --fetch "$F"
  # verify_checkpoint: the instance's checkpoint, its bootstrap, three providers, the execution check through eth_rpc_module.
  jq -n --slurpfile ri "$W/rep/A-read_instance.json" --arg c "$checkpoint" --arg id "$id" --arg b "$(python3 -I -c 'import sys; print(open(sys.argv[1], "rb").read().hex())' "$K/init/bootstrap.ssz")" \
    --argjson ch "$CHAIN" --arg l "$LODESTAR" --arg p "$PN_BEACON" --arg n "$NIMBUS" \
    '{config: $ri[0].config, checkpoint: $c, expected_instance_id: $id, bootstrap: $b,
      providers: [{name: "lodestar", url: $l}, {name: "publicnode", url: $p}, {name: "nimbus", url: $n}], min_providers: 2,
      execution: {via: "eth_rpc_module", chain_id: $ch}, timeout_secs: 60}' > "$W/req/verify_checkpoint.json"
  call ferrata_attest_module verify_checkpoint A-verify_checkpoint "@$W/req/verify_checkpoint.json"
  # A log of the record's block, read through eth_rpc_module; the record's fields from the block.
  jq -n --argjson b "$(jq -r '.result.number' "$dir/block.json" | xargs printf '%d')" --argjson ch "$CHAIN" \
    '{block: $b, block_log_index: 0, eth: {via: "eth_rpc_module", chain_id: $ch}, timeout_ms: 120000}' > "$W/req/fetch_log_proof.json"
  call ferrata_attest_module fetch_log_proof A-fetch_log_proof "@$W/req/fetch_log_proof.json"
  jq -n --slurpfile blk "$dir/block.json" --argjson ch "$CHAIN" --argjson s "$slot" \
    '$blk[0].result as $b | {chain_id: $ch, anchor_slot: $s,
      block_number: ($b.number | ltrimstr("0x") | explode | reduce .[] as $c (0; . * 16 + (if $c >= 97 then $c - 87 else $c - 48 end))),
      timestamp: $b.timestamp, block_hash: $b.hash, parent_hash: $b.parentHash, state_root: $b.stateRoot,
      receipts_root: $b.receiptsRoot}' > "$W/req/record.json"
  jq -n --slurpfile r "$W/req/record.json" --slurpfile p "$dir/proof.json" --argjson ch "$CHAIN" --arg slot "$SLOT0" \
    '{chain_id: $ch, record: $r[0], storage: {slot: $slot, get_proof: $p[0].result}}' > "$W/req/dry_run_storage.json"
  call ferrata_attest_module dry_run_attest A-dry_run_storage "@$W/req/dry_run_storage.json"
  jq -n --slurpfile r "$W/req/record.json" --argjson ch "$CHAIN" --arg v "$VAULT" \
    --arg slot "$SLOT1" '{chain_id: $ch, record: $r[0], storage: {address: $v, slot: $slot, eth: {via: "eth_rpc_module", chain_id: $ch}}}' \
    > "$W/req/dry_run_storage_eth.json"
  call ferrata_attest_module dry_run_attest A-dry_run_storage_eth "@$W/req/dry_run_storage_eth.json"
  if jq -e '.log' "$W/rep/A-fetch_log_proof.json" > /dev/null 2>&1; then
    jq -n --slurpfile r "$W/req/record.json" --slurpfile l "$W/rep/A-fetch_log_proof.json" --argjson ch "$CHAIN" \
      '{chain_id: $ch, record: $r[0], log: $l[0].log}' > "$W/req/dry_run_log.json"
    call ferrata_attest_module dry_run_attest A-dry_run_log "@$W/req/dry_run_log.json"
    jq -n --slurpfile l "$W/rep/A-fetch_log_proof.json" --arg p "$program" --arg id "$id" --argjson ch "$CHAIN" --argjson s "$slot" \
      '{program_id: $p, instance_id: $id, chain_id: $ch, slot: $s, log: $l[0].log}' > "$W/req/build_attest_tx.json"
    call ferrata_attest_module build_attest_tx A-build_attest_tx "@$W/req/build_attest_tx.json"
  fi
  marks attest-module-end
  # One attestation sent to the network: the vault's slots 0 and 1 under that record (the day-one kit's stage d).
  step attest-send /usr/bin/time -v "$BIN/ferrata-day1" send --rpc "$FERRATA_TESTNET_RPC" --keys "$K" --state "$S" --stage attest
  cp "$rep/logs/attest-sign.log" "$rep/logs/attest-send.log" "$W/attest/" 2>/dev/null
  cp "$K/tx/attest/"*.json "$W/attest/" 2>/dev/null
  marks attest-end
}

case "$cmd" in
  build) do_build ;;
  build-bridge) do_build_bridge ;;
  bridge-modules) do_bridge_modules "${3:-10}" ;;
  instance) do_instance ;;
  session) do_session "${3:-90}" ;;
  collect) python3 -I "$here/live_testnet.py" collect "$out" "${3:?RECORD directory}" || failures=$((failures + 1)) ;;
  *) die "$cmd: build, instance, session or collect" ;;
esac
note "live-testnet.sh $cmd: $failures failed steps"
exit $(( failures > 0 ? 1 : 0 ))
