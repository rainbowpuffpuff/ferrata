#!/usr/bin/env bash
# RFP-021 P.4 and RFP-022 P.4 on a container sized like a mid-range laptop (8 vCPU, 16 GiB), this tree's images:
# step 1, on the build box (mrun), from lez/: build prove_attest and prove_bridge, their dev-mode dry runs (cycles,
# the transactions settle), strip and stage them for lez/results/p4-minimum/minimum.py (jobs attest-pad, bridge-pad),
# with r0vm 3.0.5 beside them. As lez/results/p4-minimum/box-build-pad.sh, without the padding test and the
# yardstick. Output: ../tmp/perf-p4 beside the repository, or P4_BUILD_DIR (relative to lez/) (bring it to the laptop
# with mrun --get); stage/ is the owner's bundle for bench/perf/p4-laptop-owner.sh (attest-pad/, bridge-pad/,
# r0vm/r0vm and its SHA256SUMS). Steps 2 and 3 (from the laptop): docs in lez/results/perf-p4-laptop.md.
#
# With P4_RELEASE_BIN=<release_31's R0BF binary> (a path outside P4_BUILD_DIR, which this script empties) the bundle
# also holds release/: RFP-022's third user-device proof, the release's succinct proof (the holder's: its input holds
# the burn secret; the Groth16 wrap is any relayer's). release/ has ferrata-e2e (bridge/, RISC Zero 3.1's client,
# --no-default-features: no wrap), the binary (its sha256 and image id must equal docs/image-ids.md's release_31 row),
# the crown rehearsal 3's burn private-1 as witness (ferrata-e2e crown-release, as lez/results/p4-minimum/box-build-scale.sh)
# and r0vm 3.1.0 (rzup), each checked here by the executor and one real proof whose receipt is verified.
set -u
S=${P4_BUILD_DIR:-../../tmp/perf-p4}
rm -rf "$S"; mkdir -p "$S/stripped" "$S/dry" "$S/stage/r0vm"
export RISC0_HOME=$PWD/../../risc0-home-lez RISC0_BUILD_LOCKED=1
R0=$(readlink -f "$(command -v r0vm)")
{
  date -u +%FT%TZ; echo "nproc --all $(nproc --all)"; grep -m1 'model name' /proc/cpuinfo; grep MemTotal /proc/meminfo
  rustc --version; cargo --version; r0vm --version; ldd --version | head -1; echo "r0vm $R0"; sha256sum "$R0"
  echo "FERRATA_HEAD=${FERRATA_HEAD:-} FERRATA_DIRTY=${FERRATA_DIRTY:-}"
} > "$S/box-env.txt" 2>&1
cargo build -p harness --locked --bin prove_attest --bin prove_bridge > "$S/build.log" 2>&1
echo "build exit $?"; grep -E '^error|Finished' "$S/build.log" | tail -3
for b in prove_attest prove_bridge; do target/debug/$b --image-ids > "$S/$b-ids.txt" 2>&1; echo "$b: $(head -c 400 "$S/$b-ids.txt" | tr '\n' ' ')"; done
RISC0_DEV_MODE=1 RISC0_INFO=1 RUST_LOG=info target/debug/prove_attest --allow-dev-mode --shape wallet \
  --tx-out "$S/dry/attest.borsh" > "$S/dry/attest.log" 2>&1
echo "attest dry exit $?: $(grep -E 'DRY RUN' "$S/dry/attest.log" | cut -c1-200)"
RISC0_DEV_MODE=1 RISC0_INFO=1 RUST_LOG=info target/debug/prove_bridge --allow-dev-mode --shape wallet \
  --claim-out "$S/dry/claim.borsh" --burn-out "$S/dry/burn.borsh" > "$S/dry/bridge.log" 2>&1
echo "bridge dry exit $?: $(grep -E 'DRY RUN' "$S/dry/bridge.log" | cut -c1-200 | tr '\n' ' ')"
for b in prove_attest prove_bridge; do strip -o "$S/stripped/$b" target/debug/$b; done
scripts/stage-prove-attest.sh "$S/stage/attest-pad" "$S/stripped/prove_attest" > "$S/stage-attest.out" 2>&1; echo "stage attest exit $?"
scripts/stage-prove-bridge.sh "$S/stage/bridge-pad" "$S/stripped/prove_bridge" > "$S/stage-bridge.out" 2>&1; echo "stage bridge exit $?"
cp "$R0" "$S/stage/r0vm/r0vm"; (cd "$S/stage/r0vm" && sha256sum r0vm > SHA256SUMS)
# The staging scripts write the GPU jobs' command; this bundle is proven on a CPU, padded as the wallet pads.
python3 -I ../bench/perf/p4_manifest.py "$S/stage"
gzip -f "$S/dry/attest.log" "$S/dry/bridge.log"

