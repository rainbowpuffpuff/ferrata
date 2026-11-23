#!/usr/bin/env bash
# The names program (v2) on LEZ v0.4.0-rc3, on a build machine (the Modal build box, or any Linux host with LEZ's
# build dependencies and RISC Zero's toolchain). Nothing here reaches a public network: the sequencer is rc3's own,
# standalone (its in-process mock Bedrock), on loopback; the accounts are LEZ's published local test account and keys
# made for the run. The only download: Node 22.12.0 from nodejs.org (checked against its SHASUMS256) when the machine's
# Node is older than 22.7 / 20.19 (the CLI's lez.js needs ES module detection).
#
#   names/e2e/box.sh build     rc3's sequencer_service (feature `standalone`) and wallet CLI; a copy of this tree with
#                              rc3 selected (lez/scripts/lez-select.sh), where the names guest is built against rc3
#   names/e2e/box.sh harness   lez/harness/tests/names.rs and names_review.rs in that copy (rc3): their tables to
#                              names/e2e/results/harness-rc3/
#   names/e2e/box.sh ids       the image ids of the rc3 build (the copy) and of the rc1 build (this tree's lez/target,
#                              if built), with scripts/reproduce/guest_ids.py: names/e2e/results/image-ids.tsv
#   names/e2e/box.sh e2e NAME  start the sequencer, deploy the program with LEZ's wallet (program-loader deploy), run
#                              the CLI (names/js/cli.mjs) against it, record it in names/e2e/results/NAME; includes the
#                              review's M1 race: a reveal sent early (test-only switch) leaks the name, a copier commits
#                              with a backdated now_ms and reveals first, and the older commitment takes the name back
#
# Layout: LEZ v0.4.0-rc3 at ../upstream/lez-v0.4.0-rc3 beside the repository (the repository's convention, as
# lez/scripts/lez-select.sh) or LEZ_SRC; scratch in target/names-box inside the repository (git-ignored) or NAMES_WORK;
# signing with the web wallet's deploy/testnet/web/wallet/lez.js. Everything a command starts is stopped before it
# returns.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
LEZ_SRC="${LEZ_SRC:-$BESIDE/upstream/lez-v0.4.0-rc3}"
LEZ_COMMIT=bafd07be0701398f9bc435f73ab05e9a7936c301
WORK="${NAMES_WORK:-$REPO/target/names-box}"
BIN="$WORK/bin"; RUN="$WORK/run"; COPY="$WORK/tree"
RPC_PORT="${NAMES_RPC_PORT:-3041}"
BEDROCK_FUNDING_PUBLIC_KEY=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26   # LEZ's Justfile value
export CARGO_BUILD_JOBS="${NAMES_JOBS:-16}" CARGO_TERM_COLOR=never
export TMPDIR="$WORK/tmpdir" RISC0_HOME="${RISC0_HOME_LEZ:-$BESIDE/risc0-home-lez}"
mkdir -p "$BIN" "$TMPDIR"
unset GITHUB_TOKEN GH_TOKEN RISC0_DEV_MODE FERRATA_NAMES_UNSAFE_REVEAL FERRATA_NAMES_TEST_COMMIT_OFFSET_MS
die() { echo "error: $*" >&2; exit 1; }

check_lez() { # a git checkout must be at the tag, unmodified; a plain copy (the build box's) is taken as it is
  [ -f "$LEZ_SRC/Cargo.toml" ] || die "LEZ v0.4.0-rc3 not found at $LEZ_SRC"
  if head="$(git -C "$LEZ_SRC" rev-parse HEAD 2>/dev/null)" && [ "$(git -C "$LEZ_SRC" rev-parse --show-toplevel)" = "$LEZ_SRC" ]; then
    [ "$head" = "$LEZ_COMMIT" ] || die "$LEZ_SRC is not at v0.4.0-rc3"
    [ -z "$(git -C "$LEZ_SRC" status --porcelain)" ] || die "$LEZ_SRC is modified"
  fi
}

