# The bridge program on FerrataVault's own storage (`harness/tests/bridge_vault_storage.rs`)

Bridge program image `ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`. The blocks P and Q hold exactly the storage FerrataVault v5 wrote in `vault/test/RegistryStorage.t.sol` (`vault/test/vectors/registry-storage.json`: 139 and 149 slots, 3 and 6 leaves); `Init` under a synthetic block (the pairing is `bridge_inbound.rs`'s subject). Every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init (pairing under a synthetic block: slot 2 = configId) | bridge Init 42,560 → Attest 75,407 → bridge (reply) 39,738 | 157,705 | 3% | 1,731 | 1,275,488 | applied |
| 2 | Register ETH with lezUnit ×10 (not the vault's entry) | bridge Register 106,804 → Attest 220,937 → bridge (reply) 50,989 | 378,730 | 8% | 4,974 | 3,069,632 | refused: exit 247 (IMPORT_MISMATCH) |
| 3 | Register ETH with schedule 0 a second later | bridge Register 106,612 → Attest 220,937 → bridge (reply) 53,718 | 381,267 | 8% | 4,974 | 3,089,928 | refused: exit 247 (IMPORT_MISMATCH) |
| 4 | Register ETH: the vault's entry and schedule 0 at P | bridge Register 106,612 → Attest 220,937 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 435,255 | 9% | 4,974 | 3,521,832 | applied |
| 5 | Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge Register 100,426 → Attest 226,956 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 435,088 | 9% | 4,999 | 3,520,696 | applied |
| 6 | Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge Register 101,604 → Attest 223,998 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 433,308 | 9% | 4,805 | 3,504,904 | applied |
| 7 | Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge Register 103,620 → Attest 245,403 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 456,729 | 9% | 5,264 | 3,695,944 | applied |
| 8 | Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge Register 97,842 → Attest 230,329 → bridge (reply) 73,544 → bridge WriteToken 31,305 | 433,020 | 9% | 5,084 | 3,504,832 | applied |
| 9 | ImportRoot: the vault's slots 0 and 1 at P | bridge ImportRoot 64,429 → Attest 147,379 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 283,051 | 6% | 3,095 | 2,289,168 | applied |
| 10 | Claim leaf 0 (ETH, under root(n = 3)) | bridge Claim 107,796 → bridge Issue 48,553 → bridge Admit 35,962 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 18,800 | 261,680 | 5% | 2,053 | 2,109,864 | applied |
| 11 | Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Claim 107,796 → bridge Issue 47,292 → bridge Admit 35,962 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 23,698 | 265,317 | 5% | 2,053 | 2,138,960 | applied |
| 12 | Claim leaf 2 (ETH, under root(n = 3)) | bridge Claim 107,796 → bridge Issue 48,518 → bridge Admit 35,962 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 24,646 | 267,491 | 5% | 2,053 | 2,156,352 | applied |
| 13 | ImportFees ETH schedule 1 with another mint fee | bridge ImportFees 68,708 → Attest 145,314 → bridge (reply) 43,250 | 257,272 | 5% | 3,179 | 2,083,608 | refused: exit 247 (IMPORT_MISMATCH) |
| 14 | ImportFees ETH: the vault's schedule 1 at Q | bridge ImportFees 68,708 → Attest 145,314 → bridge (reply) 58,627 → bridge ExtendFees 42,626 | 315,275 | 6% | 3,179 | 2,547,632 | applied |
| 15 | ImportFees ETH: no schedule 2 at Q | bridge ImportFees 62,248 → Attest 130,554 → bridge (reply) 52,960 → bridge ExtendFees 40,597 | 286,359 | 6% | 2,768 | 2,313,016 | applied |
| 16 | ImportFees token 0xf62849…: no schedule 1 at Q | bridge ImportFees 62,941 → Attest 135,814 → bridge (reply) 52,960 → bridge ExtendFees 36,697 | 288,412 | 6% | 2,877 | 2,330,312 | applied |
| 17 | ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ImportFees 63,823 → Attest 134,786 → bridge (reply) 52,960 → bridge ExtendFees 36,697 | 288,266 | 6% | 2,811 | 2,328,616 | applied |
| 18 | ImportFees token 0xc71834…: no schedule 1 at Q | bridge ImportFees 65,187 → Attest 141,870 → bridge (reply) 52,960 → bridge ExtendFees 36,697 | 296,714 | 6% | 3,016 | 2,397,840 | applied |
| 19 | ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ImportFees 63,745 → Attest 138,032 → bridge (reply) 52,960 → bridge ExtendFees 35,612 | 290,349 | 6% | 2,945 | 2,346,352 | applied |
| 20 | ImportRoot: the vault's slots 0 and 1 at Q | bridge ImportRoot 64,739 → Attest 147,847 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 283,829 | 6% | 3,127 | 2,295,648 | applied |
| 21 | Claim leaf 3 (ETH, under root(n = 6)) | bridge Claim 107,796 → bridge Issue 52,765 → bridge Admit 35,962 → bridge Spend 38,287 → bridge Mint 18,800 → bridge Mint 18,800 → bridge Mint 24,646 | 297,056 | 6% | 2,053 | 2,392,872 | applied |
| 22 | Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Claim 107,796 → bridge Issue 47,308 → bridge Admit 35,962 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 24,893 | 266,528 | 5% | 2,053 | 2,148,648 | applied |
| 23 | Claim leaf 5 (ETH, under root(n = 6)) | bridge Claim 107,796 → bridge Issue 52,816 → bridge Admit 35,962 → bridge Spend 38,287 → bridge Mint 18,800 → bridge Mint 23,177 → bridge Mint 24,646 | 301,484 | 6% | 2,053 | 2,428,296 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init (pairing under a synthetic block: slot 2 = configId) | bridge Init | 42,560 | 1 | 1,278 | 1,204 | 0 | 2,706 | 390 | 0 |
| Init (pairing under a synthetic block: slot 2 = configId) | Attest | 75,407 | 1 | 1,444 | 941 | 365 | 1,907 | – | 0 |
| Init (pairing under a synthetic block: slot 2 = configId) | bridge (reply) | 39,738 | 1 | 898 | 370 | 390 | 1,503 | 580 | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | bridge Register | 106,804 | 1 | 5,102 | 4,448 | 580 | 10,425 | 966 | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | Attest | 220,937 | 1 | 4,763 | 4,260 | 365 | 5,577 | – | 0 |
| Register ETH with lezUnit ×10 (not the vault's entry) | bridge (reply) | 50,989 | 1 | 1,825 | 721 | 966 | 0 | – | 247 |
| Register ETH with schedule 0 a second later | bridge Register | 106,612 | 1 | 5,102 | 4,448 | 580 | 10,425 | 966 | 0 |
| Register ETH with schedule 0 a second later | Attest | 220,937 | 1 | 4,763 | 4,260 | 365 | 5,577 | – | 0 |
| Register ETH with schedule 0 a second later | bridge (reply) | 53,718 | 1 | 1,825 | 721 | 966 | 0 | – | 247 |
| Register ETH: the vault's entry and schedule 0 at P | bridge Register | 106,612 | 1 | 5,102 | 4,448 | 580 | 10,425 | 966 | 0 |
| Register ETH: the vault's entry and schedule 0 at P | Attest | 220,937 | 1 | 4,763 | 4,260 | 365 | 5,577 | – | 0 |
| Register ETH: the vault's entry and schedule 0 at P | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register ETH: the vault's entry and schedule 0 at P | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge Register | 100,426 | 1 | 5,127 | 4,473 | 580 | 10,475 | 966 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | Attest | 226,956 | 1 | 4,788 | 4,285 | 365 | 5,602 | – | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register token 0xf62849…: the vault's entry and schedule 0 at P | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge Register | 101,604 | 1 | 4,933 | 4,279 | 580 | 10,087 | 966 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | Attest | 223,998 | 1 | 4,594 | 4,091 | 365 | 5,408 | – | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register token 0x5991a2…: the vault's entry and schedule 0 at P | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge Register | 103,620 | 1 | 5,392 | 4,738 | 580 | 11,005 | 966 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | Attest | 245,403 | 1 | 5,053 | 4,550 | 365 | 5,867 | – | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register token 0xc71834…: the vault's entry and schedule 0 at P | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge Register | 97,842 | 1 | 5,212 | 4,558 | 580 | 10,645 | 886 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | Attest | 230,329 | 1 | 4,953 | 4,450 | 365 | 5,767 | – | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge (reply) | 73,544 | 1 | 1,745 | 721 | 886 | 3,226 | 580 | 0 |
| Register token 0x1d1499…: the vault's entry and schedule 0 at P | bridge WriteToken | 31,305 | 1 | 910 | 772 | 0 | 1,707 | 772 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge ImportRoot | 64,429 | 1 | 3,223 | 2,569 | 580 | 6,599 | 629 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | Attest | 147,379 | 1 | 3,153 | 2,650 | 365 | 3,733 | – | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot: the vault's slots 0 and 1 at P | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Issue | 48,553 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 0 (ETH, under root(n = 3)) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Issue | 47,292 | 1 | 1,109 | 199 | 772 | 2,374 | 772 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 1 (token 0x1d1499…, under root(n = 3)) | bridge Mint | 23,698 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Issue | 48,518 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 (ETH, under root(n = 3)) | bridge Mint | 24,646 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| ImportFees ETH schedule 1 with another mint fee | bridge ImportFees | 68,708 | 1 | 3,307 | 2,653 | 580 | 6,763 | 806 | 0 |
| ImportFees ETH schedule 1 with another mint fee | Attest | 145,314 | 1 | 3,056 | 2,553 | 365 | 3,636 | – | 0 |
| ImportFees ETH schedule 1 with another mint fee | bridge (reply) | 43,250 | 1 | 1,431 | 487 | 806 | 0 | – | 247 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge ImportFees | 68,708 | 1 | 3,307 | 2,653 | 580 | 6,763 | 806 | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | Attest | 145,314 | 1 | 3,056 | 2,553 | 365 | 3,636 | – | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge (reply) | 58,627 | 1 | 1,431 | 487 | 806 | 2,398 | 580 | 0 |
| ImportFees ETH: the vault's schedule 1 at Q | bridge ExtendFees | 42,626 | 1 | 1,248 | 258 | 852 | 2,313 | 1,040 | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge ImportFees | 62,248 | 1 | 2,896 | 2,242 | 580 | 5,941 | 618 | 0 |
| ImportFees ETH: no schedule 2 at Q | Attest | 130,554 | 1 | 2,833 | 2,330 | 365 | 3,413 | – | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees ETH: no schedule 2 at Q | bridge ExtendFees | 40,597 | 1 | 1,248 | 70 | 1,040 | 2,313 | 1,040 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge ImportFees | 62,941 | 1 | 3,005 | 2,351 | 580 | 6,159 | 618 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | Attest | 135,814 | 1 | 2,942 | 2,439 | 365 | 3,522 | – | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees token 0xf62849…: no schedule 1 at Q | bridge ExtendFees | 36,697 | 1 | 1,060 | 70 | 852 | 1,937 | 852 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ImportFees | 63,823 | 1 | 2,939 | 2,285 | 580 | 6,027 | 618 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | Attest | 134,786 | 1 | 2,876 | 2,373 | 365 | 3,456 | – | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees token 0x5991a2…: no schedule 1 at Q | bridge ExtendFees | 36,697 | 1 | 1,060 | 70 | 852 | 1,937 | 852 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge ImportFees | 65,187 | 1 | 3,144 | 2,490 | 580 | 6,437 | 618 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | Attest | 141,870 | 1 | 3,081 | 2,578 | 365 | 3,661 | – | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees token 0xc71834…: no schedule 1 at Q | bridge ExtendFees | 36,697 | 1 | 1,060 | 70 | 852 | 1,937 | 852 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ImportFees | 63,745 | 1 | 3,073 | 2,419 | 580 | 6,295 | 618 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | Attest | 138,032 | 1 | 3,010 | 2,507 | 365 | 3,590 | – | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees token 0x1d1499…: no schedule 1 at Q | bridge ExtendFees | 35,612 | 1 | 980 | 70 | 772 | 1,777 | 772 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge ImportRoot | 64,739 | 1 | 3,255 | 2,601 | 580 | 6,663 | 629 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | Attest | 147,847 | 1 | 3,185 | 2,682 | 365 | 3,765 | – | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot: the vault's slots 0 and 1 at Q | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Issue | 52,765 | 1 | 1,377 | 199 | 1,040 | 2,958 | 1,040 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Spend | 38,287 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 3 (ETH, under root(n = 6)) | bridge Mint | 24,646 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Issue | 47,308 | 1 | 1,109 | 199 | 772 | 2,374 | 772 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 4 (token 0x1d1499…, under root(n = 6)) | bridge Mint | 24,893 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Issue | 52,816 | 1 | 1,377 | 199 | 1,040 | 2,958 | 1,040 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Spend | 38,287 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 23,177 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim leaf 5 (ETH, under root(n = 6)) | bridge Mint | 24,646 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
