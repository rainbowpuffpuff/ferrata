# Day one on LEZ testnet 0.4

**Prototype, unaudited.** Both RFPs ask for deployment on LEZ testnet 0.4 (RFP-021 S.1, RFP-022 S.1). It does not exist
yet: logos-docs' "Testnet endpoints" page has a "0.4 (actor model)" row of TBDs, and LEZ's newest tag is v0.4.0-rc3
([`watch.sh`](watch.sh) checks both). This directory is the deployment that runs the hour it opens: one command, built
and rehearsed on our own LEZ v0.4.0-rc3 zone ([`results/rehearsal-1/`](results/rehearsal-1/README.md)). Nothing here
has been sent to any public network.

## The one command

On a machine with the build (below), once the owner has funded the two accounts `plan` names:

```sh
FERRATA_TESTNET_SEND=yes lez/testnet04/day1.sh go URL     # discover, plan, run, verify
```

`URL` is the LEZ RPC of the 0.4 row. Before it, read-only and safe to run any time against any endpoint:

```sh
lez/testnet04/day1.sh discover URL   # what the endpoint is: v0.4 or not, head, fees, channel, programs, the clock
lez/testnet04/day1.sh plan URL       # every transaction, signed for this deployment's fresh keys, priced; the funds needed
```

and afterwards `day1.sh verify URL` (exit 0 only if every verdict re-read from the endpoint matches), `day1.sh status
URL`, `day1.sh daemon-status`, `day1.sh daemon-stop`.

## What it deploys and does

| stage | what | on chain |
|---|---|---|
| (a) | the attestation program, **image `0x816d1fef…`**: the file `scripts/reproduce-image-ids.sh build` makes (sha256 `ddfde69b…` and the image id checked against [`docs/image-ids.md`](../../docs/image-ids.md) before anything is signed), through LEZ's `program_loader`, **immutable** | 7 `WriteSegment` + `CreateHeader`, at fresh accounts |
| (b) | `Init` of a Sepolia instance from a fresh finalized checkpoint: Lodestar's bootstrap, its block root checked against publicnode's header at the same slot (`lez-v03/scripts/fetch.py`, generic User-Agent); the program's own `Init` checks run natively first; refused if the weak-subjectivity period ends within a day | 1 transaction |
| (c) | the updater daemon (`lez/updater`, `ferrata-updater` with its sequencer backend, the public job path) following Sepolia from Lodestar's light-client data with publicnode confirming each finalized header (`min_agreeing = 2`), detached, with its own key | the committee's `Adopt` parts, then `Open`, `Miller`, `Finish` per update |
| (d) | one attestation of the live Sepolia vault `0x04eE…e44c`'s slots 0 and 1 (deposit root, leaf count) under the instance's newest record: `eth_getProof` at that record's block, the verdict computed first with the program's own function | 1 `Attest` (its event) |
| (e) | with `FERRATA_DAY1_BRIDGE=1`: the bridge program (**image `0xccde6171…`**), immutable; the owner-side Sepolia step for its vault written down, **not sent** | 6 `WriteSegment` + `CreateHeader` |

Every transaction is signed once and kept (`STATE/tx/<stage>/NN.tx`, with its hash, signers, nonces and limits);
`run` sends the stored bytes and waits for each to be in a block final on Bedrock. After a crash, a restart of the
endpoint or a rewind, `run` is simply run again: it reads the chain, skips what is applied and sends the same bytes
again; a stored transaction is never rebuilt with other nonces, so nothing can apply twice. Stages are tried again
(40 times, 15 s apart) while the endpoint does not answer.

## What it needs

- **Funds**, by the owner, before `run` (it refuses an empty payer). `plan` prints the payer's and the daemon key's
  account ids and amounts. On a v0.4 network native tokens arrive only through the zone's bridge: at genesis, or by a
  Channel Deposit from Logos Blockchain with the account id as metadata, credited about an hour later
  ([`NOTES.md`](NOTES.md) §2). Hold Logos Blockchain testnet notes before testnet 0.4 opens.
- **The build**, against the LEZ tag the network runs (`discover` says whether its programs and RPC are rc3's):
  `lez/scripts/lez-select.sh v0.4.0-rc3` in a copy of the tree, then in `lez/updater/sequencer`
  `cargo build --release --locked --bin ferrata-day1 --bin ferrata-updater`; LEZ's tree beside the repository
  (`upstream/lez-<tag>`). The two programs: `scripts/reproduce-image-ids.sh build` (or `cargo build -p methods` in
  `lez/`), from the tree as committed (LEZ v0.4.0-rc1: the deployed images are those builds). Put the binaries where
  `FERRATA_DAY1_BIN` and `FERRATA_DAY1_PROGRAMS` say (default beside the repository, `tmp/testnet04/`).
- `curl`, `python3`; `r0vm` if present (the image ids are then recomputed too).
- Keys: `plan` makes fresh random keys (payer, daemon, every program account) beside the repository, mode 600, never in
  it; `run` refuses to make new ones silently.

## What it does not do

- It does not fund anything, and does not send anything without `FERRATA_TESTNET_SEND=yes` (any URL; the tool also
  refuses to send to a sequencer not on its machine unless `FERRATA_SEQUENCER_REMOTE` names it, which the script
  sets to the URL it was given).
- It does not rebuild or change any program: the images are those of `docs/image-ids.md`. If testnet 0.4 runs a later
  LEZ than rc3 (LEZ's `dev` has a RISC Zero 3.1.0 bump, upgraded system programs and receipts in blocks since rc3:
  [`NOTES.md`](NOTES.md) §6), the tool and the daemon must be rebuilt against that tag first; `verify` then says
  whether the loader recorded our image ids.
- The bridge's configuration needs the vault, which the owner deploys on Sepolia in a separate step: stage (e) stops
  at writing that step down. No private transaction (the private placement, a private claim) is part of day one.
- It keeps the daemon running only as long as its machine runs: on a Modal box, a keepalive must ping it (the
  rehearsal's: `spike/tmp/testnet04/keepalive.sh`, beside the repository).

## Files

[`day1.sh`](day1.sh) (the command), [`NOTES.md`](NOTES.md) (the v0.4 deployment path from LEZ's source, with file and
line), [`watch.sh`](watch.sh) (the watcher), [`rehearse.sh`](rehearse.sh) and [`forward.py`](forward.py) (the
rehearsal), [`reference-rc3.json`](reference-rc3.json) (what rc3's RPC serves, for `discover`'s comparison),
`../updater/sequencer/src/day1.rs` (`ferrata-day1`: keys, signing once, sending stored bytes, inspecting, verifying),
[`results/`](results/).
