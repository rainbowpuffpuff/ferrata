#!/usr/bin/env bash
# bridge-next F1 box job 1 (run from the worktree root by mrun): the red team's finding F1 fixed in the second build.
# (1) Every guest of docs/image-ids.md's first table rebuilt in FRESH target directories (any that exist are moved out
# of the tree first; what could serve a cached build is listed in env.txt) by scripts/reproduce-image-ids.sh build,
# compared with docs/image-ids.md, e2e/crown/README.md and the live Sepolia vault's immutables (two read-only eth_calls
# over public JSON-RPC, the script's default endpoint); (2) the second build in the same fresh lez/target through the
# same wrapper, its id written into tenant.rs's SECOND_BRIDGE; (3) bridge-wire's unit tests; (4) the harness: the F1
# regression test and the red team's `init` group against both builds, every bridge suite, the privacy suite's LEZ
# half and the tenant test against the second build; (5) clippy. Steps log to ../tmp/bridge-next2/box/job1/.
set -uo pipefail
W="$PWD"; SP="$(cd "$W/.." && pwd)"; B="$SP/tmp/bridge-next2/box/job1"; mkdir -p "$B"; : > "$B/status.txt"
R="$B/repro"
step() { local name="$1"; shift; local t0; t0=$(date +%s)
  ( "$@" ) > "$B/$name.log" 2>&1; local rc=$?
  echo "$name exit $rc $(( $(date +%s) - t0 )) s $(date -u +%T)" | tee -a "$B/status.txt"; return $rc; }
cd "$W"
export CARGO_TERM_COLOR=never CARGO_BUILD_JOBS=16
unset RISC0_HOME CARGO_TARGET_DIR RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
{ date -u +%FT%TZ; echo "nproc $(nproc)"; cargo --version; r0vm --version
  echo "== environment that could redirect or cache a build:"
  env | grep -E '^(RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|SCCACHE|CARGO_TARGET_DIR|CARGO_BUILD_TARGET_DIR|CARGO_HOME|CARGO_INCREMENTAL|RISC0)' | sort
  command -v sccache || echo "no sccache on PATH"
  for f in "$HOME/.cargo/config" "$HOME/.cargo/config.toml" "$W/.cargo/config.toml" "$W/lez/.cargo/config.toml" \
           "$W/checkpoint/.cargo/config.toml" "$W/bridge/.cargo/config.toml"; do
    [ -e "$f" ] && { echo "== $f"; grep -v '^#' "$f"; }; done
  echo "== target directories before the job:"
  for d in target lez/target checkpoint/target bridge/target; do
    if [ -e "$d" ]; then echo "$d present ($(du -sh "$d" | cut -f1))"; else echo "$d absent"; fi; done
} > "$B/env.txt" 2>&1
# Outside the worktree: a moved target directory inside it would not be ignored by git, and mrun would copy it back.
X="$SP/tmp/bridge-next2/box-targets"; mkdir -p "$X"
for d in target lez/target checkpoint/target bridge/target; do
  [ -e "$d" ] && mv "$d" "$X/$(echo "$d" | tr / _).before-job1"; done
{ date -u +%FT%TZ; for d in target lez/target checkpoint/target bridge/target; do
    if [ -e "$d" ]; then echo "$d EXISTS"; else echo "$d absent"; fi; done; ls -A "$X"; } > "$B/fresh.txt"
