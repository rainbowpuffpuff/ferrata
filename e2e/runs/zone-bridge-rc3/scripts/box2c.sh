#!/usr/bin/env bash
# rc3-return box job 2c: job 2b's steps after the session again (input, oldguest, burnroot), after the collector's fix
# (a refusal of the second build is Core(..)-wrapped); the attestor rebuilt first. Run from the worktree root by mrun.
set -uo pipefail
W="$PWD"; SP="$(cd "$W/.." && pwd)"
S="$SP/tmp/rc3-return"; B="$S/box2c"; P="$S/programs-rc3"
NAME="${RL_NAME:-zone-bridge-rc3}"; RUN="$W/e2e/runs/$NAME"
export ZB_TMP="$S/zb"
mkdir -p "$B"; : > "$B/status.txt"
step() { local n="$1"; shift; local t0; t0=$(date +%s); ( "$@" ) > "$B/$n.log" 2>&1; local rc=$?
  echo "$n exit $rc $(( $(date +%s) - t0 )) s $(date -u +%T)" | tee -a "$B/status.txt"; return $rc; }
export CARGO_TERM_COLOR=never PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS RISC0_DEV_MODE
attestor() { cd "$W/checkpoint" && cargo build --release --locked -p ferrata-attestor && sha256sum target/release/ferrata_attestor; }
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
step attestor attestor || { echo "ALL DONE (attestor build failed)" | tee -a "$B/status.txt"; exit 1; }
step input input
step oldguest oldguest
step burnroot burnroot
for n in attestor input oldguest burnroot; do sed -e "s#$SP/#spike/#g" -e "s#$HOME/#~/#g" "$B/$n.log" > "$RUN/logs/box-$n.log" 2>/dev/null; done
grep -rIl -e "$SP/" -e "$HOME/" -e "/root/" "$RUN" 2>/dev/null && echo "MACHINE PATHS FOUND (listed above)" | tee -a "$B/status.txt"
echo "ALL DONE $(date -u +%T)" | tee -a "$B/status.txt"
