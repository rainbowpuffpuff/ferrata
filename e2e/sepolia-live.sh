#!/usr/bin/env bash
# The return leg on Sepolia from the first transaction to the last check, unattended and resumable.
# Start it once; started again after an interruption it reads the chain, sees which stages are
# complete and continues after the last one. It sends exactly the transactions of vault/README.md
# ("Sepolia round trip") in their nonce order and stops with a message at the first thing that is not
# as planned. Every gas limit is the node's estimate with a margin, and a stage whose transactions the
# node cannot price within the caps sends nothing (vault/script/roundtrip.sh, e2e/gasplan.py).
#
#   e2e/sepolia-live.sh [run]          the run directory under e2e/runs/ (default sepolia-live-2;
#                                      sepolia-live is the attempt of 10 November that ran out of gas)
#
# Stages: pre-flight, deploy (8 transactions), one stage per deposit (1 each; after a deposit whose root
# LEZ attests, the wait for its block's finality and the capture of what LEZ needs), submit (1), the
# vault's wait, beacon (1), the
# finalisation proof for that beacon (inputs, GPU proof, the two wrap steps, fixture), finalize (1),
# the release delay, release (2), vault/script/verify-sepolia.sh. Everything it prints also goes
# to <run>/sepolia-live.log; completed stages are listed in <run>/stages.done.
#
# Configuration comes from the environment, or from the file $FERRATA_ENV_FILE (default
# e2e/sepolia-live.env, not committed; e2e/sepolia-live.env.example lists the variables):
#   SEPOLIA_RPC_URL, ETH_FROM                 the endpoint and the sender (the fixture's deployer)
#   ETH_KEYSTORE + ETH_PASSWORD               paths of a keystore file and of a file with its password
#   (or ETH_KEYSTORE_ACCOUNT)                 (Foundry's own variables: the key is never on a command line)
#   MODAL_PROFILE                             the profile that runs gpu-prover/modal_app.py
#   FERRATA_MRUN                              optional: a wrapper that runs a shell command on a build
#                                             machine with this repository at the same path
#                                             (`<wrapper> [--put P] [--get P] -- '<command>'`); without
#                                             it cargo and r0vm run here
#   FERRATA_MWRAP                             optional: `<wrapper> input.json proof.json` runs RISC Zero's
#                                             Groth16 prover image elsewhere; without it Docker runs it here
#   FERRATA_GPU_CMD                           optional: `<command> <inputs dir> <proof> <job> <out dir>` proves
#                                             the staged inputs on a GPU and leaves prove.json and
#                                             receipt-succinct.bin in <out dir>; without it the GPU app
#                                             gpu-prover/modal_app.py runs under MODAL_PROFILE
#   FERRATA_FORK=1                            a rehearsal on a local fork (anvil): the sender is
#                                             impersonated, no key is read, and the two waits are passed
#                                             by moving the fork's clock
#   FERRATA_HALT_AFTER=<stage>                stop cleanly after deploy, deposit0-2, capture1-2, submit, beacon,
#                                             proof, finalize or release (to do the rest later, or by hand)
#   FERRATA_GAS_RPC, FERRATA_GAS_MARGIN_PCT   the node that prices the transactions and the margin (125 %)
#                                             (vault/script/roundtrip.sh)
#   FERRATA_ATTEST_DEPOSITS                   the deposits whose root LEZ attests (default "0 1"): after
#                                             deposit N the run waits for its block's finality and captures
#                                             into <run>/lez-capture-<N+1>/ what the LEZ session reads (the
#                                             latest finality update, the vault's slots 0-2 proven at its
#                                             block, the deposit logs, the period updates) with
#                                             FERRATA_CAPTURE_CMD (default: the command of the inbound plan,
#                                             e2e/runs/<run>/lez-inputs.json); on a fork nothing is captured
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
name="${1:-sepolia-live-2}"
run="$here/runs/$name"
export TMPDIR="${TMPDIR:-$repo/../tmp}" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
mkdir -p "$TMPDIR"
envfile="${FERRATA_ENV_FILE:-$here/sepolia-live.env}"
if [ -f "$envfile" ]; then set -a; . "$envfile"; set +a; fi
[ -f "$run/fixture.json" ] || { echo "no fixture in $run" >&2; exit 2; }
exec > >(tee -a "$run/sepolia-live.log") 2>&1

