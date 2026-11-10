#!/usr/bin/env bash
# The return leg on Sepolia from the first transaction to the last check, unattended and resumable.
# Start it once; started again after an interruption it reads the chain, sees which stages are
# complete and continues after the last one. It sends exactly the fifteen transactions of
# vault/README.md ("Sepolia round trip") in their nonce order and stops with a message at the
# first thing that is not as planned.
#
#   e2e/sepolia-live.sh [run]          the run directory under e2e/runs/ (default sepolia-live)
#
# Stages: pre-flight, deploy (10 transactions), submit (1), the vault's wait, beacon (1), the
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
#   FERRATA_FORK=1                            a rehearsal on a local fork (anvil): the sender is
#                                             impersonated, no key is read, and the two waits are passed
#                                             by moving the fork's clock
#   FERRATA_HALT_AFTER=<stage>                stop cleanly after deploy, submit, beacon, proof, finalize or
#                                             release (to do the rest later, or by hand)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
name="${1:-sepolia-live}"
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
case "$SEPOLIA_RPC_URL" in http://127.0.0.1:* | http://localhost:*) local_endpoint=1 ;; *) local_endpoint=0 ;; esac
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
balance="$(cast balance "$ETH_FROM" "${rpc[@]}")"
say "run $name on chain $chain: sender $ETH_FROM, nonce $(nonce) (first planned $first), balance $(cast from-wei "$balance") ETH, vault $vault"
[ "$(nonce)" -ge "$first" ] || die "the sender's nonce is below the fixture's first nonce"
[ "$(nonce)" -le "$((first + 15))" ] || die "the sender has sent more transactions than this run has"

# ---- deploy: nonces first .. first+9 ---------------------------------------------------------------------
if deployed && [ "$(nonce)" -ge "$((first + 10))" ]; then
  recorded deploy
elif [ "$(nonce)" = "$first" ]; then
  [ "$balance" -ge 10000000000000000 ] || die "the sender holds less than 0.01 ETH"
  say "deploy: 10 transactions"
  tx deploy || die "the deploy stage failed"
elif [ -f "$repo/vault/broadcast/RoundTrip.s.sol/$chain/deploy-latest.json" ]; then
  say "deploy: $(($(nonce) - first)) of 10 transactions are on chain; sending the rest of the recorded ones"
  (export FERRATA_RESUME=1; tx deploy) || die "resuming the deploy stage failed"
else
  die "the sender's nonce is $(nonce): the deploy stage is half done and forge's record of it is missing"
fi
deployed && [ "$(nonce)" -ge "$((first + 10))" ] || die "after deploy the vault has no code or the nonce is $(nonce)"
[ "$(call 'depositRoot()(bytes32)')" = "$(jq -r .depositRoot "$fixture")" ] || die "the vault's deposit root is not the fixture's"
[ "$(call 'checkpointImageId()(bytes32)')" = "$(jq -r .checkpointImageId "$fixture")" ] || die "the vault's checkpoint image id is not the fixture's"
mark deploy

# ---- submit: nonce first+10 --------------------------------------------------------------------------
read -r t submitted delta <<< "$(candidate)"
if finalised || [ "${submitted:-0}" != 0 ]; then
  recorded submit
else
  expect_nonce 10 "the candidate"
  say "submit: the candidate, with its real seal"
  tx submit || die "the submit stage failed"
  read -r t submitted delta <<< "$(candidate)"
  [ "${submitted:-0}" != 0 ] || die "the candidate is not in the vault after submit"
fi
mark submit

# ---- the wait, then the beacon: nonce first+11 ---------------------------------------------------------
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
    if [ "$(nonce)" = "$((first + 12))" ]; then
      tx record beacon > /dev/null || die "a beacon transaction is on chain but forge's record of it is missing"
    else
      expect_nonce 11 "the beacon"
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
      : "${MODAL_PROFILE:?set MODAL_PROFILE for the GPU proof}"
      gpu_run="e2e-$name-$(jq -r .blockNumber "$run/beacon.json")"
      scratch="$run/work/gpu-results"
      rm -rf "$scratch" "$receipts"; mkdir -p "$scratch" "$receipts"
      fetch() { (cd "$scratch" && modal volume get --force ferrata-prover "results/$gpu_run/$proof" . > /dev/null 2>&1); }
      if ! fetch || [ "$(jq -r '.input_sha256 // empty' "$scratch/$proof/prove.json" 2> /dev/null)" != "$input_sha" ]; then
        say "proof 2/5: the checkpoint proof on the GPU app (the queue for a GPU can take half an hour)"
        "$here/run.sh" stage-gpu "$name" > /dev/null
        (cd "$repo/gpu-prover" && FERRATA_INPUTS="$run/work/gpu-inputs" modal run --detach modal_app.py::batch_run \
          --jobs "$proof=600" --run "$gpu_run") > "$run/work/gpu-run.log" 2>&1 || die "the GPU run failed: see $run/work/gpu-run.log"
        rm -f "$repo/gpu-prover/logs/run-$gpu_run-$proof.json"
        fetch || die "the GPU proof is not on the Volume"
      fi
      [ "$(jq -r '.input_sha256 // empty' "$scratch/$proof/prove.json")" = "$input_sha" ] || die "the GPU receipt is for another input"
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

  # ---- finalize: nonce first+12 ------------------------------------------------------------------------
  expect_nonce 12 "the finalisation"
  say "finalize: with the seal that echoes the beacon"
  tx finalize || die "the finalize stage failed"
  finalised || die "the vault's finalState is not the checkpoint's new state after finalize"
else
  recorded finalize
fi
mark finalize

# ---- the release delay, then the releases: nonces first+13 .. ---------------------------------------------
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
  expect_nonce "$((13 + releases - pending))" "the releases"
  say "release: $pending release(s)"
  tx release || die "the release stage failed"
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
