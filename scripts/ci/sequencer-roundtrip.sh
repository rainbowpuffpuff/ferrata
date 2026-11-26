#!/usr/bin/env bash
# CI's bridge round trip through a real, running LEZ sequencer in standalone mode and a local fork of Sepolia
# (RFP-022 Reliability #10: "End-to-end integration tests exercise the full deposit and redemption round trip against
# a LEZ sequencer (standalone mode) and an Ethereum test network or local fork, and are included in CI.").
#
#   scripts/ci/sequencer-roundtrip.sh RESULTS-DIR [STAGE...]
#
# STAGE is one of build, run, check, summary; without one, all four in this order. The CI job
# (.github/workflows/sequencer.yml, job `sequencer-roundtrip`) runs one stage per step, each with its own timeout; the
# build box (never a laptop) runs them in one call. For the stage run, RESULTS-DIR must be
# under e2e/runs/ (forge reads the vault's parameters there: vault/foundry.toml's fs_permissions).
#
#   build    the tools, from this tree: the vault (forge), the release guest and ferrata-e2e (bridge/), the rule (B)
#            checkpoint guest and ci_zone (checkpoint/), the LEZ guests (the attestation program and the bridge program
#            of docs/image-ids.md), LEZ v0.4.0-rc1's sequencer_service with its `standalone` feature, crown-driver
#            (lez/crown/driver), and the `ferrata` tool with ferrata-relayer, seqnet-fund and seqnet-txs (cli/)
#   run      lez/live/run.sh run with FERRATA_LIVE_PHASES=hook: the standalone sequencer (its mock Bedrock actor, no
#            Logos Blockchain node; blocks every 3 s; RISC0_DEV_MODE unset), and while it runs
#            scripts/ci/sequencer-roundtrip-hook.sh: anvil forking Sepolia at its finalized block of the moment, the
#            vault and the round trip, every step asserted (that script's header lists them)
#   check    the records the hook left: steps.tsv is exactly the hook's steps in order, each once and ok (no FAILED
#            line); the facts it asserted (facts.json, and the replay's eth_call output, eth/replay.txt); the fork's
#            block under an hour old when forked and under three hours by this check's clock (FERRATA_CI_NOW, unix
#            seconds, stands for the clock in the tests of the check)
#   summary  RESULTS-DIR/README.md and, under GitHub Actions, the job summary; written whatever happened; never fails
#
# What is real and what stands in, exactly (also in the README it writes):
# - LEZ: LEZ v0.4.0-rc1's own sequencer_service, built from the unmodified tag (the tag the `ferrata` tool's sequencer
#   path, cli/, is built against); every LEZ transaction is built, signed and sent by LEZ's wallet library over the
#   sequencer's RPC (crown-driver for the operator, the `ferrata` tool for the user, seqnet-fund for the fee stand-in).
# - Ethereum: anvil forking Sepolia at a recent finalized block, read from a public Sepolia RPC; the vault (v5, rule
#   (B)) deployed on the fork by vault/script/deploy.sh (DeployVault), as on Sepolia. Nothing is sent to Sepolia:
#   anvil keeps every transaction local, and the deployer is impersonated (no key).
# - STAND-IN, Ethereum consensus: anvil has no beacon chain, so a 512-key committee the run holds finalizes the fork's
#   blocks for LEZ's light client (crown-driver's stand-in chain, as e2e/seqnet/ and the crown rehearsals; its slots
#   are recent ones, CROWN_STAND_IN_SLOT, so that the instance's Init is inside its weak-subjectivity window).
# - STAND-IN, the vault's verifier: the vault tests' MockRiscZeroVerifier behind RISC Zero's emergency stop, told to
#   accept exactly the journals of the rule (B) checkpoint guest and of the release guest, each executed in RISC Zero
#   dev mode and equal to the native journal; no Groth16 proof is made (as the `roundtrip` job of ci.yml).
# - STAND-IN, the checkpoint's chain: rule (B) needs blocks inscribed on Logos Blockchain, which a standalone
#   sequencer does not do; ci_zone (checkpoint/fidelity) puts every transaction the sequencer applied, block by
#   block, into LEZ v0.4.0-rc1 blocks inscribed by four test sequencer keys and attested by three test attestation
#   keys (n = 4, t = 3), as the `roundtrip` job does with the harness's transactions.
# - STAND-IN, the gas drop: seqnet-fund pays the user's public account /0 its LEZ fees from a funded genesis account of
#   LEZ's local network (public transfer).
#
# Needs: Rust 1.93.0 and 1.98.1, RISC Zero's toolchains (scripts/ci/install-risc0.sh: ~/.risc0 for bridge/ and
# checkpoint/, $RISC0_HOME_LEZ for LEZ), Foundry 1.3.6 (forge, anvil, cast), the upstream sources beside the
# repository (scripts/fetch-upstream.sh), libpcsclite-dev, protobuf-compiler, python3, jq, curl.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
die() { echo "sequencer-roundtrip: $*" >&2; exit 1; }

