#!/usr/bin/env bash
# FerrataVault's admin, freeze-authority and watcher actions, from a terminal (docs/cli-coverage.md, gap G4).
#
# One subcommand per action of vault/src/FerrataVault.sol that is gated on a role (plus the permissionless
# applyFees/applyRoles that finish an admin's change), and a read-only `status`. Built on Foundry's cast only
# (cast call, cast send, cast calldata); jq is used for nothing but its date formatting.
#
# Dry run by default: the target, the function, the decoded arguments, the calldata and the result of an
# eth_call of that calldata from the role's address are printed, and nothing is sent. A call that would
# revert says so, with the vault's error name decoded from the selector. With --send, and only after the
# eth_call succeeded, the transaction is sent with cast send.
#
# Keys are never a raw private key on the command line: they come through cast's own options, passed
# through verbatim (--keystore PATH [--password PW | --password-file F], --account NAME, --ledger), or, on
# a local anvil, --unlocked --from ADDRESS with one of anvil's published accounts.
#
# Amounts (caps, fees) accept wei (a plain integer), a bare decimal (in the token's decimals, read from
# the token; ETH is 18) or a decimal with an explicit unit: 1.5ether, 2gwei. Deltas, delays and indices
# are seconds or plain integers.
#
# Exit codes: 0 ok; 1 the call reverts or a read fails; 2 usage (a malformed argument is refused with
# exit 2 before any network call).
#
# Usage:
#   admin.sh [OPTIONS] ACTION [ARGS...]
#
# Options:
#   --rpc-url URL          (required) the Ethereum JSON-RPC endpoint; no default network
#   --vault ADDRESS        (required) the deployed FerrataVault
#   --from ADDRESS         the sender for the dry run (with --unlocked, also for sending)
#   --keystore PATH        cast's keystore option, passed through to cast send / cast wallet address
#   --account NAME         a keystore account in ~/.foundry/keystores (cast's option, passed through)
#   --password PW          the keystore password (cast's option, passed through)
#   --password-file PATH   the keystore password file (cast's option, passed through)
#   --ledger               cast's Ledger option, passed through
#   --unlocked             send with eth_sendTransaction (a local anvil; needs --from)
#   --send                 send the transaction (the default is the dry run)
#   -h, --help             this list
#
# Actions (the role that may call them in parentheses):
#   status [TOKEN...]                            (read-only) roles, caps, freezes, fee state, delays,
#                                                retirement, and one section per TOKEN named
#   set-token-cap TOKEN DEPOSIT|RELEASE CAPACITY REFILL   (admin) a per-token bucket; AMOUNTs in the
#                                                token's decimals
#   set-global-cap DEPOSIT|RELEASE CAPACITY REFILL       (admin) a global bucket, in value units
#                                                (shown here as ETH, 18 decimals)
#   set-release-delay TOKEN SECONDS              (admin) the delay for roots finalised later
#   raise-delta SECONDS                          (admin) the finality wait; only up
#   set-deposits-enabled TOKEN true|false        (admin) false delists the token (deposits stop)
#   announce-fees TOKEN MINT,BURN,CHECKPOINT     (admin) three comma-separated fee lists, one entry per
#                                                denomination, each an AMOUNT in the token's decimals
#   apply-fees TOKEN                             (anyone) puts an announced schedule in force once its
#                                                seven-day notice has passed
#   propose-roles W0 W1 W2 FREEZE_AUTHORITY      (admin) the timelocked watcher set and freeze authority
#   apply-roles                                  (anyone) applies the proposal once the timelock is over
#   transfer-admin NEW_ADMIN                     (admin) starts the two-step admin handover
#   accept-admin                                 (the pending admin) completes the handover
#   set-deposits-frozen true|false               (freeze authority) all deposits
#   set-releases-frozen true|false               (freeze authority) all releases
#   set-checkpoints-frozen true|false            (freeze authority) candidates and finalisation
#   set-token-deposits-frozen TOKEN true|false   (freeze authority) one token's deposits
#   set-token-releases-frozen TOKEN true|false   (freeze authority) one token's releases
#   revoke-from CHECKPOINT_INDEX                 (freeze authority) reject-only, within the window
#   retire                                       (freeze authority; anyone once retireOpensAt has passed)
#   cancel-candidate NEW_STATE                   (a watcher or the freeze authority)
#   pause-checkpoints                            (a watcher) a 12-hour pause
#
# Examples (never a raw private key on the command line):
#   admin.sh --rpc-url http://127.0.0.1:8545 --vault 0x... --from 0x... status
#   admin.sh --rpc-url ... --vault ... --account ops --send set-global-cap deposit 1000ether 1ether
#   admin.sh --rpc-url ... --vault ... --keystore ops.json --password-file pw set-token-cap \
#       0xTOKEN deposit 500000 10
#   admin.sh --rpc-url http://127.0.0.1:8545 --vault 0x... --unlocked --from 0xf39F... \
#       --send set-deposits-frozen true
#
# registerToken is deliberately not here: its TokenParams (three fee arrays, the nested cap parameters,
# the LEZ definition) are a deployment-time parameter set that vault/script/DeployVault.s.sol and the
# e2e scripts build, and it is write-once, so it is not an admin control of a deployed vault.

