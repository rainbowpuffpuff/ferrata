#!/usr/bin/env bash
# Stages what real proofs of the reference consumer's private claim through K hops need (RFP-021 P.4, hops inside
# the user's proof), in the form lez/results/p4-minimum/minimum.py reads (/bundle/<job>): the prebuilt host binary
# prove_vault_claim (it embeds the consumer and program guests and LEZ's privacy-preserving circuit) and the fixture
# directories it reads, with a MANIFEST.json of their sizes and SHA-256 sums. Nothing is proven here.
#
# usage: lez/scripts/stage-prove-vault-claim.sh OUTDIR [BINARY]
#   OUTDIR   new or empty: prove_vault_claim, fixtures/<name>/ and MANIFEST.json go there
#   BINARY   the prove_vault_claim to stage (default: lez/target/debug/prove_vault_claim, or under
#            CARGO_TARGET_DIR if that is set)
#
# The binary run from the stage reads the fixtures through FERRATA_VAULT_FIXTURE_DIR, FERRATA_SEPOLIA_FIXTURE_DIR,
# FERRATA_SEPOLIA_ATTEST_DIR and FERRATA_HOPS_FIXTURES_DIR (MANIFEST.json's "env", relative to the stage). Set
# FERRATA_HEAD and FERRATA_DIRTY where the sources are a copy without `.git`.
set -euo pipefail
lez="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$lez/.." && pwd)"
out="${1:?usage: stage-prove-vault-claim.sh OUTDIR [BINARY]}"
bin="${2:-${CARGO_TARGET_DIR:-$lez/target}/debug/prove_vault_claim}"
[ -x "$bin" ] || { echo "no binary at $bin: build it with cargo build -p harness --locked --bin prove_vault_claim" >&2; exit 1; }
[ -e "$out" ] && [ -n "$(ls -A "$out")" ] && { echo "OUTDIR must be new or empty: $out" >&2; exit 1; }
# What RealVault::load (the replay, the vault fixture) and hops::claim_hops read.
fixtures="sepolia-gloas-2026-11-10 sepolia-attest-2026-11-10 sepolia-vault-v5-2026-11-11 sepolia-hops-2026-11-18 sepolia-hops-2026-11-27"
mkdir -p "$out/fixtures"
cp -p "$bin" "$out/prove_vault_claim"
for f in $fixtures; do
  cp -rp "$repo/crates/ferrata-spectests/fixtures/$f" "$out/fixtures/"
  (cd "$out/fixtures/$f" && sha256sum -c --quiet SHA256SUMS)
done
ids=$("$out/prove_vault_claim" --image-ids | sed -n 1p)  # sed reads it all: the binary prints a table after the first line
head="${FERRATA_HEAD:-$(git -C "$repo" rev-parse HEAD 2>/dev/null || echo unknown)}"
dirty="${FERRATA_DIRTY:-$(git -C "$repo" status --porcelain 2>/dev/null | wc -l)}"
python3 -I - "$out" "$head" "$dirty" "$ids" $fixtures <<'PY'
import hashlib, json, pathlib, re, sys
root, head, dirty, ids, fixtures = pathlib.Path(sys.argv[1]), sys.argv[2], int(sys.argv[3]), sys.argv[4], sys.argv[5:]
consumer = re.search(r"consumer image ([0-9a-f]{64})", ids)
program = re.search(r"program image ([0-9a-f]{64})", ids)
if not consumer or not program:
    sys.exit(f"could not read the image ids from the binary's output: {ids!r}")
files = {str(p.relative_to(root)): {"bytes": p.stat().st_size, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
         for p in sorted(root.rglob("*")) if p.is_file() and p.name != "MANIFEST.json"}
manifest = {
    "binary_from": "lez/target/debug/prove_vault_claim",
    "ferrata_commit": head,
    "ferrata_uncommitted_changes": dirty,
    "fixtures_from": [f"crates/ferrata-spectests/fixtures/{f}" for f in fixtures],
    "expected_consumer_image_id": consumer.group(1),
    "program_image_id": program.group(1),
    "env": {
        "FERRATA_VAULT_FIXTURE_DIR": "fixtures/sepolia-vault-v5-2026-11-11",
        "FERRATA_SEPOLIA_FIXTURE_DIR": "fixtures/sepolia-gloas-2026-11-10",
        "FERRATA_SEPOLIA_ATTEST_DIR": "fixtures/sepolia-attest-2026-11-10",
        "FERRATA_HOPS_FIXTURES_DIR": "fixtures",
    },
    "prove_command": "FERRATA_EXPECT_CONSUMER_IMAGE_ID=<expected_consumer_image_id> <env, each under this directory> "
                     "RISC0_DEV_MODE=0 /usr/bin/time -v ./prove_vault_claim --hops K --shape wallet --tx-out tx-claim.borsh",
    "files": files,
}
(root / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps({k: v for k, v in manifest.items() if k != "files"}, indent=2))
print(f"{len(files)} files")
PY