OUT="${1:?usage: scripts/ci/sequencer-roundtrip.sh RESULTS-DIR (under e2e/runs/) [build|run|check|summary]...}"
shift
STAGES="${*:-build run check summary}"
for s in $STAGES; do
  case "$s" in build|run|check|summary) ;; *) die "unknown stage $s" ;; esac
done
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
case " $STAGES " in
  *" run "*) case "$OUT" in "$REPO"/e2e/runs/*) ;; *) die "RESULTS-DIR must be under e2e/runs/ (vault/foundry.toml's fs_permissions)" ;; esac ;;
esac
LOG="$OUT/sequencer-roundtrip.log"
touch "$LOG"
# shellcheck source=scripts/ci/sequencer-records.sh
. "$REPO/scripts/ci/sequencer-records.sh"
trap 'scrub_paths "$OUT" "$BESIDE" "$HOME" "$REPO"' EXIT

# The LEZ tag of the sequencer, the crown driver and the tool: v0.4.0-rc1 in CI; e2e/rc3-roundtrip/run.sh runs the same
# round trip against v0.4.0-rc3 (FERRATA_CI_LEZ_TAG), with tools it builds itself (only the run, check and summary stages).
export LEZ_TAG="${FERRATA_CI_LEZ_TAG:-v0.4.0-rc1}"
# One scratch directory beside the repository, and one target directory in it for LEZ's sequencer, crown-driver and
# cli/: they build LEZ v0.4.0-rc1's crates from the same sources, which cargo then shares.
T="${FERRATA_CI_ROUNDTRIP_TMP:-$BESIDE/tmp/ci-seq-roundtrip}"
export FERRATA_LIVE_TMP="$T"
TOOLS="$T/tools"
TGT="$T/target"
export PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$HOME/.risc0/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}" CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
unset RISC0_DEV_MODE

T0=$(date +%s)
say() { echo "[sequencer-roundtrip $(date -u +%T)] $*"; echo "[$(date -u +%T)] $*" >> "$LOG"; }
minutes() { echo "$(( ($(date +%s) - T0) / 60 )) min"; }
STEP_LOG="$LOG"
run_logged() {
  local name="$1"; shift
  say "$name (output to ${STEP_LOG#"$REPO"/})"
  local rc=0
  "$@" >> "$STEP_LOG" 2>&1 || rc=$?
  if [ "$rc" != 0 ]; then
    echo "== $name FAILED (exit $rc); the last 40 lines of ${STEP_LOG#"$REPO"/}:" >&2
    tail -n 40 "$STEP_LOG" | sed -e "s/::/: :/g" >&2
    for f in "$OUT"/zone/logs/hook.log "$OUT"/zone/logs/sequencer.log; do
      if [ -f "$f" ]; then echo "-- ${f#"$REPO"/}:" >&2; tail -n 25 "$f" | sed -e "s/::/: :/g" >&2; fi
    done
    bash "$REPO/lez/live/run.sh" stop > /dev/null 2>&1 || true
    exit "$rc"
  fi
}

lez_home() {
  local d
  if [ -n "${RISC0_HOME_LEZ:-}" ]; then echo "$RISC0_HOME_LEZ"; return; fi
  for d in "$BESIDE/risc0-home-lez" "$HOME/risc0-home-lez"; do
    if [ -d "$d" ]; then (cd "$d" && pwd); return; fi
  done
  die "no RISC Zero home with guest Rust 1.91.1 for LEZ (set RISC0_HOME_LEZ; scripts/ci/install-risc0.sh)"
}

NPROC="$(nproc)"
if [ "$NPROC" -lt 4 ]; then NPROC=4; fi
if [ "$NPROC" -gt 16 ]; then NPROC=16; fi
export FERRATA_LIVE_JOBS="${FERRATA_LIVE_JOBS:-$NPROC}" CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$NPROC}"

# ---- build ---------------------------------------------------------------------------------------------------------
stage_build() {
  local LH; LH="$(lez_home)"
  # rzup's homes, as scripts/ci/install-risc0.sh fills them: r0vm 3.0.5 for LEZ's crates (the LEZ home), 3.1.0 for
  # ferrata-e2e (~/.risc0). Neither is on a runner's PATH; each program finds its own through RISC0_HOME.
  r0vm_of "$LH" 3.0.5 > /dev/null || die "no r0vm 3.0.5 in $LH or on PATH (scripts/ci/install-risc0.sh)"
  r0vm_of "$HOME/.risc0" 3.1.0 > /dev/null || die "no r0vm 3.1.0 in ~/.risc0 (scripts/ci/install-risc0.sh)"
  command -v forge > /dev/null && command -v anvil > /dev/null && command -v cast > /dev/null || die "Foundry (forge, anvil, cast) is missing"
  [ -f "$BESIDE/upstream/lez-v0.4.0-rc1/Cargo.toml" ] || die "LEZ v0.4.0-rc1 is not beside the repository (scripts/fetch-upstream.sh)"
  [ "$(bash "$REPO/lez/scripts/lez-select.sh" --show)" = v0.4.0-rc1 ] || die "the tree's manifests do not point at LEZ v0.4.0-rc1"
  mkdir -p "$TOOLS"
  STEP_LOG="$OUT/build.log"
  say "== build (started $(date -u +%Y-%m-%dT%H:%M:%SZ)); $CARGO_BUILD_JOBS build jobs"
  run_logged "vault: libraries and build" bash -c 'cd "$1/vault" && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.1.0; } && python3 -I "$1/scripts/vault_libs.py" check && forge build' _ "$REPO"
  run_logged "bridge: the release guest (guest Rust 1.88.0) and ferrata-e2e" \
    bash -c 'cd "$1/bridge" && env -u RISC0_HOME cargo build --release --locked -p methods && env -u RISC0_HOME cargo build --release --locked -p ferrata-e2e' _ "$REPO"
  run_logged "checkpoint: the rule (B) guest (guest Rust 1.88.0)" \
    bash -c 'cd "$1/checkpoint" && env -u RISC0_HOME cargo build --release --locked -p methods' _ "$REPO"
  # The native archives that LEZ's crates link (rapidsnark, the Logos Blockchain circuits): fetched once and checked
  # against scripts/native-deps.json, as e2e/ci/run.sh does, instead of the build scripts' own unchecked downloads;
  # after each build, fetch_native.py check-build reads cargo's JSON messages: nothing else was linked.
  local vars
  vars="$(python3 -I "$REPO/scripts/fetch_native.py" env 2>> "$STEP_LOG")" || die "scripts/fetch_native.py env failed (build.log)"
  eval "$vars"
  local zone_msgs="$T/ci_zone-build.jsonl" lez_msgs="$T/lez-build.jsonl"
  rm -f "$zone_msgs" "$lez_msgs"
  run_logged "checkpoint: ci_zone (Rust 1.98.1)" \
    bash -c 'cd "$1/checkpoint/fidelity" && cargo +1.98.1 build --release --locked --bin ci_zone --message-format=json-render-diagnostics > "$2"' _ "$REPO" "$zone_msgs"
  run_logged "checkpoint: ci_zone linked native code only from the checked archives (fetch_native.py check-build)" \
    python3 -I "$REPO/scripts/fetch_native.py" check-build "$zone_msgs" --target-dir "$REPO/checkpoint/target"
  run_logged "LEZ guests: the attestation and bridge programs (the committed v0.4.0-rc1 manifests)" \
    bash -c 'cd "$1/lez" && RISC0_HOME="$2" RISC0_BUILD_LOCKED=1 RUSTC_WRAPPER="$1/scripts/guest-rustc" cargo build -p methods --locked' _ "$REPO" "$LH"
  run_logged "LEZ v0.4.0-rc1's sequencer_service (standalone)" \
    env FERRATA_LIVE_CARGO_MESSAGES="$lez_msgs" bash "$REPO/lez/live/run.sh" build-standalone
  run_logged "crown-driver (lez/crown/driver)" \
    bash -c 'cd "$1/lez/crown/driver" && CARGO_TARGET_DIR="$2" cargo build --release --locked --message-format=json-render-diagnostics >> "$3"' _ "$REPO" "$TGT" "$lez_msgs"
  run_logged "cli: ferrata, ferrata-relayer, seqnet-fund, seqnet-txs" \
    bash -c 'cd "$1/cli" && RISC0_HOME="$3" CARGO_TARGET_DIR="$2" cargo build --release --locked --features seqnet --bin ferrata --bin ferrata-relayer --bin seqnet-fund --bin seqnet-txs --message-format=json-render-diagnostics >> "$4"' _ "$REPO" "$TGT" "$LH" "$lez_msgs"
  run_logged "the sequencer, crown-driver and cli linked native code only from the checked archives (fetch_native.py check-build)" \
    python3 -I "$REPO/scripts/fetch_native.py" check-build "$lez_msgs" --target-dir "$TGT"
  local f
  for f in ferrata ferrata-relayer seqnet-fund seqnet-txs crown-driver; do cp "$TGT/release/$f" "$TOOLS/"; done
  cp "$REPO/checkpoint/target/release/ci_zone" "$REPO/bridge/target/release/ferrata-e2e" "$TOOLS/"
  find_one() { find "$1" -path '*riscv32im-risc0-zkvm-elf/release/*' -name "$2" -print -quit 2>/dev/null; }
  for f in "$REPO/bridge/target/riscv-guest:release.bin" "$REPO/checkpoint/target/riscv-guest:checkpoint_b.bin" \
           "$REPO/lez/target/riscv-guest:ferrata_lez.bin" "$REPO/lez/target/riscv-guest:ferrata_bridge.bin" \
           "$REPO/lez/target/riscv-guest:scripted.bin"; do
    local got; got="$(find_one "${f%%:*}" "${f#*:}")"
    [ -n "$got" ] || die "the guest ${f#*:} was not built under ${f%%:*}"
    cp "$got" "$TOOLS/${f#*:}"
  done
  STEP_LOG="$LOG"
  ( cd "$TOOLS" && sha256sum ./* ) > "$OUT/tools.sha256"
  say "build took $(minutes)"
}

# ---- run -----------------------------------------------------------------------------------------------------------
stage_run() {
  local f
  for f in ferrata ferrata-relayer seqnet-fund seqnet-txs crown-driver ci_zone ferrata-e2e release.bin checkpoint_b.bin \
           ferrata_lez.bin ferrata_bridge.bin scripted.bin; do
    [ -e "$TOOLS/$f" ] || die "$f is not built (stage build)"
  done
  local LH; LH="$(lez_home)"
  # LEZ's sequencer, crown-driver and the tool are on risc0-zkvm 3.0.5: its r0vm is in the LEZ home (a runner keeps it
  # in ~/risc0-home-lez, which lez/live/env.sh does not look for; ~/.risc0 holds 3.1.0 for bridge/ and checkpoint/).
  export RISC0_HOME="$LH"
  local r0vm
  r0vm="$(r0vm_of "$LH" 3.0.5)" || die "no r0vm 3.0.5 in $LH or on PATH (scripts/ci/install-risc0.sh)"
  export PATH="$(dirname "$r0vm"):$PATH"
  CI_RT_R0VM31="$(r0vm_of "$HOME/.risc0" 3.1.0)" || die "no r0vm 3.1.0 in ~/.risc0 (scripts/ci/install-risc0.sh)"
  export CI_RT_R0VM31
  rm -rf "$OUT/zone" "$OUT/run"
  mkdir -p "$OUT/run"
  export CI_RT_REPO="$REPO" CI_RT_OUT="$OUT/run" CI_RT_WORK="$T/work" CI_RT_TOOLS="$TOOLS" CI_RT_LEZ_HOME="$LH"
  # Public Sepolia RPCs, tried in this order (three attempts each, 30 s apart); anvil forks the one that answered.
  export CI_RT_SEPOLIA_RPCS="${FERRATA_CI_SEPOLIA_RPCS:-https://ethereum-sepolia-rpc.publicnode.com https://rpc.sepolia.ethpandaops.io}"
  export FERRATA_LIVE_BEDROCK=mock FERRATA_LIVE_BLOCK_TIME=3s FERRATA_LIVE_PHASES=hook
  export FERRATA_LIVE_HOOK="$REPO/scripts/ci/sequencer-roundtrip-hook.sh" FERRATA_LIVE_OUT="$OUT/zone"
  export FERRATA_PROGRAM_BIN="$TOOLS/ferrata_lez.bin" FERRATA_SCRIPTED_BIN="$TOOLS/scripted.bin"
  say "== run: LEZ $LEZ_TAG's sequencer (standalone: its mock Bedrock, no Logos Blockchain node), blocks every 3 s; the round trip in its hook"
  run_logged "run: LEZ sequencer (standalone) + scripts/ci/sequencer-roundtrip-hook.sh" bash "$REPO/lez/live/run.sh" run
  say "run done at $(minutes)"
}

# ---- check ---------------------------------------------------------------------------------------------------------
stage_check() {
  local R="$OUT/run" fail=0
  [ -f "$R/steps.tsv" ] || die "no steps.tsv in ${R#"$REPO"/} (stage run)"
  say "== check: the records of ${R#"$REPO"/}"
  for f in steps.tsv facts.json identity.json params.json deploy.json lez/transactions.tsv eth/transactions.tsv \
           checkpoint/cp-ci-b.json; do
    if [ -s "$R/$f" ]; then echo "  ok: run/$f is written"; else echo "  MISSING: run/$f" >&2; fail=1; fi
  done
  # The hook's steps, in its order, each exactly once and ok: a step that failed and went on would leave a FAILED line.
  local want="fork|zone: programs and the stand-in instance|vault deployed on the fork|deposit|operator import|fees|claim (plaintext)|claim again refused|burn|checkpoint journal|checkpoint accepted|release journal|release paid|release again refused"
  local got
  got="$(awk -F'\t' '{ printf "%s%s", (NR > 1 ? "|" : ""), $1 }' "$R/steps.tsv")"
  if [ "$got" = "$want" ]; then echo "  ok: the 14 steps of the round trip, in order, once each"
  else echo "  MISSING: the steps \"${want//|/\", \"}\" in this order, once each; steps.tsv has \"${got//|/\", \"}\"" >&2; fail=1; fi
  if awk -F'\t' '$2 != "ok" { bad = 1; print "  MISSING: a step not ok: " $0 > "/dev/stderr" } END { exit bad }' "$R/steps.tsv"; then
    echo "  ok: every line of steps.tsv is ok"
  else fail=1; fi
  python3 -I - "$R/facts.json" "$R/eth/replay.txt" "${FERRATA_CI_NOW:-$(date -u +%s)}" <<'PY' || fail=1
import datetime, json, os, sys
f = json.load(open(sys.argv[1]))
replay_out = open(sys.argv[2]).read().lower() if os.path.exists(sys.argv[2]) else ""
now = int(sys.argv[3])
r, c, d, k = f["release"], f["checkpoint"], f["deposit"], f["fork"]
fork_t = datetime.datetime.strptime(k["time"], "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=datetime.timezone.utc).timestamp()
rp = r.get("replay", {})
# NullifierAlreadyUsed() (vault/src), `cast sig`: 0xcad2ae02
refused = rp.get("callExit", 0) != 0 and rp.get("revertSelector") == "0xcad2ae02" and "0xcad2ae02" in replay_out
need = {
    "the fork's block was under an hour old when the run forked it (the hook's clock)":
        -300 <= int(k.get("ageS", 10**9)) <= 3600,
    "the fork's block is under three hours old by this check's clock": now - fork_t <= 3 * 3600,
    "the holder was minted the denomination, then debited all of it":
        f["claim"]["holdingAfterClaim"] == d["denominationWei"] and f["burn"]["holdingAfterBurn"] == "0",
    "the checkpoint counted the one burn": int(c["burnCount"]) == 1,
    "the vault's final state is the checkpoint's new state": c["vaultFinalState"].lower() == c["newState"].lower(),
    "the vault accepted the checkpoint's burn root": c["burnRootAccepted"] is True,
    "the vault's Release event pays out the denomination burnt": r["amountWei"] == d["denominationWei"],
    "the fresh address holds that amount less the relay fee and the credited fees":
        int(r["recipientBalanceWei"]) == int(r["amountWei"]) - int(r["relayFeeWei"]) - int(r["creditedFeesWei"])
        and int(r["recipientBalanceWei"]) > 0,
    "the same release again refused: its eth_call reverted with NullifierAlreadyUsed (eth/replay.txt)": refused,
    "the release's nullifier is marked used": rp.get("nullifierUsed") is True,
    "facts.json's replayRefused agrees with the replay's own result":
        r.get("replayRefused") is (refused and rp.get("nullifierUsed") is True),
}
bad = [k for k, v in need.items() if not v]
for k, v in need.items():
    print(("  ok: " if v else "  MISSING: ") + k, file=sys.stdout if v else sys.stderr)
sys.exit(1 if bad else 0)
PY
  [ "$fail" = 0 ] || die "the records do not show the round trip as designed (above)"
  say "check: every step of the round trip as designed"
}

# ---- summary -------------------------------------------------------------------------------------------------------
stage_summary() {
  python3 -I - "$OUT" "${GITHUB_STEP_SUMMARY:-}" "$LEZ_TAG" <<'PY' || echo "sequencer-roundtrip: the summary could not be written" >&2
import json, os, sys
out, step_summary, lez_tag = sys.argv[1:4]
run = os.path.join(out, "run")
lines = ["# CI: the bridge round trip through a LEZ sequencer in standalone mode and a local fork of Sepolia", "",
         "RFP-022 Reliability #10 (job `sequencer-roundtrip` of `.github/workflows/sequencer.yml`, script",
         "`scripts/ci/sequencer-roundtrip.sh`): LEZ %s's `sequencer_service` built with its `standalone` feature (its" % lez_tag,
         "mock Bedrock actor; no Logos Blockchain node), blocks every 3 s; anvil forking Sepolia at its finalized block of",
         "the moment. Stand-ins, exactly: the light client's committee (anvil has no beacon chain), the vault's verifier",
         "(the vault tests' `MockRiscZeroVerifier`, accepting only the journals the checkpoint and release guests computed",
         "in RISC Zero dev mode, each equal to the native one), the checkpoint's chain (the sequencer's applied",
         "transactions inscribed by `ci_zone`'s test keys), the gas drop (`seqnet-fund`).", ""]
facts = os.path.join(run, "facts.json")
if os.path.exists(facts):
    f = json.load(open(facts))
    k = f.get("fork", {})
    lines += ["| | |", "|---|---|",
              "| Sepolia fork | block %s (%s, %s s old when forked), hash `%s`, read from %s |" % (k.get("block"), k.get("time"), k.get("ageS", "?"), k.get("hash"), k.get("rpc", "a public Sepolia RPC")),
              "| vault | `%s` (rule B), configId `%s` |" % (f.get("vault"), f.get("configId")),
              "| deposit | %s wei, Ethereum tx `%s` |" % (f["deposit"]["denominationWei"], f["deposit"]["tx"]),
              "| claim | LEZ tx `%s`; holding %s |" % (f["claim"]["tx"], f["claim"]["holdingAfterClaim"]),
              "| burn | LEZ tx `%s` to `%s` |" % (f["burn"]["tx"], f["burn"]["recipient"]),
              "| checkpoint | burn root `%s`, %s burn(s), tip %s |" % (f["checkpoint"]["burnRoot"], f["checkpoint"]["burnCount"], f["checkpoint"]["tipId"]),
              "| release | Ethereum tx `%s`; the fresh address holds %s wei |" % (f["release"]["tx"], f["release"]["recipientBalanceWei"]), ""]
steps = os.path.join(run, "steps.tsv")
if os.path.exists(steps):
    rows = [l.rstrip("\n").split("\t") for l in open(steps) if l.strip()]
    lines += ["| step | result | seconds | detail |", "|---|---|---:|---|"]
    for r in rows:
        r += [""] * (4 - len(r))
        lines.append("| %s | %s | %s | %s |" % (r[0], r[1], r[2], r[3].replace("|", "/")))
    lines.append("")
else:
    lines += ["No steps recorded: the run stopped before the hook began (sequencer-roundtrip.log).", ""]
lines += ["- records: `run/` (steps.tsv, facts.json, the vault's identity, params and deploy record, `lez/` the zone's",
          "  transactions, `eth/` the fork's, `user/` what the user ran and saw, `checkpoint/`), `zone/` (the sequencer's",
          "  configuration, logs and environment), `sequencer-roundtrip.log`", ""]
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
