#!/usr/bin/env bash
# Industry-standard security tooling for Ferrata, re-runnable by anyone:
#   Solidity (vault/): Slither (every detector), Aderyn, Halmos (symbolic tests in vault/test/halmos/)
#   Rust (every Cargo.lock in the repository): cargo-audit; the main workspaces: cargo-deny
#   (advisories, licences, sources, bans) with the policy in deny.toml.
#
# Usage: scripts/security-tools.sh <step>...
#   steps: install versions slither aderyn halmos halmos-groups audit deny all
#   (halmos runs every check in one process; halmos-groups is the parallel, time-bounded run the note records)
#   SECTOOLS_DIR  where the tools are installed (default ~/.cache/ferrata-sectools)
#   SECTOOLS_OUT  where the reports are written (default notes/reviews/security/tools-out)
# Needs: Foundry (forge, with solc 0.8.26 available offline, as vault/foundry.toml pins), Rust/cargo, curl,
# the vault's Foundry libraries in vault/lib (vault/README.md lists the pins). The triage of every finding is
# in notes/reviews/security/tools-2026-11-11.md.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
TOOLS=${SECTOOLS_DIR:-$HOME/.cache/ferrata-sectools}
OUT=${SECTOOLS_OUT:-$ROOT/notes/reviews/security/tools-out}
export PATH="$TOOLS/bin:$TOOLS/venv/bin:$PATH"

# Versions of the 2026-11-11 run (install pins them; set one to empty for the latest).
SLITHER_VERSION=${SLITHER_VERSION:-0.11.6}
HALMOS_VERSION=${HALMOS_VERSION:-0.3.3}
ADERYN_VERSION=${ADERYN_VERSION:-0.6.8}
CARGO_AUDIT_VERSION=${CARGO_AUDIT_VERSION:-0.23.0}
CARGO_DENY_VERSION=${CARGO_DENY_VERSION:-0.20.2}

# Reports name paths relative to the repository and the home directory as ~, so they carry no machine paths.
scrub() { for f in "$@"; do [ -f "$f" ] && sed -i -e "s|$ROOT/|./|g" -e "s|$ROOT|.|g" -e "s|$HOME|~|g" "$f"; done; }

pin() { [ -n "$2" ] && printf '%s==%s' "$1" "$2" || printf '%s' "$1"; }
cpin() { [ -n "$2" ] && printf -- '--version %s' "$2"; }

