# RFP-022 F.13 end to end: the protocol fees charged on LEZ in the wrapped token (ADR 0048), on a LEZ v0.4.0-rc3 sequencer

27 November 2026, on the build box (Modal), by `e2e/rc3-roundtrip/run.sh f13-lezfee-1` with
`FERRATA_RC3_RT_HOOK=e2e/f13/hook.sh` and `FERRATA_RC3_RT_STAGES="run summary"`: CI's round trip on LEZ v0.4.0-rc3's
standalone sequencer and anvil forking Sepolia (`e2e/runs/rc3-roundtrip-1/REHEARSAL.md` describes that rebuild), with
the hook of `e2e/f13/hook.sh` in place of CI's. `README.md` beside this file is the round trip's own generic summary
(CI's summary stage; its single "claim" line is the first claim); `run/steps.tsv` is the hook's steps;
`run/facts.json`'s `fees` holds the F.13 facts below; `run/user/` what each `ferrata` command printed;
`run/lez/1*-collect-*.json` what each collection read, minted or was refused.

The code: this tree's vault (ADR 0048's deposit, release and refund) and this tree's bridge program,
`ferrata_bridge_lezfee` (`0x9d02d54d…a321`, `docs/image-ids.md`), deployed through LEZ's program loader, which
recomputed that image id.

What ran, every step ok (23 of 23):

| Step | What it showed (`run/steps.tsv`, `run/facts.json`) |
|---|---|
| deposit under schedule 0 | `ferrata deposit` of 0.001 ETH: the vault's escrow +1,002,000,000,000,000 wei (the denomination and the relay fee 2,000,000,000,000), `lezFees` +0, the Ethereum fee recipient's credit (`owed`) +0 |
| fee change | the admin's `announceFees` (mint fees 5,000,000,000,000 / 50,000,000,000,000 / 500,000,000,000,000 wei and burn fees 2,000,000,000,000 / 20,000,000,000,000 / 200,000,000,000,000 wei for the three denominations, checkpoint fees unchanged), seven days passed on the fork's clock, `applyFees`: schedule 1 in force |
| deposit under schedule 1 | `ferrata deposit` again (priced by the tool under schedule 1): escrow +1,007,000,000,000,000 wei (the denomination, the relay fee and the mint fee 5,000,000,000,000), `lezFees` +5,000,000,000,000 (one `LezFeesAccrued`), no `OwedCredited`, the Ethereum fee recipient credited nothing |
| operator import | crown-driver: `Init`, `Register` ETH, `ImportRoot` n = 1 and n = 2 (schedule 1 not imported: neither claims nor collections read it) |
| claim of leaf 0 (schedule 0), claim of leaf 1 (schedule 1) | `ferrata claim --plaintext --leaf N --relayer <the user's public /1>`: each minted 1,000,000,000,000,000 to /0 and 2,000,000,000,000 to /1, the same amounts whatever the deposit's schedule |
| mint fees collected | `CollectFees` (`crown-driver collect-fees`, capture 2's record, header(id, 11665600) of block 12224229): the vault's `lezFees` 5,000,000,000,000 attested and minted to the configuration's fee recipient on LEZ (holding 0 → 5,000,000,000,000; `fees(cid, ETH)` counts it) |
| nothing more to collect | the same collection again: refused by the program, 244 `ALREADY_WRITTEN`, nothing minted |
| claim again refused, burn, checkpoint journal, checkpoint accepted, release journal | as CI's round trip: 250 `ALREADY_CLAIMED`; one denomination burnt (no fee on LEZ); the rule (B) checkpoint accepted |
| release paid | the release under schedule 1 paid 995,000,000,000,000 wei to the fresh address (1,000,000,000,000,000 less the relay fee 2,000,000,000,000, the burn fee 2,000,000,000,000 and the checkpoint fee 1,000,000,000,000); the checkpoint fee credited on Ethereum (`OwedCredited`); the burn fee kept in escrow (escrow −998,000,000,000,000) and counted (`LezFeesAccrued`; `lezFees` 5,000,000,000,000 → 7,000,000,000,000) |
| record after the release | the vault captured after the release; `crown-b --skip-stages 2 --updates-only yes`: header(id, 11665664) of block 12226934 recorded |
| burn fee collected | `CollectFees` under that record: `lezFees` 7,000,000,000,000 attested, 2,000,000,000,000 (the burn fee) minted to the fee recipient (holding 5,000,000,000,000 → 7,000,000,000,000) |
| older record refused | `CollectFees` under capture 2's record again (total 5,000,000,000,000, below the 7,000,000,000,000 minted): refused by the program, 247 `IMPORT_MISMATCH`, nothing minted |
| escrow backs LEZ | every deposit claimed, the burn released, every fee collected: the vault's ETH escrow, 1,011,000,000,000,000 wei, equals the wrapped ETH on LEZ (the user's /0 1,000,000,000,000,000, /1 4,000,000,000,000, the fee recipient 7,000,000,000,000) |
| release again refused | `NullifierAlreadyUsed` |

What it shows for F.13: both protocol fees are charged on LEZ, in the wrapped token, to the configuration's fee
recipient, at rates the admin set (and zero before), with a fee change in between: the claims across the change publish
and mint the same values, the burn burns one denomination, and the collections are totals over the deposits and the
release that name no user's transaction; every unit the vault escrows backs a unit on LEZ, fees included. What it is
not: the claims are plaintext (CI's job claims so: each publishes its deposit; the private claim of this program is
proven for real in `lez/results/f13/prove-bridge-lezfee.md`); the collections are sent by the operator's
`crown-driver collect-fees` from the captures' storage proofs, not by `ferrata-relayer collect-fees`: a stand-in
record's block hash is the stand-in's (anvil's fork mode writes a zero state root, which the capture replaces), which no
Ethereum endpoint serves to a relayer; the stand-ins are CI's (a test committee for the light client, the vault's
verifier replaced on the fork by the tests' mock accepting the dev-mode journals, `ci_zone`'s keys for the checkpoint's
chain); the sequencer is a local one in standalone mode; nothing was sent to Sepolia or to any public network.

Re-run: `FERRATA_RC3_RT_HOOK=e2e/f13/hook.sh FERRATA_RC3_RT_STAGES="run summary" e2e/rc3-roundtrip/run.sh NAME` on a
build machine with LEZ v0.4.0-rc1 and rc3 beside the repository (as `REHEARSAL.md` of `rc3-roundtrip-1` says) and
RISC Zero's toolchains (`scripts/ci/install-risc0.sh`).
