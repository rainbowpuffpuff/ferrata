Receipts of two real proofs of the fixture `cp-empty-1`, made with checkpoint image
`0xbbf58feda031d9a7263d64efeac5979762443acd1beb3a4b31ef73e1163334c0` (the program with SPEC-v3 D3's
checked producer and 17-word journal, *before* the posting-timeframe amendment changed the key-set
hash and so the image id). `seal.hex` is the on-chain seal (selector `0x73c457ba` ‖ 256-byte Groth16
proof), `journal.hex` the 544-byte journal, `receipt-groth16.bin` the bincode Groth16 receipt; the
224,314-byte succinct receipts are not committed. Logs: `results/logs/`.

- `cp-empty-1-under-load/`: first run (other agents loading the machine).
- `cp-empty-1-run2/`: second run (started on a quiet machine).
