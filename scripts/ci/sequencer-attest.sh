#!/usr/bin/env bash
# CI's attestation against a real, running LEZ sequencer in standalone mode, on Sepolia data fetched when the run
# starts (RFP-021 Supportability #2: "End-to-end integration tests exercise attestation production and consumption
# against a LEZ sequencer (standalone mode) and an Ethereum test network or local fork, and are included in CI.").
# Unlike the `roundtrip` job, which runs LEZ's state transition in process, this starts LEZ's own sequencer_service:
# every transaction is built, signed and sent by LEZ's wallet over the sequencer's RPC, through its mempool and block
# builder, and read back by following its blocks with LEZ's own block application (lez/live/README.md).
#
#   scripts/ci/sequencer-attest.sh RESULTS-DIR [STAGE...]
#
# STAGE is one of checkpoint, build, data, run, check, summary; without one, all six in this order. The CI job
# (.github/workflows/sequencer.yml, job `sequencer-attest`) runs one stage per step, each with its own timeout; the
# build box (spike/modal-build's mrun; never a laptop) runs them in one call. Between stages the state is in
# RESULTS-DIR and in the scratch beside the repository (tmp/sequencer-<tag>).
#
#   checkpoint  scripts/fetch-sepolia-checkpoint.py: the bootstrap of a recent finalized Sepolia checkpoint from
#               Lodestar's public beacon API (the period's first checkpoint if it serves it, else its finalized one),
#               the root the second provider (publicnode's beacon API) reports at its slot, which becomes the
#               instance's trusted checkpoint, and the best update of every period from the bootstrap's. Three
#               attempts, two minutes apart; then the run stops: nothing recorded is replayed in its place.
#   build       the guests from the committed tree (its LEZ v0.4.0-rc1 manifests: the images of docs/image-ids.md,
#               which the run checks), then LEZ $LEZ_TAG's sequencer_service with its `standalone` feature (its mock
#               Bedrock actor: no Logos Blockchain node) and the live driver against $LEZ_TAG
#               (lez/scripts/lez-select.sh; the manifests are put back afterwards). LEZ_TAG: FERRATA_CI_LEZ_TAG,
#               default v0.4.0-rc3, the tag of the test network's deployment kit (lez/testnet04/).
#   data        lez/scripts/fetch-attest-fixture.py --network sepolia --same-address none: Lodestar's newest finality
#               update, the execution block its finalized header binds (RLP header and receipts from a public Sepolia
#               RPC, a second RPC's copy), eth_getProof of Sepolia USDC's totalSupply_ at that block, its first ERC-20
#               Transfer, the second beacon provider's root of the finalized block. `live-driver fresh-check` runs
#               the whole chain natively on it; until its finality update advances the checkpoint's store, it is
#               fetched again every two minutes, at most 12 times.
#   run         lez/live/run.sh run: the standalone sequencer (blocks every 3 s, RISC0_DEV_MODE unset, so it verifies
#               private proofs) and the driver's phases `info deploy fresh-follow fresh-scenarios fresh-consume
#               fresh-private export` (lez/live/driver/src/fresh.rs; FERRATA_CI_PRIVATE=skip leaves out the private
#               attestation and its real proof), each capped at FERRATA_LIVE_PHASE_SECONDS (default 5,400: the proof
#               took 1,222 s with 4 threads on the build box and 2,927 s on GitHub's 4-core runner, run 45333224944).
#   check       the records: every transaction's outcome as designed, by count and by name, phase by phase; the
#               deployed images equal docs/image-ids.md's; instance A holds the finality update's block, with the
#               root the second provider reports.
#   summary     RESULTS-DIR/README.md and, under GitHub Actions, the job summary ($GITHUB_STEP_SUMMARY): the checkpoint
#               and the block used, the outcome of every phase. Written whatever happened before; never fails.
#
# What the phases do (lez/live/driver/src/fresh.rs): instance A of the attestation program follows Sepolia from the
# checkpoint (Init, the period update that hands over the next committee, the finality update: attestation
# *production*); the twin deployment's refusals on the same data (a signature that is not a point; the finality
# update carrying another update's signature: steps 1 and 2 pass, step 3 refuses; steps sent by the wrong actor; a
# second Init; an update for another chain's instance; a job opened before its signature slot on an instance whose
# clock runs ten years later; duplicated and replayed steps); attestation *consumption* under A's record: `Attest`
# as the root of a transaction (a storage slot, a log, both) and its refusals, the reference consumer's `CallAttest`
# keeping the program's whole reply, its `Claim` of the Transfer crediting the sender once and the replay refused,
# and its `AttestPrivately` at a private account, proven for real by LEZ's wallet and verified by the sequencer.
# No key is held for any refusal: each is made from the real data. Nothing is sent to any network: the sequencer is
# on 127.0.0.1; the public endpoints are read only (scripts/pubnet.py's generic User-Agent, no key).
#
# Needs what the `roundtrip` job needs: Rust 1.93.0 and 1.98.1, RISC Zero's cargo-risczero and r0vm 3.0.5 with guest
# Rust 1.91.1 in $RISC0_HOME_LEZ (scripts/ci/install-risc0.sh), the upstream sources beside the repository
# (scripts/fetch-upstream.sh: LEZ v0.4.0-rc1 for the guests, $LEZ_TAG for the sequencer and the driver),
# libpcsclite-dev, protobuf-compiler, python3, curl.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
die() { echo "sequencer-attest: $*" >&2; exit 1; }

