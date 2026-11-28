#!/usr/bin/env bash
# Rebuild the LEZ guests that a v0.4.0 image-id record names, and compare their ids with the record; or make a record.
#
#   scripts/verify-lez040-ids.sh [RECORD_DIR ...]       # default: every docs/port-v0.4.0/image-ids/*/
#   scripts/verify-lez040-ids.sh --record NAME CRATE... # build lez/'s methods crates CRATE... at HEAD the same way and
#                                                       # write docs/port-v0.4.0/image-ids/NAME/{ids.tsv,build.json}
#
# For each record (build.json: the Ferrata commit, the methods crates; ids.tsv: what that build gave): a clean git
# worktree of the recorded commit beside the repository, `lez/scripts/lez-select.sh v0.4.0` there (it needs
# ../upstream/lez-v0.4.0 at LEZ v0.4.0, which scripts/fetch-upstream.sh fetches), the methods crates built with the
# guest build wrapper (scripts/guest-rustc) in a fresh target directory, scripts/reproduce/guest_ids.py's table, and
# each recorded guest's image id and binary sha256 compared with the rebuilt ones. RISC0_HOME_LEZ is the RISC Zero
# home with LEZ's guest Rust (default ../risc0-home-lez, as scripts/reproduce-image-ids.sh). The worktree is removed
# afterwards. Exit 1 on any difference. Two guests take about two minutes on four cores.
# scripts/verify.py lez040-image-ids-rebuild runs it (toolchain tier); lez040-image-ids (offline) holds the records to
# this tree's source.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$(cd "$HERE/.." && pwd)"
BESIDE="$(cd "$SRC/.." && pwd)"
R0LEZ="${RISC0_HOME_LEZ:-$BESIDE/risc0-home-lez}"
LEZ_COMMIT=a255b42c0fc80d1cb219454a3a1f5ec3b5d92115
[ -d "$R0LEZ" ] || { echo "verify-lez040-ids: no RISC Zero home for LEZ's guests at $R0LEZ (set RISC0_HOME_LEZ)" >&2; exit 1; }
[ -f "$BESIDE/upstream/lez-v0.4.0/Cargo.toml" ] || { echo "verify-lez040-ids: LEZ v0.4.0 not at $BESIDE/upstream/lez-v0.4.0 (scripts/fetch-upstream.sh)" >&2; exit 1; }

wt=""
cleanup() { [ -z "$wt" ] || git -C "$SRC" worktree remove --force "$wt" >/dev/null 2>&1 || true; }
trap cleanup EXIT

build_at() { # COMMIT CRATE...: a clean worktree of COMMIT beside the repository, selected on v0.4.0, CRATEs built
  local commit="$1" c args=()
  shift
  # A rebuild works at another path than a record's build of the same commit (`-rebuild`), so an id that depended on
  # the build's path would differ (scripts/guest-rustc remaps the paths that reach a guest).
  wt="$BESIDE/ferrata-verify-lez040-ids-${commit:0:8}${WT_SUFFIX:-}"
  if [ -e "$wt" ]; then echo "verify-lez040-ids: $wt exists: remove it first" >&2; wt=""; exit 2; fi
  if ! git -C "$SRC" worktree add --detach "$wt" "$commit" >/dev/null 2>&1; then
    echo "verify-lez040-ids: this clone lacks commit $commit (git fetch)" >&2; wt=""; exit 2
  fi
  mkdir -p "$wt/.scratch"
  ( cd "$wt" && bash lez/scripts/lez-select.sh v0.4.0 )
  for c in "$@"; do args+=(-p "$c"); done
  ( cd "$wt/lez" && unset CARGO_TARGET_DIR RUSTFLAGS CARGO_ENCODED_RUSTFLAGS FERRATA_GUEST_REMAP_EXTRA \
      && RUSTC_WRAPPER="$wt/scripts/guest-rustc" RISC0_BUILD_LOCKED=1 RISC0_HOME="$R0LEZ" CARGO_TERM_COLOR=never \
         cargo build "${args[@]}" --locked ) > "$wt/.scratch/build.log" 2>&1
}

done_with() { git -C "$SRC" worktree remove --force "$wt" >/dev/null 2>&1 || true; wt=""; }

if [ "${1:-}" != --record ]; then WT_SUFFIX=-rebuild; fi
if [ "${1:-}" = --record ]; then
  [ $# -ge 3 ] || { echo "usage: $0 --record NAME CRATE..." >&2; exit 2; }
  name="$2"; shift 2
  commit="$(git -C "$SRC" rev-parse HEAD)"
  out="$SRC/docs/port-v0.4.0/image-ids/$name"
  [ ! -e "$out" ] || { echo "verify-lez040-ids: $out exists" >&2; exit 2; }
  started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  if ! build_at "$commit" "$@"; then echo "the build failed:"; tail -n 20 "$wt/.scratch/build.log"; exit 1; fi
  mkdir -p "$out"
  python3 -I "$wt/scripts/reproduce/guest_ids.py" "$wt" "$out/ids.tsv" > /dev/null
  python3 -I "$HERE/reproduce/lez040_build_json.py" "$out/build.json" "$commit" "$LEZ_COMMIT" "$started" \
    "$(cd "$wt/lez" && cargo --version)" "$(ls "$R0LEZ/toolchains" | tr '\n' ' ')" \
    "$(r0vm --version 2>/dev/null | head -n 1 || echo 'not installed')" "$@"
  cat "$out/ids.tsv"
  done_with
  exit 0
fi

if [ $# -gt 0 ]; then recs=("$@"); else recs=("$SRC"/docs/port-v0.4.0/image-ids/*/); fi
fail=0
for rec in "${recs[@]}"; do
  rec="${rec%/}"
  if [ ! -f "$rec/build.json" ] || [ ! -f "$rec/ids.tsv" ]; then
    echo "verify-lez040-ids: $rec has no build.json and ids.tsv" >&2; exit 2
  fi
  commit="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["ferrata_commit"])' "$rec/build.json")"
  mapfile -t crates < <(python3 -I -c 'import json,sys; [print(c) for c in json.load(open(sys.argv[1]))["crates"]]' "$rec/build.json")
  if ! build_at "$commit" "${crates[@]}"; then
    echo "$(basename "$rec"): the build failed:"; tail -n 20 "$wt/.scratch/build.log"; fail=1
  else
    python3 -I "$wt/scripts/reproduce/guest_ids.py" "$wt" "$wt/.scratch/ids.tsv" > /dev/null || true
    python3 -I "$HERE/reproduce/compare_lez040_ids.py" "$rec/ids.tsv" "$wt/.scratch/ids.tsv" "$(basename "$rec")" || fail=1
  fi
  done_with
done
exit $fail
