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
#   FERRATA_LGX=DIR modules/scripts/send-stack.sh run OUT
#                                                one headless Logos Core (OUT/logos, or LOGOSCTL) with them and
#                                                ferrata_monitor_module and ferrata_relayer_module (DIR/lgx-<name>/: check.sh's
#                                                OUT) on a local anvil (31337; nothing public): the two named custodians
#                                                (keystore_module's and signer_manager_module's `configure`), `custody` (the
#                                                key imported, unlocked for tx_sender_module without confirmation), then
#                                                each daemon transaction twice from one anvil snapshot, the plain path
#                                                (alloy) and --send-via logos (tx_sender_module): the relayer's
#                                                submitCandidate, finalizeAttested, release and claimOwed, the monitor's
#                                                cancelCandidate and pauseCheckpoints; the fallback. OUT/calls.jsonl,
#                                                OUT/run/ (each step's transactions, receipts, vault state), OUT/run/compare.md
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
tx_sender_module logos-evm-tx-sender-module f1754c3b13c7c580ee449116ba8982ca193e114d
evm_signer_cli logos-evm-signer-cli 2cb46d8d2a8456afa01b26906ffb206d064e26ec
evm_keystore_cli logos-evm-keystore-cli c6db2ad320f6e14d938d63d5d3fe4977ec909ca5"
# evm_signer_cli and evm_keystore_cli (the headless approver and custodian, pinning the same signer manager, keystore
# and builder a99b32a as tx_sender_module) are built the same way; modules/scripts/bridge-send-stack.sh uses them.
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
  : "${LOGOSCTL:=$OUT/logos/bin/logosctl}"
  : "${FERRATA_LGX:?a directory with lgx-ferrata_monitor_module/ and lgx-ferrata_relayer_module/ (check.sh OUT)}"
  local LT="${LOGOS_TMPDIR:-/var/tmp/ss}" S="${OUT:?}/session" R="${OUT:?}/run" anvil_pid rpc=http://127.0.0.1:18545
  mkdir -p "$LT"; rm -rf "${S:?}" "${R:?}" "${OUT:?}/home"; mkdir -p "$OUT/home" "$R/keys"; chmod 700 "$R/keys"
  : > "$OUT/calls.jsonl"; : > "$R/steps.jsonl"
  anvil --port 18545 --chain-id 31337 > "$R/anvil.log" 2>&1 & anvil_pid=$!
  for _ in $(seq 60); do cast chain-id --rpc-url "$rpc" > /dev/null 2>&1 && break; sleep 0.5; done
  # The chain's clock is the run's, not the wall's: 12 s per block, so both branches of a step mine at the same times.
  # (Unpinned, they did not: the vault's `submittedAt` and the cost of `keylessFrom = block.timestamp`, 2,800 gas
  # cheaper when the value is unchanged, then differ between the paths for reasons that are anvil's clock.)
  cast rpc --rpc-url "$rpc" anvil_setBlockTimestampInterval 12 > /dev/null
  ctl() { TMPDIR="$LT" "$LOGOSCTL" --config-dir "$S" "$@"; }
  # c MODULE METHOD ARGS...: one call through Logos Core, recorded in calls.jsonl; LAST holds its result. Keys and
  # passwords travel only as file paths.
  c() {
    local m="$1" f="$2" out rc; shift 2
    out="$(ctl --timeout 900000 call "$m" "$f" "$@" --json < /dev/null 2>&1)"; rc=$?
    jq -c -n --args '$ARGS.positional' -- "$@" > "$OUT/.args" 2>/dev/null || echo '[]' > "$OUT/.args"
    jq -c -n --arg m "$m" --arg f "$f" --slurpfile a "$OUT/.args" --arg o "$out" --argjson rc "$rc" \
      '{module: $m, method: $f, args: $a[0], exit: $rc,
        reply: ($o | fromjson? // $o | if type == "object" and (.result | type) == "string" then .result |= (fromjson? // .) else . end)}' \
      >> "$OUT/calls.jsonl"
    LAST=$(jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else (.result // .) end' <<< "$out" 2>/dev/null || echo "$out")
    echo "$m.$f -> exit $rc: ${LAST:0:400}"
  }
  step_log() {
    jq -c -n --arg s "$1" --arg ok "$2" --arg note "${3:-}" '{step: $s, ok: ($ok == "1"), note: $note}' >> "$R/steps.jsonl"
    echo "== $1: $([ "$2" = 1 ] && echo ok || echo FAILED) ${3:-}"
  }

  # ---- the devnet: keys, the vault, ETH registered, a deposit ------------------------------------------------
  mapfile -t accts < <(cast rpc --rpc-url "$rpc" eth_accounts | jq -r '.[]')
  local deployer=${accts[0]} admin=${accts[1]} prover=${accts[2]} anyone=${accts[3]} fees=${accts[4]} recipient=${accts[5]}
  local freezer=${accts[6]} WATCHER_KEY RELAYER_KEY watcher relayer pw="$R/keys/password"
  WATCHER_KEY=$(cast wallet new | awk '/Private key/ {print $3}'); RELAYER_KEY=$(cast wallet new | awk '/Private key/ {print $3}')
  watcher=$(cast wallet address --private-key "$WATCHER_KEY"); relayer=$(cast wallet address --private-key "$RELAYER_KEY")
  ( umask 077; echo "$WATCHER_KEY" > "$R/keys/watcher.key"; echo "$RELAYER_KEY" > "$R/keys/relayer.key"; echo "ferrata devnet only" > "$pw" )
  for a in "$watcher" "$relayer"; do cast rpc --rpc-url "$rpc" anvil_setBalance "$a" 0x56bc75e2d63100000 > /dev/null; done # 100 ETH, local
  # The relayer's plain path signs with a Web3 keystore (as it does on Sepolia): the same key, encrypted.
  cast wallet import relayer --private-key "$RELAYER_KEY" --keystore-dir "$R/keys" --unsafe-password "$(cat "$pw")" \
    > "$R/keystore-import.log" 2>&1
  [ -f "$R/keys/relayer" ] || { echo "no relayer keystore: $(cat "$R/keystore-import.log")"; return 1; }
  label() { cast keccak "ferrata/send-stack/$1"; }
  local zero=0x0000000000000000000000000000000000000000000000000000000000000000 eth=0x0000000000000000000000000000000000000000
  local genesis vault deploy_block cid
  genesis=$(label genesis)
  ( cd "$repo/vault" && FIXTURE_MODE_A=false FIXTURE_GENESIS=$genesis FIXTURE_ADMIN=$admin FIXTURE_FREEZER=$freezer \
    FIXTURE_WATCHER=$watcher FIXTURE_FEES=$fees forge script script/MonitorFixture.s.sol:MonitorFixture --rpc-url "$rpc" --slow \
    --broadcast --unlocked --sender "$deployer" ) > "$R/forge.log" 2>&1 || { tail -n 30 "$R/forge.log"; return 1; }
  vault=$(cast to-check-sum-address "$(jq -r '.transactions[] | select(.contractName=="FerrataVault") | .contractAddress' \
    "$repo/vault/broadcast/MonitorFixture.s.sol/31337/run-latest.json" | tail -1)")
  deploy_block=$(cast block-number --rpc-url "$rpc"); cid=$(cast call --rpc-url "$rpc" "$vault" "lezConfigId()(bytes32)")
  local max=115792089237316195423570985008687907853269984665640564039457584007913129639935
  local denom=100000000000000000 relay_fee=1000000000000000 cp_fee=100000000000000
  if cast send --unlocked --from "$admin" --rpc-url "$rpc" "$vault" \
    "registerToken(address,(uint256[],uint256[],uint256,uint256,(uint256,uint256),(uint256,uint256),uint256,bytes32,uint8,uint256[]))" \
    "$eth" "([$denom],[$relay_fee],1000000000,1,($max,0),($max,0),0,$(label lez-eth),18,[$cp_fee])" > "$R/register.log" 2>&1
  then step_log register-eth 1; else step_log register-eth 0 "$(tail -n 3 "$R/register.log")"; fi
  if cast send --unlocked --from "$anyone" --rpc-url "$rpc" --value $((denom + relay_fee)) "$vault" "depositETH(uint8,bytes32,uint64)" \
    0 "$(label commitment/0)" 0 > "$R/deposit.log" 2>&1
  then step_log deposit 1; else step_log deposit 0 "$(tail -n 3 "$R/deposit.log")"; fi
  jq -n --arg rpc "$rpc" --arg vault "$vault" --arg cid "$cid" --argjson deployBlock "$deploy_block" --arg watcher "$watcher" \
    --arg relayer "$relayer" --arg recipient "$recipient" --arg prover "$prover" --arg anvil "$(anvil --version | head -n 1)" \
    '{chainId: 31337, rpc: $rpc, vault: $vault, lezConfigId: $cid, deployBlock: $deployBlock, watcher: $watcher,
      relayer: $relayer, recipient: $recipient, prover: $prover, anvil: $anvil}' > "$R/devnet.json"

  # ---- Logos Core: Logos's send stack and verified proxy, Ferrata's monitor and relayer -------------------------
  HOME="$OUT/home" TMPDIR="$LT" "$LOGOSCTL" --config-dir "$S" daemon start --detach > "$OUT/daemon.txt" 2>&1
  {
    local d m
    for d in "$OUT"/lgx-*/ "$FERRATA_LGX/lgx-ferrata_monitor_module" "$FERRATA_LGX/lgx-ferrata_relayer_module"; do
      ctl install "$d"/*.lgx -y 2>&1 | tail -n 1
    done
    for m in tx_sender_module verified_proxy_module ferrata_monitor_module ferrata_relayer_module; do ctl module load "$m" 2>&1 | tail -n 2; done
    ctl module list --json > "$OUT/modules-loaded.json" 2>&1; jq -c '[.[] | {name, status, version}]' "$OUT/modules-loaded.json"
    c eth_rpc_module set_chain_config 31337 "{\"endpoint\":\"$rpc\",\"name\":\"anvil\",\"testnet\":true}"
    c eth_rpc_module block_number 31337
    # The patched glue at run time: in `required` mode the gate asks verified_proxy_module's `status` through the
    # generated client (glue.rs `proxy_status`); a devnet has no beacon chain, so the proxy is not usable and the
    # call is refused rather than answered directly (fail closed). Then the chain goes back to `off`.
    c eth_rpc_module set_verified_proxy_mode 31337 required
    c eth_rpc_module verified_proxy_status 31337
    c eth_rpc_module block_number 31337
    c eth_rpc_module set_verified_proxy_mode 31337 off
    c eth_rpc_module verified_proxy_status 31337
    # Roles: both modules' `configure` is ungated upstream (keystore e83a899 glue.rs 36-45, signer manager bd5c84d
    # glue.rs 27-30) and TOTAL: the defaults stay, the two Ferrata daemon modules are added as custodians.
    c keystore_module caller_identity
    c keystore_module configure '{"approvers":["evm_signer_ui"],"custodians":["evm_keystore_ui","ferrata_monitor_module","ferrata_relayer_module"],"managers":["signer_manager_module"]}'
    c signer_manager_module caller_identity
    c signer_manager_module configure '{"approvers":["evm_signer_ui"],"custodians":["evm_keystore_ui","ferrata_monitor_module","ferrata_relayer_module"],"signers":["keystore_module"]}'
    c ferrata_monitor_module custody "{\"key_file\":\"$R/keys/watcher.key\",\"password_file\":\"$pw\",\"ttl_ms\":7200000}"
    c ferrata_relayer_module custody "{\"key_file\":\"$R/keys/relayer.key\",\"password_file\":\"$pw\",\"ttl_ms\":7200000}"
    c signer_manager_module accounts
    c signer_manager_module unattended_signatures
    c tx_sender_module prepare "{\"chainId\":31337,\"from\":\"$watcher\",\"calls\":[{\"to\":\"$vault\",\"data\":\"$(cast calldata 'pauseCheckpoints()')\"}]}"
  } > "$R/logos-setup.log" 2>&1
  cat "$R/logos-setup.log"

  # ---- each transaction twice from one state: the plain path, then (anvil reverted) through tx_sender_module --
  local view="$R/node-view.json" blocks='{}' i
  for i in $(seq 1 20); do blocks=$(jq -c --arg k "$i" --arg h "$(label "block/$i")" '. + {($k): $h}' <<< "$blocks"); done
  jq -n --argjson b "$blocks" --arg k1 "$(label key/1)" --arg k2 "$(label key/2)" --arg k3 "$(label key/3)" --arg k4 "$(label key/4)" \
    '{node: "lez-honest", finalHeight: 20, finalBlocks: $b, keySets: [{fromBlock: 0, keys: [$k1, $k2, $k3, $k4], postingTimeframe: 900}]}' > "$view"
  # cp_tuple PARENT NEWSTATE TIPID TIPHASH BURNROOT BURNCOUNT: a Checkpoint for cast; cp_journal: its 17-word journal
  # (relayer.rs `checkpoint_journal`), what the relayer's checkpoint job carries.
  cp_tuple() { echo "(3,2,$1,$2,$5,$6,0,false,$zero,$3,$4,4,3)"; }
  cp_journal() {
    cast abi-encode "f(bytes32,uint256,address,bytes32,uint256,uint256,bytes32,bytes32,bytes32,uint256,uint256,uint256,bytes32,uint256,bytes32,uint256,uint256)" \
      0xbdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7 31337 "$vault" "$cid" 3 2 "$1" "$2" "$5" "$6" 0 0 "$zero" "$3" "$4" 4 3
  }
  block_now() { cast block-number --rpc-url "$rpc"; }
  # warp SECONDS: one block that many seconds after the latest.
  warp() {
    cast rpc --rpc-url "$rpc" evm_setNextBlockTimestamp $(( $(cast block latest --field timestamp --rpc-url "$rpc") + $1 )) > /dev/null
    cast rpc --rpc-url "$rpc" evm_mine > /dev/null
  }
  probe() {
    jq -n --arg final "$(cast call --rpc-url "$rpc" "$vault" 'finalState()(bytes32)')" \
      --arg count "$(cast call --rpc-url "$rpc" "$vault" 'checkpointCount()(uint256)')" \
      --arg cand1 "$(cast call --rpc-url "$rpc" "$vault" 'candidates(bytes32)(bytes32,bytes32,uint64,uint32,uint48,uint32,uint64,address)' "$(label state/1)" | tr '\n' ' ')" \
      --arg cand2 "$(cast call --rpc-url "$rpc" "$vault" 'candidates(bytes32)(bytes32,bytes32,uint64,uint32,uint48,uint32,uint64,address)' "$(label state/2)" | tr '\n' ' ')" \
      --arg rootEntry "$(cast call --rpc-url "$rpc" "$vault" 'burnRootCheckpoint(bytes32)(uint256)' "$(label burn-root/1)")" \
      --arg nullifierUsed "$(cast call --rpc-url "$rpc" "$vault" 'nullifierUsed(bytes32)(bool)' "$(label nullifier/1)")" \
      --arg escrowed "$(cast call --rpc-url "$rpc" "$vault" 'escrowed(address)(uint256)' "$eth")" \
      --arg owedRelayer "$(cast call --rpc-url "$rpc" "$vault" 'owed(address,address)(uint256)' "$relayer" "$eth")" \
      --arg frozenUntil "$(cast call --rpc-url "$rpc" "$vault" 'checkpointsFrozenUntil()(uint256)' 2>&1)" \
      --arg recipientBalance "$(cast balance --rpc-url "$rpc" "$recipient")" \
      --arg vaultBalance "$(cast balance --rpc-url "$rpc" "$vault")" \
      '{finalState: $final, checkpointCount: $count, candidate_state1: $cand1, candidate_state2: $cand2,
        burnRootEntry_root1: $rootEntry, nullifierUsed_1: $nullifierUsed, escrowedETH: $escrowed, owedRelayerETH: $owedRelayer,
        checkpointsFrozenUntil: $frozenUntil, recipientBalance: $recipientBalance, vaultBalance: $vaultBalance}'
  }
  # record DIR B0 B1: every transaction mined in blocks (B0, B1] (only the daemon sends there), its receipt, then the probe.
  record() {
    local dir=$1 b0=$2 b1=$3 b h
    mkdir -p "$dir"; : > "$dir/txs.jsonl"
    for b in $(seq $((b0 + 1)) "$b1"); do
      for h in $(cast block "$b" --json --rpc-url "$rpc" | jq -r '.transactions[]'); do
        jq -c -n --argjson tx "$(cast tx "$h" --json --rpc-url "$rpc")" --argjson rc "$(cast receipt "$h" --json --rpc-url "$rpc")" \
          '{tx: $tx, receipt: $rc}' >> "$dir/txs.jsonl"
      done
    done
    probe > "$dir/probe.json"
  }
  # ab STEP PLAIN LOGOS: snapshot; the plain run (function PLAIN DIR); recorded; revert; the run through Logos Core
  # (function LOGOS DIR); recorded. Only the Logos branch goes on.
  ab() {
    local step=$1 snap b0 b1 t0
    snap=$(cast rpc --rpc-url "$rpc" evm_snapshot | tr -d '"'); b0=$(block_now)
    t0=$(cast block latest --field timestamp --rpc-url "$rpc")
    cast rpc --rpc-url "$rpc" evm_setNextBlockTimestamp $((t0 + 12)) > /dev/null
    "$2" "$R/$step/plain" > "$R/$step-plain.log" 2>&1; b1=$(block_now); record "$R/$step/plain" "$b0" "$b1"
    cast rpc --rpc-url "$rpc" evm_revert "$snap" > /dev/null
    [ "$(block_now)" = "$b0" ] || echo "$step: the revert left block $(block_now), not $b0"
    cast rpc --rpc-url "$rpc" evm_setNextBlockTimestamp $((t0 + 12)) > /dev/null
    "$3" "$R/$step/logos" > "$R/$step-logos.log" 2>&1; b1=$(block_now); record "$R/$step/logos" "$b0" "$b1"
    step_log "$step" "$([ -s "$R/$step/plain/txs.jsonl" ] && [ -s "$R/$step/logos/txs.jsonl" ] && echo 1 || echo 0)" \
      "plain $(wc -l < "$R/$step/plain/txs.jsonl") tx, logos $(wc -l < "$R/$step/logos/txs.jsonl") tx"
  }
  relayer_args_plain() { echo "--eth-rpc $rpc --vault $vault --chain-id 31337 --from-block $deploy_block --keystore $R/keys/relayer --password-file $pw"; }
  relayer_args_logos() { echo "--eth-rpc $rpc --vault $vault --chain-id 31337 --from-block $deploy_block --send-via logos --from $relayer"; }
  # Both daemons run as they run in production: `start` (the daemon in its own thread; the method returns), then
  # `status` until it ends by its bound (--rounds, --max-polls). A daemon thread's call to tx_sender_module needs the
  # module's Qt thread free (logos-rust-sdk src/plugin.rs 212-222), which a blocking `run` would hold: the modules'
  # `run` refuses --send-via logos.
  # until_done MODULE STATUS-ARG...: poll `status` until the daemon no longer runs; LAST: the final status.
  until_done() {
    local m=$1 st; shift
    for _ in $(seq 1200); do
      st=$(ctl --timeout 60000 call "$m" status "$@" --json < /dev/null 2>/dev/null |
        jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' 2>/dev/null)
      [ "$(jq -r '.running' <<< "$st" 2>/dev/null)" = true ] || break
      sleep 0.25
    done
    LAST=$st
  }
  # relayer_round DIR STATE ARGS...: one round of the relayer module (its inbox DIR/inbox with JOB, its state STATE).
  relayer_round() {
    local dir=$1 state=$2; shift 2
    mkdir -p "$dir/inbox"; if [ -n "${JOB:-}" ]; then printf '%s\n' "$JOB" > "$dir/inbox/job.json"; fi
    c ferrata_relayer_module start "$(jq -c -n --args '{args: $ARGS.positional}' -- "$@" --state "$state" run --inbox "$dir/inbox" --rounds 1 --poll-secs 1)"
    printf '%s\n' "$LAST" > "$dir/start.json"
    until_done ferrata_relayer_module '{"since":0}'
    printf '%s\n' "$LAST" > "$dir/reply.json"
  }
  # The plain branch starts from a copy of the Logos branch's relayer state (the branch that goes on).
  rp() { mkdir -p "$1/state"; cp -a "$R/relayer-logos-state/." "$1/state/"; relayer_round "$1" "$1/state" $(relayer_args_plain); }
  rl() { relayer_round "$1" "$R/relayer-logos-state" $(relayer_args_logos); }
  mkdir -p "$R/relayer-logos-state"
  local s1 s2 j2 rj
  s1=$(label state/1)
  # R1: the relayer submits a rule (B) checkpoint (its prover's job; the fixture's verifier accepts the empty seal).
  JOB=$(jq -c -n --arg j "$(cp_journal "$genesis" "$s1" 10 "$(label block/10)" "$(label burn-root/1)" 1)" '{kind: "checkpoint", journal: $j, seal: "0x"}')
  ab R1-submitCandidate rp rl
  # R2: after the vault's wait (1 h), the relayer finalises it (finalizeAttested).
  JOB=""; warp 3601
  ab R2-finalizeAttested rp rl
  # R3: a holder's release job (journal, empty seal): the relayer releases the deposit to the recipient, is paid the
  # relay fee, and collects the checkpoint fee the release credited to it (claimOwed, in the same round).
  rj=$(cast abi-encode "f(bytes32,uint256,address,bytes32,bytes32,bytes32,address,uint256,address,address,uint256)" \
    0x6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e51 31337 "$vault" "$cid" "$(label burn-root/1)" \
    "$(label nullifier/1)" "$eth" "$denom" "$recipient" "$relayer" "$relay_fee")
  JOB=$(jq -c -n --arg j "$rj" '{kind: "release", journal: $j, seal: "0x"}')
  ab R3-release rp rl
  JOB=""
  # M1: a wrong candidate (its tip 12 conflicts with the node's final tier); the monitor cancels it.
  s2=$(label state/2); j2=$(cp_tuple "$s1" "$s2" 12 "$(label forged/12)" "$(label burn-root/2)" 2)
  submit() {
    cast send --unlocked --from "$prover" --rpc-url "$rpc" "$vault" \
      "submitCandidate((uint32,uint32,bytes32,bytes32,bytes32,uint64,uint64,bool,bytes32,uint64,bytes32,uint32,uint32),bytes)" "$1" 0x > /dev/null
  }
  monargs() { echo "--rpc $rpc --vault $vault --node file:$view --start-block $deploy_block --poll 1 --confirmations 0 --margin 120 --alert-level critical --max-polls 2"; }
  monitor_run() { # DIR STATE ARGS...: the monitor module's daemon until its --max-polls are done
    local dir=$1 state=$2; shift 2; mkdir -p "$dir"
    c ferrata_monitor_module start "$(jq -c -n --args '{args: $ARGS.positional}' -- "$@" --state "$state" --alert-log "$dir/alerts.jsonl")"
    printf '%s\n' "$LAST" > "$dir/start.json"
    until_done ferrata_monitor_module
    printf '%s\n' "$LAST" > "$dir/reply.json"
  }
  # The plain branch starts from a copy of the Logos branch's state file (its cancellation count).
  mp() { mkdir -p "$1"; cp "$R/monitor-logos.json" "$1/state.json" 2>/dev/null || true; monitor_run "$1" "$1/state.json" $(monargs) --key-file "$R/keys/watcher.key"; }
  ml() { monitor_run "$1" "$R/monitor-logos.json" $(monargs) --send-via logos --from "$watcher"; }
  # `run` (the blocking method) with --send-via logos is refused, with the reason.
  c ferrata_monitor_module run "$(jq -c -n --args '{args: $ARGS.positional}' -- $(monargs) --state "$R/refused.json" --send-via logos --from "$watcher")" > "$R/run-refused.log" 2>&1
  submit "$j2" && ab M1-cancelCandidate mp ml
  # M2: the same wrong state submitted again: cancelled again, and the watcher pauses checkpoints (pauseCheckpoints).
  submit "$j2" && ab M2-cancel-and-pause mp ml
  # F: the fallback. tx_sender_module unloaded: holding the watcher key as well, the monitor sends the plain way.
  {
    local s3 j3 b0
    s3=$(label state/3); j3=$(cp_tuple "$s1" "$s3" 13 "$(label forged/13)" "$(label burn-root/3)" 3)
    warp 43300 # past the pause
    submit "$j3"
    ctl module unload tx_sender_module 2>&1 | tail -n 2
    b0=$(block_now)
    monitor_run "$R/F-fallback" "$R/monitor-logos.json" $(monargs) --send-via logos --from "$watcher" --key-file "$R/keys/watcher.key"
    record "$R/F-fallback" "$b0" "$(block_now)"
    step_log F-fallback "$([ -s "$R/F-fallback/txs.jsonl" ] && echo 1 || echo 0)" \
      "$(jq -c 'select(.kind=="cancelled") | .via' "$R/F-fallback/alerts.jsonl" 2>/dev/null | tail -n 1)"
  } > "$R/F-fallback.log" 2>&1

  cp "$S"/logs/*.log "$OUT/" 2>/dev/null || true
  ctl daemon stop > /dev/null 2>&1; kill "$anvil_pid" 2>/dev/null
  # Names for the comparison: the vault's selectors (forge's methodIdentifiers) and its events' topic0.
  local art="$repo/vault/out/FerrataVault.sol/FerrataVault.json"
  jq '.methodIdentifiers' "$art" > "$R/selectors.json"
  jq -r 'def t: if (.type | startswith("tuple")) then "(" + ([.components[] | t] | join(",")) + ")" + (.type | ltrimstr("tuple")) else .type end;
         .abi[] | select(.type == "event") | .name + "(" + ([.inputs[] | t] | join(",")) + ")"' "$art" |
    while read -r sig; do printf '%s %s\n' "$(cast keccak "$sig")" "$sig"; done > "$R/events.txt"
  python3 -I "$here/send_stack_compare.py" "$R" > "$R/compare.md" 2> "$R/compare.err"; echo "compare exit $?"
  cat "$R/steps.jsonl"
}

case "$MODE" in build) build ;; run) run ;; *) sed -n 2,30p "$0"; exit 2 ;; esac