say() { echo "[$(date -u +%FT%TZ)] $*"; }
die() { say "STOP: $*"; say "Nothing more is sent. Fix the cause and start this script again: it continues after the last complete stage."; exit 1; }
trap 'say "interrupted or failed at line $LINENO"' ERR

: "${SEPOLIA_RPC_URL:?set SEPOLIA_RPC_URL}" "${ETH_FROM:?set ETH_FROM}"
fixture="$run/fixture.json"
manifest="$run/manifest.json"
done_file="$run/stages.done"
proof=cp-e2e-finalize
export FERRATA_FIXTURE="$fixture" FERRATA_MANIFEST="$manifest"
rpc=(--rpc-url "$SEPOLIA_RPC_URL")
# Local: a loopback host without a userinfo (in `http://127.0.0.1:1@HOST` cast would connect to HOST).
case "$SEPOLIA_RPC_URL" in *@*) local_endpoint=0 ;; http://127.0.0.1:* | http://localhost:*) local_endpoint=1 ;; *) local_endpoint=0 ;; esac
if [ "${FERRATA_FORK:-}" = 1 ]; then
  [ "$local_endpoint" = 1 ] || die "FERRATA_FORK=1 is for a local endpoint, not $SEPOLIA_RPC_URL"
  export FERRATA_UNLOCKED=1
  unset ETH_KEYSTORE ETH_PASSWORD ETH_KEYSTORE_ACCOUNT
else
  [ "${FERRATA_UNLOCKED:-}" != 1 ] || die "FERRATA_UNLOCKED=1 only goes with FERRATA_FORK=1"
  [ -n "${ETH_KEYSTORE_ACCOUNT:-}" ] || { [ -f "${ETH_KEYSTORE:-}" ] && [ -f "${ETH_PASSWORD:-}" ]; } ||
    die "no signer: set ETH_KEYSTORE and ETH_PASSWORD to the keystore file and the password file, or ETH_KEYSTORE_ACCOUNT"
fi
for tool in cast forge jq sha256sum; do command -v "$tool" > /dev/null || die "$tool is not installed"; done

