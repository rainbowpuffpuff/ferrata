#!/usr/bin/env bash
# e2e/seqnet's hook for a second deployer (docs/guides/deployer-guide.md §11): lez/crown/session.sh x runs it while the
# zone runs; drive.sh (mode deployer) has deployed the vault on anvil with vault/script/deploy.sh and this deployer's
# own settings (FERRATA_* variables), made one smoke-test deposit with the `ferrata` tool, and captured the vault.
#   a. the LEZ configuration: crown-driver crown-b --operator-only over capture-1 (the stand-in committee's finality
#      update for the deposit's block, Init with this deployer's roles and caps (CROWN_ADMIN and CROWN_FREEZE_AUTHORITY,
#      by default the accounts of two fresh random keys drive.sh made, or CROWN_ROLES_LABEL's derived keys;
#      CROWN_TOKEN_CAP, CROWN_GLOBAL_CAP), Register ETH from the vault's registry, ImportRoot)
#   b. on the vault, as its admin: register this deployer's own token with its own denominations and fees, set caps,
#      announce a fee schedule and let it come into force, hand the admin role to this deployer's own address
#   c. every setting read back from both chains and compared with what was chosen: checks.txt (PASS or FAIL per line)
set -uo pipefail
. "$SEQNET_ENV"
. "$REPO/e2e/seqnet/common.sh"

