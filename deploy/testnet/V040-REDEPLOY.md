# Moving the public test network from LEZ v0.4.0-rc3 to LEZ v0.4.0

**Not run by this project.** This is the owner’s planned whole-network regenesis, not an in-place upgrade. The present tree still fails the gates in §2 and §6: **do not start this procedure until every gate passes and the rule (B) result below is accepted.** Send the downtime announcement through the network’s normal owner channel.

## 1. What runs today and what changes

Today’s host runs the units and pinned artifacts in `deploy/testnet/README.md`: one Bedrock validator, one sequencer, LEZ’s indexer/explorer, the gateway/faucet, attestation updater/monitor, and the Sepolia bridge services. The new release starts a new Bedrock chain and a new LEZ chain. State is not carried over: no rc3 balance, account, program, instance, transaction, or explorer link exists on v0.4.0.

| Piece | Today (rc3) | After (v0.4.0) | Why (LEZ PR) |
|---|---|---|---|
| LEZ | `v0.4.0-rc3`, `bafd07be0701398f9bc435f73ab05e9a7936c301` | `v0.4.0`, `a255b42c0fc80d1cb219454a3a1f5ec3b5d92115`; released 27 Nov 2026 | release pin |
| Bedrock | Logos Blockchain 0.3.5-rc.1, `V1` settings | 0.4.0-rc.2 through zone-sdk v0.2.0, `V2` settings; fresh node state | 1154 |
| LEZ genesis/state | `reverted`; `exit_delay`; stake entries with public key/signature | receipts; `finality_depth`; stake entries with ownership id/witness and committee-voted parameters; regenesis | 1107, 1026, 1089 |
| Public accounts | ids derived from public keys; 32-byte genesis keys | ids derive from typed initial keys; genesis keys are 33 bytes; make operator/faucet keys again | 1135 |
| Wallet | rc3 import uses `--private-key`; storage sealed by a password (`LEE_WALLET_PASSWORD` or the terminal), already in rc3 | import uses `--secret`; never import an rc3 key—it derives another, empty account; storage sealed as before | 1135 (987 is in rc3) |
| Proving/programs | risc0 3.0.5 and its image ids/program accounts | risc0 3.1.0: every image id changes; rebuild and redeploy all programs; initialise a new Sepolia instance | 1124 |
| Fees/messaging | previous fee and cross-zone configuration | private transactions metered by size; a signing public account may pay a private fee; receipts report payment; cross-zone messaging removed | 1044, 1080, 1107, 1147 |
| Bridge | Sepolia vault bound to rc3 configuration and rule (A)/(B) | new configuration and vault; rebuild rule (A)/(B) for 0.4.0-rc.2 blocks; initialise pairing again | 1154 plus bridge deployment |

