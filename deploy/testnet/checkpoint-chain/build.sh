#!/usr/bin/env bash
# realblocks-crown (checkpoint/realblocks) with checkpoints on posted ones: --parent-state FILE builds the next rule (B)
# checkpoint on the state a posted checkpoint ended at, and every run writes that state (state.json) beside its input.
# Without it the tool builds from the configuration's genesis state only, so the vault could take one checkpoint and
# no later burn could be released. ferrata-bridge-checkpoint uses it.
#
#   deploy/testnet/checkpoint-chain/build.sh SRC      SRC: a checkout of the bridge branch (checkpoint/ at commit
#                                                     0c5906d5 or a later one the patch applies to)
#
# realblocks-chain.patch: src/parent_state.rs (the state file: written, read back, checked against its digest and the
# frontier's canonical form), realblocks-crown's --parent-state (T0 = its tip, its key set, configuration tip, burn
# frontier, imported count and sunset; its tip hash checked against the node's inscription; with --genesis-state, then
# the vault's finalState, its digest checked), the burns of a span continuing the parent's tree, and tests/chained.rs
# (hand-built blocks of a one-key zone in the 0.3.5-rc.1 layout: two chained checkpoints end at the state one
# checkpoint over the same blocks ends at; the second span on the genesis parent is refused; bad state files are refused).
set -euo pipefail
SRC="$(cd "${1:?usage: $0 SRC}" && pwd)"; HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
( cd "$SRC" && { patch -p1 -R --dry-run -s < "$HERE/realblocks-chain.patch" > /dev/null 2>&1 || patch -p1 < "$HERE/realblocks-chain.patch"; } )
( cd "$SRC/checkpoint" && cargo test --release --locked -p realblocks --test chained && cargo build --release --locked -p realblocks --bin realblocks-crown )
sha256sum "${CARGO_TARGET_DIR:-$SRC/checkpoint/target}/release/realblocks-crown"
