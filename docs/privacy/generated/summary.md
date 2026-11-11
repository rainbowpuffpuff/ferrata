# Privacy suite: generated results

Bridge program image `cf41b7e5c0583686a273b5114368f4873bbd614249647f43604a6caf8e675390` (built at this worktree's path); vault `0xa0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0` on chain 1; configuration `0xba0bb1e8bb8df68960b1bdd645d0b1078e9d5b8b46efa1b4fedaaec83dc0a243`; 20 users per direction. Written by `scripts/privacy/adversary.py`.

## The differential gate

| records | bytes (LEZ transaction / Ethereum calldata), min-max | identical once nullifiers, fresh randomness, own addresses and time are cut |
|---|---|---|
| deposit (A: private destinations) | 100-100 | yes |
| deposit (B: public destinations) | 100-100 | yes |
| claim, private (default) | 8,937-8,937 | yes |
| claim, naive nullifier N = fid | 8,937-8,937 | yes |
| claim, plaintext | 2,053-2,053 | NO: lez:claim:plaintext:01 vs lez:claim:plaintext:00: part tx differs at byte 133 of 1837/1837 after the cut (604a6caf8e675390505dac63c2398893a28af818fa03ceff121a6a8e1635d324 / 604a6caf8e6753907828282b5c8a554cd898798f67a1f779fe87c3cb85c5c28d) |
| burn, private holding | 7,857-7,857 | yes |
| release, NOut of the release statement | 324-324 | yes |
| release, naive NOut = burnId | 324-324 | yes |

## The adversary

Chance is 1/20 = 5.0%. Success = mean over the right-hand records of 1/|best candidates| when the true partner is among them. Order-only = pairing the k-th of each side by time (allowed signal; informational).

| game | success | exactly identified | candidate-set sizes (size: records) | without derived identifiers | with order-bearing state | order only |
|---|---:|---:|---|---:|---:|---:|
| inbound, private (default) | 5.0% | 0 of 20 | 20: 20 | 5.0% | 5.0% | 0.0% |
| inbound, naive nullifier N = fid | 100.0% | 20 of 20 | 1: 20 | 5.0% | 100.0% | 0.0% |
| inbound, plaintext claim | 100.0% | 20 of 20 | 1: 20 | 100.0% | 100.0% | 5.0% |
| outbound, private (default) | 5.0% | 0 of 20 | 20: 20 | 5.0% | 5.0% | 5.0% |
| outbound, naive NOut = burnId | 100.0% | 20 of 20 | 1: 20 | 5.0% | 100.0% | 5.0% |
| round trip: deposit to release (informational) | 5.0% | 0 of 20 | 20: 20 | 5.0% | 5.0% | 5.0% |
| on LEZ: claim to burn (informational) | 4.5% | 0 of 20 | 1: 2, 20: 18 | 4.5% | 4.5% | 5.0% |

## Recovery from the seed alone (after 10 claims and 10 releases)

- unclaimed deposits recovered: 10 of 10; claimed ones seen as claimed: 10 of 10
- unreleased burns recovered: 10 of 10; released ones seen as released: 10 of 10
- other users' items wrongly taken as own: 0; recipients decrypted and authenticated: 20 of 20; recovered NOut equal to the vault's: 20 of 20
- the bridge core's `recover_burns` (D12's v2 mac, review I-4) on the same notes: 20 found

Private actions proven against the newest commitment-set root: 80 of 80.

## Receipts

- Ethereum gas used, deposits by leaf index (universe A): [139740, 100062, 80162, 100184, 80162, 80284, 80284, 100306, 80162, 80284, 80284, 80406, 80284, 80406, 80406, 100428, 80162, 80284, 80284, 80406]; the same in universe B (other commitments, public destinations): True
- Ethereum gas used, releases: distinct values [80461, 105461] (NOut of the release statement), [80461, 105461] (naive)
- execution gas by position, minus the first: private claims [0, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720]; plaintext claims [0, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720, 4720]; burns [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]; releases (Ethereum) [0, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000]
- LEZ claims: the same gas steps by position in both universes (users in different orders): yes
- LEZ burns: only the first differs: yes
- Ethereum releases: only the first differs, by 25,000 (the relayer's account created): yes
- LEZ, claim, private (default): gas_exec 547629, 552349; cycles 137865, 142585; gas_stor 224063; fee_base 6173536, 6211296
- LEZ, burn, private holding: gas_exec 514490; cycles 104726; gas_stor 224063; fee_base 5908424
- LEZ, claim, plaintext: gas_exec 255012, 259732; cycles 255012, 259732; gas_stor 2053; fee_base 2056520, 2094280

