#!/usr/bin/env bash
# The reproduction check: the README's quick start and the guests' image ids, each with its own log, then one line
# per check and a verdict. Made for the image of scripts/reproduce/Dockerfile, where the tree is a copy at
# /work/ferrata (scripts/reproduce/run.sh starts it with Docker, scripts/reproduce/modal_app.py on Modal); it runs as
# well on any machine with those toolchains and with ../upstream and ../risc0-home-lez beside the tree. It changes the
# tree it runs in (vault/lib, vectors/, target/ and, through the LEZ harness, the tables in lez/results/): run it on a
# copy.
#
#   scripts/reproduce/check.sh [--out DIR] [--only NAME[,NAME...]]
#
#   verify-sepolia  python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-3, then e2e/runs/crown-live-1: read-only
#                   calls to a public Sepolia endpoint ($SEPOLIA_RPC_URL if set; a second public one if the first fails)
#   vault           cd vault && forge install --no-git <vault/README.md's pins> && FOUNDRY_OFFLINE=false forge build
#                   && forge test
#   core            scripts/fetch-vectors.sh && cargo test --workspace --locked
#   guest-ids       scripts/reproduce-image-ids.sh build, then compare: the 14 guests with docs/image-ids.md, the
#                   crown run's programs with e2e/crown/README.md
#   lez-harness     cd lez && RISC0_HOME=../../risc0-home-lez RISC0_DEV_MODE=1 cargo test -p harness --locked
#                   -- --test-threads 1   (FERRATA_HARNESS_THREADS changes the 1)
#
# Exit 0 when every check run passed. DIR (default target/reproduce/<UTC time>) gets one log per check,
# environment.txt and packages.txt (the versions), tree-changed.txt (committed files the checks rewrote), summary.txt.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$(cd "$HERE/../.." && pwd)"
BESIDE="$(cd "$SRC/.." && pwd)"
ALL="verify-sepolia vault core guest-ids lez-harness"
ONLY="" OUT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --only) ONLY="$(echo "$2" | tr ',' ' ')"; shift 2 ;;
    -h|--help) sed -n '2,22p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
for c in $ONLY; do
  case " $ALL " in *" $c "*) ;; *) echo "unknown check $c (one of: $ALL)" >&2; exit 2 ;; esac
done
CHECKS="${ONLY:-$ALL}"
OUT="${OUT:-$SRC/target/reproduce/$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$OUT" && OUT="$(cd "$OUT" && pwd)" || exit 2
export RISC0_HOME_LEZ="${RISC0_HOME_LEZ:-$BESIDE/risc0-home-lez}" CARGO_TERM_COLOR=never
THREADS="${FERRATA_HARNESS_THREADS:-1}"
COMMIT="${FERRATA_COMMIT:-$(git -C "$SRC" rev-parse HEAD 2>/dev/null || echo 'not a git checkout')}"
T_START=$(date +%s)
mmss() { printf '%d:%02d' $(($1 / 60)) $(($1 % 60)); }
say() { echo "[$(date -u +%T)] $*"; }