OUT="${1:?usage: scripts/ci/sequencer-attest.sh RESULTS-DIR [checkpoint|build|data|run|check|summary]...}"
shift
STAGES="${*:-checkpoint build data run check summary}"
for s in $STAGES; do
  case "$s" in checkpoint|build|data|run|check|summary) ;; *) die "unknown stage $s" ;; esac
done
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
LOG="$OUT/sequencer-attest.log"
touch "$LOG"
FRESH="$OUT/fresh"
RECORDS="$OUT/records"
# shellcheck source=scripts/ci/sequencer-records.sh
. "$REPO/scripts/ci/sequencer-records.sh"

export LEZ_TAG="${FERRATA_CI_LEZ_TAG:-v0.4.0-rc3}"
if [ "$LEZ_TAG" = v0.4.0-rc1 ]; then SCRATCH_NAME=sequencer; else SCRATCH_NAME="sequencer-$LEZ_TAG"; fi
export FERRATA_LIVE_TMP="${FERRATA_LIVE_TMP:-$BESIDE/tmp/$SCRATCH_NAME}"
BIN="$FERRATA_LIVE_TMP/bin"

# On every exit: the manifests back at the committed LEZ tag if this call moved them, and no path of the machine in
# what it leaves behind (CI uploads RESULTS-DIR whether the run passed or failed).
SELECTED=""
SELECT_MARK="$OUT/.lez-select-mark"
restore_selection() {
  if [ -n "$SELECTED" ]; then
    bash "$REPO/lez/scripts/lez-select.sh" v0.4.0-rc1 >> "$LOG" 2>&1 || echo "sequencer-attest: lez-select.sh v0.4.0-rc1 FAILED" >&2
    # patch(1) leaves a backup of a lockfile it applied with an offset; this call made it.
    find "$REPO/lez" -maxdepth 3 -name 'Cargo.lock.orig' -newer "$SELECT_MARK" -delete 2>/dev/null || true
    SELECTED=""
  fi
  rm -f "$SELECT_MARK"
}
on_exit() {
  restore_selection
  scrub_paths "$OUT" "$BESIDE" "$HOME" "$REPO"
}
trap on_exit EXIT

T0=$(date +%s)
say() { echo "[sequencer-attest $(date -u +%T)] $*"; echo "[$(date -u +%T)] $*" >> "$LOG"; }
minutes() { echo "$(( ($(date +%s) - T0) / 60 )) min"; }

# A build or run step: its whole output goes to $STEP_LOG (the build's to build.log); the tail is shown only when it
# fails.
STEP_LOG="$LOG"
run_logged() {
  local name="$1"; shift
  say "$name (output to ${STEP_LOG#"$REPO"/})"
  local rc=0
  "$@" >> "$STEP_LOG" 2>&1 || rc=$?
  if [ "$rc" != 0 ]; then
    echo "== $name FAILED (exit $rc); the last 40 lines of ${STEP_LOG#"$REPO"/}:" >&2
    tail -n 40 "$STEP_LOG" >&2
    echo "== the driver's and the sequencer's logs:" >&2
    for f in "$RECORDS"/logs/driver.log "$RECORDS"/logs/sequencer.log; do
      if [ -f "$f" ]; then echo "-- ${f#"$REPO"/}:" >&2; tail -n 20 "$f" >&2; fi
    done
    bash "$REPO/lez/live/run.sh" stop > /dev/null 2>&1 || true
    exit "$rc"
  fi
}

