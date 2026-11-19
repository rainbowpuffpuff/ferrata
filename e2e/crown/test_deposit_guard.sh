#!/usr/bin/env bash
# e2e/crown/sepolia.sh deposit never sends a deposit that may already be in flight. This was the marker guard's test
# (a ".sending" marker that stopped every later run until a human removed it); the marker is replaced by the network
# journal (scripts/pubnet.py, e2e/NETWORK.md), and this command now runs the journal's guard test, which covers the
# same cases on a real local anvil: a pending transaction of the depositor (nothing sent), a send that began and did
# not finish (nothing sent until its fate is known), the fate found (the stage done, one transaction), and more.
#
#   bash e2e/crown/test_deposit_guard.sh
exec bash "$(cd "$(dirname "$0")" && pwd)/test_send_guard.sh" "$@"
