#!/usr/bin/env bash
# Deploy a Ferrata release on a LEZ testnet (testnet 0.4 when it opens) and an EVM chain: one command.
#
#   scripts/deploy-testnet.sh --release DIR --lez URL --evm-rpc URL --chain-id N [options]      plan only (default)
#   scripts/deploy-testnet.sh ... --send                                                         deploy for real
#
# Required:
#   --release DIR     a release directory (scripts/release.sh package); verified before anything else
#   --lez URL         the LEZ sequencer's RPC endpoint (testnet 0.4; LEZ's wallet defaults to https://testnet.lez.logos.co)
#   --evm-rpc URL     the EVM chain's JSON-RPC endpoint
#   --chain-id N      the EVM chain id the endpoint must report (1, Ethereum mainnet, is refused: prototype, unaudited)
# Options:
#   --mode A|B        the vault's finality rule (default B)
#   --params FILE     the vault's other parameters (lezConfigId, genesisState, nMin, tMin, postingTimeframe, the LEZ
#                     token definitions); the image ids always come from the release (default: the release's
#                     vault/script/params/sepolia.json, which a testnet deployment must replace with its own values)
#   --gas-rule R      eip7825 (default) | eip8037 (Sepolia since Glamsterdam)
#   --wallet-home DIR LEZ's wallet home (LEE_WALLET_HOME_DIR), whose wallet_config.json must name --lez; its funded
#                     account --payer pays the program deployments
#   --payer LABEL     the LEZ account that pays (a label or id the wallet knows)
#   --skip-lez | --skip-evm   one side only
#   --out DIR         the deployment record (default deployments/<chain id>-<mode>-<release version>/)
#   --send            send the transactions; without it nothing is sent and every command is printed
# The EVM key: Foundry's own variables (ETH_FROM, ETH_KEYSTORE + ETH_PASSWORD), as vault/script/deploy.sh reads them;
# FERRATA_UNLOCKED=1 for a local node that impersonates ETH_FROM (anvil --auto-impersonate). Never on a command line.
# Needs: bash, python3, curl, cast and forge (Foundry 1.3.6) and, for the LEZ side with --send, LEZ's `wallet`
# (v0.4.0-rc1 or the testnet's version) on PATH or in $LEZ_WALLET. The vault is deployed from this source tree
# (vault/script/deploy.sh), and only if forge's build of it is byte-identical to the release's bytecode.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
REL="" LEZ="" RPC="" CHAIN="" MODE=B PARAMS="" GAS_RULE=eip7825 WHOME="${LEE_WALLET_HOME_DIR:-}" PAYER="" OUTD="" SEND=0
SKIP_LEZ=0 SKIP_EVM=0
while [ $# -gt 0 ]; do
  case "$1" in
    --release) REL=$2; shift 2 ;;
    --lez) LEZ=$2; shift 2 ;;
    --evm-rpc) RPC=$2; shift 2 ;;
    --chain-id) CHAIN=$2; shift 2 ;;
    --mode) MODE=$2; shift 2 ;;
    --params) PARAMS=$2; shift 2 ;;
    --gas-rule) GAS_RULE=$2; shift 2 ;;
    --wallet-home) WHOME=$2; shift 2 ;;
    --payer) PAYER=$2; shift 2 ;;
    --out) OUTD=$2; shift 2 ;;
    --skip-lez) SKIP_LEZ=1; shift ;;
    --skip-evm) SKIP_EVM=1; shift ;;
    --send) SEND=1; shift ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "deploy: unknown argument $1" >&2; exit 2 ;;
  esac
done
die() { echo "deploy: $*" >&2; exit 1; }
[ -n "$REL" ] && [ -d "$REL" ] || die "--release DIR is required"
[ "$SKIP_LEZ" = 1 ] || [ -n "$LEZ" ] || die "--lez URL is required"
[ "$SKIP_EVM" = 1 ] || { [ -n "$RPC" ] && [ -n "$CHAIN" ]; } || die "--evm-rpc URL and --chain-id N are required"
case "$MODE" in A|B) ;; *) die "--mode A|B" ;; esac
[ "$CHAIN" != 1 ] || die "chain id 1 (Ethereum mainnet) is refused: the code is a prototype, unaudited"
REL=$(cd "$REL" && pwd)
VER=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$REL/MANIFEST.json")
OUTD=${OUTD:-$ROOT/vault/deployments/${CHAIN:-lez}-$MODE-$VER}
mkdir -p "$OUTD"; OUTD=$(cd "$OUTD" && pwd)
LOG=$OUTD/deploy.log
say() { echo "$*" | tee -a "$LOG"; }
run() { say "+ $*"; if [ "$SEND" = 1 ]; then "$@" 2>&1 | tee -a "$LOG"; return "${PIPESTATUS[0]}"; fi; }
: > "$LOG"
say "ferrata deploy: release $VER ($REL), mode $MODE, $( [ "$SEND" = 1 ] && echo SEND || echo 'plan only (nothing is sent)')"

# 1. The release: every checksum, the manifest, the image ids.
say "== 1. release"
if ! "$ROOT/scripts/release.sh" verify "$REL" 2>&1 | tee -a "$LOG"; then die "the release does not verify"; fi
for p in ferrata_lez ferrata_bridge checkpoint checkpoint_b release; do
  say "  $p $(python3 -I "$ROOT/scripts/release_manifest.py" id "$REL" "$p")"
done