# ---- the environment -------------------------------------------------------------------------------------------
missing=""
for t in cargo rustup rzup r0vm forge python3 git curl; do command -v "$t" > /dev/null || missing="$missing $t"; done
[ -z "$missing" ] || { echo "not found:$missing (HOME=$HOME, PATH=$PATH): run this in the image of scripts/reproduce/Dockerfile" >&2; exit 2; }
solc="$(ls "$HOME"/.svm/0.8.26/solc-0.8.26 "$HOME"/.local/share/svm/0.8.26/solc-0.8.26 2>/dev/null | head -n 1)"
ver() { grep -o -m 1 -E '[0-9]+\.[0-9]+\.[0-9]+' | head -n 1; }
guest_rust() { ls "$1/toolchains" 2>/dev/null | grep -o -E 'v[0-9.]+-rust' | sed 's/^v//; s/-rust$//' | tr '\n' ' ' | sed 's/ $//'; }
RUSTS="$(for t in 1.93.0 1.98.1 1.88; do rustc +$t --version 2>/dev/null | ver; done | tr '\n' ' ' | sed 's/ $//')"
TOOLS="rust ${RUSTS:-?} (default $(rustc --version | ver)); RISC Zero r0vm $(r0vm --version 2>&1 | ver), guest rust $(guest_rust "$HOME/.risc0") (lez: $(guest_rust "$RISC0_HOME_LEZ")); forge $(forge --version 2>&1 | ver), solc $([ -n "$solc" ] && "$solc" --version 2>/dev/null | ver || echo 'not cached'); $(python3 --version 2>&1)"
{
  echo "commit: $COMMIT"
  echo "date: $(date -u '+%F %T') UTC; $(nproc) CPUs; $(awk '/MemTotal/ {printf "%.0f GiB", $2 / 1048576}' /proc/meminfo) memory; $(df -h --output=avail "$SRC" | tail -n 1 | tr -d ' ') free at the tree"
  echo "toolchains: $TOOLS"
  for t in 1.93.0 1.98.1 1.88; do echo "rustc +$t: $(rustc +$t --version 2>&1)"; done
  echo "cargo: $(cargo --version)"; echo "rustup: $(rustup --version 2>/dev/null)"
  echo "rzup: $(rzup --version 2>&1)"
  echo "~/.risc0:"; rzup show 2>&1 | sed 's/^/  /'
  echo "$RISC0_HOME_LEZ:"; RISC0_HOME="$RISC0_HOME_LEZ" rzup show 2>&1 | sed 's/^/  /'
  echo "r0vm: $(r0vm --version 2>&1)"; echo "cargo risczero: $(cargo risczero --version 2>&1)"
  echo "forge: $(forge --version 2>&1 | tr '\n' ' ')"
  [ -n "$solc" ] && echo "solc: $solc, sha256 $(sha256sum "$solc" | cut -c1-64)"
  echo "python3: $(python3 --version 2>&1), $(command -v python3)"
  for d in "$BESIDE"/upstream/*/; do echo "upstream $(basename "$d"): $(git -C "$d" rev-parse HEAD 2>&1)"; done
  echo "packages: $(dpkg-query -W -f '${Package} ${Version}; ' clang libclang-dev cmake protobuf-compiler libssl-dev \
    libpcsclite-dev libsqlite3-dev git curl python3 2>/dev/null)"
} > "$OUT/environment.txt" 2>&1
dpkg-query -W -f '${Package}=${Version}\n' > "$OUT/packages.txt" 2> /dev/null

# Every file of the tree before the checks, to say afterwards which committed files they rewrote.
( cd "$SRC" && find . \( -name .git -o -name target -o -name node_modules -o -path ./vault/lib -o -path ./vault/out \
    -o -path ./vault/cache -o -path ./bridge/evm-verify/lib -o -path ./bridge/evm-verify/out \) -prune -o -type f -print0 \
  | LC_ALL=C sort -z | xargs -0 sha256sum ) > "$OUT/tree-before.sha256"

say "Ferrata reproduction check, commit $COMMIT; checks: $CHECKS; logs in $OUT"
say "$TOOLS"

# ---- the checks (each runs in a subshell at the root of the tree) -----------------------------------------------
c_verify_sepolia() {   # the final programs' live run (crown-live-3), then the first live run (crown-live-1)
  local rpcs r run one rc=0
  if [ -n "${SEPOLIA_RPC_URL:-}" ]; then rpcs=("$SEPOLIA_RPC_URL")
  else rpcs=(https://ethereum-sepolia-rpc.publicnode.com https://rpc.sepolia.ethpandaops.io); fi
  for run in crown-live-3 crown-live-1; do
    one=1
    for r in "${rpcs[@]}"; do
      echo "== python3 -I scripts/verify-sepolia.py e2e/runs/$run --rpc $([ -n "${SEPOLIA_RPC_URL:-}" ] && echo '$SEPOLIA_RPC_URL' || echo "$r")"
      python3 -I scripts/verify-sepolia.py "e2e/runs/$run" --rpc "$r"; one=$?
      [ $one = 0 ] && break
    done
    [ $one = 0 ] || rc=$one
  done
  return $rc
}
c_vault() {
  cd vault || return 1
  if [ ! -d lib/risc0-ethereum ]; then
    echo "== forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0"
    forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 \
      risc0/risc0-ethereum@v3.0.0 || return 1
  fi
  echo "== FOUNDRY_OFFLINE=false forge build"; FOUNDRY_OFFLINE=false forge build || return 1
  echo "== forge test"; forge test
}
c_core() {
  echo "== scripts/fetch-vectors.sh"; scripts/fetch-vectors.sh || return 1
  echo "== cargo test --workspace --locked"; cargo test --workspace --locked
}
c_guest_ids() {
  echo "== scripts/reproduce-image-ids.sh build --out $OUT/guest-ids"
  scripts/reproduce-image-ids.sh build --out "$OUT/guest-ids" || return 1
  echo "== scripts/reproduce-image-ids.sh compare --out $OUT/guest-ids"
  scripts/reproduce-image-ids.sh compare --out "$OUT/guest-ids"
}
c_lez_harness() {
  cd lez || return 1
  echo "== RISC0_HOME=$RISC0_HOME_LEZ RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads $THREADS"
  RISC0_HOME="$RISC0_HOME_LEZ" RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads "$THREADS"
}

# What a check's log says, in one line.
detail() { # name log
  case "$1" in
    verify-sepolia)   # one result per run: the last endpoint's
      awk '/^== python3 -I scripts\/verify-sepolia.py / {run = $5; sub(/.*\//, "", run)}
           /^[0-9]+ passed, [0-9]+ pending, [0-9]+ failed/ {if (!(run in res)) ord[++n] = run; res[run] = $0}
           END {for (i = 1; i <= n; i++) printf "%s%s: %s", (i > 1 ? "; " : ""), ord[i], res[ord[i]]}' "$2" ;;
    vault)
      grep -E '[0-9]+ tests? passed, [0-9]+ failed, [0-9]+ skipped' "$2" | tail -n 1 | sed -E 's/.*: ([0-9]+ tests? passed)/\1/' ;;
    core|lez-harness)
      awk '/^test result:/ {p += $4; f += $6; i += $8; n++}
           END {printf "%d passed, %d failed, %d ignored in %d test runs", p, f, i, n}' "$2" ;;
    guest-ids)
      { grep -E '^[0-9]+ of [0-9]+ guests equal' "$2"; grep -E 'ids of the crown run' "$2"; } | tr '\n' ';' | sed 's/;$//; s/;/; /' ;;
  esac
}

declare -A RESULT TIME DETAIL
for name in $CHECKS; do
  log="$OUT/$name.log"; t0=$(date +%s); last=0
  say "$name: started"
  ( cd "$SRC" && "c_${name//-/_}" ) > "$log" 2>&1 &
  pid=$!
  while kill -0 "$pid" 2> /dev/null; do
    sleep 5
    el=$(($(date +%s) - t0))
    if [ $((el - last)) -ge 60 ]; then
      last=$el; say "$name: $(mmss $el) ... $(tail -n 1 "$log" 2> /dev/null | tr -d '\r' | cut -c1-100)"
    fi
  done
  wait "$pid"; rc=$?
  TIME[$name]=$(($(date +%s) - t0))
  RESULT[$name]=$([ $rc = 0 ] && echo PASS || echo FAIL)
  DETAIL[$name]="$(detail "$name" "$log")"
  [ $rc = 0 ] || DETAIL[$name]="${DETAIL[$name]:-no result line} (exit $rc; last lines in $name.log)"
  say "$name: ${RESULT[$name]} in $(mmss "${TIME[$name]}"): ${DETAIL[$name]}"
done
# verify-sepolia ran in a subshell: which endpoint answered is in its log.
if [ -f "$OUT/verify-sepolia.log" ]; then
  again="$(sed -n 's#^== python3 -I scripts/verify-sepolia.py e2e/runs/\([^ ]*\) .*#\1#p' "$OUT/verify-sepolia.log" \
    | uniq -d | tr '\n' ' ' | sed 's/ $//')"
  [ -n "$again" ] && DETAIL[verify-sepolia]="${DETAIL[verify-sepolia]} (second public endpoint for $again; the first failed)"
fi

# ---- committed files the checks rewrote -------------------------------------------------------------------------
( cd "$SRC" && sha256sum --quiet -c "$OUT/tree-before.sha256" 2> /dev/null ) | sed -n 's/: FAILED.*$//p' \
  | sed 's#^\./##' > "$OUT/tree-changed.txt"
nch=$(wc -l < "$OUT/tree-changed.txt")
if [ "$nch" = 0 ]; then TREE="no committed file changed"
else
  TREE="$nch committed files rewritten ($(sed -E 's#^([^/]+/[^/]+)/.*#\1/#' "$OUT/tree-changed.txt" | sort | uniq -c \
    | sort -rn | head -n 3 | awk '{printf "%s%s %d", (NR > 1 ? ", " : ""), $2, $1}')): tree-changed.txt"
fi

# ---- summary ------------------------------------------------------------------------------------------------------
pass=0; total=0; failed=""
for name in $CHECKS; do total=$((total + 1)); [ "${RESULT[$name]}" = PASS ] && pass=$((pass + 1)) || failed="$failed $name"; done
for name in $failed; do echo; echo "---- the end of $name.log"; tail -n 15 "$OUT/$name.log"; done
{
  echo
  echo "Ferrata reproduction check: commit $COMMIT"
  echo "$(date -u '+%F %T') UTC, $(nproc) CPUs; $TOOLS"
  echo
  printf '%-15s %-6s %7s  %s\n' check result time detail
  for name in $CHECKS; do
    printf '%-15s %-6s %7s  %s\n' "$name" "${RESULT[$name]}" "$(mmss "${TIME[$name]}")" "${DETAIL[$name]}"
  done
  echo
  echo "tree: $TREE"
  if [ -z "$failed" ]; then echo "VERDICT: PASS, $pass of $total checks, $(mmss $(($(date +%s) - T_START))) in all"
  else echo "VERDICT: FAIL, $pass of $total checks passed (failed:$failed), $(mmss $(($(date +%s) - T_START))) in all"; fi
  echo "logs: $OUT"
} | tee "$OUT/summary.txt"
[ -z "$failed" ]
