# The bridge program on FerrataVault's own storage (`harness/tests/bridge_vault_storage.rs`)

Bridge program image `cf41b7e5c0583686a273b5114368f4873bbd614249647f43604a6caf8e675390`. The blocks P and Q hold exactly the storage FerrataVault v5 wrote in `vault/test/RegistryStorage.t.sol` (`vault/test/vectors/registry-storage.json`: 139 and 149 slots, 3 and 6 leaves); `Init` under a synthetic block (the pairing is `bridge_inbound.rs`'s subject). Every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init (pairing under a synthetic block: slot 2 = configId) | bridge Init 41,577 → Attest 74,387 → bridge (reply) 39,144 | 155,108 | 3% | 1,731 | 1,254,712 | applied |
| 2 | Register ETH with lezUnit ×10 (not the vault's entry) | bridge Register 104,658 → Attest 219,872 → bridge (reply) 49,768 | 374,298 | 7% | 4,974 | 3,034,176 | refused: exit 247 (IMPORT_MISMATCH) |
| 3 | Register ETH with schedule 0 a second later | bridge Register 104,466 → Attest 219,872 → bridge (reply) 52,497 | 376,835 | 8% | 4,974 | 3,054,472 | refused: exit 247 (IMPORT_MISMATCH) |
| 4 | Register ETH: the vault's entry and schedule 0 at P | bridge Register 104,466 → Attest 219,872 → bridge (reply) 73,240 → bridge WriteToken 31,489 | 429,067 | 9% | 4,974 | 3,472,328 | applied |
| 5 | Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge Register 98,411 → Attest 225,935 → bridge (reply) 73,240 → bridge WriteToken 31,489 | 429,075 | 9% | 4,999 | 3,472,592 | applied |
| 6 | Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge Register 99,579 → Attest 222,977 → bridge (reply) 73,240 → bridge WriteToken 31,489 | 427,285 | 9% | 4,805 | 3,456,720 | applied |
| 7 | Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge Register 101,455 → Attest 244,382 → bridge (reply) 73,240 → bridge WriteToken 31,489 | 450,566 | 9% | 5,264 | 3,646,640 | applied |
| 8 | Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge Register 95,815 → Attest 229,262 → bridge (reply) 71,448 → bridge WriteToken 30,316 | 426,841 | 9% | 5,084 | 3,455,400 | applied |
| 9 | ImportRoot: the vault's slots 0 and 1 at P | bridge ImportRoot 62,202 → Attest 146,307 → bridge (reply) 52,241 → bridge WriteRoot 16,513 | 277,263 | 6% | 3,095 | 2,242,864 | applied |
| 10 | Claim leaf 0 (ETH, under root(n = 3)) | bridge Claim 107,479 → bridge Issue 48,135 → bridge Admit 33,886 → bridge Spend 30,300 → bridge Mint 17,560 → bridge Mint 17,560 | 254,920 | 5% | 2,053 | 2,055,784 | applied |
| 11 | Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Claim 107,479 → bridge Issue 46,823 → bridge Admit 33,886 → bridge Spend 30,300 → bridge Mint 17,560 → bridge Mint 22,801 | 258,849 | 5% | 2,053 | 2,087,216 | applied |
| 12 | Claim leaf 2 (ETH, under root(n = 3)) | bridge Claim 107,479 → bridge Issue 48,100 → bridge Admit 33,886 → bridge Spend 30,300 → bridge Mint 17,560 → bridge Mint 23,761 | 261,086 | 5% | 2,053 | 2,105,112 | applied |
| 13 | ImportFees ETH schedule 1 with another mint fee | bridge ImportFees 66,700 → Attest 144,300 → bridge (reply) 42,097 | 253,097 | 5% | 3,179 | 2,050,208 | refused: exit 247 (IMPORT_MISMATCH) |
| 14 | ImportFees ETH: the vault's schedule 1 at Q | bridge ImportFees 66,700 → Attest 144,300 → bridge (reply) 56,755 → bridge ExtendFees 41,163 | 308,918 | 6% | 3,179 | 2,496,776 | applied |
| 15 | ImportFees ETH: no schedule 2 at Q | bridge ImportFees 60,352 → Attest 129,482 → bridge (reply) 51,094 → bridge ExtendFees 39,119 | 280,047 | 6% | 2,768 | 2,262,520 | applied |
| 16 | ImportFees token 0xf62849…: no schedule 1 at Q | bridge ImportFees 60,895 → Attest 134,749 → bridge (reply) 51,094 → bridge ExtendFees 35,552 | 282,290 | 6% | 2,877 | 2,281,336 | applied |
| 17 | ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ImportFees 61,820 → Attest 133,721 → bridge (reply) 51,094 → bridge ExtendFees 35,552 | 282,187 | 6% | 2,811 | 2,279,984 | applied |
| 18 | ImportFees token 0xc71834…: no schedule 1 at Q | bridge ImportFees 63,137 → Attest 140,775 → bridge (reply) 51,094 → bridge ExtendFees 35,552 | 290,558 | 6% | 3,016 | 2,348,592 | applied |
| 19 | ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ImportFees 61,669 → Attest 136,967 → bridge (reply) 51,094 → bridge ExtendFees 34,429 | 284,159 | 6% | 2,945 | 2,296,832 | applied |
| 20 | ImportRoot: the vault's slots 0 and 1 at Q | bridge ImportRoot 62,512 → Attest 146,833 → bridge (reply) 52,241 → bridge WriteRoot 16,513 | 278,099 | 6% | 3,127 | 2,249,808 | applied |
| 21 | Claim leaf 3 (ETH, under root(n = 6)) | bridge Claim 107,479 → bridge Issue 52,468 → bridge Admit 33,886 → bridge Spend 36,659 → bridge Mint 17,560 → bridge Mint 17,560 → bridge Mint 23,761 | 289,373 | 6% | 2,053 | 2,331,408 | applied |
| 22 | Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Claim 107,479 → bridge Issue 46,839 → bridge Admit 33,886 → bridge Spend 30,300 → bridge Mint 17,560 → bridge Mint 24,008 | 260,072 | 5% | 2,053 | 2,097,000 | applied |
| 23 | Claim leaf 5 (ETH, under root(n = 6)) | bridge Claim 107,479 → bridge Issue 52,519 → bridge Admit 33,886 → bridge Spend 36,659 → bridge Mint 17,560 → bridge Mint 22,280 → bridge Mint 23,761 | 294,144 | 6% | 2,053 | 2,369,576 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init (pairing under a synthetic block: slot 2 = configId) | bridge Init | 41,577 | 1 | 1,278 | 1,204 | 0 | 2,706 | 390 | 0 |
| Init (pairing under a synthetic block: slot 2 = configId) | Attest | 74,387 | 1 | 1,412 | 941 | 333 | 1,875 | – | 0 |
| Init (pairing under a synthetic block: slot 2 = configId) | bridge (reply) | 39,144 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | bridge Register | 104,658 | 1 | 5,042 | 4,448 | 520 | 10,305 | 906 | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | bridge (reply) | 49,768 | 1 | 1,765 | 721 | 906 | 0 | – | 247 |
| Register ETH with schedule 0 a second later | bridge Register | 104,466 | 1 | 5,042 | 4,448 | 520 | 10,305 | 906 | 0 |
| Register ETH with schedule 0 a second later | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH with schedule 0 a second later | bridge (reply) | 52,497 | 1 | 1,765 | 721 | 906 | 0 | – | 247 |
| Register ETH: the vault's entry and schedule 0 at P | bridge Register | 104,466 | 1 | 5,042 | 4,448 | 520 | 10,305 | 906 | 0 |
| Register ETH: the vault's entry and schedule 0 at P | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH: the vault's entry and schedule 0 at P | bridge (reply) | 73,240 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH: the vault's entry and schedule 0 at P | bridge WriteToken | 31,489 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge Register | 98,411 | 1 | 5,067 | 4,473 | 520 | 10,355 | 906 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | Attest | 225,935 | 1 | 4,756 | 4,285 | 333 | 5,570 | – | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge (reply) | 73,240 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge WriteToken | 31,489 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge Register | 99,579 | 1 | 4,873 | 4,279 | 520 | 9,967 | 906 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | Attest | 222,977 | 1 | 4,562 | 4,091 | 333 | 5,376 | – | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge (reply) | 73,240 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge WriteToken | 31,489 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge Register | 101,455 | 1 | 5,332 | 4,738 | 520 | 10,885 | 906 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | Attest | 244,382 | 1 | 5,021 | 4,550 | 333 | 5,835 | – | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge (reply) | 73,240 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge WriteToken | 31,489 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge Register | 95,815 | 1 | 5,152 | 4,558 | 520 | 10,525 | 826 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | Attest | 229,262 | 1 | 4,921 | 4,450 | 333 | 5,735 | – | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge (reply) | 71,448 | 1 | 1,685 | 721 | 826 | 3,097 | 520 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge WriteToken | 30,316 | 1 | 901 | 763 | 0 | 1,689 | 763 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge ImportRoot | 62,202 | 1 | 3,163 | 2,569 | 520 | 6,479 | 569 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | Attest | 146,307 | 1 | 3,121 | 2,650 | 333 | 3,701 | – | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge (reply) | 52,241 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge WriteRoot | 16,513 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Issue | 48,135 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Spend | 30,300 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Issue | 46,823 | 1 | 1,100 | 199 | 763 | 2,356 | 763 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Spend | 30,300 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Mint | 22,801 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Issue | 48,100 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Spend | 30,300 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Mint | 23,761 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| ImportFees ETH schedule 1 with another mint fee | bridge ImportFees | 66,700 | 1 | 3,247 | 2,653 | 520 | 6,643 | 746 | 0 |
| ImportFees ETH schedule 1 with another mint fee | Attest | 144,300 | 1 | 3,024 | 2,553 | 333 | 3,604 | – | 0 |
| ImportFees ETH schedule 1 with another mint fee | bridge (reply) | 42,097 | 1 | 1,371 | 487 | 746 | 0 | – | 247 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge ImportFees | 66,700 | 1 | 3,247 | 2,653 | 520 | 6,643 | 746 | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | Attest | 144,300 | 1 | 3,024 | 2,553 | 333 | 3,604 | – | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge (reply) | 56,755 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge ExtendFees | 41,163 | 1 | 1,239 | 258 | 843 | 2,295 | 1,031 | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge ImportFees | 60,352 | 1 | 2,836 | 2,242 | 520 | 5,821 | 558 | 0 |
| ImportFees ETH: no schedule 2 at Q | Attest | 129,482 | 1 | 2,801 | 2,330 | 333 | 3,381 | – | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge (reply) | 51,094 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge ExtendFees | 39,119 | 1 | 1,239 | 70 | 1,031 | 2,295 | 1,031 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge ImportFees | 60,895 | 1 | 2,945 | 2,351 | 520 | 6,039 | 558 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | Attest | 134,749 | 1 | 2,910 | 2,439 | 333 | 3,490 | – | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge (reply) | 51,094 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge ExtendFees | 35,552 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ImportFees | 61,820 | 1 | 2,879 | 2,285 | 520 | 5,907 | 558 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | Attest | 133,721 | 1 | 2,844 | 2,373 | 333 | 3,424 | – | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge (reply) | 51,094 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ExtendFees | 35,552 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge ImportFees | 63,137 | 1 | 3,084 | 2,490 | 520 | 6,317 | 558 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | Attest | 140,775 | 1 | 3,049 | 2,578 | 333 | 3,629 | – | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge (reply) | 51,094 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge ExtendFees | 35,552 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ImportFees | 61,669 | 1 | 3,013 | 2,419 | 520 | 6,175 | 558 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | Attest | 136,967 | 1 | 2,978 | 2,507 | 333 | 3,558 | – | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge (reply) | 51,094 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ExtendFees | 34,429 | 1 | 971 | 70 | 763 | 1,759 | 763 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge ImportRoot | 62,512 | 1 | 3,195 | 2,601 | 520 | 6,543 | 569 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | Attest | 146,833 | 1 | 3,153 | 2,682 | 333 | 3,733 | – | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge (reply) | 52,241 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge WriteRoot | 16,513 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Issue | 52,468 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Spend | 36,659 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 23,761 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Issue | 46,839 | 1 | 1,100 | 199 | 763 | 2,356 | 763 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Spend | 30,300 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Mint | 24,008 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Issue | 52,519 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Spend | 36,659 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 22,280 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 23,761 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
