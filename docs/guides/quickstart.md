# Quickstart: every command

The README's quickstart in full, moved here unchanged on 16 November 2026 (only the links are rewritten for this
folder). Commands run from the repository root.

**In one command**, in a container with every toolchain pinned: the checks below and the rebuild of all 14 guest
programs against [`docs/image-ids.md`](../image-ids.md), ending in one line per check and a verdict (a 10 GB image;
half an hour on 16 CPUs). On a Modal account (`pip install modal`, then `modal setup`), the route we have run:

```sh
modal run --detach scripts/reproduce/modal_app.py
```

With Docker on x86-64, `scripts/reproduce/run.sh` builds the same Dockerfile and runs the same `check.sh` in it: run
once under Docker on a fresh x86-64 Linux machine, 5 of 5 in 22 minutes ([`scripts/reproduce/runs/2026-11-12-docker-aws/`](../../scripts/reproduce/runs/2026-11-12-docker-aws/README.md)). What it pins and what
it leaves out: [`scripts/reproduce/README.md`](../../scripts/reproduce/README.md).

Commands run from the repository root. The first two need only Rust (with curl and tar for the vectors)
and Python 3.

The light-client core on the official consensus vectors and real mainnet and Sepolia data (native tests,
a laptop is enough):

```sh
scripts/fetch-vectors.sh && cargo test --workspace --locked
```

(163 tests in 25 binaries; under two minutes on a 16-core machine with an empty cargo registry.)

The live Sepolia round trip on the final programs, checked from its committed records, read-only (Python's
standard library only; it asks a public endpoint unless you give your own with `--rpc URL` or `SEPOLIA_RPC_URL`;
`e2e/runs/crown-live-1` is the first live run, on earlier programs):

```sh
python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-3
```

The rest needs RISC Zero's toolchain (`rzup`, r0vm 3.0.5), Foundry (forge 1.3.6, solc 0.8.26) and, for
the Groth16 wraps, Docker on x86-64. The vault's suite (install the pinned dependencies first:
[`vault/README.md`](../../vault/README.md)):

```sh
cd vault && FOUNDRY_OFFLINE=false forge build && forge test
```

`vault/foundry.toml` keeps forge offline, so the first build must be allowed to fetch solc 0.8.26
(`FOUNDRY_OFFLINE=false`; without it forge stops at "can't install missing solc 0.8.26 in offline mode"); later
runs need no network. Expected: 464 passed and 1 skipped, the privacy suite's Ethereum half, which replays the
output of its LEZ half (`scripts/privacy/run.sh` runs both).

The attestation and bridge programs on LEZ v0.4.0-rc1's own state machine, in process, with RISC Zero in
dev mode. LEZ's guests are built with RISC Zero's guest Rust 1.91.1, kept in a RISC Zero home of their own
beside the repository (with `~/.risc0`'s 1.88.0 their build stops at "ruint@1.21.1 requires rustc 1.90");
`scripts/ci/install-risc0.sh` installs `rzup`, r0vm 3.0.5 and both guest toolchains:

```sh
scripts/fetch-upstream.sh
RISC0_HOME_LEZ=$PWD/../risc0-home-lez scripts/ci/install-risc0.sh
cd lez && RISC0_HOME=$PWD/../../risc0-home-lez RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads 1
```

On a 16-core machine: about 7 minutes of build, then 56 tests in 30 binaries in about 21 minutes (one test
thread; `vault_consumer` alone runs six minutes without printing; about 13 minutes with `--test-threads 8`). The
tests rewrite the 21 files they write in `lez/results/`; the committed ones were recorded from this tree's programs
([`docs/image-ids.md`](../image-ids.md); bridge image `ccde6171…`) on 12 November 2026, so git shows them unchanged
(the Stage 1, 4 and 5 tables that the `tables` binary of `lez/RESULTS.md` §8 writes are of the earlier images that
file names). The rest of the setup, the real proofs and the tables: [`lez/RESULTS.md`](../../lez/RESULTS.md) §8.

The larger jobs have one command each (`just m0`, `just bench`, `just release`:
[docs/release-engineering.md](../release-engineering.md)); they need [`just`](https://github.com/casey/just), or
run the script each recipe wraps, named there (`just m0` is `python3 -I scripts/m0.py`, which only reads files).
Every component's re-check command is in [docs/components.md](../components.md).
