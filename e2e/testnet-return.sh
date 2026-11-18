#!/usr/bin/env bash
# The leg back for the public LEZ testnet (LEZ v0.3.1): a burn there, released by a FerrataVault on Sepolia.
# A TESTNET STAND-IN arrangement, not Ferrata's design (vault/src/standin/README.md, docs/trust.md §3.6):
#   REAL      the unchanged vault (rule (B)), the deposits, and every release: a zero-knowledge proof of the unchanged
#             `release` program, wrapped to Groth16 and checked by RISC Zero's verifier;
#   STAND-IN  the burn root: no checkpoint proof exists for a v0.3.1 zone whose sequencer runs no attestor, so the
#             vault's verifier is StandInCheckpointVerifier and the root is accepted on the signatures of keys that the
#             operator of this run holds (lez-v03-return/standin.py).
#
#   e2e/testnet-return.sh plan   <run>            reads only: the deployer's nonce, the addresses, the LEZ tip, the
#                                                 vault's parameters and what the run sends -> e2e/runs/<run>/
#   e2e/testnet-return.sh fork   <run>            the whole Ethereum side on a local anvil fork of Sepolia; the sender
#                                                 is impersonated, no key is read, nothing leaves this machine
#   e2e/testnet-return.sh live   <run> <stage>    one stage on Sepolia; refuses unless FERRATA_SEPOLIA_SEND=yes
#                                                 stages: deploy | deposit <j> | attest | submit | finalize |
#                                                         release <i> | refusals <phase> | status | all
#   e2e/testnet-return.sh prove  <run> [step]     the release proofs (sends nothing): build | inputs | cpu | wrap |
#                                                 seal | all
#   e2e/testnet-return.sh attest <run>            the signed checkpoint (sends nothing; also a live stage)
#   e2e/testnet-return.sh verify <run> [--fork]   re-reads every verdict from the endpoint against the run's records
#
# Stages in order (fork runs them all; live: one at a time, or `all`, which stops wherever the LEZ side must act):
#   plan -> deploy (3 or 4 transactions) -> deposit 0, deposit 1 -> [the LEZ side imports, mints and burns] ->
#   attest (reads the LEZ node, signs) -> submit -> the wait delta -> finalize -> prove -> the release delay ->
#   release 0, release 1 -> verify
#
# Settings: the environment, or the file $FERRATA_ENV_FILE (default e2e/sepolia-live.env, not committed; the same file
# and variables as e2e/sepolia-live.sh):
#   SEPOLIA_RPC_URL, ETH_FROM            the endpoint and the sender
#   ETH_KEYSTORE + ETH_PASSWORD          (live) paths of a keystore file and of a file with its password: Foundry's own
#   (or ETH_KEYSTORE_ACCOUNT)            variables, read by forge itself; this script never opens either file
#   FERRATA_STANDIN_KEYS                 the directory of the stand-in keys (standin.py keygen): outside this repository
#   FERRATA_STANDIN_THRESHOLD            default 2
#   FERRATA_LEZ_RPC                      the LEZ node (default https://testnet.lez.logos.co)
#   FERRATA_LEZ_CONFIG_ID                the LEZ v0.3.1 bridge configuration's id (the vault's lezConfigId; it also is
#                                        in every burn leaf). Unset: a labelled placeholder, for a rehearsal only
#   FERRATA_LEZ_PROGRAM | FERRATA_LEZ_LAYOUT   (live attest) the bridge program's id on LEZ, from which the layout
#                                        of lez-v03-bridge's burn accumulator is derived; or a layout file
#                                        (lez-v03-return/INTERFACE.md). FERRATA_IMPORTED: the leaf count LEZ imported
#                                        (default: the vault's)
#   FERRATA_WALLET_SEED_TEXT             (live attest, prove) the seed text lez-v03-bridge's driver burned with: the
#                                        wallet's notes are recomputed from it, in accumulator order
#                                        (FERRATA_BURN_INDICES, default 0,1; FERRATA_BURN_KINDS, default
#                                        public,private; FERRATA_BURN_RECIPIENT, default ETH_FROM; FERRATA_DRIVER_NOTES:
#                                        the driver's record of the notes it sent, compared). Or FERRATA_BURN_NOTES /
#                                        FERRATA_BURNS: the wallet's notes, or every burn in order (INTERFACE.md)
#   FERRATA_DEPOSITS                     the two deposits [{j, denomIndex, commitment, kind, dest}] of the LEZ side's
#                                        plan (lez-v03-bridge's deposits.json is read as it is). Unset: commitments
#                                        to labelled test destinations (a rehearsal)
#   FERRATA_REGISTRY                     a deployed FerrataRegistry to link (checked byte for byte); unset: deploy one
#   FERRATA_RISC_ZERO_STOP               RISC Zero's emergency stop (default: crown-live-3's, owner renounced)
#   FERRATA_DELTA, FERRATA_RELEASE_DELAY default 120 s each (crown-live-3: 3600 s and 120 s)
#   FERRATA_MRUN, FERRATA_MWRAP          optional wrappers, as in e2e/sepolia-live.sh: where cargo and r0vm run, and
#                                        what runs RISC Zero's Groth16 prover image
#   FERRATA_PROGRAMS                     the directory with release.bin (default e2e/programs)
#   FERRATA_GAS_MARGIN_PCT               default 125
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
mode="${1:-}"; name="${2:-}"
case "$mode" in plan | fork | live | prove | verify | attest) ;; *) sed -n '2,56p' "$0" >&2; exit 2 ;; esac
[ -n "$name" ] || { echo "a run name" >&2; exit 2; }
shift 2
run="$here/runs/$name"
export TMPDIR="${TMPDIR:-$repo/../tmp}" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
unset RISC0_DEV_MODE
mkdir -p "$TMPDIR" "$run/logs" "$run/checkpoint"
envfile="${FERRATA_ENV_FILE:-$here/sepolia-live.env}"
if [ -f "$envfile" ]; then set -a; . "$envfile"; set +a; fi

