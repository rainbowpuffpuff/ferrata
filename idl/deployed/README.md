# The interfaces of the deployed programs

`ferrata_bridge_b1.idl.json` and `ferrata_bridge_b1.vectors.json` are the bridge program's IDL and its encoding vectors
as they stood before ADR 0046 (at `65acaa4a`): the interface of `ferrata_bridge_b1` (`0x847a4336…7515`,
[`docs/image-ids.md`](../../docs/image-ids.md)), the program the test network runs. Since ADR 0046 this tree's program
(`ferrata_bridge_f13`) has [`../ferrata_bridge.idl.json`](../ferrata_bridge.idl.json): its claims name no fee schedule
(24 bytes shorter). Whatever reads or writes the test network's bridge transactions uses these frozen copies:
`scripts/verify-live-roundtrip.py`, `scripts/lez-read.py` (`ferrata_bridge_b1`), the explorer's published IDL
(`deploy/testnet/install-web.sh`) and the web wallet's tests. They are replaced when the test network moves to the
program of ADR 0046 ([`deploy/testnet/F13-REDEPLOY.md`](../../deploy/testnet/F13-REDEPLOY.md)).