copy_tree() { # this tree with rc3 selected; the worktree itself stays as committed
  rm -rf "$COPY"; mkdir -p "$COPY"
  ln -sfn "$(dirname "$LEZ_SRC")" "$WORK/upstream"; ln -sfn "$RISC0_HOME" "$WORK/risc0-home-lez"
  tar -C "$REPO" --exclude=./lez/target --exclude=./target --exclude=./.git --exclude=./checkpoint/target \
      --exclude=./bridge/target --exclude=./vault/out --exclude=./vault/cache --exclude=./vault/lib -cf - . | tar -C "$COPY" -xf -
  (cd "$COPY/lez" && scripts/lez-select.sh v0.4.0-rc3 && scripts/lez-select.sh --show)
  # The manifests lez-select.sh does not list (the recorded-statement and rebate crates) follow it too.
  grep -rl --include=Cargo.toml 'upstream/lez-v0.4.0-rc1' "$COPY/lez" | xargs -r sed -i 's#upstream/lez-v0.4.0-rc1#upstream/lez-v0.4.0-rc3#g'
  ! grep -rl --include=Cargo.toml 'upstream/lez-v0.4.0-rc1' "$COPY/lez"
}

build() {
  check_lez
  ( cd "$LEZ_SRC" && CARGO_TARGET_DIR="$WORK/target-lez" cargo build --release --locked -p sequencer_service --features standalone )
  cp "$WORK/target-lez/release/sequencer_service" "$BIN/sequencer_service_standalone"
  ( cd "$LEZ_SRC" && CARGO_TARGET_DIR="$WORK/target-lez" cargo build --release --locked -p wallet )
  cp "$WORK/target-lez/release/wallet" "$BIN/wallet"
  copy_tree
  ( cd "$COPY/lez" && CARGO_TARGET_DIR="$WORK/target-rc3" cargo build --release -p methods-names )
  # The newest build of the guest (the one just made; risc0-build keeps older ones in other directories).
  local b; b="$(find "$WORK/target-rc3" -path '*riscv32im-risc0-zkvm-elf/release/*' -name 'ferrata_names.bin' -type f -printf '%T@ %p\n' | sort -n | tail -n 1 | cut -d' ' -f2-)"
  [ -n "$b" ] || die "no ferrata_names.bin"
  cp "$b" "$BIN/ferrata_names.bin"
  ( cd "$BIN" && sha256sum sequencer_service_standalone wallet ferrata_names.bin | tee SHA256SUMS )
}

harness() {
  copy_tree   # always this tree as it is now (a copy left by an earlier run would test older sources)
  local OUT="$HERE/results/harness-rc3"; rm -rf "$OUT"; mkdir -p "$OUT"
  local t
  for t in names names_review; do
    ( cd "$COPY/lez" && RISC0_DEV_MODE=1 CARGO_TARGET_DIR="$WORK/target-rc3" \
        cargo test -p harness --features names --test "$t" -- --nocapture --test-threads 1 ) > "$WORK/harness-rc3-$t.log" 2>&1 \
      || { grep -E "panicked|^error" -A3 "$WORK/harness-rc3-$t.log" | head -20; die "harness $t failed on rc3"; }
    grep -E '^test [a-z0-9_]+ \.\.\.|^test result' "$WORK/harness-rc3-$t.log" | sed -E 's/^(test [a-z0-9_]+ \.\.\.).* (ok|FAILED)$/\1 \2/; s/^(test [a-z0-9_]+ \.\.\.) .*/\1/' >> "$OUT/summary.txt"
  done
  cp "$COPY"/lez/results/names-*.md "$COPY"/lez/results/names-vectors.json "$OUT/"
  cat "$OUT/summary.txt"
}

ids() {
  [ -d "$COPY/lez" ] || die "build first"
  ln -sfn "$WORK/target-rc3" "$COPY/lez/target"
  local OUT="$HERE/results/image-ids.tsv"
  python3 -I "$REPO/scripts/reproduce/guest_ids.py" "$COPY" "$WORK/ids-rc3.tsv"
  { head -1 "$WORK/ids-rc3.tsv" | sed 's/^/build\t/'; grep -P '\tferrata_names\t' "$WORK/ids-rc3.tsv" | sed 's/^/rc3\t/'; } > "$OUT"
  if [ -d "$REPO/lez/target" ]; then
    python3 -I "$REPO/scripts/reproduce/guest_ids.py" "$REPO" "$WORK/ids-rc1.tsv" || true
    grep -P '\tferrata_names\t' "$WORK/ids-rc1.tsv" | sed 's/^/rc1\t/' >> "$OUT" || true
  fi
  cat "$OUT"
}