# ---- The toolchains ------------------------------------------------------------------------------------------------
export PATH="$HOME/.cargo/bin:$HOME/.risc0/bin:$PATH"
toolchains() {
  local lez_home="${RISC0_HOME_LEZ:-}" d
  if [ -z "$lez_home" ]; then
    for d in "$BESIDE/risc0-home-lez" "$HOME/risc0-home-lez"; do
      if [ -d "$d" ]; then lez_home="$(cd "$d" && pwd)"; break; fi
    done
  fi
  [ -n "$lez_home" ] || die "no RISC Zero home with guest Rust 1.91.1 for LEZ (set RISC0_HOME_LEZ; scripts/ci/install-risc0.sh)"
  export RISC0_HOME="$lez_home"
  # LEZ's crates (the sequencer, the driver, the wallet's prover) are on risc0-zkvm 3.0.5, which finds its r0vm through
  # RISC0_HOME; rzup puts it in no bin/ directory, so it goes on PATH too, for what calls `r0vm` by name.
  local r0vm
  r0vm="$(r0vm_of "$lez_home" 3.0.5)" || die "no r0vm 3.0.5 in $lez_home or on PATH (scripts/ci/install-risc0.sh)"
  export PATH="$(dirname "$r0vm"):$PATH"
}
unset RISC0_DEV_MODE   # the sequencer verifies receipts; the private attestation's proof is real

# Parallelism: lez/live/env.sh defaults to 4 build jobs under nice 19; a bigger machine gets more, a small runner 4.
# The private attestation's prover threads likewise: each holds a segment in memory, so a 4-core runner with 16 GB
# proves with 4 (measured on the build box with 4: 1,222 s, e2e/runs/ci-sequencer-2026-11-24/attest/).
NPROC="$(nproc)"
if [ "$NPROC" -lt 4 ]; then NPROC=4; fi
if [ "$NPROC" -gt 16 ]; then NPROC=16; fi
export FERRATA_LIVE_JOBS="${FERRATA_LIVE_JOBS:-$NPROC}"
export FERRATA_LIVE_PROOF_THREADS="${FERRATA_LIVE_PROOF_THREADS:-$NPROC}"

guest_bin() {
  local b
  b="$(find "$REPO/lez/target/riscv-guest" -path '*release*' -name "$1" -print -quit 2>/dev/null || true)"
  [ -n "$b" ] || die "the guest binary $1 was not built (lez/target/riscv-guest; stage build)"
  echo "$b"
}

# ---- checkpoint ----------------------------------------------------------------------------------------------------
stage_checkpoint() {
  say "== checkpoint: a recent finalized Sepolia checkpoint, from public endpoints"
  mkdir -p "$FRESH"
  local i
  for i in 1 2 3; do
    rm -rf "$FRESH/checkpoint"
    echo "== fetch-sepolia-checkpoint.py, attempt $i ($(date -u +%FT%TZ))" >> "$FRESH/fetch.log"
    if python3 -I "$REPO/scripts/fetch-sepolia-checkpoint.py" "$FRESH/checkpoint" >> "$FRESH/fetch.log" 2>&1; then
      say "checkpoint: $(grep -E '^bootstrap:' "$FRESH/fetch.log" | tail -n 1)"
      say "checkpoint: $(grep -E '^done:' "$FRESH/fetch.log" | tail -n 1 | sed 's#^done: [^;]*; ##')"
      return 0
    fi
    rm -rf "$FRESH/checkpoint"
    echo "sequencer-attest: fetching the checkpoint failed (attempt $i of 3); the last lines of fresh/fetch.log:" >&2
    tail -n 8 "$FRESH/fetch.log" >&2
    if [ "$i" != 3 ]; then sleep 120; fi
  done
  die "no Sepolia checkpoint could be fetched from the public endpoints (fresh/fetch.log): the run stops here; nothing recorded is replayed in its place"
}

