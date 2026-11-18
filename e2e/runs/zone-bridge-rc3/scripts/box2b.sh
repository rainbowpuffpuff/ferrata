#!/usr/bin/env bash
# rc3-return box job 2b (stage 2's session), started detached by mrun from the worktree root, pinged by keepalive.sh.
# One zone session on LEZ v0.4.0-rc3's sequencer_service and its Logos Blockchain 0.3.5-rc.1 node (job 2a's builds, in
# the rc3 copy of the tree): run.sh phases `info hook export` with the hook lez/live/hooks/return-leg.sh (crown-a with the
# fork digest, the anvil vault with RISC Zero's verifier and the 0.3.5 ids, deposits, the attestor, crown-b with real
# private proofs, the final round, the node's answers); then zone-bridge.sh's `input` stage on the run (realblocks-crown
# in the 0.3.5-rc.1 layout, the collector, the executor on checkpoint_b_0_3_5) and crown-release's burn-root check.
# Records: e2e/runs/$RL_NAME (the run) and lez/results/v0.4.0-rc3/return-leg/zone (the zone's own records), in the worktree.
# Steps log to ../tmp/rc3-return/box2b/<step>.log; status.txt; "ALL DONE" ends it.
set -uo pipefail
W="$PWD"; SP="$(cd "$W/.." && pwd)"
S="$SP/tmp/rc3-return"; B="$S/box2b"; C="$SP/ferrata-wt-rc3-return-z"; L="$S/live"; P="$S/programs-rc3"
NAME="${RL_NAME:-zone-bridge-rc3}"; RUN="$W/e2e/runs/$NAME"; ZOUT="$W/lez/results/v0.4.0-rc3/return-leg/zone"
export ZB_TMP="$S/zb"
mkdir -p "$B" "$ZB_TMP"; : > "$B/status.txt"
step() { local n="$1"; shift; local t0; t0=$(date +%s); ( "$@" ) > "$B/$n.log" 2>&1; local rc=$?
  echo "$n exit $rc $(( $(date +%s) - t0 )) s $(date -u +%T)" | tee -a "$B/status.txt"; return $rc; }
echo "start $(date -u +%FT%T) nproc $(nproc) processors $(grep -c ^processor /proc/cpuinfo)" >> "$B/status.txt"
export CARGO_TERM_COLOR=never PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS RISC0_DEV_MODE

U3="$SP/upstream/lez-v0.4.0-rc3"
clone() { # a new container keeps only the snapshot (targets, scratch): LEZ v0.4.0-rc3 again, and the rc3 copy of the tree
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
  ls -la "$L/bin" "$L/bedrock/.resolved-host" && cat "$L/bin/sequencer_service.sha256"
}
session() {
  rm -rf "$RUN" "$ZOUT"
  cd "$C" && LEZ_TAG=v0.4.0-rc3 FERRATA_LIVE_TMP="$L" FERRATA_LIVE_JOBS=16 FERRATA_LIVE_NICE=0 \
    FERRATA_PROGRAM_BIN="$P/ferrata_lez.bin" FERRATA_SCRIPTED_BIN="$P/scripted.bin" \
    FERRATA_LIVE_HARNESS_RESULTS="$C/lez/results/v0.4.0-rc3" FERRATA_LIVE_TICK_AFTER=1 FERRATA_LIVE_TICK_WINDOW="1 2" \
    FERRATA_LIVE_PROOF_THREADS=32 FERRATA_LIVE_OUT="$ZOUT" \
    FERRATA_LIVE_HOOK="$W/lez/live/hooks/return-leg.sh" RL_WORKTREE="$W" RL_RUN="$RUN" RL_BRIDGE_BIN="$P/ferrata_bridge.bin" \
    RL_PROGRAMS="$P" RL_DRIVER="$L/bin/crown-driver" RL_PROOF_THREADS=32 \
    FERRATA_LIVE_PHASES="info hook export" lez/live/run.sh run
}
input() {
  cd "$W" && ZB_PROGRAMS="$P" ZB_CHECKPOINT_B=checkpoint_b_0_3_5 ZB_N_MIN=1 ZB_T_MIN=1 ZB_LEAVE_OUT=none \
    bash checkpoint/attestor/zone-bridge.sh input "$RUN"
}
oldguest() { # the deployed checkpoint_b (d480efc6 layout) on this zone's input: refused in the executor
  local n; n="$(cd "$RUN/checkpoint" && ls zone-*.input | grep -v -e '-all' -e '-below' | head -n 1)"
  jq -n '{kind: "checkpoint-b", journal: "00"}' > "$S/old-meta.json"
  if "$W/bridge/target/release/ferrata-e2e" exec --program "$P/checkpoint_b.bin" --input "$RUN/checkpoint/$n" --meta "$S/old-meta.json" \
       > "$RUN/checkpoint/exec-deployed-checkpoint_b.txt" 2>&1; then echo "ACCEPTED (unexpected)"; return 1; fi
  grep -o 'checkpoint refused: [^"]*' "$RUN/checkpoint/exec-deployed-checkpoint_b.txt" | head -n 1
}
burnroot() { # the bridge core's burn tree over the bridge program's two burns (crown-release), equal to the journal's
  mkdir -p "$S/burnroot" && cd "$W" && "$W/bridge/target/release/ferrata-e2e" crown-release --identity "$RUN/identity.json" \
    --burns "$RUN/checkpoint/burns.json" --notes "$RUN/lez/session-b/burn-notes.json" --out-dir "$S/burnroot"
}
step clone clone || { echo "ALL DONE (clone failed)" >> "$B/status.txt"; exit 1; }
step copytree copytree || { echo "ALL DONE (copytree failed)" >> "$B/status.txt"; exit 1; }
step session session
step input input
step oldguest oldguest
step burnroot burnroot
( cd "$C" && LEZ_TAG=v0.4.0-rc3 FERRATA_LIVE_TMP="$L" lez/live/run.sh stop ) > /dev/null 2>&1
pkill -x anvil 2>/dev/null || true
rm -rf "$C/lez/live/__pycache__" "$W/lez/live/__pycache__"
for n in clone copytree session input oldguest burnroot; do sed -e "s#$SP/#spike/#g" -e "s#$HOME/#~/#g" "$B/$n.log" > "$RUN/logs/box-$n.log" 2>/dev/null; done
grep -rIl -e "$SP/" -e "$HOME/" -e "/root/" "$RUN" "$ZOUT" 2>/dev/null && echo "MACHINE PATHS FOUND (listed above)" >> "$B/status.txt"
echo "ALL DONE $(date -u +%T)" >> "$B/status.txt"
