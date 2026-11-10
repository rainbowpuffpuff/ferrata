# Sourced by the scripts of this directory.
#
# Layout (as in ../RESULTS.md §6): LEZ v0.4.0-rc1 at upstream/lez-v0.4.0-rc1 beside the
# repository, unmodified, and scratch in tmp/ beside the repository. Nothing these scripts write
# goes to /tmp or into the LEZ tree: build outputs, downloaded circuit artifacts, the sequencer's
# home, the wallet, logs and the unabridged block log all live under tmp/sequencer.

LIVE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$LIVE_DIR/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
# Which LEZ: the tag, the source tree and the commit it must be at. The default is the tag of the
# recorded run in results/; LEZ_TAG=v0.4.0-rc2 selects rc2 (results/v0.4.0-rc2/). The driver's
# manifests must point at the same tag (../scripts/lez-select.sh).
LEZ_TAG="${LEZ_TAG:-v0.4.0-rc1}"
LEZ_SRC="${LEZ_SRC:-$BESIDE/upstream/lez-$LEZ_TAG}"
case "$LEZ_TAG" in
  v0.4.0-rc1) LEZ_COMMIT="${LEZ_COMMIT:-1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21}" ;;
  v0.4.0-rc2) LEZ_COMMIT="${LEZ_COMMIT:-e3edf47402a82557103ca719b30d0bd2684338d1}" ;;
  *) [ -n "${LEZ_COMMIT:-}" ] || { echo "error: no commit known for LEZ $LEZ_TAG: set LEZ_COMMIT" >&2; exit 1; } ;;
esac

# Scratch: tmp/sequencer for v0.4.0-rc1, tmp/sequencer-<tag> for another tag.
if [ "$LEZ_TAG" = v0.4.0-rc1 ]; then _scratch=sequencer; else _scratch="sequencer-$LEZ_TAG"; fi
export FERRATA_LIVE_TMP="${FERRATA_LIVE_TMP:-$BESIDE/tmp/$_scratch}"
mkdir -p "$FERRATA_LIVE_TMP/logs" "$FERRATA_LIVE_TMP/bin"
export TMPDIR="$FERRATA_LIVE_TMP"
# The Logos circuit crates download their prover artifacts (47 MB) into the user's cache
# directory at build time and read them from there at run time.
export XDG_CACHE_HOME="${FERRATA_LIVE_XDG_CACHE:-$FERRATA_LIVE_TMP/xdg-cache}"

# Parallelism. The defaults keep a laptop usable; a build machine sets FERRATA_LIVE_JOBS=16
# FERRATA_LIVE_NICE=0.
export CARGO_BUILD_JOBS="${FERRATA_LIVE_JOBS:-4}"
if [ "${FERRATA_LIVE_NICE:-1}" = 0 ]; then NICE=""; else NICE="nice -n 19"; fi

TARGET="$FERRATA_LIVE_TMP/target"             # LEZ's sequencer and the driver
BIN="$FERRATA_LIVE_TMP/bin"

# The guest toolchain of ../RESULTS.md §6, if it is installed there.
if [ -z "${RISC0_HOME:-}" ] && [ -d "$BESIDE/risc0-home-lez" ]; then
  export RISC0_HOME="$BESIDE/risc0-home-lez"
fi

# LEZ's wallet links libpcsclite (keycard support). Where the development package is not
# installed and cannot be (no root), README.md describes a link shim; it is picked up here.
if ! pkg-config --exists libpcsclite 2>/dev/null && [ -f "$FERRATA_LIVE_TMP/pcsc-shim/libpcsclite.pc" ]; then
  export PKG_CONFIG_PATH="$FERRATA_LIVE_TMP/pcsc-shim${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
fi

# Never authenticate to an outside service from these scripts.
unset GITHUB_TOKEN GH_TOKEN

die() { echo "error: $*" >&2; exit 1; }

check_lez() {
  [ -f "$LEZ_SRC/Cargo.toml" ] || die "LEZ $LEZ_TAG not found at $LEZ_SRC (scripts/fetch-upstream.sh fetches v0.4.0-rc1)"
  if [ -d "$LEZ_SRC/.git" ]; then
    local head; head="$(git -C "$LEZ_SRC" rev-parse HEAD)"
    [ "$head" = "$LEZ_COMMIT" ] || die "LEZ checkout is at $head, expected $LEZ_COMMIT ($LEZ_TAG)"
    [ -z "$(git -C "$LEZ_SRC" status --porcelain)" ] || die "the LEZ checkout is modified; this run needs it unmodified"
  fi
}

# The driver links LEZ's crates by path: its manifests must point at this tag.
check_driver_tag() {
  local selected; selected="$("$REPO/lez/scripts/lez-select.sh" --show)"
  [ "$selected" = "$LEZ_TAG" ] || die "the driver's manifests point at LEZ $selected, not $LEZ_TAG: run lez/scripts/lez-select.sh $LEZ_TAG first"
}
