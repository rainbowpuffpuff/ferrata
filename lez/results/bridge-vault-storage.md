# The bridge program on FerrataVault's own storage (`harness/tests/bridge_vault_storage.rs`)

Bridge program image `fe2f248724ddbeada455f2f132ae178d84434db3f26ddce50e06d72cc7ae946d`. The blocks P and Q hold exactly the storage FerrataVault v5 wrote in `vault/test/RegistryStorage.t.sol` (`vault/test/vectors/registry-storage.json`: 139 and 149 slots, 3 and 6 leaves); `Init` under a synthetic block (the pairing is `bridge_inbound.rs`'s subject). Every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init (pairing under a synthetic block: slot 2 = configId) | bridge Init 41,200 → Attest 74,387 → bridge (reply) 39,116 | 154,703 | 3% | 1,731 | 1,251,472 | applied |
| 2 | Register ETH with lezUnit ×10 (not the vault's entry) | bridge Register 104,280 → Attest 219,872 → bridge (reply) 49,719 | 373,871 | 7% | 4,974 | 3,030,760 | refused: exit 247 (IMPORT_MISMATCH) |
| 3 | Register ETH with schedule 0 a second later | bridge Register 104,088 → Attest 219,872 → bridge (reply) 52,448 | 376,408 | 8% | 4,974 | 3,051,056 | refused: exit 247 (IMPORT_MISMATCH) |
| 4 | Register ETH: the vault's entry and schedule 0 at P | bridge Register 104,088 → Attest 219,872 → bridge (reply) 73,183 → bridge WriteToken 28,912 | 426,055 | 9% | 4,974 | 3,448,232 | applied |
| 5 | Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge Register 98,033 → Attest 225,935 → bridge (reply) 73,183 → bridge WriteToken 28,912 | 426,063 | 9% | 4,999 | 3,448,496 | applied |
| 6 | Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge Register 99,201 → Attest 222,977 → bridge (reply) 73,183 → bridge WriteToken 28,912 | 424,273 | 8% | 4,805 | 3,432,624 | applied |
| 7 | Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge Register 101,077 → Attest 244,382 → bridge (reply) 73,183 → bridge WriteToken 28,912 | 447,554 | 9% | 5,264 | 3,622,544 | applied |
| 8 | Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge Register 95,437 → Attest 229,262 → bridge (reply) 71,389 → bridge WriteToken 27,924 | 424,012 | 8% | 5,084 | 3,432,768 | applied |
| 9 | ImportRoot: the vault's slots 0 and 1 at P | bridge ImportRoot 61,877 → Attest 146,307 → bridge (reply) 52,749 → bridge WriteRoot 16,225 | 277,158 | 6% | 3,095 | 2,242,024 | applied |
| 10 | Claim leaf 0 (ETH, under root(n = 3)) | bridge Claim 107,397 → bridge Issue 44,075 → bridge Admit 34,187 → bridge Spend 30,034 → bridge Mint 17,155 → bridge Mint 17,155 | 250,003 | 5% | 2,061 | 2,016,512 | applied |
| 11 | Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Claim 107,397 → bridge Issue 42,875 → bridge Admit 33,899 → bridge Spend 30,034 → bridge Mint 17,155 → bridge Mint 22,403 | 253,763 | 5% | 2,061 | 2,046,592 | applied |
| 12 | Claim leaf 2 (ETH, under root(n = 3)) | bridge Claim 107,397 → bridge Issue 43,752 → bridge Admit 33,899 → bridge Spend 30,034 → bridge Mint 17,155 → bridge Mint 23,359 | 255,596 | 5% | 2,061 | 2,061,256 | applied |
| 13 | ImportFees ETH schedule 1 with another mint fee | bridge ImportFees 66,398 → Attest 144,300 → bridge (reply) 42,048 | 252,746 | 5% | 3,179 | 2,047,400 | refused: exit 247 (IMPORT_MISMATCH) |
| 14 | ImportFees ETH: the vault's schedule 1 at Q | bridge ImportFees 66,398 → Attest 144,300 → bridge (reply) 57,255 → bridge ExtendFees 37,766 | 305,719 | 6% | 3,179 | 2,471,184 | applied |
| 15 | ImportFees ETH: no schedule 2 at Q | bridge ImportFees 60,050 → Attest 129,482 → bridge (reply) 51,594 → bridge ExtendFees 36,021 | 277,147 | 6% | 2,768 | 2,239,320 | applied |
| 16 | ImportFees token 0xf62849…: no schedule 1 at Q | bridge ImportFees 60,593 → Attest 134,749 → bridge (reply) 51,594 → bridge ExtendFees 33,181 | 280,117 | 6% | 2,877 | 2,263,952 | applied |
| 17 | ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ImportFees 61,518 → Attest 133,721 → bridge (reply) 51,594 → bridge ExtendFees 33,181 | 280,014 | 6% | 2,811 | 2,262,600 | applied |
| 18 | ImportFees token 0xc71834…: no schedule 1 at Q | bridge ImportFees 62,835 → Attest 140,775 → bridge (reply) 51,594 → bridge ExtendFees 33,181 | 288,385 | 6% | 3,016 | 2,331,208 | applied |
| 19 | ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ImportFees 61,367 → Attest 136,967 → bridge (reply) 51,594 → bridge ExtendFees 32,014 | 281,942 | 6% | 2,945 | 2,279,096 | applied |
| 20 | ImportRoot: the vault's slots 0 and 1 at Q | bridge ImportRoot 62,187 → Attest 146,833 → bridge (reply) 52,749 → bridge WriteRoot 16,225 | 277,994 | 6% | 3,127 | 2,248,968 | applied |
| 21 | Claim leaf 3 (ETH, under root(n = 6)) | bridge Claim 107,397 → bridge Issue 46,973 → bridge Admit 33,899 → bridge Spend 36,393 → bridge Mint 17,155 → bridge Mint 17,155 → bridge Mint 23,359 | 282,331 | 6% | 2,061 | 2,275,136 | applied |
| 22 | Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Claim 107,397 → bridge Issue 42,603 → bridge Admit 33,899 → bridge Spend 30,034 → bridge Mint 17,155 → bridge Mint 23,606 | 254,694 | 5% | 2,061 | 2,054,040 | applied |
| 23 | Claim leaf 5 (ETH, under root(n = 6)) | bridge Claim 107,397 → bridge Issue 47,024 → bridge Admit 33,899 → bridge Spend 36,393 → bridge Mint 17,155 → bridge Mint 21,882 → bridge Mint 23,359 | 287,109 | 6% | 2,061 | 2,313,360 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init (pairing under a synthetic block: slot 2 = configId) | bridge Init | 41,200 | 1 | 1,278 | 1,204 | 0 | 2,706 | 390 | 0 |
| Init (pairing under a synthetic block: slot 2 = configId) | Attest | 74,387 | 1 | 1,412 | 941 | 333 | 1,875 | – | 0 |
| Init (pairing under a synthetic block: slot 2 = configId) | bridge (reply) | 39,116 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | bridge Register | 104,280 | 1 | 5,042 | 4,448 | 520 | 10,305 | 906 | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | bridge (reply) | 49,719 | 1 | 1,765 | 721 | 906 | 0 | – | 247 |
| Register ETH with schedule 0 a second later | bridge Register | 104,088 | 1 | 5,042 | 4,448 | 520 | 10,305 | 906 | 0 |
| Register ETH with schedule 0 a second later | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH with schedule 0 a second later | bridge (reply) | 52,448 | 1 | 1,765 | 721 | 906 | 0 | – | 247 |
| Register ETH: the vault's entry and schedule 0 at P | bridge Register | 104,088 | 1 | 5,042 | 4,448 | 520 | 10,305 | 906 | 0 |
| Register ETH: the vault's entry and schedule 0 at P | Attest | 219,872 | 1 | 4,731 | 4,260 | 333 | 5,545 | – | 0 |
| Register ETH: the vault's entry and schedule 0 at P | bridge (reply) | 73,183 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH: the vault's entry and schedule 0 at P | bridge WriteToken | 28,912 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge Register | 98,033 | 1 | 5,067 | 4,473 | 520 | 10,355 | 906 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | Attest | 225,935 | 1 | 4,756 | 4,285 | 333 | 5,570 | – | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge (reply) | 73,183 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge WriteToken | 28,912 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge Register | 99,201 | 1 | 4,873 | 4,279 | 520 | 9,967 | 906 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | Attest | 222,977 | 1 | 4,562 | 4,091 | 333 | 5,376 | – | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge (reply) | 73,183 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge WriteToken | 28,912 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge Register | 101,077 | 1 | 5,332 | 4,738 | 520 | 10,885 | 906 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | Attest | 244,382 | 1 | 5,021 | 4,550 | 333 | 5,835 | – | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge (reply) | 73,183 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge WriteToken | 28,912 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge Register | 95,437 | 1 | 5,152 | 4,558 | 520 | 10,525 | 826 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | Attest | 229,262 | 1 | 4,921 | 4,450 | 333 | 5,735 | – | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge (reply) | 71,389 | 1 | 1,685 | 721 | 826 | 3,032 | 520 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge WriteToken | 27,924 | 1 | 836 | 698 | 0 | 1,559 | 698 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge ImportRoot | 61,877 | 1 | 3,163 | 2,569 | 520 | 6,479 | 569 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | Attest | 146,307 | 1 | 3,121 | 2,650 | 333 | 3,701 | – | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge (reply) | 52,749 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge WriteRoot | 16,225 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Issue | 44,075 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Admit | 34,187 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Spend | 30,034 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Issue | 42,875 | 1 | 1,043 | 207 | 698 | 2,259 | 698 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Admit | 33,899 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Spend | 30,034 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Mint | 22,403 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Issue | 43,752 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Admit | 33,899 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Spend | 30,034 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Mint | 23,359 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| ImportFees ETH schedule 1 with another mint fee | bridge ImportFees | 66,398 | 1 | 3,247 | 2,653 | 520 | 6,643 | 746 | 0 |
| ImportFees ETH schedule 1 with another mint fee | Attest | 144,300 | 1 | 3,024 | 2,553 | 333 | 3,604 | – | 0 |
| ImportFees ETH schedule 1 with another mint fee | bridge (reply) | 42,048 | 1 | 1,371 | 487 | 746 | 0 | – | 247 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge ImportFees | 66,398 | 1 | 3,247 | 2,653 | 520 | 6,643 | 746 | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | Attest | 144,300 | 1 | 3,024 | 2,553 | 333 | 3,604 | – | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge (reply) | 57,255 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge ExtendFees | 37,766 | 1 | 1,174 | 258 | 778 | 2,165 | 966 | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge ImportFees | 60,050 | 1 | 2,836 | 2,242 | 520 | 5,821 | 558 | 0 |
| ImportFees ETH: no schedule 2 at Q | Attest | 129,482 | 1 | 2,801 | 2,330 | 333 | 3,381 | – | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge (reply) | 51,594 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge ExtendFees | 36,021 | 1 | 1,174 | 70 | 966 | 2,165 | 966 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge ImportFees | 60,593 | 1 | 2,945 | 2,351 | 520 | 6,039 | 558 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | Attest | 134,749 | 1 | 2,910 | 2,439 | 333 | 3,490 | – | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge (reply) | 51,594 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge ExtendFees | 33,181 | 1 | 986 | 70 | 778 | 1,789 | 778 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ImportFees | 61,518 | 1 | 2,879 | 2,285 | 520 | 5,907 | 558 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | Attest | 133,721 | 1 | 2,844 | 2,373 | 333 | 3,424 | – | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge (reply) | 51,594 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ExtendFees | 33,181 | 1 | 986 | 70 | 778 | 1,789 | 778 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge ImportFees | 62,835 | 1 | 3,084 | 2,490 | 520 | 6,317 | 558 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | Attest | 140,775 | 1 | 3,049 | 2,578 | 333 | 3,629 | – | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge (reply) | 51,594 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge ExtendFees | 33,181 | 1 | 986 | 70 | 778 | 1,789 | 778 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ImportFees | 61,367 | 1 | 3,013 | 2,419 | 520 | 6,175 | 558 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | Attest | 136,967 | 1 | 2,978 | 2,507 | 333 | 3,558 | – | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge (reply) | 51,594 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ExtendFees | 32,014 | 1 | 906 | 70 | 698 | 1,629 | 698 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge ImportRoot | 62,187 | 1 | 3,195 | 2,601 | 520 | 6,543 | 569 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | Attest | 146,833 | 1 | 3,153 | 2,682 | 333 | 3,733 | – | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge (reply) | 52,749 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge WriteRoot | 16,225 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Issue | 46,973 | 1 | 1,311 | 207 | 966 | 2,843 | 966 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Admit | 33,899 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Spend | 36,393 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 23,359 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Issue | 42,603 | 1 | 1,043 | 207 | 698 | 2,259 | 698 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Admit | 33,899 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Spend | 30,034 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Mint | 23,606 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Issue | 47,024 | 1 | 1,311 | 207 | 966 | 2,843 | 966 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Admit | 33,899 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Spend | 36,393 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 21,882 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 23,359 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