Result of the rule (B) check on a v0.4.0 zone (28 Nov 2026, [`e2e/runs/zone-v040-four-keys-1/`](../../e2e/runs/zone-v040-four-keys-1/README.md)): **refused.** A zone of four v0.4.0 sequencers on a 0.4.0-rc.2 node works (every key inscribes in turn), but this tree's rc3-era attestor stops at the first LEZ block in the channel (`NotStrictlyOrdered { what: "actors" }`: v0.4.0 encodes a transaction's actors as a map, LEZ PR 1144). The Bedrock layer (0.4.0-rc.2's block JSON, inscriptions) was read. `checkpoint_b_0_3_5_31` shares that decoder, so the bridge's checkpoints need rule (A)/(B) programs and an attestor built for v0.4.0's LEZ layout (new image ids, a new vault pinning them) before the bridge can move. Since then `lez-040-core` has added checkpoint-core's v0.4.0 LEZ decoder (feature `lez-0-4-0`; it parses, re-hashes and links all 107 blocks of that run). The attestor and a v0.4.0 rule (B) program are not built on it yet, so the result stands. In the bridge run on v0.4.0 ([`e2e/runs/zone-bridge-v040/`](../../e2e/runs/zone-bridge-v040/README.md)), both deposits were claimed (the private one with a real proof) and both burns applied, but the attestors signed no tip: no checkpoint, no release.

Evidence boundary: the supplied upstream snapshots confirm the `V2`, genesis-key, receipt, stake-layout, `finality_depth`, and `--secret` forms. They carry lockfiles (`Cargo.lock`, `flake.lock`) but no git history. In a git clone of LEZ, v0.4.0 (`a255b42c`) and v0.4.0-rc4 (`313d2a72`) have the same git tree (`3049ef3065d54e7473f8d203200e8289fdfb7c75`, `git rev-parse <tag>^{tree}`): the release is rc4's code. PR provenance, size metering/payment details, non-migration code, and the old-key empty-account runtime result are **not confirmed** there. The release and Bedrock/risc0 pins are independently recorded in `checkpoint/attestor/zone-lib.sh`. Both supplied wallet snapshots mention passwords, so a password *introduced between these tags* is not confirmed. These boundaries do not make reuse of old state or keys safe.

## 2. Before you start: hard gates

Work from a reviewed, committed tree containing the external v0.4.0 ports; `deploy.sh bundle` archives `HEAD`, not uncommitted edits. Set artifacts on the build workstation:

```sh
export ART=/path/to/v040-host-binaries PROGRAMS_DIR=/path/to/v040-programs WEB_TGZ=/path/to/v040-web.tar.gz
export FERRATA_LEZ_TAG=v0.4.0 D=deploy/testnet/deploy.sh STACK="${STACK:-ferrata-testnet}"
git status --short                         # no uncommitted source/dependency/deployment changes
```

1. **Selector/manifests/lockfiles and all program binaries.** `lez/scripts/lez-select.sh v0.4.0` must succeed. The 15 v0.4.0 guest records exist (`lez/results/v0.4.0/image-ids.md`; `docs/image-ids.md`'s "LEZ v0.4.0" section from lez-040-verify). The gate is to rebuild the reviewed tree, stage the binaries, and port the publisher pins that are still rc3's: `deploy.sh bundle` (it requires `ferrata_bridge_b1.bin` and writes rc3's `SHA256SUMS`), day1/publish (`deploy.sh` and `lez/testnet04/day1.sh` publish rc3 ids), the extra sites and the IDL. No artifact is approved for deployment until these checks pass. Verify, rather than trusting filenames:

   ```sh
   lez/scripts/lez-select.sh v0.4.0 && test "$(lez/scripts/lez-select.sh --show)" = v0.4.0
   test -d lez/lockfiles && test -n "$(find lez/lockfiles -maxdepth 1 -name '*v0.4.0*' -print -quit)" && grep -F '3.1.0' docs/image-ids.md
   ( cd "$PROGRAMS_DIR" && sha256sum -c SHA256SUMS )
   for f in ferrata_lez ferrata_bridge ferrata_holdings ferrata_consumer; do   # v0.4.0's bridge guest: ferrata_bridge (0x7e39359d…), not B1
     test -s "$PROGRAMS_DIR/$f.bin" && r0vm --elf "$PROGRAMS_DIR/$f.bin" --id
   done
   bash scripts/verify-lez040-ids.sh   # rebuild and compare each v0.4.0 record (lez-040-verify; reproduce-image-ids.sh is pinned to rc1)
   ```

   Record each output beside the updated `docs/image-ids.md`; each id must be its v0.4.0 value. Redeploy every additional site program too (holdings, consumer, names, messages, and any others in the current `/programs.json`).

2. **Ferrata services and users’ clients.** `ferrata-day1`, `ferrata-updater`, and `attest-monitor` must be built and exercised against v0.4.0 elsewhere in this project, with their sha256 recorded. The updater must use the final RISC Zero dependencies; day1 must create typed 33-byte genesis keys. The gateway/faucet, published-account sweep, gas-drop tools, status block reader, and Bedrock funding/top-up helpers must pass the same rehearsal. Ship only v0.4.0 web-wallet and `ferrata` CLI builds plus the gas-drop guest.

   ```sh
   for f in ferrata-day1 ferrata-updater attest-monitor ferrata ferrata-relayer ferrata-gasdrop seqnet-fund; do test -x "$ART/$f"; done
   sha256sum "$ART/ferrata-day1" "$ART/ferrata-updater" "$ART/attest-monitor" "$ART/ferrata" "$ART/gasdrop.bin" "$WEB_TGZ"
   test -s "$WEB_TGZ" && tar -tzf "$WEB_TGZ" >/dev/null
   ```

3. **Bridge.** Stage v0.4.0 bridge/attestor/relayer/crown-driver binaries, a release guest, and rule (A)/(B) guests proven to read 0.4.0-rc.2 blocks. The rule (B) result above must say accepted. On v0.4.0, `install-bridge.sh` takes every artifact's sha256 from the operator's pins file (`FERRATA_BRIDGE_PINS`, in `sha256sum` format, naming the five binaries, `release.bin` and the rule (B) program `FERRATA_BRIDGE_CHECKPOINT_B` once each; rc3's pins are never used on v0.4.0). It also takes the rule (A)/(B) and release image ids (`FERRATA_CHECKPOINT_IMAGE_A`, `FERRATA_CHECKPOINT_IMAGE_B`, `FERRATA_RELEASE_IMAGE_ID`) and checks all of them before installing anything (`deploy/testnet/tests/test_bridge_tag.py`, which also runs §3.7's command). Write those values from the v0.4.0 builds before this step. Stage them on the host as `/opt/ferrata/bridge-v040-bin` and `/opt/ferrata/programs/31-v040` after stopping the network.

4. **Recovery.** Identify the current data volume and confirm an earlier snapshot is readable. The procedure creates a quiesced `$SNAP` and an rc3 archive before changing anything. Do not proceed until `snapshot-completed` finishes and the archive checks pass.

   ```sh
   IID="$(aws cloudformation describe-stacks --stack-name "$STACK" --query 'Stacks[0].Outputs[?OutputKey==`InstanceId`].OutputValue' --output text)"
   VOL="$(aws cloudformation describe-stacks --stack-name "$STACK" --query 'Stacks[0].Outputs[?OutputKey==`DataVolumeId`].OutputValue' --output text)"
   aws ec2 describe-volumes --volume-ids "$VOL"
   aws ec2 describe-snapshots --owner-ids self --filters Name=volume-id,Values="$VOL" --query 'Snapshots[-1].[SnapshotId,State]'
   ```

## 3. Procedure

Run from the reviewed tree. `deploy.sh` sources `lez-tag.sh` and injects the selection into `provision`, `run`, and `programs` over SSM (`deploy.sh` lines 80–89). **Export `FERRATA_LEZ_TAG=v0.4.0` in the workstation shell for every invocation, including separate SSM sessions.** `deploy.sh sh` does not forward it: put `FERRATA_LEZ_TAG=v0.4.0` explicitly inside each remote command that invokes a testnet script. In particular, `run install` reaches `provision.sh install`, then `install.sh`, with the variable set.

### 3.1 Announce, record rc3, and stop writes

Announce a full reset: balances and accounts will not migrate, the bridge will close, and the completion time will be announced after verification. Save the rc3 status/output and the old vault address, then stop and disable every writer and public entry point:

```sh
mkdir -p .scratch; STAMP="$(date -u +%Y%m%dT%H%M%SZ)"; "$D" status > ".scratch/rc3-status-$STAMP.txt"
"$D" sh 'systemctl disable --now caddy ferrata-gasdrop ferrata-updater ferrata-monitor ferrata-bridge-relayer ferrata-bridge-monitor ferrata-bridge-prover ferrata-bridge-attestor ferrata-bridge-import.timer ferrata-topup.timer ferrata-status.timer ferrata-gateway ferrata-explorer ferrata-indexer ferrata-sequencer ferrata-node'
"$D" sh 'systemctl is-active ferrata-node ferrata-sequencer ferrata-gateway || true'
```

Expected: no unit remains `active`; public RPC is unavailable. `ferrata-node-api-guard` may stay active. Keep `$STAMP` for rollback.

### 3.2 Archive rc3, then snapshot the stopped data volume

Write the archive **before** the snapshot so that a restored volume contains the rc3 root binaries and configuration as well as the old live state:

```sh
"$D" sh "set -eu; A=/var/lib/ferrata/rc3-$STAMP; install -d -m 700 \"\$A\" \"\$A/opt/src\" \"\$A/etc\" \"\$A/srv\" \"\$A/var/log\"; for p in /opt/ferrata/bin /opt/ferrata/programs /opt/ferrata/explorer /opt/ferrata/explorer-site; do cp -a \"\$p\" \"\$A/opt/\"; done; cp -a /opt/ferrata/src/ferrata \"\$A/opt/src/\"; cp -a /etc/ferrata \"\$A/etc/\"; cp -a /srv/ferrata/www \"\$A/srv/\"; cp -a /var/log/ferrata \"\$A/var/log/\"; cp -a /opt/ferrata/provision.sh \"\$A/opt/\"; if [ -f /opt/ferrata/lez-tag.sh ]; then cp -a /opt/ferrata/lez-tag.sh \"\$A/opt/\"; fi"
"$D" sh 'sync'
SNAP="$(aws ec2 create-snapshot --volume-id "$VOL" --description "Ferrata rc3 before v0.4.0 $STAMP" --tag-specifications 'ResourceType=snapshot,Tags=[{Key=Project,Value=ferrata-testnet},{Key=Name,Value=ferrata-rc3-pre-v040}]' --query SnapshotId --output text)"
printf '%s %s %s\n' "$STAMP" "$VOL" "$SNAP"      # record this
"$D" sh "printf 'snapshot=$SNAP\n' > /var/lib/ferrata/rc3-$STAMP/ARCHIVE-INFO"  # metadata only: snapshot itself precedes it
aws ec2 wait snapshot-completed --snapshot-ids "$SNAP"
aws ec2 describe-snapshots --snapshot-ids "$SNAP" --query 'Snapshots[0].[SnapshotId,State,Progress]'
```

Expected state `completed`, progress `100%`. The old volume and snapshot are never deleted by this runbook.

### 3.3 Keep rc3 state aside

```sh
"$D" sh "set -eu; A=/var/lib/ferrata/rc3-$STAMP; for d in node sequencer indexer day1 keys faucet updater monitor status gasdrop bridge web-served; do test ! -e \"/var/lib/ferrata/\$d\" || mv \"/var/lib/ferrata/\$d\" \"\$A/\"; done; test -f \"\$A/keys/operator/payer.key\"; test -f \"\$A/opt/bin/sequencer_service\"; test ! -e /var/lib/ferrata/keys"
```

Expected: all checks silent/successful; no old key or chain directory remains at its live path. Never delete this archive. The snapshot contains the root-artifact archive made above and the rc3 live directories before they were moved.

### 3.4 Fetch and build v0.4.0

Upload the v0.4.0 source/programs/site, then provision in stages. `deploy.sh run` performs `fetch` first, so do not repeat `fetch`. With `FERRATA_LEZ_TAG=v0.4.0` it first runs the host's installed `/opt/ferrata/provision.sh fetch` (an rc3 host's copy predates `lez-tag.sh`), puts this tree's `provision.sh` and `lez-tag.sh` in its place, and fetches again with them (`deploy/testnet/tests/test_lez_tag.py` checks the command).

```sh
PROGRAMS_DIR="$PROGRAMS_DIR" WEB_TGZ="$WEB_TGZ" FERRATA_LEZ_TAG=v0.4.0 "$D" bundle
FERRATA_LEZ_TAG=v0.4.0 "$D" run toolchain build-zone build-ferrata build-explorer
"$D" sh 'test "$(git -C /opt/ferrata/src/upstream/lez-v0.4.0 rev-parse HEAD)" = a255b42c0fc80d1cb219454a3a1f5ec3b5d92115; /opt/ferrata/bin/r0vm --version; cat /opt/ferrata/bin/bedrock/metadata.json; cat /opt/ferrata/bin/zone.sha256 /opt/ferrata/bin/ferrata.sha256 /opt/ferrata/bin/explorer.sha256'
```

Expected: the exact commit, r0vm **3.1.0**, Bedrock **0.4.0-rc.2**, all stages `done`, and recorded sums. A selector, dependency, monitor, updater, or recursion-artifact failure stops the migration; restore service from §5 rather than improvising on the live host.

### 3.5 New keys, genesis, node, and sequencer

```sh
FERRATA_LEZ_TAG=v0.4.0 "$D" run install
"$D" sh 'set -eu; grep -Fx v0.4.0 /var/lib/ferrata/keys/operator/payer.key.tag /var/lib/ferrata/keys/faucet/payer.key.tag; test "$(for k in operator faucet; do jq -r .payer_public_key "/var/lib/ferrata/keys/$k/accounts.json"; done | grep -Ec "^00[0-9a-f]{64}$")" = 2 || { echo "typed keys: not 2" >&2; exit 1; }; echo "typed keys: 2"; grep "^  0: !V2$" /var/lib/ferrata/node/deployment-settings.yaml; systemctl is-active ferrata-node ferrata-sequencer ferrata-indexer; curl -fsS http://127.0.0.1:18080/leader/aged-notes | jq -e ".count > 0"; curl -fsS http://127.0.0.1:3040 -H "content-type: application/json" -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getLastBlockId\",\"params\":[]}" | jq -e ".result >= 2"'
"$D" sh 'systemctl stop caddy ferrata-gateway'
```

Expected: two `v0.4.0` markers, `typed keys: 2` (66 hex characters each; the command stops otherwise), `V2`, three `active` lines, leader aged notes, and height at least 2. `install.sh` creates the keys/genesis, copies `V2` settings, funds the fresh Bedrock key, and starts node/sequencer; no manual import is needed. Keep public ingress closed until programs and site are ready.

### 3.6 Programs: plan, fund, run, publish, handover

```sh
FERRATA_LEZ_TAG=v0.4.0 "$D" programs plan
FERRATA_LEZ_TAG=v0.4.0 "$D" programs fund
FERRATA_LEZ_TAG=v0.4.0 "$D" programs run
FERRATA_LEZ_TAG=v0.4.0 "$D" programs publish
FERRATA_LEZ_TAG=v0.4.0 "$D" programs handover
```

Check the plan before funding: fresh payer/daemon ids and updated v0.4.0 image ids, not rc3’s `0x816d1fef…`/`0x847a4336…`. `run` deploys the attestation and bridge programs and initialises a **new** Sepolia instance. `handover` must print `active` for updater and monitor.

Deploy all remaining programs from their updated ids (substitute exact values, replacing the bridge name if the ported kit renamed it), then publish again:

```sh
"$D" sh 'FERRATA_LEZ_TAG=v0.4.0 /opt/ferrata/src/ferrata/deploy/testnet/deploy-extra-programs.sh ferrata_holdings=/opt/ferrata/programs/ferrata_holdings.bin:0xUPDATED_HOLDINGS_IMAGE ferrata_consumer=/opt/ferrata/programs/ferrata_consumer.bin:0xUPDATED_CONSUMER_IMAGE'
FERRATA_LEZ_TAG=v0.4.0 "$D" programs publish
"$D" sh 'systemctl stop caddy ferrata-gateway'
```

Use further `NAME=/absolute/file.bin:0xIMAGE` arguments for names/messages and every other current program. The script verifies each id with r0vm and records new immutable accounts; it never migrates program state.

### 3.7 New bridge vault and configuration (no reuse)

Only after §2’s bridge gates pass, use the **ported** installer and a newly predicted vault address. `install-bridge.sh` makes fresh host roles/keystores and LEZ configuration; then follow the dry-then-live owner deployment of `F13-REDEPLOY.md` §3 with the host’s `/var/lib/ferrata/bridge/params.json` copied to local `P.json`:

The installer refuses to start without these inputs, and `deploy.sh sh` passes on no variable from the workstation, so
each one is written into the remote command itself:
`FERRATA_BRIDGE_PINS` (a sha256sum file listing the five binaries, `release.bin` and the rule (B) file once each),
`FERRATA_BRIDGE_CHECKPOINT_B` (that file's name), `FERRATA_CHECKPOINT_IMAGE_B` (its image id) and
`FERRATA_RELEASE_IMAGE_ID` (the release program's id; it may be left out only when the pinned `release.bin` is rc3's).
The installer writes the rule (B) and release ids into the bridge prover's program table
(`/etc/ferrata/bridge-prover-programs.json`, given to `ferrata-bridge-prover.service` by a drop-in). It ends by running
`ferrata-bridge-configure`, so it also needs `FERRATA_CHECKPOINT_IMAGE_A` (the v0.4.0 rule (A) id), checked with the
others before anything is installed. `ferrata-bridge-configure` writes into the vault's parameters the release id from
that table and a rule (B) id that must equal the table's, so the vault pins the programs the prover proves with. It
refuses to run without the table. install.sh records the host's tag in `/etc/ferrata/lez-tag`. A host command run without
`FERRATA_LEZ_TAG` then uses v0.4.0, and a variable that disagrees with the record is refused.

```sh
"$D" sh 'FERRATA_LEZ_TAG=v0.4.0 FERRATA_BRIDGE_PINS=/opt/ferrata/bridge-v040-pins.sha256 FERRATA_BRIDGE_CHECKPOINT_B=<the v0.4.0 rule (B) file name> FERRATA_CHECKPOINT_IMAGE_A=0x<v0.4.0 rule (A) id> FERRATA_CHECKPOINT_IMAGE_B=0x<v0.4.0 rule (B) id> FERRATA_RELEASE_IMAGE_ID=0x<the release program id> /opt/ferrata/src/ferrata/deploy/testnet/install-bridge.sh /opt/ferrata/bridge-v040-bin /opt/ferrata/programs/31-v040 0xNEW_PREDICTED_VAULT'
"$D" sh 'FERRATA_LEZ_TAG=v0.4.0 FERRATA_CHECKPOINT_IMAGE_A=0x<v0.4.0 rule (A) id> FERRATA_CHECKPOINT_IMAGE_B=0x<v0.4.0 rule (B) id> /opt/ferrata/bin/ferrata-bridge-configure --vault 0xNEW_PREDICTED_VAULT'
MODAL_PROFILE=OWNER_PROFILE modal run vault/script/deploy_sepolia.py --params P.json --freeze-authority 0xOWNER --watchers 0xWATCHER --expect-vault 0xNEW_PREDICTED_VAULT --dry
MODAL_PROFILE=OWNER_PROFILE modal run vault/script/deploy_sepolia.py --params P.json --freeze-authority 0xOWNER --watchers 0xWATCHER --expect-vault 0xNEW_PREDICTED_VAULT
"$D" sh '/opt/ferrata/bin/ferrata-bridge-start --vault 0xNEW_PREDICTED_VAULT --dry && /opt/ferrata/bin/ferrata-bridge-start --vault 0xNEW_PREDICTED_VAULT && /opt/ferrata/bin/ferrata-bridge-lez-init --send && /opt/ferrata/bin/ferrata-bridge-attestor setup && systemctl enable --now ferrata-bridge-attestor ferrata-bridge-import.timer'
```

Expected: dry and live vault deployments bind slot 2 to the new config id; `bridge-start --dry` prints `"ok": true`; pairing/attestor setup creates fresh state, and the three bridge units become active. Fund the new watcher/relayer addresses before live start as the existing installer instructs. The old vault remains on Sepolia but is not part of this zone.

### 3.8 Gas-drop relayer: new seed, guest, account, and funding

The gas drop is chain-local too. Stage `/opt/ferrata/programs/gasdrop.bin`; substitute the new bridge header exactly as `programs.json` records it (no `Public/` prefix). `init` prints a fresh public account /0 and leaves terms paused. Fund that printed account from the new operator, shield the operating float, then start:

```sh
"$D" sh '/opt/ferrata/bin/ferrata-gasdrop-setup init /opt/ferrata/programs/gasdrop.bin 200000000 NEW_BRIDGE_HEADER'
"$D" sh 'set -eu; RL="$(sed -n s/^FERRATA_RELAYER_LEZ_ACCOUNT=//p /etc/ferrata/gasdrop.env)"; runuser -u ferrata -- env XDG_CACHE_HOME=/opt/ferrata/cache RISC0_SERVER_PATH=/opt/ferrata/bin/r0vm /opt/ferrata/bin/ferrata-day1 transfer --from-key /var/lib/ferrata/keys/operator/payer.key --to "$RL" --amount 20000000000 --rpc http://127.0.0.1:3040 --state /var/lib/ferrata/keys/operator/tool --wait 300; /opt/ferrata/bin/ferrata-gasdrop-setup fund 15000000000; /opt/ferrata/bin/ferrata-gasdrop-setup start; /opt/ferrata/bin/ferrata-gasdrop-setup status'
"$D" sh 'systemctl stop caddy ferrata-gateway'
```

**Not yet runnable on v0.4.0 as written.** The relayer's account `RL` is new, and on v0.4.0 a transfer to a never-used
public account must name its typed initial key (`ferrata-day1 transfer … --to-pk <66 hex>`). `ferrata-gasdrop-setup init`
records only the account's id (`FERRATA_RELAYER_LEZ_ACCOUNT`, read from `ferrata keys`), so the second command's
transfer cannot reach it. Before §3.8, the v0.4.0 relayer CLI must print the account's initial key and the setup must
record it (for example `FERRATA_RELAYER_LEZ_INITIAL_KEY`) for the transfer's `--to-pk` (§6).

Expected: both units active, terms without `paused`, and a funded drop account. The amounts match `README.md`’s existing deployment; adjust them only as an announced policy change. Do not restore the rc3 seed, inbox, terms, or drops.

### 3.9 Site configuration and verification

`handover` already reruns `install-web.sh`; repeat it after all ids are final, then open ingress and check locally and publicly:

```sh
"$D" sh 'FERRATA_LEZ_TAG=v0.4.0 /opt/ferrata/src/ferrata/deploy/testnet/install-web.sh /opt/ferrata/web.tar.gz; systemctl start caddy ferrata-gateway; systemctl is-active caddy ferrata-gateway ferrata-updater ferrata-monitor'
FERRATA_LEZ_TAG=v0.4.0 "$D" programs verify
HOST="$(aws cloudformation describe-stacks --stack-name "$STACK" --query 'Stacks[0].Outputs[?OutputKey==`Hostname`].OutputValue' --output text)"
curl -fsS "https://$HOST/programs.json" | jq -e '.lez == "v0.4.0" and (.programs | length >= 2)'
curl -fsS "https://$HOST/status/status.json" | jq '{lez,bedrock,attestation,alarms}'
# the two reads the v0.4.0 clients depend on: the version probe before any send, and a send's receipt
curl -fsS "https://$HOST/rpc" -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getSequencerStatus","params":[]}' | jq -e '.result'
curl -fsS "https://$HOST/rpc" -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getTransactionReceipt","params":["<a test send hash>"]}' | jq -e '.result.success'
"$D" sh 'tail -n 80 /var/log/ferrata/deploy-programs.log; tail -n 50 /var/log/ferrata/updater.log; tail -n 50 /var/log/ferrata/monitor.log'
```

Expected: `programs verify` succeeds; published program/instance ids match the new deployment; public `programs.json` says `v0.4.0`; all required services are `active`; head advances and there are no critical alarms; updater/monitor follow the new Sepolia instance. Test a newly generated wallet/account and a faucet claim to a never-used v0.4.0 account, with `public_key` in v0.4.0's form (the account's typed initial key: 66 hex, `00` and its public key, as the v0.4.0 web wallet sends it; the gateway refuses rc3's 64-hex form on v0.4.0), before announcing completion. `getSequencerStatus` must identify v0.4.0 (the wallet refuses to sign when it cannot read it), and the test send's receipt must report its success and fee. Keep LEZ’s own explorer on its documented SSM port forward; the public `/explorer/` bundle contains only the new chain.

What this tree has run on LEZ v0.4.0 (local, on a build box; none of it on the test network): the standalone
sequencer with LEZ's wallet, a public and a private transfer with receipts and a program deployed by `program-loader`
([`e2e/runs/zone-v040-standalone-1/`](../../e2e/runs/zone-v040-standalone-1/README.md)); a zone of four sequencers on
a node 0.4.0-rc.2 ([`zone-v040-four-keys-1/`](../../e2e/runs/zone-v040-four-keys-1/README.md)); the committee's
parameter vote and an unstake paid 69 node blocks (2 × `finality_depth` − 1) after its key's removal
([`zone-v040-vote-2/`](../../e2e/runs/zone-v040-vote-2/README.md)). The test network's own `local.json` genesis gives
`finality_depth` 35 too: an operator's full unstake on the redeployed network waits that long. Also: `sequencer-attest`
rehearsed on a box ([`e2e/runs/ci-sequencer-attest-v040/`](../../e2e/runs/ci-sequencer-attest-v040/README.md): the
attestation program deployed, Init, period and finality updates, the scenarios and the consumption as designed; the
private attestation not sent: its credit receipt, proven for real, was dropped by the standalone sequencer; an
earlier attempt was refused for an empty fee note). The bridge's deposit-to-burn half
([`e2e/runs/zone-bridge-v040/`](../../e2e/runs/zone-bridge-v040/README.md): both claims and both burns applied; no
attestation, so no release). Beside this branch, at the integration: the clients' v0.4.0 runs (`cli/results/v040-*`), the
harness on v0.4.0 (`docs/port-v0.4.0/harness-a7a45cb6/`), and the wallet's and explorer's replays. None of it is a
deployment rehearsal.

## 4. What changes for users

- Generate wallets and accounts again with the v0.4.0 web wallet and v0.4.0 `ferrata`/LEZ wallet builds. Set a wallet-storage password. Do not import rc3 public keys or reuse rc3 account ids; old-key import derives a different, empty account.
- Public account ids change. Balances, private notes, name/message state, program accounts, and the Sepolia attestation instance are chain-local and start again. Request faucet funds for a new account with its typed initial key (66 hex: `00` and its public key), which the v0.4.0 web wallet sends; rc3's 64-hex form is refused on v0.4.0.
- Old transaction/explorer links are dead. The public explorer and site configuration show the new genesis onward; rc3 history exists only in the owner’s retained archive/snapshot. LEZ’s loopback explorer is available to the operator through SSM, not directly to users.
- Fees now follow v0.4.0 size/public-payer rules and receipts report payment; clients must understand them. Cross-zone messaging is unavailable. The bridge uses only the newly announced Sepolia vault; do not deposit to the rc3 vault.

## 5. Rollback to the complete rc3 network

Use the recorded `$SNAP` and `$STAMP`. First announce rollback and disable v0.4.0 units **before stopping the instance**, so they cannot mutate restored rc3 data at boot. Restore creates a replacement EBS volume (in-place snapshot restoration is not an EBS operation):

```sh
FERRATA_LEZ_TAG=v0.4.0 "$D" sh 'systemctl disable caddy ferrata-gasdrop ferrata-updater ferrata-monitor ferrata-bridge-relayer ferrata-bridge-monitor ferrata-bridge-prover ferrata-bridge-attestor ferrata-bridge-import.timer ferrata-topup.timer ferrata-status.timer ferrata-gateway ferrata-explorer ferrata-indexer ferrata-sequencer ferrata-node'
AZ="$(aws ec2 describe-instances --instance-ids "$IID" --query 'Reservations[0].Instances[0].Placement.AvailabilityZone' --output text)"
NEWVOL="$(aws ec2 create-volume --snapshot-id "$SNAP" --availability-zone "$AZ" --volume-type gp3 --encrypted --tag-specifications 'ResourceType=volume,Tags=[{Key=Project,Value=ferrata-testnet},{Key=Name,Value=ferrata-rc3-restored}]' --query VolumeId --output text)"
aws ec2 wait volume-available --volume-ids "$NEWVOL"
aws ec2 stop-instances --instance-ids "$IID" && aws ec2 wait instance-stopped --instance-ids "$IID"
aws ec2 detach-volume --volume-id "$VOL" && aws ec2 wait volume-available --volume-ids "$VOL"
aws ec2 attach-volume --instance-id "$IID" --volume-id "$NEWVOL" --device /dev/sdf && aws ec2 wait volume-in-use --volume-ids "$NEWVOL"
aws ec2 start-instances --instance-ids "$IID" && aws ec2 wait instance-running --instance-ids "$IID"
```

Wait for SSM, restore root-volume artifacts from the archived copy, install rc3 unit files, and start the old services. Restoring `/etc/ferrata` from the archive also drops v0.4.0's host tag record (`/etc/ferrata/lez-tag`), the gateway's `FERRATA_LEZ_TAG` line and the prover's program table. The prover's v0.4.0 drop-in sits outside `/etc/ferrata`, so the command removes it by name; left behind, it would point the rc3 prover at a table that no longer exists. An rc3 host has no `/opt/ferrata/lez-tag.sh` (its `provision.sh` predates it), so the archive copies it only if present:

```sh
"$D" sh "set -eu; A=/var/lib/ferrata/rc3-$STAMP; test -f \"\$A/opt/bin/sequencer_service\"; test -f /var/lib/ferrata/keys/operator/payer.key; rm -rf /opt/ferrata/bin /opt/ferrata/programs /opt/ferrata/explorer /opt/ferrata/explorer-site /opt/ferrata/src/ferrata /etc/ferrata /srv/ferrata/www; for p in bin programs explorer explorer-site; do cp -a \"\$A/opt/\$p\" /opt/ferrata/; done; cp -a \"\$A/opt/src/ferrata\" /opt/ferrata/src/; cp -a \"\$A/etc/ferrata\" /etc/; cp -a \"\$A/srv/www\" /srv/ferrata/; cp -a \"\$A/opt/provision.sh\" /opt/ferrata/; rm -f /opt/ferrata/lez-tag.sh; if [ -f \"\$A/opt/lez-tag.sh\" ]; then cp -a \"\$A/opt/lez-tag.sh\" /opt/ferrata/; fi; rm -f /etc/systemd/system/ferrata-bridge-prover.service.d/lez-v040.conf; install -m 644 /opt/ferrata/src/ferrata/deploy/testnet/host/systemd/* /etc/systemd/system/; systemctl daemon-reload; systemctl enable --now ferrata-node ferrata-sequencer ferrata-indexer ferrata-explorer ferrata-updater ferrata-monitor ferrata-topup.timer ferrata-status.timer ferrata-bridge-attestor ferrata-bridge-import.timer ferrata-bridge-relayer ferrata-bridge-monitor ferrata-bridge-prover ferrata-gasdrop ferrata-gateway caddy"
"$D" sh 'systemctl is-active ferrata-node ferrata-sequencer ferrata-gateway; curl -fsS http://127.0.0.1:3040 -H "content-type: application/json" -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getLastBlockId\",\"params\":[]}"'
```

Expected: old chain height/state resumes and rc3 accounts, faucet, programs, explorer history, bridge, and old Sepolia vault work again. v0.4.0-only accounts and links disappear; users must use rc3 clients/accounts and not cross-import keys. Keep `$VOL` and `$NEWVOL` (the attached restored volume). CloudFormation still records the old `DataVolumeId`: record this drift and reconcile the stack deliberately before any stack update or later snapshot procedure.

## 6. What this tree does not yet provide for v0.4.0

The network **must not be moved** until each item below is supplied and its §2 gate has passed:

- `lez/scripts/lez-select.sh v0.4.0` exists (the selector's fourth tag, `lez/lockfiles/v0.4.0.patch`, feature `lez-040`), so `provision.sh fetch` selects v0.4.0; whether every crate this host builds (`ferrata-day1`, `ferrata-updater`, `attest-monitor`) builds and works on it is the next gates' question.
- The 15 v0.4.0 guest records exist (`lez/results/v0.4.0/image-ids.md`; `docs/image-ids.md`'s "LEZ v0.4.0" section from lez-040-verify), reproducible with `scripts/verify-lez040-ids.sh` (lez-040-verify). Still rc3's: `scripts/reproduce-image-ids.sh` (pinned to rc1), `deploy.sh bundle` (it requires `ferrata_bridge_b1.bin` and writes rc3's `SHA256SUMS`), the day1/publish pins (`deploy.sh`, `lez/testnet04/day1.sh`), the extra sites and the IDL. Staged v0.4.0 binaries and their publisher changes are missing.
- `ferrata-day1` and `ferrata-updater` have v0.4.0 code: built with v0.4.0 selected, the updater passed its recorded-data tests and demo (`docs/port-v0.4.0/harness-a7a45cb6/`, at the integration), and the attestation program took its Init, period and finality updates on v0.4.0 ([`e2e/runs/ci-sequencer-attest-v040/`](../../e2e/runs/ci-sequencer-attest-v040/README.md)). The live driver's private attestation is not shown: in the latest rehearsal its credit receipt was dropped by the standalone sequencer (`lez-040-core` `ab7d51c4` waits for inclusion from the submission; not re-run). `attest-monitor` has a v0.4.0 build path (`cli/v040/build.sh`) but no recorded test. The host binaries and their hashes, and an integrated updater/monitor rehearsal, remain gates.
- The gateway forwards `getSequencerStatus` and `getTransactionReceipt` on v0.4.0, no longer forwards cross-zone dead letters, and takes v0.4.0's typed initial key at the faucet (`deploy/testnet/tests/test_gateway_tag.py`). The published-account sweep and the gas drop need a live rehearsal. The explorer's recursion fallback is rc3's only, and its artifacts are not pinned if the download is refused.
- The gas drop's funding (§3.8): `ferrata-gasdrop-setup` records the relayer's new account by its id only. On v0.4.0 a
  transfer to a never-used account needs its typed initial key (`--to-pk`), which the relayer CLI (`ferrata keys`) must
  print and the setup must record.
- Clients: use `cli/v040/build.sh` (lez-040-clients), not `cli-rc3/build.sh`. Its release target leaves out `seqnet-fund`. The gas drop and the private fee path are unrehearsed, and the tested artifacts and site metadata must still be staged. `ferrata-status` and `ferrata-bridge-identity` read only fields that v0.4.0's blocks keep (tested on recorded v0.4.0 blocks: `deploy/testnet/tests/test_host_blocks.py`). Their installed binaries and live RPC behaviour remain part of the rehearsal.
- Bridge: the installer and configuration take the operator's v0.4.0 pins and rule (A)/(B)/release ids. The vault pins what the prover proves with, and checkpoint-core has a v0.4.0 parser. Still to do: build and pin v0.4.0 rule (A)/(B) guests, port the attestor to that parser, and complete a real-proof bridge/vault rehearsal through checkpoint and release. No accepted rule (B) result and no v0.4.0 rule (A)/(B) image ids exist yet; this remains a hard gate.
