#!/usr/bin/env bash
# p4-minimum, second pass, on the build box (mrun --profile ws-4), from spike/ferrata-wt-p4/lez:
# build prove_native_transfer (debug, as prove_attest) and ferrata-e2e (release, as the crown runner),
# run both in dev mode / the executor for their cycles, make the release witnesses of the crown
# rehearsal 3 (final programs) and stage everything for minimum.py.
set -u
S=../../tmp/p4-scale            # spike/tmp/p4-scale, beside the repository
OUT=$S/box
rm -rf "$OUT"; mkdir -p "$OUT/stage/transfer" "$OUT/stage/release" "$OUT/dry"
export RISC0_HOME=$PWD/../../risc0-home-lez RISC0_BUILD_LOCKED=1
{
  date -u +%FT%TZ; echo "nproc $(nproc)"; grep -m1 'model name' /proc/cpuinfo; grep MemTotal /proc/meminfo
  rustc --version; cargo --version; r0vm --version; ldd --version | head -1
  R0=$(readlink -f "$(command -v r0vm)"); echo "r0vm $R0"; sha256sum "$R0"
  echo "FERRATA_HEAD=${FERRATA_HEAD:-} FERRATA_DIRTY=${FERRATA_DIRTY:-}"
} > "$OUT/box-env.txt" 2>&1
cat "$OUT/box-env.txt"
# bridge/ sits beside lez/, so $OUT is the same relative path from there
( cd ../bridge && /usr/bin/time -v cargo build --release --locked -p ferrata-e2e > "$OUT/build-e2e.log" 2>&1; echo "e2e build exit $?" > "$OUT/build-e2e.exit" ) &
/usr/bin/time -v cargo build -p harness --locked --bin prove_native_transfer > "$OUT/build-transfer.log" 2>&1
echo "transfer build exit $?"; grep -E '^(error|warning: unused)|Finished|Elapsed \(wall' "$OUT/build-transfer.log" | tail -8
grep -B2 -A12 '^error' "$OUT/build-transfer.log" | head -80
wait; cat "$OUT/build-e2e.exit"; grep -E '^error|Finished|Elapsed \(wall' "$OUT/build-e2e.log" | tail -4
T=target/debug/prove_native_transfer
$T --image-ids
# dev mode: the executor runs every session (r0vm logs its cycles), the receipts are fake
for c in "wallet" "bare" "wallet --dummies 0" "wallet --dummies 3" "bare --recovery yes" "bare --padding 512"; do
  n=$(echo "$c" | tr ' ' '_' | tr -d '-')
  RISC0_DEV_MODE=1 RISC0_INFO=1 RUST_LOG=info $T --allow-dev-mode --shape $c --tx-out "$OUT/dry/$n.borsh" > "$OUT/dry/$n.log" 2>&1
  echo "dry $c exit $?: $(grep -E 'DRY RUN|shape' "$OUT/dry/$n.log" | cut -c1-220 | tr '\n' ' ')"
  grep -E 'number of segments|user cycles|total cycles' "$OUT/dry/$n.log" | sed 's/.*session[^ ]* //' | tr '\n' ' '; echo
done
RISC0_DEV_MODE=1 $T --settle "$OUT/dry/wallet.borsh" > "$OUT/dry/settle-wallet.log" 2>&1; echo "dry settle exit $?: $(grep SETTLED "$OUT/dry/settle-wallet.log" | cut -c1-200)"
# the release witnesses of the crown rehearsal 3 and the release program's executor runs
E=../bridge/target/release/ferrata-e2e
R=../e2e/runs/crown-rehearsal-3
$E image-id "$S/in/release.bin"
$E crown-release --identity $R/identity.json --burns $R/checkpoint/input/burns.json \
  --notes $R/lez/session-b/burn-notes.json --out-dir "$OUT/stage/release/witness" > "$OUT/crown-release.log" 2>&1
echo "crown-release exit $?"; ls "$OUT/stage/release/witness"
for n in public-0 private-1; do
  python3 -I -c 'import json,sys; a=json.load(open(sys.argv[1]))["journal"].removeprefix("0x"); b=json.load(open(sys.argv[2]))["journal"].removeprefix("0x"); print(sys.argv[3], "journal equals the rehearsal'"'"'s:", a==b)' \
    "$OUT/stage/release/witness/release-$n.json" "$R/releases/release-$n.json" "$n"
  RISC0_INFO=1 RUST_LOG=info $E exec --program "$S/in/release.bin" --input "$OUT/stage/release/witness/release-$n.input" \
    --meta "$OUT/stage/release/witness/release-$n.json" > "$OUT/exec-release-$n.log" 2>&1
  echo "exec $n exit $?: $(grep -E '^\{' "$OUT/exec-release-$n.log" | tail -1)"
done
# stage
cp $T "$OUT/stage/transfer/"
cp $E "$S/in/release.bin" "$OUT/stage/release/"
for d in transfer release; do
  python3 -I - "$OUT/stage/$d" <<'PY'
import hashlib, json, os, sys
root = sys.argv[1]
files = {}
for dp, _, fs in os.walk(root):
    for f in sorted(fs):
        p = os.path.join(dp, f); rel = os.path.relpath(p, root)
        if rel == "MANIFEST.json":
            continue
        files[rel] = {"sha256": hashlib.sha256(open(p, "rb").read()).hexdigest(), "bytes": os.path.getsize(p)}
json.dump({"files": files, "ferrata_head": os.environ.get("FERRATA_HEAD"), "ferrata_dirty": os.environ.get("FERRATA_DIRTY")},
          open(os.path.join(root, "MANIFEST.json"), "w"), indent=1)
print(root, len(files), "files")
PY
done
file $T | cut -c1-160; ldd $E | sed 's/ (0x.*//' | tr '\n' ' '; echo
du -sh "$OUT/stage"/*