# ---- build ---------------------------------------------------------------------------------------------------------
stage_build() {
  toolchains
  [ -f "$BESIDE/upstream/lez-v0.4.0-rc1/Cargo.toml" ] || die "LEZ v0.4.0-rc1 is not beside the repository (scripts/fetch-upstream.sh)"
  [ -f "$BESIDE/upstream/lez-$LEZ_TAG/Cargo.toml" ] || die "LEZ $LEZ_TAG is not beside the repository (scripts/fetch-upstream.sh)"
  [ -x "$REPO/scripts/guest-rustc" ] || die "scripts/guest-rustc is missing (the guests are built through it)"
  local selected
  selected="$(bash "$REPO/lez/scripts/lez-select.sh" --show)"
  [ "$selected" = v0.4.0-rc1 ] || die "the tree's manifests point at LEZ $selected; the committed selection is v0.4.0-rc1"
  say "== build (started $(date -u +%Y-%m-%dT%H:%M:%SZ)); scratch ${FERRATA_LIVE_TMP#"$BESIDE"/}; $FERRATA_LIVE_JOBS build jobs"
  STEP_LOG="$OUT/build.log"
  run_logged "build: the guest programs (lez methods; the committed v0.4.0-rc1 manifests)" \
    bash -c 'cd "$1/lez" && RISC0_BUILD_LOCKED=1 RUSTC_WRAPPER="$1/scripts/guest-rustc" cargo build -p methods --locked' _ "$REPO"
  local b
  for b in ferrata_lez.bin scripted.bin ferrata_consumer.bin; do
    say "guest $b: sha256 $(sha256sum "$(guest_bin "$b")" | cut -c1-16)"
  done
  if [ "$LEZ_TAG" != v0.4.0-rc1 ]; then
    touch "$SELECT_MARK"
    SELECTED=1
    run_logged "select LEZ $LEZ_TAG for the sequencer and the driver (lez/scripts/lez-select.sh)" \
      bash "$REPO/lez/scripts/lez-select.sh" "$LEZ_TAG"
  fi
  run_logged "build: LEZ $LEZ_TAG's sequencer_service (standalone)" bash "$REPO/lez/live/run.sh" build-standalone
  run_logged "build: the live driver against LEZ $LEZ_TAG" bash "$REPO/lez/live/run.sh" build-driver
  restore_selection
  STEP_LOG="$LOG"
  say "build took $(minutes)"
}

# ---- data ----------------------------------------------------------------------------------------------------------
stage_data() {
  [ -f "$FRESH/checkpoint/SHA256SUMS" ] || die "no checkpoint in ${FRESH#"$REPO"/} (stage checkpoint)"
  [ -x "$BIN/live-driver" ] || die "the live driver is not built (stage build)"
  say "== data: the newest finality update and the facts under its block, from public endpoints"
  rm -rf "$FRESH/attest" "$FRESH"/attest-try-* "$FRESH/check.json"
  local i rc d
  for i in $(seq 1 12); do
    d="$FRESH/attest-try-$i"
    echo "== fetch-attest-fixture.py --network sepolia, attempt $i ($(date -u +%FT%TZ))" >> "$FRESH/fetch.log"
    if python3 -I "$REPO/lez/scripts/fetch-attest-fixture.py" --network sepolia --same-address none "$d" >> "$FRESH/fetch.log" 2>&1; then
      rc=0
      "$BIN/live-driver" fresh-check --fresh-checkpoint "$FRESH/checkpoint" --fresh-attest "$d" --out "$FRESH/check.json" \
        >> "$FRESH/fetch.log" 2>&1 || rc=$?
      case "$rc" in
        0)
          mv "$d" "$FRESH/attest"
          rm -rf "$FRESH"/attest-try-*
          say "data: $(python3 -I -c 'import json, sys; c = json.load(open(sys.argv[1])); print("finalized slot %s (%s), block %s, %s signers; checked natively" % (c["finalizedSlot"], c["finalizedTime"], c["block"], c["signers"]))' "$FRESH/check.json")"
          return 0 ;;
        3) say "data: attempt $i: $(grep -E '^fresh-check:' "$FRESH/fetch.log" | tail -n 1)"; rm -rf "$d" ;;
        *)
          tail -n 15 "$FRESH/fetch.log" >&2
          die "the fetched data does not check natively (live-driver fresh-check exit $rc; fresh/fetch.log)" ;;
      esac
    else
      say "data: attempt $i: the fetch stopped: $(tail -n 1 "$FRESH/fetch.log" | cut -c1-200)"
      rm -rf "$d"
    fi
    if [ "$i" != 12 ]; then sleep 120; fi
  done
  die "no finality update that advances the checkpoint's store in 12 attempts over 22 minutes (fresh/fetch.log): the run stops here"
}

