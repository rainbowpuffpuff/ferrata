#!/usr/bin/env bash
# CI's attestation against a real, running LEZ sequencer in standalone mode (RFP-021 Supportability #2:
# "End-to-end integration tests exercise attestation production and consumption against a LEZ sequencer
# (standalone mode) ... and are included in CI."). Unlike the `roundtrip` job, which runs LEZ's state
# transition in process, this starts LEZ's own sequencer_service: every transaction is built, signed and
# sent by LEZ's wallet over the sequencer's RPC, through its mempool and block builder.
#
#   scripts/ci/sequencer-attest.sh RESULTS-DIR
#
# The CI job (.github/workflows/sequencer.yml, job `sequencer-attest`) runs exactly this script; so does
# the build box (spike/modal-build's mrun; never on a laptop). Needs what the `roundtrip` job needs:
# Rust 1.93.0 and 1.98.1 (LEZ v0.4.0-rc1's pinned host toolchain, lez/rust-toolchain.toml), RISC Zero's
# cargo-risczero and r0vm 3.0.5 with guest Rust 1.91.1 in $RISC0_HOME_LEZ (scripts/ci/install-risc0.sh),
# the upstream sources beside the repository (scripts/fetch-upstream.sh), libpcsclite-dev and
# protobuf-compiler. No public network: the Ethereum data is the recorded mainnet fixtures of
# crates/ferrata-spectests/fixtures/ (the period-1899 light-client updates, the Stage 4 attestation
# fixture), locally verified.
#
# What runs (lez/live/README.md; the phases of the live driver, lez/live/driver/src/main.rs): LEZ
# v0.4.0-rc1's sequencer_service built with its `standalone` feature (its mock Bedrock actor; no Logos
# Blockchain node is started), block interval 3 s (the README's rehearsal), and these phases:
#   info            the guest binaries, the accounts of the run, the sequencer's head
#   deploy          the attestation program (main, twin) and LEZ's scripted test program, through the
#                   loader (attestation *production* is set up)
#   scenarios       an instance (Init), a finality update that advances it, the next-committee update,
#                   and the refusals (15 refused with the program's exit codes, 2 reverted otherwise:
#                   the wallet's default gas limit, LEZ's validity window)
#   attest-private  attestation *consumption*: the reference consumer (lez/guests/consumer) deployed,
#                   a new mainnet instance of the attestation program initialized from the Stage 4
#                   fixture (crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10) and advanced
#                   by the fixture's finality update, then the consumer's AttestPrivately in ONE
#                   transaction: it reads the record of the finalized mainnet block and verifies an
#                   ERC-20 Transfer log of it against the record; only the Read is public. The proof
#                   is made for real by LEZ's wallet (there is no way to send a privacy-preserving
#                   transaction without one: the sequencer, started without RISC0_DEV_MODE, verifies
#                   the receipt and drops dev-mode receipts; on the build box proving took 280 s at 32
#                   threads, lez/results/v0.4.0-rc3/real-proofs/README.md row (b)); everything else in
#                   this run needs no proof at all.
#   export          the records of the whole run, into RESULTS-DIR/records/
# Nothing it starts survives it: lez/live/run.sh stops the sequencer before it returns and once more if
# a phase fails. The script exits non-zero if any phase fails or any expected record is missing, and
# prints a one-screen summary at the end.
#
# The phase `attest-private` is the only driver phase that lets a consumer consume an attestation
# without the Bedrock node: the hook phases (lez/live/hooks/crown.sh) need the node's channel API and
# the `inbound` phase needs the frozen prebuilt bridge binary (lez/live/frozen/README.md), which is not
# in the repository. What would cover the program's `Attest` handler (the bridge's Init/ImportRoot) is
# behind one of those, so this run covers consumption through AttestPrivately instead.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
die() { echo "sequencer-attest: $*" >&2; exit 1; }

OUT="${1:?usage: scripts/ci/sequencer-attest.sh RESULTS-DIR (the records are written below it)}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
LOG="$OUT/sequencer-attest.log"
: > "$LOG"
# shellcheck source=scripts/ci/sequencer-records.sh
. "$REPO/scripts/ci/sequencer-records.sh"
# No path of the machine that ran this in what it leaves behind, on every exit: CI uploads the directory
# whether the run passed or failed.
trap 'scrub_paths "$OUT" "$BESIDE" "$HOME"' EXIT

T0=$(date +%s)
say() { echo "[sequencer-attest $(date -u +%T)] $*"; }
minutes() { echo "$(( ($(date +%s) - T0) / 60 )) min"; }

