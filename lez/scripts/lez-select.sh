#!/usr/bin/env bash
# Point Ferrata's LEZ crates at one LEZ tag.
#
#   lez/scripts/lez-select.sh TAG     # TAG: v0.4.0-rc1 (the default; what the repository commits)
#                                     #      or v0.4.0-rc2, v0.4.0-rc3, v0.4.0
#   LEZ_TAG=TAG lez/scripts/lez-select.sh
#   lez/scripts/lez-select.sh --show  # print the tag the manifests point at
#
# Cargo has no variables in manifests, so this rewrites, in place and nothing else:
# - every LEZ path dependency of lez/ (`upstream/lez-<tag>/`, the clone beside the repository), in
#   the manifests listed in `manifests` below: wire/, bridge-wire/, holdings-wire/, names-wire/, statements-wire/,
#   rebate-wire/, harness/, updater/, advance/, every guest under guests/ but msg/ and msg-v2/, live/driver/,
#   crown/driver/ and updater/sequencer/ (both compile live/driver's modules) and the frozen copy
#   live/frozen/bridge-wire/ that live/driver/ builds against: one tag in each graph;
# - the messages guests (guests/msg/, guests/msg-v2/, `rc3_pinned`), built against v0.4.0-rc3 (the test network's
#   tag) whatever the tag, except v0.4.0, which moves them too;
# - LEZ's crates that moved: v0.4.0 moved `program_loader_core` from lez/programs/program_loader/core to
#   lee/program_loader (LEZ #946), so its path follows the tag;
# - the RISC Zero pins: the guests' and the harness's `risc0-zkvm = { version = "=…" }`, the methods crates'
#   `risc0-build = { version = "=…" }` and the drivers' `risc0-zkos-v1compat = "=…"` are LEZ's: 3.0.5, 3.0.5 and
#   2.2.2 up to v0.4.0-rc3, 3.1.0, 3.0.7 and 2.2.3 on v0.4.0 (LEZ #1124);
# - the default feature of harness/, updater/, live/driver/, crown/driver/ and updater/sequencer/ (the line marked
#   `# lez-select`): `lez-rc2` on v0.4.0-rc2, `lez-rc3` (which implies `lez-rc2`) on v0.4.0-rc3, `lez-040` (which
#   implies `lez-rc3`) on v0.4.0, none on v0.4.0-rc1 (the calls whose LEZ API differs between the tags);
# - the sources of the guests and wire crates in `sourced_040` (the line `path = "…/main.rs" # lez-select`, or
#   `lib.rs`): their v0.4.0 twins `main_lez040.rs` / `lib_lez040.rs` on v0.4.0 (lez/scripts/lez040-sources.py generates
#   them for LEZ #1144's API), the originals on the release candidates, whose builds and image ids stay what they were;
# - the lockfiles: lockfiles/<tag>.patch holds what the tag changes in the committed lockfiles (dependency edges of
#   LEZ's own crates; on v0.4.0 also the RISC Zero, Logos Blockchain and zone-sdk versions, in the guests' lockfiles
#   too); it is applied, or reverted for v0.4.0-rc1.
# Selecting the tag already selected changes nothing. The LEZ checkout must be at the tag's commit
# and unmodified. Builds against another tag than v0.4.0-rc1 should use their own target directory
# (CARGO_TARGET_DIR), or every switch rebuilds the guests.
set -euo pipefail
lez="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
beside="$(cd "$lez/../.." && pwd)"
manifests=(wire bridge-wire harness guests/program guests/program-keccak-soft guests/consumer
           guests/consumer-keccak-accel guests/scripted guests/bridge live/driver live/frozen/bridge-wire
           holdings-wire guests/holdings updater advance crown/driver updater/sequencer names-wire guests/names
           statements-wire rebate-wire guests/statements guests/statements-reader guests/rebate guests/program-class
           guests/program-residue-experimental)
rc3_pinned=(guests/msg guests/msg-v2)
methods=(methods methods-msg methods-names methods-program-class methods-rebate methods-residue-experimental
         methods-statements)
featured=(harness live/driver crown/driver updater/sequencer updater)
sourced_040=(names-wire guests/bridge guests/consumer guests/consumer-keccak-accel guests/msg guests/msg-v2)
tags="v0.4.0-rc1, v0.4.0-rc2, v0.4.0-rc3, v0.4.0"

