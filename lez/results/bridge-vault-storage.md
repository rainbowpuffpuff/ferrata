# The bridge program on FerrataVault's own storage (`harness/tests/bridge_vault_storage.rs`)

Bridge program image `d0c11833f2944d1d928bb4a40ffe9300591561dba0c7ca20f1c66265258b23cd`. The blocks P and Q hold exactly the storage FerrataVault v5 wrote in `vault/test/RegistryStorage.t.sol` (`vault/test/vectors/registry-storage.json`: 139 and 149 slots, 3 and 6 leaves); `Init` under a synthetic block (the pairing is `bridge_inbound.rs`'s subject). Every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init (pairing under a synthetic block: slot 2 = configId) | bridge Init 41,721 → Attest 74,387 → bridge (reply) 39,242 | 155,350 | 3% | 1,731 | 1,256,648 | applied |
| 2 | Register ETH with lezUnit ×10 (not the vault's entry) | bridge Register 104,787 → Attest 219,872 → bridge (reply) 49,908 | 374,567 | 7% | 4,974 | 3,036,328 | refused: exit 247 (IMPORT_MISMATCH) |
| 3 | Register ETH with schedule 0 a second later | bridge Register 104,595 → Attest 219,872 → bridge (reply) 52,637 | 377,104 | 8% | 4,974 | 3,056,624 | refused: exit 247 (IMPORT_MISMATCH) |
| 4 | Register ETH: the vault's entry and schedule 0 at P | bridge Register 104,595 → Attest 219,872 → bridge (reply) 73,478 → bridge WriteToken 31,613 | 429,558 | 9% | 4,974 | 3,476,256 | applied |
| 5 | Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge Register 98,540 → Attest 225,935 → bridge (reply) 73,478 → bridge WriteToken 31,613 | 429,566 | 9% | 4,999 | 3,476,520 | applied |
| 6 | Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge Register 99,708 → Attest 222,977 → bridge (reply) 73,478 → bridge WriteToken 31,613 | 427,776 | 9% | 4,805 | 3,460,648 | applied |
| 7 | Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge Register 101,584 → Attest 244,382 → bridge (reply) 73,478 → bridge WriteToken 31,613 | 451,057 | 9% | 5,264 | 3,650,568 | applied |
| 8 | Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge Register 95,944 → Attest 229,262 → bridge (reply) 71,686 → bridge WriteToken 30,440 | 427,332 | 9% | 5,084 | 3,459,328 | applied |
| 9 | ImportRoot: the vault's slots 0 and 1 at P | bridge ImportRoot 62,187 → Attest 146,307 → bridge (reply) 52,437 → bridge WriteRoot 16,615 | 277,546 | 6% | 3,095 | 2,245,128 | applied |
| 10 | Claim leaf 0 (ETH, under root(n = 3)) | bridge Claim 107,610 → bridge Issue 48,364 → bridge Admit 33,983 → bridge Spend 22,171 → bridge Mint 14,616 → bridge Mint 14,616 | 241,360 | 5% | 2,053 | 1,947,304 | applied |
| 11 | Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Claim 107,610 → bridge Issue 47,052 → bridge Admit 33,983 → bridge Spend 22,171 → bridge Mint 14,616 → bridge Mint 20,070 | 245,502 | 5% | 2,053 | 1,980,440 | applied |
| 12 | Claim leaf 2 (ETH, under root(n = 3)) | bridge Claim 107,610 → bridge Issue 48,329 → bridge Admit 33,983 → bridge Spend 22,171 → bridge Mint 14,616 → bridge Mint 20,909 | 247,618 | 5% | 2,053 | 1,997,368 | applied |
| 13 | ImportFees ETH schedule 1 with another mint fee | bridge ImportFees 66,794 → Attest 144,300 → bridge (reply) 42,247 | 253,341 | 5% | 3,179 | 2,052,160 | refused: exit 247 (IMPORT_MISMATCH) |
| 14 | ImportFees ETH: the vault's schedule 1 at Q | bridge ImportFees 66,794 → Attest 144,300 → bridge (reply) 57,003 → bridge ExtendFees 41,260 | 309,357 | 6% | 3,179 | 2,500,288 | applied |
| 15 | ImportFees ETH: no schedule 2 at Q | bridge ImportFees 60,446 → Attest 129,482 → bridge (reply) 51,342 → bridge ExtendFees 39,216 | 280,486 | 6% | 2,768 | 2,266,032 | applied |
| 16 | ImportFees token 0xf62849…: no schedule 1 at Q | bridge ImportFees 60,989 → Attest 134,749 → bridge (reply) 51,342 → bridge ExtendFees 35,649 | 282,729 | 6% | 2,877 | 2,284,848 | applied |
| 17 | ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ImportFees 61,914 → Attest 133,721 → bridge (reply) 51,342 → bridge ExtendFees 35,649 | 282,626 | 6% | 2,811 | 2,283,496 | applied |
| 18 | ImportFees token 0xc71834…: no schedule 1 at Q | bridge ImportFees 63,231 → Attest 140,775 → bridge (reply) 51,342 → bridge ExtendFees 35,649 | 290,997 | 6% | 3,016 | 2,352,104 | applied |
| 19 | ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ImportFees 61,763 → Attest 136,967 → bridge (reply) 51,342 → bridge ExtendFees 34,526 | 284,598 | 6% | 2,945 | 2,300,344 | applied |
| 20 | ImportRoot: the vault's slots 0 and 1 at Q | bridge ImportRoot 62,497 → Attest 146,833 → bridge (reply) 52,437 → bridge WriteRoot 16,615 | 278,382 | 6% | 3,127 | 2,252,072 | applied |
| 21 | Claim leaf 3 (ETH, under root(n = 6)) | bridge Claim 107,610 → bridge Issue 52,697 → bridge Admit 33,983 → bridge Spend 24,414 → bridge Mint 14,616 → bridge Mint 14,616 → bridge Mint 20,909 | 268,845 | 5% | 2,053 | 2,167,184 | applied |
| 22 | Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Claim 107,610 → bridge Issue 47,068 → bridge Admit 33,983 → bridge Spend 22,171 → bridge Mint 14,616 → bridge Mint 21,156 | 246,604 | 5% | 2,053 | 1,989,256 | applied |
| 23 | Claim leaf 5 (ETH, under root(n = 6)) | bridge Claim 107,610 → bridge Issue 52,748 → bridge Admit 33,983 → bridge Spend 24,414 → bridge Mint 14,616 → bridge Mint 19,553 → bridge Mint 20,909 | 273,833 | 5% | 2,053 | 2,207,088 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init (pairing under a synthetic block: slot 2 = configId) | bridge Init | 41,721 | 1 | 1,278 | 1,204 | 0 | 2,706 | 390 | 0 |
| Init (pairing under a synthetic block: slot 2 = configId) | Attest | 74,387 | 1 | 1,412 | 941 | 333 | 1,875 | – | 0 |
| Init (pairing under a synthetic block: slot 2 = configId) | bridge (reply) | 39,242 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | bridge Register | 104,787 | 1 | 5,042 | 4,448 | 520 | 10,305 | 906 | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | bridge (reply) | 49,908 | 1 | 1,765 | 721 | 906 | 0 | – | 247 |
| Register ETH with schedule 0 a second later | bridge Register | 104,595 | 1 | 5,042 | 4,448 | 520 | 10,305 | 906 | 0 |
| Register ETH with schedule 0 a second later | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH with schedule 0 a second later | bridge (reply) | 52,637 | 1 | 1,765 | 721 | 906 | 0 | – | 247 |
| Register ETH: the vault's entry and schedule 0 at P | bridge Register | 104,595 | 1 | 5,042 | 4,448 | 520 | 10,305 | 906 | 0 |
| Register ETH: the vault's entry and schedule 0 at P | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH: the vault's entry and schedule 0 at P | bridge (reply) | 73,478 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH: the vault's entry and schedule 0 at P | bridge WriteToken | 31,613 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge Register | 98,540 | 1 | 5,067 | 4,473 | 520 | 10,355 | 906 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | Attest | 225,935 | 1 | 4,756 | 4,285 | 333 | 5,570 | – | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge (reply) | 73,478 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge WriteToken | 31,613 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge Register | 99,708 | 1 | 4,873 | 4,279 | 520 | 9,967 | 906 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | Attest | 222,977 | 1 | 4,562 | 4,091 | 333 | 5,376 | – | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge (reply) | 73,478 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge WriteToken | 31,613 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge Register | 101,584 | 1 | 5,332 | 4,738 | 520 | 10,885 | 906 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | Attest | 244,382 | 1 | 5,021 | 4,550 | 333 | 5,835 | – | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge (reply) | 73,478 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge WriteToken | 31,613 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge Register | 95,944 | 1 | 5,152 | 4,558 | 520 | 10,525 | 826 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | Attest | 229,262 | 1 | 4,921 | 4,450 | 333 | 5,735 | – | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge (reply) | 71,686 | 1 | 1,685 | 721 | 826 | 3,097 | 520 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge WriteToken | 30,440 | 1 | 901 | 763 | 0 | 1,689 | 763 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge ImportRoot | 62,187 | 1 | 3,163 | 2,569 | 520 | 6,479 | 569 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | Attest | 146,307 | 1 | 3,121 | 2,650 | 333 | 3,701 | – | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge (reply) | 52,437 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge WriteRoot | 16,615 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Issue | 48,364 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Spend | 22,171 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Issue | 47,052 | 1 | 1,100 | 199 | 763 | 2,356 | 763 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Spend | 22,171 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Mint | 20,070 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Issue | 48,329 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Spend | 22,171 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Mint | 20,909 | 1 | 409 | 129 | 142 | 576 | 142 | 0 |
| ImportFees ETH schedule 1 with another mint fee | bridge ImportFees | 66,794 | 1 | 3,247 | 2,653 | 520 | 6,643 | 746 | 0 |
| ImportFees ETH schedule 1 with another mint fee | Attest | 144,300 | 1 | 3,024 | 2,553 | 333 | 3,604 | – | 0 |
| ImportFees ETH schedule 1 with another mint fee | bridge (reply) | 42,247 | 1 | 1,371 | 487 | 746 | 0 | – | 247 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge ImportFees | 66,794 | 1 | 3,247 | 2,653 | 520 | 6,643 | 746 | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | Attest | 144,300 | 1 | 3,024 | 2,553 | 333 | 3,604 | – | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge (reply) | 57,003 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge ExtendFees | 41,260 | 1 | 1,239 | 258 | 843 | 2,295 | 1,031 | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge ImportFees | 60,446 | 1 | 2,836 | 2,242 | 520 | 5,821 | 558 | 0 |
| ImportFees ETH: no schedule 2 at Q | Attest | 129,482 | 1 | 2,801 | 2,330 | 333 | 3,381 | – | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge (reply) | 51,342 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge ExtendFees | 39,216 | 1 | 1,239 | 70 | 1,031 | 2,295 | 1,031 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge ImportFees | 60,989 | 1 | 2,945 | 2,351 | 520 | 6,039 | 558 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | Attest | 134,749 | 1 | 2,910 | 2,439 | 333 | 3,490 | – | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge (reply) | 51,342 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge ExtendFees | 35,649 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ImportFees | 61,914 | 1 | 2,879 | 2,285 | 520 | 5,907 | 558 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | Attest | 133,721 | 1 | 2,844 | 2,373 | 333 | 3,424 | – | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge (reply) | 51,342 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ExtendFees | 35,649 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge ImportFees | 63,231 | 1 | 3,084 | 2,490 | 520 | 6,317 | 558 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | Attest | 140,775 | 1 | 3,049 | 2,578 | 333 | 3,629 | – | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge (reply) | 51,342 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge ExtendFees | 35,649 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ImportFees | 61,763 | 1 | 3,013 | 2,419 | 520 | 6,175 | 558 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | Attest | 136,967 | 1 | 2,978 | 2,507 | 333 | 3,558 | – | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge (reply) | 51,342 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ExtendFees | 34,526 | 1 | 971 | 70 | 763 | 1,759 | 763 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge ImportRoot | 62,497 | 1 | 3,195 | 2,601 | 520 | 6,543 | 569 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | Attest | 146,833 | 1 | 3,153 | 2,682 | 333 | 3,733 | – | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge (reply) | 52,437 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge WriteRoot | 16,615 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Issue | 52,697 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Spend | 24,414 | 1 | 408 | 270 | 0 | 1,113 | 77 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 20,909 | 1 | 409 | 129 | 142 | 576 | 142 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Issue | 47,068 | 1 | 1,100 | 199 | 763 | 2,356 | 763 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Spend | 22,171 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Mint | 21,156 | 1 | 409 | 129 | 142 | 576 | 142 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Issue | 52,748 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Spend | 24,414 | 1 | 408 | 270 | 0 | 1,113 | 77 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 19,553 | 1 | 341 | 129 | 74 | 440 | 74 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 20,909 | 1 | 409 | 129 | 142 | 576 | 142 | 0 |
