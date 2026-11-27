# Redeploying the bridge under ADR 0048 (RFP-022 F.13): a new vault and a new LEZ configuration

ADR 0048 ([`docs/decisions/0048-protocol-fees-minted-on-lez.md`](../../docs/decisions/0048-protocol-fees-minted-on-lez.md))
changes both halves together: the vault escrows each deposit's mint fee and each release's burn fee and counts them in
`lezFees(token)` (a SHA-256 slot); the bridge program's claim names no fee schedule and mints the denomination and the
relay fee only (as under ADR 0046), and its new instruction `CollectFees` mints the fees the vault counted, in the
wrapped token, to the configuration's fee recipient on LEZ. **Neither half may be paired with the other's earlier
version**: B1's program would mint each claim's mint fee on LEZ and never collect the burn fees, and the new program
refuses claims in B1's layout (24 bytes longer: its fee-schedule range); a vault of ADR 0046 (never deployed) credits
the fees on Ethereum and publishes no `lezFees`, so the new program would collect nothing from it. The test network's
bridge program (`ferrata_bridge_b1`, `0x847a4336…7515`) and the Sepolia vault of 23 November 2026 keep running as
they are; nothing here changes them. Until this procedure is run, this tree's `ferrata`, relayer and bridge module
address that network as it is and build B1's claims for it ([`cli/wallet/src/deployed.rs`](../../cli/wallet/src/deployed.rs),
EVIDENCE row 281), and `ferrata-relayer collect-fees` refuses it (B1 has no `CollectFees`): `ferrata` and the relayer by
the image id the network's program loader names for the bridge account, the module by the image id it records for the
deployment, which it checks against the program account at open. After it, `ferrata` reads `ferrata_bridge_lezfee`'s id
there and builds this tree's claims without a rebuild; the module needs the new deployment (program account,
configuration, vault) added to its list (`KNOWN` in `modules/ferrata_bridge_module/rust-lib/calls/src/net.rs`), as any
new deployment does. This is the owner's procedure: it deploys a new program and a new vault and sends transactions on
Sepolia and on the test network. **Not run** by this project.

What is new and what stays:

| Piece | Before (deployed) | After (this tree) |
|---|---|---|
| bridge program | `ferrata_bridge_b1` `0x847a4336…7515`, binary sha256 `537cbe7a…` | `ferrata_bridge_lezfee` `0x2ba8dc18…17fb`, binary sha256 `9cd9d357…` ([`docs/image-ids.md`](../../docs/image-ids.md)) |
| attestation program, release and checkpoint guests | unchanged | unchanged (19 of 20 guests rebuild to their recorded ids) |
| vault | v5, mint fee escrowed and minted on LEZ by each claim, burn fee credited on Ethereum | v5 with ADR 0048's deposit, release and refund: both protocol fees escrowed and counted in `lezFees` |
| clients (`ferrata`, the module, crown-driver, relayer) | claims with a range | this tree's `ferrata` builds either layout by the image id the zone's loader names, the module by its deployment's recorded id (B1's today); the relayer collects fees (`collect-fees`) on a zone of `ferrata_bridge_lezfee` only; crown-driver builds no claim for this network |
| the web wallet (`deploy/testnet/web/wallet`), the explorer's IDL, `scripts/lez-read.py` | speak B1's claims (`idl/deployed/`) | not updated in this tree: the web wallet's claim encoder (`claim.js`) and the three readers move to `idl/ferrata_bridge.idl.json` with the redeploy (`idl/deployed/README.md`) |

## 1. Build and check (any machine; no key)

```sh
scripts/fetch-upstream.sh
scripts/reproduce-image-ids.sh build --out ../ids-lezfee         # every guest of this tree
scripts/reproduce-image-ids.sh compare --out ../ids-lezfee       # 20 of 20 MATCH, ferrata_bridge as ferrata_bridge_lezfee
sha256sum ../ids-lezfee/programs/ferrata_bridge.bin              # 9cd9d3573562cb5a8452e413ee17b402bbf963d753e2c55294708a23c29d2aed
cd vault && forge test --no-match-contract PrivacySuite          # the vault of ADR 0048
```

