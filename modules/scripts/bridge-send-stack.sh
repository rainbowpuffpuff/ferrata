#!/usr/bin/env bash
# ferrata_bridge_module's Ethereum side through Logos Core's own modules, in one headless Logos Core on a local anvil
# (chain id 11155111, nothing public): the vault read through eth_rpc_module; the user's deposits (ETH, and an ERC-20's
# approve + deposit as one bundle) and the admin's and the freeze authority's vault calls sent by the module through
# tx_sender_module, each approved by a person at Logos's headless approver evm_signer_cli (keys imported with
# Logos's evm_keystore_cli into keystore_module; the module holds none); a refused approval; a frozen token's deposit
# refused before anything is signed; a send forced to revert after its approval (its own gas limit too low), reported
# as reverted; every other send with the margin over the stack's estimate (send.rs). Step 10 sends the vault actions
# left, each from the account that may send it: the global cap, delta raised, the admin role handed to the user and
# back (each hand-over accepted by the pending admin), the four other freezes set and lifted, the watchers proposed and
# applied by anyone after their 14-day time lock and step 3c's fees after their seven days (anvil's clock moved on),
# and last the vault retired by its freeze authority. What the vault refuses on this fixture (delta out of its bounds,
# a revocation with no checkpoint, roles applied early, retirement by anyone before retireOpensAt) is sent too: the
# module must name the vault's error before anything is signed, the approver getting no request. Every effect read
# back with Foundry's cast.
#
#   modules/scripts/bridge-send-stack.sh build OUT   the send stack and the two CLIs (send-stack.sh build, builder
#                                                    5f74cdb grafted), ferrata_bridge_module's dev .lgx and logosctl
#                                                    (check.sh); then /nix saved to $NIX_SAVE when it is set
#   modules/scripts/bridge-send-stack.sh build-module OUT   only the module's .lgx and logosctl (check.sh), the send
#                                                    stack's .lgx already in OUT/stack (built by `build`, or linked)
#   modules/scripts/bridge-send-stack.sh run OUT     the run: OUT/run/ (calls.jsonl, prompts, receipts), OUT/checks.tsv
# The module's .lgx hash is reproducible only with the same build directory: Nix builds it unsandboxed under
# $LOGOS_TMPDIR/nix-build-..., and that path ends up in the .so (panic locations, crate-metadata symbol hashes). Both
# build modes use LOGOS_TMPDIR=/var/tmp/bss (MODULE_TMPDIR overrides it); another directory gives another hash for the
# same source.
# Box only (spike/modal-build/mrun: Nix, a 1 GiB stack, Foundry 1.3.6).
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
MODE=${1:?build|build-module|run}; OUT=$(mkdir -p "${2:?OUT}" && cd "$2" && pwd)
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
STACK="eth_rpc_module fee_module keystore_module signer_manager_module tx_sender_module evm_signer_cli evm_keystore_cli"

build() {
  local t0=$(date +%s)
  ONLY="$STACK" bash "$here/send-stack.sh" build "$OUT/stack" 2>&1 | tail -n 40
  echo "send stack: $(( $(date +%s) - t0 )) s"
  build_module
  if [ -n "${NIX_SAVE:-}" ]; then
    t0=$(date +%s)
    tar -C / -cf - nix | zstd -q -T0 -3 -o "$NIX_SAVE.part" -f && mv "$NIX_SAVE.part" "$NIX_SAVE"
    echo "nix saved: $(du -h "$NIX_SAVE" | cut -f1) in $(( $(date +%s) - t0 )) s"
  fi
}

