#!/usr/bin/env bash
# bridge-next box job 2 (run from the worktree root by mrun): every guest rebuilt in FRESH target directories (the
# ones job 1 filled are moved aside first, restored at the end) with scripts/reproduce-image-ids.sh build, compared
# with docs/image-ids.md, e2e/crown/README.md and the live Sepolia vault's immutables; then the second build in the same
# fresh lez/target through the same wrapper, all 15 listed and compared; then the tenant test against the second build.
set -uo pipefail
W="$PWD"; SP="$(cd "$W/.." && pwd)"; B="$SP/tmp/bridge-next/box/job2"; mkdir -p "$B"; : > "$B/status.txt"
R="$B/repro"
step() { local name="$1"; shift; local t0; t0=$(date +%s)
  ( "$@" ) > "$B/$name.log" 2>&1; local rc=$?
  echo "$name exit $rc $(( $(date +%s) - t0 )) s $(date -u +%T)" | tee -a "$B/status.txt"; return $rc; }
cd "$W"
# Outside the worktree: a moved target directory inside it would not be ignored by git, and mrun would copy it back.
X="$SP/tmp/bridge-next/box-targets"; mkdir -p "$X"; rm -rf "$X"/*.repro
for d in lez checkpoint bridge; do [ -e "$d/target" ] && mv "$d/target" "$X/$d.job1"; done
{ date -u +%FT%TZ; for d in lez checkpoint bridge; do if [ -e "$d/target" ]; then echo "$d/target EXISTS"; else echo "$d/target absent"; fi; done
  ls "$X"; } > "$B/fresh.txt"
rm -rf "$R"
export RISC0_HOME_LEZ="$SP/risc0-home-lez" CARGO_BUILD_JOBS=16 CARGO_TERM_COLOR=never
unset RISC0_HOME CARGO_TARGET_DIR RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
step repro bash scripts/reproduce-image-ids.sh build --out "$R"
step compare bash scripts/reproduce-image-ids.sh compare --out "$R" --vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c
step next bash -c 'cd lez && RUSTC_WRAPPER="$PWD/../scripts/guest-rustc" RISC0_BUILD_LOCKED=1 RISC0_HOME="$RISC0_HOME_LEZ" cargo build -p methods-bridge-next --locked'
step ids15 python3 -I scripts/reproduce/guest_ids.py "$W" "$B/ids-with-next.tsv"
mkdir -p "$B/repro15/r0bf"
cp "$R/ids.txt" "$R/methods-ids.txt" "$B/repro15/"; cp "$R/r0bf/ids.jsonl" "$B/repro15/r0bf/"; cp "$B/ids-with-next.tsv" "$B/repro15/ids.tsv"
step compare15 bash scripts/reproduce-image-ids.sh compare --out "$B/repro15"
{ echo "guest binaries newer than fresh.txt (written before the build):"
  find lez/target checkpoint/target bridge/target -path '*riscv*' -type f \( -name '*.bin' -o -name 'ferrata_*' -o -name 'checkpoint*' -o -name segment -o -name release \) -newer "$B/fresh.txt" 2>/dev/null | grep -v '/deps/\|/build/\|\.d$' | sort
  echo; echo "Compiling lines per log:"; for f in "$R"/logs/*-methods.log "$B/next.log"; do echo "$(grep -c 'Compiling' "$f") $f"; done
  echo; grep -h 'Compiling ferrata_bridge\|Compiling ferrata_lez \|Compiling ferrata-lez-bridge-wire' "$R/logs/lez-methods.log" "$B/next.log" | head -8
} > "$B/rebuild-evidence.txt" 2>&1
for d in lez checkpoint bridge; do mv "$d/target" "$X/$d.repro"; [ -e "$X/$d.job1" ] && mv "$X/$d.job1" "$d/target"; done
export RISC0_DEV_MODE=1 RISC0_HOME="$SP/risc0-home-lez"
step tenant-next bash -c 'cd lez && cargo test -p harness --locked --features bridge-next --test tenant -- --nocapture --test-threads 1'
# The deployed program's suites again (their call sites now go through holder_of / nullifier_of): their records must
# come out unchanged (mrun copies back only changed files).
step suites-default bash -c 'cd lez && cargo test -p harness --locked --test bridge_inbound --test bridge_private \
  --test bridge_burn --test bridge_controls --test bridge_vault_storage --test bridge_roles --test bridge_secfix \
  --test bridge_time -- --nocapture'
echo "ALL DONE $(date -u +%T)" >> "$B/status.txt"
