#!/usr/bin/env bash
# `ferrata attest` against a running LEZ sequencer (RFP-021 U.2), on the build box (spike/modal-build's mrun; never on a
# laptop). Local only: a LEZ v0.4.0-rc1 zone (sequencer + Logos Blockchain node, LEZ's local configuration, dev-mode
# leader proofs) that we start and stop here; nothing is sent to any public network, and nothing is read from one.
#
#   mrun --put ../tmp/miniapp-sequencer/inputs --timeout 10800 -- 'bash cli/tests/attest-seq.sh NAME'
#
# 1. Build: crown-driver (lez/crown/driver) and the zone's bundle (crown-live-3's binaries and final programs, this
#    tree's driver and session.sh: lez/crown/build.sh repack), the `ferrata` tool (cli/, --features seqnet) and
#    `attest-monitor` (monitor/attest).
# 2. LEZ session A (lez/crown/session.sh a, CROWN_CHAIN=sepolia): a new zone, the final attestation and bridge programs
#    deployed, the Sepolia instance initialised from crates/ferrata-spectests/fixtures/sepolia-2026-11-12 (its Init is
#    valid by the weak-subjectivity bound until 28 Nov 2026 03:13Z) and its updates applied.
# 3. The zone again (session.sh x) with cli/tests/attest-seq-hook.sh: every command of `ferrata attest` against the
#    zone's sequencer, on real data only (Sepolia's updates as the lab recorded them; mainnet's bootstrap, finality
#    update and proofs of the attestation fixture of 10 Nov 2026, served over JSON-RPC by modules/scripts/recorded_rpc.py),
#    each a dry run first and then sent; then attest-monitor reads the zone's instance through the sequencer's RPC.
# Records: cli/results/attest-seq-NAME/ (synced back by mrun). ATTEST_SEQ_HOOK names another hook (the Logos Core run:
# modules/ferrata_attest_module/scripts/logos-run-hook.sh, which runs this one first).
set -euo pipefail
NAME="${1:?usage: attest-seq.sh NAME}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
T="${ATTEST_SEQ_TMP:-$BESIDE/tmp/attest-seq}"
IN="$BESIDE/tmp/miniapp-sequencer/inputs"
TGT="$T/target-lez"
RUN="$REPO/cli/results/attest-seq-$NAME"
W="$T/work-$NAME"
VOL="$W/vol"
export CARGO_TERM_COLOR=never PATH="$HOME/.cargo/bin:$PATH"
say() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$RUN/run.log"; }
die() { say "STOP: $*"; exit 1; }
rm -rf "$RUN" "$W"; mkdir -p "$RUN/cli" "$RUN/logs" "$W" "$VOL/inputs" "$T/bin"
dpkg -s libpcsclite-dev > /dev/null 2>&1 || { apt-get update -qq && apt-get install -y -qq libpcsclite-dev > /dev/null; }
command -v zstd > /dev/null || { apt-get update -qq && apt-get install -y -qq zstd > /dev/null; }
command -v jq > /dev/null || { apt-get update -qq && apt-get install -y -qq jq > /dev/null; }
say "attest-seq $NAME"

# ---- 1. build ----------------------------------------------------------------------------------------
L="$RUN/logs"
say "build: crown-driver"
( cd "$REPO/lez/crown/driver" && { [ -f Cargo.lock ] || cp "$REPO/lez/live/driver/Cargo.lock" Cargo.lock; } \
    && CARGO_TARGET_DIR="$TGT" cargo build --release ) > "$L/build-driver.log" 2>&1 \
  || { grep -E -A7 '^error' "$L/build-driver.log" | head -40; die "crown-driver build"; }
say "build: ferrata (cli, --features seqnet)"
( cd "$REPO/cli" && CARGO_TARGET_DIR="$TGT" cargo build --release --features seqnet --bin ferrata --bin seqnet-fund ) > "$L/build-cli.log" 2>&1 \
  || { grep -E -A7 '^error' "$L/build-cli.log" | head -60; die "cli build"; }
say "build: attest-monitor"
( cd "$REPO/monitor/attest" && cargo build --release --locked ) > "$L/build-monitor.log" 2>&1 \
  || { grep -E -A7 '^error' "$L/build-monitor.log" | head -40; die "attest-monitor build"; }
cp "$TGT/release/crown-driver" "$TGT/release/ferrata" "$TGT/release/seqnet-fund" "$REPO/monitor/attest/target/release/attest-monitor" "$T/bin/"
say "build: the zone's bundle (crown-live-3's, this tree's crown-driver and session.sh)"
mkdir -p "$T/crown-lez/bin"; cp "$T/bin/crown-driver" "$T/crown-lez/bin/crown-driver"
CROWN_LEZ_TMP="$T/crown-lez" bash "$REPO/lez/crown/build.sh" repack "$IN/bundle-crown-live-3.tar.zst" > "$L/repack.log" 2>&1 \
  || { tail -n 20 "$L/repack.log"; die "repack"; }
cp "$T/crown-lez/bundle.tar.zst" "$VOL/bundle.tar.zst"
( cd "$T/bin" && sha256sum ferrata attest-monitor crown-driver ) > "$RUN/binaries.sha256"
echo "bundle sha256 $(sha256sum "$VOL/bundle.tar.zst" | cut -c1-64)" >> "$RUN/binaries.sha256"

# ---- 2. LEZ session A --------------------------------------------------------------------------------
export CROWN_LEZ_VOL="$VOL" CROWN_LEZ_WORK="$W/zone" CROWN_CHAIN=sepolia
say "LEZ session A: a new zone, the final programs, the Sepolia instance from sepolia-2026-11-12"
bash "$REPO/lez/crown/session.sh" a > "$L/session-a.log" 2>&1 || { tail -n 30 "$L/session-a.log"; die "session A"; }
SA="$VOL/session-a/session-a.json"
jq '{tip, instance: .instance, attestation, bridge}' "$SA" > "$RUN/session-a.json"
say "session A: attestation program $(jq -r .attestation.account "$SA") (image $(jq -r .attestation.imageId "$SA")), Sepolia instance $(jq -r '.instance.id // .instance' "$SA")"

# ---- 3. the zone with the hook -----------------------------------------------------------------------
say "session X: ferrata attest against the zone's sequencer"
ATTEST_SEQ_RUN="$RUN" ATTEST_SEQ_BIN="$T/bin" ATTEST_SEQ_REPO="$REPO" ATTEST_SEQ_SA="$SA" ATTEST_SEQ_WORK="$W/hook" \
  CROWN_HOOK="${ATTEST_SEQ_HOOK:-$HERE/attest-seq-hook.sh}" CROWN_DEADLINE_MIN=150 \
  bash "$REPO/lez/crown/session.sh" x > "$L/session-x.log" 2>&1 || say "session X exited with $?"
cp "$W/zone/logs/hook.log" "$RUN/hook.log" 2>/dev/null || true
grep -E 'STOP|hook' "$L/session-x.log" | tail -n 5 | tee -a "$RUN/run.log" || true
say "done: $(grep -c . "$RUN/attest.jsonl" 2>/dev/null || echo 0) records in attest.jsonl"
