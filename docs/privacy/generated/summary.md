# Privacy suite: generated results

Bridge program image `23cc502547f9f0a57b1a46f0bde18e48e6a84fdedae3f7d83edfb03ffe739132` (built at this worktree's path); vault `0xa0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0` on chain 1; configuration `0x4bec031cc3356fc3dfd865548e4e4d42199f70edc6363e1bc31f2d211a69b492`; 20 users per direction. Written by `scripts/privacy/adversary.py`.

## The differential gate

| records | bytes (LEZ transaction / Ethereum calldata), min-max | identical once nullifiers, fresh randomness, own addresses and time are cut |
|---|---|---|
| deposit (A: private destinations) | 100-100 | yes |
| deposit (B: public destinations) | 100-100 | yes |
| claim, private (default) | 8,953-8,953 | yes |
| claim, naive nullifier N = fid | 8,953-8,953 | yes |
| claim, plaintext | 2,061-2,061 | NO: lez:claim:plaintext:01 vs lez:claim:plaintext:00: part tx differs at byte 5 of 1837/1837 after the cut (00070000003461fb6c10ff2a3297878f0c14c97c8853d10a15c6b59ef7 / 000700000046ffca34eae608130be303a685bafc3fb616fd90d4ea077c) |
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
| on LEZ: claim to burn (informational) | 4.5% | 0 of 20 | 1: 1, 2: 1, 20: 18 | 4.5% | 4.5% | 5.0% |

## Recovery from the seed alone (after 10 claims and 10 releases)

- unclaimed deposits recovered: 10 of 10; claimed ones seen as claimed: 10 of 10
- unreleased burns recovered: 10 of 10; released ones seen as released: 10 of 10
- other users' items wrongly taken as own: 0; recipients decrypted and authenticated: 20 of 20; recovered NOut equal to the vault's: 20 of 20
- the bridge core's `recover_burns` (v1 mac) on the same notes: 0 found

Private actions proven against the newest commitment-set root: 80 of 80.

## Receipts

- Ethereum gas used, deposits by leaf index (universe A): [138240, 98562, 78662, 98684, 78662, 78784, 78784, 98806, 78662, 78784, 78784, 78906, 78784, 78906, 78906, 98928, 78662, 78784, 78784, 78906]; the same in universe B (other commitments, public destinations): True
- Ethereum gas used, releases: distinct values [78781, 103781] (NOut of the release statement), [78781, 103781] (naive)
- execution gas by position, minus the first: private claims [0, 5188, 5189, 5189, 5187, 5188, 5188, 5186, 5186, 5186, 5186, 5186, 5186, 5187, 5186, 5186, 5187, 5186, 5186, 5186]; plaintext claims [0, 5188, 5189, 5189, 5187, 5188, 5188, 5186, 5186, 5186, 5186, 5186, 5186, 5187, 5186, 5186, 5187, 5186, 5186, 5186]; burns [0, -578, -578, -578, -578, -578, -578, -578, -578, -578, -578, -578, -578, -578, -578, -578, -578, -578, -578, -578]; releases (Ethereum) [0, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000]
- LEZ claims: the same gas steps by position in both universes (users in different orders): yes
- LEZ burns: only the first differs: yes
- Ethereum releases: only the first differs, by 25,000 (the relayer's account created): yes
- LEZ, claim, private (default): gas_exec [536214, 541400, 541401, 541402, 541403]; cycles [126450, 131636, 131637, 131638, 131639]; gas_stor 224063; fee_base [6082216, 6123704, 6123712, 6123720, 6123728]
- LEZ, burn, private holding: gas_exec 510617, 511195; cycles 100853, 101431; gas_stor 224063; fee_base 5877440, 5882064
- LEZ, claim, plaintext: gas_exec [236585, 241771, 241772, 241773, 241774]; cycles [236585, 241771, 241772, 241773, 241774]; gas_stor 2061; fee_base [1909168, 1950656, 1950664, 1950672, 1950680]