install() {
  mkdir -p "$TOOLS/bin"
  # Python tools in their own virtual environment (Python 3.12 through uv: halmos needs >= 3.11).
  if ! command -v uv >/dev/null; then
    curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR="$TOOLS/bin" UV_NO_MODIFY_PATH=1 sh
  fi
  ( [ -x "$TOOLS/venv/bin/python" ] || uv venv --python 3.12 "$TOOLS/venv" ) &&
    uv pip install --python "$TOOLS/venv/bin/python" "$(pin slither-analyzer "$SLITHER_VERSION")" \
      "$(pin halmos "$HALMOS_VERSION")" &
  local p1=$!
  # Rust tools from crates.io, each in its own root (they build in parallel), linked into $TOOLS/bin.
  for c in cargo-audit:"$CARGO_AUDIT_VERSION" cargo-deny:"$CARGO_DENY_VERSION"; do
    name=${c%%:*}; ver=${c#*:}
    ( command -v "$name" >/dev/null && [ -z "$ver" ] ) ||
      ( cargo install --locked --root "$TOOLS/$name" $(cpin "$name" "$ver") "$name" &&
        ln -sf "$TOOLS/$name/bin/$name" "$TOOLS/bin/$name" ) &
  done
  # Aderyn: the release binary (cargo-dist archive); crates.io as a fallback.
  if ! command -v aderyn >/dev/null; then
    local url=https://github.com/Cyfrin/aderyn/releases/latest/download/aderyn-x86_64-unknown-linux-gnu.tar.xz
    [ -n "$ADERYN_VERSION" ] &&
      url=https://github.com/Cyfrin/aderyn/releases/download/aderyn-v$ADERYN_VERSION/aderyn-x86_64-unknown-linux-gnu.tar.xz
    ( mkdir -p "$TOOLS/aderyn" && curl -fLsS "$url" | tar -xJ -C "$TOOLS/aderyn" --strip-components=1 &&
      ln -sf "$TOOLS/aderyn/aderyn" "$TOOLS/bin/aderyn" ) ||
      ( cargo install --locked --root "$TOOLS/aderyn-src" $(cpin aderyn "$ADERYN_VERSION") aderyn &&
        ln -sf "$TOOLS/aderyn-src/bin/aderyn" "$TOOLS/bin/aderyn" )
  fi
  wait
  wait $p1
  versions
}

versions() {
  for t in forge slither halmos aderyn cargo-audit cargo-deny; do
    case $t in
      forge) v=$(forge --version 2>&1 | head -1) ;;
      slither) v=$(slither --version 2>&1 | tail -1) ;;
      halmos) v=$(halmos --version 2>&1 | tail -1) ;;
      aderyn) v=$(aderyn --version 2>&1 | tail -1) ;;
      cargo-audit) v=$(cargo-audit --version 2>&1 | tail -1) ;;
      cargo-deny) v=$(cargo-deny --version 2>&1 | tail -1) ;;
    esac
    printf '%-12s %s\n' "$t" "$v"
  done
  printf '%-12s %s\n' solc "$(ls ~/.svm 2>/dev/null | tr '\n' ' ')" z3 "$("$TOOLS/venv/bin/python" -c 'import z3; print(z3.get_version_string())' 2>/dev/null)"
}

slither_() {
  mkdir -p "$OUT"
  cd "$ROOT/vault" || return 1
  # Every detector (Slither's default runs all of them; nothing excluded), src/ only (lib/ test/ script/ filtered).
  slither . --filter-paths 'lib/|test/|script/' --json "$OUT/slither.json" --checklist \
    > "$OUT/slither-checklist.md" 2> "$OUT/slither.log"
  echo "slither exit $?"
  slither . --filter-paths 'lib/|test/|script/' --print human-summary > /dev/null 2> "$OUT/slither-summary.log"
  scrub "$OUT"/slither*
}

aderyn_() {
  mkdir -p "$OUT"
  cd "$ROOT/vault" || return 1
  aderyn . --src src -o "$OUT/aderyn.md" > "$OUT/aderyn.log" 2>&1; echo "aderyn md exit $?"
  aderyn . --src src -o "$OUT/aderyn.json" >> "$OUT/aderyn.log" 2>&1; echo "aderyn json exit $?"
  scrub "$OUT"/aderyn*
}

