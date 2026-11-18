#!/usr/bin/env bash
# rc3-return box job 3 (stage 3, the proofs), run from the worktree root by mrun after job 2b: zone-bridge.sh's `prove`
# (checkpoint_b_0_3_5's succinct proof on the box's CPU, identity_p254) and `releases` (crown-release, the two release
# proofs, identity_p254 each) on the run; the Groth16 step then runs on the laptop's side (mwrap).
set -uo pipefail
W="$PWD"; SP="$(cd "$W/.." && pwd)"; S="$SP/tmp/rc3-return"; B="$S/box3"; P="$S/programs-rc3"
NAME="${RL_NAME:-zone-bridge-rc3}"; RUN="$W/e2e/runs/$NAME"
export ZB_TMP="$S/zb" ZB_PROGRAMS="$P" ZB_CHECKPOINT_B=checkpoint_b_0_3_5 ZB_N_MIN=1 ZB_T_MIN=1 ZB_LEAVE_OUT=none
export CARGO_TERM_COLOR=never PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH"; unset RISC0_DEV_MODE
mkdir -p "$B"; : > "$B/status.txt"
step() { local n="$1"; shift; local t0; t0=$(date +%s); ( "$@" ) > "$B/$n.log" 2>&1; local rc=$?
  echo "$n exit $rc $(( $(date +%s) - t0 )) s $(date -u +%T)" | tee -a "$B/status.txt"; return $rc; }
echo "start $(date -u +%FT%T) nproc $(nproc) processors $(grep -c ^processor /proc/cpuinfo); $(grep -m1 'model name' /proc/cpuinfo)" >> "$B/status.txt"
step prove bash checkpoint/attestor/zone-bridge.sh prove "$RUN"
step releases bash checkpoint/attestor/zone-bridge.sh releases "$RUN"
for n in prove releases; do sed -e "s#$SP/#spike/#g" -e "s#$HOME/#~/#g" "$B/$n.log" > "$RUN/logs/box-$n.log"; done
cp "$B/status.txt" "$RUN/logs/box3-status.txt"
ls -la "$RUN/work"/*/input.json
echo "ALL DONE $(date -u +%T)" | tee -a "$B/status.txt"
