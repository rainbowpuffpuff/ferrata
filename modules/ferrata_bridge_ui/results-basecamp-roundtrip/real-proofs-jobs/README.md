# Real proofs inside Basecamp: the whole private round trip, the claim and the burn as jobs (17 November 2026)

**With real proofs, the user's whole private round trip ran in the mini-app inside Logos Basecamp, and no call of the
view was cut.** `lez_core`, loaded in Basecamp, proved the private claim (247.1 s) and the private burn (222.9 s) for
real; the LEZ sequencer, verifying every receipt (no `RISC0_DEV_MODE`), applied them in LEZ blocks 74 and 97; after the
checkpoint a relayer released 0.000997 ETH to the burn's fresh address. Basecamp still ends a view's call to a core
module at 20 s (its pin `b642a9a`; [`../README.md`](../README.md), "L4"; [`../l4-real-proofs/`](../l4-real-proofs/README.md)
measured that cut). `ferrata_bridge_module` works around it: its `claim` and `burn` start a job in a thread of the
module and answer at once, and the view polls the job. **13 of 13 steps, 64 checks; 191 calls of the view, 162 of
them polls of a job, the longest 3,176 ms, none cut.** Run `e2e/runs/seqnet-basecamp-jobs-real/`
([SUMMARY](../../../../e2e/runs/seqnet-basecamp-jobs-real/SUMMARY.md)), 17 Nov 2026, 14:50–15:08 UTC, on the Modal build box.

## The jobs ([`jobs.json`](jobs.json): the module's own record, read at the end through `wallet jobs`)

| Job | Started by | Phases (ms after the start) | Proof | Applied | Reply (the job's `result`) |
|---|---|---|---|---|---|
| 1, the private claim | the view's `claim` call, answered in 7 ms | queued 0, proving 56, sent 247,167, settled 256,007 | 247,111 ms by LEZ's wallet inside `lez_core`; receipt 239,532 B | LEZ block 74, transaction 256,068 B, private, not reverted | exactly the synchronous reply's fields (`settled`, `tx_hash`, `block`, `proof`, `balance` 0.001 ETH, …) |
| 2, the private burn | the view's `burn` call, answered in 3 ms | queued 0, proving 36, sent 222,986, settled 225,048 | 222,950 ms; receipt 239,046 B | LEZ block 97, transaction 255,031 B, private, not reverted | the same, and `release_delay_ms` 21,600,000 (6 h), the delay the view started the burn with |

While each job was proving, the driver checked (`events.jsonl`): the view's button was disabled and read "Proof being
made (job N)"; a second `claim` (`burn`) sent through the view's `logos`, as a view that lost the reply would, was
answered in 6 ms (9 ms) with the running job and `"started": false` (no second transaction); a `scan`, which would
wait for `lez_core`, was refused in 7 ms (6 ms) with "busy: job N …". The view's polls were answered meanwhile: the
module's Qt thread, waiting in the SDK for `lez_core`'s reply on the job's behalf, answers other calls in that wait.

## The steps (`events.jsonl`; the chains read directly after each: `chain-checks.jsonl`)

| Step | ms | What was checked |
|---|---:|---|
| Basecamp opens the mini-app | 82 | inside Logos, the view's transport is the core module; Basecamp's bridge has `callModuleAsync` |
| Basecamp's Module Inspector loads the core modules | 3,274 | `ferrata_bridge_module` and `lez_core` loaded by a click (picture 02) |
| the session (the host's step) | 200 | `wallet open` (the chain service) and `wallet lez` (the user's LEZ wallet inside `lez_core`), through the view's `logos` |
| connect | 2,046 | the account is anvil's depositor; the Ethereum balance shown equals anvil's |
| registry before the operator's first import | 1,167 | ETH registered on the vault, no LEZ side yet |
| deposit, private by default | 2,812 | one `Deposit` log more, naming the user; the vault holds 1,002,000,000,000,000 wei more (0.001 ETH and its fees) |
| positions before the import | 1,144 | the deposit recovered from the seed, waiting for the root import |
| the operator's import and the gas drop | 275,972 | LEZ transactions applied, none reverted (the gas drop's stand-in, `ops.sh fund`, includes the user's private transfer public /0 → private /0, proven for real by LEZ's wallet outside Basecamp in 206 s); the token's LEZ side shown |
| the private claim, once its random delay has passed | 262,748 | job 1 above; the claim's transaction in LEZ block 74, the block the view names is the sequencer's; the deposit shows as claimed |
| the private burn to a fresh address | 231,278 | job 2 above; the private holding spent; the note for the fresh recipient; the release held from the burn's settlement for the job's 6 h |
| the checkpoint and the release by a relayer | 23,896 | a checkpoint finalized on the vault; the release held by its delay until the view's clock passed it; one `Release` log to the fresh address; it holds 997,000,000,000,000 wei (0.001 ETH less the relay fee, 0.000002 ETH, and the checkpoint fee, 0.000001 ETH) |
| positions after the round trip | 2,293 | 0 not claimed, 0 not released |
| records | 31 | every call of the view answered within Basecamp's 20,000 ms, none cut by the host |