vault="$(jq -r .vault "$fixture")"
state="$(jq -r .newState "$fixture")"
first="$(jq -r '.first_nonce // 0' "$run/params.json")"
releases="$(jq -r .releaseCount "$fixture")"
deposits="$(jq -r .depositCount "$fixture")"
# Nonce offsets from the first: deploy 8 (vault/script/RoundTrip.s.sol deploy()), one per deposit, then the
# candidate, the beacon, the finalisation and the releases.
n_deploy=8
o_submit=$((n_deploy + deposits)); o_beacon=$((o_submit + 1)); o_final=$((o_submit + 2)); o_rel=$((o_submit + 3))
total=$((o_rel + releases))
attest="${FERRATA_ATTEST_DEPOSITS-0 1}"
# The default lives in its own variable: inside ${FERRATA_CAPTURE_CMD:-...} the '}' of '{vault}' would close the
# expansion early (the command got '{vault:0=...' and every capture of the 10 Nov run failed).
default_capture_cmd='python3 -I lez/scripts/fetch-attest-fixture.py --network sepolia --storage {vault}:0=depositRoot(),1=nextLeafIndex(),2=storedLezConfigId() --deposits {vault}:{from_block} --updates-from 1410 {out}'
capture_cmd="${FERRATA_CAPTURE_CMD:-$default_capture_cmd}"
call() { cast call "$vault" "$@" "${rpc[@]}"; }
word() { awk 'NR==1 {print $1}'; }
nonce() { cast nonce "$ETH_FROM" "${rpc[@]}"; }
now() { cast block latest --field timestamp "${rpc[@]}"; }
deployed() { [ "$(cast code "$vault" "${rpc[@]}")" != 0x ]; }
candidate() { # prints: t submittedAt deltaAtSubmit
  call 'candidates(bytes32)(bytes32,bytes32,uint64,uint64,bool,uint32,uint64,bytes32,uint256,uint256,uint256)' "$state" |
    sed -n '6p;9p;10p' | awk '{print $1}' | tr '\n' ' '
}
finalised() { deployed && [ "$(call 'finalState()(bytes32)')" = "$state" ]; }
mark() {
  grep -q "^$1 " "$done_file" 2> /dev/null || echo "$1 $(date -u +%FT%TZ)" >> "$done_file"
  say "stage complete: $1"
  if [ "${FERRATA_HALT_AFTER:-}" = "$1" ]; then say "halting after $1, as FERRATA_HALT_AFTER asks"; exit 0; fi
}
expect_nonce() { # offset from the first nonce, what is about to be sent
  local n; n="$(nonce)"
  [ "$n" = "$((first + $1))" ] || die "the sender's nonce is $n where $((first + $1)) is planned before $2: a transaction outside this run was sent, or a stage is half done"
}
tx() { (cd "$repo/vault" && script/roundtrip.sh "$@"); }
recorded() { # make sure a stage that is on chain is also in the manifest
  jq -e --arg s "$1" '.stages[$s]' "$manifest" > /dev/null 2>&1 || tx record "$1" > /dev/null ||
    die "stage $1 is on chain but not in the manifest, and forge's broadcast log for it is missing"
}
capture() { # deposit $1's block finalized, then what the LEZ session reads, into $run/lez-capture-$(($1 + 1))
  local i="$1" n=$(($1 + 1)) dir block from cmd got
  dir="$run/lez-capture-$n"
  [ -f "$dir/SHA256SUMS" ] && return 0
  if [ "${FERRATA_FORK:-}" = 1 ]; then say "capture $n: skipped on a fork (its vault is not on Sepolia)"; return 0; fi
  block=$(($(jq -r --arg s "deposit$i" '.stages[$s][-1].blockNumber' "$manifest")))
  from=$(($(jq -r '.stages.deploy[] | select(.contractAddress != null) | .blockNumber' "$manifest" | tail -n 1)))
  say "capture $n: waiting for block $block (deposit $i) to be finalized"
  until [ "$(cast block finalized --field number "${rpc[@]}" 2> /dev/null || echo 0)" -ge "$block" ]; do sleep 30; done
  cmd="${capture_cmd//\{vault\}/$vault}"; cmd="${cmd//\{from_block\}/$from}"; cmd="${cmd//\{out\}/$dir.part}"
  for attempt in $(seq 10); do
    rm -rf "$dir.part"
    say "capture $n, attempt $attempt: $cmd"
    if (cd "$repo" && $cmd) > "$run/lez-capture-$n.log" 2>&1 && [ -f "$dir.part/SHA256SUMS" ]; then
      # the finality update must cover the deposit, and the vault then held exactly n leaves
      got="$(jq -r '[.block // 0, (.deposits.count // -1)] | @tsv' "$dir.part/fixture.json" 2> /dev/null)"
      if [ "$(echo "$got" | cut -f1)" -ge "$block" ] 2> /dev/null && [ "$(echo "$got" | cut -f2)" = "$n" ]; then
        mv "$dir.part" "$dir"
        say "capture $n: $(grep -m1 -E 'finalized slot' "$run/lez-capture-$n.log" || echo done) (block, leaves: $got)"
        return 0
      fi
      say "capture $n: the capture holds block and leaves $got, not >= $block and $n"
    else
      tail -n 3 "$run/lez-capture-$n.log"
    fi
    say "capture $n: the next finality update in about 6.4 minutes"
    sleep 390
  done
  die "capture $n failed ten times: see $run/lez-capture-$n.log"
}
pass_time() { # until the chain's clock reaches $1
  local left; left=$(($1 - $(now)))
  while [ "$left" -gt 0 ]; do
    if [ "${FERRATA_FORK:-}" = 1 ]; then
      cast rpc "${rpc[@]}" evm_increaseTime "$left" > /dev/null
      cast rpc "${rpc[@]}" anvil_mine 0x1 > /dev/null
    else
      say "waiting $left s, until $(date -u -d "@$1" +%FT%TZ) on the chain's clock ($2)"
      sleep "$((left + 13))"
    fi
    left=$(($1 - $(now)))
  done
}
box() { # [--put P]... [--get P]... -- command : cargo and r0vm work, on the build machine if there is one
  local opts=()
  while [ "$1" != -- ]; do opts+=("$1" "$2"); shift 2; done
  shift
  if [ -n "${FERRATA_MRUN:-}" ]; then (cd "$repo" && "$FERRATA_MRUN" --timeout 2400 "${opts[@]}" -- "$1"); else (cd "$repo" && bash -c "$1"); fi
}