# Node 22.7+ / 20.19+ for the CLI: the machine's, or nodejs.org's 22.12.0 checked against its SHASUMS256.
NODE=node
ensure_node() {
  if command -v node >/dev/null 2>&1 && node -e 'const [a,b]=process.versions.node.split(".").map(Number); process.exit(a>22||(a===22&&b>=7)||(a===20&&b>=19)?0:1)'; then
    NODE=node; return
  fi
  local N="$WORK/node22" V=v22.12.0 T=node-v22.12.0-linux-x64.tar.xz
  if [ ! -x "$N/bin/node" ]; then
    mkdir -p "$N.dl" "$N"
    curl -sSfL -o "$N.dl/$T" "https://nodejs.org/dist/$V/$T"
    curl -sSfL -o "$N.dl/SHASUMS256.txt" "https://nodejs.org/dist/$V/SHASUMS256.txt"
    (cd "$N.dl" && grep " $T\$" SHASUMS256.txt | sha256sum -c -) || die "Node's checksum does not match"
    tar -xJf "$N.dl/$T" -C "$N" --strip-components=1
  fi
  NODE="$N/bin/node"
}

rpc() { curl -sS -m 10 -H 'Content-Type: application/json' \
  --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}" "http://127.0.0.1:$RPC_PORT"; }

SEQ_PID=""
stop_all() { if [ -n "$SEQ_PID" ]; then kill -TERM "$SEQ_PID" 2>/dev/null || true; sleep 1; kill -KILL "$SEQ_PID" 2>/dev/null || true; fi; }

start_sequencer() {
  rm -rf "$RUN"; mkdir -p "$RUN/logs"
  python3 -I - "$LEZ_SRC/lez/sequencer/service/networks/local.json" "$RUN/sequencer_config.json" <<'EOF'
import json, sys
src, dst = sys.argv[1:3]
c = json.load(open(src))
c["gossip"]["listen_addr"] = "/ip4/127.0.0.1/udp/0/quic-v1"
c["metrics_listen_address"] = "127.0.0.1:9001"
c["block_create_timeout"] = "2s"
json.dump(c, open(dst, "w"), indent=2)
EOF
  "$BIN/sequencer_service_standalone" setup "$RUN/sequencer_home" --config "$RUN/sequencer_config.json" \
    --funding-public-key "$BEDROCK_FUNDING_PUBLIC_KEY" --create-channel > "$RUN/logs/setup.log" 2>&1 \
    || { tail -30 "$RUN/logs/setup.log"; die "sequencer setup failed"; }
  RUST_LOG="info,risc0_zkvm=warn,kameo=warn" "$BIN/sequencer_service_standalone" start "$RUN/sequencer_home" \
    --rpc-listen-address "127.0.0.1:$RPC_PORT" > "$RUN/logs/sequencer.log" 2>&1 &
  SEQ_PID=$!
  trap stop_all EXIT
  local i head=""
  for i in $(seq 1 180); do
    head="$(rpc getLastBlockId 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p')" || true
    [ -n "$head" ] && [ "$head" -ge 1 ] && break
    kill -0 "$SEQ_PID" 2>/dev/null || { tail -40 "$RUN/logs/sequencer.log"; die "the sequencer exited"; }
    sleep 1
  done
  [ -n "$head" ] || die "the sequencer's RPC did not answer"
  echo "== sequencer up: head $head"
}

wallet() {
  export LEE_WALLET_HOME_DIR="$RUN/cli-wallet" LEE_WALLET_PASSWORD="local-test-only"
  if [ ! -f "$LEE_WALLET_HOME_DIR/wallet_config.json" ]; then
    mkdir -p "$LEE_WALLET_HOME_DIR"
    sed "s#http://127.0.0.1:3040#http://127.0.0.1:$RPC_PORT#" "$LEZ_SRC/lez/wallet/configs/debug/wallet_config.json" \
      > "$LEE_WALLET_HOME_DIR/wallet_config.json"
  fi
  "$BIN/wallet" "$@"
}
new_public() { wallet account new public 2>&1 | grep -o -E 'Public/[1-9A-HJ-NP-Za-km-z]{32,44}' | head -1 | cut -d/ -f2; }
field() { python3 -I -c 'import json,sys; v=json.load(open(sys.argv[1])); print(eval("v" + sys.argv[2]))' "$1" "$2"; }