# A build or run step: its whole output goes to the log; the tail is shown only when it fails.
run_logged() {
  local name="$1"; shift
  say "$name (output to ${LOG#$REPO/})"
  local rc=0
  "$@" >> "$LOG" 2>&1 || rc=$?
  if [ "$rc" != 0 ]; then
    echo "== $name FAILED (exit $rc); the last 40 lines of ${LOG#$REPO/}:" >&2
    tail -n 40 "$LOG" >&2
    echo "== the driver's and the sequencer's logs:" >&2
    for f in "$OUT"/records/logs/driver.log "$OUT"/records/logs/sequencer.log; do
      if [ -f "$f" ]; then echo "-- ${f#$REPO/}:" >&2; tail -n 20 "$f" >&2; fi
    done
    bash "$REPO/lez/live/run.sh" stop > /dev/null 2>&1 || true
    exit "$rc"
  fi
}

# ---- The toolchains ------------------------------------------------------------------------------
export PATH="$HOME/.cargo/bin:$HOME/.risc0/bin:$PATH"
lez_home="${RISC0_HOME_LEZ:-}"
if [ -z "$lez_home" ]; then
  for d in "$BESIDE/risc0-home-lez" "$HOME/risc0-home-lez"; do
    if [ -d "$d" ]; then lez_home="$(cd "$d" && pwd)"; break; fi
  done
fi
[ -n "$lez_home" ] || die "no RISC Zero home with guest Rust 1.91.1 for LEZ (set RISC0_HOME_LEZ; scripts/ci/install-risc0.sh)"
export RISC0_HOME="$lez_home"
command -v r0vm > /dev/null || die "r0vm is not on PATH (scripts/ci/install-risc0.sh)"
unset RISC0_DEV_MODE   # the sequencer must verify receipts; the driver's proof must be real

[ -f "$BESIDE/upstream/lez-v0.4.0-rc1/Cargo.toml" ] || die "LEZ v0.4.0-rc1 is not beside the repository (scripts/fetch-upstream.sh)"
[ -x "$REPO/scripts/guest-rustc" ] || die "scripts/guest-rustc is missing (the guests are built through it)"
[ -f "$REPO/lez/live/driver/Cargo.toml" ] || die "lez/live/driver is not in this checkout"

# The driver links LEZ's crates by path: its manifests must point at the tag env.sh builds (v0.4.0-rc1).
selected="$(bash "$REPO/lez/scripts/lez-select.sh" --show)"
[ "$selected" = v0.4.0-rc1 ] || die "the driver's manifests point at LEZ $selected, not v0.4.0-rc1 (lez/scripts/lez-select.sh v0.4.0-rc1)"

# Parallelism: lez/live/env.sh defaults to 4 build jobs under nice 19; give a bigger machine more, a
# small runner 4. The proof's threads (RAYON_NUM_THREADS of LEZ's wallet) oversubscribe harmlessly;
# the recorded attest proof used 32 threads on a 16-core box.
NPROC="$(nproc)"
if [ "$NPROC" -lt 4 ]; then NPROC=4; fi
if [ "$NPROC" -gt 16 ]; then NPROC=16; fi
export FERRATA_LIVE_JOBS="${FERRATA_LIVE_JOBS:-$NPROC}"
export FERRATA_LIVE_PROOF_THREADS="${FERRATA_LIVE_PROOF_THREADS:-16}"
# The Logos circuit crates download their prover artifacts (47 MB) at build time: keep them beside the
# repository, where lez/live/env.sh puts them too.
export FERRATA_LIVE_TMP="${FERRATA_LIVE_TMP:-$BESIDE/tmp/sequencer}"
export XDG_CACHE_HOME="${FERRATA_LIVE_XDG_CACHE:-$FERRATA_LIVE_TMP/xdg-cache}"
say "scratch ${FERRATA_LIVE_TMP#$BESIDE/}; build jobs $FERRATA_LIVE_JOBS; proof threads $FERRATA_LIVE_PROOF_THREADS"

# ---- Build ---------------------------------------------------------------------------------------
say "== build (started $(date -u +%Y-%m-%dT%H:%M:%SZ))"
run_logged "build: LEZ's sequencer_service (standalone)" bash "$REPO/lez/live/run.sh" build-standalone
run_logged "build: the live driver" bash "$REPO/lez/live/run.sh" build-driver
run_logged "build: the guest programs (lez methods)" \
  bash -c 'cd "$1/lez" && RISC0_BUILD_LOCKED=1 RUSTC_WRAPPER="$1/scripts/guest-rustc" cargo build -p methods --locked' _ "$REPO"