# ---- pre-flight --------------------------------------------------------------------------------------
chain="$(cast chain-id "${rpc[@]}")"
[ "$chain" = "$(jq -r .chainId "$fixture")" ] || die "the endpoint is chain $chain, the fixture is for $(jq -r .chainId "$fixture")"
[ "$(echo "$ETH_FROM" | tr 'A-F' 'a-f')" = "$(jq -r .deployer "$fixture" | tr 'A-F' 'a-f')" ] || die "ETH_FROM is not the fixture's deployer"
jq -e '.candidate.seal and ([.releases[].seal] | all)' "$fixture" > /dev/null || die "the fixture lacks the candidate's or a release's seal"
# A transaction of an interrupted start that is still in the mempool: let it land before reading the chain.
for _ in $(seq 60); do
  [ "$(cast nonce "$ETH_FROM" --block pending "${rpc[@]}")" = "$(nonce)" ] && break
  say "a transaction of the sender is still pending; waiting for it"
  sleep 10
done
[ "$(cast nonce "$ETH_FROM" --block pending "${rpc[@]}")" = "$(nonce)" ] || die "a transaction of the sender has been pending for ten minutes"
balance="$(cast balance "$ETH_FROM" "${rpc[@]}")"
say "run $name on chain $chain: sender $ETH_FROM, nonce $(nonce) (first planned $first), balance $(cast from-wei "$balance") ETH, vault $vault"
[ "$(nonce)" -ge "$first" ] || die "the sender's nonce is below the fixture's first nonce"
[ "$(nonce)" -le "$((first + total))" ] || die "the sender has sent more transactions than this run has ($total)"
gas_price="$(cast gas-price "${rpc[@]}")"
say "gas price $gas_price wei; the plan's ~65M gas (Sepolia's own estimates, vault/GAS.md) at twice that price: $(cast from-wei $((65000000 * 2 * gas_price))) ETH"
[ "$balance" -ge $((65000000 * 2 * gas_price)) ] || die "the sender cannot pay ~65M gas at twice the gas price of now"
if ! finalised; then # what the finalisation proof needs, checked before the first transaction and not after the wait
  for f in release.bin checkpoint.bin segment.bin; do
    [ -f "$here/programs/$f" ] || die "e2e/programs/$f is missing: the recorded guest programs (e2e/README.md)"
  done
  if [ -n "${FERRATA_GPU_CMD:-}" ]; then
    [ -x "$FERRATA_GPU_CMD" ] || die "FERRATA_GPU_CMD ($FERRATA_GPU_CMD) is not an executable file"
  else
    command -v modal > /dev/null || die "modal is not installed: the finalisation proof runs on the GPU app"
    [ -n "${MODAL_PROFILE:-}" ] || die "set MODAL_PROFILE, the profile that runs gpu-prover/modal_app.py"
    modal app list --json > /dev/null 2>&1 || die "modal does not answer under the profile $MODAL_PROFILE"
  fi
  if [ -n "${FERRATA_MRUN:-}" ]; then
    [ -x "$FERRATA_MRUN" ] || die "FERRATA_MRUN ($FERRATA_MRUN) is not an executable file"
  else
    for tool in cargo r0vm; do command -v "$tool" > /dev/null || die "$tool is not installed (or set FERRATA_MRUN)"; done
  fi
  if [ -n "${FERRATA_MWRAP:-}" ]; then
    [ -x "$FERRATA_MWRAP" ] || die "FERRATA_MWRAP ($FERRATA_MWRAP) is not an executable file"
  else
    command -v docker > /dev/null || die "docker is not installed (or set FERRATA_MWRAP)"
  fi
fi

