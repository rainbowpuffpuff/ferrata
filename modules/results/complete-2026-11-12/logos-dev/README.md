# Through Logos Core (dev `logosctl`), 12 November 2026

`modules/scripts/check.sh` (PORTABLE=0 E2E=1 BRIDGE_E2E=1) on the Modal build box: `steps.md` (every step,
seconds), `summary.md` (every static call through Logos Core against native, and the two end-to-end runs'
checks), `replies/` (the static calls' replies through the daemon), `logs/` (the last 60 lines of each step's
log), `e2e/` (e2e.sh through the daemon: its checks; of its replies only `advance`'s are kept here, the
full set of an identical native run is in `../e2e-native/rep`), `bridge-e2e/` (bridge_e2e.sh through the
daemon: every request and the daemon's raw reply).