halmos_() {
  mkdir -p "$OUT"
  cd "$ROOT/vault" || return 1
  # halmos 0.3.3 declares its sha256 (and ripemd160) function over arg_size bits for an arg_size-byte input, so any
  # sha256 it meets (the vault's Merkle tree, journals) stops it with a Z3 sort mismatch. The one-line fix (bits =
  # bytes * 8), applied idempotently to the installed copy:
  local sevm
  for sevm in "$TOOLS"/venv/lib/python3*/site-packages/halmos/sevm.py; do
    [ -f "$sevm" ] && sed -i -e 's/BitVecSorts\[arg_size\], BitVecSort256/BitVecSorts[arg_size * 8], BitVecSort256/' \
      -e 's/BitVecSorts\[arg_size\], BitVecSort160/BitVecSorts[arg_size * 8], BitVecSort160/' "$sevm"
  done
  # halmos 0.3.3 also leaves sha256 uninterpreted on concrete input, so a domain tag the test computes never equals the
  # vault's precomputed literal and every release path is lost (the reachability checks caught it). Concrete input is
  # hashed for real, as halmos does for keccak; symbolic input stays an uninterpreted function.
  for sevm in "$TOOLS"/venv/lib/python3*/site-packages/halmos/sevm.py; do
    [ -f "$sevm" ] && "$TOOLS/venv/bin/python" - "$sevm" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
old = "                ret = ByteVec(f_sha256(wrapped))\n"
new = ("                ret = ByteVec(f_sha256(wrapped))\n"
       "                if not is_bv(unwrapped) or __import__('z3').is_bv_value(unwrapped):  # sectools: concrete sha256\n"
       "                    data = bytes(unwrapped) if not is_bv(unwrapped) else unwrapped.as_long().to_bytes(arg_size, 'big')\n"
       "                    ret = ByteVec(__import__('z3').BitVecVal(int.from_bytes(__import__('hashlib').sha256(data).digest(), 'big'), 256))\n")
if "sectools: concrete sha256" not in s and old in s:
    open(p, "w").write(s.replace(old, new, 1))
PY
  done
  # A concatenation of constants (abi.encodePacked of a literal and a concrete address, as the registry's namespaced
  # storage slot is derived) is not a bit-vector *value* until it is simplified, so the fix above missed it and the slot
  # stayed symbolic ("NotConcreteError: symbolic storage base slot", 140 times in the 11 November long run). Simplify
  # first (release-eng, docs/release-engineering.md §5); idempotent like the fixes above.
  for sevm in "$TOOLS"/venv/lib/python3*/site-packages/halmos/sevm.py; do
    [ -f "$sevm" ] && "$TOOLS/venv/bin/python" - "$sevm" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
old = "                if not is_bv(unwrapped) or __import__('z3').is_bv_value(unwrapped):  # sectools: concrete sha256\n"
new = ("                unwrapped = __import__('z3').simplify(unwrapped) if is_bv(unwrapped) else unwrapped  # sectools: simplified sha256 input\n" + old)
if "sectools: simplified sha256 input" not in s and old in s:
    open(p, "w").write(s.replace(old, new, 1))
PY
  done
  # The symbolic properties live in test/halmos/ (contract VaultHalmos, functions check_*).
  # Bounds: dynamic arrays of length 1 and byte strings of 4 bytes in generated calldata (see the triage note).
  # HALMOS_TIMEOUT (seconds) bounds one run; HALMOS_ARGS adds options, e.g. --function check_reach.
  ${HALMOS_TIMEOUT:+timeout --kill-after=30 $HALMOS_TIMEOUT} \
    halmos --contract VaultHalmos --function check_ --default-array-lengths 1 --default-bytes-lengths 4 \
    ${HALMOS_ARGS:-} > "$OUT/halmos.log" 2>&1
  echo "halmos exit $?"
  scrub "$OUT/halmos.log"
}