# ---- deploy: nonces first .. first+7 ---------------------------------------------------------------------
if deployed && [ "$(nonce)" -ge "$((first + n_deploy))" ]; then
  recorded deploy
elif [ "$(nonce)" = "$first" ]; then
  [ "$balance" -ge 10000000000000000 ] || die "the sender holds less than 0.01 ETH"
  # Nothing of this run is on chain yet, so any forge log for this chain id is another run's (a rehearsal
  # on a fork has the same chain id): remove them, so that a later resume or record reads only this run's.
  rm -f "$repo/vault/broadcast/RoundTrip.s.sol/$chain/"*-latest.json
  say "deploy: $n_deploy transactions"
  tx deploy || die "the deploy stage failed"
elif [ -f "$repo/vault/broadcast/RoundTrip.s.sol/$chain/deploy-latest.json" ]; then
  say "deploy: $(($(nonce) - first)) of $n_deploy transactions are on chain; sending the rest of the recorded ones"
  (export FERRATA_RESUME=1; tx deploy) || die "resuming the deploy stage failed"
else
  die "the sender's nonce is $(nonce): the deploy stage is half done and forge's record of it is missing"
fi
deployed && [ "$(nonce)" -ge "$((first + n_deploy))" ] || die "after deploy the vault has no code or the nonce is $(nonce)"
[ "$(call 'checkpointImageId()(bytes32)')" = "$(jq -r .checkpointImageId "$fixture")" ] || die "the vault's checkpoint image id is not the fixture's"
[ "$(call 'storedLezConfigId()(bytes32)')" = "$(jq -r .lezConfigId "$fixture")" ] || die "the vault's LEZ configuration id is not the fixture's"
mark deploy

# ---- the deposits: nonces first+8 .., each its own stage; LEZ attests the root after those of $attest -----
for i in $(seq 0 $((deposits - 1))); do
  if [ "$(call 'nextLeafIndex()(uint64)' | word)" -gt "$i" ]; then
    recorded "deposit$i"
  else
    expect_nonce $((n_deploy + i)) "deposit $i"
    say "deposit $i: $(jq -r ".deposits[$i] | \"\(.amount) of \(.token)\"" "$fixture") at leaf $i"
    tx "deposit$i" || die "the deposit$i stage failed"
    [ "$(call 'nextLeafIndex()(uint64)' | word)" -gt "$i" ] || die "deposit $i is not in the vault after its stage"
  fi
  mark "deposit$i"
  case " $attest " in *" $i "*) capture "$i"; mark "capture$((i + 1))" ;; esac
done
[ "$(call 'depositRoot()(bytes32)')" = "$(jq -r .depositRoot "$fixture")" ] || die "the vault's deposit root is not the fixture's"

# ---- submit ------------------------------------------------------------------------------------------
read -r t submitted delta <<< "$(candidate)"
if finalised || [ "${submitted:-0}" != 0 ]; then
  recorded submit
else
  expect_nonce "$o_submit" "the candidate"
  say "submit: the candidate, with its real seal"
  tx submit || die "the submit stage failed"
  read -r t submitted delta <<< "$(candidate)"
  [ "${submitted:-0}" != 0 ] || die "the candidate is not in the vault after submit"
fi
mark submit

