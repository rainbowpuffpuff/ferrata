# deploy/testnet: Ferrata's public LEZ v0.4.0-rc3 test network on AWS

**Prototype, unaudited; a test network** (tokens have no value; it may be reset). One EC2 host in `us-east-1` runs a
whole LEZ zone **anchored to its own Logos Blockchain (Bedrock) node**, Ferrata's programs on it, a live Sepolia
attestation instance, LEZ's own indexer and explorer, a faucet and an HTTPS gateway, and the Sepolia bridge demo's
services. The network is built from source on the host from this tree and LEZ v0.4.0-rc3 (`provision.sh`); a clean AWS
account needs, besides these files, the artifacts listed under "Artifacts" (program binaries, the bridge demo's
binaries, the site's bundle), each named with its source and checked by sha256. The live addresses, what works and the
costs: the network's own landing page and `/AGENTS.md`.

## What runs

| Unit | What | Listens |
|---|---|---|
| `ferrata-node` | Logos Blockchain node 0.3.5-rc.1 (the release LEZ rc3 pins, by LEZ's own resolver), LEZ's local `node-config.yaml` and `deployment-settings.yaml`: one validator, leader proofs in dev mode (`POL_PROOF_DEV_MODE=true`, as LEZ's container) | 127.0.0.1:18080 (API), p2p on loopback |
| `ferrata-sequencer` | LEZ `sequencer_service` v0.4.0-rc3 (`bafd07be`, unmodified), LEZ's `local.json` with 10 s blocks, this network's genesis (two accounts whose keys were made on the host), `priority_fee_percent` 900; started in phase with the node's epochs by `host/bin/ferrata-sequencer-run` | 127.0.0.1:3040 (JSON-RPC), 127.0.0.1:9000 (metrics) |
| `ferrata-indexer` | LEZ `indexer_service` (feature `prove`, as LEZ's Dockerfile): the zone rebuilt from the node's channel alone | :8779, loopback traffic only (its CLI takes no address) |
| `ferrata-explorer` | LEZ `explorer_service` (Leptos, `cargo leptos build --release`) over the indexer; not published (operator: SSM port forward) | 127.0.0.1:8081 |
| `ferrata-gateway` | `host/bin/ferrata-gateway`, user `ferrata-gw` (owns only the faucet's key and ledger): JSON-RPC method allowlist, requests priced by work per IP, response-byte budgets, a network-wide `sendTransaction` cap, batch/range/list caps, CORS; the faucet (`ferrata-day1 transfer`) | 127.0.0.1:8090 |
| `ferrata-updater` | `ferrata-updater` (lez/updater, sequencer backend) with the configuration `lez/testnet04/day1.sh` wrote for its Sepolia instance | 127.0.0.1:9464 (`/health`) |
| `ferrata-monitor` | `attest-monitor watch` against Lodestar's and publicnode's Sepolia beacon APIs | - |
| `ferrata-topup.timer` | `host/bin/ferrata-topup`: claims the leader's rewards (`POST /leader/claim`) into the sequencer's Bedrock funding key when it runs low; never spends the SDP key (that can halt the chain for good: Operating notes) | - |
| `ferrata-status.timer` | `host/bin/ferrata-status`: `status.json`, `history.json`, CloudWatch metrics (`Ferrata/Testnet`) | - |
| `ferrata-gasdrop` | `ferrata-relayer --gas-drops-only` (cli/, built against LEZ v0.4.0-rc3 by `cli-rc3/build.sh`), user `ferrata-gasdrop`: the network's gas-drop relayer (RFP-022 U.5, Priv.7). The gateway publishes its terms at `/relayer/gas-drop` (its LEZ account, the guest's image id, the native LEZ per drop, for a claim and one burn), serves the guest at `/relayer/gas-drop/guest` and writes posted jobs into its inbox after its checks and caps; it verifies each receipt and sends one sealed credit per deposit from its private account /0, proven on this host. No Ethereum key. `ferrata claim` and `ferrata burn` use it with no setting; the web wallet's bridge tab names it. Set up by `host/bin/ferrata-gasdrop-setup` (below) | - (the gateway on 8090 fronts it) |
| `caddy` | TLS (Let's Encrypt; the domain and `a-b-c-d.sslip.io`), the site, the `rpc.` name; admin API off, timeouts, 1.1 MB request bodies, `script-src 'self'` on every page | 80, 443 |

Two problems of LEZ rc3 zones met in this repository's earlier runs are handled here, not in LEZ: an inscription funded
with LEZ's default 12 % fee reserve just before the node's storage price steps up at an epoch is never mined and the
zone stalls for about 20 minutes before rewinding (LEZ issue 1109; `lez/testnet04/results/rehearsal-1/`) - the reserve
is raised to 900 % and the sequencer started in phase with the epochs, as `lez/lab/` does; and the funding key of LEZ's
local node runs dry after about 1,468 blocks - `ferrata-bedrock-split` funds it once on a fresh chain (2×10^11 from the
SDP key's genesis note, whose change stays the leader's stake) and the top-up timer refills it with the leader's
claimed rewards, never from the SDP key.

## AWS resources (`stack.yaml`, all tagged `Project=ferrata-testnet`)

EC2 instance (Ubuntu 24.04, IMDSv2 only, encrypted gp3 root), encrypted gp3 data volume on `/var/lib/ferrata`
(`DeletionPolicy: Snapshot`) with daily Data Lifecycle Manager snapshots (7 kept), Elastic IP, security group with
**80 and 443 only** (no port 22: SSH is disabled; administration through SSM Session Manager and Run Command), an
instance role with `AmazonSSMManagedInstanceCore`, an explicit deny on every SSM parameter, writes to its own log
groups and metric namespaces only and read-only access to `bundle/` and `hotfix/` of its artifact bucket, a private,
encrypted, TLS-only S3 bucket for the source bundle, three log groups (30-day retention), EC2 auto-recover and
auto-reboot alarms on the status checks, alarms on the network's own metrics (head stale, Bedrock halted, Bedrock
finality lag, attestation lag (also when its monitor reports nothing), data and root disk, the status page's critical
and warning alarms) and on egress (over 5 GB in an hour, 10 GB in a day), all notifying the SNS topic
`ferrata-testnet-alerts` (output `AlertTopicArn`).

## Deploy from a clean account

```sh
deploy/testnet/deploy.sh stack c7i.8xlarge        # the stack, a big instance for the build
PROGRAMS_DIR=... [WEB_TGZ=...] [RECURSION_ZKR=...] deploy/testnet/deploy.sh bundle
                                                  # this tree, the four program binaries (each sha256 checked), the
                                                  # site's bundle and RISC Zero's recursion_zkr.zip (Artifacts) to S3
deploy/testnet/deploy.sh provision                # base, fetch, toolchains, LEZ rc3 and Ferrata builds (about 25 min)
deploy/testnet/deploy.sh run install              # keys, genesis, configuration, units, Caddy, site: the network starts
deploy/testnet/deploy.sh programs plan            # the day-one kit's plan, fresh keys made on the host
deploy/testnet/deploy.sh programs fund            # the operator's genesis account pays the plan's payer and daemon key
deploy/testnet/deploy.sh programs run             # programs deployed, Sepolia instance, updater, attestation, bridge B1
deploy/testnet/deploy.sh programs handover        # the updater and attest-monitor under systemd (before any resize:
                                                  # `run` leaves the updater detached, and a stop/start ends it)
deploy/testnet/deploy.sh resize m7i.large         # the steady state (stop/start; address, volumes and state kept)
deploy/testnet/deploy.sh sh '/opt/ferrata/src/ferrata/deploy/testnet/deploy-extra-programs.sh NAME=FILE:IMAGE ...'
deploy/testnet/deploy.sh programs publish          # programs.json and the site's config with every program
```

The program binaries are the ones of `docs/image-ids.md` (`scripts/reproduce-image-ids.sh build` makes them; the kit
refuses any other bytes). Keys are made on the host (`ferrata-day1 keygen`, mode 600, owner `ferrata`) and never leave
it; nothing here prints one.

## Owner steps (not automated: personal data or account-wide settings)

- **Alerts:** the topic has no subscriber. Subscribe an address and confirm the e-mail it receives:
  `aws sns subscribe --region us-east-1 --topic-arn <AlertTopicArn> --protocol email --notification-endpoint <address>`.
- **Operate as a scoped role, not the account root:** an IAM role (or IAM Identity Center permission set) limited to
  this stack, its `Project=ferrata-testnet` resources and `ssm:SendCommand`/`ssm:StartSession` on the tagged instance.
- **Account-wide guards:** `aws s3control put-public-access-block --account-id <account> --public-access-block-configuration
  BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true`;
  `aws ec2 enable-snapshot-block-public-access --state block-all-sharing`;
  `aws ssm update-service-setting --setting-id /ssm/documents/console/public-sharing-permission --setting-value Disable`;
  `aws ec2 enable-ebs-encryption-by-default`; GuardDuty on;
  `aws cloudformation update-termination-protection --enable-termination-protection --stack-name ferrata-testnet`.

## The Sepolia bridge demo

`install-bridge.sh BIN PROGRAMS_31 VAULT` puts in place, without starting anything: two throwaway Sepolia keys made on
the host (`host/bin/ferrata-bridge-keys`: a watcher for `monitor`, a relayer; encrypted keystores, each user's own
mode-700 directory, only the addresses printed), the binaries of "Artifacts" (each checked against a sha256 pinned in
the script), the RISC Zero 3.1 `release` and `checkpoint_b_0_3_5` programs (sha256 checked), the Modal SDK for the
prover client (hash-locked, `host/modal-requirements.txt`), the units `ferrata-bridge-relayer`, `-monitor`, `-prover`
(own users, each sandboxed to its own directory; the relayer's inbox is inside the relayer's directory and the prover
client sees that inbox only), and the LEZ-side configuration for the vault address the deploy tool's `--dry` predicts
(`ferrata-bridge-configure`: this zone's identity at T0, two LEZ role keys, `crown-driver config-id`) with the deploy's
`params.json` (`/var/lib/ferrata/bridge/params.json`: `lezConfigId`, `genesisState`, nMin/tMin 1/1).

Then, in order: the owner funds the watcher and the relayer (about 0.05 Sepolia ETH each); deploys the vault with
`deploy_sepolia.py --params params.json --watchers <watcher> --freeze-authority <their key> --expect-vault <VAULT>`
(`--dry` first); and one command starts the services: `ferrata-bridge-start --vault <VAULT>` (it reads the chain id,
the vault's code, its slot 2 against `lezConfigId`, the deployment block and both balances, writes
`/etc/ferrata/bridge.env`, enables and starts the three units). The prover client calls `prove_bytes` of the Modal app
`ferrata-prover` deployed in a workspace that holds no secret, with that workspace's token in
`/etc/ferrata/bridge-prover.env` (root, mode 600). The LEZ side follows the vault: `ferrata-bridge-lez-init --send`
(the bridge program's `Init` pairing the configuration with the vault's slot 2, and ETH's `Register`, under a finalized
Sepolia header the zone's instance holds), and `ferrata-bridge-import.timer` imports the vault's deposit roots.

The way back (LEZ to Ethereum), rule (B):
- `ferrata-bridge-attestor setup` once, then `ferrata-bridge-attestor.service`: its own key, certified once by the
  sequencer's channel key, signs the node's final tier every 60 s.
- `ferrata-bridge-checkpoint --wait-burn --prove`: once the zone holds a burn, the checkpoint over the zone's blocks
  since its parent, from the node's answers alone (realblocks-crown, `checkpoint-chain/`), proven on the Modal GPU; the
  relayer submits it, the watcher checks its tip is final, and the relayer finalizes it after the vault's window. Its
  parent is the vault's `finalState`: the configuration's tip at first, then the state the last final checkpoint ended
  at (its `state.json`); a request waits until the previous one is final.
- A holder's release: `ferrata release --burn <i> --input-out F.input` on their own machine (the CLI of
  `cli-rc3/`; the proof's input needs their phrase), then `ferrata-bridge-release F.input` on the host: proven on the
  GPU, sent by the relayer (it pays the gas and is paid the registered relay fee).

## Artifacts: what comes from where

| Artifact | Source | Checked by |
|---|---|---|
| LEZ `sequencer_service`, `wallet`, `indexer_service`, `explorer_service`, the Bedrock node | LEZ v0.4.0-rc3 (`bafd07be`), built by `provision.sh` | the commit, in `provision.sh` |
| `ferrata-day1` (with the bridge stages), `ferrata-updater`, `attest-monitor` | this tree, built by `provision.sh` | - |
| `ferrata_lez.bin`, `ferrata_bridge_b1.bin`, `ferrata_holdings.bin`, `ferrata_consumer.bin` | `scripts/reproduce-image-ids.sh build`; `PROGRAMS_DIR` of `deploy.sh bundle` | sha256 of `docs/image-ids.md`, in `deploy.sh bundle` |
| `release.bin`, `checkpoint_b_0_3_5.bin` (RISC Zero 3.1) | `docs/image-ids.md`'s 3.1 rows | sha256 in `install-bridge.sh` and `ferrata-bridge-prover` |
| `ferrata-relayer`, `monitor`, `crown-driver` | `cli/`, `monitor/`, `lez/crown/driver/` at commit `05650037` (`cargo build --release --locked`) | sha256 in `install-bridge.sh` |
| `realblocks-crown`, `ferrata_attestor` | `checkpoint/` at commit `0c5906d5`, realblocks-crown with `checkpoint-chain/realblocks-chain.patch` (`checkpoint-chain/build.sh`) | sha256 in `install-bridge.sh` |
| `cast` | Foundry v1.3.6's release tarball | sha256 in `install-bridge.sh` |
| the holders' `ferrata` CLI on LEZ rc3 | `cli/` at `0c5906d5` with `cli-rc3/` **of that commit** (`git show 0c5906d5:deploy/testnet/cli-rc3/build.sh`: its patch still carries the `DryRun` hunk, no features); the tree's own `cli-rc3/build.sh` goes with its own `cli/` (since `847bb4e5`: `--features lez-rc3`), and from the gas drop on builds `ferrata`, `ferrata-relayer`, `ferrata-gasdrop` and `seqnet-fund` (compiled on the build box from `priv7-gasdrop` `df7ffc0e`: `results/2026-11-25-cli-rc3-gasdrop.txt`; the row takes the merged commit once installed) | its builder prints the sha256 |
| `web.tar.gz` (the site) | not built by these scripts: the build page (`build/`), the explorer bundle (`explorer/`), the names resolver (`names/`, from `names/js`), the web wallet (`wallet/`) and the messages module (`msg/`), each from this tree (`deploy/testnet/web/`, `names/js/`); `WEB_TGZ` of `deploy.sh bundle` | - |
| `recursion_zkr.zip` | RISC Zero v3.0.5's `src/recursion_zkr.zip`, for `build-explorer` when RISC Zero's URL refuses the host (it did: 403); `RECURSION_ZKR` of `deploy.sh bundle` | sha256 `744b999f…` in `deploy.sh` and `provision.sh` |
| AWS CLI, CloudWatch agent, Caddy, `cargo-leptos`, the Modal SDK | pinned versions in `provision.sh` and `host/modal-requirements.txt` | sha256 (Caddy: its repository key's fingerprint) |

**What the sha256 pins of Rust binaries mean.** The bridge demo's binaries pinned in `install-bridge.sh`, the
`ferrata-day1` named in a redeploy and the holders' CLI were built by this project (on its build host, from the commits
named above, `cargo build --release --locked`). A Rust build embeds absolute paths (the cargo registry under the
builder's home directory, the source checkout's path), so a build of the same commit elsewhere gives other bytes and
`install-bridge.sh` refuses it; the pins check that the binaries are the ones this project built and tested, not that
anyone can reproduce them bit for bit. To check a binary yourself, rebuild it under the same paths, or rebuild with
`RUSTFLAGS="--remap-path-prefix=$HOME=/home --remap-path-prefix=$PWD=/src"` on both sides and compare those builds
with each other (the pins here are not of such builds). The program binaries are checked against the sha256s and
image ids of `docs/image-ids.md` (`scripts/reproduce-image-ids.sh` is how this project built them).

## Operate

```sh
deploy/testnet/deploy.sh status                                  # outputs and status.json
deploy/testnet/deploy.sh sh 'systemctl status ferrata-sequencer' # any root command over SSM
deploy/testnet/deploy.sh sh 'tail -n 50 /var/log/ferrata/sequencer-run.log'
aws ssm start-session --target <instance id>                     # a shell, if needed
```

Logs: `/var/log/ferrata/*.log` (logrotate daily, 200 MB cap, 5 kept, as `ferrata`: `install.sh` creates each unit's log
owned by it); Caddy's access log `/var/log/caddy/access.log` (client IPs, 30 days); the sequencer's, updater's,
monitor's, top-up's, gateway's and Caddy's go to CloudWatch Logs (`/ferrata-testnet/access` for the last two, 30 days).
The gas drop's routes (`/relayer/gas-drop`, its guest) have no access log (Caddy `log_skip`) and the gateway's line for an
accepted job names neither the address nor the deposit's nullifier N; the relayer's own log, `/var/log/ferrata/gasdrop.log`
(N, the one-time account, the credit's transaction; no addresses), stays on the host and is not shipped. What remains:
the claim's own `sendTransaction` through `/rpc` is in the access log with its address, as every transaction is. Restarts are systemd's (`Restart=always`); the hardware is EC2's
(auto-recover). A reset of the network: stop the units, move `/var/lib/ferrata/{node,sequencer,indexer,day1}` aside,
`run install`, then `programs plan|fund|run` (new keys, new program accounts).

## Operating notes (learned on the live network, 23 Nov 2026)

- **Never spend the Bedrock leader's stake.** The node leads only with *aged, unspent* notes of its wallet's keys
  (`GET /leader/aged-notes`). LEZ's local configuration gives the sequencer the leader's funding key, and the only large
  stake is the SDP key's 10^12 genesis note. If every aged note is spent the leader wins no slot, the chain makes no
  block, no new note can age: the chain never moves again, and restarts do not help (it happened once: `count 0`,
  chain reset). `install.sh` therefore runs `host/bin/ferrata-bedrock-split` on a fresh chain before the sequencer's
  first start (2×10^11 to the sequencer's key; the stake's change note ages at the next-but-one epoch boundary, 405 s
  measured) and nothing spends the SDP key afterwards; `ferrata-topup` refills the sequencer's key with leader reward
  claims; `ferrata-status` raises `bedrock-halted` and `bedrock-leader-no-stake`, and the stack alarms on
  `BedrockTipAgeSeconds`.
- **Restarts and resizes are safe** while the leader has aged notes: the resize to m7i.large stopped everything for
  about 75 s; Bedrock and the zone resumed by themselves, the sequencer back in phase on its first start.
- **A fresh public account** is credited only with its public key as admission evidence: the faucet takes
  `public_key`, `ferrata-day1 transfer --to-pk`.
- **Program segments**: LEZ's loader chunks the user ELF inside the binary; `deploy-extra-programs.sh` takes the count
  from LEZ's CLI. A 98 KB segment's inscription costs the sequencer's key about 1M at the 900 % reserve, and four in a
  row stepped Bedrock's storage price 1 → 5.
- **LEZ's published test accounts:** rc3's pre-genesis state funds two public test accounts whose keys are in LEZ's
  source. `install.sh` sweeps them into the faucet (`host/bin/ferrata-sweep-published`) on every fresh chain before the
  gateway opens; `ferrata-status` raises `published-test-account-funded` if they hold funds again.
- **The node runs with LEZ's published local keys** (`bedrock/node-config.yaml`: the leader's and SDP keys, the
  blend and swarm keys), as LEZ's local stack does, and its API on 127.0.0.1:18080 spends them without
  authentication: `ferrata-node-api-guard` (nftables) refuses that port to every local user but root, `ferrata`,
  `ferrata-attestor` and `ferrata-watcher`.
- **A service that moves files between two directories needs them on one mount.** The relayer moves each finished job
  from its inbox into its state with rename(2); with the two bound as separate mounts (`BindPaths`) that fails with
  EXDEV, and the relayer stopped after its first submission without finalizing it (23 Nov 2026;
  `results/2026-11-24-relayer-exdev-replay.txt`). The inbox is now inside the relayer's directory, and the unit refuses
  to start otherwise.
- The public domain is configured on the host only (`/etc/ferrata/site.env`, `FERRATA_DOMAIN=...`), not in this
  repository; Caddy serves it and the `a-b-c-d.sslip.io` name.

## Tests

- `python3 -I deploy/testnet/tests/test_gateway.py`: the gateway against a stand-in sequencer (allowlist,
  batch/range/list caps, work-priced per-IP limits, per-client concurrency, the network-wide send cap, response-size
  cap and byte budgets reserved before forwarding, limits before the body, socket timeout, concurrency cap, CORS;
  byte budgets with answers larger than the reserve, a client's share, small answers within their own budget, an
  answer built and not sent charged to its client (the sequencer builds 2 answers for 8 requests, 1 for 5 over the
  cap) and a client that waits served; the faucet's checks, per-account/IP/network limits, failures counted and not
  echoed, the waiting-slot cap, and claims decided by their own transactions with a stand-in tool that records each
  before sending it: a lost answer, a transfer applied after the gateway looked, another source paying the same
  amount, claims interrupted before and after their record, a transfer whose nonce another spent, one sent again,
  one that does not settle in time, an older tool refused).
- `python3 -I deploy/testnet/tests/test_gateway_gasdrop.py`: the gateway's gas-drop routes (terms 404/200/503, the guest,
  a job into the inbox as 0640 named by its N, the same N again 409, malformed jobs 400, 413, the inbox cap and the
  per-IP, per-/24 and hourly caps 429), 26 checks.
- `python3 -I deploy/testnet/tests/test_units.py`: the units read as files (the relayer's inbox and state on one
  mount, every bridge unit sandboxed, every unit's log created for logrotate, the node API guard) and, where
  `unshare -rm` is allowed, the EXDEV mechanism itself.
- `python3 -I deploy/testnet/tests/test_web_config.py`: `web-config.py` (`install-web.sh`'s config step) on a stand-in web
  root: messages v2 for the messaging module (v1 as legacy), left off on a host without it; the wallet's inbox and names
  panels on the program their bundled client is written for (v1 for the wallet as this repository bundles it, also
  where v2 is deployed and a v2 config was served before; v2 for v2 clients; off where that program is missing or the
  client is not recognised); the explorer's catalogue on v1, what the host set in a served config kept (the live
  file's values first, the copy served after the last good run filling only what it lacks or leaves null, also after
  a run stopped half way), the module's rpc the site's own, files written whole (`/AGENTS.md` included), each AGENTS
  section once.
- `python3 -I deploy/testnet/tests/test_install_web.py`: `install-web.sh` itself, its shell steps run on a scratch
  copy (its paths moved into a temporary tree, curl, caddy and systemctl stubbed): deposits closed by hand
  (`lezReady` false, a later `fromBlock`) stay closed at the next install, and after a run stopped after step 3 the next
  good run brings the host's values back.
- `python3 -I deploy/testnet/tests/test_checkpoint_parent.py`: `ferrata-bridge-checkpoint`'s parent, its refusal to
  stack candidates and to build when the vault's `finalState` cannot be read, against a stand-in Sepolia endpoint;
  `--rebuild` with a stand-in `realblocks-crown` (its copy kept where no parent is chosen from).
- `deploy/testnet/checkpoint-chain/build.sh SRC`: realblocks' `tests/chained.rs` (two chained checkpoints end at the
  state of one over the same blocks).
- `deploy/testnet/tests/relayer-replay/replay.sh FERRATA_RELAYER`: the relayer's stall and its fix on an anvil fork of
  Sepolia.

## The gas-drop relayer: deploying it (as root over SSM, after review)

1. Build `ferrata`, `ferrata-relayer`, `ferrata-gasdrop` (the gateway's receipt checker) and `seqnet-fund` against LEZ
   v0.4.0-rc3 with this tree's `cli-rc3/build.sh SRC` (it builds the four; compiled on the build box,
   `results/2026-11-25-cli-rc3-gasdrop.txt`) and the gas-drop guest whose image id the tool pins (`bridge/`'s `gasdrop`,
   `0x89580416…`, docs/image-ids.md); install them in `/opt/ferrata/bin`.
2. `host/bin/ferrata-gasdrop-setup init GUEST 200000000 BRIDGE_ACCOUNT_HEX`: the user, `/var/lib/ferrata/gasdrop`, a fresh
   seed, `/etc/ferrata/gasdrop.env`, the terms (`/etc/ferrata/gasdrop-terms.json`, with the vault and chain from
   `/etc/ferrata/bridge.env`, paused) and the gateway's `GASDROP_*` (the checker `GASDROP_CHECK` among them: without it the
   gateway takes no job); it prints the relayer's LEZ public account /0.
3. Pay that account native LEZ from the operator's account (the faucet's tool: `ferrata-day1 transfer --from-key ... --to
   <account> --amount 20000000000 --rpc http://127.0.0.1:3040 ...`), then `ferrata-gasdrop-setup fund 15000000000` (public
   /0 to the drop account, private /0, proven here; the terms lose "paused").
4. Install the new gateway, its unit (the inbox's `BindPaths`) and the Caddyfile (`/relayer/gas-drop` to the gateway, no
   access log: `caddy validate --config /etc/caddy/Caddyfile` before the reload), then `ferrata-gasdrop-setup start`;
   check `curl -s https://<host>/relayer/gas-drop` (200, `accepting`), a junk POST (400, `pending` unchanged) and
   `ferrata-gasdrop-setup status`. The gateway keeps every N it admitted in `/var/lib/ferrata/faucet/gasdrop-admitted.sqlite`
   (no addresses; 7 days after the job left the inbox) and answers a repeat 409; run `check-job` once under the
   gateway's unit (`MemoryMax=1G`) after the install, e.g. through a real job.

Pause it by setting `"paused": "<reason>"` in the terms (the tool then stops a claim, with the reason, rather than pay
from the user's own account); stop with `systemctl disable --now ferrata-gasdrop`.

