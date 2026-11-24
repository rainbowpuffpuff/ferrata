#!/usr/bin/env bash
# The `ferrata` CLI (cli/, at the commit README "Artifacts" names) built against LEZ v0.4.0-rc3, the LEZ of this testnet
# (cli/ links v0.4.0-rc1): the owner's release tool (`ferrata release`), and claim/burn/recover for anyone on this zone.
#
#   deploy/testnet/cli-rc3/build.sh SRC        SRC: a checkout of that commit (`git archive <commit> | tar -x`), with
#                                              LEZ v0.4.0-rc3 at SRC/../upstream/lez-v0.4.0-rc3 (as for lez/)
#
# 1. lez/scripts/lez-select.sh v0.4.0-rc3 (lez/'s manifests, features and lockfiles);
# 2. the LEZ paths of every other manifest on v0.4.0-rc3 (cli/, cli/wallet/ and the crates lez-select does not list);
# 3. cli-rc3.patch: the zone read in pages of 50 blocks, each tried
#    again after a pause (the public RPC caps a range at 50 and answers 429 while a caller's budget refills); the
#    vault's logs tried again while a load-balanced Ethereum endpoint's backend lags the head another reported;
#    `release --input-out FILE` also writes FILE with extension .json: {"kind": "release", "journal": hex}, the journal
#    the remote prover holds the proof to (ferrata-bridge-prover --meta);
# 4. cli/Cargo.lock replaced by Cargo.lock.v0.4.0-rc3 here (resolved for rc3's crates once, 23 Nov 2026), then
#    cargo build --release --locked --features lez-rc3 (rc3's `ExecutionFailureKind::DryRun(report)`, in cli/ since)
#    --bin ferrata --bin ferrata-relayer (the network's gas-drop relayer, host/systemd/ferrata-gasdrop.service)
#    --bin ferrata-gasdrop (the gateway's receipt checker, `check-job`); then --features lez-rc3,seqnet --bin seqnet-fund
#    (the drop account's funding, `ferrata-gasdrop-setup fund`). The tree's own build.sh goes with its cli/: a cli/
#    older than 847bb4e5 (no `lez-rc3` feature, e.g. 0c5906d5) is built with the build.sh of its own commit.
# The binary is linked to glibc and libpcsclite.so.1 (pcsc-lite) of the build machine.
set -euo pipefail
SRC="$(cd "${1:?usage: $0 SRC}" && pwd)"; HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[ -f "$SRC/../upstream/lez-v0.4.0-rc3/Cargo.toml" ] || { echo "LEZ v0.4.0-rc3 not found at $SRC/../upstream/lez-v0.4.0-rc3" >&2; exit 1; }
( cd "$SRC/lez" && { [ "$(scripts/lez-select.sh --show)" = v0.4.0-rc3 ] || scripts/lez-select.sh v0.4.0-rc3; } )
grep -rl --include=Cargo.toml 'upstream/lez-v0.4.0-rc1/' "$SRC" | xargs -r sed -i 's#upstream/lez-v0.4.0-rc1/#upstream/lez-v0.4.0-rc3/#'
( cd "$SRC" && { patch -p1 -R --dry-run -s < "$HERE/cli-rc3.patch" > /dev/null 2>&1 || patch -p1 < "$HERE/cli-rc3.patch"; } )
install -m 644 "$HERE/Cargo.lock.v0.4.0-rc3" "$SRC/cli/Cargo.lock"
( cd "$SRC/cli" && cargo build --release --locked --features lez-rc3 --bin ferrata --bin ferrata-relayer --bin ferrata-gasdrop \
  && cargo build --release --locked --features lez-rc3,seqnet --bin seqnet-fund )
OUT="${CARGO_TARGET_DIR:-$SRC/cli/target}/release"
sha256sum "$OUT/ferrata" "$OUT/ferrata-relayer" "$OUT/ferrata-gasdrop" "$OUT/seqnet-fund"