# ---- the wait, then the beacon ---------------------------------------------------------------------------
if ! finalised; then
  over=$((submitted + delta + (t - 1) * $(call 'turnSeconds()(uint256)' | word)))
  have=""
  if [ -f "$run/beacon.json" ]; then
    have="$(call 'beacons(uint256)(uint256,bytes32)' "$(jq -r .index "$run/beacon.json")" 2> /dev/null | sed -n 2p || true)"
  fi
  if [ -n "$have" ] && [ "$have" = "$(jq -r .nonce "$run/beacon.json")" ] && [ "$(jq -r .timestamp "$run/beacon.json")" -ge "$over" ]; then
    recorded beacon
  else
    pass_time "$over" "the candidate's wait W"
    if [ "$(nonce)" = "$((first + o_beacon + 1))" ]; then
      tx record beacon > /dev/null || die "a beacon transaction is on chain but forge's record of it is missing"
    else
      expect_nonce "$o_beacon" "the beacon"
      say "beacon: recordBeacon"
      tx beacon || die "the beacon stage failed"
    fi
    [ "$(jq -r .timestamp "$run/beacon.json")" -ge "$over" ] || die "the recorded beacon is earlier than the end of the wait"
  fi
  mark beacon
  nonce_wanted="$(jq -r .nonce "$run/beacon.json")"
  say "the finalisation has to echo nonce $nonce_wanted (block $(jq -r .blockNumber "$run/beacon.json"))"

  # ---- the finalisation proof for that beacon; nothing here signs or sends ------------------------------
  if [ "$(jq -r '.echoNonce // empty' "$fixture")" != "$nonce_wanted" ] || ! jq -e '.finalize.seal' "$fixture" > /dev/null; then
    say "proof 1/5: the zone with the echo, the finalisation input and its native journal"
    box --put "$here/programs" -- "e2e/run.sh inputs $name" || die "building the finalisation input failed"
    [ "$(jq -r '.echoNonce // empty' "$fixture")" = "$nonce_wanted" ] || die "the rebuilt fixture does not echo the beacon's nonce"
    input_sha="$(sha256sum "$run/$proof.input" | cut -d' ' -f1)"
    receipts="$run/receipts/$proof"
    if [ "$(jq -r '.input_sha256 // empty' "$receipts/prove.json" 2> /dev/null)" != "$input_sha" ]; then
      [ -n "${FERRATA_GPU_CMD:-}" ] || : "${MODAL_PROFILE:?set MODAL_PROFILE for the GPU proof}"
      gpu_run="e2e-$name-$(jq -r .blockNumber "$run/beacon.json")"
      gpu_job="job $gpu_run input $input_sha"
      gpu_log="$run/work/gpu-run.log"
      scratch="$run/work/gpu-results"
      rm -rf "$scratch" "$receipts"; mkdir -p "$scratch" "$receipts"
      proven() { # the receipt of exactly this input is on the app's Volume (FERRATA_GPU_CMD: in $scratch)
        if [ -z "${FERRATA_GPU_CMD:-}" ]; then
          rm -rf "${scratch:?}/$proof"
          (cd "$scratch" && modal volume get --force ferrata-prover "results/$gpu_run/$proof" . > /dev/null 2>&1) || return 1
        fi
        [ "$(jq -r '.input_sha256 // empty' "$scratch/$proof/prove.json" 2> /dev/null)" = "$input_sha" ]
      }
      queued() { # a job for this input that an earlier start of this script left on the app, not yet stopped
        local app
        [ -z "${FERRATA_GPU_CMD:-}" ] || return 1
        [ "$(head -n 1 "$gpu_log" 2> /dev/null)" = "$gpu_job" ] || return 1
        app="$(grep -o 'ap-[A-Za-z0-9]\{12,\}' "$gpu_log" | head -n 1)"
        [ -n "$app" ] && modal app list --json 2> /dev/null |
          jq -e --arg app "$app" 'any(.[]; .app_id == $app and .stopped_at == null)' > /dev/null
      }
      if ! proven && queued; then
        say "proof 2/5: the GPU job an earlier start of this script queued is still on the app; waiting for it"
        while queued; do sleep 30; done
      fi
      if ! proven; then
        say "proof 2/5: the checkpoint proof on the GPU app (the queue for a GPU can take half an hour)"
        "$here/run.sh" stage-gpu "$name" > /dev/null
        echo "$gpu_job" > "$gpu_log"
        if [ -n "${FERRATA_GPU_CMD:-}" ]; then
          "$FERRATA_GPU_CMD" "$run/work/gpu-inputs" "$proof" "$gpu_run" "$scratch/$proof" >> "$gpu_log" 2>&1 ||
            die "the GPU run failed: see $gpu_log"
        else
          (cd "$repo/gpu-prover" && FERRATA_INPUTS="$run/work/gpu-inputs" modal run --detach modal_app.py::batch_run \
            --jobs "$proof=600" --run "$gpu_run") >> "$gpu_log" 2>&1 || die "the GPU run failed: see $gpu_log"
          rm -f "$repo/gpu-prover/logs/run-$gpu_run-$proof.json"
        fi
        proven || die "the GPU proof of this input is not on the Volume: see $gpu_log"
      fi
      cp "$scratch/$proof/receipt-succinct.bin" "$scratch/$proof/prove.json" "$receipts/"
    fi
    say "proof 3/5: identity_p254"
    box --get "$run/work/$proof" -- "e2e/run.sh p254 $name $proof" || die "identity_p254 failed"
    say "proof 4/5: RISC Zero's Groth16 prover image"
    rm -f "$run/work/$proof/proof.json"
    if [ -n "${FERRATA_MWRAP:-}" ]; then
      "$FERRATA_MWRAP" "$run/work/$proof/input.json" "$run/work/$proof/proof.json" > "$run/work/$proof/prover.log" 2>&1 || die "the prover image failed: see $run/work/$proof/prover.log"
    else
      docker run --rm -v "$run/work/$proof:/mnt" risczero/risc0-groth16-prover:v2025-04-03.1 > "$run/work/$proof/prover.log" 2>&1 || die "the prover image failed"
    fi
    say "proof 5/5: the Groth16 receipt, verified, and the fixture"
    box --put "$run/work/$proof/proof.json" -- "e2e/run.sh groth16 $name $proof && e2e/run.sh fixture $name" || die "assembling the receipt failed"
    jq -e '.finalize.seal' "$fixture" > /dev/null || die "the fixture has no finalisation seal"
  fi
  mark proof

  # ---- finalize ----------------------------------------------------------------------------------------
  expect_nonce "$o_final" "the finalisation"
  say "finalize: with the seal that echoes the beacon"
  # recordBeacon is open to anyone: name the beacon the proof echoes, which need not be the latest by now.
  beacon_index="$(jq -er .index "$run/beacon.json")" || die "beacon.json does not say which of the vault's beacons it is"
  (export FERRATA_BEACON_INDEX="$beacon_index"; tx finalize) || die "the finalize stage failed"
  finalised || die "the vault's finalState is not the checkpoint's new state after finalize"