# ---- run -----------------------------------------------------------------------------------------------------------
stage_run() {
  toolchains
  [ -f "$FRESH/check.json" ] && [ -d "$FRESH/attest" ] || die "no checked data (stages checkpoint and data)"
  local program scripted consumer phases
  program="$(guest_bin ferrata_lez.bin)"; scripted="$(guest_bin scripted.bin)"; consumer="$(guest_bin ferrata_consumer.bin)"
  phases="info deploy fresh-follow fresh-scenarios fresh-consume"
  if [ "${FERRATA_CI_PRIVATE:-real}" != skip ]; then phases="$phases fresh-private"; fi
  phases="$phases export"
  echo "$phases" > "$OUT/phases.txt"
  export FERRATA_LIVE_BEDROCK=mock FERRATA_LIVE_BLOCK_TIME=3s
  export FERRATA_PROGRAM_BIN="$program" FERRATA_SCRIPTED_BIN="$scripted"
  export FERRATA_LIVE_DRIVER_EXTRA="--consumer-bin $consumer --fresh-checkpoint $FRESH/checkpoint --fresh-attest $FRESH/attest"
  export FERRATA_LIVE_PHASES="$phases" FERRATA_LIVE_OUT="$RECORDS"
  export FERRATA_LIVE_PHASE_SECONDS="${FERRATA_LIVE_PHASE_SECONDS:-5400}"
  rm -rf "$RECORDS"
  say "== run: LEZ $LEZ_TAG's sequencer (standalone: its mock Bedrock, no Logos Blockchain node), blocks every 3 s; phases: $phases (each at most $FERRATA_LIVE_PHASE_SECONDS s)"
  run_logged "run: LEZ sequencer (standalone) + the driver's phases" bash "$REPO/lez/live/run.sh" run
  say "run done at $(minutes)"
}

# ---- check ---------------------------------------------------------------------------------------------------------
status_count() { awk -F'\t' -v s="$2" 'NR > 1 && $6 == s' "$1" | wc -l; }
row_status() { # RUN-TSV STEP: the status and the exit code's number of the row named STEP (the last one)
  awk -F'\t' -v step="$2" 'NR > 1 && $2 == step { s = $6; split($7, e, " "); c = e[1] } END { if (s != "") print s " " c }' "$1"
}
# The image id docs/image-ids.md's first table gives a guest (`name`), without 0x.
doc_image() {
  python3 -I - "$REPO/docs/image-ids.md" "$1" <<'PY'
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
m = re.search(r"`" + re.escape(sys.argv[2]) + r"` \| `0x([0-9a-f]{64})`", text)
print(m.group(1) if m else "")
PY
}