rm -rf "$R"
export RISC0_HOME_LEZ="$SP/risc0-home-lez"
step repro bash scripts/reproduce-image-ids.sh build --out "$R"
step compare bash scripts/reproduce-image-ids.sh compare --out "$R" --vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c
step next bash -c 'cd lez && RUSTC_WRAPPER="$PWD/../scripts/guest-rustc" RISC0_BUILD_LOCKED=1 RISC0_HOME="$RISC0_HOME_LEZ" cargo build -p methods-bridge-next --locked'
grep -h "FERRATA_BRIDGE_NEXT_ID\|FERRATA_BRIDGE_NEXT_PATH" lez/target/debug/build/methods-bridge-next-*/out/methods.rs > "$B/next-id.txt" 2>&1
step ids15 python3 -I scripts/reproduce/guest_ids.py "$W" "$B/ids-with-next.tsv"
{ echo "guest binaries newer than fresh.txt (written before the build):"
  find lez/target checkpoint/target bridge/target -path '*riscv*' -type f \( -name '*.bin' -o -name 'ferrata_*' -o -name 'checkpoint*' -o -name segment -o -name release \) -newer "$B/fresh.txt" 2>/dev/null | grep -v '/deps/\|/build/\|\.d$' | sort
  echo; echo "guest binaries NOT newer than fresh.txt (expected: none):"
  find lez/target checkpoint/target bridge/target -path '*riscv*' -type f \( -name '*.bin' -o -name 'ferrata_*' -o -name 'checkpoint*' -o -name segment -o -name release \) ! -newer "$B/fresh.txt" 2>/dev/null | grep -v '/deps/\|/build/\|\.d$' | sort
  echo; echo "Compiling lines per log:"; for f in "$R"/logs/*-methods.log "$B/next.log"; do echo "$(grep -c 'Compiling' "$f") $f"; done
  echo; grep -h 'Compiling ferrata_bridge\|Compiling ferrata_lez \|Compiling ferrata-lez-bridge-wire\|Compiling ferrata_bridge_next' "$R/logs/lez-methods.log" "$B/next.log" | head -8
} > "$B/rebuild-evidence.txt" 2>&1
NEWID="$(awk -F'\t' '$2=="ferrata_bridge_next"{print substr($3,3)}' "$B/ids-with-next.tsv")"
echo "second build: $NEWID" >> "$B/status.txt"
if [ "${#NEWID}" = 64 ]; then
  sed -i "s/^const SECOND_BRIDGE: &str = \"[0-9a-f]\{64\}\";/const SECOND_BRIDGE: \&str = \"$NEWID\";/" lez/harness/tests/tenant.rs
fi
grep -n '^const SECOND_BRIDGE' lez/harness/tests/tenant.rs >> "$B/status.txt"
export RISC0_DEV_MODE=1 RISC0_HOME="$SP/risc0-home-lez"
step wire-next bash -c 'cd lez && cargo test -p ferrata-lez-bridge-wire --locked --features config-keyed --lib'
step wire-default bash -c 'cd lez && cargo test -p ferrata-lez-bridge-wire --locked --lib'
step norun-default bash -c 'cd lez && cargo test -p harness --locked --no-run'
step norun-next bash -c 'cd lez && cargo test -p harness --locked --features bridge-next --no-run'
step f1-default bash -c 'cd lez && cargo test -p harness --locked --test bridge_secfix f1_ -- --nocapture'
step f1-next bash -c 'cd lez && cargo test -p harness --locked --features bridge-next --test bridge_secfix f1_ -- --nocapture'
sha256sum lez/results/redteam-2026-11-18/redteam_bridge.init.json > "$B/redteam-init.sha256"
step redteam-init-default bash -c 'cd lez && cargo test -p harness --locked --test redteam_bridge -- --exact init --nocapture'
step redteam-init-next bash -c 'cd lez && cargo test -p harness --locked --features bridge-next --test redteam_bridge -- --exact init --nocapture'
{ ls -l --time-style=+%T lez/results/redteam-2026-11-18/redteam_bridge.init.json lez/results/bridge-next/redteam-2026-11-18/redteam_bridge.init.json
  sha256sum lez/results/redteam-2026-11-18/redteam_bridge.init.json lez/results/bridge-next/redteam-2026-11-18/redteam_bridge.init.json; } >> "$B/redteam-init.sha256" 2>&1
step suites-next bash -c 'cd lez && cargo test -p harness --locked --features bridge-next --test bridge_inbound --test bridge_private \
  --test bridge_burn --test bridge_controls --test bridge_vault_storage --test bridge_roles --test bridge_secfix \
  --test bridge_time -- --nocapture'
step tenant-next bash -c 'cd lez && cargo test -p harness --locked --features bridge-next --test tenant -- --nocapture --test-threads 1'
step privacy-next bash -c 'cd lez && FERRATA_PRIVACY_DIR="$W/target/privacy-next" cargo test -p harness --locked --features bridge-next \
  --test privacy_suite -- --nocapture'
step secfix-default bash -c 'cd lez && cargo test -p harness --locked --test bridge_secfix -- --nocapture'
export RISC0_SKIP_BUILD=1
step clippy-wire bash -c 'cd lez && cargo clippy -p ferrata-lez-bridge-wire --locked --features config-keyed --all-targets'
step clippy-harness-next bash -c 'cd lez && cargo clippy -p harness --locked --features bridge-next --all-targets'
step clippy-harness-default bash -c 'cd lez && cargo clippy -p harness --locked --all-targets'
echo "ALL DONE $(date -u +%T)" >> "$B/status.txt"