else
  recorded finalize
fi
mark finalize

# ---- the release delay, then the releases ---------------------------------------------------------------
pending=0
delay=0
for i in $(seq 0 $((releases - 1))); do
  [ "$(call 'nullifierUsed(bytes32)(bool)' "$(jq -r ".releases[$i].nullifier" "$fixture")")" = true ] || pending=$((pending + 1))
  d="$(call 'releaseDelayFor(address,uint256)(uint256)' "$(jq -r ".releases[$i].token" "$fixture")" 0 | word)"
  [ "$d" -le "$delay" ] || delay="$d"
done
if [ "$pending" -gt 0 ]; then
  finalized_at="$(call 'burnRootStatus(bytes32)(bool,uint256)' "$(jq -r .burnRoot "$fixture")" | sed -n 2p | awk '{print $1}')"
  pass_time "$((finalized_at + delay))" "the release delay"
  expect_nonce "$((o_rel + releases - pending))" "the releases"
  if [ "$pending" -lt "$releases" ] && [ -f "$repo/vault/broadcast/RoundTrip.s.sol/$chain/release-latest.json" ]; then
    say "release: $((releases - pending)) of $releases are paid; sending the rest of the recorded ones"
    (export FERRATA_RESUME=1; tx release) || die "resuming the release stage failed"
  else
    say "release: $pending release(s)"
    tx release || die "the release stage failed"
  fi
else
  recorded release
fi
mark release

# ---- the re-check --------------------------------------------------------------------------------------
say "verify: every transaction, the wiring, the seals, the balances, and what must be refused"
"$repo/vault/script/verify-sepolia.sh" "$manifest" "$fixture" || die "verify-sepolia.sh found a difference"
mark verify
spent="$(jq -r '[.stages[][] | (.gasUsed | ltrimstr("0x")) + " " + (.effectiveGasPrice | ltrimstr("0x"))] | .[]' "$manifest" |
  while read -r g p; do echo "$((16#$g)) $((16#$p))"; done | awk '{gas += $1; wei += $1 * $2} END {printf "%d gas, %.9f ETH", gas, wei / 1e18}')"
say "DONE: $(jq '[.stages[][]] | length' "$manifest") transactions on chain $chain, $spent; vault $vault; manifest $manifest"
