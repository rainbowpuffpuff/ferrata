# The bridge program on FerrataVault's own storage (`harness/tests/bridge_vault_storage.rs`)

Bridge program image `43615dc48098b3731990de7700204d2a1bea0e9f963ee7c4dd629048dd5ef19c`. The blocks P and Q hold exactly the storage FerrataVault v5 wrote in `vault/test/RegistryStorage.t.sol` (`vault/test/vectors/registry-storage.json`: 139 and 149 slots, 3 and 6 leaves); `Init` under a synthetic block (the pairing is `bridge_inbound.rs`'s subject). Every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init (pairing under a synthetic block: slot 2 = configId) | bridge Init 42,488 → Attest 74,387 → bridge (reply) 39,419 | 156,294 | 3% | 1,731 | 1,264,200 | applied |
| 2 | Register ETH with lezUnit ×10 (not the vault's entry) | bridge Register 106,268 → Attest 219,872 → bridge (reply) 50,670 | 376,810 | 8% | 4,974 | 3,054,272 | refused: exit 247 (IMPORT_MISMATCH) |
| 3 | Register ETH with schedule 0 a second later | bridge Register 106,076 → Attest 219,872 → bridge (reply) 53,399 | 379,347 | 8% | 4,974 | 3,074,568 | refused: exit 247 (IMPORT_MISMATCH) |
| 4 | Register ETH: the vault's entry and schedule 0 at P | bridge Register 106,076 → Attest 219,872 → bridge (reply) 74,476 → bridge WriteToken 32,147 | 432,571 | 9% | 4,974 | 3,500,360 | applied |
| 5 | Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge Register 100,037 → Attest 225,935 → bridge (reply) 74,476 → bridge WriteToken 32,147 | 432,595 | 9% | 4,999 | 3,500,752 | applied |
| 6 | Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge Register 101,233 → Attest 222,977 → bridge (reply) 74,476 → bridge WriteToken 32,147 | 430,833 | 9% | 4,805 | 3,485,104 | applied |
| 7 | Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge Register 103,090 → Attest 244,382 → bridge (reply) 74,476 → bridge WriteToken 32,147 | 454,095 | 9% | 5,264 | 3,674,872 | applied |
| 8 | Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge Register 97,420 → Attest 229,262 → bridge (reply) 72,789 → bridge WriteToken 30,974 | 430,445 | 9% | 5,084 | 3,484,232 | applied |
| 9 | ImportRoot: the vault's slots 0 and 1 at P | bridge ImportRoot 63,991 → Attest 146,307 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 281,031 | 6% | 3,095 | 2,273,008 | applied |
| 10 | Claim leaf 0 (ETH, under root(n = 3)) | bridge Claim 107,702 → bridge Issue 47,201 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 18,161 | 257,706 | 5% | 2,053 | 2,078,072 | applied |
| 11 | Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Claim 107,702 → bridge Issue 45,889 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 23,017 | 261,250 | 5% | 2,053 | 2,106,424 | applied |
| 12 | Claim leaf 2 (ETH, under root(n = 3)) | bridge Claim 107,702 → bridge Issue 47,166 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 23,965 | 263,475 | 5% | 2,053 | 2,124,224 | applied |
| 13 | ImportFees ETH schedule 1 with another mint fee | bridge ImportFees 68,349 → Attest 144,300 → bridge (reply) 42,997 | 255,646 | 5% | 3,179 | 2,070,600 | refused: exit 247 (IMPORT_MISMATCH) |
| 14 | ImportFees ETH: the vault's schedule 1 at Q | bridge ImportFees 68,349 → Attest 144,300 → bridge (reply) 58,259 → bridge ExtendFees 41,471 | 312,379 | 6% | 3,179 | 2,524,464 | applied |
| 15 | ImportFees ETH: no schedule 2 at Q | bridge ImportFees 61,884 → Attest 129,482 → bridge (reply) 52,487 → bridge ExtendFees 39,427 | 283,280 | 6% | 2,768 | 2,288,384 | applied |
| 16 | ImportFees token 0xf62849…: no schedule 1 at Q | bridge ImportFees 62,526 → Attest 134,749 → bridge (reply) 52,487 → bridge ExtendFees 35,860 | 285,622 | 6% | 2,877 | 2,307,992 | applied |
| 17 | ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ImportFees 63,420 → Attest 133,721 → bridge (reply) 52,487 → bridge ExtendFees 35,860 | 285,488 | 6% | 2,811 | 2,306,392 | applied |
| 18 | ImportFees token 0xc71834…: no schedule 1 at Q | bridge ImportFees 64,816 → Attest 140,775 → bridge (reply) 52,487 → bridge ExtendFees 35,860 | 293,938 | 6% | 3,016 | 2,375,632 | applied |
| 19 | ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ImportFees 63,280 → Attest 136,967 → bridge (reply) 52,487 → bridge ExtendFees 34,737 | 287,471 | 6% | 2,945 | 2,323,328 | applied |
| 20 | ImportRoot: the vault's slots 0 and 1 at Q | bridge ImportRoot 64,301 → Attest 146,833 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 281,867 | 6% | 3,127 | 2,279,952 | applied |
| 21 | Claim leaf 3 (ETH, under root(n = 6)) | bridge Claim 107,702 → bridge Issue 51,535 → bridge Admit 35,565 → bridge Spend 37,291 → bridge Mint 18,161 → bridge Mint 18,161 → bridge Mint 23,965 | 292,380 | 6% | 2,053 | 2,355,464 | applied |
| 22 | Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Claim 107,702 → bridge Issue 45,905 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 24,212 | 262,461 | 5% | 2,053 | 2,116,112 | applied |
| 23 | Claim leaf 5 (ETH, under root(n = 6)) | bridge Claim 107,702 → bridge Issue 51,586 → bridge Admit 35,565 → bridge Spend 37,291 → bridge Mint 18,161 → bridge Mint 22,496 → bridge Mint 23,965 | 296,766 | 6% | 2,053 | 2,390,552 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init (pairing under a synthetic block: slot 2 = configId) | bridge Init | 42,488 | 1 | 1,278 | 1,204 | 0 | 2,706 | 390 | 0 |
| Init (pairing under a synthetic block: slot 2 = configId) | Attest | 74,387 | 1 | 1,412 | 941 | 333 | 1,875 | – | 0 |
| Init (pairing under a synthetic block: slot 2 = configId) | bridge (reply) | 39,419 | 1 | 898 | 370 | 390 | 1,487 | 564 | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | bridge Register | 106,268 | 1 | 5,086 | 4,448 | 564 | 10,393 | 950 | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | bridge (reply) | 50,670 | 1 | 1,809 | 721 | 950 | 0 | – | 247 |
| Register ETH with schedule 0 a second later | bridge Register | 106,076 | 1 | 5,086 | 4,448 | 564 | 10,393 | 950 | 0 |
| Register ETH with schedule 0 a second later | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH with schedule 0 a second later | bridge (reply) | 53,399 | 1 | 1,809 | 721 | 950 | 0 | – | 247 |
| Register ETH: the vault's entry and schedule 0 at P | bridge Register | 106,076 | 1 | 5,086 | 4,448 | 564 | 10,393 | 950 | 0 |
| Register ETH: the vault's entry and schedule 0 at P | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH: the vault's entry and schedule 0 at P | bridge (reply) | 74,476 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register ETH: the vault's entry and schedule 0 at P | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge Register | 100,037 | 1 | 5,111 | 4,473 | 564 | 10,443 | 950 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | Attest | 225,935 | 1 | 4,756 | 4,285 | 333 | 5,570 | – | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge (reply) | 74,476 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge Register | 101,233 | 1 | 4,917 | 4,279 | 564 | 10,055 | 950 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | Attest | 222,977 | 1 | 4,562 | 4,091 | 333 | 5,376 | – | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge (reply) | 74,476 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge Register | 103,090 | 1 | 5,376 | 4,738 | 564 | 10,973 | 950 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | Attest | 244,382 | 1 | 5,021 | 4,550 | 333 | 5,835 | – | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge (reply) | 74,476 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge Register | 97,420 | 1 | 5,196 | 4,558 | 564 | 10,613 | 870 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | Attest | 229,262 | 1 | 4,921 | 4,450 | 333 | 5,735 | – | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge (reply) | 72,789 | 1 | 1,729 | 721 | 870 | 3,185 | 564 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge WriteToken | 30,974 | 1 | 901 | 763 | 0 | 1,689 | 763 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge ImportRoot | 63,991 | 1 | 3,207 | 2,569 | 564 | 6,567 | 613 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | Attest | 146,307 | 1 | 3,121 | 2,650 | 333 | 3,701 | – | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Issue | 47,201 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Issue | 45,889 | 1 | 1,100 | 199 | 763 | 2,356 | 763 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Mint | 23,017 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Issue | 47,166 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Mint | 23,965 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| ImportFees ETH schedule 1 with another mint fee | bridge ImportFees | 68,349 | 1 | 3,291 | 2,653 | 564 | 6,731 | 790 | 0 |
| ImportFees ETH schedule 1 with another mint fee | Attest | 144,300 | 1 | 3,024 | 2,553 | 333 | 3,604 | – | 0 |
| ImportFees ETH schedule 1 with another mint fee | bridge (reply) | 42,997 | 1 | 1,415 | 487 | 790 | 0 | – | 247 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge ImportFees | 68,349 | 1 | 3,291 | 2,653 | 564 | 6,731 | 790 | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | Attest | 144,300 | 1 | 3,024 | 2,553 | 333 | 3,604 | – | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge (reply) | 58,259 | 1 | 1,415 | 487 | 790 | 2,366 | 564 | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge ExtendFees | 41,471 | 1 | 1,239 | 258 | 843 | 2,295 | 1,031 | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge ImportFees | 61,884 | 1 | 2,880 | 2,242 | 564 | 5,909 | 602 | 0 |
| ImportFees ETH: no schedule 2 at Q | Attest | 129,482 | 1 | 2,801 | 2,330 | 333 | 3,381 | – | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge ExtendFees | 39,427 | 1 | 1,239 | 70 | 1,031 | 2,295 | 1,031 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge ImportFees | 62,526 | 1 | 2,989 | 2,351 | 564 | 6,127 | 602 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | Attest | 134,749 | 1 | 2,910 | 2,439 | 333 | 3,490 | – | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge ExtendFees | 35,860 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ImportFees | 63,420 | 1 | 2,923 | 2,285 | 564 | 5,995 | 602 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | Attest | 133,721 | 1 | 2,844 | 2,373 | 333 | 3,424 | – | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ExtendFees | 35,860 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge ImportFees | 64,816 | 1 | 3,128 | 2,490 | 564 | 6,405 | 602 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | Attest | 140,775 | 1 | 3,049 | 2,578 | 333 | 3,629 | – | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge ExtendFees | 35,860 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ImportFees | 63,280 | 1 | 3,057 | 2,419 | 564 | 6,263 | 602 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | Attest | 136,967 | 1 | 2,978 | 2,507 | 333 | 3,558 | – | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ExtendFees | 34,737 | 1 | 971 | 70 | 763 | 1,759 | 763 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge ImportRoot | 64,301 | 1 | 3,239 | 2,601 | 564 | 6,631 | 613 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | Attest | 146,833 | 1 | 3,153 | 2,682 | 333 | 3,733 | – | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Issue | 51,535 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Spend | 37,291 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 23,965 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Issue | 45,905 | 1 | 1,100 | 199 | 763 | 2,356 | 763 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Mint | 24,212 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Issue | 51,586 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Spend | 37,291 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 22,496 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 23,965 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