F=("$BIN/ferrata" --seed "$U/seed.txt" --eth-rpc "$RPC" --chain-id 11155111 --vault "$VAULT" --from-block "$FROM"
   --sequencer "$SEQ" --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE")
C="$RUN/checks.txt"; : > "$C"
fails=0
check() { # what expected got
  if [ "$(echo "$2" | tr 'A-F' 'a-f')" = "$(echo "$3" | tr 'A-F' 'a-f')" ]; then echo "PASS $1: $3" >> "$C"
  else echo "FAIL $1: expected $2, got $3" >> "$C"; fails=$((fails + 1)); fi
}
call() { cast call "$VAULT" "$@" --rpc-url "$RPC" | awk '{print $1}'; }
send() { local from="$1"; shift; cast send "$VAULT" "$@" --rpc-url "$RPC" --unlocked --from "$from" > "$W/send.log" 2>&1 \
  || { tail -n 5 "$W/send.log"; die "cast send $1"; }; say "vault: $1 (from $from)"; }

# ---- a. LEZ: the configuration, with this deployer's roles and caps ----------------------------------------------
"$BIN/crown-driver" config-id --identity "$SA" --vault "$VAULT" > "$RUN/lez/config.json"
if [ -n "${CROWN_ADMIN:-}" ]; then roles="the deployer's own accounts (CROWN_ADMIN, CROWN_FREEZE_AUTHORITY)"
else roles="keys derived from the label '${CROWN_ROLES_LABEL:-crown}': public, for a local zone only"; fi
say "LEZ configuration: configId $(jq -r .configId "$RUN/lez/config.json"); admin $(jq -r .controls.admin "$RUN/lez/config.json")," \
  "freeze authority $(jq -r .controls.freezeAuthority "$RUN/lez/config.json") ($roles)"
operator_import
u 06-status-zone "${F[@]}" status || die "status on the zone"

# ---- b. the vault, as its admin -------------------------------------------------------------------------------------
cd "$REPO/vault"
TOKEN2="$(forge create script/TestToken.sol:TestToken --rpc-url "$RPC" --unlocked --from "$DEPLOYER" --broadcast --json 2>/dev/null \
  | jq -r .deployedTo)"
[ -n "$TOKEN2" ] && [ "$TOKEN2" != null ] || die "the second token was not deployed"
cd - > /dev/null
echo "TOKEN2=$TOKEN2" >> "$SEQNET_ENV"
# This deployer's token: 6 decimals, two denominations (50 and 500), relay fees 0.5%, checkpoint fees 0.2%, deposit and
# release caps of 5,000 a day, the release delay chosen for the deployment.
D2="[50000000,500000000]"; R2="[250000,2500000]"; K2="[100000,1000000]"; CAP2=5000000000; REF2=$((5000000000 / 86400))
DEF2=0x00000000000000000000000000000000000000000000000000000000000000d2
send "$DEPLOYER" "registerToken(address,(uint256[],uint256[],uint256,uint256,(uint256,uint256),(uint256,uint256),uint256,bytes32,uint8,uint256[]))" \
  "$TOKEN2" "($D2,$R2,1,1,($CAP2,$REF2),($CAP2,$REF2),$FERRATA_RELEASE_DELAY,$DEF2,6,$K2)"
# Caps on ETH: 1 ETH a day each way, and a global deposit cap.
ETH=0x0000000000000000000000000000000000000000
send "$DEPLOYER" "setTokenCap(address,uint8,uint256,uint256)" $ETH 0 1000000000000000000 11574074074074
send "$DEPLOYER" "setTokenCap(address,uint8,uint256,uint256)" $ETH 1 1000000000000000000 11574074074074
send "$DEPLOYER" "setGlobalCap(uint8,uint256,uint256)" 0 10000000000 115740
# A fee schedule for ETH: mint and burn fees of 0.1%, checkpoint fees of 0.2%; in force seven days later.
send "$DEPLOYER" "announceFees(address,uint256[],uint256[],uint256[])" $ETH \
  "[1000000000000,10000000000000,100000000000000]" "[1000000000000,10000000000000,100000000000000]" \
  "[2000000000000,20000000000000,200000000000000]"
pass_time 604801 "the fee schedule's seven days"
cast send "$VAULT" "applyFees(address)" $ETH --rpc-url "$RPC" --unlocked --from "$DEPLOYER" > /dev/null || die "applyFees"
# The admin role handed to this deployer's own address (two steps).
cast rpc anvil_setBalance "$SD_ADMIN" 0x8ac7230489e80000 --rpc-url "$RPC" > /dev/null
send "$DEPLOYER" "transferAdmin(address)" "$SD_ADMIN"
send "$SD_ADMIN" "acceptAdmin()"

# ---- c. read everything back ----------------------------------------------------------------------------------------
u 07-status-vault "${F[@]}" status --token "$TOKEN2" || die "status of the vault"
check "vault admin (handed over)" "$SD_ADMIN" "$(call 'admin()(address)')"
check "vault freeze authority" "$FERRATA_FREEZE_AUTHORITY" "$(call 'freezeAuthority()(address)')"
check "vault fee recipient" "$FERRATA_FEE_RECIPIENT" "$(call 'feeRecipient()(address)')"
IFS=, read -r -a WS <<< "$FERRATA_WATCHERS"
for i in 0 1; do check "vault watcher $i" "${WS[$i]}" "$(call 'watchers(uint256)(address)' $i)"; done
check "vault finality depth: delta (the candidate's wait, s)" "$SEQNET_DELTA" "$(call 'delta()(uint256)')"
check "vault finality depth: deltaFloor (s)" "$SEQNET_DELTA" "$(call 'deltaFloor()(uint256)')"
check "vault revocation window (s)" "$FERRATA_REVOKE_WINDOW" "$(call 'revokeWindow()(uint256)')"
check "vault minimum release delay (s)" "$FERRATA_MIN_RELEASE_DELAY" "$(call 'minReleaseDelay()(uint256)')"
check "vault release delay of ETH (s)" "$FERRATA_RELEASE_DELAY" "$(call 'releaseDelayFor(address,uint256)(uint256)' $ETH 0)"
check "vault release delay of the second token (s)" "$FERRATA_RELEASE_DELAY" "$(call 'releaseDelayFor(address,uint256)(uint256)' "$TOKEN2" 0)"
check "vault mode (1 = rule B)" 1 "$(call 'mode()(uint8)')"
check "vault lezConfigId = the LEZ configuration's id" "$(jq -r .configId "$RUN/lez/config.json")" "$(call 'lezConfigId()(bytes32)')"
check "second token: denominations" "[50000000, 500000000]" \
  "$(cast call "$VAULT" 'tokenConfig(address)(bool,bool,uint256,uint256,uint256[])' "$TOKEN2" --rpc-url "$RPC" | tail -n 1 | sed -E 's/ \[[0-9.e+]+\]//g')"
for i in 0 1; do
  f="$(cast call "$VAULT" 'fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)' "$TOKEN2" 0 $i --rpc-url "$RPC" | awk '{print $1}' | paste -sd' ')"
  r="$(echo "$R2" | tr -d '[]' | cut -d, -f$((i + 1)))"; k="$(echo "$K2" | tr -d '[]' | cut -d, -f$((i + 1)))"
  check "second token denomination $i: mint, relay, burn, checkpoint fees" "0 $r 0 $k" "$f"
done
check "second token: deposit cap (capacity)" "$CAP2" "$(cast call "$VAULT" 'tokenCap(address,uint8)(uint256,uint256,uint256)' "$TOKEN2" 0 --rpc-url "$RPC" | head -n 1 | awk '{print $1}')"
check "ETH: release cap (capacity)" 1000000000000000000 "$(cast call "$VAULT" 'tokenCap(address,uint8)(uint256,uint256,uint256)' $ETH 1 --rpc-url "$RPC" | head -n 1 | awk '{print $1}')"
# fees(token, 1, i) holds schedule 1's values from its announcement on; feeState's `current` says it is in force.
check "ETH fee schedule in force (feeState.current)" 1 "$(cast call "$VAULT" 'feeState(address)(uint64,uint64)' $ETH --rpc-url "$RPC" | head -n 1 | awk '{print $1}')"
for i in 0 1 2; do
  f="$(cast call "$VAULT" 'fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)' $ETH 1 $i --rpc-url "$RPC" | awk '{print $1}' | paste -sd' ')"
  m=$((1000000000000 * 10 ** i)); r=$((2000000000000 * 10 ** i))
  check "ETH fee schedule 1, denomination $i: mint, relay, burn, checkpoint" "$m $r $m $r" "$f"
done
# The LEZ controls as config-id computed them: they reach the chain as configId, a hash over them that the vault pins
# (lezConfigId, read above) and under which the zone accepted Init (below); the global cap is also read from the zone.
cfg="$RUN/lez/config.json"
dflt="$(CROWN_ADMIN= CROWN_FREEZE_AUTHORITY= CROWN_ROLES_LABEL=crown CROWN_TOKEN_CAP= CROWN_GLOBAL_CAP= "$BIN/crown-driver" config-id --identity "$SA" --vault "$VAULT")"
if [ -n "${CROWN_ADMIN:-}" ]; then
  check "LEZ admin in configId = the deployer's own account" "$CROWN_ADMIN" "$(jq -r .controls.admin "$cfg")"
  check "LEZ freeze authority in configId = the deployer's own account" "$CROWN_FREEZE_AUTHORITY" "$(jq -r .controls.freezeAuthority "$cfg")"
else
  [ "$(jq -r .controls.admin "$cfg")" != "$(echo "$dflt" | jq -r .controls.admin)" ] && check "LEZ admin differs from the default label's" yes yes \
    || check "LEZ admin differs from the default label's" yes no
  [ "$(jq -r .controls.freezeAuthority "$cfg")" != "$(echo "$dflt" | jq -r .controls.freezeAuthority)" ] && check "LEZ freeze authority differs from the default label's" yes yes \
    || check "LEZ freeze authority differs from the default label's" yes no
fi
[ "$(jq -r .configId "$cfg")" != "$(echo "$dflt" | jq -r .configId)" ] && check "the roles and caps enter configId (another configId than the default's)" yes yes \
  || check "the roles and caps enter configId (another configId than the default's)" yes no
check "LEZ token cap in configId (value units)" "${CROWN_TOKEN_CAP:-unlimited}" "$(jq -r .controls.tokenCap.capacity "$cfg")"
check "LEZ global cap in configId (value units)" "${CROWN_GLOBAL_CAP:-unlimited}" "$(jq -r .controls.globalCap.capacity "$cfg")"
if [ -n "${CROWN_GLOBAL_CAP:-}" ]; then
  gate="$(grep -m1 '^  gate Gate' "$RUN/user/06-status-zone.txt" || true)"
  check "LEZ global cap read from the zone (the gate's mint and burn buckets)" "$CROWN_GLOBAL_CAP $CROWN_GLOBAL_CAP" \
    "$(echo "$gate" | grep -o -E '(mint|burn): Bucket \{ capacity: [0-9]+' | awk '{print $NF}' | paste -sd' ')"
fi
grep -q 'configuration ready' "$RUN/user/06-status-zone.txt" && check "LEZ configuration initialised under this configId" yes yes \
  || check "LEZ configuration initialised under this configId" yes no
records
say "checks: $(grep -c '^PASS' "$C") passed, $fails failed (checks.txt)"
say "hook-deployer: done"
[ "$fails" = 0 ]