die() { echo "lez-select: $*" >&2; exit 1; }
commit_of() {
  case "$1" in
    v0.4.0-rc1) echo 1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21 ;;
    v0.4.0-rc2) echo e3edf47402a82557103ca719b30d0bd2684338d1 ;;
    v0.4.0-rc3) echo bafd07be0701398f9bc435f73ab05e9a7936c301 ;;
    v0.4.0) echo a255b42c0fc80d1cb219454a3a1f5ec3b5d92115 ;;
    *) return 1 ;;
  esac
}
features_of() {
  case "$1" in v0.4.0-rc1) echo "" ;; v0.4.0-rc2) echo '"lez-rc2"' ;; v0.4.0-rc3) echo '"lez-rc3"' ;; *) echo '"lez-040"' ;; esac
}
pinned_tag_of() { if [ "$1" = v0.4.0 ]; then echo v0.4.0; else echo v0.4.0-rc3; fi; }
# risc0-zkvm, risc0-build, risc0-zkos-v1compat: LEZ's workspace pins at the tag.
risc0_of() { if [ "$1" = v0.4.0 ]; then echo "3.1.0 3.0.7 2.2.3"; else echo "3.0.5 3.0.5 2.2.2"; fi; }
selected() {
  local found
  found="$(sed -n 's#.*upstream/lez-\([A-Za-z0-9.+-]*\)/.*#\1#p' "${manifests[@]/%//Cargo.toml}" | sort -u)"
  [ "$(echo "$found" | wc -l)" = 1 ] || die "the manifests point at several LEZ trees: $(echo $found)"
  echo "$found"
}
retarget() { # manifest tag: the LEZ tree, and the paths of the crates that moved
  sed -i -E -e "s#upstream/lez-[A-Za-z0-9.+-]*/#upstream/lez-$2/#g" "$1"
  if [ "$2" = v0.4.0 ]; then
    sed -i -E -e 's#(upstream/lez-[A-Za-z0-9.+-]*/)lez/programs/program_loader/core"#\1lee/program_loader"#g' "$1"
  else
    sed -i -E -e 's#(upstream/lez-[A-Za-z0-9.+-]*/)lee/program_loader"#\1lez/programs/program_loader/core"#g' "$1"
  fi
}
repin() { # manifest tag
  local zkvm build v1compat
  read -r zkvm build v1compat <<< "$(risc0_of "$2")"
  sed -i -E -e "s/^(risc0-zkvm = \\{ version = \"=)[0-9.]+(\")/\\1$zkvm\\2/" \
            -e "s/^(risc0-build = \\{ version = \"=)[0-9.]+(\")/\\1$build\\2/" \
            -e "s/^(risc0-zkos-v1compat = \"=)[0-9.]+(\")/\\1$v1compat\\2/" "$1"
}

cd "$lez"
if [ "${1:-}" = --show ]; then selected; exit 0; fi
tag="${1:-${LEZ_TAG:-v0.4.0-rc1}}"
commit="$(commit_of "$tag")" || die "unknown LEZ tag $tag (known: $tags)"
need_tree() { # tag: the LEZ checkout beside the repository, at the tag's commit and unmodified
  local src="$beside/upstream/lez-$1" want head
  want="$(commit_of "$1")"
  [ -f "$src/Cargo.toml" ] || die "LEZ $1 not found at $src"
  if [ -d "$src/.git" ]; then
    head="$(git -C "$src" rev-parse HEAD)"
    [ "$head" = "$want" ] || die "$src is at $head, expected $want ($1)"
    [ -z "$(git -C "$src" status --porcelain)" ] || die "$src is modified"
  fi
}
need_tree "$tag"
[ "$(pinned_tag_of "$tag")" = "$tag" ] || need_tree "$(pinned_tag_of "$tag")"

from="$(selected)"
if [ "$from" = "$tag" ]; then echo "lez-select: already $tag"; exit 0; fi

# Lockfiles: back to the committed ones, then forward to the tag's. Checked before anything is written.
lockpatch() { # tag direction(apply|revert) [--dry-run]
  local p="$lez/lockfiles/$1.patch"
  [ -f "$p" ] || { echo "lez-select: no lockfiles/$1.patch: cargo will update the lockfiles" >&2; return 0; }
  if [ "$2" = revert ]; then
    patch -p1 -R --quiet --no-backup-if-mismatch ${3:-} < "$p"
  else
    patch -p1 --forward --quiet --no-backup-if-mismatch ${3:-} < "$p"
  fi
}
if [ "$from" = v0.4.0-rc1 ]; then
  lockpatch "$tag" apply --dry-run || die "lockfiles/$tag.patch does not apply to the lockfiles"
else
  lockpatch "$from" revert --dry-run || die "lockfiles/$from.patch does not revert cleanly"
fi

for m in "${featured[@]}"; do
  grep -q '^default = \[[^]]*\] # lez-select$' "$m/Cargo.toml" || die "$m/Cargo.toml has no '# lez-select' default line"
done
for m in "${sourced_040[@]}"; do
  grep -q '^path = "[^"]*/\(main\|lib\)\(_lez040\)\{0,1\}\.rs" # lez-select$' "$m/Cargo.toml" \
    || die "$m/Cargo.toml has no '# lez-select' source path line"
done
for m in "${manifests[@]}"; do retarget "$m/Cargo.toml" "$tag"; repin "$m/Cargo.toml" "$tag"; done
for m in "${rc3_pinned[@]}"; do retarget "$m/Cargo.toml" "$(pinned_tag_of "$tag")"; repin "$m/Cargo.toml" "$tag"; done
for m in "${methods[@]}"; do repin "$m/Cargo.toml" "$tag"; done
for m in "${featured[@]}"; do
  sed -i -E "s/^default = \[[^]]*\] # lez-select$/default = [$(features_of "$tag")] # lez-select/" "$m/Cargo.toml"
done
for m in "${sourced_040[@]}"; do
  if [ "$tag" = v0.4.0 ]; then
    sed -i -E 's#^path = "(.*)/(main|lib)\.rs" \# lez-select$#path = "\1/\2_lez040.rs" \# lez-select#' "$m/Cargo.toml"
  else
    sed -i -E 's#^path = "(.*)/(main|lib)_lez040\.rs" \# lez-select$#path = "\1/\2.rs" \# lez-select#' "$m/Cargo.toml"
  fi
done
[ "$from" = v0.4.0-rc1 ] || lockpatch "$from" revert
[ "$tag" = v0.4.0-rc1 ] || lockpatch "$tag" apply
echo "lez-select: $from -> $(selected) ($commit)"
