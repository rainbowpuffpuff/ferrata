#!/usr/bin/env bash
# SPEL's LEZ v0.4 line (logos-co/spel PR #301, head c90ca0b) against Ferrata's programs (idl/README.md, "SPEL v0.4").
# On the build box (spike/modal-build/mrun), with the PR's tree and LEZ v0.4.0-rc3 beside the repository:
#   mrun --put ../upstream/spel-c90ca0b --put ../upstream/lez-v0.4.0-rc3 -- 'bash idl/spel-v2/run.sh'
# RECORDED=FILE (relative to the directory it is run from, or absolute): also decode lez/live/driver's idl-recorded
# output. FERRATA_REV names the tree in the record (default: git's HEAD, marked when the tree has changes).
# 1. SPEL as the PR has it: the attestation program's IDL generated from lez/wire, compared with idl/ferrata_attestation.idl.json,
#    every vector (and with RECORDED=FILE every recorded message) decoded by SPEL's decoder; the bridge's generation
#    expected to fail on the tuple in BridgeMessage::Spend.
# 2. SPEL with patches/spel-c90ca0b-idl-gen-tuples.patch (a tuple described as a struct with positional fields): the
#    bridge program's IDL generated from lez/bridge-wire, compared, its vectors decoded.
# Writes idl/generated/<program>.idl-v2.json and idl/generated/spel-v2.txt.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
UP="$(cd "$REPO/../upstream" && pwd)"
OUT="$REPO/idl/generated"
if [ -n "${RECORDED:-}" ]; then
  [ -f "$RECORDED" ] || { echo "run.sh: RECORDED=$RECORDED: no such file" >&2; exit 1; }
  RECORDED="$(realpath "$RECORDED")"   # the steps below run in other directories
fi
if [ -z "${FERRATA_REV:-}" ] && FERRATA_REV="$(git -C "$REPO" rev-parse --short=8 HEAD 2>/dev/null)"; then
  git -C "$REPO" diff --quiet HEAD -- 2>/dev/null || FERRATA_REV="$FERRATA_REV with uncommitted changes"
fi
mkdir -p "$OUT"
LOG="$OUT/spel-v2.txt"
{
  echo "# idl/spel-v2/run.sh: SPEL PR #301 (c90ca0b, LEZ v0.4.0-rc3) on Ferrata ${FERRATA_REV:-(tree not named)}"
  [ -n "${RECORDED:-}" ] && echo "# recorded messages: $(basename "$RECORDED") ($(stat -c %s "$RECORDED") bytes)"
  echo "## 1. SPEL as PR #301 has it"
} > "$LOG"
( cd "$HERE" && FERRATA_REPO="$REPO" cargo run -q --locked -- "$OUT" ${RECORDED:-} --expect-generation-failure ferrata_bridge ) 2>&1 | tee -a "$LOG"
echo "## 2. SPEL with patches/spel-c90ca0b-idl-gen-tuples.patch" | tee -a "$LOG"
rm -rf "$UP/spel-c90ca0b-tuples"
cp -r "$UP/spel-c90ca0b" "$UP/spel-c90ca0b-tuples"
patch -s -d "$UP/spel-c90ca0b-tuples" -p1 < "$HERE/patches/spel-c90ca0b-idl-gen-tuples.patch"
T="$HERE/target/tuples-crate"
rm -rf "$T"; mkdir -p "$T"
cp -r "$HERE/src" "$HERE/Cargo.lock" "$T/"
sed -e "s#\.\./\.\./\.\./upstream/spel-c90ca0b/#$UP/spel-c90ca0b-tuples/#" -e "s#\.\./\.\./\.\./upstream/#$UP/#" \
  "$HERE/Cargo.toml" > "$T/Cargo.toml"
( cd "$T" && CARGO_TARGET_DIR="$HERE/target" FERRATA_REPO="$REPO" cargo run -q --locked -- "$OUT" --only ferrata_bridge ) 2>&1 | tee -a "$LOG"
echo "spel-v2 run: done" | tee -a "$LOG"