set -euo pipefail

PROG=$(basename "$0")

usage_error() {
  echo "$PROG: $*" >&2
  echo "$PROG: run '$PROG --help' for the action list" >&2
  exit 2
}

die() {
  echo "$PROG: $*" >&2
  exit 1
}

# The vault's custom errors (vault/src/FerrataVault.sol and its linked FerrataRegistry; none carries
# parameters), each with the selector cast sig 'Name()' gives; VerificationFailed is RISC Zero's
# verifier's, raised through the vault. vault/script/test-admin.sh re-checks every selector against
# cast sig, and admin/dump.mjs's self-test holds the same table for the view.
declare -A ERROR_NAMES=(
  [0xa85e6f1a]=AlreadyRefunded [0x0ac03ea0]=AlreadyRetired [0xa3d09665]=BeaconTooEarly
  [0x02d2e0be]=BurnRootNotAccepted [0x6e076395]=CandidateAlreadySubmitted [0xf9573524]=CandidateMismatch
  [0xc359ad04]=CapBelowFloor [0x47e716d5]=CheckpointAlreadyRevoked [0x79392e50]=CheckpointsAreFrozen
  [0x5da8a302]=DeltaCanOnlyRise [0x955d76fc]=DeltaTooLong [0x3ab7db88]=DenominationNotMultipleOfLezUnit
  [0x36d5cac2]=DepositsAreFrozen [0x717a1648]=DepositsDisabled [0x867783b7]=DirectEthNotAccepted
  [0xc3fe5506]=EchoNonceMismatch [0x986e96bb]=EchoNonceNotZero [0x6d963f88]=EthTransferFailed
  [0xe06d2eb5]=FeeCountMismatch [0x476084ae]=FeeNotMultipleOfLezUnit [0xcd4e6167]=FeeTooHigh
  [0x403e7fa6]=FunctionNotFound [0x89fd9b50]=GlobalDepositCapExceeded [0x5f21a1ef]=GlobalReleaseCapExceeded
  [0x70e8473d]=ImportedLeafCountDecreased [0x56060da8]=ImportedLeafCountTooHigh
  [0x90b8ab88]=InsufficientEscrow [0xcf9d57bb]=InvalidCommitteeFloor [0x0b54ac26]=InvalidDelta
  [0x84daa5ff]=InvalidDenominationIndex [0x3b6a5c8d]=InvalidDenominations
  [0x32acd2cb]=InvalidGenesisState [0xbb326ac0]=InvalidImageIds [0x9e9ce8a0]=InvalidLezConfigId
  [0x93ce8475]=InvalidLezUnit [0x73a2141e]=InvalidMaxFreeze [0x66b08299]=InvalidMerklePath
  [0x368a3d9e]=InvalidMinReleaseDelay [0x2cd454b1]=InvalidRelayFee [0x67781202]=InvalidRevokeWindow
  [0xeaa956d7]=InvalidTurnSeconds [0x3920d5ba]=InvalidUnitWeight [0x741eaf46]=KeySetBelowFloor
  [0x0dc5fde9]=NoProposal [0x7bfa4b9f]=NotAdmin [0x6701dea0]=NotFreezeAuthority [0x797dba54]=NothingOwed
  [0x058d9a1b]=NotPendingAdmin [0x3742d1f6]=NotRefundable [0xc465dd19]=NotRetired [0x8d23bcde]=NotWatcher
  [0xcad2ae02]=NullifierAlreadyUsed [0x78e897c4]=ParentNotFinal [0x96b70a71]=RefundNotYetOpen
  [0xee1de162]=RelayFeeCountMismatch [0x4d268ba9]=RelayFeeTooHigh [0xedd7d80f]=ReleaseDelayNotElapsed
  [0xfea64e07]=ReleaseDelayOutOfRange [0x6a62cb6b]=ReleasesAreFrozen [0xe69fe27c]=RetireNotOpen
  [0xd8088b63]=RevokeWindowPassed [0x39a17874]=RolesMustDiffer [0x3b03848f]=StaleCandidate
  [0xef573563]=SunsetIsFinal [0xcc2bb57f]=TimelockNotOver [0x7d4fffb2]=TokenAlreadyRegistered
  [0x0140df75]=TokenDepositCapExceeded [0xb6c684dd]=TokenDepositsAreFrozen [0x72a73200]=TokenHasNoCode
  [0x259ba1ad]=TokenNotRegistered [0xf1fb62e6]=TokenReleaseCapExceeded [0xdad32ed3]=TokenReleasesAreFrozen
  [0xb0a6ea29]=TransferAmountMismatch [0xb48f2cf8]=TreeFull [0xecc81a8a]=UnknownBeacon
  [0x3c6ed8ee]=UnknownCandidate [0x8f21ef34]=UnknownCheckpoint [0x732ee0ce]=UnknownLeaf
  [0x491fe534]=UseDepositETH [0x95961660]=VaultRetired [0xc5a82eda]=VerifierHasNoCode
  [0x05941599]=VerifierNotStop [0xc8e9f884]=VerifierStopped [0x2628a860]=WaitNotOver
  [0xf40c8d67]=WrongCommitteeFloor [0x93526816]=WrongDepositIndex [0x3acace01]=WrongEthAmount
  [0x2a580d6d]=WrongMode [0xd92e233d]=ZeroAddress [0x51bda03d]=ZeroCommitment
  [0xd27b4443]=ZeroRecipient [0x439cc0cd]=VerificationFailed
)