say() { echo "[$(date -u +%FT%TZ)] $*"; }
die() { say "STOP: $*"; exit 1; }
for tool in cast forge jq python3 sha256sum; do command -v "$tool" > /dev/null || die "$tool is not installed"; done

public_rpc=https://ethereum-sepolia-rpc.publicnode.com
: "${SEPOLIA_RPC_URL:=$public_rpc}" "${ETH_FROM:=0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07}"
lez_rpc="${FERRATA_LEZ_RPC:-https://testnet.lez.logos.co}"
stop="${FERRATA_RISC_ZERO_STOP:-0x1dB3fa8041E8178f1608c2F7dF627d7EC16C8221}"
threshold="${FERRATA_STANDIN_THRESHOLD:-2}"
margin="${FERRATA_GAS_MARGIN_PCT:-125}"
standin_py=(python3 -I "$repo/lez-v03-return/standin.py")
verify_py=(python3 -I "$repo/lez-v03-return/verify.py")
programs="${FERRATA_PROGRAMS:-$here/programs}"
E="${FERRATA_E2E:-$repo/bridge/target/release/ferrata-e2e}"
release_id=0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9 # docs/image-ids.md, `release`
plan="$run/plan.json"; id="$run/identity.json"; crown="$run/crown.json"; manifest="$run/manifest.json"

