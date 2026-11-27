#!/usr/bin/env bash
# The bridge round trip of CI's `sequencer-roundtrip` job (scripts/ci/sequencer-roundtrip.sh and its hook: the user's
# deposit, claim, burn and release through the `ferrata` tool, the operator's steps through crown-driver) against LEZ
# v0.4.0-rc3's standalone sequencer, the LEZ of our public test network, and anvil forking Sepolia (docs/VERIFY.md §5's
# rehearsal). On a copy of this tree, so the manifests switched to rc3 there are never this tree's.
#
#   e2e/rc3-roundtrip/run.sh NAME          records in e2e/runs/NAME/ (copied back from the copy)
#
# What differs from CI's job, exactly:
# - the sequencer (`sequencer_service`, `standalone`), crown-driver and the tool (`ferrata`, `ferrata-relayer`,
#   `seqnet-fund`, `seqnet-txs`, features `seqnet,lez-rc3`) are built against LEZ v0.4.0-rc3: lez/scripts/lez-select.sh
#   v0.4.0-rc3 for lez/, and for cli/ what deploy/testnet/cli-rc3/build.sh does (its LEZ paths, cli-rc3.patch,
#   Cargo.lock.v0.4.0-rc3);
# - the guests (the attestation and bridge programs, the release and rule (B) checkpoint guests) and ci_zone are built
#   from the committed manifests first, as CI builds them (the image ids of docs/image-ids.md);
# - then scripts/ci/sequencer-roundtrip.sh's stages run, check and summary, with FERRATA_CI_LEZ_TAG=v0.4.0-rc3.
# The stand-ins are CI's, listed in scripts/ci/sequencer-roundtrip.sh. Nothing is sent to any public network: anvil
# keeps every Ethereum transaction and the sequencer is local. Heavy: run it on a build machine, not a laptop.
# Needs what scripts/ci/sequencer-roundtrip.sh's build stage needs, and LEZ v0.4.0-rc3 beside the repository.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
NAME="${1:?usage: e2e/rc3-roundtrip/run.sh NAME (records in e2e/runs/NAME/)}"
case "$NAME" in *[!A-Za-z0-9._-]*|'') echo "rc3-roundtrip: bad NAME" >&2; exit 2 ;; esac
T="${FERRATA_RC3_RT_TMP:-$BESIDE/tmp/rc3-roundtrip}"
SRC="$T/src"
TOOLS="$T/ci/tools"
die() { echo "rc3-roundtrip: $*" >&2; exit 1; }
say() { echo "[rc3-roundtrip $(date -u +%T)] $*"; }
[ -f "$BESIDE/upstream/lez-v0.4.0-rc1/Cargo.toml" ] || die "LEZ v0.4.0-rc1 is not beside the repository (scripts/fetch-upstream.sh)"
[ -f "$BESIDE/upstream/lez-v0.4.0-rc3/Cargo.toml" ] || die "LEZ v0.4.0-rc3 is not beside the repository (scripts/fetch-upstream.sh)"
mkdir -p "$T" "$TOOLS"
LOG="$T/build.log"

# ---- the copy ----------------------------------------------------------------------------------------------------
if [ ! -f "$SRC/.copied" ]; then
  say "copying the tree to $SRC"
  rm -rf "$SRC" && mkdir -p "$SRC"
  tar -C "$REPO" --exclude=./target --exclude='./*/target' --exclude='./*/*/target' --exclude=./.scratch \
    --exclude=./e2e/runs -cf - . | tar -C "$SRC" -xf -
  mkdir -p "$SRC/e2e/runs"
  touch "$SRC/.copied"
fi
# The copy's "beside" is $T: upstream/ there links to the real one.
ln -sfn "$BESIDE/upstream" "$T/upstream"

lez_home() {
  local d
  if [ -n "${RISC0_HOME_LEZ:-}" ]; then echo "$RISC0_HOME_LEZ"; return; fi
  for d in "$BESIDE/risc0-home-lez" "$HOME/risc0-home-lez"; do
    if [ -d "$d" ]; then (cd "$d" && pwd); return; fi
  done
  echo "$HOME/.risc0"
}
LH="$(lez_home)"
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$HOME/.risc0/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
unset RISC0_DEV_MODE