# ------------------------------------------------------------------------------------------------------------
# Argument validation. Everything here runs before any network call, so a malformed argument is exit 2
# without a single RPC request.
# ------------------------------------------------------------------------------------------------------------

is_uint() { [[ $1 =~ ^[0-9]+$ ]]; }

# norm_address ADDRESS: `eth` (case-insensitively) is the vault's ETH (address(0)); anything else must be
# a 20-byte hex value, and is echoed checksummed (cast refuses some non-checksummed forms).
norm_address() {
  local a=$1
  if [[ $a == eth || $a == ETH ]]; then echo "0x0000000000000000000000000000000000000000"; return 0; fi
  [[ $a =~ ^0[xX][0-9a-fA-F]{40}$ ]] || usage_error "not an address: '$a' (ETH is 'eth')"
  cast to-check-sum-address "$a" 2>/dev/null || usage_error "not an address: '$a'"
}

is_bytes32() { [[ $1 =~ ^0[xX][0-9a-fA-F]{64}$ ]]; }

# norm_bool WORD: echoes true/false.
norm_bool() {
  case $1 in
    true|TRUE|1) echo true ;;
    false|FALSE|0) echo false ;;
    *) usage_error "not a boolean: '$1' (true or false)" ;;
  esac
}

# norm_direction WORD: echoes 0 (deposit) or 1 (release).
norm_direction() {
  case $1 in
    deposit|DEPOSIT|Deposit) echo 0 ;;
    release|RELEASE|Release) echo 1 ;;
    *) usage_error "not a direction: '$1' (deposit or release)" ;;
  esac
}

