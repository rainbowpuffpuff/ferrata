#!/usr/bin/env bash
# The four Halmos properties that did not finish in 900 s in the run of 11 November
# (docs/reviews/security/tools-2026-11-11.md §3.1), re-run with a longer budget: one Halmos process per
# property and storage layout, in parallel, each bounded by HALMOS_LONG_TIMEOUT seconds (default 4800).
#
# Usage: scripts/halmos-long.sh install     Halmos 0.3.3 alone (uv, Python 3.12) into $SECTOOLS_DIR
#        scripts/halmos-long.sh run         the runs below; logs in $SECTOOLS_OUT/halmos-long/<run>/halmos.log
#        scripts/halmos-long.sh summary     one line per run: PASS / FAIL / not finished, paths, time
# Same tool, pins, local fixes and bounds as scripts/security-tools.sh (whose `halmos` step this calls per run);
# only the time budget differs. Needs about 16 cores; meant for the build box (spike/modal-build/mrun).
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
export SECTOOLS_DIR=${SECTOOLS_DIR:-$HOME/.cache/ferrata-sectools}
BASE=${SECTOOLS_OUT:-$ROOT/docs/reviews/security/tools-out}/halmos-long
T=${HALMOS_LONG_TIMEOUT:-4800}
# Unbuffered: Halmos prints its results only at the end, and `timeout` stops Python without a flush, so a buffered
# run that hits its bound leaves nothing; unbuffered, everything printed so far stays in the log.
export PYTHONUNBUFFERED=1
# name | Halmos --function regex | extra options. The function filter is a regex searched in the signature,
# so "\(" after the name keeps check_no_role_call_moves_funds from also matching its _after_retire twin.
RUNS=(
  "after_stop_only_owed_leaves-solidity|check_after_stop_only_owed_leaves\\(|"
  "no_role_call_moves_funds-solidity|check_no_role_call_moves_funds\\(|"
  "no_role_call_moves_funds_after_retire-solidity|check_no_role_call_moves_funds_after_retire\\(|"
  "role_entry_points_move_no_funds-solidity|check_role_entry_points_move_no_funds\\(|"
  "no_role_call_moves_funds-generic|check_no_role_call_moves_funds\\(|--storage-layout generic"
  "no_role_call_moves_funds_after_retire-generic|check_no_role_call_moves_funds_after_retire\\(|--storage-layout generic"
)

install_() {
  export PATH="$SECTOOLS_DIR/bin:$SECTOOLS_DIR/venv/bin:$PATH"
  mkdir -p "$SECTOOLS_DIR/bin"
  command -v uv >/dev/null ||
    curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR="$SECTOOLS_DIR/bin" UV_NO_MODIFY_PATH=1 sh
  [ -x "$SECTOOLS_DIR/venv/bin/python" ] || uv venv --python 3.12 "$SECTOOLS_DIR/venv"
  uv pip install --python "$SECTOOLS_DIR/venv/bin/python" "halmos==${HALMOS_VERSION:-0.3.3}"
  "$SECTOOLS_DIR/venv/bin/halmos" --version
}

run_() {
  local r name fn opts first="" i
  for r in "${RUNS[@]}"; do
    IFS='|' read -r name fn opts <<<"$r"
    mkdir -p "$BASE/$name"
    ( SECTOOLS_OUT=$BASE/$name HALMOS_ARGS="--function $fn $opts" HALMOS_TIMEOUT=$T \
        "$ROOT/scripts/security-tools.sh" halmos > "$BASE/$name/runner.log" 2>&1
      echo "$name: exit $? after $SECONDS s" >> "$BASE/$name/runner.log" ) &
    # The first run compiles the vault (forge build, as Halmos calls it); the others start once it is running,
    # so that six forge builds do not write the same out/ and cache/ at once (at most 600 s).
    if [ -z "$first" ]; then
      first=$name
      for i in $(seq 120); do grep -q -E 'Running [0-9]+ tests' "$BASE/$first/halmos.log" 2>/dev/null && break; sleep 5; done
    fi
    sleep 5
  done
  wait
  summary_
}

summary_() {
  local d
  for d in "$BASE"/*/; do
    printf '%s: ' "$(basename "$d")"
    if sed 's/\x1b\[[0-9;]*m//g' "$d/halmos.log" 2>/dev/null | grep -E '^\[(PASS|FAIL|TIMEOUT|ERROR)' | cut -c1-200 | tr '\n' ' ' | grep .; then :
    else echo "no PASS/FAIL line (not finished within its budget): $(tail -n 1 "$d/runner.log" 2>/dev/null)"; fi
  done
}

for step in "$@"; do
  case $step in
    install) install_ ;;
    run) run_ ;;
    summary) summary_ ;;
    *) echo "unknown step $step" >&2; exit 2 ;;
  esac
done