if [ -n "${P4_RELEASE_BIN:-}" ]; then
  RS=$S/stage/release
  mkdir -p "$RS/witness" "$S/release"
  # release_31's row of docs/image-ids.md: image id and the binary's sha256
  read -r WANT_ID WANT_SHA < <(python3 -I -c '
import sys
for l in open(sys.argv[1]):
    if l.startswith("| release (`bridge/guests`): `release_31` |"):
        c = [x.strip().strip("`") for x in l.split("|")]
        print(c[2], c[3]); break' ../docs/image-ids.md)
  echo "docs/image-ids.md release_31: $WANT_ID $WANT_SHA"
  R31=$S.risc0-31   # beside S (kept between runs, not part of what is brought back)
  R0_31=$(find "$R31" -maxdepth 4 -type f -name r0vm 2> /dev/null | head -1)
  if [ -z "$R0_31" ]; then
    RISC0_HOME=$R31 rzup install r0vm 3.1.0 > "$S/release/rzup.log" 2>&1; echo "rzup r0vm 3.1.0 exit $?"
    R0_31=$(find "$R31" -maxdepth 4 -type f -name r0vm | head -1)
  fi
  echo "r0vm 3.1.0: $("$R0_31" --version) $(sha256sum "$R0_31" | cut -c1-64)"
  ( cd ../bridge && cargo build --release --locked -p ferrata-e2e --no-default-features ) > "$S/release/build-e2e.log" 2>&1
  echo "e2e build exit $?"; grep -E '^error|Finished' "$S/release/build-e2e.log" | tail -3
  strip -o "$RS/ferrata-e2e" ../bridge/target/release/ferrata-e2e
  cp "$P4_RELEASE_BIN" "$RS/release.bin"
  cp "$R0_31" "$RS/r0vm"
  E=$RS/ferrata-e2e
  GOT=$("$E" image-id "$RS/release.bin")
  echo "image-id: $GOT"
  python3 -I -c 'import json, sys; r = json.loads(sys.argv[1]); ok = r["image_id"] == sys.argv[2] and r["sha256"] == sys.argv[3]
print("release.bin equals docs/image-ids.md release_31 (image id and sha256):", ok); sys.exit(0 if ok else 1)' "$GOT" "$WANT_ID" "$WANT_SHA"
  echo "release.bin check exit $?"
  R=../e2e/runs/crown-rehearsal-3
  "$E" crown-release --identity $R/identity.json --burns $R/checkpoint/input/burns.json \
    --notes $R/lez/session-b/burn-notes.json --out-dir "$S/release/witness" > "$S/release/crown-release.log" 2>&1
  echo "crown-release exit $?"
  cp "$S/release/witness/release-private-1.input" "$S/release/witness/release-private-1.json" "$RS/witness/"
  IN=$RS/witness/release-private-1.input META=$RS/witness/release-private-1.json
  RISC0_SERVER_PATH=$RS/r0vm RISC0_INFO=1 "$E" exec --program "$RS/release.bin" --input "$IN" --meta "$META" \
    > "$S/release/exec.log" 2>&1
  echo "exec exit $?: $(grep -E '^\{' "$S/release/exec.log" | tail -1)"
  # one real proof with the staged files, its receipt verified (the laptop repeats exactly this)
  RISC0_DEV_MODE=0 RISC0_SERVER_PATH=$RS/r0vm RISC0_INFO=1 RUST_LOG=info /usr/bin/time -v "$E" prove \
    --program "$RS/release.bin" --input "$IN" --meta "$META" --out-dir "$S/release/prove" > "$S/release/prove.log" 2>&1
  echo "prove exit $?: $(grep -E '^\{' "$S/release/prove.log" | tail -1 | cut -c1-400)"
  grep -E 'Maximum resident|Elapsed \(wall' "$S/release/prove.log"
  RISC0_DEV_MODE=0 RISC0_SERVER_PATH=$RS/r0vm "$E" verify --receipt "$S/release/prove/receipt-succinct.bin" \
    --image-id "$WANT_ID" --meta "$META" > "$S/release/verify.log" 2>&1
  echo "verify exit $?: $(tail -1 "$S/release/verify.log" | cut -c1-300)"
  python3 -I - "$RS" "$WANT_ID" "$("$RS/r0vm" --version)" <<'PY'
import hashlib, json, os, sys
root, image_id, r0vm_version = sys.argv[1:4]
files = {}
for dp, _, fs in os.walk(root):
    for f in sorted(fs):
        p = os.path.join(dp, f); rel = os.path.relpath(p, root)
        if rel != "MANIFEST.json":
            files[rel] = {"sha256": hashlib.sha256(open(p, "rb").read()).hexdigest(), "bytes": os.path.getsize(p)}
json.dump({"job": "release", "image_id": image_id, "program": "release_31 (docs/image-ids.md)", "witness": "private-1",
           "witness_from": "e2e/runs/crown-rehearsal-3 (ferrata-e2e crown-release)", "r0vm_version": r0vm_version,
           "prove_command": "RISC0_DEV_MODE=0 RISC0_SERVER_PATH=<this directory>/r0vm (r0vm 3.1.0, CPU) /usr/bin/time -v "
                            "./ferrata-e2e prove --program release.bin --input witness/release-private-1.input --meta "
                            "witness/release-private-1.json --out-dir prove; then ./ferrata-e2e verify --receipt "
                            "prove/receipt-succinct.bin --image-id <image_id> --meta witness/release-private-1.json "
                            "(bench/perf/p4-laptop-owner.sh)",
           "files": files, "ferrata_head": os.environ.get("FERRATA_HEAD"), "ferrata_dirty": os.environ.get("FERRATA_DIRTY")},
          open(os.path.join(root, "MANIFEST.json"), "w"), indent=1)
print(root, len(files), "files")
PY
  gzip -f "$S/release/prove.log"
fi
du -sh "$S/stage"/*
