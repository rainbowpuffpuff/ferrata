# Frozen wire of the inbound crossing's programs

The inbound crossing (`../inbound/`) deploys two prebuilt binaries and never rebuilds them:
`ferrata_lez.bin` (the attestation program) and `ferrata_bridge.bin` (the bridge program, image
`66722fc7…f4fa`), checked by SHA-256 against `sepolia-plan.json` (`../inbound/run.sh`). The bridge's
image id is part of the `configId` that the Sepolia vault pins, so the real crossing must use
exactly these binaries.

The bridge program in `../../bridge-wire` and `../../guests/bridge` has moved on since: the
registry, fees by leaf index, caps, freezes, roles and `Sunset` (branch `bridge-complete`), merged
with the Gloas header of the crossing's program (branch `inbound-live`). Its messages and its
configuration differ, and so does its image id (`../../BRIDGE-RESULTS.md` §8).

`bridge-wire/` here is `lez/bridge-wire` as it was when the binaries were frozen (branch
`inbound-live` a597270), `src/lib.rs` byte for byte (SHA-256 `a11f87d5…1f50`); only the paths in
its `Cargo.toml` differ. The live driver (`../driver`) builds against it, so its messages are the
deployed program's. It uses `../../wire` as it stands: the attestation wire has not changed since
the binaries were frozen; if it does, freeze a copy of it here too.