The view's calls ([`view-calls.json`](view-calls.json)): `job` 162 (0–13 ms), `scan` 14 (35–103 ms), `info` 5, `jobs`
2 (6 ms, and 3,176 ms for the one the view makes when it opens, answered once Basecamp had loaded the core module),
`registry` 2, `price` 2, `deposit` 67 ms, `claim` 7 ms, `burn` 3 ms, `release` 92 ms; no error.

## Pictures ([`pictures/`](pictures/), Basecamp's window, 1440×1250, 31–67 kB each)

A job in progress: [`14b-claim-job-proving.png`](pictures/14b-claim-job-proving.png) and
[`17b-burn-job-proving.png`](pictures/17b-burn-job-proving.png) ("Job 1: the proof of the private claim is being made by
the LEZ wallet, 6 s so far", the button "Proof being made (job 1)", the header's line). Finished:
[`15-claim-settled-privately.png`](pictures/15-claim-settled-privately.png) ("Job 1: the private claim settled after
256 s (the proof 247 s)"; "real proof, 239532-byte receipt in 247.1 s") and
[`18-burn-settled-privately.png`](pictures/18-burn-settled-privately.png). Every other step: 01–13, 14, 16, 17, 19–23,
as in [`../pictures/`](../pictures/). All data in them is the run's own (no stub).

## What is real here, and what is not

- **Real**: the private claim and burn executed and proven by LEZ's wallet inside `lez_core`, in Basecamp's module
  process, on the box's CPU, without dev mode; the sequencer (LEZ v0.4.0-rc1, its Logos Blockchain node) verifying
  every receipt; the final bridge and attestation programs of [`docs/image-ids.md`](../../../../docs/image-ids.md)
  (unchanged); Basecamp `b642a9a` hosting the view and both core modules; every user step a click in Basecamp's window.
- **Not real** (as in the dev-mode run, [`../README.md`](../README.md)): Ethereum is a local anvil (chain id 11155111,
  nothing public), whose blocks a stand-in committee finalizes for LEZ's light client; the vault's verifier is the vault
  tests' mock, so the checkpoint and the release carry no Groth16 proof; the module's Ethereum reads, the deposit's
  signing and sending and the release go through the stand-in `seqnet-chain`, not `eth_rpc_module`, `keystore_module`
  and `tx_sender_module`; `lez_core` is patched (row 113); the operator's import, the gas drop and the checkpoint are
  `ops.sh` stand-ins; the session is opened by the test; the core modules are loaded in the Module Inspector; the
  user's waiting is the view's clock moved on (`clockOffset`), the vault's release delay anvil's.
- **Not shown here**: the view closed and opened again while a proof is made (R2, [`../README.md`](../README.md));
  a module restarted during a job (the jobs, at most 64, live in the module's memory: they are lost, and the view then
  says "Unknown job"); real proofs on the chain service's own path (`BASECAMP_LEZ=plain`: there the chain service proves,
  and a call of the view to it would wait); the claim's own random delay after a reopened view (the view keeps it in
  memory, from the deposit).

## Which code

