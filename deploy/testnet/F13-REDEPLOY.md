# Redeploying the bridge under ADR 0046 (RFP-022 F.13): a new vault and a new LEZ configuration

ADR 0046 changes both halves together: the vault credits each deposit's mint fee to the fee recipient and escrows the
denomination and the relay fee; the bridge program's claim names no fee schedule and mints the denomination and the
relay fee only. **Neither half may be paired with the other's earlier version**: the earlier program would mint a mint
fee the new vault never escrowed, and the new program refuses claims in the earlier layout (B1's claim is 24 bytes longer:
its fee-schedule range). The test network's bridge program (`ferrata_bridge_b1`, `0x847a4336…7515`) and the Sepolia
vault of 23 November 2026 keep running as they are; nothing here changes them. Until this procedure is run, this tree's
`ferrata` and bridge module address that network as it is and build B1's claims for it
([`cli/wallet/src/deployed.rs`](../../cli/wallet/src/deployed.rs), EVIDENCE row 281): `ferrata` by the image id the
network's program loader names for the bridge account, the module by the image id it records for the deployment, which
it checks against the program account at open. After it, `ferrata` reads `ferrata_bridge_f13`'s id there and builds this
tree's claims without a rebuild; the module needs the new deployment (program account, configuration, vault) added to
its list (`KNOWN` in `modules/ferrata_bridge_module/rust-lib/calls/src/net.rs`), as any new deployment does. This is the owner's procedure: it deploys a new
program and a new vault and sends transactions on Sepolia and on the test network. **Not run** by this project.

What is new and what stays:

| Piece | Before (deployed) | After (this tree) |
|---|---|---|
| bridge program | `ferrata_bridge_b1` `0x847a4336…7515`, binary sha256 `537cbe7a…` | `ferrata_bridge_f13` `0xffb47f6e…7675`, binary sha256 `c84fe2db…` ([`docs/image-ids.md`](../../docs/image-ids.md)) |
| attestation program, release and checkpoint guests | unchanged | unchanged (19 of 20 guests rebuild to their recorded ids) |
| vault | v5, mint fee escrowed and minted on LEZ | v5 with ADR 0046's deposit and refund |
| clients (`ferrata`, the module, crown-driver, relayer) | claims with a range | this tree's `ferrata` builds either layout by the image id the zone's loader names, the module by its deployment's recorded id (B1's today); crown-driver and the relayer build no claim for this network |
| the web wallet (`deploy/testnet/web/wallet`), the explorer's IDL, `scripts/lez-read.py` | speak B1's claims (`idl/deployed/`) | not updated in this tree: the web wallet's claim encoder (`claim.js`) and the three readers move to `idl/ferrata_bridge.idl.json` with the redeploy (`idl/deployed/README.md`) |

## 1. Build and check (any machine; no key)

```sh
scripts/fetch-upstream.sh
scripts/reproduce-image-ids.sh build --out ../ids-f13            # every guest of this tree
scripts/reproduce-image-ids.sh compare --out ../ids-f13          # 20 of 20 MATCH, ferrata_bridge as ferrata_bridge_f13
sha256sum ../ids-f13/programs/ferrata_bridge.bin                 # c84fe2dbbc008c1a94c096fb6c598eb1580da2996b6b1e54ae807d0041e08fdc
cd vault && forge test --no-match-contract PrivacySuite          # the vault of ADR 0046 (636 passed, 11 skipped, at this commit)
```

The rc3 tools (`deploy/testnet/cli-rc3/build.sh`, the crown driver with `lez/scripts/lez-select.sh v0.4.0-rc3`, the
relayer, the monitor) are rebuilt from this tree; record their sha256 and put them in `deploy/testnet/install-bridge.sh`'s
pins in place of the earlier ones (the pins name the binaries this project built and tested; a rebuild on another
machine has its own sums: the README's "What the sha256 pins of Rust binaries mean").

## 2. LEZ: deploy the program of ADR 0046 (the test network's host, as root)

The day-one kit in this tree pins this tree's bridge program (`lez/testnet04/day1.sh`: `BRIDGE_SHA256`,
`BRIDGE_IMAGE`). `deploy-programs.sh` installs the bridge binary from `/opt/ferrata/programs/ferrata_bridge_b1.bin`
(the name of the program it first deployed); give it this tree's binary under a name of its own and point the copy at
it, then run only the kit's bridge stage on the existing network:

```sh
install -m 644 ferrata_bridge.bin /opt/ferrata/programs/ferrata_bridge_f13.bin   # sha256 c84fe2db…
sed -i 's#/opt/ferrata/programs/ferrata_bridge_b1.bin#/opt/ferrata/programs/ferrata_bridge_f13.bin#' \
  /opt/ferrata/src/ferrata/deploy/testnet/deploy-programs.sh
deploy/testnet/deploy-programs.sh plan      # the plan names the bridge program 0xffb47f6e…; check before sending
deploy/testnet/deploy-programs.sh run       # (e) deploys it if it is not on chain
```

`day1.sh run` skips what is on chain: (a) when the attestation program is there byte for byte, (b) when the instance
exists, (d) when its attestation is applied, and (e) deploys the bridge program only when it is not. **Check (c)
first**: the kit starts its updater daemon unless its own pid file names a running one, and on this host the updater
was handed to systemd (`deploy-programs.sh handover`, unit `ferrata-updater`). Run `lez/testnet04/day1.sh
daemon-status` before `run`; if it does not report the systemd updater, do not run `run` (a second updater would spend
the instance's payer twice for nothing): this project has not rehearsed `run` on an already-deployed network. Record
the program account (e) prints (`bridge-owner-step.md` in the kit's state).

## 3. Sepolia: the vault of ADR 0046

`bridge-owner-step.md` (written by the kit) gives the configuration's fields; fill a params file from
`vault/script/params/sepolia-31.template.json` with them (the new program's `bridgeHeader` and `bridgeImageId`
`0xffb47f6e…7675`, the vault address `--expect-vault` predicts) and deploy with the existing tool, dry first:

```sh
MODAL_PROFILE=<workspace with the deployer key> modal run vault/script/deploy_sepolia.py --params P.json \
  --freeze-authority 0x… --watchers 0x… --expect-vault 0x… --dry
MODAL_PROFILE=<same> modal run vault/script/deploy_sepolia.py --params P.json \
  --freeze-authority 0x… --watchers 0x… --expect-vault 0x…
```

## 4. LEZ: the configuration, the token, the operators

As for the bridge demo today (`deploy/testnet/README.md`, "The Sepolia bridge demo"): `Init` (pairing: the vault's slot
2), `Register` ETH (no `ImportFees` is needed for claims any more: ADR 0046), then the relayer, the checkpoint prover and
the monitor from this tree's binaries. Publish the new program in the site's `programs.json` beside B1's
(`deploy-programs.sh publish`), and say on the testers' page which configuration the tools speak.

## 5. What to record afterwards

The program's deploy transactions and account, the vault's deployment record, the configuration id, and a first round
trip (`docs/VERIFY.md` §5 with this tree's `ferrata`): a deposit under a nonzero mint fee showing the fee recipient's
`owed` credit, its claim minting the denomination and the relay fee only. Then `python3 -I scripts/verify-sepolia.py`
on two endpoints, as for the earlier vaults.
