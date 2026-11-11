#!/usr/bin/env bash
# Industry-standard security tooling for Ferrata, re-runnable by anyone:
#   Solidity (vault/): Slither (every detector), Aderyn, Halmos (symbolic tests in vault/test/halmos/)
#   Rust (every Cargo.lock in the repository): cargo-audit; the main workspaces: cargo-deny
#   (advisories, licences, sources, bans) with the policy in deny.toml.
#
# Usage: scripts/security-tools.sh <step>...      steps: install versions slither aderyn halmos audit deny all
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
}

aderyn_() {
  mkdir -p "$OUT"
  cd "$ROOT/vault" || return 1
  aderyn . --src src -o "$OUT/aderyn.md" > "$OUT/aderyn.log" 2>&1; echo "aderyn md exit $?"
  aderyn . --src src -o "$OUT/aderyn.json" >> "$OUT/aderyn.log" 2>&1; echo "aderyn json exit $?"
}

halmos_() {
  mkdir -p "$OUT"
  cd "$ROOT/vault" || return 1
  # The symbolic properties live in test/halmos/ (contracts named *Halmos*, functions check_*).
  halmos --contract-regex 'Halmos' --function check_ ${HALMOS_ARGS:-} > "$OUT/halmos.log" 2>&1
  echo "halmos exit $?"
}

audit_() {
  mkdir -p "$OUT"
  cd "$ROOT" || return 1
  : > "$OUT/cargo-audit.txt"
  local rc=0
  for lock in $(git ls-files '*Cargo.lock'); do
    echo "=== $lock" >> "$OUT/cargo-audit.txt"
    cargo-audit audit --no-fetch -f "$lock" --json > "$OUT/cargo-audit-$(echo "$lock" | tr / _).json" 2>/dev/null
    cargo-audit audit --no-fetch -f "$lock" >> "$OUT/cargo-audit.txt" 2>&1 || rc=1
  done
  echo "cargo-audit exit $rc"
}

deny_() {
  mkdir -p "$OUT"
  cd "$ROOT" || return 1
  local rc=0
  : > "$OUT/cargo-deny.txt"
  # The host workspaces (the guests share their dependency sets and are covered by cargo-audit above).
  for m in Cargo.toml ${DENY_MANIFESTS:-cli/Cargo.toml lez/Cargo.toml bridge/Cargo.toml checkpoint/Cargo.toml}; do
    echo "=== $m" >> "$OUT/cargo-deny.txt"
    cargo-deny --manifest-path "$m" --all-features check --config "$ROOT/deny.toml" \
      advisories licenses sources >> "$OUT/cargo-deny.txt" 2>&1 || rc=1
  done
  echo "cargo-deny exit $rc"
}

[ $# -gt 0 ] || set -- all
for step in "$@"; do
  case $step in
    install) install ;;
    versions) versions ;;
    slither) slither_ ;;
    aderyn) aderyn_ ;;
    halmos) halmos_ ;;
    audit) cargo-audit audit --version >/dev/null 2>&1; cargo-audit audit -f "$ROOT/Cargo.lock" -q >/dev/null 2>&1; audit_ ;;
    deny) deny_ ;;
    all) versions; slither_; aderyn_; halmos_; cargo-audit audit -f "$ROOT/Cargo.lock" -q >/dev/null 2>&1; audit_; deny_ ;;
    *) echo "unknown step $step" >&2; exit 2 ;;
  esac
done
