#!/usr/bin/env bash
# Point Ferrata's LEZ crates at one LEZ tag.
#
#   lez/scripts/lez-select.sh TAG     # TAG: v0.4.0-rc1 (the default; what the repository commits)
#                                     #      or v0.4.0-rc2
#   LEZ_TAG=TAG lez/scripts/lez-select.sh
#   lez/scripts/lez-select.sh --show  # print the tag the manifests point at
#
# Cargo has no variables in manifests, so this rewrites, in place and nothing else:
# - every LEZ path dependency of lez/ (`upstream/lez-<tag>/`, the clone beside the repository, in
#   the manifests of wire/, bridge-wire/, harness/, guests/*/, live/driver/ and the frozen copy
#   live/frozen/bridge-wire/ that live/driver/ builds against: one tag in the driver's graph);
# - the default feature of harness/ and live/driver/ (the line marked `# lez-select`): `lez-rc2`
#   on v0.4.0-rc2, none on v0.4.0-rc1 (the few calls whose LEZ API differs between the tags);
# - the lockfiles: lockfiles/<tag>.patch holds what the tag changes in the v0.4.0-rc1 lockfiles
#   (dependency edges of LEZ's own crates); it is applied, or reverted for v0.4.0-rc1.
# Selecting the tag already selected changes nothing. The LEZ checkout must be at the tag's commit
# and unmodified. Builds against another tag than v0.4.0-rc1 should use their own target directory
# (CARGO_TARGET_DIR), or every switch rebuilds the guests.
set -euo pipefail
lez="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
beside="$(cd "$lez/../.." && pwd)"
manifests=(wire bridge-wire harness guests/program guests/program-keccak-soft guests/consumer
           guests/consumer-keccak-accel guests/scripted guests/bridge live/driver live/frozen/bridge-wire)
featured=(harness live/driver)

die() { echo "lez-select: $*" >&2; exit 1; }
commit_of() {
  case "$1" in
    v0.4.0-rc1) echo 1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21 ;;
    v0.4.0-rc2) echo e3edf47402a82557103ca719b30d0bd2684338d1 ;;
    *) return 1 ;;
  esac
}
features_of() { case "$1" in v0.4.0-rc1) echo "" ;; *) echo '"lez-rc2"' ;; esac; }
selected() {
  local tags
  tags="$(sed -n 's#.*upstream/lez-\([A-Za-z0-9.+-]*\)/.*#\1#p' "${manifests[@]/%//Cargo.toml}" | sort -u)"
  [ "$(echo "$tags" | wc -l)" = 1 ] || die "the manifests point at several LEZ trees: $(echo $tags)"
  echo "$tags"
}

cd "$lez"
if [ "${1:-}" = --show ]; then selected; exit 0; fi
tag="${1:-${LEZ_TAG:-v0.4.0-rc1}}"
commit="$(commit_of "$tag")" || die "unknown LEZ tag $tag (known: v0.4.0-rc1, v0.4.0-rc2)"
src="$beside/upstream/lez-$tag"
[ -f "$src/Cargo.toml" ] || die "LEZ $tag not found at $src"
if [ -d "$src/.git" ]; then
  head="$(git -C "$src" rev-parse HEAD)"
  [ "$head" = "$commit" ] || die "$src is at $head, expected $commit ($tag)"
  [ -z "$(git -C "$src" status --porcelain)" ] || die "$src is modified"
fi

from="$(selected)"
if [ "$from" = "$tag" ]; then echo "lez-select: already $tag"; exit 0; fi

# Lockfiles: back to v0.4.0-rc1's, then forward to the tag's. Checked before anything is written.
lockpatch() { # tag direction(apply|revert) [--dry-run]
  local p="$lez/lockfiles/$1.patch"
  [ -f "$p" ] || { echo "lez-select: no lockfiles/$1.patch: cargo will update the lockfiles" >&2; return 0; }
  if [ "$2" = revert ]; then patch -p1 -R --quiet ${3:-} < "$p"; else patch -p1 --forward --quiet ${3:-} < "$p"; fi
}
if [ "$from" = v0.4.0-rc1 ]; then
  lockpatch "$tag" apply --dry-run || die "lockfiles/$tag.patch does not apply to the lockfiles"
else
  lockpatch "$from" revert --dry-run || die "lockfiles/$from.patch does not revert cleanly"
fi

for m in "${manifests[@]}"; do
  sed -i -E "s#upstream/lez-[A-Za-z0-9.+-]*/#upstream/lez-$tag/#g" "$m/Cargo.toml"
done
for m in "${featured[@]}"; do
  grep -q '^default = \[[^]]*\] # lez-select$' "$m/Cargo.toml" || die "$m/Cargo.toml has no '# lez-select' default line"
  sed -i -E "s/^default = \[[^]]*\] # lez-select$/default = [$(features_of "$tag")] # lez-select/" "$m/Cargo.toml"
done
[ "$from" = v0.4.0-rc1 ] || lockpatch "$from" revert
[ "$tag" = v0.4.0-rc1 ] || lockpatch "$tag" apply
echo "lez-select: $from -> $(selected) ($commit)"
