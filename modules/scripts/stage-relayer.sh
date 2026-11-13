#!/usr/bin/env bash
# Stage ferrata_relayer_module's sources into its crate directory (rust-lib/vendor/, ignored by git).
#
# The relayer daemon is `ferrata_cli::relayerd` (cli/), which the `ferrata-relayer` binary runs. The module builds
# cli/'s library without its `lez-wallet` feature: LEZ's wallet library links LEZ's node-side crates (RocksDB,
# libp2p), which the module builder does not build; the module reaches LEZ through the devnet's zone. So:
#
#   vendor/ferrata/cli/            cli/'s manifest with only its package, dependencies, features and workspace
#                                  (no binaries, examples, tests, dev-dependencies, patches or profiles: the module
#                                  links the library), src/ without the binaries (main.rs, bin/), and cli/wallet
#                                  (ferrata-wallet: the root import, the release rule, shared with ferrata_bridge_module)
#   vendor/ferrata/...             the repository's crates cli/ links, in the repository's own layout, with the
#                                  workspace manifests they inherit from (the root's, bridge/'s): lez/{wire,bridge-wire},
#                                  crates/ferrata-core, bridge/crates/ferrata-bridge-core
#   vendor/upstream/lez-v0.4.0-rc1/  the LEZ tree beside the repository (without target/ and .git): lee, lee_core
#
#   modules/scripts/stage-relayer.sh
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
spike=$(cd "$repo/.." && pwd)
lez_up="$spike/upstream/lez-v0.4.0-rc1"
commit=${FERRATA_COMMIT:-$(git -C "$repo" rev-parse --verify -q HEAD 2>/dev/null || echo unknown)}
r="$repo/modules/ferrata_relayer_module/rust-lib"
v="$r/vendor"
[ -d "$lez_up/lee" ] || { echo "stage-relayer.sh: $lez_up missing" >&2; exit 1; }

rm -rf "$v"
mkdir -p "$v/ferrata" "$v/upstream"
copy() { # SOURCE_DIR (relative to the repository): its Cargo.toml, README.md, build.rs and src/
  local src="$repo/$1" dst="$v/ferrata/$1"
  mkdir -p "$dst"
  cp -r "$src/Cargo.toml" "$src/src" "$dst/"
  for f in README.md build.rs; do if [ -f "$src/$f" ]; then cp "$src/$f" "$dst/"; fi; done
}
crates="cli/wallet lez/wire lez/bridge-wire crates/ferrata-core bridge/crates/ferrata-bridge-core"
for c in $crates; do copy "$c"; done
cp "$repo/Cargo.toml" "$v/ferrata/Cargo.toml"
cp "$repo/bridge/Cargo.toml" "$v/ferrata/bridge/Cargo.toml"

# cli/: the library only.
mkdir -p "$v/ferrata/cli/src"
(cd "$repo/cli/src" && find . -maxdepth 1 -name '*.rs' ! -name main.rs -exec cp {} "$v/ferrata/cli/src/" \;)
python3 -I - "$repo/cli/Cargo.toml" "$v/ferrata/cli/Cargo.toml" <<'PY'
import re, sys
src, dst = sys.argv[1:3]
keep = {"[package]", "[workspace]", "[dependencies]", "[features]"}
out, on = [], True
for line in open(src).read().splitlines():
    m = re.match(r"^\s*(\[\[?[^\]]+\]\]?)\s*$", line)
    if m:
        on = m.group(1) in keep
        if on:
            out.append(line)
            if m.group(1) == "[package]":
                out.append("# Staged by modules/scripts/stage-relayer.sh: the library only (no binaries, examples, tests).")
                out += ["autobins = false", "autoexamples = false", "autotests = false", "autobenches = false"]
        continue
    if on:
        out.append(line)
open(dst, "w").write("\n".join(out) + "\n")
PY
grep -q '^lez-wallet = ' "$v/ferrata/cli/Cargo.toml" || { echo "stage-relayer.sh: cli/Cargo.toml has no lez-wallet feature" >&2; exit 1; }

# The LEZ tree.
tar -C "$spike/upstream" --exclude='lez-v0.4.0-rc1/target' --exclude='lez-v0.4.0-rc1/.git' -cf - lez-v0.4.0-rc1 | tar -C "$v/upstream" -xf -

# What was staged, for version().
sum=$( (cd "$repo" && for c in cli $crates; do find "$c/Cargo.toml" "$c/src" -type f; done | LC_ALL=C sort -u | xargs sha256sum) | sha256sum | cut -d' ' -f1)
lez_rev=$(git -C "$lez_up" rev-parse --short HEAD 2>/dev/null || echo "1806e87 (lez/RESULTS.md)")
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/cli/Cargo.toml" | head -n 1)
python3 -I - "$v/STAGED.json" "$version" "$commit" "$sum" "$lez_rev" <<'PY'
import json, sys
out, version, commit, s, lez = sys.argv[1:6]
json.dump({"crate": "ferrata-cli", "version": version, "commit": commit, "source_sha256": s, "lez": "v0.4.0-rc1 " + lez,
           "features": [], "daemon": "ferrata_cli::relayerd (the ferrata-relayer binary's code)"}, open(out, "w"), sort_keys=True)
PY
echo "stage-relayer.sh: ferrata-cli $version (relayerd) into ferrata_relayer_module, commit $commit, sources sha256 $sum, LEZ $lez_rev ($(du -sh "$v" | cut -f1))"
