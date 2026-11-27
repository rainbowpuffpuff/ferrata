# Privacy suite: generated results

Bridge program image `9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa321`; vault `0xa0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0` on chain 1; configuration `0xc3faeb3e2051fe0d94cc62f192db0747d1c603711c1ea3c505dcfab79851c682`; 20 users per direction; proofs: **dev** (LEZ's privacy-preserving circuit `8fc948d117c11d55f2036748013b3dc599d6995545f0b020d090f544081cf257`). Written by `scripts/privacy/adversary.py`.

## The differential gate

| records | bytes (LEZ transaction / Ethereum calldata), min-max | identical once nullifiers, fresh randomness, own addresses and time are cut |
|---|---|---|
| deposit (A: private destinations) | 100-100 | yes |
| deposit (B: public destinations) | 100-100 | yes |
| claim, private (default) | 8,889-8,889 | yes |
| claim, naive nullifier N = fid | 8,889-8,889 | yes |
| claim, plaintext | 2,029-2,029 | NO: lez:claim:plaintext:01 vs lez:claim:plaintext:00: part tx differs at byte 69 of 1813/1813 after the cut (f68e5bc343bfa3216961de8711a72eb219fd1415565b9abb2fa11b25aa3a87a1 / f68e5bc343bfa3214b8435f95edf9e9d0a4c8c314d26e62afe2d1f9d2da44a29) |
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

- Ethereum gas used, deposits by leaf index (universe A): [140664, 100986, 81086, 101108, 81086, 81208, 81208, 101230, 81086, 81208, 81208, 81330, 81208, 81330, 81330, 101352, 81086, 81208, 81208, 81330]; the same in universe B (other commitments, public destinations): True
- Ethereum gas used, releases: distinct values [83597, 108597] (NOut of the release statement), [83597, 108597] (naive)
- execution gas by position, minus the first: private claims [0, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360]; plaintext claims [0, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360, 4360]; burns [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]; releases (Ethereum) [0, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000, -25000]
- LEZ claims: the same gas steps by position in both universes (users in different orders): yes
- LEZ burns: only the first differs: yes
- Ethereum releases: only the first differs, by 25,000 (the relayer's account created): yes
- LEZ, claim, private (default): gas_exec 552751, 557111; cycles 142987, 147347; gas_stor 224063; fee_base 6214512, 6249392
- LEZ, burn, private holding: gas_exec 511906; cycles 102142; gas_stor 224063; fee_base 5887752
- LEZ, claim, plaintext: gas_exec 260943, 265303; cycles 260943, 265303; gas_stor 2029; fee_base 2103776, 2138656

## Proofs

What the collectors recorded about each proof (not public: proving time, the checks) and how each part of the proof varies across the records of a step (public).

| step | receipt kind | proof bytes | transaction bytes | seal words | proving time, s (min / median / max; sum) | journal = the message's frame | claim = ok(circuit, journal) | seal outputs = claim digest |
|---|---|---|---|---|---|---|---|---|
| claim, private (default) | fake | 4,501 | 8,889 | - | 1.2 / 1.3 / 1.3; 25 | 20 of 20 | 20 of 20 | - |
| burn, private holding | fake | 3,985 | 7,857 | - | 0.7 / 0.9 / 0.9; 17 | 20 of 20 | 20 of 20 | - |
| release, NOut of the release statement | - | - | - | - | - | - | - | - |
| release, naive NOut = burnId | - | - | - | - | - | - | - | - |

The three check columns are computed by this script from the public bytes alone (the claim parsed and hashed by RISC Zero's rules, the seal's output words decoded from Montgomery form).

| step | proof field | bytes | distinct across the records | class |
|---|---|---|---:|---|
| claim, private (default) | `proof.claim` | 4,500 | 20 of 20 | a function of public data |
| claim, private (default) | `proof.kind` | 1 | 1 of 20 | constant |
| claim, private (default) | `proof.length` | 4 | 1 of 20 | constant |
| claim, private (default) | `witness_set.signatures` | 4 | 1 of 20 | constant |
| burn, private holding | `proof.claim` | 3,984 | 20 of 20 | a function of public data |
| burn, private holding | `proof.kind` | 1 | 1 of 20 | constant |
| burn, private holding | `proof.length` | 4 | 1 of 20 | constant |
| burn, private holding | `witness_set.signatures` | 4 | 1 of 20 | constant |
| release, NOut of the release statement | `calldata.seal.length` | 32 | 1 of 20 | constant |
| release, NOut of the release statement | `calldata.seal.offset` | 32 | 1 of 20 | constant |
| release, NOut of the release statement | `calldata.seal[0]` | 32 | 1 of 20 | constant |
| release, NOut of the release statement | `receipt.calldataGas` | 32 | 1 of 20 | a function of public data |
| release, naive NOut = burnId | `calldata.seal.length` | 32 | 1 of 20 | constant |
| release, naive NOut = burnId | `calldata.seal.offset` | 32 | 1 of 20 | constant |
| release, naive NOut = burnId | `calldata.seal[0]` | 32 | 1 of 20 | constant |
| release, naive NOut = burnId | `receipt.calldataGas` | 32 | 2 of 20 | a function of public data |


