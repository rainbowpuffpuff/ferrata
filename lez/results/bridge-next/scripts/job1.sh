#!/usr/bin/env bash
# bridge-next box job 1 (run from the worktree root by mrun): lockfile, the second guest, the wire's unit test, the
# harness compiled both ways, the default tenant test (the deployed figures must come out unchanged), the bridge
# suites against the second build (results under lez/results/bridge-next/). Steps log to ../tmp/bridge-next/box/job1/.
set -uo pipefail
W="$PWD"; SP="$(cd "$W/.." && pwd)"; B="$SP/tmp/bridge-next/box/job1"; mkdir -p "$B"; : > "$B/status.txt"
export CARGO_TERM_COLOR=never CARGO_BUILD_JOBS=16 RISC0_HOME="$SP/risc0-home-lez"
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
step() { local name="$1"; shift; local t0; t0=$(date +%s)
  ( "$@" ) > "$B/$name.log" 2>&1; local rc=$?
  echo "$name exit $rc $(( $(date +%s) - t0 )) s $(date -u +%T)" | tee -a "$B/status.txt"; return $rc; }
{ date -u; nproc; free -g | head -2; cargo --version; r0vm --version; ls "$SP/upstream"; ls "$SP/risc0-home-lez/toolchains"; git -C "$W" rev-parse HEAD; } > "$B/env.txt" 2>&1
cd "$W/lez"
step lock bash -c 'cargo metadata --format-version 1 --offline > /dev/null || cargo metadata --format-version 1 > /dev/null' || exit 1
step guest cargo build -p methods-bridge-next --locked || exit 1
grep -h "FERRATA_BRIDGE_NEXT_ID\|FERRATA_BRIDGE_NEXT_PATH" target/debug/build/methods-bridge-next-*/out/methods.rs > "$B/next-id.txt"
step wire cargo test -p ferrata-lez-bridge-wire --locked --features config-keyed --lib
step norun-default cargo test -p harness --locked --no-run || exit 1
step norun-next cargo test -p harness --locked --features bridge-next --no-run || exit 1
export RISC0_DEV_MODE=1
step tenant-default cargo test -p harness --locked --test tenant -- --nocapture --test-threads 1
step suites-next cargo test -p harness --locked --features bridge-next --test bridge_inbound --test bridge_private \
  --test bridge_burn --test bridge_controls --test bridge_vault_storage --test bridge_roles --test bridge_secfix \
  --test bridge_time -- --nocapture
step privacy-next env FERRATA_PRIVACY_DIR="$W/target/privacy-next" cargo test -p harness --locked --features bridge-next \
  --test privacy_suite -- --nocapture
echo "ALL DONE $(date -u +%T)" >> "$B/status.txt"
