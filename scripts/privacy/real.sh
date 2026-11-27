#!/usr/bin/env bash
# The privacy suite with real proofs (proposal B3's gate measured on what is delivered): LEZ's private claims and
# burns proven by r0vm on one GPU and verified by LEZ's settlement; each release statement proven, wrapped to
# Groth16 and checked in the vault by RISC Zero's Groth16 verifier; then the analysis of scripts/privacy/run.sh.
#
#   real.sh build        build box: the test binary, ferrata-e2e and the release program; the release program's id
#                        checked against docs/image-ids.md; staged in target/privacy/real/stage with MANIFEST.json
#   real.sh dev          build box: the staged binary in dev mode (fake receipts), the Foundry half (mock verifier),
#                        the analysis -> $OUT/dev/ (the same programs and code: the baseline)
#   real.sh volume       laptop: the CUDA r0vm (R0VM_CUDA, default gpu-prover/bin/r0vm-cuda; sha256 checked against
#                        gpu-prover/logs/bin-meta/r0vm-cuda.json) into the Volume ferrata-prover of $MODAL_PROFILE
#   real.sh gpu RUN      laptop: scripts/privacy/gpu_app.py on one L4 ($MODAL_PROFILE), fetched to target/privacy/real/RUN
#   real.sh wrap RUN     laptop: each release's Groth16 step (RISC Zero's prover image through e2e/crown/wrap.py,
#                        FERRATA_WRAP_PROFILE, default $MODAL_PROFILE)
#   real.sh finish RUN   build box: the Groth16 receipts assembled and verified, release-seals.json, the Foundry half
#                        with the real verifier, then `analyze`
#   real.sh analyze RUN  build box: the analysis -> $OUT/generated/ and $OUT/proofs.json
#
# OUT: FERRATA_PRIVACY_OUT, default target/privacy/real/b1. The records in docs/privacy/real/ are of the bridge program
# the deployments of November 2026 run (docs/image-ids.md `ferrata_bridge`) and reproduce from f13faee2, where these
# steps wrote them in place; this tree's steps run B1's program (`ferrata_bridge_b1`) and must not write over them.
#
# The build box steps run through spike/modal-build/mrun (with --put target/privacy/real/RUN for `finish`, and
# --env FERRATA_COMMIT=<commit> for `build`: the box's copy is not a git checkout); the
# laptop steps only call Modal. Raw data stays under target/privacy/real/ (not committed).
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
T="$root/target/privacy/real"; S="$T/stage"  # under target/privacy: the vault's fs_permissions
step="${1:?step}"; RUN="${2:-}"
expect_id() {  # the image id docs/image-ids.md records for a program named in its first column: exactly one row
  local ids
  ids="$(grep -F ": \`$1\` |" "$root/docs/image-ids.md" | awk -F'|' '{gsub(/[` ]/, "", $3); print $3}')"
  [ -n "$ids" ] && [ "$(printf '%s\n' "$ids" | wc -l)" = 1 ] \
    || { echo "docs/image-ids.md: not exactly one row for \`$1\`" >&2; return 1; }
  printf '%s\n' "$ids"
}
# This tree's bridge program: since ADR 0048 docs/image-ids.md's `ferrata_bridge_lezfee` (`ferrata_bridge_f13` before it, `ferrata_bridge_b1` before that, the
# test network's; its `ferrata_bridge` row is the program the live runs deployed, at f13faee2).
BRIDGE_ENTRY=ferrata_bridge_lezfee
OUT="${FERRATA_PRIVACY_OUT:-$T/b1}"
E="$S/ferrata-e2e"

