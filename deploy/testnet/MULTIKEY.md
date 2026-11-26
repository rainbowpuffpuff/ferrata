# The test network's zone with more than one sequencer key (RFP-022 F.7), and the Sepolia return leg over it

**For the owner of the test network. Nothing here has been run on it.** The same mechanics ran on a local zone of
four LEZ v0.4.0-rc3 sequencers on a Logos Blockchain 0.3.5-rc.1 node, with the bridge, a 3-of-4 checkpoint and the
releases through RISC Zero's verifier 3.1.0 (`e2e/runs/zone-bridge-rc3-multi/`, `vault/test/real/ZoneBridgeRc3Multi.t.sol`,
`checkpoint/attestor/zone-bridge.sh`). That zone started with four keys in its genesis; this network has one, so the
step this runbook adds is growing the key set on a running zone (upstream's `sequencer_service stake`, the
committee's channel configuration change), which the local four-key run did not exercise on rc3 (it ran on rc1, EVIDENCE
row 101).

## What it changes, for good

- **The zone's key set.** Today the channel accredits one key (the first sequencer's) and the vault's checkpoints
  are 1 of 1. After `wait`, the channel accredits 1 + N keys and every later checkpoint needs t(1 + N) attestations
  (t(4) = 3). The vault on Sepolia (`0x1949…2787`, floors nMin 1, tMin 1) accepts such checkpoints unchanged: the
  floors are minimums, the journal names n and t, and the checkpoint program follows the key-set change inscribed in
  its span (as row 101's rotation, which the vault took over its genesis key set). Going back to one key is another
  configuration change the new committee must sign (t of the new set): not scripted here.
- **Liveness.** Each key holds the turn for 60 slots (240 s); a key whose sequencer is down costs its turn's posting
  timeout (25 slots, 100 s) every round. If a further sequencer stops for good, blocks come only in the turns of
  those running.

## What can go wrong

- **The rc3 stall at a turn boundary** (`e2e/runs/zone-rc3-four-keys/STALL.md`): two sequencers publishing the same
  block height across a turn boundary, both inscriptions removed by the node, one sequencer building on its own
  unmined lineage. Its cause is not settled (a candidate: every sequencer pays its inscriptions from the node's one
  funding key, as here and in upstream's devnet). The local four-key run's sequencers were started in phase with the
  node's epochs (`checkpoint/attestor/zone-bridge.sh`, `seq_aligned`) and the host's `ferrata-sequencer-run` does the
  same for each unit; whether that avoids the race is not shown. Watch `ferrata-status` and the explorer; if the
  channel's tip stops for more than two posting timeouts, stop the further sequencers (`systemctl stop
  'ferrata-sequencer@*'`): their turns then time out and the first sequencer's continue.
- **One operator.** Every key, sequencer and attestor is on this host and held by one operator: this shows the
  t-of-n mechanism on a public network and Sepolia, not t independent parties (`docs/trust.md` §4.2).

## Steps (as root on the host: `deploy/testnet/deploy.sh sh '<command>'`)

The host scripts are installed under `/opt/ferrata/bin` by `install-bridge.sh`; this tree adds `ferrata-zone-keys`,
the units `ferrata-sequencer@.service` and `ferrata-bridge-attestor@.service`, and makes `ferrata-bridge-attestor`
(`FERRATA_ATTESTOR_INSTANCE`) and `ferrata-bridge-checkpoint` (t(n) attestations of one tip from every attestor
directory) work with several sequencers. Put this tree on the host (`deploy/testnet/deploy.sh bundle`, then
`deploy/testnet/deploy.sh run` with no step: it runs `provision.sh fetch`, which unpacks the tree to
`/opt/ferrata/src/ferrata`), then install only these five files, without rerunning `install.sh`:

```sh
S=/opt/ferrata/src/ferrata/deploy/testnet/host
install -m 755 "$S"/bin/ferrata-zone-keys "$S"/bin/ferrata-bridge-attestor "$S"/bin/ferrata-bridge-checkpoint /opt/ferrata/bin/
install -m 644 "$S"/systemd/ferrata-sequencer@.service "$S"/systemd/ferrata-bridge-attestor@.service /etc/systemd/system/
systemctl daemon-reload
ferrata-zone-keys plan 3                       # read only: the channel now, the configurations, the ports
ferrata-zone-keys configs 3                    # /etc/ferrata/zone-keys/sequencer_config-{0..3}.json
ferrata-zone-keys setup 3                      # three homes, three channel keys (printed; the keys stay on the host)
ferrata-zone-keys stake --wallet-home <a LEZ wallet home with a funded public account> --from Public/<account>   # dry
ferrata-zone-keys stake --wallet-home <same> --from Public/<account> --send                                      # sends 3 stakes on LEZ
ferrata-zone-keys wait 3                       # until the channel accredits 4 keys (the first sequencer signs the new set)
ferrata-zone-keys start 3                      # the first sequencer restarted on a fixed gossip port; sequencers 1..3
                                               # and their attestors started
```

The stake amount and the account's funds are LEZ's (`sequencer_service stake --help` on the host). Then a burn by a
holder (the web wallet or the CLI), and the return leg as today: `ferrata-bridge-checkpoint --wait-burn --prove` now
takes three of the four attestations of one tip; the prover client proves it on the GPU; the relayer submits it and
finalizes it after the vault's window (1 h); `ferrata-bridge-release` pays the holder's release.

## What to record afterwards (for the integration piece)

`ferrata-zone-keys plan 3` before and the channel after `wait`; the checkpoint directory
(`/var/lib/ferrata/bridge/checkpoint/zone-<parent>-<tip>/`: its `realblocks-crown.txt` names n, t and the
attestations kept, and the key-set change in its span); the candidate's and the finalisation's Sepolia transactions and
the release's; then `python3 -I scripts/verify-sepolia.py` on two endpoints as for row 250.
