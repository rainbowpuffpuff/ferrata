# relayer-e2e (logosctl)

- PASS the module loaded in a Logos Core of its own, version() {"module":"ferrata_relayer_module","version":"0.1.0","lez_wallet":false}
relayer: spike/ferrata-wt-final-verify/modules/scripts/relayer-module.sh
- PASS fork: relayer_fork.rs on an anvil fork of Sepolia with crown-live-1's proofs (71 s): 34 daemon lines; 1 checkpoint(s), stop: SIGTERM
- PASS roots: no light client on the zone yet: waiting (the zone holds no instance 0x1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d of the attestation program)
- PASS roots: a record of 2 deposits (block 6, slot 9000064): ImportRoot n = 2 applied ({"leafCount":2,"recordSlot":9000064,"block":6,"tx":"devnet block 4"})
- PASS roots: the relayer's ImportRoot is byte for byte ferrata_bridge_module's import_root for the same record (importable, 2215 bytes)
- PASS roots: up to date: no import line, nothing sent
- PASS roots: a third deposit not yet recorded: waiting (the newest record, header(id, 9000064) of block 6, holds 2 deposits, 2 are imported, the vault holds 3: waiting for a later record)
- PASS roots: a record the proofs do not match (state root changed): refused by the dry run, code 110 (ferrata_bridge_module's import_root: 110 UNKNOWN), nothing sent
- PASS roots: the next honest record: ImportRoot n = 3 applied ({"leafCount":3,"previous":2,"recordSlot":9000192})
- PASS roots: the cadence: a fourth deposit recorded within the hour of the last attempt (period 498516) is not imported (imported [2,3])
- PASS roots: the next hour of anvil's clock: the daemon imports n = 4; clean stop on SIGTERM (exit 0, {"rounds":8,"reason":"SIGTERM"})

11 passed, 0 failed