# parse_amount DECIMALS SPEC: the amount in base units. Wei (a plain integer), a bare decimal (in
# DECIMALS) or a decimal with an explicit unit (ether/eth = 18, gwei = 9).
parse_amount() {
  local decimals=$1 spec=$2 num unit whole frac pad zeros out extra
  if is_uint "$spec"; then echo "$spec"; return 0; fi
  if [[ $spec =~ ^([0-9]+(\.[0-9]*)?|\.[0-9]+)(ether|eth|gwei)?$ ]]; then
    num=${BASH_REMATCH[1]}
    unit=${BASH_REMATCH[3]}
  else
    usage_error "malformed amount '$spec' (wei, a decimal, or a decimal with ether/gwei)"
  fi
  case $unit in
    ether|eth) decimals=18 ;;
    gwei) decimals=9 ;;
  esac
  if [[ $num == *.* ]]; then whole=${num%%.*}; frac=${num#*.}; else whole=$num; frac=""; fi
  [[ -n $whole ]] || whole=0
  if (( ${#frac} > decimals )); then
    extra=${frac:decimals}
    [[ $extra =~ ^0+$ ]] || usage_error "malformed amount '$spec': more than $decimals decimals"
    frac=${frac:0:decimals}
  fi
  pad=$((decimals - ${#frac}))
  zeros=$(printf '%*s' "$pad" "")
  zeros=${zeros// /0}
  out="$whole$frac$zeros"
  out=${out#"${out%%[!0]*}"} # strip leading zeros
  [[ -n $out ]] || out=0
  echo "$out"
}

# fmt_wei WEI DECIMALS: the same amount as a decimal string (for the status view only).
fmt_wei() {
  local w=$1 d=$2 int frac
  if (( d == 0 )); then echo "$w"; return 0; fi
  while (( ${#w} <= d )); do w="0$w"; done
  int=${w:0:$((${#w} - d))}
  frac=${w: -d}
  while [[ -n $frac && $frac == *0 ]]; do frac=${frac%0}; done
  if [[ -n $frac ]]; then echo "$int.$frac"; else echo "$int"; fi
}

# fmt_time EPOCH: the epoch as UTC ISO 8601 (jq's todate; cast has no date formatting).
fmt_time() { jq -rn --argjson t "$1" '$t | todate' 2>/dev/null || echo "$1"; }

# parse_fee_list DECIMALS LIST: a comma-separated list of amounts (announce-fees), as one entry per
# denomination, echoed back comma-separated in base units. An empty list is passed on empty.
parse_fee_list() {
  local decimals=$1 list=$2 out="" item value
  [[ -n $list ]] || { echo ""; return 0; }
  local IFS=","
  for item in $list; do
    [[ $item != *[[:space:]]* ]] || usage_error "malformed fee list '$list' (comma-separated amounts, no spaces)"
    # The status is checked here: bash does not apply set -e to an assignment whose command
    # substitution failed inside a function that itself runs in one.
    value=$(parse_amount "$decimals" "$item") || exit 2
    out+="${out:+,}$value"
  done
  echo "$out"
}

# token_decimals TOKEN: the token's decimals, read from the token (ETH is 18 by definition).
token_decimals() {
  local t=$1 d
  if [[ $t == 0x0000000000000000000000000000000000000000 ]]; then echo 18; return 0; fi
  if ! d=$(cast call "$t" "decimals()" --rpc-url "$RPC" 2>&1); then
    die "could not read decimals() of $t: $d"
  fi
  [[ -n $d && $d != 0x ]] || die "no decimals() answer from $t (not an ERC-20?)"
  d=$(cast to-dec "$d") || die "could not decode the decimals of $t"
  echo "$d"
}

# ------------------------------------------------------------------------------------------------------------
# Options. Whatever wallet options are given are passed through to cast verbatim; --from is used for the
# dry run's eth_call (and, with --unlocked, for sending). A raw private key is refused outright.
# ------------------------------------------------------------------------------------------------------------

RPC=""
VAULT=""
FROM=""
SEND=0
UNLOCKED=0
WALLET_OPTS=()
ACTION=""
ARGS=()

command -v cast > /dev/null 2>&1 || die "cast (Foundry) is not on PATH"
command -v jq > /dev/null 2>&1 || die "jq is not on PATH (the status view formats times with it)"

while (($#)); do
  case $1 in
    --rpc-url) (($# >= 2)) || usage_error "--rpc-url needs a URL"; RPC=$2; shift 2 ;;
    --vault) (($# >= 2)) || usage_error "--vault needs an address"; VAULT=$2; shift 2 ;;
    --from) (($# >= 2)) || usage_error "--from needs an address"; FROM=$2; shift 2 ;;
    --keystore|--account|--password|--password-file)
      (($# >= 2)) || usage_error "$1 needs a value"; WALLET_OPTS+=("$1" "$2"); shift 2 ;;
    --ledger|--trezor) WALLET_OPTS+=("$1"); shift ;;
    --unlocked) UNLOCKED=1; shift ;;
    --send) SEND=1; shift ;;
    --private-key) usage_error "a raw private key on the command line is not accepted; use --keystore, --account, --ledger or --unlocked" ;;
    -h|--help) awk 'NR == 1 {next} /^set -euo pipefail/ {exit} {sub(/^# ?/, ""); print}' "$0"; exit 0 ;;
    --) shift; while (($#)); do if [[ -z $ACTION ]]; then ACTION=$1; else ARGS+=("$1"); fi; shift; done ;;
    -*) usage_error "unknown option '$1'" ;;
    *) if [[ -z $ACTION ]]; then ACTION=$1; else ARGS+=("$1"); fi; shift ;;
  esac
done

[[ -n $RPC ]] || usage_error "--rpc-url is required (there is no default network)"
[[ -n $VAULT ]] || usage_error "--vault is required (the deployed FerrataVault's address)"
VAULT=$(norm_address "$VAULT") || exit 2
[[ -n $ACTION ]] || usage_error "no action given"

if [[ -z $FROM ]] && ((${#WALLET_OPTS[@]} == 0)); then
  usage_error "no sender for the dry run: pass --from ADDRESS, or --keystore/--account/--ledger"
fi
if ((UNLOCKED)) && [[ -z $FROM ]]; then
  usage_error "--unlocked needs --from (the account anvil has unlocked)"
fi

# The dry run calls from the role's address: --from as given, or derived from the wallet options.
if [[ -z $FROM ]]; then
  FROM=$(cast wallet address ${WALLET_OPTS[@]+"${WALLET_OPTS[@]}"} 2>&1) || die "could not derive the sender address: $FROM"
fi
FROM=$(norm_address "$FROM")

# ------------------------------------------------------------------------------------------------------------
# The dry run and the send, shared by every action.
# ------------------------------------------------------------------------------------------------------------

print_revert() {
  local text=$1 sel name
  sel=$(printf '%s\n' "$text" | grep -oE '0x[0-9a-fA-F]{8}' | head -n 1 || true)
  if [[ -n $sel ]]; then
    name=${ERROR_NAMES[$sel]:-}
    if [[ -n $name ]]; then
      echo "            the vault refused it: $name() (error selector $sel)"
      return 0
    fi
    echo "            reverted with an unknown error, selector $sel"
    return 0
  fi
  printf '            %s\n' "$text"
}

# run_action ROLE SIGNATURE "decoded arguments" CAST-ARGS...
run_action() {
  local role=$1 sig=$2 desc=$3
  shift 3
  local calldata out
  calldata=$(cast calldata "$sig" ${@+"$@"} 2>&1) || die "cast calldata failed for $sig: $calldata"
  echo "vault:      $VAULT"
  echo "endpoint:   $RPC"
  echo "action:     $ACTION"
  echo "role:       $role"
  echo "function:   $sig"
  echo "arguments:  $desc"
  echo "sender:     $FROM"
  echo "calldata:   $calldata"
  if ! out=$(cast call "$VAULT" "$sig" ${@+"$@"} --from "$FROM" --rpc-url "$RPC" 2>&1); then
    echo "dry run:    the call would revert from $FROM"
    print_revert "$out"
    exit 1
  fi
  if [[ -n $out && $out != 0x ]]; then
    echo "dry run:    OK, it would return $out"
  else
    echo "dry run:    OK (no return value)"
  fi
  if ((!SEND)); then
    echo "not sent:   dry run only (pass --send to send this transaction)"
    exit 0
  fi
  echo "sending:    from $FROM"
  local -a send=(cast send "$VAULT" "$sig" ${@+"$@"} --rpc-url "$RPC")
  if ((UNLOCKED)); then
    send+=(--unlocked --from "$FROM")
  else
    send+=(${WALLET_OPTS[@]+"${WALLET_OPTS[@]}"})
  fi
  if ! out=$("${send[@]}" 2>&1); then
    echo "$out"
    die "the transaction could not be sent (the dry run had succeeded; the chain moved?)"
  fi
  printf '%s\n' "$out"
  exit 0
}

# ------------------------------------------------------------------------------------------------------------
# The read-only status view.
# ------------------------------------------------------------------------------------------------------------

# readget "sig(in)(out)" ARGS...: the vault getter's answer, decoded by cast itself (cast call takes the
# output signature in the same string), one value per line, with the [1.2e3] annotations cast adds.
readget() {
  local sig=$1 out
  shift
  if ! out=$(cast call "$VAULT" "$sig" ${@+"$@"} --rpc-url "$RPC" 2>&1); then
    die "the read $sig failed: $out"
  fi
  if [[ -z $out || $out == 0x ]]; then echo ""; return 0; fi
  printf '%s
' "$out"
}

# field INDEX DECODED: the INDEXth value (0-based) of readget's answer, without cast's [1.2e3] annotations.
field() {
  local i=$1 text=$2 v
  local -a lines
  mapfile -t lines <<< "$text"
  [[ ${lines[$i]+x} ]] || { echo ""; return 0; }
  clean "${lines[$i]}"
}

# rest INDEX DECODED: every value from INDEX on, comma-separated (cast prints an array one per line).
rest() {
  local i=$1 text=$2 out=""
  local -a lines
  mapfile -t lines <<< "$text"
  for ((; i < ${#lines[@]}; i++)); do
    out+="${out:+, }$(clean "${lines[$i]}")"
  done
  printf '%s\n' "$out"
}

# clean LINE: cast decode-abi's [1.2e3] annotations removed (the decimals stay).
clean() { printf '%s\n' "$1" | sed 's/ \[[^]]*\]//g'; }

# fmt_cap WEI DECIMALS: "unlimited" for the bucket's UNLIMITED sentinel (2^256 - 1), or the amount in wei
# with its decimal form.
fmt_cap() {
  if [[ $1 == 115792089237316195423570985008687907853269984665640564039457584007913129639935 ]]; then
    echo "unlimited"
  else
    echo "$1 ($(fmt_wei "$1" "$2") at $2 decimals)"
  fi
}

do_status() {
  local tokens=("$@") block now chain line d t td registered enabled cap dep rel chk authority effective pending
  # The token arguments are validated before any network call, like every other action's arguments.
  local i
  for i in "${!tokens[@]}"; do
    tokens[$i]=$(norm_address "${tokens[$i]}") || exit 2
  done
  chain=$(cast chain-id --rpc-url "$RPC") || die "could not read the chain id"
  block=$(cast block-number --rpc-url "$RPC") || die "could not read the block number"
  now=$(cast block latest --field timestamp --rpc-url "$RPC") || die "could not read the block time"
  echo "vault:        $VAULT"
  echo "endpoint:     $RPC"
  echo "chain:        $chain   block $block   block time $now ($(fmt_time "$now"))"
  echo "mode:         $(field 0 "$(readget "mode()(uint8)")") (0 = A, 1 = B)"
  echo "retired:      $(field 0 "$(readget "retired()(bool)")")"
  line=$(field 0 "$(readget "retireOpensAt()(uint256)")")
  echo "retireOpensAt: $line ($(fmt_time "$line"))"
  echo "delta:        $(field 0 "$(readget "delta()(uint256)")") s"
  echo "revocationEpoch: $(field 0 "$(readget "revocationEpoch()(uint256)")")"
  echo "checkpoints:  $(field 0 "$(readget "checkpointCount()(uint256)")") finalised, $(field 0 "$(readget "beaconCount()(uint256)")") beacons, deposits' next leaf $(field 0 "$(readget "nextLeafIndex()(uint64)")")"
  echo "roles:"
  echo "  admin:            $(field 0 "$(readget "admin()(address)")")"
  echo "  pendingAdmin:     $(field 0 "$(readget "pendingAdmin()(address)")")"
  echo "  freezeAuthority:  $(field 0 "$(readget "freezeAuthority()(address)")")"
  echo "  watchers:         $(field 0 "$(readget "watchers(uint256)(address)" 0)") $(field 0 "$(readget "watchers(uint256)(address)" 1)") $(field 0 "$(readget "watchers(uint256)(address)" 2)")"
  echo "  feeRecipient:     $(field 0 "$(readget "feeRecipient()(address)")")"
  # roleProposal() returns the proposed freeze authority and the time it takes effect (the proposed
  # watchers are only in the RolesProposed event, which a read-only call cannot see).
  line=$(readget "roleProposal()(address,uint256)")
  authority=$(field 0 "$line")
  effective=$(field 1 "$line")
  if [[ $effective == 0 ]]; then
    echo "  roleProposal:     none"
  else
    echo "  roleProposal:     freeze authority $authority, effective at $effective ($(fmt_time "$effective"))"
  fi
  echo "global caps (value units; available is what could be consumed now):"
  for d in 0 1; do
    line=$(readget "globalCap(uint8)(uint256,uint256,uint256)" "$d")
    echo "  $([[ $d == 0 ]] && echo deposit || echo release): capacity $(fmt_cap "$(field 0 "$line")" 18), refill $(field 1 "$line")/s, available $(fmt_cap "$(field 2 "$line")" 18)"
  done
  echo "freezes (the time each lapses; 0 = not frozen):"
  dep=$(field 0 "$(readget "depositsFrozenUntil()(uint256)")")
  rel=$(field 0 "$(readget "releasesFrozenUntil()(uint256)")")
  chk=$(field 0 "$(readget "checkpointsFrozenUntil()(uint256)")")
  echo "  deposits:    $dep ($(fmt_time "$dep"))$([[ $dep -gt $now ]] && echo '  [IN FORCE]' || echo '  [not in force]')"
  echo "  releases:   $rel ($(fmt_time "$rel"))$([[ $rel -gt $now ]] && echo '  [IN FORCE]' || echo '  [not in force]')"
  echo "  checkpoints: $chk ($(fmt_time "$chk"))$([[ $chk -gt $now ]] && echo '  [IN FORCE]' || echo '  [not in force]')"
  if ((${#tokens[@]})); then
    for t in "${tokens[@]}"; do
      t=$(norm_address "$t") || exit 2
      echo "token $t:"
      line=$(readget "tokenConfig(address)(bool,bool,uint256,uint256,uint256[])" "$t")
      registered=$(field 0 "$line")
      if [[ $registered != true ]]; then
        echo "  not registered"
        continue
      fi
      enabled=$(field 1 "$line")
      echo "  depositsEnabled: $enabled   lezUnit $(field 2 "$line")   unitWeight $(field 3 "$line")   denominations $(rest 4 "$line")"
      td=$(token_decimals "$t")
      for d in 0 1; do
        line=$(readget "tokenCap(address,uint8)(uint256,uint256,uint256)" "$t" "$d")
        echo "  $([[ $d == 0 ]] && echo deposit || echo release) cap: capacity $(fmt_cap "$(field 0 "$line")" "$td"), refill $(field 1 "$line")/s, available $(fmt_cap "$(field 2 "$line")" "$td")"
      done
      line=$(readget "feeState(address)(uint64,uint64)" "$t")
      pending=$(field 1 "$line")
      echo "  fee schedule in force: $(field 0 "$line")$([[ $pending != 0 ]] && echo ", one announced (in force from $(fmt_time "$pending"))")"
      echo "  deposits frozen until: $(field 0 "$(readget "tokenDepositsFrozenUntil(address)(uint256)" "$t")")"
      echo "  releases frozen until: $(field 0 "$(readget "tokenReleasesFrozenUntil(address)(uint256)" "$t")")"
      echo "  escrowed: $(field 0 "$(readget "escrowed(address)(uint256)" "$t")")"
      local checkpointcount
      checkpointcount=$(field 0 "$(readget "checkpointCount()(uint256)")")
      echo "  release delay for roots finalised now: $(field 0 "$(readget "releaseDelayFor(address,uint256)(uint256)" "$t" "$checkpointcount")") s"
    done
  else
    echo "per-token state: name tokens as arguments to show them (the vault cannot enumerate them)"
  fi
  echo "read at block $block; every read is read-only (eth_call)"
}

# ------------------------------------------------------------------------------------------------------------
# The actions.
# ------------------------------------------------------------------------------------------------------------

set -- ${ARGS[@]+"${ARGS[@]}"}

case $ACTION in
  status) do_status "$@" ;;

  set-token-cap)
    (($# == 4)) || usage_error "set-token-cap TOKEN DEPOSIT|RELEASE CAPACITY REFILL"
    token=$(norm_address "$1") || exit 2
    dir=$(norm_direction "$2")
    d=$(token_decimals "$token") || exit 1
    capacity=$(parse_amount "$d" "$3") || exit 2
    refill=$(parse_amount "$d" "$4") || exit 2
    run_action admin "setTokenCap(address,uint8,uint256,uint256)" \
      "token $token, direction $2, capacity $capacity ($(fmt_wei "$capacity" "$d") at $d decimals), refill per second $refill ($(fmt_wei "$refill" "$d") at $d decimals)" \
      "$token" "$dir" "$capacity" "$refill"
    ;;

  set-global-cap)
    (($# == 3)) || usage_error "set-global-cap DEPOSIT|RELEASE CAPACITY REFILL"
    dir=$(norm_direction "$1")
    capacity=$(parse_amount 18 "$2") || exit 2
    refill=$(parse_amount 18 "$3") || exit 2
    run_action admin "setGlobalCap(uint8,uint256,uint256)" \
      "direction $1, capacity $capacity value units ($(fmt_wei "$capacity" 18) shown as ETH), refill per second $refill ($(fmt_wei "$refill" 18))" \
      "$dir" "$capacity" "$refill"
    ;;

  set-release-delay)
    (($# == 2)) || usage_error "set-release-delay TOKEN SECONDS"
    token=$(norm_address "$1") || exit 2
    is_uint "$2" || usage_error "the delay is a number of seconds: '$2'"
    run_action admin "setReleaseDelay(address,uint256)" \
      "token $token, delay $2 s (for roots finalised after this call)" "$token" "$2"
    ;;

  raise-delta)
    (($# == 1)) || usage_error "raise-delta SECONDS"
    is_uint "$1" || usage_error "the delta is a number of seconds: '$1'"
    run_action admin "raiseDelta(uint256)" "new delta $1 s (it can only rise)" "$1"
    ;;

  set-deposits-enabled)
    (($# == 2)) || usage_error "set-deposits-enabled TOKEN true|false"
    token=$(norm_address "$1") || exit 2
    flag=$(norm_bool "$2") || exit 2
    run_action admin "setDepositsEnabled(address,bool)" "token $token, deposits enabled: $flag" "$token" "$flag"
    ;;

  announce-fees)
    (($# == 4)) || usage_error "announce-fees TOKEN MINTFEES BURNFEES CHECKPOINTFEES (each a comma-separated list, one entry per denomination)"
    token=$(norm_address "$1") || exit 2
    d=$(token_decimals "$token") || exit 1
    mint=$(parse_fee_list "$d" "$2") || exit 2
    burn=$(parse_fee_list "$d" "$3") || exit 2
    checkpoint=$(parse_fee_list "$d" "$4") || exit 2
    run_action admin "announceFees(address,uint256[],uint256[],uint256[])" \
      "token $token, mint fees $mint, burn fees $burn, checkpoint fees $checkpoint (base units at $d decimals; in force after the seven-day notice)" \
      "$token" "[$mint]" "[$burn]" "[$checkpoint]"
    ;;

  apply-fees)
    (($# == 1)) || usage_error "apply-fees TOKEN"
    token=$(norm_address "$1") || exit 2
    run_action "anyone" "applyFees(address)" \
      "token $token (permissionless; nothing happens before the notice is over)" "$token"
    ;;

  propose-roles)
    (($# == 4)) || usage_error "propose-roles WATCHER0 WATCHER1 WATCHER2 FREEZE_AUTHORITY"
    w0=$(norm_address "$1") || exit 2
    w1=$(norm_address "$2") || exit 2
    w2=$(norm_address "$3") || exit 2
    fa=$(norm_address "$4") || exit 2
    run_action admin "proposeRoles(address[3],address)" \
      "watchers [$w0, $w1, $w2], freeze authority $fa (applied by anyone after the 14-day timelock; address(0) keeps the current freeze authority)" \
      "[$w0, $w1, $w2]" "$fa"
    ;;

  apply-roles)
    (($# == 0)) || usage_error "apply-roles takes no arguments"
    run_action "anyone" "applyRoles()" "the pending role proposal, once its timelock is over"
    ;;

  transfer-admin)
    (($# == 1)) || usage_error "transfer-admin NEW_ADMIN"
    newadmin=$(norm_address "$1") || exit 2
    run_action admin "transferAdmin(address)" "new admin $newadmin (it must call accept-admin)" "$newadmin"
    ;;

  accept-admin)
    (($# == 0)) || usage_error "accept-admin takes no arguments"
    run_action "the pending admin" "acceptAdmin()" "completes the two-step admin handover"
    ;;

  set-deposits-frozen)
    (($# == 1)) || usage_error "set-deposits-frozen true|false"
    flag=$(norm_bool "$1")
    run_action "freeze authority" "setDepositsFrozen(bool)" "all deposits frozen: $flag (a freeze lasts maxFreeze unless renewed)" "$flag"
    ;;

  set-releases-frozen)
    (($# == 1)) || usage_error "set-releases-frozen true|false"
    flag=$(norm_bool "$1")
    run_action "freeze authority" "setReleasesFrozen(bool)" "all releases frozen: $flag" "$flag"
    ;;

  set-checkpoints-frozen)
    (($# == 1)) || usage_error "set-checkpoints-frozen true|false"
    flag=$(norm_bool "$1")
    run_action "freeze authority" "setCheckpointsFrozen(bool)" "candidates and finalisation frozen: $flag" "$flag"
    ;;

  set-token-deposits-frozen)
    (($# == 2)) || usage_error "set-token-deposits-frozen TOKEN true|false"
    token=$(norm_address "$1") || exit 2
    flag=$(norm_bool "$2") || exit 2
    run_action "freeze authority" "setTokenDepositsFrozen(address,bool)" "token $token, its deposits frozen: $flag" "$token" "$flag"
    ;;

  set-token-releases-frozen)
    (($# == 2)) || usage_error "set-token-releases-frozen TOKEN true|false"
    token=$(norm_address "$1") || exit 2
    flag=$(norm_bool "$2") || exit 2
    run_action "freeze authority" "setTokenReleasesFrozen(address,bool)" "token $token, its releases frozen: $flag" "$token" "$flag"
    ;;

  revoke-from)
    (($# == 1)) || usage_error "revoke-from CHECKPOINT_INDEX"
    is_uint "$1" || usage_error "the checkpoint index is a number: '$1'"
    run_action "freeze authority" "revokeFrom(uint256)" \
      "checkpoint $1 and every later one stop counting (reject-only, within the revoke window)" "$1"
    ;;

  retire)
    (($# == 0)) || usage_error "retire takes no arguments"
    run_action "freeze authority (anyone once retireOpensAt has passed)" "retire()" \
      "stops deposits forever; escrow, checkpoints and releases are untouched"
    ;;

  cancel-candidate)
    (($# == 1)) || usage_error "cancel-candidate NEW_STATE"
    is_bytes32 "$1" || usage_error "not a 32-byte hash: '$1'"
    run_action "a watcher or the freeze authority" "cancelCandidate(bytes32)" "the pending candidate of state $1" "$1"
    ;;

  pause-checkpoints)
    (($# == 0)) || usage_error "pause-checkpoints takes no arguments"
    run_action "a watcher" "pauseCheckpoints()" "submitCandidate and finalisation stop until 12 h from now"
    ;;

  *)
    usage_error "unknown action '$ACTION'"
    ;;
esac
