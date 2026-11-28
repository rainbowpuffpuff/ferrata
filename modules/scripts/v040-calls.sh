#!/usr/bin/env bash
# Test the attestation and bridge modules' Rust calls crates against LEZ v0.4.0 (a255b42c), beside the repository's
# LEZ v0.4.0-rc1 build. It rewrites manifests, so it runs on a copy of the tree, never on a working tree:
#
#   modules/scripts/v040-calls.sh SRC [MODULE...]
#       SRC     a copy of the tree (`git archive HEAD | tar -x -C SRC`), with LEZ v0.4.0 at
#               SRC/../upstream/lez-v0.4.0 (scripts/fetch-upstream.sh fetches it)
#       MODULE  ferrata_attest_module and/or ferrata_bridge_module (default: both)
#
# Needs bash, python3, cargo, git, patch, sed, and LEZ v0.4.0's Rust dependencies (in cargo's cache or from the
# network). Use a separate CARGO_TARGET_DIR for this build when switching LEZ tags in other copies. The calls crates
# get their own workspaces in the copy, so the Logos SDK scaffold and its sources are not needed; cargo writes their
# standalone Cargo.lock files there. Tests run with RISC0_DEV_MODE=1, as modules/scripts/native.sh runs them.
#
# 1. lez/scripts/lez-select.sh v0.4.0: lez/'s manifests, the `lez-040` feature, and LEZ's lockfile patch;
# 2. each selected module's calls/Cargo.toml and cli/wallet/Cargo.toml (staged into the bridge module): every
#    `upstream/lez-v0.4.0-rc1/` path to `upstream/lez-v0.4.0/`, and the wallet's RISC Zero client pin to 3.1.0;
# 3. stage.py with FERRATA_LEZ_TAG=v0.4.0, an empty [workspace] in each calls manifest, then cargo test there;
# 4. one PASS/FAIL verdict per module, with a failing exit status if any requested module fails.
#
# What PASS covers: the crates build on LEZ v0.4.0's lee crates and RISC Zero 3.1.0, and their tests pass, the bridge
# module's with the wallet's v0.4.0 bridge image id (feature lez-040). The bridge module's network session (net.rs,
# lezkey.rs: its own LEZ v0.4.0-rc3 encoder for the deployed test network) is not ported: it serves only the
# deployment compiled into KNOWN (the rc3 test network's program, configuration, vault and chain) and refuses any
# other network at open, a v0.4.0 one included, before anything is signed.
set -euo pipefail
SRC="$(cd "${1:?usage: $0 SRC [MODULE...]}" && pwd)"; shift
[ ! -e "$SRC/.git" ] || { echo "refused: $SRC is a git working tree; run on a copy (the manifests are rewritten)" >&2; exit 1; }
[ -f "$SRC/../upstream/lez-v0.4.0/Cargo.toml" ] || { echo "LEZ v0.4.0 not found at $SRC/../upstream/lez-v0.4.0" >&2; exit 1; }
if [ $# = 0 ]; then set -- ferrata_attest_module ferrata_bridge_module; fi
for m in "$@"; do
  case "$m" in
    ferrata_attest_module|ferrata_bridge_module) ;;
    *) echo "v040-calls.sh: no module $m (choose ferrata_attest_module or ferrata_bridge_module)" >&2; exit 2 ;;
  esac
done

( cd "$SRC/lez" && { [ "$(scripts/lez-select.sh --show)" = v0.4.0 ] || scripts/lez-select.sh v0.4.0; } )
manifests=("$SRC/cli/wallet/Cargo.toml")
for m in "$@"; do manifests+=("$SRC/modules/$m/rust-lib/calls/Cargo.toml"); done
for manifest in "${manifests[@]}"; do
  sed -i -E 's#upstream/lez-v0\.4\.0-rc1/#upstream/lez-v0.4.0/#g; s#(risc0-zkvm = \{ version = )"=3\.0\.5"#\1"=3.1.0"#' "$manifest"
  ! grep -vE '^\s*#' "$manifest" | grep -qE 'upstream/lez-v0\.4\.0-rc1/|risc0-zkvm = \{ version = "=3\.0\.5"' \
    || { echo "v040-calls.sh: $manifest still names LEZ rc1 or risc0 3.0.5" >&2; exit 1; }
done

FERRATA_LEZ_TAG=v0.4.0 python3 -I "$SRC/modules/scripts/stage.py" "$@"
for m in "$@"; do
  manifest="$SRC/modules/$m/rust-lib/calls/Cargo.toml"
  if ! grep -qE '^\[workspace\][[:space:]]*$' "$manifest"; then
    printf "\n# Standalone in the v0.4.0 copy: do not load the module's Logos SDK scaffold.\n[workspace]\n" >> "$manifest"
  fi
done

failed=0
for m in "$@"; do
  # The bridge module's calls take the wallet's v0.4.0 bridge image id (feature lez-040).
  features=(); [ "$m" != ferrata_bridge_module ] || features=(--features lez-040)
  if (cd "$SRC/modules/$m/rust-lib/calls" && RISC0_DEV_MODE=1 cargo test "${features[@]}"); then
    echo "v040-calls: $m: PASS"
  else
    status=$?
    echo "v040-calls: $m: FAIL (cargo test exited $status)" >&2
    failed=1
  fi
done
exit "$failed"