# 2. LEZ: the attestation program and the bridge program, through LEZ's program loader.
if [ "$SKIP_LEZ" = 0 ]; then
  say "== 2. LEZ at $LEZ"
  code=$(curl -s -o /dev/null -m 15 -w '%{http_code}' "$LEZ" || true)
  say "  endpoint answers HTTP ${code:-none}"
  [ "$SEND" = 0 ] || [ "${code:-000}" != 000 ] || die "the LEZ endpoint does not answer"
  W=${LEZ_WALLET:-wallet}
  if [ "$SEND" = 1 ]; then
    command -v "$W" >/dev/null || die "LEZ's wallet not found (LEZ_WALLET)"
    [ -n "$WHOME" ] && [ -f "$WHOME/wallet_config.json" ] || die "--wallet-home DIR with wallet_config.json"
    python3 -I -c 'import json,sys; c=json.load(open(sys.argv[1])); a=c["sequencer_addr"].rstrip("/"); b=sys.argv[2].rstrip("/"); sys.exit(0 if a==b else f"wallet_config.json names {a}, not {b}")' \
      "$WHOME/wallet_config.json" "$LEZ" || die "the wallet points at another sequencer"
    [ -n "$PAYER" ] || die "--payer LABEL: a funded LEZ account"
    export LEE_WALLET_HOME_DIR=$WHOME
  fi
  for p in ferrata_lez ferrata_bridge; do
    bytes=$(stat -c %s "$REL/programs/$p.bin")
    n=$(( (bytes + 98303) / 98304 ))   # MAX_SEGMENT_DATA_LEN 96 KiB (program_loader core), at most 20 segments
    [ "$n" -le 20 ] || die "$p: $bytes bytes need $n segments, more than LEZ's 20"
    say "  $p: $bytes bytes, $n segments of at most 96 KiB"
    segs=()
    for i in $(seq 0 $((n - 1))); do
      run "$W" account new public --label "ferrata/$VER/$p/segment-$i" || die "account $p segment $i"
      segs+=("ferrata/$VER/$p/segment-$i")
    done
    run "$W" account new public --label "ferrata/$VER/$p/header" || die "account $p header"
    run "$W" program-loader deploy --elf "$REL/programs/$p.bin" --header "ferrata/$VER/$p/header" \
      --segments "${segs[@]}" --immutable ${PAYER:+--payer "$PAYER"} || die "deploy $p"
  done
  say "  next on LEZ (not automated here): Init of the attestation program from a bootstrap, then the bridge's"
  say "  configuration for the vault below (ferrata CLI / lez/live/driver, docs/guides)"
fi

# 3. The EVM chain: FerrataRegistry, the emergency stop, the Groth16 verifier and FerrataVault (vault/script/deploy.sh).
if [ "$SKIP_EVM" = 0 ]; then
  say "== 3. EVM chain at $RPC"
  got=$(cast chain-id --rpc-url "$RPC" 2>/dev/null || true)
  say "  chain id $got (expected $CHAIN)"
  [ "$got" = "$CHAIN" ] || die "the endpoint reports chain id '${got:-none}', not $CHAIN"
  base=${PARAMS:-$REL/vault/script/params/sepolia.json}
  python3 -I "$ROOT/scripts/release_manifest.py" params "$REL" "$base" > "$OUTD/params.json"
  say "  parameters: $OUTD/params.json (image ids from the release, the rest from $base)"
  # The vault deployed must be the release's: forge's build of this tree, byte for byte.
  ( cd "$ROOT/vault" && forge build > "$OUTD/forge-build.log" 2>&1 ) || die "forge build failed ($OUTD/forge-build.log)"
  if ! python3 -I - "$REL" "$ROOT/vault/out" <<'PY' | tee -a "$LOG"; then die "this tree's vault is not the release's: check out the release's commit"; fi
import hashlib, json, sys
rel, out = sys.argv[1], sys.argv[2]
m = json.load(open(f"{rel}/MANIFEST.json"))
bad = 0
for name, e in m["vault"].items():
    a = json.load(open(f"{out}/{name}.sol/{name}.json"))
    for k, field in (("creation_sha256", "bytecode"), ("runtime_sha256", "deployedBytecode")):
        h = hashlib.sha256(a[field]["object"].encode()).hexdigest()
        ok = h == e[k]
        bad += not ok
        print(f"  {name} {field}: {'equal to the release' if ok else 'DIFFERS from the release'}")
sys.exit(1 if bad else 0)
PY
  export FERRATA_MODE=$MODE FERRATA_RPC_URL=$RPC FERRATA_PARAMS=$OUTD/params.json FERRATA_GAS_RULE=$GAS_RULE \
    FERRATA_OUT=$OUTD/vault-deployment.json FERRATA_LOG_DIR=$OUTD/forge-logs
  if [ "$SEND" = 1 ]; then
    [ -n "${ETH_FROM:-}" ] || die "ETH_FROM: the deployer's address"
    run "$ROOT/vault/script/deploy.sh" || die "vault deployment failed (its record: $FERRATA_OUT)"
  else
    say "+ FERRATA_MODE=$MODE FERRATA_RPC_URL=$RPC FERRATA_PARAMS=$OUTD/params.json FERRATA_GAS_RULE=$GAS_RULE vault/script/deploy.sh"
  fi
fi

python3 -I - "$OUTD" "$VER" "$MODE" "${LEZ:-}" "${CHAIN:-}" "$SEND" <<'PY'
import json, os, sys, time
d, ver, mode, lez, chain, send = sys.argv[1:]
rec = {"release": ver, "mode": mode, "lez": lez or None, "chain_id": chain or None, "sent": send == "1",
       "at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
p = os.path.join(d, "vault-deployment.json")
if os.path.exists(p):
    rec["vault"] = json.load(open(p))
json.dump(rec, open(os.path.join(d, "deployment.json"), "w"), indent=1)
print(f"record: {d}/deployment.json")
PY
say "done"