case "$step" in
build)
  mkdir -p "$S" "$T/logs"
  export RUSTC_WRAPPER="$root/scripts/guest-rustc" CARGO_TERM_COLOR=never RISC0_BUILD_LOCKED=1
  unset CARGO_TARGET_DIR RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
  ( cd "$root/lez" && RISC0_HOME="${RISC0_HOME_LEZ:-$root/../risc0-home-lez}" \
      cargo test -p harness --locked --test privacy_suite --no-run --message-format=json ) > "$T/logs/build-lez.json" 2> "$T/logs/build-lez.log"
  bin="$(jq -r 'select(.reason == "compiler-artifact" and .target.name == "privacy_suite" and .executable != null) | .executable' "$T/logs/build-lez.json" | tail -n 1)"
  ( cd "$root/bridge" && env -u RISC0_HOME cargo build --release -p methods --locked \
      && env -u RISC0_HOME cargo build --release -p ferrata-e2e --locked ) > "$T/logs/build-bridge.log" 2>&1
  elf="$(find "$root/bridge/target/riscv-guest" -path '*riscv32im-risc0-zkvm-elf/release/*' ! -path '*/release/deps/*' \
          ! -path '*/release/build/*' -name release -type f | head -n 1)"
  cp "$bin" "$S/privacy_suite" && strip --strip-debug "$S/privacy_suite"
  cp "$root/bridge/target/release/ferrata-e2e" "$E" && strip --strip-debug "$E"
  "$E" program-binary "$elf" "$S/release.bin" > "$T/logs/release-id.json"
  got="$(jq -r .image_id "$T/logs/release-id.json")"; want="$(expect_id release)"; bridge="$(expect_id $BRIDGE_ENTRY)"
  echo "release program: $got (docs/image-ids.md: $want); bridge program expected: $bridge"
  [ "$got" = "$want" ] || { echo "the release program's id is not docs/image-ids.md's" >&2; exit 1; }
  python3 -I - "$S" "$want" "$bridge" "${FERRATA_COMMIT:-$(git -C "$root" rev-parse HEAD 2> /dev/null || echo unknown)}" "$root/gpu-prover/logs/bin-meta/r0vm-cuda.json" <<'EOF'
import hashlib, json, os, subprocess, sys
s, release, bridge, commit, meta = sys.argv[1:6]
sha = lambda p: hashlib.sha256(open(p, "rb").read()).hexdigest()
files = {n: {"sha256": sha(os.path.join(s, n)), "bytes": os.path.getsize(os.path.join(s, n))}
         for n in ("privacy_suite", "ferrata-e2e", "release.bin")}
ldd = {n: subprocess.run(["ldd", os.path.join(s, n)], capture_output=True, text=True).stdout.split("\n")
       for n in ("privacy_suite", "ferrata-e2e")}
m = {"commit": commit, "files": files, "expected": {"release": release, "bridge": bridge}, "users": 20,
     "r0vm_cuda_sha256": json.load(open(meta))["sha256"], "ldd": ldd,
     "glibc": subprocess.run(["ldd", "--version"], capture_output=True, text=True).stdout.split("\n")[0]}
json.dump(m, open(os.path.join(s, "MANIFEST.json"), "w"), indent=1)
print(json.dumps({n: v["bytes"] for n, v in files.items()}))
EOF
  ;;
dev)
  D="$T/dev-data"; rm -rf "$D"; mkdir -p "$D" "$T/logs"
  bridge="$(expect_id $BRIDGE_ENTRY)"
  ( cd "$T" && FERRATA_PRIVACY_DIR="$D" RISC0_DEV_MODE=1 FERRATA_EXPECT_BRIDGE_IMAGE_ID="$bridge" \
      "$S/privacy_suite" --exact privacy_suite_lez_half --nocapture --test-threads 1 ) > "$T/logs/dev-lez.log" 2>&1
  ( cd "$root/vault" && FERRATA_PRIVACY_DIR="$D" forge test --match-contract PrivacySuiteTest -vv ) > "$T/logs/dev-forge.log" 2>&1
  python3 -I "$root/scripts/privacy/adversary.py" "$D" "$OUT/dev" | tee "$T/logs/dev-analysis.json"
  ;;
volume)
  src="${R0VM_CUDA:-$root/gpu-prover/bin/r0vm-cuda}"
  want="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha256"])' "$root/gpu-prover/logs/bin-meta/r0vm-cuda.json")"
  [ "$(sha256sum "$src" | cut -d' ' -f1)" = "$want" ] || { echo "$src is not the recorded r0vm-cuda" >&2; exit 1; }
  modal volume create ferrata-prover 2> /dev/null || true
  modal volume put -f ferrata-prover "$src" bin/r0vm-cuda > /dev/null
  modal volume put -f ferrata-prover "$root/gpu-prover/logs/bin-meta/r0vm-cuda.json" bin/r0vm-cuda.json > /dev/null
  modal volume ls ferrata-prover bin
  ;;