guest_bin() {
  local b
  b="$(find "$REPO/lez/target/riscv-guest" -path '*release*' -name "$1" -print -quit 2>/dev/null || true)"
  [ -n "$b" ] || die "the guest binary $1 was not built (lez/target/riscv-guest)"
  echo "$b"
}
PROGRAM_BIN="$(guest_bin ferrata_lez.bin)"
SCRIPTED_BIN="$(guest_bin scripted.bin)"
CONSUMER_BIN="$(guest_bin ferrata_consumer.bin)"
say "guests: $(basename "$PROGRAM_BIN") $(sha256sum "$PROGRAM_BIN" | cut -c1-16), $(basename "$SCRIPTED_BIN") $(sha256sum "$SCRIPTED_BIN" | cut -c1-16), $(basename "$CONSUMER_BIN") $(sha256sum "$CONSUMER_BIN" | cut -c1-16)"
say "build took $(minutes)"

# ---- The run -------------------------------------------------------------------------------------
export FERRATA_LIVE_BEDROCK=mock FERRATA_LIVE_BLOCK_TIME=3s
export FERRATA_PROGRAM_BIN="$PROGRAM_BIN" FERRATA_SCRIPTED_BIN="$SCRIPTED_BIN"
export FERRATA_LIVE_DRIVER_EXTRA="--consumer-bin $CONSUMER_BIN"
export FERRATA_LIVE_PHASES="info deploy scenarios attest-private export"
export FERRATA_LIVE_OUT="$OUT/records"
say "== run: the sequencer's mock Bedrock (standalone build; no Logos Blockchain node), blocks every $FERRATA_LIVE_BLOCK_TIME, phases: $FERRATA_LIVE_PHASES"
run_logged "run: LEZ sequencer (standalone) + the driver's phases" bash "$REPO/lez/live/run.sh" run
say "run took $(minutes)"

# ---- The records ---------------------------------------------------------------------------------
RECORDS="$OUT/records"
fail=0
check() { # DESCRIPTION COMMAND...: the command must succeed
  local what="$1"; shift
  if "$@" > /dev/null 2>&1; then echo "  ok: $what"; else echo "  MISSING: $what" >&2; fail=1; fi
}
status_count() { awk -F'\t' -v s="$2" 'NR > 1 && $6 == s' "$1" | wc -l; }

for f in run.tsv summary.txt chain.tsv programs.tsv instances.tsv environment.md logs/driver.log logs/sequencer.log; do
  check "records/$f is written" test -s "$RECORDS/$f"
done

APPLIED="$(status_count "$RECORDS/run.tsv" applied)"
REFUSED="$(status_count "$RECORDS/run.tsv" refused)"
REVERTED="$(status_count "$RECORDS/run.tsv" reverted)"
DROPPED="$(status_count "$RECORDS/run.tsv" dropped)"
REJECTED="$(status_count "$RECORDS/run.tsv" rejected)"
UNFINISHED="$(status_count "$RECORDS/run.tsv" unfinished)"
TOTAL="$(( $(wc -l < "$RECORDS/run.tsv") - 1 ))"

[ "$APPLIED" -ge 25 ] || { echo "  MISSING: at least 25 applied transactions, got $APPLIED" >&2; fail=1; }
[ "$REFUSED" -ge 10 ] || { echo "  MISSING: at least 10 refusals (the scenarios' exit codes), got $REFUSED" >&2; fail=1; }
[ "$REVERTED" -ge 2 ] || { echo "  MISSING: the two designed reverts (the wallet's default gas limit, the validity window), got $REVERTED" >&2; fail=1; }
for pair in "dropped $DROPPED" "rejected $REJECTED" "unfinished $UNFINISHED"; do
  n="${pair#* }"
  if [ "$n" != 0 ]; then echo "  MISSING: no ${pair%% *} transaction in this run, got $n" >&2; fail=1; fi
done

# A named refusal: the scenarios' "Init of an existing instance" (ALREADY_INITIALIZED).
named_refusal() { awk -F'\t' '$2 == "Init of an existing instance" && $6 == "refused" { found = 1 } END { exit !found }' "$1"; }
check 'one named refusal: "Init of an existing instance" refused' named_refusal "$RECORDS/run.tsv"

# The reference consumer's result: the private attestation applied, in a block.
CONSUMER_ROW="$(awk -F'\t' 'NR > 1 && $2 ~ /AttestPrivately/' "$RECORDS/run.tsv" | head -n 1)"
if [ -z "$CONSUMER_ROW" ] || [ "$(printf '%s' "$CONSUMER_ROW" | awk -F'\t' '{print $6}')" != applied ]; then
  echo "  MISSING: the consumer's AttestPrivately applied (run.tsv: ${CONSUMER_ROW:-no row})" >&2
  fail=1
fi
CONSUMER_TX="$(printf '%s' "$CONSUMER_ROW" | awk -F'\t' '{print $3}')"
CONSUMER_BLOCK="$(printf '%s' "$CONSUMER_ROW" | awk -F'\t' '{print $4}')"
CONSUMER_GAS="$(printf '%s' "$CONSUMER_ROW" | awk -F'\t' '{print "execution gas " $8 ", storage gas " $9 ", fee " $13}')"

