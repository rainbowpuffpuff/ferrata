## Limits of the public endpoints

The JSON-RPC at `https://{{HOST}}/rpc` is shared. The gateway in front of the sequencer prices each request by its work:

- **Per IP:** at most 6 requests at once; about 10 tokens a second (bursts of 60). A request costs 1; `getBlockRange`
  1 more per 10 blocks (at most **50 blocks per call**); `getProofsAndRoot` and `getAccountsNonces` 1 more per list
  entry beyond 10 (at most 64 entries).
- **Answers:** at most 8 MiB each. Bytes are budgeted: about 128 KiB/s per IP (a quarter of the 512 KiB/s for all
  callers together; bursts of 4 MiB and 32 MiB). An answer is sent only while your budget holds it (one larger than
  the burst goes when the budget is full); otherwise 429 with the seconds to wait. When all callers together have
  used up theirs, answers of up to 16 KiB still pass: a few heavy clients cannot stop everyone's small calls.
- **`sendTransaction`:** 0.5 a second per IP (bursts of 12) and **0.5 a second for all public callers together**
  (bursts of 10). The zone makes one block of at most 20 transactions every 10 s; most of it is kept for the network's
  own services (the attestation updater, the faucet).
- Batches of at most 20 requests; bodies of at most 1.1 MB; a method outside the read/send allowlist gets 403.
- On HTTP **429 or 503**, wait the `Retry-After` seconds and try again. Do not retry a 400: change the request.
- **Faucet:** 10 LGO a claim; one claim per account a day, 3 per IP a day (failed ones count), 10 per /24 (IPv6 /64)
  a day, 120 an hour for everyone; one claim at a time per IP and one transfer at a time (429 "busy": try again after
  10 s).
- **Logs:** each request is logged with the client's IP address, to apply these limits and to handle abuse; the logs
  are kept for 30 days.