# The run recorded in the triage note: one Halmos process per group of checks, in parallel, each bounded in time
# (targeted checks 600 s; the entry-point check and the checks over Halmos-generated calldata 900 s, the latter under
# both storage layouts). Logs go to $OUT/halmos/<group>/halmos.log. Needs about 20 cores; ~15 minutes.
halmos_groups() {
  local base=$OUT g f x t d
  for g in check_release check_after_stop_no_release check_announced_fees check_registered_fees check_refund_rules \
    check_sunset_closes_deposits check_sunset_is_final check_reach_re check_reach_an check_reach_d check_reach_s \
    check_role_entry_points check_after_stop_only_owed_leaves check_no_role_call_moves_funds check_reach_ro check_reach_af \
    check_after_stop_only_owed_leaves:sol check_no_role_call_moves_funds:sol check_reach_ro:sol check_reach_af:sol; do
    f=${g%%:*}
    case $g in
      *:sol) x=""; t=900; d=$f-solidity-layout ;;
      check_no_role*|check_after_stop_only*|check_reach_ro|check_reach_af) x="--storage-layout generic"; t=900; d=$f ;;
      check_role_entry_points) x=""; t=900; d=$f ;;
      *) x=""; t=600; d=$f ;;
    esac
    ( OUT=$base/halmos/$d HALMOS_ARGS="--function $f $x" HALMOS_TIMEOUT=$t halmos_ > /dev/null 2>&1 ) &
    sleep 2
  done
  wait
  for d in "$base"/halmos/*/; do
    printf '%s: ' "$(basename "$d")"
    sed 's/\x1b\[[0-9;]*m//g' "$d/halmos.log" | grep -E '^\[(PASS|FAIL|TIMEOUT|ERROR)' | cut -c1-160 | tr '\n' ' '
    echo
  done
}

audit_() {
  mkdir -p "$OUT"
  cd "$ROOT" || return 1
  : > "$OUT/cargo-audit.txt"
  local rc=0
  # Every lockfile of the repository (git's list; a plain find where there is no .git, as on a build box).
  local locks
  locks=$(git ls-files '*Cargo.lock' 2>/dev/null)
  [ -n "$locks" ] || locks=$(find . -name Cargo.lock -not -path '*/target/*' -not -path './vault/lib/*' | sed 's|^\./||' | sort)
  for lock in $locks; do
    echo "=== $lock" >> "$OUT/cargo-audit.txt"
    cargo-audit audit --no-fetch -f "$lock" --json > "$OUT/cargo-audit-$(echo "$lock" | tr / _).json" 2>/dev/null
    cargo-audit audit --no-fetch -f "$lock" >> "$OUT/cargo-audit.txt" 2>&1 || rc=1
  done
  echo "cargo-audit exit $rc"
  scrub "$OUT"/cargo-audit*
}

deny_() {
  mkdir -p "$OUT"
  cd "$ROOT" || return 1
  local rc=0
  : > "$OUT/cargo-deny.txt"
  # The host workspaces (the guests share their dependency sets and are covered by cargo-audit above).
  for m in Cargo.toml ${DENY_MANIFESTS:-cli/Cargo.toml lez/Cargo.toml bridge/Cargo.toml checkpoint/Cargo.toml}; do
    echo "=== $m" >> "$OUT/cargo-deny.txt"
    # cargo-deny finds deny.toml at the repository root (the manifest's directory or a parent).
    cargo-deny --manifest-path "$m" --all-features check advisories licenses sources \
      >> "$OUT/cargo-deny.txt" 2>&1 || rc=1
  done
  echo "cargo-deny exit $rc"
  scrub "$OUT/cargo-deny.txt"
  # One line per diagnostic: workspace, severity[check], crate, advisory id (the full text with the inclusion
  # graphs is cargo-deny.txt).
  awk '/^=== /{m=$2} /^(error|warning)\[/{if(h!="")print m" | "h" | "c" | "id; h=$0; c=""; id=""}
       /registry\+|git\+/{if(c=="")c=$3" "$4} /├ ID: /{id=$NF} END{if(h!="")print m" | "h" | "c" | "id}' \
    "$OUT/cargo-deny.txt" > "$OUT/cargo-deny-summary.txt"
}

[ $# -gt 0 ] || set -- all
for step in "$@"; do
  case $step in
    install) install ;;
    versions) versions ;;
    slither) slither_ ;;
    aderyn) aderyn_ ;;
    halmos) halmos_ ;;
    halmos-groups) halmos_groups ;;
    audit) cargo-audit audit --version >/dev/null 2>&1; cargo-audit audit -f "$ROOT/Cargo.lock" -q >/dev/null 2>&1; audit_ ;;
    deny) deny_ ;;
    all) versions; slither_; aderyn_; halmos_; cargo-audit audit -f "$ROOT/Cargo.lock" -q >/dev/null 2>&1; audit_; deny_ ;;
    *) echo "unknown step $step" >&2; exit 2 ;;
  esac
done