# ---- the tag-independent tools, from the committed (v0.4.0-rc1) manifests -----------------------------------------
cd "$SRC"
[ "$(bash lez/scripts/lez-select.sh --show)" = v0.4.0-rc1 ] || bash lez/scripts/lez-select.sh v0.4.0-rc1 >> "$LOG" 2>&1
if [ ! -f "$TOOLS/.guests" ]; then
  say "vault libraries and build"
  ( cd vault && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 \
      OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.1.0; } && forge build ) >> "$LOG" 2>&1 \
    || die "vault build (see $LOG)"
  say "bridge: the release guest and ferrata-e2e"
  ( cd bridge && env -u RISC0_HOME cargo build --release --locked -p methods && env -u RISC0_HOME cargo build --release \
      --locked -p ferrata-e2e ) >> "$LOG" 2>&1 || die "bridge build (see $LOG)"
  say "checkpoint: the rule (B) guest and ci_zone"
  ( cd checkpoint && env -u RISC0_HOME cargo build --release --locked -p methods ) >> "$LOG" 2>&1 || die "checkpoint guest"
  eval "$(python3 -I scripts/fetch_native.py env 2>> "$LOG")"
  ( cd checkpoint/fidelity && cargo +1.98.1 build --release --locked --bin ci_zone ) >> "$LOG" 2>&1 || die "ci_zone"
  say "LEZ guests: the attestation and bridge programs (the committed manifests)"
  ( cd lez && RISC0_HOME="$LH" RISC0_BUILD_LOCKED=1 RUSTC_WRAPPER="$SRC/scripts/guest-rustc" \
      cargo build -p methods --locked ) >> "$LOG" 2>&1 || die "LEZ guests (see $LOG)"
  find_one() { find "$1" -path '*riscv32im-risc0-zkvm-elf/release/*' -name "$2" -print -quit 2>/dev/null; }
  for f in "bridge/target/riscv-guest:release.bin" "checkpoint/target/riscv-guest:checkpoint_b.bin" \
           "lez/target/riscv-guest:ferrata_lez.bin" "lez/target/riscv-guest:ferrata_bridge.bin" \
           "lez/target/riscv-guest:scripted.bin"; do
    got="$(find_one "${f%%:*}" "${f#*:}")"
    [ -n "$got" ] || die "the guest ${f#*:} was not built under ${f%%:*}"
    cp "$got" "$TOOLS/${f#*:}"
  done
  cp checkpoint/target/release/ci_zone bridge/target/release/ferrata-e2e "$TOOLS/"
  touch "$TOOLS/.guests"
fi

# ---- the LEZ-bound tools against v0.4.0-rc3 -----------------------------------------------------------------------
if [ ! -f "$TOOLS/.rc3" ]; then
  say "switching the copy to LEZ v0.4.0-rc3"
  bash lez/scripts/lez-select.sh v0.4.0-rc3 >> "$LOG" 2>&1 || die "lez-select v0.4.0-rc3"
  # every manifest of the copy, as deploy/testnet/cli-rc3/build.sh does (cli's path dependencies reach beyond cli/)
  { grep -rl --include=Cargo.toml --exclude-dir=target 'upstream/lez-v0.4.0-rc1/' . || true; } \
    | xargs -r sed -i 's#upstream/lez-v0.4.0-rc1/#upstream/lez-v0.4.0-rc3/#'
  patch -p1 -R --dry-run -s < deploy/testnet/cli-rc3/cli-rc3.patch > /dev/null 2>&1 || patch -p1 < deploy/testnet/cli-rc3/cli-rc3.patch >> "$LOG"
  install -m 644 deploy/testnet/cli-rc3/Cargo.lock.v0.4.0-rc3 cli/Cargo.lock
  TGT="$T/ci/target"
  say "LEZ v0.4.0-rc3's sequencer_service (standalone)"
  LEZ_TAG=v0.4.0-rc3 FERRATA_LIVE_TMP="$T/ci" bash lez/live/run.sh build-standalone >> "$LOG" 2>&1 || die "sequencer (see $LOG)"
  say "crown-driver (rc3)"
  ( cd lez/crown/driver && CARGO_TARGET_DIR="$TGT" cargo build --release --locked ) >> "$LOG" 2>&1 || die "crown-driver (see $LOG)"
  say "cli: ferrata, ferrata-relayer, seqnet-fund, seqnet-txs (features seqnet, lez-rc3)"
  ( cd cli && RISC0_HOME="$LH" CARGO_TARGET_DIR="$TGT" cargo build --release --locked --features seqnet,lez-rc3 \
      --bin ferrata --bin ferrata-relayer --bin seqnet-fund --bin seqnet-txs ) >> "$LOG" 2>&1 || die "cli (see $LOG)"
  for f in ferrata ferrata-relayer seqnet-fund seqnet-txs crown-driver; do cp "$TGT/release/$f" "$TOOLS/"; done
  touch "$TOOLS/.rc3"
fi
( cd "$TOOLS" && sha256sum ./* ) > "$T/tools.sha256"

# ---- the round trip -----------------------------------------------------------------------------------------------
say "the round trip: scripts/ci/sequencer-roundtrip.sh run check summary, FERRATA_CI_LEZ_TAG=v0.4.0-rc3"
rc=0
# FERRATA_RC3_RT_HOOK (a hook under this tree, e2e/f13/hook.sh) runs that hook instead of CI's, with the stages of
# FERRATA_RC3_RT_STAGES (default: run check summary; another hook's records need `run summary`, CI's check being CI's).
HOOK_ARG=()
if [ -n "${FERRATA_RC3_RT_HOOK:-}" ]; then
  [ -f "$SRC/$FERRATA_RC3_RT_HOOK" ] || die "no hook $FERRATA_RC3_RT_HOOK in the copy"
  HOOK_ARG=(FERRATA_CI_RT_HOOK="$SRC/$FERRATA_RC3_RT_HOOK")
fi
read -r -a STAGES <<< "${FERRATA_RC3_RT_STAGES:-run check summary}"
env "${HOOK_ARG[@]}" FERRATA_CI_LEZ_TAG=v0.4.0-rc3 FERRATA_CI_ROUNDTRIP_TMP="$T/ci" RISC0_HOME_LEZ="$LH" \
  bash scripts/ci/sequencer-roundtrip.sh "e2e/runs/$NAME" "${STAGES[@]}" || rc=$?
mkdir -p "$REPO/e2e/runs"
rm -rf "$REPO/e2e/runs/$NAME" && cp -r "e2e/runs/$NAME" "$REPO/e2e/runs/$NAME"
cp "$T/tools.sha256" "$REPO/e2e/runs/$NAME/tools.sha256"
say "records in e2e/runs/$NAME (exit $rc)"
exit "$rc"
