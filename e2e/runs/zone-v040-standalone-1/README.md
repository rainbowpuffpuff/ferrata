# LEZ v0.4.0's standalone sequencer: a public and a private transfer with receipts, LEZ's sealed wallet, a program deployed

**Run:** from `checkpoint/`, `ZONE_PROGRAM=<ferrata_rebate.bin>:0xeddda0ba… attestor/zone-v040.sh standalone <bundle> OUT
120` on a Modal build box, 28 November 2026, 02:18–02:24 UTC ([`run.log`](run.log), [`facts.json`](facts.json)). The
bundle is `checkpoint/attestor/zone-bundle.sh v0.4.0`'s, built on the same box from LEZ's own tree at the release tag
(v0.4.0, `a255b42c`, unmodified): LEZ's `sequencer_service` with its `standalone` feature (its mock Bedrock actor; no
Logos Blockchain node in this run), LEZ's `wallet`, and r0vm 3.1.0 (the risc0-zkvm of v0.4.0's lockfile, LEZ PR 1124);
each binary's sha256 is in `facts.json`. The program deployed is `ferrata_rebate.bin` as built for v0.4.0 from this
tree's `lez-040-core` with `lez/scripts/lez-select.sh v0.4.0` (image id `0xeddda0ba…8712`, 305,052 bytes). Nothing
was sent to any public network; RISC0_DEV_MODE was unset for the sequencer and the wallet (real proofs, verified).

**What it shows, each from the record:**

| | |
|---|---|
| The standalone sequencer on LEZ's `local.json` (finality_depth 35, no exit_delay: LEZ PR 1026) | block 2 18 s after start, block 21 after 312 s |
| LEZ's wallet, its storage sealed under a password made for the run (LEE_WALLET_PASSWORD; LEZ PR 987, already in rc3) | `storage.json` holds `version`, `kdf`, `nonce`, `ciphertext` (`wallet_storage_sealed`); `check-health` passed |
| LEZ's published local test account imported by `account import public --secret` (v0.4.0's form, LEZ PR 1135) | `Public/7VDm7YRP…oain`, the v0.4.0 tree's own `PUB_ACC_A_TEXT_ADDR` |
| A public transfer of 1,000 to a new account; its receipt from the sequencer (`getTransactionReceipt`, LEZ PR 1107) | `success: true`; storage gas 621 bytes, base fee 4,968; the new account holds 1,000 |
| A transfer of 500 into a new private account of the wallet: the privacy circuit proven by the wallet (r0vm 3.1.0), verified by the sequencer | proven and sent in 140 s; receipt `success: true`, execution gas 409,764, storage gas 235,661 bytes (a private transaction metered by its size: LEZ PR 1044), base fee 5,163,400 |
| `wallet program-loader deploy` of `ferrata_rebate.bin`, immutable, at fresh header and segment accounts | header `Public/2diWPJ77…b7eH`, 3 segments, 121 s; r0vm printed the image id the build records |
| The blocks as the sequencer serves them (`getBlock`, base64 Borsh): [`blocks/`](blocks/) | read by the test network's block readers (`deploy/testnet/tests/test_host_blocks.py`: `ferrata-status` and `ferrata-bridge-identity` agree on id, hash, time and status) |

Checked by `python3 -I checkpoint/attestor/check_zone_v040.py e2e/runs/zone-v040-standalone-1` (`facts.json: ok`).

**Not in this run:** a Logos Blockchain node (the standalone build has none; see `zone-v040-four-keys-1/` and
`zone-v040-vote-1/`), a generic `wallet program call` (no call valid on a freshly deployed `ferrata_rebate` was
available: its `Land` on an unconfigured pool exits with its own refusal code), Ferrata's attestation and bridge
programs (not yet built for v0.4.0 when this ran).

**Its paths:** the run's own scrub refused a relative TMPDIR given to it (the script now makes its work directory absolute); the records were scrubbed afterwards with `scripts/scrub_records.py` (the run's scratch as `work/`) and checked: none names the machine.

**Earlier attempts (not records):** the first two launches stopped on script bugs fixed before this run (a shadowed
records variable; the private transfer's hash read from the transaction's data instead of the wallet's
`Transaction hash is` line); one box container was lost mid-run and the runs were started again.
