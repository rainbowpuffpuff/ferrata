#!/usr/bin/env bash
# rc3-return box job 2a (stage 2's builds), run from the worktree root by mrun (in the foreground of that call, so the
# box is snapshotted after it). As m0-rc3's boxA/boxB: LEZ v0.4.0-rc3 cloned (commit checked); a copy of this tree beside
# the worktree with rc3 selected (lez-select.sh; the worktree stays as committed): rc3's sequencer_service, the live
# driver, crown-driver (rc3) and the 0.3.5-rc.1 node; in the worktree: ferrata_attestor and realblocks-crown (this
# tree's 0.3.5-rc.1 reading), the vault (forge); the programs directory for the zone-bridge stages.
# Steps log to ../tmp/rc3-return/box2a/<step>.log; status.txt.
set -uo pipefail
W="$PWD"; SP="$(cd "$W/.." && pwd)"
S="$SP/tmp/rc3-return"; B="$S/box2a"; C="$SP/ferrata-wt-rc3-return-z"; L="$S/live"
U3="$SP/upstream/lez-v0.4.0-rc3"
mkdir -p "$B"; : > "$B/status.txt"
step() { local n="$1"; shift; local t0; t0=$(date +%s); ( "$@" ) > "$B/$n.log" 2>&1; local rc=$?
  echo "$n exit $rc $(( $(date +%s) - t0 )) s $(date -u +%T)" | tee -a "$B/status.txt"; return $rc; }
export CARGO_TERM_COLOR=never PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
export TMPDIR="$S" XDG_CACHE_HOME="$S/xdg-cache"
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS RISC0_DEV_MODE

clone() {
  [ -d "$U3/.git" ] || git clone -q --filter=blob:none --branch v0.4.0-rc3 \
      https://github.com/logos-blockchain/logos-execution-zone.git "$U3" || return 1
  test "$(git -C "$U3" rev-parse HEAD)" = bafd07be0701398f9bc435f73ab05e9a7936c301 || return 1
  test -z "$(git -C "$U3" status --porcelain)" || return 1
  echo "rc3 at $(git -C "$U3" rev-parse HEAD), clean"
}
copytree() {
  rm -rf "$C"; mkdir -p "$C"
  tar -C "$W" --exclude=./lez/target --exclude=./target --exclude=./.git --exclude=./checkpoint/target --exclude=./bridge/target \
      --exclude=./vault/out --exclude=./vault/cache --exclude=./vault/lib -cf - . | tar -C "$C" -xf - || return 1
  cd "$C/lez" && scripts/lez-select.sh v0.4.0-rc3 && scripts/lez-select.sh --show
}
lv() { ( cd "$C" && LEZ_TAG=v0.4.0-rc3 FERRATA_LIVE_TMP="$L" FERRATA_LIVE_JOBS=16 FERRATA_LIVE_NICE=0 lez/live/run.sh "$@" ); }
crowndrv() {
  cd "$C/lez/crown/driver" && CARGO_TARGET_DIR="$L/target" CARGO_BUILD_JOBS=16 cargo build --release || return 1
  mkdir -p "$L/bin" && cp "$L/target/release/crown-driver" "$L/bin/" && sha256sum "$L/bin/crown-driver"
}
tools() {
  cd "$W/checkpoint" && cargo build --release --locked -p realblocks --bin realblocks-crown && cargo build --release --locked -p ferrata-attestor || return 1
  cargo +1.88 test -p checkpoint-core --features bedrock-0-3-5 2>&1 | grep -E '^(test |test result|     Running)'; [ "${PIPESTATUS[0]}" = 0 ] || return 1
  cargo test --release --locked -p ferrata-attestor 2>&1 | grep -E '^(test result|     Running)'; [ "${PIPESTATUS[0]}" = 0 ] || return 1
  sha256sum target/release/realblocks-crown target/release/ferrata_attestor
}
vault() {
  cd "$W/vault" || return 1
  [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 \
    risc0/risc0-ethereum@v3.0.0 || return 1
  forge build > /dev/null && ls out | wc -l
}
programs() { # the zone-bridge stages' programs: the four R0BF of the reproduce build and the second build's two
  local P="$S/programs-rc3"; rm -rf "$P"; mkdir -p "$P"
  cp "$S"/repro/r0bf/*.bin "$P/" && cp "$S"/r0bf-035/*.bin "$P/" || return 1
  cat "$S/repro/r0bf/ids.jsonl" "$S/r0bf-035/ids.jsonl" > "$P/ids.jsonl"
  cp "$S/repro/programs/ferrata_lez.bin" "$S/repro/programs/ferrata_bridge.bin" "$P/" || return 1
  local f; f="$(find "$W/lez/target/riscv-guest" -path '*release*' -name scripted.bin | head -n 1)"; [ -n "$f" ] && cp "$f" "$P/"
  ( cd "$P" && sha256sum *.bin && for f in ferrata_lez.bin ferrata_bridge.bin scripted.bin; do echo "$f $(r0vm --elf "$f" --id | tail -n 1)"; done )
  cat "$P/ids.jsonl"
}

step clone clone || { echo "ALL DONE (clone failed)" >> "$B/status.txt"; exit 1; }
step copytree copytree || { echo "ALL DONE (copytree failed)" >> "$B/status.txt"; exit 1; }
( step bedrock lv build-bedrock ) & pb=$!
( step tools tools ) & pt=$!
step sequencer lv build-sequencer
step driver lv build-driver
step crowndrv crowndrv
step vault vault
wait $pb $pt
step programs programs
( cd "$L" && sha256sum bin/* bedrock/.resolved-host/logos-blockchain-node 2>/dev/null; cat bedrock/.resolved-host/metadata.json 2>/dev/null ) > "$B/facts.log"
echo "ALL DONE $(date -u +%T)" | tee -a "$B/status.txt"