e2e() {
  local name="${1:-local}"
  local OUT="$HERE/results/$name"; rm -rf "$OUT"; mkdir -p "$OUT"
  ensure_node
  "$NODE" --version > "$OUT/node-version.txt"
  start_sequencer
  # LEZ's published local test account A (its key is in LEZ's own source, lez/testnet_initial_state) pays for
  # everything: imported into the CLI wallet, read from that source, never written here.
  local sk A best
  sk="$(python3 -I - "$LEZ_SRC/lez/testnet_initial_state/src/lib.rs" <<'PY'
import re, sys
src = open(sys.argv[1]).read()
m = re.search(r"PRIVATE_KEY_PUB_ACC_A: \[u8; 32\] = \[([^\]]*)\]", src)
print(bytes(int(x) for x in m.group(1).replace("\n", " ").split(",") if x.strip()).hex())
PY
)"
  A="$(wallet account import public --private-key "$sk" 2>&1 | grep -o -E 'Public/[1-9A-HJ-NP-Za-km-z]{32,44}' | head -1 | cut -d/ -f2)"
  [ -n "$A" ] || die "could not import LEZ's local test account"
  best="$(rpc getAccountBalance "[\"$A\"]" | sed -n 's/.*"result":"\{0,1\}\([0-9]*\)"\{0,1\}.*/\1/p')"
  echo "== payer $A, balance $best"
  # Deploy: a header account and as many segment accounts as the loader chunks the program into (asked first).
  local H; H="$(new_public)"; local segs=() n=1 out
  segs+=("Public/$(new_public)")
  out="$(wallet program-loader deploy --elf "$BIN/ferrata_names.bin" --header "Public/$H" --segments "${segs[@]}" --payer "Public/$A" --immutable 2>&1)" || true
  if echo "$out" | grep -q -E 'splits into [0-9]+ segment'; then
    n="$(echo "$out" | grep -o -E 'splits into [0-9]+' | head -1 | grep -o -E '[0-9]+')"
    while [ "${#segs[@]}" -lt "$n" ]; do segs+=("Public/$(new_public)"); done
    out="$(wallet program-loader deploy --elf "$BIN/ferrata_names.bin" --header "Public/$H" --segments "${segs[@]}" --payer "Public/$A" --immutable 2>&1)"
  fi
  echo "$out" > "$OUT/deploy.txt"
  echo "$out" | grep -q -i 'error' && { tail -5 "$OUT/deploy.txt"; die "deployment failed"; }
  echo "== deployed at $H (${#segs[@]} segments, immutable)"
  local P="$H"
  export FERRATA_RPC="http://127.0.0.1:$RPC_PORT" FERRATA_NAMES_PROGRAM="$P"
  local CLI=("$NODE" "$REPO/names/js/cli.mjs") K="$RUN/keys"; mkdir -p "$K"
  run() { # label, expected exit, command...
    local label="$1" want="$2"; shift 2
    local got=0; { echo "\$ ferrata-names ${*}"; "${CLI[@]}" "$@"; } > "$OUT/step.txt" 2>&1 || got=$?
    { echo "## $label (exit $got, expected $want)"; echo '```'; cat "$OUT/step.txt"; echo '```'; } >> "$OUT/transcript.md"
    cp "$OUT/step.txt" "$RUN/last.txt"; rm -f "$OUT/step.txt"
    [ "$got" = "$want" ] && echo "ok   $label" | tee -a "$OUT/checks.txt" || { echo "FAIL $label (exit $got, expected $want)" | tee -a "$OUT/checks.txt"; }
  }
  owner_is() { # label, name, account: the record at the name's address is that name's and that account's
    local got; got="$("${CLI[@]}" whois "$2" | python3 -I -c 'import json,sys; v=json.load(sys.stdin); r=v["record"] or {}; print(r.get("name"), r.get("owner"))')"
    echo "## $1: whois $2 → $got" >> "$OUT/transcript.md"
    [ "$got" = "$2 $3" ] && echo "ok   $1" | tee -a "$OUT/checks.txt" || echo "FAIL $1 (got $got)" | tee -a "$OUT/checks.txt"
  }
  balance() { rpc getAccountBalance "[\"$1\"]" | sed -n 's/.*"result":"\{0,1\}\([0-9]*\)"\{0,1\}.*/\1/p'; }
  charged() { # label, account, expected exit, command... : the fee the action cost its signer (balance before - after)
    local label="$1" acct="$2" want="$3"; shift 3
    local b0; b0="$(balance "$acct")"; run "$label" "$want" "$@"; local b1; b1="$(balance "$acct")"
    echo "| $label | $((b0 - b1)) |" >> "$OUT/fees.md"
  }
  wait_window() { # state file: until the CLI would send its reveal (now - 30 s >= revealFrom)
    local from; from="$(field "$1" '["revealFrom"]')"
    local s=$(( (from + 30000 + 2000) / 1000 - $(date +%s) ))
    [ "$s" -gt 0 ] && { echo "   waiting $s s for $(basename "$1")'s reveal window"; sleep "$s"; } || true
  }
  echo "# Fees charged on the local rc3 sequencer (measured: the signer's balance before and after; native units)" > "$OUT/fees.md"
  echo "| action | fee |" >> "$OUT/fees.md"; echo "|---|---:|" >> "$OUT/fees.md"
  local who
  for who in alice bob mallory; do "${CLI[@]}" keygen "$K/$who.key" > "$K/$who.json"; done
  local a b m
  a="$(field "$K/alice.json" '["account"]')"; b="$(field "$K/bob.json" '["account"]')"; m="$(field "$K/mallory.json" '["account"]')"
  for who in alice bob mallory; do
    wallet auth-transfer send --from "Public/$A" --to-pk "$(field "$K/$who.json" '["publicKey"]')" --amount 10000000000 > "$OUT/fund-$who.txt" 2>&1
  done
  sleep 6
  { echo "program $P"; echo "alice $a"; echo "bob $b"; echo "mallory $m"; } > "$OUT/accounts.txt"
  cd "$RUN"   # the CLI's commitment state files (NAME.commit.json) stay in the run directory

  run "rules" 0 rules
  run "check Alice (normalised to alice)" 0 check Alice
  run "check a--b (refused offline)" 1 check a--b
  run "check faucet1 (a variant of a reserved name, refused offline)" 1 check faucet1
  run "resolve alice before it exists" 1 resolve alice

  # The review's M1 race, replayed: alice's reveal is sent too early (the test-only switch the CLI otherwise refuses),
  # reverts, and so publishes `alice`; mallory commits with now_ms 105 s back and reveals first, as soon as her window
  # opens; alice reveals in her own window: her commitment is older, so the name is hers.
  charged "alice: commit alice (a hash; the name stays hidden)" "$a" 0 commit alice --key "$K/alice.key" --state "$RUN/alice.commit.json"
  run "alice: reveal at once (refused by the CLI: too early; nothing sent)" 1 reveal alice --key "$K/alice.key" --state "$RUN/alice.commit.json"
  run "alice: reveal at once with --force (ignored; nothing sent)" 1 reveal alice --force --key "$K/alice.key" --state "$RUN/alice.commit.json"
  FERRATA_NAMES_UNSAFE_REVEAL=1 run "alice: reveal sent early anyway (test-only switch): TOO_EARLY, reverted, the name is public" 1 reveal alice --key "$K/alice.key" --state "$RUN/alice.commit.json"
  FERRATA_NAMES_TEST_COMMIT_OFFSET_MS=-75000 charged "mallory: commit alice with now_ms 105 s back (the copier)" "$m" 0 commit alice --key "$K/mallory.key" --state "$RUN/mallory-alice.commit.json"
  wait_window "$RUN/mallory-alice.commit.json"
  charged "mallory: reveal alice first (registered, provisional)" "$m" 0 reveal alice --key "$K/mallory.key" --state "$RUN/mallory-alice.commit.json"
  owner_is "after mallory's reveal, alice is mallory's" alice "$m"
  wait_window "$RUN/alice.commit.json"
  charged "alice: reveal alice in her window (older commitment: Displaced)" "$a" 0 reveal alice --key "$K/alice.key" --state "$RUN/alice.commit.json"
  owner_is "after alice's reveal, alice is alice's" alice "$a"
  run "account mallory (alice still in her list)" 0 account "$m"
  charged "mallory: forget (the names that left her)" "$m" 0 forget --key "$K/mallory.key"
  run "account mallory (empty list)" 0 account "$m"

  run "resolve alice" 0 resolve alice
  run "whois alice" 0 whois alice
  charged "alice: set-primary alice" "$a" 0 set-primary alice --key "$K/alice.key"
  run "reverse alice's account" 0 reverse "$a"
  run "bob reveals alice's commitment with his key (refused by the CLI: no commitment; nothing sent)" 1 reveal alice --key "$K/bob.key" --state "$RUN/alice.commit.json"
  run "bob registers alice: commit, wait, reveal (refused by the CLI: taken; the reveal not sent)" 1 register alice --key "$K/bob.key" --state "$RUN/bob-alice.commit.json"
  run "bob offers alice to himself (not owner: reverted on chain)" 1 transfer alice "$b" --key "$K/bob.key"
  python3 -I -c 'import sys; open(sys.argv[1],"w").write("11"*32+"\n"+"22"*1184+"\n")' "$RUN/receiver.keys"
  charged "alice: set-target alice, add a private receiver" "$a" 0 set-target alice --receiver-keys "$RUN/receiver.keys" --key "$K/alice.key"
  run "resolve alice (with receiver)" 0 resolve alice
  charged "alice: renew alice" "$a" 0 renew alice --key "$K/alice.key"
  charged "alice: offer alice to bob (transfer)" "$a" 0 transfer alice "$b" --key "$K/alice.key"
  run "resolve alice (still alice's, offer pending)" 0 resolve alice
  charged "bob: accept alice" "$b" 0 accept alice --key "$K/bob.key"
  owner_is "after bob's accept, alice is bob's" alice "$b"
  run "reverse alice's account (her stored primary no longer points at her)" 1 reverse "$a"
  run "account alice (alice still in her list)" 0 account "$a"
  charged "alice: forget (the names that left her)" "$a" 0 forget --key "$K/alice.key"
  charged "bob: set-primary alice" "$b" 0 set-primary alice --key "$K/bob.key"
  run "reverse bob's account" 0 reverse "$b"
  charged "bob: release alice" "$b" 0 release alice --key "$K/bob.key"
  run "resolve alice after release" 1 resolve alice
  run "bob: register bob (commit, wait, reveal)" 0 register bob --key "$K/bob.key" --state "$RUN/bob.commit.json"
  run "whois b0b (digit zero: a look-alike of bob is registered)" 0 whois b0b
  run "resolve b0b (not registered: a look-alike holds its address)" 1 resolve b0b
  run "faucet is reserved (refused before sending)" 1 register faucet --key "$K/bob.key"
  for who in alice mallory; do
    local acct; acct="$(field "$K/$who.json" '["account"]')"
    rpc getAccountView "[{\"account_id\":\"$acct\",\"program_account_id\":\"$P\"}]" > "$OUT/getAccountView-$who.json"
  done
  rpc getLastBlockId > "$OUT/head.json"
  # No machine paths in what is committed.
  grep -rl "$BESIDE" "$OUT" | xargs -r sed -i "s#$RUN/#<run>/#g; s#$REPO/#<repo>/#g; s#$BESIDE/#<beside>/#g"
  grep -c '^ok' "$OUT/checks.txt" | xargs echo "checks passed:" | tee -a "$OUT/checks.txt"
  ! grep -q '^FAIL' "$OUT/checks.txt"
}

case "${1:-}" in
  build) build ;;
  harness) harness ;;
  ids) ids ;;
  e2e) e2e "${2:-local}" ;;
  *) die "usage: box.sh build | harness | ids | e2e NAME" ;;
esac