# The module's dev .lgx and logosctl (check.sh), in the fixed build directory (see the header), and their hashes.
build_module() {
  local t0=$(date +%s)
  MODULES=ferrata_bridge_module PORTABLE=0 E2E=0 LOGOS_TMPDIR="${MODULE_TMPDIR:-/var/tmp/bss}" RISC0_DEV_MODE=1 bash "$here/check.sh" "$OUT/check" > "$OUT/check.log" 2>&1
  echo "check.sh rc=$? ($(( $(date +%s) - t0 )) s, LOGOS_TMPDIR=${MODULE_TMPDIR:-/var/tmp/bss})"; tail -n 12 "$OUT/check/report/summary.md" 2>/dev/null
  ( cd "$OUT" && sha256sum stack/lgx-*/*.lgx check/lgx-ferrata_bridge_module/*.lgx 2>/dev/null ) | sed "s#  .*/#  #" > "$OUT/lgx-sha256.txt"
  echo "LOGOS_TMPDIR=${MODULE_TMPDIR:-/var/tmp/bss}" >> "$OUT/lgx-sha256.txt"
  ls -d "$OUT"/stack/lgx-* "$OUT"/check/lgx-ferrata_bridge_module "$OUT"/check/logos/bin/logosctl 2>&1
}

run() {
  local LOGOSCTL="${LOGOSCTL:-$OUT/check/logos/bin/logosctl}" LT="${LOGOS_TMPDIR:-/var/tmp/bss}" S="$OUT/session" R="$OUT/run"
  local RPC=http://127.0.0.1:18547 CHAIN=11155111 ETH=0x0000000000000000000000000000000000000000
  mkdir -p "$LT"; rm -rf "${S:?}" "${R:?}" "$OUT/home"; mkdir -p "$OUT/home" "$R/keys"; chmod 700 "$R/keys"
  : > "$R/calls.jsonl"; : > "$OUT/checks.tsv"
  local pass=0 fail=0
  check() { # NAME EXPECTED ACTUAL
    if [ "$2" = "$3" ]; then pass=$((pass + 1)); echo "PASS $1"; else fail=$((fail + 1)); echo "FAIL $1: expected [$2], got [$3]"; fi
    printf '%s\t%s\t%s\t%s\n' "$([ "$2" = "$3" ] && echo PASS || echo FAIL)" "$1" "$2" "$3" >> "$OUT/checks.tsv"
  }
  ctl() { HOME="$OUT/home" TMPDIR="$LT" "$LOGOSCTL" --config-dir "$S" "$@"; }
  # j MODULE METHOD ARGS...: one call through Logos Core, recorded; its reply (the method's JSON) on stdout.
  j() {
    local m="$1" f="$2" out rc; shift 2
    out="$(ctl --timeout 900000 call "$m" "$f" "$@" --json < /dev/null 2>&1)"; rc=$?
    local reply; reply=$(jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else (.result // .) end' <<< "$out" 2>/dev/null || echo "\"$out\"")
    [ -n "$reply" ] || reply=$(jq -R -s -c . <<< "$out")
    jq -c -n --arg m "$m" --arg f "$f" --argjson rc "$rc" --argjson r "$reply" --args '{module: $m, method: $f, args: $ARGS.positional, exit: $rc, reply: $r}' -- "$@" >> "$R/calls.jsonl"
    echo "$reply"
  }
  low() { tr 'A-F' 'a-f'; }
  # Anvil's well-known test accounts: 1 the admin (the deployer), 2 the freeze authority, 3 the user.
  local K1=59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d K2=5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a
  local K3=7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6
  local A1=0x70997970c51812dc3a010c7d01b50e0d17dc79c8 A2=0x3c44cdddb6a900fa2b585dd299e03d12fa4293bc A3=0x90f79bf6eb2c4f870365e785982e1f101e93b906
  ( umask 077; printf '%s' "$K1" > "$R/keys/admin.key"; printf '%s' "$K2" > "$R/keys/freeze.key"; printf '%s' "$K3" > "$R/keys/user.key"
    printf '%s' "ferrata devnet only" > "$R/keys/password" )
  local PW="$R/keys/password"

  anvil --port 18547 --chain-id $CHAIN > "$R/anvil.log" 2>&1 & local anvil_pid=$!
  for _ in $(seq 60); do cast chain-id --rpc-url $RPC > /dev/null 2>&1 && break; sleep 0.5; done
  ( cd "$repo/vault" && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.1.0; } \
    && FERRATA_FREEZE_AUTHORITY=$A2 forge script script/LogosSendStack.s.sol:LogosSendStack --rpc-url $RPC --broadcast --private-key $K1 ) > "$R/deploy.log" 2>&1 \
    || { tail -n 30 "$R/deploy.log"; kill $anvil_pid; return 1; }
  local V T
  V=$(grep -E '^\s+vault 0x' "$R/deploy.log" | awk '{print $2}' | low); T=$(grep -E '^\s+token 0x' "$R/deploy.log" | awk '{print $2}' | low)
  echo "vault $V token $T" | tee "$R/addresses.txt"
  cast send $T 'mint(address,uint256)' $A3 100000000 --from $A3 --unlocked --rpc-url $RPC > /dev/null 2>&1

  # ---- one Logos Core: Logos's send stack and its headless approver and custodian, and ferrata_bridge_module ----
  # FERRATA_DEPLOYMENTS: the one deployment ferrata_bridge_module may send to (this vault on this chain), set by the
  # person who starts Logos Core; the module refuses any other (calls/src/send.rs `pinned`).
  FERRATA_DEPLOYMENTS="$CHAIN:$V" HOME="$OUT/home" TMPDIR="$LT" "$LOGOSCTL" --config-dir "$S" daemon start --detach > "$R/daemon.txt" 2>&1; sleep 3
  local d m
  for d in "$OUT"/stack/lgx-*/ "$OUT/check/lgx-ferrata_bridge_module/"; do ctl install "$d"/*.lgx -y 2>&1 | tail -n 1; done > "$R/install.log"
  # The providers first, then the approver (it must be up before the first send: an unclaimed request is swept after
  # 60 s), the custodian, and the bridge module.
  for m in keystore_module signer_manager_module eth_rpc_module fee_module tx_sender_module evm_signer_cli evm_keystore_cli ferrata_bridge_module; do
    ctl module load "$m" 2>&1 | tail -n 1
  done > "$R/load.log"
  ctl module list --json > "$R/modules-loaded.json" 2>&1
  for m in keystore_module signer_manager_module eth_rpc_module fee_module tx_sender_module evm_signer_cli evm_keystore_cli ferrata_bridge_module; do
    check "loaded: $m" "loaded" "$(jq -r --arg m "$m" '.[] | select(.name == $m) | .status' "$R/modules-loaded.json" 2>/dev/null | head -1)"
  done
  # Roles, as Logos's own headless doc-test names them (logos-eth-wallet-backend doctests/headless-send.test.yaml):
  # configure is total; the Basecamp surfaces are kept beside the two headless ones.
  j keystore_module configure '{"approvers":["evm_signer_ui","evm_signer_cli"],"custodians":["evm_keystore_ui","evm_keystore_cli"],"managers":["signer_manager_module"]}' > /dev/null
  j signer_manager_module configure '{"approvers":["evm_signer_ui","evm_signer_cli"],"custodians":["evm_keystore_ui"],"signers":["keystore_module"]}' > /dev/null
  j eth_rpc_module init_defaults > /dev/null
  j eth_rpc_module set_chain_config $CHAIN "{\"endpoint\":\"$RPC\"}" > /dev/null
  j eth_rpc_module set_chain_enabled $CHAIN true > /dev/null
  check "eth_rpc_module: anvil's chain id" "true" "$(j eth_rpc_module verify_chain_id $CHAIN | jq -r '.ok // .matches // false' 2>/dev/null)"
  for who in admin freeze user; do
    j evm_keystore_cli import_private_key "@$R/keys/$who.key" "@$PW" > "$R/import-$who.json"
  done
  sleep 2
  j signer_manager_module accounts > "$R/accounts.json"
  for a in $A1 $A2 $A3; do
    check "keystore_module holds $a (via evm_keystore_cli)" "yes" "$(grep -qi "${a#0x}" "$R/accounts.json" && echo yes)"
  done
  # The approver's prompts, as a person on duty sees them.
  ctl watch evm_signer_cli --event prompt --json > "$R/prompts.jsonl" 2>&1 & local watch_pid=$!
  sleep 2

  local E="{\"via\":\"eth_rpc_module\",\"chain_id\":$CHAIN}"
  # approve_next NAME DECISION PASSWORD_FILE: wait for the approver's rendered request, record it, approve or reject.
  # Nothing on screen after 5 s: the request is looked up as a person would (the manager's queue as `evm_signer_cli
  # list` reads it, then tx_sender_module's live sends) and shown by handle (`evm_signer_cli show`); every such
  # recovery is recorded in recoveries.jsonl with what each side said (a review's run lost one approval this way:
  # prompted at 22:51:24, then gone from the approver's queue while tx_sender_module still awaited it).
  approve_next() {
    local name="$1" decision="$2" st h b="" q live want sh i
    for i in $(seq 240); do
      st=$(j evm_signer_cli status); h=$(jq -r '.rendered.handle // empty' <<< "$st"); [ -n "$h" ] && break
      if [ $(( i % 10 )) = 0 ]; then
        q=$(j evm_signer_cli list); live=$(j tx_sender_module live_sends)
        want=$(jq -r '[.pending[]? | select(.requester == "tx_sender_module") | .handle] | first // empty' <<< "$q" 2>/dev/null)
        [ -n "$want" ] || want=$(jq -r '[.sends[]? | select(.status == "awaitingApproval") | .handle] | last // empty' <<< "$live" 2>/dev/null)
        if [ -n "$want" ]; then
          sh=$(j evm_signer_cli show "str:$want")
          jq -c -n --arg name "$name" --argjson poll "$i" --arg handle "$want" --argjson status "${st:-null}" --argjson list "${q:-null}" \
            --argjson live "${live:-null}" --argjson show "$(jq -c 'del(.text, .render_lines, .claim_lines, .interpretation_lines)' <<< "${sh:-null}" 2>/dev/null || echo null)" \
            '{name: $name, poll: $poll, handle: $handle, status: $status, list: $list, live_sends: $live, show: $show}' >> "$R/recoveries.jsonl" 2>/dev/null \
            || echo "{\"name\":\"$name\",\"poll\":$i,\"handle\":\"$want\"}" >> "$R/recoveries.jsonl"
          if [ "$(jq -r '.ok // false' <<< "$sh" 2>/dev/null)" = true ]; then h="$want"; b=$(jq -r '.bundle_id' <<< "$sh"); break; fi
        fi
      fi
      sleep 0.5
    done
    echo "$st" > "$R/approver-$name.json"
    [ -n "$h" ] || { echo "no request reached the approver ($name)"; return 1; }
    [ -n "$b" ] || b=$(jq -r '.rendered.bundle_id' <<< "$st")
    if [ "$decision" = approve ]; then j evm_signer_cli approve "str:$h" "str:$b" "@$3" > "$R/approve-$name.json"
    else j evm_signer_cli reject "str:$h" > "$R/approve-$name.json"; fi
  }
  # send_through NAME METHOD REQUEST DECISION PASSWORD_FILE: the module's call (it waits for the approval and the
  # broadcast) in the background, the person's decision at the approver, the module's reply in $R/NAME.json.
  send_through() {
    local name="$1" method="$2" req="$3" decision="$4" pwf="$5" bg t0
    echo "$req" > "$R/$name.request.json"
    t0=$(date +%s%3N)
    ( j ferrata_bridge_module "$method" "@$R/$name.request.json" > "$R/$name.json" ) & bg=$!
    approve_next "$name" "$decision" "$pwf"
    wait $bg
    echo "$(( $(date +%s%3N) - t0 ))" > "$R/$name.ms"
  }

  # 1. the admin's view, read through eth_rpc_module.
  local S1; S1=$(j ferrata_bridge_module admin "{\"action\":\"status\",\"vault\":\"$V\",\"eth\":$E,\"tokens\":[\"$ETH\",\"$T\"]}")
  echo "$S1" > "$R/status-before.json"
  check "status through eth_rpc_module: the admin" "$A1" "$(jq -r .roles.admin <<< "$S1")"
  check "status: the freeze authority" "$A2" "$(jq -r .roles.freeze_authority <<< "$S1")"
  check "status: answered by eth_rpc_module" "eth_rpc_module" "$(jq -r .eth.via <<< "$S1")"
  check "status: the test token not registered yet" "false" "$(jq -r '.registry[1].token.registered' <<< "$S1")"

  # 1b. Only the deployment the person named (FERRATA_DEPLOYMENTS at the daemon's start): a deposit naming another
  # vault (here the test token's address) is refused before anything is read from it or asked of the signer.
  local OV; OV=$(j ferrata_bridge_module deposit "{\"bridge_seed\":\"0x$(printf '5e%.0s' $(seq 32))\",\"chain_id\":$CHAIN,\"vault\":\"$T\",\"depositor\":\"$A3\",\"kind\":\"private\",\"dest\":\"0x$(printf 'de%.0s' $(seq 32))\",\"eth\":$E,\"token\":\"$ETH\",\"denom_index\":0,\"send\":{\"via\":\"tx_sender_module\",\"from\":\"$A3\",\"chain_id\":$CHAIN}}")
  echo "$OV" > "$R/deposit-other-vault.json"
  check "pinned deployment: a deposit naming another vault is refused before anything is asked" "yes" \
    "$(jq -r '.error // ""' <<< "$OV" | grep -q "^refused before anything was asked: vault $T on chain $CHAIN is not a deployment this module may send to" && echo yes)"
  check "pinned deployment: no send reached tx_sender_module" "0" "$(j tx_sender_module live_sends | jq -r '.sends | length' 2>/dev/null)"

  # 2. the admin registers the test token through the send stack (one approval at evm_signer_cli).
  local REG="{\"token\":\"$T\",\"denominations\":[\"1000000\",\"10000000\"],\"relay_fees\":[\"1000\",\"10000\"],\"lez_unit\":\"1\",\"unit_weight\":\"1\",\
\"deposit_cap\":{\"capacity\":\"1000000000000\",\"refill_per_second\":\"1000\"},\"release_cap\":{\"capacity\":\"1000000000000\",\"refill_per_second\":\"1000\"},\
\"release_delay\":\"120\",\"lez_definition\":\"0x$(printf '%064d' 0)\",\"decimals\":6,\"checkpoint_fees\":[\"0\",\"0\"]}"
  local SEND1="{\"via\":\"tx_sender_module\",\"from\":\"$A1\",\"chain_id\":$CHAIN}"
  send_through admin-register admin "{\"action\":\"register_token\",\"vault\":\"$V\",\"eth\":$E,\"params\":$REG,\"send\":$SEND1}" approve "$PW"
  check "admin register_token: settled through tx_sender_module" "settled" "$(jq -r .verdict "$R/admin-register.json")"
  check "admin register_token: the vault holds the token (cast)" "true" "$(cast call $V 'tokenConfig(address)(bool,bool,uint256,uint256,uint256[])' $T --rpc-url $RPC | head -1)"
  check "admin register_token: sent by the admin (cast)" "$A1" "$(cast tx "$(jq -r .tx_hash "$R/admin-register.json")" from --rpc-url $RPC | low)"
  check "admin register_token: the approver saw tx_sender_module's request" "tx_sender_module" "$(jq -r .rendered.requester "$R/approver-admin-register.json")"

  # 3. the admin's cap through the send stack.
  send_through admin-cap admin "{\"action\":\"set_token_cap\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$T\",\"direction\":\"deposit\",\"capacity\":\"2000000000000\",\"refill_per_second\":\"5000\"},\"send\":$SEND1}" approve "$PW"
  check "admin set_token_cap: settled" "settled" "$(jq -r .verdict "$R/admin-cap.json")"
  check "admin set_token_cap: cast reads it" "2000000000000" "$(cast call $V 'tokenCap(address,uint8)(uint256,uint256,uint256)' $T 0 --rpc-url $RPC | head -1 | awk '{print $1}')"
  # The limit it was sent with: the stack's own estimate + 25% + 20000 (send.rs margin), above what it used.
  check "admin set_token_cap: sent with the estimate + 25% + 20000" "yes" \
    "$(jq -r '.send.gas[0] | if .limit == (.estimate + (.estimate / 4 | floor) + 20000) then "yes" else "no: \(.)" end' "$R/admin-cap.json")"
  check "admin set_token_cap: the receipt's gas used below its limit (cast)" "yes" \
    "$(u=$(cast receipt "$(jq -r .tx_hash "$R/admin-cap.json")" gasUsed --rpc-url $RPC); l=$(cast tx "$(jq -r .tx_hash "$R/admin-cap.json")" gas --rpc-url $RPC); [ "$u" -lt "$l" ] && echo yes || echo "no: used $u of $l")"

  # 3b. A send approved, broadcast, mined and REVERTED (forced: the caller's own gas limit of 30000, below what
  # setTokenCap needs): the module must report it as reverted, naming the call, and nothing must change.
  send_through admin-cap-reverts admin "{\"action\":\"set_token_cap\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$T\",\"direction\":\"deposit\",\"capacity\":\"3000000000000\",\"refill_per_second\":\"5000\"},\"send\":{\"via\":\"tx_sender_module\",\"from\":\"$A1\",\"chain_id\":$CHAIN,\"gas_limit\":30000}}" approve "$PW"
  check "forced revert: the module reports it reverted" "reverted false" "$(jq -r '"\(.verdict) \(.settled)"' "$R/admin-cap-reverts.json")"
  check "forced revert: the error names the call and its limit" "yes" "$(jq -r '.error // ""' "$R/admin-cap-reverts.json" | grep -q 'reverted: call 1 of 1.*of its limit 30000' && echo yes)"
  check "forced revert: the receipt's status is 0 (cast)" "0" "$(cast receipt "$(jq -r .tx_hash "$R/admin-cap-reverts.json")" status --rpc-url $RPC 2>/dev/null | awk '{print $1}')"
  check "forced revert: the cap is unchanged (cast)" "2000000000000" "$(cast call $V 'tokenCap(address,uint8)(uint256,uint256,uint256)' $T 0 --rpc-url $RPC | head -1 | awk '{print $1}')"

  # 3c. The admin's other settings through the stack, each read back with cast: delisting the token and listing it
  # again, a fee schedule announced (a mint fee of 0.1%: announcing the fees already in force withdraws a pending
  # schedule instead, FerrataRegistry.announceFees), a release delay set.
  send_through admin-delist admin "{\"action\":\"set_deposits_enabled\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$T\",\"enabled\":false},\"send\":$SEND1}" approve "$PW"
  check "admin set_deposits_enabled(false): settled, deposits disabled (cast)" "settled false" \
    "$(jq -r .verdict "$R/admin-delist.json") $(cast call $V 'tokenConfig(address)(bool,bool,uint256,uint256,uint256[])' $T --rpc-url $RPC | sed -n 2p)"

  # 4. the user's ETH deposit, built and sent by the module (one approval).
  local SEED="0x$(printf '5e%.0s' $(seq 32))" DEST="0x$(printf 'de%.0s' $(seq 32))"
  local SEND3="{\"via\":\"tx_sender_module\",\"from\":\"$A3\",\"chain_id\":$CHAIN}"
  local DEP="\"bridge_seed\":\"$SEED\",\"chain_id\":$CHAIN,\"vault\":\"$V\",\"depositor\":\"$A3\",\"kind\":\"private\",\"dest\":\"$DEST\",\"eth\":$E"
  # While the token is delisted the module refuses its deposit before anything is asked (the vault's views).
  local DL; DL=$(j ferrata_bridge_module deposit "{$DEP,\"token\":\"$T\",\"denom_index\":0,\"send\":$SEND3}")
  echo "$DL" > "$R/deposit-delisted.json"
  check "delisted: the module refuses the ERC-20 deposit before signing" "DepositsDisabled" "$(jq -r '.error // ""' <<< "$DL" | grep -o DepositsDisabled | head -1)"
  send_through admin-list admin "{\"action\":\"set_deposits_enabled\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$T\",\"enabled\":true},\"send\":$SEND1}" approve "$PW"
  check "admin set_deposits_enabled(true): settled, deposits enabled (cast)" "settled true" \
    "$(jq -r .verdict "$R/admin-list.json") $(cast call $V 'tokenConfig(address)(bool,bool,uint256,uint256,uint256[])' $T --rpc-url $RPC | sed -n 2p)"
  send_through admin-fees admin "{\"action\":\"announce_fees\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$T\",\"mint_fees\":[\"1000\",\"10000\"],\"burn_fees\":[\"0\",\"0\"],\"checkpoint_fees\":[\"0\",\"0\"]},\"send\":$SEND1}" approve "$PW"
  check "admin announce_fees: settled, a schedule pending (cast feeState: pending time > 0)" "settled yes" \
    "$(jq -r .verdict "$R/admin-fees.json") $(p=$(cast call $V 'feeState(address)(uint64,uint64)' $T --rpc-url $RPC | sed -n 2p | awk '{print $1}'); [ "${p:-0}" -gt 0 ] && echo yes || echo "no ($p)")"
  send_through admin-delay admin "{\"action\":\"set_release_delay\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$T\",\"delay\":\"600\"},\"send\":$SEND1}" approve "$PW"
  check "admin set_release_delay(600): settled, the next checkpoints' delay (cast)" "settled 600" \
    "$(jq -r .verdict "$R/admin-delay.json") $(cast call $V 'releaseDelayFor(address,uint256)(uint256)' $T 18446744073709551615 --rpc-url $RPC | awk '{print $1}')"
  local escrow0; escrow0=$(cast balance $V --rpc-url $RPC)
  send_through deposit-eth deposit "{$DEP,\"token\":\"$ETH\",\"denom_index\":0,\"send\":$SEND3}" approve "$PW"
  check "deposit ETH: settled through tx_sender_module" "settled" "$(jq -r .verdict "$R/deposit-eth.json")"
  check "deposit ETH: leaf 0" "0" "$(jq -r .sent.leaf_index "$R/deposit-eth.json")"
  check "deposit ETH: the vault's Deposit commitment is the module's" "$(jq -r .commitment "$R/deposit-eth.json")" \
    "$(cast logs --from-block 0 --address $V 'Deposit(address indexed,uint64,uint64,address indexed,uint256,bytes32)' --rpc-url $RPC --json | jq -r '.[0].data' | cut -c 195-258 | sed 's/^/0x/')"
  check "deposit ETH: the vault received the price" "$(jq -r .price.total "$R/deposit-eth.json")" "$(( $(cast balance $V --rpc-url $RPC) - escrow0 ))"
  check "deposit ETH: sent by the user" "$A3" "$(cast tx "$(jq -r .sent.tx_hash "$R/deposit-eth.json")" from --rpc-url $RPC | low)"

  # 5. the user's ERC-20 deposit: approve + deposit, one bundle, one approval.
  send_through deposit-erc20 deposit "{$DEP,\"token\":\"$T\",\"denom_index\":0,\"send\":$SEND3}" approve "$PW"
  check "deposit ERC-20: settled (approve + deposit, one approval)" "settled 2" "$(jq -r '.verdict + " " + (.send.hashes | length | tostring)' "$R/deposit-erc20.json")"
  check "deposit ERC-20: the vault holds the price" "$(jq -r .price.total "$R/deposit-erc20.json")" "$(cast call $T 'balanceOf(address)(uint256)' $V --rpc-url $RPC | awk '{print $1}')"
  check "deposit ERC-20: leaf 1" "1" "$(jq -r .sent.leaf_index "$R/deposit-erc20.json")"

  # 6. a deposit the person refuses at the approver: nothing signed, nothing sent.
  local n0; n0=$(cast call $V 'nextLeafIndex()(uint64)' --rpc-url $RPC)
  send_through deposit-refused deposit "{$DEP,\"token\":\"$ETH\",\"denom_index\":0,\"send\":$SEND3}" reject "$PW"
  check "refused at the approver: the module reports it" "rejected" "$(jq -r '.error // ""' "$R/deposit-refused.json" | grep -o '^rejected' | head -1)"
  check "refused at the approver: no deposit" "$n0" "$(cast call $V 'nextLeafIndex()(uint64)' --rpc-url $RPC)"

  # 7. the freeze authority freezes ETH deposits through the send stack; the module refuses a deposit before signing.
  local SEND2="{\"via\":\"tx_sender_module\",\"from\":\"$A2\",\"chain_id\":$CHAIN}"
  send_through freeze admin "{\"action\":\"set_token_deposits_frozen\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$ETH\",\"frozen\":true},\"send\":$SEND2}" approve "$PW"
  check "freeze ETH deposits: settled, sent by the freeze authority" "settled $A2" "$(jq -r .verdict "$R/freeze.json") $(cast tx "$(jq -r .tx_hash "$R/freeze.json")" from --rpc-url $RPC | low)"
  local FR; FR=$(j ferrata_bridge_module deposit "{$DEP,\"token\":\"$ETH\",\"denom_index\":0,\"send\":$SEND3}")
  echo "$FR" > "$R/deposit-frozen.json"
  check "frozen: the module refuses before anything is signed (the vault's error named)" "TokenDepositsAreFrozen" "$(jq -r '.error // ""' <<< "$FR" | grep -o TokenDepositsAreFrozen | head -1)"
  send_through unfreeze admin "{\"action\":\"set_token_deposits_frozen\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$ETH\",\"frozen\":false},\"send\":$SEND2}" approve "$PW"
  check "unfreeze: lifted (cast)" "0" "$(cast call $V 'tokenDepositsFrozenUntil(address)(uint256)' $ETH --rpc-url $RPC | awk '{print $1}')"

  # 8. the module's own reads of the result, through eth_rpc_module, against cast.
  local L; L=$(j ferrata_bridge_module admin "{\"action\":\"read\",\"what\":\"deposit_list\",\"vault\":\"$V\",\"eth\":$E}")
  echo "$L" > "$R/deposit-list.json"
  check "deposit list through eth_rpc_module: 2 deposits, root = depositRoot" "2 $(cast call $V 'depositRoot()(bytes32)' --rpc-url $RPC)" "$(jq -r '"\(.value.deposits | length) \(.value.root)"' <<< "$L")"
  check "the reads' route: eth_rpc_module, direct" "eth_rpc_module direct" "$(jq -r '"\(.eth.via) \(.eth.routes[0])"' <<< "$L")"

  # 9. A deposit broadcast and not mined in time (anvil's automine off; the request's own receipt wait, 8 s): the module
  # reports it broadcast and not yet mined, naming the transaction and asking not to send it again, never as failed;
  # once a block is mined it is there.
  cast rpc evm_setAutomine false --rpc-url $RPC > /dev/null
  send_through deposit-unmined deposit "{$DEP,\"token\":\"$ETH\",\"denom_index\":0,\"send\":{\"via\":\"tx_sender_module\",\"from\":\"$A3\",\"chain_id\":$CHAIN,\"receipt_timeout_s\":8}}" approve "$PW"
  local UH; UH=$(jq -r '.tx_hash // ""' "$R/deposit-unmined.json")
  check "not mined in time: reported broadcast and not yet mined, not failed" "unconfirmed true false" "$(jq -r '"\(.verdict) \(.pending) \(.settled)"' "$R/deposit-unmined.json")"
  check "not mined in time: the text names the transaction and says not to send it again" "yes" \
    "$(jq -r '.what // ""' "$R/deposit-unmined.json" | grep -F "$UH" | grep -q "Do not send this deposit again" && [ -n "$UH" ] && echo yes)"
  # eth_getTransactionReceipt answers null at once for a pending transaction (`cast receipt` would wait for it).
  check "not mined in time: no receipt yet (eth_getTransactionReceipt null)" "null" "$(cast rpc eth_getTransactionReceipt "$UH" --rpc-url $RPC 2>&1 | head -c 40)"
  cast rpc evm_mine --rpc-url $RPC > /dev/null; cast rpc evm_setAutomine true --rpc-url $RPC > /dev/null
  check "not mined in time: mined with the next block (cast receipt status 1)" "1" "$(cast receipt "$UH" status --rpc-url $RPC 2>/dev/null | awk '{print $1}')"

  # 10. The vault actions the steps above leave out (admin.rs `vault_action`), each sent by the module through the send
  # stack from the account that may send it and approved at evm_signer_cli (send_through: the keystore account is the
  # request's send.from; the three imported keys share one password file); every effect read back with cast against
  # the vault's own getters. The admin's from A1 (SEND1), the freeze authority's from A2 (SEND2), accept_admin from the
  # pending admin, `anyone`'s from A3, the user (SEND3), except retire: only the freeze authority may retire before
  # retireOpensAt(). Where the vault refuses on this fixture (raiseDelta out of its bounds; revokeFrom: no checkpoint is
  # ever finalised here; applyRoles before its time lock; retire by anyone before retireOpensAt) the same request is
  # sent straight to the module, which must name the vault's error from its eth_call before anything is signed: a valid
  # check too, with no request then at the approver nor any send at tx_sender_module. The admin ends as A1 again;
  # retire comes last.
  local Z=0x0000000000000000000000000000000000000000 MAXF d0 fs0
  local UNLIMITED=115792089237316195423570985008687907853269984665640564039457584007913129639935
  MAXF=$(cast call $V 'maxFreeze()(uint256)' --rpc-url $RPC | awk '{print $1}') # 30 days on this fixture
  vals() { awk '{print $1}' | paste -sd' ' -; } # cast's values on one line, without its [1e9] notes
  block_time() { # a block's timestamp, in decimal
    local t re='^(0x[0-9a-fA-F]+|[0-9]+)$'; t=$(cast block "$1" --field timestamp --rpc-url $RPC | awk '{print $1}')
    [[ $t =~ $re ]] && echo $(( t ))
  }
  sender() { cast tx "$(jq -r .tx_hash "$1")" from --rpc-url $RPC | low; } # who sent a reply's transaction
  # vault_send NAME ACTION PARAMS SEND: the action through the send stack, approved; the reply in $R/admin-NAME.json
  # (so the receipts check after this step covers it too).
  vault_send() { send_through "admin-$1" admin "{\"action\":\"$2\",\"vault\":\"$V\",\"eth\":$E,\"params\":$3,\"send\":$4}" approve "$PW"; }
  # nothing_asked: "yes" when the approver shows no request and tx_sender_module holds no send.
  nothing_asked() {
    local st live; st=$(j evm_signer_cli status); live=$(j tx_sender_module live_sends)
    jq -r -n --argjson st "$st" --argjson live "$live" 'if $st.pending_count == 0 and $st.rendered.handle == null and ($live.sends | length) == 0
      then "yes" else "no: \($st.pending_count) pending at the approver, \($live.sends | length) sends at tx_sender_module" end' 2>/dev/null \
      || echo "no: unreadable"
  }
  # refusal < REPLY: the vault error a reply names when the module refused it with eth_call before sending, else the reply.
  refusal() {
    local r n; r=$(cat); n=$(jq -r '.error // ""' <<< "$r" 2>/dev/null | sed -n 's/^refused: vault error \([A-Za-z]*\)() (checked with eth_call before sending.*/\1/p')
    echo "${n:-not refused with eth_call: $(head -c 300 <<< "$r")}"
  }
  # vault_refused NAME ACTION PARAMS SEND: vault_send's request straight to the module ($R/refused-NAME.json): the vault
  # error it names, then nothing_asked.
  vault_refused() {
    j ferrata_bridge_module admin "{\"action\":\"$2\",\"vault\":\"$V\",\"eth\":$E,\"params\":$3,\"send\":$4}" > "$R/refused-$1.json"
    echo "$(refusal < "$R/refused-$1.json") $(nothing_asked)"
  }
  # deposit_refused NAME: the user's ETH deposit as in step 4 ($R/deposit-NAME.json): the vault error named, then nothing_asked.
  deposit_refused() {
    j ferrata_bridge_module deposit "{$DEP,\"token\":\"$ETH\",\"denom_index\":0,\"send\":$SEND3}" > "$R/deposit-$1.json"
    echo "$(refusal < "$R/deposit-$1.json") $(nothing_asked)"
  }
  # since_block REPLY TIME: the reply's verdict and TIME (a time the vault stored) less its transaction's block time.
  since_block() {
    local b t=""; b=$(jq -r '.block // empty' "$1" 2>/dev/null); [ -n "$b" ] && t=$(block_time "$b")
    if [[ $2 =~ ^[0-9]+$ && $t =~ ^[0-9]+$ ]]; then echo "$(jq -r .verdict "$1") $(( $2 - t ))"; else echo "$(jq -r .verdict "$1") [$2] at block time [$t]"; fi
  }

  # 10a. The admin's global cap (direction "release" = Direction.Release, 1: that bucket may not go below the
  # deployment's release floor, 0 here) and delta, raised within its bounds (never down, at most MAX_DELTA, 30 days).
  vault_send global-cap set_global_cap '{"direction":"release","capacity":"5000000000000","refill_per_second":"2000000"}' "$SEND1"
  check "set_global_cap(release): settled, the global release bucket's capacity and refill (cast globalCap(1))" "settled 5000000000000 2000000" \
    "$(jq -r .verdict "$R/admin-global-cap.json") $(cast call $V 'globalCap(uint8)(uint256,uint256,uint256)' 1 --rpc-url $RPC | head -2 | vals)"
  check "set_global_cap(release): the global deposit bucket untouched, unlimited (cast globalCap(0))" "$UNLIMITED" \
    "$(cast call $V 'globalCap(uint8)(uint256,uint256,uint256)' 0 --rpc-url $RPC | head -1 | vals)"
  d0=$(cast call $V 'delta()(uint256)' --rpc-url $RPC | vals)
  vault_send raise-delta raise_delta '{"delta":"1200"}' "$SEND1"
  check "raise_delta(1200): settled, delta 600 -> 1200 (cast delta())" "600 settled 1200" \
    "$d0 $(jq -r .verdict "$R/admin-raise-delta.json") $(cast call $V 'delta()(uint256)' --rpc-url $RPC | vals)"
  check "raise_delta(900, below delta): refused before signing (DeltaCanOnlyRise), nothing at the approver" "DeltaCanOnlyRise yes" \
    "$(vault_refused delta-lower raise_delta '{"delta":"900"}' "$SEND1")"
  check "raise_delta(30 days + 1 s, above MAX_DELTA): refused before signing (DeltaTooLong), nothing at the approver" "DeltaTooLong yes" \
    "$(vault_refused delta-too-long raise_delta '{"delta":"2592001"}' "$SEND1")"

  # 10b. The admin role handed to the user (A3) and back, each hand-over accepted by the pending admin (acceptAdmin:
  # the pending admin only, never the freeze authority).
  vault_send transfer-admin transfer_admin "{\"new_admin\":\"$A3\"}" "$SEND1"
  check "transfer_admin(A3) by the admin A1: settled, A3 pending, A1 still the admin (cast)" "settled $A3 $A1" \
    "$(jq -r .verdict "$R/admin-transfer-admin.json") $(cast call $V 'pendingAdmin()(address)' --rpc-url $RPC | low) $(cast call $V 'admin()(address)' --rpc-url $RPC | low)"
  vault_send accept-admin accept_admin '{}' "$SEND3"
  check "accept_admin by the pending admin A3: settled, sent by A3, A3 the admin, none pending (cast)" "settled $A3 $A3 $Z" \
    "$(jq -r .verdict "$R/admin-accept-admin.json") $(sender "$R/admin-accept-admin.json") $(cast call $V 'admin()(address)' --rpc-url $RPC | low) $(cast call $V 'pendingAdmin()(address)' --rpc-url $RPC | low)"
  vault_send transfer-admin-back transfer_admin "{\"new_admin\":\"$A1\"}" "$SEND3"
  check "transfer_admin(A1) by the admin A3: settled, sent by A3, A1 pending (cast)" "settled $A3 $A1" \
    "$(jq -r .verdict "$R/admin-transfer-admin-back.json") $(sender "$R/admin-transfer-admin-back.json") $(cast call $V 'pendingAdmin()(address)' --rpc-url $RPC | low)"
  vault_send accept-admin-back accept_admin '{}' "$SEND1"
  check "accept_admin by the pending admin A1: settled, A1 the admin again, none pending (cast)" "settled $A1 $Z" \
    "$(jq -r .verdict "$R/admin-accept-admin-back.json") $(cast call $V 'admin()(address)' --rpc-url $RPC | low) $(cast call $V 'pendingAdmin()(address)' --rpc-url $RPC | low)"

  # 10c. The freeze authority's four other freezes, each set (until its block's time + maxFreeze) and lifted (0). The
  # checkpoint freeze, lifted, keeps the time it ended (its block's), from which keyless retirement counts.
  vault_send deposits-frozen set_deposits_frozen '{"frozen":true}' "$SEND2"
  check "set_deposits_frozen(true): settled, frozen for maxFreeze from its block's time (cast depositsFrozenUntil())" "settled $MAXF" \
    "$(since_block "$R/admin-deposits-frozen.json" "$(cast call $V 'depositsFrozenUntil()(uint256)' --rpc-url $RPC | vals)")"
  check "set_deposits_frozen: the module refuses an ETH deposit before signing (DepositsAreFrozen), nothing at the approver" \
    "DepositsAreFrozen yes" "$(deposit_refused all-frozen)"
  vault_send deposits-lifted set_deposits_frozen '{"frozen":false}' "$SEND2"
  check "set_deposits_frozen(false): settled, lifted (cast depositsFrozenUntil() 0)" "settled 0" \
    "$(jq -r .verdict "$R/admin-deposits-lifted.json") $(cast call $V 'depositsFrozenUntil()(uint256)' --rpc-url $RPC | vals)"
  vault_send releases-frozen set_releases_frozen '{"frozen":true}' "$SEND2"
  check "set_releases_frozen(true): settled, frozen for maxFreeze from its block's time (cast releasesFrozenUntil())" "settled $MAXF" \
    "$(since_block "$R/admin-releases-frozen.json" "$(cast call $V 'releasesFrozenUntil()(uint256)' --rpc-url $RPC | vals)")"
  vault_send releases-lifted set_releases_frozen '{"frozen":false}' "$SEND2"
  check "set_releases_frozen(false): settled, lifted (cast releasesFrozenUntil() 0)" "settled 0" \
    "$(jq -r .verdict "$R/admin-releases-lifted.json") $(cast call $V 'releasesFrozenUntil()(uint256)' --rpc-url $RPC | vals)"
  vault_send checkpoints-frozen set_checkpoints_frozen '{"frozen":true}' "$SEND2"
  check "set_checkpoints_frozen(true): settled, frozen for maxFreeze from its block's time (cast checkpointsFrozenUntil())" "settled $MAXF" \
    "$(since_block "$R/admin-checkpoints-frozen.json" "$(cast call $V 'checkpointsFrozenUntil()(uint256)' --rpc-url $RPC | vals)")"
  vault_send checkpoints-lifted set_checkpoints_frozen '{"frozen":false}' "$SEND2"
  check "set_checkpoints_frozen(false): settled, lifted at its block's time, not 0 (cast checkpointsFrozenUntil())" "settled 0" \
    "$(since_block "$R/admin-checkpoints-lifted.json" "$(cast call $V 'checkpointsFrozenUntil()(uint256)' --rpc-url $RPC | vals)")"
  vault_send token-releases-frozen set_token_releases_frozen "{\"token\":\"$T\",\"frozen\":true}" "$SEND2"
  check "set_token_releases_frozen(token, true): settled, frozen for maxFreeze from its block's time (cast tokenReleasesFrozenUntil)" "settled $MAXF" \
    "$(since_block "$R/admin-token-releases-frozen.json" "$(cast call $V 'tokenReleasesFrozenUntil(address)(uint256)' $T --rpc-url $RPC | vals)")"
  vault_send token-releases-lifted set_token_releases_frozen "{\"token\":\"$T\",\"frozen\":false}" "$SEND2"
  check "set_token_releases_frozen(token, false): settled, lifted (cast tokenReleasesFrozenUntil 0)" "settled 0" \
    "$(jq -r .verdict "$R/admin-token-releases-lifted.json") $(cast call $V 'tokenReleasesFrozenUntil(address)(uint256)' $T --rpc-url $RPC | vals)"

  # 10d. revoke_from: revokeFrom takes only a finalised checkpoint (index < checkpointCount(), not yet revoked, within
  # revokeWindow of its finalisation) and this fixture finalises none, so the freeze authority's request for index 0 is
  # refused before signing (with revokeFrom's own error: the call reached it).
  check "revoke_from(0) with no checkpoint (cast checkpointCount() 0): refused before signing (UnknownCheckpoint), nothing at the approver" \
    "0 UnknownCheckpoint yes" "$(cast call $V 'checkpointCount()(uint256)' --rpc-url $RPC | vals) $(vault_refused revoke-from revoke_from '{"index":0}' "$SEND2")"

  # 10e. The admin proposes the watchers (A3 and two unused entries; freeze authority address(0): unchanged), in force
  # ROLE_TIMELOCK (14 days) after the proposal; applying them sooner is refused. anvil's clock is then moved on 14 days
  # and a minute (one block mined), and anyone (A3) applies them, and the fee schedule step 3c announced (due seven days
  # after its announcement; no deposit of the token since has put it in force).
  vault_send propose-roles propose_roles "{\"watchers\":[\"$A3\",\"$Z\",\"$Z\"],\"freeze_authority\":\"$Z\"}" "$SEND1"
  check "propose_roles(watchers A3, 0, 0; freeze authority unchanged): settled, in force 14 days after its block's time (cast roleProposal())" \
    "settled 1209600 $Z" "$(since_block "$R/admin-propose-roles.json" "$(cast call $V 'roleProposal()(address,uint256)' --rpc-url $RPC | sed -n 2p | vals)") $(cast call $V 'roleProposal()(address,uint256)' --rpc-url $RPC | head -1 | low)"
  check "apply_roles before the time lock: refused before signing (TimelockNotOver), nothing at the approver" "TimelockNotOver yes" \
    "$(vault_refused apply-roles-early apply_roles '{}' "$SEND3")"
  cast rpc evm_increaseTime 1209660 --rpc-url $RPC > /dev/null; cast rpc evm_mine --rpc-url $RPC > /dev/null
  vault_send apply-roles apply_roles '{}' "$SEND3"
  check "apply_roles after the time lock, by anyone (A3): settled, sent by A3, watcher 0 A3, freeze authority still A2, none pending (cast)" \
    "settled $A3 $A3 $A2 0" "$(jq -r .verdict "$R/admin-apply-roles.json") $(sender "$R/admin-apply-roles.json") $(cast call $V 'watchers(uint256)(address)' 0 --rpc-url $RPC | low) $(cast call $V 'freezeAuthority()(address)' --rpc-url $RPC | low) $(cast call $V 'roleProposal()(address,uint256)' --rpc-url $RPC | sed -n 2p | vals)"
  fs0=$(cast call $V 'feeState(address)(uint64,uint64)' $T --rpc-url $RPC | head -1 | vals)
  vault_send apply-fees apply_fees "{\"token\":\"$T\"}" "$SEND3"
  check "apply_fees(token) after the seven days, by anyone (A3): settled, sent by A3, schedule 0 -> 1, none pending, mint fee 1000 (cast feeState, fees)" \
    "0 settled $A3 1 0 1000" "$fs0 $(jq -r .verdict "$R/admin-apply-fees.json") $(sender "$R/admin-apply-fees.json") $(cast call $V 'feeState(address)(uint64,uint64)' $T --rpc-url $RPC | vals) $(cast call $V 'fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)' $T 1 0 --rpc-url $RPC | head -1 | vals)"

  # 10f. retire, last (deposits stop for good): the freeze authority may retire at any time, anyone else only from
  # retireOpensAt() (here the checkpoint freeze's end above + delta + 30 days, still ahead after anvil's 14 days).
  check "retire by anyone (A3) before retireOpensAt(): refused before signing (RetireNotOpen), nothing at the approver" "RetireNotOpen yes" \
    "$(vault_refused retire-anyone retire '{}' "$SEND3")"
  vault_send retire retire '{}' "$SEND2"
  check "retire by the freeze authority A2: settled, sent by A2, retired (cast retired())" "settled $A2 true" \
    "$(jq -r .verdict "$R/admin-retire.json") $(sender "$R/admin-retire.json") $(cast call $V 'retired()(bool)' --rpc-url $RPC)"
  check "retire: the module refuses an ETH deposit before signing (VaultRetired), nothing at the approver" "VaultRetired yes" "$(deposit_refused retired)"

  # Every broadcast send's receipts read back by the module: all successful but the forced revert; and every call carried
  # a limit above its own estimate (or the caller's own).
  local bad=""
  for f in "$R"/admin-*.json "$R"/deposit-eth.json "$R"/deposit-erc20.json "$R"/freeze.json "$R"/unfreeze.json; do
    case "$f" in *.request.json|*admin-cap-reverts.json) continue ;; esac
    jq -e '(.send.receipts | length) > 0 and all(.send.receipts[]; .status == "success" and .gas_used < .gas_limit)' "$f" > /dev/null 2>&1 || bad="$bad $(basename "$f")"
  done
  check "every other send: every receipt successful, gas used below its limit" "" "${bad# }"
  for f in "$R"/*.ms; do printf '%s\t%s\n' "$(basename "$f" .ms)" "$(cat "$f")"; done > "$R/send-ms.tsv"
  j tx_sender_module live_sends > "$R/live-sends.json"
  kill $watch_pid 2>/dev/null
  ctl daemon stop > /dev/null 2>&1; kill $anvil_pid 2>/dev/null
  echo "checks: $pass passed, $fail failed" | tee "$OUT/summary.txt"
  # Requests the approver's screen dropped and the test showed again by handle (approve_next): reported, not a check.
  echo "approver recoveries: $(cat "$R/recoveries.jsonl" 2>/dev/null | wc -l)" | tee -a "$OUT/summary.txt"
  [ "$fail" -eq 0 ]
}

case "$MODE" in
  build) build ;;
  build-module) build_module ;;
  run) run ;;
  *) sed -n '2,26p' "$0"; exit 2 ;;
esac