The rc3 tools (`deploy/testnet/cli-rc3/build.sh`, the crown driver with `lez/scripts/lez-select.sh v0.4.0-rc3`, the
relayer, the monitor) are rebuilt from this tree; record their sha256 and put them in `deploy/testnet/install-bridge.sh`'s
pins in place of the earlier ones (the pins name the binaries this project built and tested; a rebuild on another
machine has its own sums: the README's "What the sha256 pins of Rust binaries mean").

## 2. LEZ: deploy the program of ADR 0048 (the test network's host, as root)

The day-one kit in this tree pins this tree's bridge program (`lez/testnet04/day1.sh`: `BRIDGE_SHA256`,
`BRIDGE_IMAGE`). `deploy-programs.sh` installs the bridge binary from `/opt/ferrata/programs/ferrata_bridge_b1.bin`
(the name of the program it first deployed); give it this tree's binary under a name of its own and point the copy at
it, then run only the kit's bridge stage on the existing network:

```sh
install -m 644 ferrata_bridge.bin /opt/ferrata/programs/ferrata_bridge_lezfee.bin   # sha256 9cd9d357…
sed -i 's#/opt/ferrata/programs/ferrata_bridge_b1.bin#/opt/ferrata/programs/ferrata_bridge_lezfee.bin#' \
  /opt/ferrata/src/ferrata/deploy/testnet/deploy-programs.sh
deploy/testnet/deploy-programs.sh plan      # the plan names the bridge program 0x2ba8dc18…; check before sending
deploy/testnet/deploy-programs.sh run       # (e) deploys it if it is not on chain
```

`day1.sh run` skips what is on chain: (a) when the attestation program is there byte for byte, (b) when the instance
exists, (d) when its attestation is applied, and (e) deploys the bridge program only when it is not. **Check (c)
first**: the kit starts its updater daemon unless its own pid file names a running one, and on this host the updater
was handed to systemd (`deploy-programs.sh handover`, unit `ferrata-updater`). Run `lez/testnet04/day1.sh
daemon-status` before `run`; if it does not report the systemd updater, do not run `run` (a second updater would spend
the instance's payer twice for nothing): this project has not rehearsed `run` on an already-deployed network. Record
the program account (e) prints (`bridge-owner-step.md` in the kit's state).

## 3. Sepolia: the vault of ADR 0048

`bridge-owner-step.md` (written by the kit) gives the configuration's fields; fill a params file from
`vault/script/params/sepolia-31.template.json` with them (the new program's `bridgeHeader` and `bridgeImageId`
`0x2ba8dc18…17fb`, the vault address `--expect-vault` predicts) and deploy with the existing tool, dry first:

```sh
MODAL_PROFILE=<workspace with the deployer key> modal run vault/script/deploy_sepolia.py --params P.json \
  --freeze-authority 0x… --watchers 0x… --expect-vault 0x… --dry
MODAL_PROFILE=<same> modal run vault/script/deploy_sepolia.py --params P.json \
  --freeze-authority 0x… --watchers 0x… --expect-vault 0x…
```

The vault's `feeRecipient` parameter stays required (the deployment parameters did not change) but receives nothing
under ADR 0048; the fee recipient that is paid is the LEZ configuration's (`Controls::fee_recipient`, in the
configuration `Init` writes and `configId` commits to).

## 4. LEZ: the configuration, the token, the operators

As for the bridge demo today (`deploy/testnet/README.md`, "The Sepolia bridge demo"): `Init` (pairing: the vault's slot
2), `Register` ETH (no `ImportFees` is needed for claims or collections), then the relayer, the checkpoint prover and
the monitor from this tree's binaries. Publish the new program in the site's `programs.json` beside B1's
(`deploy-programs.sh publish`), and say on the testers' page which configuration the tools speak. Protocol fees start
at zero; a fee change is the admin's `announceFees` on the vault, in force seven days later.

## 5. What to record afterwards

The program's deploy transactions and account, the vault's deployment record, the configuration id, and a first round
trip (`docs/VERIFY.md` §5 with this tree's `ferrata`): a deposit under a nonzero mint fee showing `lezFees` grow by it
(`LezFeesAccrued`) and no Ethereum credit, its claim minting the denomination and the relay fee only, then
`ferrata-relayer ... collect-fees --token 0x0000000000000000000000000000000000000000` minting the fee to the fee
recipient on LEZ (its JSON line names the record, the total and the fee recipient's holding before and after), and,
after a release under a nonzero burn fee, a second collection of that fee. The same steps were rehearsed on anvil and a
local LEZ v0.4.0-rc3 sequencer (`e2e/f13/hook.sh`; its record is cited by the F.13 row). Then
`python3 -I scripts/verify-sepolia.py` on two endpoints, as for the earlier vaults.