keys_dir() {
  [ -n "${FERRATA_STANDIN_KEYS:-}" ] || die "set FERRATA_STANDIN_KEYS to the stand-in keys' directory (standin.py keygen)"
  case "$(realpath -m "$FERRATA_STANDIN_KEYS")/" in "$repo"/*) die "the stand-in keys must not be inside the repository" ;; esac
  echo "$FERRATA_STANDIN_KEYS"
}
rpc() { echo --rpc-url "$SEPOLIA_RPC_URL"; }
call() { cast call "$(jq -r .vault "$plan")" "$@" --rpc-url "$SEPOLIA_RPC_URL"; }
word() { awk 'NR==1 {print $1}'; }
now() { cast block latest --field timestamp --rpc-url "$SEPOLIA_RPC_URL"; }
pass_time() { # until the chain's clock reaches $1
  local left; left=$(($1 - $(now)))
  while [ "$left" -gt 0 ]; do
    if [ "${FERRATA_FORK:-}" = 1 ]; then
      cast rpc --rpc-url "$SEPOLIA_RPC_URL" evm_increaseTime "$left" > /dev/null
      cast rpc --rpc-url "$SEPOLIA_RPC_URL" anvil_mine 0x1 > /dev/null
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
sepolia() { # the crown run's sender for deposits, the candidate, the finalisation and the releases (one transaction each)
  SEPOLIA_RPC_URL="$SEPOLIA_RPC_URL" ETH_FROM="$ETH_FROM" FERRATA_GAS_MARGIN_PCT="$margin" "$here/crown/sepolia.sh" "$run" "$@"
}

# ---- plan: reads only ----------------------------------------------------------------------------------------------
do_plan() {
  local chain nonce pending balance price signers genesis tip config placeholder standin registry vault n_vault deposits
  chain="$(cast chain-id --rpc-url "$SEPOLIA_RPC_URL")"
  [ "$chain" = 11155111 ] || die "the endpoint is chain $chain, not Sepolia"
  if [ -f "$plan" ] && [ "$(cast code "$(jq -r .standIn.address "$plan")" --rpc-url "$SEPOLIA_RPC_URL")" != 0x ]; then
    say "plan: this run's deployment has begun (the stand-in has code); the plan is kept as it is"
    return 0
  fi
  nonce="$(cast nonce "$ETH_FROM" --rpc-url "$SEPOLIA_RPC_URL")"
  pending="$(cast nonce "$ETH_FROM" --block pending --rpc-url "$SEPOLIA_RPC_URL")"
  [ "$nonce" = "$pending" ] || die "the sender has a pending transaction (nonce $nonce, pending $pending)"
  balance="$(cast balance "$ETH_FROM" --rpc-url "$SEPOLIA_RPC_URL")"
  price="$(cast gas-price --rpc-url "$SEPOLIA_RPC_URL")"
  # RISC Zero's verifier behind its stop: the one the live vaults use, untripped, with no owner.
  [ "$(cast code "$stop" --rpc-url "$SEPOLIA_RPC_URL")" != 0x ] || die "RISC Zero's stop $stop has no code"
  [ "$(cast call "$stop" 'paused()(bool)' --rpc-url "$SEPOLIA_RPC_URL")" = false ] || die "RISC Zero's stop is tripped"
  local groth16 owner
  groth16="$(cast call "$stop" 'verifier()(address)' --rpc-url "$SEPOLIA_RPC_URL")"
  owner="$(cast call "$stop" 'owner()(address)' --rpc-url "$SEPOLIA_RPC_URL")"
  [ "$(cast call "$groth16" 'SELECTOR()(bytes4)' --rpc-url "$SEPOLIA_RPC_URL")" = 0x73c457ba ] || die "not RISC Zero 3.0's Groth16 verifier"
  signers="$("${standin_py[@]}" signers --dir "$(keys_dir)" | jq -c .signers)"
  [ "$(jq length <<< "$signers")" -ge "$threshold" ] || die "fewer stand-in keys than the threshold"
  standin="$(cast compute-address "$ETH_FROM" --nonce "$nonce" | awk '{print $NF}')"
  registry="${FERRATA_REGISTRY:-}"
  if [ -n "$registry" ]; then n_vault=$((nonce + 1)); else
    registry="$(cast compute-address "$ETH_FROM" --nonce $((nonce + 1)) | awk '{print $NF}')"; n_vault=$((nonce + 2))
  fi
  vault="$(cast compute-address "$ETH_FROM" --nonce "$n_vault" | awk '{print $NF}')"
  for a in "$standin" "$vault"; do
    [ "$(cast code "$a" --rpc-url "$SEPOLIA_RPC_URL")" = 0x ] || die "$a already has code"
  done
  # The vault's genesis: the newest block the public LEZ node reports Finalized, and the stand-in key set.
  genesis="$("${standin_py[@]}" genesis --rpc "$lez_rpc" --signers "$(jq -r 'join(",")' <<< "$signers")")"
  tip="$(jq -c .tip <<< "$genesis")"
  config="${FERRATA_LEZ_CONFIG_ID:-}"; placeholder=false
  if [ -z "$config" ]; then
    placeholder=true
    config="0x$(printf 'ferrata testnet-return: PLACEHOLDER lezConfigId (no LEZ v0.3.1 bridge configuration was given), vault %s' "${vault,,}" | sha256sum | cut -d' ' -f1)"
  fi
  # The deposits: the LEZ side's plan, or commitments to labelled test destinations (crown-live-3's test accounts).
  if [ -n "${FERRATA_DEPOSITS:-}" ]; then
    # This script's list, or lez-v03-bridge's deposits.json (leaf_index, kind 1 private / 2 public, commitment bare hex).
    deposits="$(jq -c 'to_entries | map(.key as $k | .value | {j: (.j // $k), denomIndex: (.denomIndex // 0),
      commitment: (if (.commitment | startswith("0x")) then .commitment else "0x" + .commitment end),
      kind: (if .kind == 1 then "private" elif .kind == 2 then "public" else .kind end), dest: .dest,
      origin: (.origin // "the LEZ side'"'"'s plan")})' "$FERRATA_DEPOSITS")"
  else
    local c0 c1 d0=0x54d662b664519cb8aef608ae84a63e041cc16019e4a07c34828311996f0c817a d1=0x2d6cb849edc6d8b1599c558ecd87bc09b4420bdc60f1b3be85547a5834b153fc
    c0="$(python3 -I "$here/crown/identity.py" commitment --chain-id "$chain" --vault "$vault" --depositor "$ETH_FROM" --j 0 --kind public --dest "$d0")"
    c1="$(python3 -I "$here/crown/identity.py" commitment --chain-id "$chain" --vault "$vault" --depositor "$ETH_FROM" --j 1 --kind private --dest "$d1")"
    deposits="$(jq -nc --arg c0 "$c0" --arg c1 "$c1" --arg d0 "$d0" --arg d1 "$d1" '[
      {j: 0, denomIndex: 0, commitment: $c0, kind: "public", dest: $d0, origin: "STAND-IN: a labelled test destination and the test bridge seed"},
      {j: 1, denomIndex: 0, commitment: $c1, kind: "private", dest: $d1, origin: "STAND-IN: a labelled test destination and the test bridge seed"}]')"
  fi
  jq -n --argjson chain "$chain" --arg deployer "$ETH_FROM" --argjson nonce "$nonce" --arg standin "$standin" \
    --arg registry "$registry" --argjson reused "$([ -n "${FERRATA_REGISTRY:-}" ] && echo true || echo false)" \
    --arg vault "$vault" --arg stop "$stop" --arg groth16 "$groth16" --arg owner "$owner" --argjson signers "$signers" \
    --argjson threshold "$threshold" --arg config "$config" --argjson placeholder "$placeholder" \
    --arg genesis "$(jq -r .genesisState <<< "$genesis")" --argjson tip "$tip" --arg lez "$lez_rpc" \
    --arg release "$release_id" --argjson delta "${FERRATA_DELTA:-120}" --argjson delay "${FERRATA_RELEASE_DELAY:-120}" \
    --arg balance "$balance" --arg price "$price" --argjson deposits "$deposits" --arg at "$(date -u +%FT%TZ)" '{
      what: "TESTNET STAND-IN return leg for the public LEZ testnet (vault/src/standin/README.md). REAL: the vault, the deposits, the release proofs. STAND-IN: the burn root is attested by keys the operator of this run holds.",
      plannedAt: $at, chainId: $chain, deployer: $deployer, firstNonce: $nonce,
      standIn: {address: $standin, riscZero: $stop, riscZeroOwner: $owner, groth16Verifier: $groth16,
                imageId: "0x66ff80f9020fb4926743819c158cc5f797ee1a19777e27a472147bbaf6e2dc38",
                signers: $signers, threshold: $threshold},
      registry: $registry, registryReused: $reused, vault: $vault, mode: "B",
      lezConfigId: $config, lezConfigIdIsPlaceholder: $placeholder, releaseImageId: $release,
      genesisState: $genesis, lez: {rpc: $lez, genesisTip: $tip},
      delta: $delta, releaseDelay: $delay, revokeWindow: 86400, deposits: $deposits,
      sender: {balanceWei: $balance, gasPriceWei: $price}}' > "$plan"
  jq '{label: "testnet-return (stand-in)", chainId, vault, configId: .lezConfigId, genesisState, delta, deployer, firstNonce}' "$plan" > "$id"
  jq '{chainId, deployer, standIn: {riscZero: .standIn.riscZero, signers: .standIn.signers, threshold: .standIn.threshold},
       releaseImageId, lezConfigId, genesisState, delta, releaseDelay, revokeWindow,
       ethLezDefinition: "0x0000000000000000000000000000000000000000000000000000000000000000",
       expect: {standIn: .standIn.address, vault: .vault}}
      + (if .registryReused then {registry: .registry} else {} end)' "$plan" > "$run/params.json"
  jq '{chainId, vault, deposits}' "$plan" > "$crown" # nothing of this run is deployed yet: any earlier candidate is void
  say "plan $name: sender $ETH_FROM, nonce $nonce, balance $(cast from-wei "$balance") ETH, gas price $price wei"
  say "  stand-in verifier $standin (nonce $nonce): $threshold of $(jq -c . <<< "$signers"), in front of RISC Zero's stop $stop"
  say "  registry $registry ($([ -n "${FERRATA_REGISTRY:-}" ] && echo reused || echo "nonce $((nonce + 1))")); vault $vault (nonce $n_vault), rule (B)"
  say "  lezConfigId $config$([ "$placeholder" = true ] && echo ' (PLACEHOLDER: set FERRATA_LEZ_CONFIG_ID to the LEZ bridge configuration and plan again before a live run)')"
  say "  genesisState $(jq -r .genesisState <<< "$genesis") at LEZ block $(jq -r .id <<< "$tip") ($(jq -r .status <<< "$tip") on $lez_rpc)"
  say "  delta ${FERRATA_DELTA:-120} s, release delay ${FERRATA_RELEASE_DELAY:-120} s; 2 deposits of 0.001002 ETH"
  # What it costs on Sepolia (EIP-8037 state gas): crown-live-3's own figures for the same transactions (its candidate
  # carried a Groth16 seal; this one's signatures cost less), Sepolia's own estimate of the stand-in's creation
  # (5,080,177, asked on 18 Nov 2026); at twice the gas price of now.
  local gas=$((5080177 + 39559884 + 3218360 + 559652 + 268026 + 729349 + 844349 + 661632 + 465768))
  [ -n "${FERRATA_REGISTRY:-}" ] || gas=$((gas + 19700914))
  say "  about $gas gas in $([ -n "${FERRATA_REGISTRY:-}" ] && echo 9 || echo 10) transactions; at twice the gas price of now: $(cast from-wei $((gas * 2 * price))) ETH, plus 0.002004 ETH escrowed by the deposits (paid back by the releases less 0.000002 ETH of fees)"
}

# ---- deploy: one transaction per forge run, each recorded ----------------------------------------------------------
record() { # appends the receipts of a forge broadcast file to the manifest under a stage's name
  local stage="$1" bfile="$2" rows='[]' h r chain
  chain="$(jq -r .chainId "$plan")"
  [ -f "$manifest" ] || echo '{"chainId": '"$chain"', "stages": {}}' > "$manifest"
  for h in $(jq -r '.transactions[].hash' "$bfile"); do
    jq -e --arg h "$h" '[.stages[][] | .hash] | index($h)' "$manifest" > /dev/null && continue
    r="$(cast receipt "$h" --json --rpc-url "$SEPOLIA_RPC_URL")"
    [ "$(jq -r .status <<< "$r")" = 0x1 ] || die "$stage: transaction $h failed"
    rows="$(jq -c --argjson r "$r" --arg h "$h" '. + [{hash: $h, blockNumber: ($r.blockNumber), status: ($r.status),
      gasUsed: ($r.gasUsed), to: ($r.to), contractAddress: ($r.contractAddress)}]' <<< "$rows")"
  done
  jq --arg s "$stage" --argjson rows "$rows" '.stages[$s] = ((.stages[$s] // []) + $rows)' "$manifest" > "$manifest.tmp"
  mv "$manifest.tmp" "$manifest"
}
nonce_max() { # the deployer's nonce at its freshest (vault/script/deploy.sh says why)
  local m=-1 k v
  for k in 1 2 3; do
    for v in "$(cast nonce "$ETH_FROM" --rpc-url "$SEPOLIA_RPC_URL" 2> /dev/null || true)" \
             "$(cast nonce "$ETH_FROM" --block pending --rpc-url "$SEPOLIA_RPC_URL" 2> /dev/null || true)"; do
      if [[ "$v" =~ ^[0-9]+$ ]] && [ "$v" -gt "$m" ]; then m="$v"; fi
    done
    if [ "$k" -lt 3 ]; then sleep "${FERRATA_NONCE_GAP:-4}"; fi
  done
  echo "$m"
}
do_deploy() {
  local chain out sign=() last="" step log tries n0 b rule=eip8037
  local transient='-32001|block not found|header not found|failed to get account|failed to get storage|missing trie node'
  chain="$(jq -r .chainId "$plan")"
  out="deployments/standin-$name.json"
  mkdir -p "$repo/vault/deployments"
  [ "${FERRATA_FORK:-}" = 1 ] && sign=(--unlocked)
  if [ -f "$run/deploy.json" ] && [ "$(jq -r .complete "$run/deploy.json")" = true ]; then say "deploy: complete"; return 0; fi
  [ -f "$run/deploy.json" ] && [ ! -f "$repo/vault/$out" ] && cp "$run/deploy.json" "$repo/vault/$out"
  for n in $(seq 1 8); do
    log="$run/logs/deploy-$n.log"; tries=0
    while :; do
      n0="$(nonce_max)"
      if (cd "$repo/vault" && env -u ETH_FROM FERRATA_STANDIN_PARAMS="$run/params.json" FERRATA_OUT="$out" \
          FERRATA_GAS_RULE="$rule" FERRATA_GAS_MARGIN_PCT="$margin" \
          forge script script/standin/DeployStandIn.s.sol:DeployStandIn --rpc-url "$SEPOLIA_RPC_URL" --sender "$ETH_FROM" \
          ${sign[@]+"${sign[@]}"} --broadcast --skip-simulation --slow --gas-estimate-multiplier "$margin" -vv) > "$log" 2>&1; then
        break
      fi
      tail -n 25 "$log" | sed "s,$SEPOLIA_RPC_URL,<endpoint>,g" >&2
      if [ "$tries" -lt 5 ] && grep -q -E -e "$transient" "$log" && ! grep -q -E 'Hash: 0x|"hash": *"0x' "$log" \
          && [ "$n0" -ge 0 ] && [ "$(nonce_max)" = "$n0" ]; then
        tries=$((tries + 1)); mv "$log" "$log.transient-$tries"
        say "deploy: the endpoint failed a read and nothing was sent (nonce still $n0); again in 24 s ($tries of 5)"
        sleep "${FERRATA_RETRY_SLEEP:-24}"; continue
      fi
      die "deploy: run $n failed; nothing more is sent (log $log)"
    done
    step="$({ grep -o 'FERRATA_STEP [A-Za-z0-9]*' "$log" || true; } | tail -n 1 | cut -d' ' -f2)"
    grep -h -o -E 'FERRATA_(GAS|CREATED) .*' "$log" || true
    cp "$repo/vault/$out" "$run/deploy.json"
    b="$repo/vault/broadcast/DeployStandIn.s.sol/$chain/run-latest.json"
    if grep -q 'FERRATA_GAS' "$log" && [ -f "$b" ]; then record deploy "$b"; fi
    if grep -q FERRATA_COMPLETE "$log"; then break; fi
    [ -n "$step" ] && [ "$step" != "$last" ] || die "deploy: no progress at step '$step' (log $log)"
    last="$step"
  done
  [ "$(jq -r .complete "$run/deploy.json")" = true ] || die "deploy.json is not complete"
  [ "$(jq -r '.vault | ascii_downcase' "$run/deploy.json")" = "$(jq -r '.vault | ascii_downcase' "$plan")" ] || die "the vault is not at the planned address"
  [ "$(call 'storedLezConfigId()(bytes32)')" = "$(jq -r .lezConfigId "$plan")" ] || die storedLezConfigId
  [ "$(call 'checkpointImageId()(bytes32)')" = "$(jq -r .standIn.imageId "$plan")" ] || die "the vault's checkpoint id is not the stand-in's"
  local code; code="$(cast code "$(jq -r .standIn.address "$plan")" --rpc-url "$SEPOLIA_RPC_URL")"
  jq --arg h "$(cast keccak "$code")" '.standIn.codeHash = $h' "$plan" > "$plan.tmp" && mv "$plan.tmp" "$plan"
  say "deploy complete: stand-in $(jq -r .standIn.address "$plan"), vault $(jq -r .vault "$plan") (rule (B), verifier = the stand-in)"
}

# ---- attest: the burn root at a final LEZ block, signed by the stand-in keys -----------------------------------------
do_attest() {
  local cp="$run/checkpoint" parent signers args=() imported
  [ -f "$cp/candidate.json" ] && { say "attest: $cp/candidate.json exists"; return 0; }
  if [ -n "${FERRATA_BURNS:-}" ]; then cp "$FERRATA_BURNS" "$cp/burns.json"; fi
  if [ ! -f "$cp/burns.json" ] && [ -n "${FERRATA_WALLET_SEED_TEXT:-}" ]; then
    # lez-v03-bridge's wallet: its notes recomputed from the seed text its driver was given (INTERFACE.md §2).
    local check=()
    [ -n "${FERRATA_DRIVER_NOTES:-}" ] && check=(--check "$FERRATA_DRIVER_NOTES")
    "${standin_py[@]}" wallet-burns --identity "$id" --seed-text "$FERRATA_WALLET_SEED_TEXT" \
      --recipient "${FERRATA_BURN_RECIPIENT:-$ETH_FROM}" --indices "${FERRATA_BURN_INDICES:-0,1}" \
      --kinds "${FERRATA_BURN_KINDS:-public,private}" "${check[@]}" --out "$cp"
  fi
  if [ ! -f "$cp/burns.json" ] && [ -n "${FERRATA_BURN_NOTES:-}" ]; then
    cp "$FERRATA_BURN_NOTES" "$cp/burn-notes.json"
    "${standin_py[@]}" burns-from-notes --identity "$id" --notes "$cp/burn-notes.json" --out "$cp/burns.json"
  fi
  if [ ! -f "$cp/burns.json" ]; then
    [ "${FERRATA_FORK:-}" = 1 ] || [ "${FERRATA_NATIVE_BURNS:-}" = yes ] || die "no burns: set FERRATA_WALLET_SEED_TEXT, FERRATA_BURN_NOTES or FERRATA_BURNS (INTERFACE.md)"
    say "attest: STAND-IN burns of a labelled test wallet, computed natively (a rehearsal; not from LEZ blocks)"
    "${standin_py[@]}" native-burns --identity "$id" --recipient "$ETH_FROM" --out "$cp"
  fi
  signers="$(jq -r '.standIn.signers | join(",")' "$plan")"
  imported="${FERRATA_IMPORTED:-}"
  local layout="${FERRATA_LEZ_LAYOUT:-}" leaves
  if [ "$(cast code "$(jq -r .vault "$plan")" --rpc-url "$SEPOLIA_RPC_URL")" != 0x ]; then
    parent="$(call 'finalState()(bytes32)')"
    leaves="$(call 'nextLeafIndex()(uint64)' | word)"
    if [ -z "$layout" ] && [ -n "${FERRATA_LEZ_PROGRAM:-}" ]; then
      # lez-v03-bridge's own layout, derived from its program id and the configuration (INTERFACE.md §1).
      layout="$cp/layout.json"
      "${standin_py[@]}" layout --program "$FERRATA_LEZ_PROGRAM" --config-id "$(jq -r .lezConfigId "$plan")" \
        --imported "${imported:-$leaves}" --out "$layout"
    fi
    [ -z "$layout" ] && [ -z "$imported" ] && imported="$leaves"
    [ -n "$layout" ] && args+=(--layout "$layout")
    # The deposit root LEZ imported must be the vault's, when the layout names it and LEZ imported every leaf.
    if [ -n "$layout" ] && jq -e .imported_root "$layout" > /dev/null && [ "${imported:-$leaves}" = "$leaves" ] \
        && [ "${FERRATA_SKIP_DEPOSIT_ROOT:-}" != yes ]; then
      args+=(--deposit-root "$(call 'depositRoot()(bytes32)')")
    fi
  else
    # Before the vault exists (a rehearsal signs ahead of its fork): the first checkpoint builds on the genesis state.
    [ "${FERRATA_NATIVE_BURNS:-}" = yes ] || die "the vault is not deployed"
    parent="$(jq -r .genesisState "$plan")"
    [ -n "$imported" ] || imported="$(jq '.deposits | length' "$plan")"
  fi
  [ -n "$imported" ] && [ -z "${FERRATA_LEZ_PROGRAM:-}" ] && args+=(--imported "$imported")
  [ -n "${FERRATA_LEZ_BLOCK:-}" ] && args+=(--block "$FERRATA_LEZ_BLOCK")
  "${standin_py[@]}" checkpoint --identity "$id" --burns "$cp/burns.json" --parent-state "$parent" --signers "$signers" \
    --threshold "$(jq -r .standIn.threshold "$plan")" --rpc "$lez_rpc" "${args[@]}" --out "$cp/checkpoint.json"
  "${standin_py[@]}" sign --checkpoint "$cp/checkpoint.json" --verifier "$(jq -r .standIn.address "$plan")" \
    --keys "$(keys_dir)" --out "$cp/candidate.json"
  "${standin_py[@]}" check-seal --candidate "$cp/candidate.json"
  jq --slurpfile c "$cp/candidate.json" '.candidate = {journal: $c[0].journal, seal: $c[0].seal}' "$crown" > "$crown.tmp" && mv "$crown.tmp" "$crown"
  say "attest: burn root $(jq -r .checkpoint.burnRoot "$cp/candidate.json") over $(jq -r .checkpoint.burnCount "$cp/candidate.json") burns at LEZ block $(jq -r .checkpoint.tipId "$cp/candidate.json"), signed by $(jq -c .signedBy "$cp/candidate.json") (STAND-IN)"
}

# ---- prove: the release proofs (REAL), each a CPU proof of the unchanged release program wrapped to Groth16 ----------
proofs() { [ -f "$run/releases/releases.json" ] && jq -r '.[] | "release-" + .name' "$run/releases/releases.json"; }
do_prove() {
  local step="${1:-all}" rel="$run/releases" cp="$run/checkpoint" p
  [ -f "$programs/release.bin" ] || die "$programs/release.bin is missing (the recorded release program, image $release_id)"
  if [ "$step" = build ] || [ "$step" = all ]; then
    box -- "cd bridge && cargo build --release --locked -p ferrata-e2e 2>&1 | tail -n 3"
  fi
  if [ "$step" = inputs ] || [ "$step" = all ]; then
    [ -f "$cp/burns.json" ] && [ -f "$cp/burn-notes.json" ] || die "prove: $cp/burns.json and burn-notes.json first (attest)"
    box --put "$programs/release.bin" --put "$cp/burn-notes.json" -- "mkdir -p '$rel' && '$E' image-id '$programs/release.bin' | jq -c '.program = \"release.bin\"' > '$rel/program.json' && '$E' crown-release --identity '$id' --burns '$cp/burns.json' --notes '$cp/burn-notes.json' --out-dir '$rel'"
    [ "$(jq -r .image_id "$rel/program.json")" = "$release_id" ] || die "release.bin is not image $release_id"
  fi
  if [ "$step" = cpu ] || [ "$step" = all ]; then
    for p in $(proofs); do
      [ -f "$run/work/$p/input.json" ] && continue
      say "prove: $p on the CPU (succinct), then identity_p254"
      # The inputs hold the release secrets: they stay on the build machine and are made again here, not committed.
      box --put "$programs/release.bin" --put "$cp/burn-notes.json" --get "$run/work/$p" -- "'$E' crown-release --identity '$id' --burns '$cp/burns.json' --notes '$cp/burn-notes.json' --out-dir '$rel' > /dev/null && '$E' exec --program '$programs/release.bin' --input '$rel/$p.input' --meta '$rel/$p.json' > '$rel/$p.exec.json' && '$E' prove --program '$programs/release.bin' --input '$rel/$p.input' --meta '$rel/$p.json' --out-dir '$run/work/$p' > '$rel/$p.prove.json' && '$E' p254 --receipt '$run/work/$p/receipt-succinct.bin' --image-id $release_id --out-dir '$run/work/$p' > '$rel/$p.p254.json'"
    done
  fi
  if [ "$step" = wrap ] || [ "$step" = all ]; then
    for p in $(proofs); do
      [ -f "$run/work/$p/proof.json" ] && continue
      say "prove: $p in RISC Zero's Groth16 prover image"
      if [ -n "${FERRATA_MWRAP:-}" ]; then
        "$FERRATA_MWRAP" "$run/work/$p/input.json" "$run/work/$p/proof.json" > "$run/logs/$p.wrap.log" 2>&1 || die "the prover image failed: $run/logs/$p.wrap.log"
      else
        command -v docker > /dev/null || die "docker is not installed (or set FERRATA_MWRAP)"
        docker run --rm -v "$run/work/$p:/mnt" risczero/risc0-groth16-prover:v2025-04-03.1 > "$run/logs/$p.wrap.log" 2>&1 || die "the prover image failed"
      fi
    done
  fi
  if [ "$step" = seal ] || [ "$step" = all ]; then
    for p in $(proofs); do
      [ -f "$rel/receipts/$p/seal.hex" ] && continue
      box --put "$run/work/$p" -- "'$E' groth16 --receipt '$run/work/$p/receipt-succinct.bin' --image-id $release_id --proof '$run/work/$p/proof.json' --out-dir '$rel/receipts/$p' > '$rel/$p.groth16.json' && '$E' verify --receipt '$rel/receipts/$p/receipt-groth16.bin' --image-id $release_id --meta '$rel/$p.json' | jq -c '.receipt = \"receipt-groth16.bin\"' > '$rel/$p.verify.json'"
    done
    # crown.json's releases[]: the wallet's statements with their seals, the journal each proof committed checked.
    local rows='[]' e pn seal
    while read -r e; do
      pn="release-$(jq -r .name <<< "$e")"
      [ "$(tr -d ' \n' < "$rel/receipts/$pn/journal.hex" | sed 's/^0x//')" = "$(jq -r .journal "$rel/$pn.json")" ] || die "$pn: the proof's journal is not the native one"
      seal="0x$(tr -d ' \n' < "$rel/receipts/$pn/seal.hex" | sed 's/^0x//')"
      rows="$(jq -c --argjson e "$e" --arg s "$seal" '. + [$e + {seal: $s, relayFee: "0"}]' <<< "$rows")"
      [ "$(jq -r .relayFee <<< "$e" | sed 's/^0x0*$//')" = "" ] || die "$pn: a relay fee other than 0"
    done < <(jq -c '.[]' "$rel/releases.json")
    jq --argjson r "$rows" '.releases = $r' "$crown" > "$crown.tmp" && mv "$crown.tmp" "$crown"
    say "prove: $(jq '.releases | length' "$crown") release proofs of image $release_id, Groth16 seals in crown.json"
  fi
}

# ---- the stages that send ------------------------------------------------------------------------------------------
candidate_times() { # prints: submittedAt deltaAtSubmit
  call 'candidates(bytes32)(bytes32,bytes32,uint64,uint32,uint48,uint32,uint64,address)' "$(jq -r .checkpoint.newState "$run/checkpoint/candidate.json")" |
    sed -n '5p;6p' | awk '{print $1}' | tr '\n' ' '
}
do_stage() {
  local stage="$1" arg="${2:-}"
  case "$stage" in
    plan) do_plan ;;
    deploy) do_deploy ;;
    deposit) sepolia deposit "$arg"; say "deposit $arg: leaf count $(call 'nextLeafIndex()(uint64)' | word)" ;;
    attest) do_attest ;;
    refusals) "${verify_py[@]}" refusals "$run" --rpc "$SEPOLIA_RPC_URL" --phase "$arg" | tee -a "$run/logs/refusals-$arg.txt" ;;
    submit) sepolia submit ;;
    wait)
      read -r at delta <<< "$(candidate_times)"
      [ "${at:-0}" != 0 ] || die "the candidate is not submitted"
      pass_time "$((at + delta))" "the candidate's wait delta" ;;
    finalize) sepolia finalize ;;
    prove) do_prove "${arg:-all}" ;;
    release-wait)
      local fin; fin="$(call 'burnRootStatus(bytes32)(bool,uint256)' "$(jq -r .checkpoint.burnRoot "$run/checkpoint/candidate.json")" | sed -n 2p | awk '{print $1}')"
      [ "${fin:-0}" != 0 ] || die "the burn root is not final"
      pass_time "$((fin + $(jq -r .releaseDelay "$plan")))" "the release delay" ;;
    release) sepolia release "$arg" ;;
    status) sepolia status ;;
    gas) gas_table ;;
    *) die "unknown stage $stage" ;;
  esac
}
gas_table() {
  jq -r '.stages | to_entries[] | .key as $s | .value[] | [$s, (.contractAddress // .to), .gasUsed, .blockNumber, .hash] | @tsv' "$manifest" |
    while IFS=$'\t' read -r s to g b h; do printf '%s\t%s\t%d\t%d\t%s\n' "$s" "$to" "$g" "$b" "$h"; done > "$run/gas.tsv"
  awk -F'\t' '{printf "  %-18s %10d gas  block %d  %s\n", $1, $3, $4, $5; t += $3} END {printf "  %-18s %10d gas in %d transactions\n", "total", t, NR}' "$run/gas.tsv"
}

case "$mode" in
plan) do_plan ;;
attest) do_attest ;; # sends nothing: reads the LEZ node (and the vault's final state), signs with the stand-in keys
prove) do_prove "${1:-all}" ;;
verify)
  if [ "${1:-}" = --fork ]; then "${verify_py[@]}" check "$run" --rpc "$SEPOLIA_RPC_URL" --fork; else
    case "$SEPOLIA_RPC_URL" in http://127.0.0.1:* | http://localhost:*) die "verify reads a public Sepolia endpoint (verify --fork for the rehearsal's local one)" ;; esac
    "${verify_py[@]}" check "$run" --rpc "$SEPOLIA_RPC_URL" --lez-rpc "$lez_rpc" | tee "$run/verify.txt"
  fi ;;
live)
  [ "${FERRATA_SEPOLIA_SEND:-}" = yes ] || die "live sends transactions to Sepolia: it runs only with FERRATA_SEPOLIA_SEND=yes (the owner's decision)"
  [ "${FERRATA_FORK:-}" != 1 ] && [ "${FERRATA_UNLOCKED:-}" != 1 ] || die "live is not a fork: unset FERRATA_FORK and FERRATA_UNLOCKED"
  case "$SEPOLIA_RPC_URL" in http://127.0.0.1:* | http://localhost:*) die "live is for Sepolia, not a local endpoint" ;; esac
  [ -n "${ETH_KEYSTORE_ACCOUNT:-}" ] || { [ -f "${ETH_KEYSTORE:-}" ] && [ -f "${ETH_PASSWORD:-}" ]; } ||
    die "no signer: set ETH_KEYSTORE and ETH_PASSWORD to the keystore file and the password file, or ETH_KEYSTORE_ACCOUNT"
  [ -f "$plan" ] || die "no plan: e2e/testnet-return.sh plan $name first"
  [ "$(echo "$ETH_FROM" | tr 'A-F' 'a-f')" = "$(jq -r '.deployer | ascii_downcase' "$plan")" ] || die "ETH_FROM is not the plan's deployer"
  [ "$(cast chain-id --rpc-url "$SEPOLIA_RPC_URL")" = 11155111 ] || die "the endpoint is not Sepolia"
  # ~72 M gas at twice the gas price of now, and the two deposits.
  [ "$(cast balance "$ETH_FROM" --rpc-url "$SEPOLIA_RPC_URL")" -ge $((72000000 * 2 * $(cast gas-price --rpc-url "$SEPOLIA_RPC_URL") + 2004000000000000)) ] ||
    die "the sender cannot pay ~72M gas at twice the gas price of now plus the deposits"
  [ "$(jq -r .lezConfigIdIsPlaceholder "$plan")" = false ] || die "the plan's lezConfigId is a placeholder: set FERRATA_LEZ_CONFIG_ID and plan again"
  exec > >(tee -a "$run/logs/live.log") 2>&1
  stage="${1:?stage}"
  if [ "$stage" = all ]; then
    do_stage deploy; do_stage deposit 0; do_stage deposit 1
    if [ ! -f "$run/checkpoint/candidate.json" ] && [ -z "${FERRATA_BURNS:-}${FERRATA_BURN_NOTES:-}${FERRATA_WALLET_SEED_TEXT:-}" ]; then
      say "the deposits are in the vault. Now the LEZ side: import, mint, burn. Then run this again with FERRATA_WALLET_SEED_TEXT (or FERRATA_BURN_NOTES, FERRATA_BURNS) and FERRATA_LEZ_PROGRAM (or FERRATA_LEZ_LAYOUT) set; it stops unless the burns' block is Finalized on the LEZ node."
      exit 0
    fi
    do_stage attest; do_stage refusals pre-submit; do_stage submit; do_stage prove; do_stage refusals pre-final
    do_stage wait; do_stage finalize; do_stage release-wait; do_stage release 0; do_stage release 1
    do_stage refusals post-release; do_stage gas
    say "DONE: now e2e/testnet-return.sh verify $name"
  else
    do_stage "$stage" "${2:-}"
  fi ;;
fork)
  port="${FERRATA_FORK_PORT:-8545}"
  fork_url="${FERRATA_FORK_URL:-$SEPOLIA_RPC_URL}"
  case "$fork_url" in http://127.0.0.1:* | http://localhost:*) die "FERRATA_FORK_URL must be a Sepolia endpoint to fork from" ;; esac
  exec > >(tee -a "$run/logs/fork.log") 2>&1
  anvil --fork-url "$fork_url" --port "$port" --auto-impersonate > "$run/logs/anvil.log" 2>&1 &
  anvil_pid=$!
  trap 'kill "$anvil_pid" 2> /dev/null || true' EXIT
  export SEPOLIA_RPC_URL="http://127.0.0.1:$port" FERRATA_FORK=1 FERRATA_UNLOCKED=1
  unset ETH_KEYSTORE ETH_PASSWORD ETH_KEYSTORE_ACCOUNT
  for _ in $(seq 1 90); do cast chain-id --rpc-url "$SEPOLIA_RPC_URL" > /dev/null 2>&1 && break; sleep 1; done
  say "fork of chain $(cast chain-id --rpc-url "$SEPOLIA_RPC_URL") at block $(cast block-number --rpc-url "$SEPOLIA_RPC_URL") (from $fork_url); nothing is sent to Sepolia"
  cast rpc --rpc-url "$SEPOLIA_RPC_URL" anvil_setBalance "$ETH_FROM" 0x3635c9adc5dea00000 > /dev/null # 1000 ETH, on the fork only
  [ -f "$plan" ] || do_plan
  [ "$(cast nonce "$ETH_FROM" --rpc-url "$SEPOLIA_RPC_URL")" = "$(jq -r .firstNonce "$plan")" ] || die "the sender's nonce on the fork is not the plan's first nonce: plan again"
  rm -f "$repo/vault/broadcast/DeployStandIn.s.sol/11155111/"*-latest.json "$repo/vault/broadcast/Crown.s.sol/11155111/"*-latest.json
  do_stage deploy
  do_stage deposit 0; do_stage deposit 1
  do_stage attest
  do_stage refusals pre-submit
  do_stage submit
  if [ "$(jq '[.releases // [] | .[] | select(.seal != null)] | length' "$crown")" != 2 ]; then do_stage prove; fi
  do_stage refusals pre-final
  do_stage wait; do_stage finalize
  do_stage release-wait; do_stage release 0; do_stage release 1
  do_stage refusals post-release
  do_stage gas
  "${verify_py[@]}" check "$run" --rpc "$SEPOLIA_RPC_URL" --fork | tee "$run/verify-fork.txt"
  [ "${PIPESTATUS[0]}" = 0 ] || die "verify found a difference"
  say "DONE (fork): $(wc -l < "$run/gas.tsv") transactions on a local fork of Sepolia; nothing was sent to any network" ;;
esac