gpu)
  : "${RUN:?run name}"; mkdir -p "$T/modal-tmp"; export TMPDIR="$T/modal-tmp"
  ( cd "$root" && modal run --detach scripts/privacy/gpu_app.py --run "$RUN" ) | tee "$T/$RUN.gpu.log"
  modal volume get --force ferrata-prover "privacy/$RUN.tar.gz" "$T/$RUN.tar.gz" > /dev/null
  tar -xzf "$T/$RUN.tar.gz" -C "$T"
  python3 -I -c 'import json,sys; r=json.load(open(sys.argv[1])); print(json.dumps({k: r.get(k) for k in ("ok","error","lez_half","gpu_stats","container_s")}))' "$T/$RUN/run.json"
  ;;
wrap)
  : "${RUN:?run name}"; export FERRATA_WRAP_PROFILE="${FERRATA_WRAP_PROFILE:-${MODAL_PROFILE:?}}"
  for d in "$T/$RUN"/releases/release-*/; do
    d="${d%/}"; [ -s "$d/p254/proof.json" ] && continue
    python3 "$root/e2e/crown/wrap.py" "$d/p254/input.json" "$d/p254/proof.json" 2> "$d.wrap.log" \
      || { echo "$(basename "$d"): wrap failed" >&2; tail -n 5 "$d.wrap.log" >&2; exit 1; }
    tail -n 1 "$d.wrap.log"
  done
  ;;
finish)
  : "${RUN:?run name}"; G="$T/$RUN"; D="$G/data"; rid="$(expect_id release)"
  for d in "$G"/releases/release-*/; do
    d="${d%/}"; n="$(basename "$d")"
    "$E" groth16 --receipt "$d/receipt-succinct.bin" --image-id "$rid" --proof "$d/p254/proof.json" --out-dir "$d/groth16" > "$d.groth16.json"
    "$E" verify --receipt "$d/groth16/receipt-groth16.bin" --image-id "$rid" --meta "$D/releases/$n.json" > "$d.verify.json"
    "$E" verify --receipt "$d/receipt-succinct.bin" --image-id "$rid" --meta "$D/releases/$n.json" >> "$d.verify.json"
  done
  python3 -I - "$G" "$rid" <<'EOF'
import glob, json, os, sys
g, rid = sys.argv[1:3]
seals = []
for d in sorted(glob.glob(os.path.join(g, "releases", "release-*/"))):
    seals.append("0x" + open(os.path.join(d, "groth16", "seal.hex")).read().strip().removeprefix("0x"))
json.dump({"release_image_id": rid, "seals": seals}, open(os.path.join(g, "data", "release-seals.json"), "w"), indent=1)
print(f"release-seals.json: {len(seals)} seals of {sorted({len(s) // 2 - 1 for s in seals})} bytes")
EOF
  ( cd "$root/vault" && FERRATA_PRIVACY_DIR="$D" forge test --match-contract PrivacySuiteTest -vv ) > "$G/forge.log" 2>&1 \
    || { tail -n 30 "$G/forge.log"; exit 1; }
  grep -E 'PASS|FAIL|Suite result' "$G/forge.log" | head -n 3
  bash "$0" analyze "$RUN"
  ;;
analyze)
  : "${RUN:?run name}"; G="$T/$RUN"
  # Controls for the seals' recurring runs: three receipts of the same recursion circuit from another day, other
  # programs and other statements (gpu-prover/, EVIDENCE rows 5 and 10).
  r="$root/gpu-prover/results"
  export FERRATA_PRIVACY_CONTROL_RECEIPTS="$r/gpu-l4-bridge/lez-bridge/bridge-private-claim-tx.borsh,$r/gpu-l4-bridge/lez-bridge/bridge-private-burn-tx.borsh,$r/gpu-l4-attest-2/lez-attest/stage4-private-attest-tx.borsh"
  python3 -I "$root/scripts/privacy/adversary.py" "$G/data" "$OUT/generated" | tee "$G/analysis.json"
  python3 -I "$root/scripts/privacy/proofs.py" "$G" "$OUT/proofs.json"
  ;;
*) echo "unknown step $step" >&2; exit 2 ;;
esac