stage_check() {
  local fail=0 f
  check() { # DESCRIPTION COMMAND...: the command must succeed
    local what="$1"; shift
    if "$@" > /dev/null 2>&1; then echo "  ok: $what"; else echo "  MISSING: $what" >&2; fail=1; fi
  }
  want() { # STEP STATUS [EXIT]
    local got; got="$(row_status "$RECORDS/run.tsv" "$1")"
    if [ "${got%% *}" = "$2" ] && { [ -z "${3:-}" ] || [ "${got#* }" = "$3" ]; }; then echo "  ok: \"$1\" $got"
    else echo "  MISSING: \"$1\" $2${3:+ exit $3}, got: ${got:-no row}" >&2; fail=1; fi
  }
  [ -f "$RECORDS/run.tsv" ] || die "no records in ${RECORDS#"$REPO"/} (stage run)"
  say "== check: the records of ${RECORDS#"$REPO"/}"
  for f in run.tsv summary.txt chain.tsv programs.tsv instances.tsv environment.md logs/driver.log logs/sequencer.log; do
    check "records/$f is written" test -s "$RECORDS/$f"
  done
  local applied refused reverted dropped rejected unfinished phase n
  applied="$(status_count "$RECORDS/run.tsv" applied)"; refused="$(status_count "$RECORDS/run.tsv" refused)"
  reverted="$(status_count "$RECORDS/run.tsv" reverted)"; dropped="$(status_count "$RECORDS/run.tsv" dropped)"
  rejected="$(status_count "$RECORDS/run.tsv" rejected)"; unfinished="$(status_count "$RECORDS/run.tsv" unfinished)"
  [ "$dropped" = 0 ] && [ "$rejected" = 0 ] && [ "$unfinished" = 0 ] \
    && echo "  ok: nothing dropped, rejected or unfinished" \
    || { echo "  MISSING: no dropped, rejected or unfinished transaction, got $dropped, $rejected, $unfinished" >&2; fail=1; }
  [ "$reverted" = 2 ] && echo "  ok: exactly the two designed reverts" \
    || { echo "  MISSING: exactly the two designed reverts, got $reverted" >&2; fail=1; }
  [ "$refused" -ge 21 ] && echo "  ok: $refused refusals with the program's exit codes (21 designed)" \
    || { echo "  MISSING: the 21 designed refusals, got $refused" >&2; fail=1; }
  for phase in deploy fresh-follow fresh-scenarios fresh-consume $(grep -o 'fresh-private' "$OUT/phases.txt" || true); do
    n="$(awk -F'\t' -v p="$phase" 'NR > 1 && $1 == p && $6 == "applied"' "$RECORDS/run.tsv" | wc -l)"
    [ "$n" -ge 1 ] && echo "  ok: phase $phase: $n applied" || { echo "  MISSING: phase $phase applied nothing" >&2; fail=1; }
  done
  # Production: A initialised from the checkpoint and advanced by the finality update.
  want "fresh A Init Init" applied
  want "fresh A finality update Finish (step 3)" applied
  # The scenarios, by name.
  want "fresh twin Init adopt 1/2 with the wallet's default gas limit" reverted
  want "fresh malformed signature: Open (step 1)" refused 41
  want "fresh forged signature: Finish (step 3)" refused 40
  want "fresh Apply sent as the transaction root" refused
  want "fresh Init of an existing instance" refused
  want "fresh other chain period update Finish (step 3)" refused 40
  want "fresh Open before its signature slot (validity window)" reverted
  want "fresh finality update Finish (step 3)" applied
  want "fresh duplicate Open (same job)" refused
  want "fresh Open an applied update again" refused 52
  # Consumption, public and private.
  want "fresh Attest: the slot and the log in one request" applied
  want "fresh Attest refused: no block header for a record that binds a block hash" refused
  want "fresh Attest refused: a false value for the slot" refused
  want "fresh consumer CallAttest: the Transfer log" applied
  want "fresh consumer Claim: the ERC-20 Transfer (credits its sender once)" applied
  want "fresh consumer Claim again (replay)" refused
  if grep -q fresh-private "$OUT/phases.txt"; then
    want "fresh private attestation (consumer AttestPrivately), real proof" applied
  fi
  # The images: the final programs of docs/image-ids.md, as the sequencer's loader recorded them.
  local name doc got
  for name in main:ferrata_lez twin:ferrata_lez scripted:scripted consumer:ferrata_consumer; do
    doc="$(doc_image "${name#*:}")"
    got="$(awk -F'\t' -v n="${name%%:*}" '$1 == n { print $3 }' "$RECORDS/programs.tsv")"
    [ -n "$doc" ] && [ "$got" = "$doc" ] && echo "  ok: ${name%%:*} is ${name#*:} 0x${got:0:8}… (docs/image-ids.md)" \
      || { echo "  MISSING: ${name%%:*} deployed as ${name#*:} of docs/image-ids.md (0x$doc), got ${got:-none}" >&2; fail=1; }
  done
  check "the consumer deployed through the loader (its header and every segment applied)" consumer_deployed "$RECORDS"
  # A at the finality update's block, with the second provider's root.
  local want_slot want_root inst
  want_slot="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["finalizedSlot"])' "$FRESH/check.json")"
  want_root="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["finalizedRoot"][2:])' "$FRESH/check.json")"
  inst="$(awk -F'\t' 'NR > 1 && $1 ~ /^fresh A/' "$RECORDS/instances.tsv")"
  if [ -n "$inst" ] && [ "$(echo "$inst" | awk -F'\t' '{print $5 " " $7 " " $8}')" = "true $want_slot $want_root" ]; then
    echo "  ok: instance A active at finalized slot $want_slot, root 0x${want_root:0:16}… (the second provider's)"
  else
    echo "  MISSING: instance A active at slot $want_slot with root $want_root: ${inst:-no row}" >&2; fail=1
  fi
  echo "== $applied applied, $refused refused with an exit code, $reverted reverted, $dropped dropped, $rejected rejected, $unfinished unfinished"
  [ "$fail" = 0 ] || die "the records do not show every phase as designed (above)"
  say "check: every phase as designed"
}

# ---- summary -------------------------------------------------------------------------------------------------------
stage_summary() {
  python3 -I - "$OUT" "$LEZ_TAG" "${GITHUB_STEP_SUMMARY:-}" <<'PY' || echo "sequencer-attest: the summary could not be written" >&2
import json, os, sys
out, tag, step_summary = sys.argv[1:4]
rec = os.path.join(out, "records")
lines = ["# CI: attestation against a LEZ sequencer in standalone mode, on fresh Sepolia data", ""]
lines += ["RFP-021 Supportability #2 (job `sequencer-attest` of `.github/workflows/sequencer.yml`, script",
          "`scripts/ci/sequencer-attest.sh`): LEZ %s's `sequencer_service` built with its `standalone` feature (its mock" % tag,
          "Bedrock actor; no Logos Blockchain node), blocks every 3 s, `RISC0_DEV_MODE` unset; the Ethereum side is",
          "Sepolia, read from public endpoints when the run started.", ""]
check = os.path.join(out, "fresh", "check.json")
if os.path.exists(check):
    c = json.load(open(check))
    t = c["transfer"]
    lines += ["| | |", "|---|---|",
              "| checkpoint | Sepolia slot %s (%s, period %s, %s layout), block root `%s`, the second provider's; Init valid until %s |"
              % (c["bootstrapSlot"], c["bootstrapTime"], c["bootstrapPeriod"], c["layout"], c["checkpoint"], c["initValidUntil"]),
              "| period updates | %s |" % ", ".join("%s (finalized slot %s; advanced %s, adopted %s)" % (p["period"], p["finalizedSlot"], p["advanced"], p["adopted"]) for p in c["periods"]),
              "| finality update | finalizes slot %s (%s), root `%s` (the second provider's), signature slot %s, %s/512 signers |"
              % (c["finalizedSlot"], c["finalizedTime"], c["finalizedRoot"], c["signatureSlot"], c["signers"]),
              "| its execution block | %s, hash `%s` |" % (c["block"], c["blockHash"]),
              "| storage fact | `%s` slot `%s` = `%s` (Sepolia USDC `totalSupply_`, equal to `totalSupply()` at the block) |"
              % (c["storage"]["contract"], c["storage"]["slot"], c["storage"]["value"]),
              "| log fact | ERC-20 Transfer, tx %s log %s: token `%s`, from `%s` to `%s`, amount %s |"
              % (t["txIndex"], t["logIndex"], t["token"], t["from"], t["to"], t["amount"]), ""]
else:
    lines += ["No checked Sepolia data: the run stopped before it (fresh/fetch.log).", ""]
run = os.path.join(rec, "run.tsv")
if os.path.exists(run):
    rows = [l.rstrip("\n").split("\t") for l in open(run)][1:]
    phases = []
    for r in rows:
        if r[0] not in phases:
            phases.append(r[0])
    lines += ["| phase | sent | applied | refused (exit code) | reverted | other |", "|---|---:|---:|---:|---:|---:|"]
    for p in phases:
        st = [r[5] for r in rows if r[0] == p]
        lines.append("| %s | %d | %d | %d | %d | %d |" % (p, len(st), st.count("applied"), st.count("refused"), st.count("reverted"),
                     len(st) - st.count("applied") - st.count("refused") - st.count("reverted")))
    lines.append("")
else:
    lines += ["No run records: the run stopped before the sequencer started.", ""]
log = os.path.join(rec, "logs", "driver.log")
failed = [l.strip() for l in open(log)] if os.path.exists(log) else []
failed = [l for l in failed if l.startswith("== phase") and "FAILED" in l]
phases_file = os.path.join(out, "phases.txt")
if os.path.exists(phases_file):
    lines.append("- phases: `%s`" % open(phases_file).read().strip())
lines.append("- outcome: " + ("**%s**" % failed[-1] if failed else "every phase ran to its end (the check step asserts each outcome)"))
lines += ["- records: `records/` (run.tsv the manifest, summary.txt, chain.tsv, blocks.borsh, programs.tsv, instances.tsv,",
          "  environment.md, logs/, private/), `fresh/` (the fetched data, each directory pinned by its SHA256SUMS; check.json,",
          "  fetch.log), `sequencer-attest.log`", ""]
text = "\n".join(lines) + "\n"
open(os.path.join(out, "README.md"), "w").write(text)
if step_summary:
    open(step_summary, "a").write(text)
print(text)
PY
}

for stage in $STAGES; do
  "stage_$stage"
done