The module and the view at commit `48a3b13d` (branch `basecamp-jobs`; [`modules/README.md`](../../../README.md),
`wallet`; `modules/ferrata_bridge_module/rust-lib/calls/src/jobs.rs`); the driver
`modules/ferrata_bridge_ui/basecamp/roundtrip.mjs` of that commit. `ferrata_bridge_module`'s `.lgx` sha256
`f19d92e4e60a1a8d9b8148f76a5555526d06efbde397b587f1c1fa979e99853d` (built by `modules/scripts/check.sh` on the box:
native tests 18 of 18, 7 of them the job API's, 0 step and 0 check failures); `lez_core` `cc73ebf` with
`modules/patches/lez_core-cc73ebf-authorizes-public-pda.patch`, `.lgx` sha256 `df272fd3…66ce41` (as on 16 November);
Basecamp `b642a9aba732b80343a3a7b622dad8c65e534bdb` (`#dev`), its lgpm; logosctl `66d098d` for the module's checks.
`basecamp-versions.txt` has the install root's and the modules root's listing.

## Commands, in full (on the build box, from the worktree root; `T` = `../tmp/lez-core-v04-seq`, with e2e/seqnet's inputs)

```
# 1. ferrata_bridge_module: its native tests and calls, then its dev .lgx (as `e2e/seqnet/run.sh build-lezcore`'s check step)
env MODULES=ferrata_bridge_module PORTABLE=0 E2E=0 LOGOS_TMPDIR=<short tmp> RISC0_DEV_MODE=1 bash modules/scripts/check.sh $T/modcheck
cp -rL $T/modcheck/lgx-ferrata_bridge_module $T/logos/lgx-ferrata_bridge_module
# 2. Basecamp b642a9a (#dev; from the Nix store when built before), the view's install root, the modules root
bash modules/ferrata_bridge_ui/scripts/basecamp-build.sh <basecamp-build> $T/logos/lgx-ferrata_bridge_module $T/logos/lgx-lez_core-patched
# 3. the round trip inside Basecamp with real proofs (dev mode: SEQNET_PROOFS=dev; the view reopened: BASECAMP_REOPEN=1)
SEQNET_TMP=$T BASECAMP_BUILD=<basecamp-build> SEQNET_PROOFS=real BASECAMP_REOPEN=0 bash e2e/seqnet/run.sh basecamp basecamp-jobs-real
#    which runs hook-basecamp.sh: logos-basecamp-dev --module <ui-install> --modules-dir <modules-root> --width 1440 --height 1250 \
#      --test modules/ferrata_bridge_ui/basecamp/roundtrip.mjs   (BASECAMP_WAIT_MS=1800000 with real proofs)
python3 -I e2e/seqnet/summary.py e2e/runs/seqnet-basecamp-jobs-real modules/ferrata_bridge_ui/results-basecamp-roundtrip/real-proofs-jobs
python3 modules/ferrata_bridge_ui/scripts/shrink-screenshots.py modules/ferrata_bridge_ui/results-basecamp-roundtrip/real-proofs-jobs/pictures
```

## Environment

The Modal build box (`spike/modal-build/mrun`): an Ubuntu 24.04 container, 2 cores requested and up to 32 allowed, Nix 2.28.3;
Basecamp in its standalone mode on the offscreen platform (`QT_QPA_PLATFORM=offscreen`), driven by its QML inspector
(logos-qt-mcp, `--test`); `lez_core` proving on the box's CPU (`RISC0_DEV_MODE` unset in Basecamp's environment, the
zone's `r0vm`); a LEZ v0.4.0-rc1 devnet (sequencer and Logos Blockchain node) and anvil, both local.

## Files

`events.jsonl` (steps, clicks, checks, the jobs' progress), `chain-checks.jsonl` (the chains' own state after each
step), `chain-calls.jsonl` (the chain service's calls), `view-calls.json` (the view's call log), `jobs.json` (the
module's jobs), `lez-report.json` (every call to `lez_core`), `session.json`, `summary.json`, `basecamp-run.log`,
`basecamp-versions.txt`, `pictures/`. The chains' side: `e2e/runs/seqnet-basecamp-jobs-real/`.
