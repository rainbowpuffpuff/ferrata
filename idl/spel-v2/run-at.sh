#!/usr/bin/env bash
# idl/spel-v2/run.sh's two steps against another tree of SPEL's LEZ v0.4 line (logos-co/spel PR #301 at a later head),
# to see whether what run.sh shows at c90ca0b still holds there (RFP-021 U.4, RFP-022 U.4). On the build box, with the
# tree exported beside the repository (git archive of the head into upstream/spel-<rev>):
#   mrun --put ../upstream/spel-<rev> --put ../upstream/lez-v0.4.0-rc3 -- 'bash idl/spel-v2/run-at.sh <rev>'
# 1. SPEL as that head has it: the programs' IDLs generated, compared with the hand-written ones, the vectors decoded,
#    the bridge's generation expected to fail on the tuple in BridgeMessage::Spend.
# 2. That head with patches/spel-c90ca0b-idl-gen-tuples.patch applied (it must apply), the bridge's IDL compared.
# The runner (src/main.rs) is the one run.sh builds, copied into a scratch crate whose dependency points at the given
# tree; where that head's GenOptions takes `message: Option<String>` and `states: Vec<String>` (from 79594580), the
# copy's options are written that way, nothing else changed. Its Cargo.lock is resolved afresh (not --locked) and kept
# beside the record. Writes idl/generated/spel-v2-<rev>.txt.
set -euo pipefail
REV="${1:?usage: run-at.sh REV (upstream/spel-REV beside the repository)}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
UP="$(cd "$REPO/../upstream" && pwd)"
SPEL="$UP/spel-$REV"
[ -d "$SPEL/spel-framework-core" ] || { echo "run-at.sh: no SPEL tree at $SPEL" >&2; exit 1; }
OUT="$REPO/idl/generated/at-$REV"
LOG="$REPO/idl/generated/spel-v2-$REV.txt"
mkdir -p "$OUT"
crate() { # DIR SPEL_TREE: the runner as a crate against SPEL_TREE
  rm -rf "$1"; mkdir -p "$1"
  cp -r "$HERE/src" "$1/"
  sed -e "s#\.\./\.\./\.\./upstream/spel-c90ca0b/#$2/#" -e "s#\.\./\.\./\.\./upstream/#$UP/#" "$HERE/Cargo.toml" > "$1/Cargo.toml"
  if grep -q "pub states: Vec<String>" "$2/spel-framework-core/src/idl_gen.rs"; then
    python3 -I - "$1/src/main.rs" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); s = p.read_text()
old = "            message: p.message.to_string(),\n            state: Some((*state).to_string()),\n"
new = "            message: Some(p.message.to_string()),\n            states: vec![(*state).to_string()],\n"
assert old in s, "the runner's GenOptions changed: adapt run-at.sh"
p.write_text(s.replace(old, new))
PY
  fi
}
T="$HERE/target/at-$REV"
{
  echo "# idl/spel-v2/run-at.sh: SPEL PR #301 at $REV (LEZ $(grep -o 'tag = "[^"]*"' "$SPEL/spel-cli/Cargo.toml" | head -1)) on Ferrata $(git -C "$REPO" rev-parse --short=8 HEAD 2>/dev/null || echo '(tree not named)')"
  echo "## 1. SPEL as $REV has it"
} > "$LOG"
crate "$T/as-is" "$SPEL"
( cd "$T/as-is" && CARGO_TARGET_DIR="$HERE/target" FERRATA_REPO="$REPO" cargo run -q -- "$OUT" --expect-generation-failure ferrata_bridge ) 2>&1 | tee -a "$LOG"
echo "## 2. SPEL $REV with patches/spel-c90ca0b-idl-gen-tuples.patch" | tee -a "$LOG"
rm -rf "$SPEL-tuples"; cp -r "$SPEL" "$SPEL-tuples"
patch -s -d "$SPEL-tuples" -p1 < "$HERE/patches/spel-c90ca0b-idl-gen-tuples.patch"
crate "$T/tuples" "$SPEL-tuples"
( cd "$T/tuples" && CARGO_TARGET_DIR="$HERE/target" FERRATA_REPO="$REPO" cargo run -q -- "$OUT" --only ferrata_bridge ) 2>&1 | tee -a "$LOG"
cp "$T/as-is/Cargo.lock" "$OUT/Cargo.lock" 2>/dev/null || true
echo "spel-v2 run at $REV: done" | tee -a "$LOG"
