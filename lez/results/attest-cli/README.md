# `ferrata-attest`: one run on the Modal build box, 11 November 2026

`run.txt` is the output of (from `lez/`, after `cargo build -p harness --locked --bin ferrata-attest`):

```sh
RISC0_DEV_MODE=1 target/debug/ferrata-attest --message-out results/attest-cli/messages \
  status --then slot --then slot --send --then slot --expect-eq 0x01 --then slot --expect-eq 0x01 --send \
  --then log --then log --send --then receipt --tx 1 --send --then receipt --tx 56 \
  --then log --tx 84 --log 0 --then log --tx 84 --log 0 --send --then account --send \
  --then log --block-log-index 100 --send
```

`exit.txt`: exit status 3 (some commands were refused, as their dry runs predicted; 1 would mean a
dry run and its execution disagreed) after 107 s, of which 100.9 s following Sepolia (`stderr.txt`).
`messages/message-N.{hex,debug}`: each command's `Attest` message (borsh) and its Rust `Debug` text,
N counting the commands from 1 (`status`); message 9 (the 39-log receipt, 345,894 bytes) is not kept.
`idl-decode.txt`: messages 2, 6 and 8 decoded with `idl/ferrata_attestation.idl.json` alone
(`python3 -I idl/check.py --decode ...`), each equal to its Rust `Debug` text.