# The consumer was deployed, and its attestation instance is active at the fixture's finalized slot.
check 'the consumer deployed through the loader (its header and every segment applied)' consumer_deployed "$RECORDS"
SLOT="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["finalized_slot"])' "$REPO/crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10/fixture.json")"
INST_ROW="$(awk -F'\t' 'NR > 1 && $1 ~ /^attest-mainnet/' "$RECORDS/instances.tsv")"
if [ -z "$INST_ROW" ]; then
  echo "  MISSING: the attest-mainnet instance in instances.tsv" >&2; fail=1
elif [ "$(printf '%s' "$INST_ROW" | awk -F'\t' '{print $5}')" != true ] || [ "$(printf '%s' "$INST_ROW" | awk -F'\t' '{print $7}')" != "$SLOT" ]; then
  echo "  MISSING: the attest-mainnet instance is not active at the fixture's finalized slot $SLOT: $INST_ROW" >&2
  fail=1
fi

if [ "$fail" != 0 ]; then
  echo "sequencer-attest: expected records are missing from ${OUT#$REPO/}" >&2
  exit 1
fi

# ---- The summary, the record of this run ----------------------------------------------------------
STARTED="$(sed -n 's/^- started \(.*\), finished.*/\1/p' "$RECORDS/environment.md")"
FINISHED="$(sed -n 's/^- started .*finished \(.*\) (UTC).*/\1/p' "$RECORDS/environment.md")"
{
  echo "# CI: attestation against a LEZ sequencer in standalone mode (run of $STARTED)"
  echo
  echo "RFP-021 Supportability #2: attestation production and consumption against a running LEZ"
  echo "sequencer in standalone mode (the \`sequencer-attest\` job of .github/workflows/sequencer.yml)."
  echo "Recorded here from a run on the repository's own remote Linux x86-64 build box (16 cores,"
  echo "64 GiB, through spike/modal-build's mrun), from this worktree, with the upstream LEZ"
  echo "v0.4.0-rc1 sources beside the repository."
  echo
  echo "- command: \`bash scripts/ci/sequencer-attest.sh ${OUT#$REPO/}\` (from the repository root)"
  echo "- started $STARTED, finished $FINISHED (UTC)"
  echo "- the zone: LEZ v0.4.0-rc1's sequencer_service built with its \`standalone\` feature (the mock"
  echo "  Bedrock actor; no Logos Blockchain node); with the mock no block becomes final, so the"
  echo "  driver settles every transaction at the sequencer's head"
  echo "- phases: $FERRATA_LIVE_PHASES"
  echo "- transactions: $TOTAL sent; $APPLIED applied, $REFUSED refused by the program with an exit code,"
  echo "  $REVERTED reverted otherwise (the two designed: the wallet's default gas limit, LEZ's validity"
  echo "  window), $DROPPED dropped, $REJECTED rejected at the RPC, $UNFINISHED unfinished"
  echo "- the reference consumer's private attestation: applied in block $CONSUMER_BLOCK, transaction"
  echo "  \`$CONSUMER_TX\`, charged $CONSUMER_GAS; its instance is active at the fixture's finalized slot $SLOT"
  echo "- records: records/ (run.tsv the manifest, summary.txt, chain.tsv, blocks.borsh, programs.tsv,"
  echo "  instances.tsv, environment.md, logs/, private/); the whole output: sequencer-attest.log"
  echo "- not covered by this run: the attestation program's \`Attest\` handler (only the bridge program's"
  echo "  messages call it, and the phases that send those need either the Bedrock node's channel API or"
  echo "  the frozen prebuilt bridge binary, which is not in the repository); consumption here is the"
  echo "  reference consumer's \`AttestPrivately\`, whose real proof is the run's only proof"
  echo
  echo "The records are self-describing; the machine's own paths in them are replaced with relative"
  echo "ones by lez/live/run-live.sh and by this script."
} > "$OUT/README.md"


echo "== summary: $TOTAL transactions sent; $APPLIED applied, $REFUSED refused with an exit code, $REVERTED reverted, $DROPPED dropped, $REJECTED rejected, $UNFINISHED unfinished"
echo "== the reference consumer's AttestPrivately: applied in block $CONSUMER_BLOCK (transaction $CONSUMER_TX, $CONSUMER_GAS); its instance active at the fixture's finalized slot $SLOT"
sed -n '1,6p' "$RECORDS/summary.txt" | sed 's/^/    /'
echo "== records in ${OUT#$REPO/} (README.md, records/, sequencer-attest.log); total $(minutes)"
