# second build (7c7d2b3a…1e2f) against deployed program (ccde6171…71dd), table by table

Written by `lez/scripts/compare-tables.py`: every cell of second build (7c7d2b3a…1e2f)'s tables that differs from
deployed program (ccde6171…71dd)'s, rows paired in order.

| file | tables | rows | rows equal | rows that differ | text lines that differ |
|---|---:|---:|---:|---:|---:|
| `bridge-burn.md` | 4 | 56 | 35 | 21 | 3 |
| `bridge-controls.md` | 3 | 331 | 259 | 72 | 1 |
| `bridge-gloas.md` | 2 | 38 | 25 | 13 | 1 |
| `bridge-inbound.md` | 2 | 125 | 95 | 30 | 1 |
| `bridge-private-claim.md` | 2 | 3 | 2 | 1 | 6 |
| `bridge-roles.md` | 3 | 295 | 256 | 39 | 1 |
| `bridge-time.md` | 2 | 152 | 122 | 30 | 1 |
| `bridge-transfer-private.md` | 2 | 3 | 1 | 2 | 1 |
| `bridge-vault-storage.md` | 2 | 125 | 72 | 53 | 1 |
| `a1-successors-ws.md` | only in deployed program (ccde6171…71dd) | | | | |
| `environment.md` | only in deployed program (ccde6171…71dd) | | | | |
| `holdings.md` | only in deployed program (ccde6171…71dd) | | | | |
| `images.md` | only in deployed program (ccde6171…71dd) | | | | |
| `shell-costs.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage1-finality.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage1-one-transaction.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage1-refusals.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage1-replay.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage4-attest-consumer.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage4-attest-keccak-soft.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage4-attest-private.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage4-attest-refusals.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage4-attest.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage5-attest-refusals.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage5-attest.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage5-consumer-private-kind.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage5-consumer-private.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage5-consumer-public.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage5-sepolia-replay.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage5-vault-synthetic.md` | only in deployed program (ccde6171…71dd) | | | | |
| `stage5-vault-v5.md` | only in deployed program (ccde6171…71dd) | | | | |
| `tenant.md` | only in deployed program (ccde6171…71dd) | | | | |

## `bridge-burn.md`

- 1 / Burn 1 ETH (A) by the payer: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,736 → bridge Admit 35,566 → bridge Burn 21,612` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 52,736 → bridge Admit 35,566 → bridge Burn 21,612`; execution gas: `166,477` → `165,055` (-1,422); base fee: `1,338,528` → `1,327,152` (-11,376)
- 2 / Burn: the same note again: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,736 → bridge Admit 35,566 → bridge Burn 22,940` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 52,736 → bridge Admit 35,566 → bridge Burn 22,940`; execution gas: `167,805` → `166,383` (-1,422); base fee: `1,349,152` → `1,337,776` (-11,376)
- 3 / Burn 0.5 ETH (not a denomination): handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 32,513` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 32,513`; execution gas: `89,076` → `87,654` (-1,422); base fee: `719,320` → `707,944` (-11,376)
- 4 / Burn 1 unit of another token (no holding of it): handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 26,457` → `bridge Burn 24,804 → bridge Burn 25,620`; execution gas: `51,119` → `50,424` (-695); base fee: `415,664` → `410,104` (-5,560)
- 5 / Burn 10 ETH with 9 held: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 26,591` → `bridge Burn 24,804 → bridge Burn 25,770`; execution gas: `51,253` → `50,574` (-679); base fee: `416,736` → `411,304` (-5,432)
- 7 / Burn under a configuration the payer holds nothing of: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 26,224` → `bridge Burn 24,804 → bridge Burn 21,388`; execution gas: `50,886` → `46,192` (-4,694); base fee: `413,800` → `376,248` (-37,552)
- 11 / Burn 1 ETH (B) by the payer: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,355 → bridge Burn 48,868 → bridge Admit 35,566 → bridge Burn 21,612` → `bridge Burn 24,804 → bridge Burn 29,915 → bridge Burn 48,868 → bridge Admit 35,566 → bridge Burn 21,612`; execution gas: `162,063` → `160,765` (-1,298); base fee: `1,303,216` → `1,292,832` (-10,384)
- Burn 1 ETH (A) by the payer / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A) by the payer / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn: the same note again / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn: the same note again / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn 0.5 ETH (not a denomination) / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 0.5 ETH (not a denomination) / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn 1 unit of another token (no holding of it) / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 unit of another token (no holding of it) / bridge Burn: cycles: `26,457` → `25,620` (-837); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68)
- Burn 10 ETH with 9 held / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 10 ETH with 9 held / bridge Burn: cycles: `26,591` → `25,770` (-821); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68)
- Burn under a configuration the payer holds nothing of / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn under a configuration the payer holds nothing of / bridge Burn: cycles: `26,224` → `21,388` (-4,836); input bytes (ReceiveInput): `465` → `323` (-142); of which pre-state: `142` → `0` (-142)
- Burn 1 ETH (B) by the payer / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (B) by the payer / bridge Burn: cycles: `31,355` → `29,915` (-1,440); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `821` → `685` (-136); post-state bytes: `74` → `6` (-68)

## `bridge-controls.md`

- 5 / Register ETH (A): entry, schedules 0 and 1, count at block 100: handlers run (cycles each): `bridge Register 73,920 → Attest 151,273 → bridge (reply) 74,476 → bridge WriteToken 32,147` → `bridge Register 73,907 → Attest 151,273 → bridge (reply) 74,463 → bridge WriteToken 32,147`; execution gas: `331,816` → `331,790` (-26); base fee: `2,676,504` → `2,676,296` (-208)
- 6 / Register ETH (A) again: handlers run (cycles each): `bridge Register 80,680 → Attest 165,778 → bridge (reply) 74,257 → bridge WriteToken 34,994` → `bridge Register 80,667 → Attest 165,778 → bridge (reply) 74,244 → bridge WriteToken 34,994`; execution gas: `355,709` → `355,683` (-26); base fee: `2,870,312` → `2,870,104` (-208)
- 7 / Register T2 (A): vault A holds no entry for it: handlers run (cycles each): `bridge Register 66,224 → Attest 147,735 → bridge (reply) 50,017` → `bridge Register 66,211 → Attest 147,735 → bridge (reply) 50,004`; execution gas: `263,976` → `263,950` (-26); base fee: `2,132,872` → `2,132,664` (-208)
- 9 / Register ETH (B) with other relay fees than the vault's: handlers run (cycles each): `bridge Register 76,862 → Attest 156,184 → bridge (reply) 50,670` → `bridge Register 76,849 → Attest 156,184 → bridge (reply) 50,657`; execution gas: `283,716` → `283,690` (-26); base fee: `2,291,888` → `2,291,680` (-208)
- 15 / Register ETH (B): handlers run (cycles each): `bridge Register 76,864 → Attest 156,184 → bridge (reply) 74,476 → bridge WriteToken 32,147` → `bridge Register 76,851 → Attest 156,184 → bridge (reply) 74,463 → bridge WriteToken 32,147`; execution gas: `339,671` → `339,645` (-26); base fee: `2,739,528` → `2,739,320` (-208)
- 16 / Register T2 (B): a second token of the same configuration: handlers run (cycles each): `bridge Register 72,964 → Attest 168,339 → bridge (reply) 72,789 → bridge WriteToken 30,974` → `bridge Register 72,951 → Attest 168,339 → bridge (reply) 72,776 → bridge WriteToken 30,974`; execution gas: `345,066` → `345,040` (-26); base fee: `2,783,328` → `2,783,120` (-208)
- 17 / Register T3 (A) at block 101: schedule 1 present, covers nothing yet: handlers run (cycles each): `bridge Register 81,712 → Attest 179,681 → bridge (reply) 74,257 → bridge WriteToken 32,147` → `bridge Register 81,699 → Attest 179,681 → bridge (reply) 74,244 → bridge WriteToken 32,147`; execution gas: `367,797` → `367,771` (-26); base fee: `2,968,408` → `2,968,200` (-208)
- 37 / Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 51,585 → bridge Admit 35,564 → bridge Spend 37,291 → bridge Mint 18,161 → bridge Mint 18,161 → bridge Mint 18,161` → `bridge Claim 107,702 → bridge Issue 52,633 → bridge Admit 35,564 → bridge Spend 38,243 → bridge Mint 18,761 → bridge Mint 18,761 → bridge Mint 18,761`; execution gas: `286,625` → `290,425` (+3,800); base fee: `2,309,424` → `2,339,824` (+30,400)
- 41 / Claim leaf 2 after the Tick (0.1 ETH refilled): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 51,549 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 22,496` → `bridge Claim 107,702 → bridge Issue 52,597 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 23,096`; execution gas: `266,389` → `269,446` (+3,057); base fee: `2,147,536` → `2,171,992` (+24,456)
- 43 / Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap: handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 47,293 → bridge Admit 28,502` → `bridge Claim 107,702 → bridge Issue 48,341 → bridge Admit 28,502`; execution gas: `183,497` → `184,545` (+1,048); base fee: `1,484,400` → `1,492,784` (+8,384)
- 44 / Claim B's 1,000 T2 (1 ETH of value units) under the same global cap: handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 45,451 → bridge Admit 35,564 → bridge Spend 24,293 → bridge Mint 22,784` → `bridge Claim 107,702 → bridge Issue 46,499 → bridge Admit 35,564 → bridge Spend 24,959 → bridge Mint 18,761`; execution gas: `235,794` → `233,485` (-2,309); base fee: `1,902,776` → `1,884,304` (-18,472)
- 47 / Burn 1 ETH (A): handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,735 → bridge Admit 35,565 → bridge Burn 21,612` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 52,735 → bridge Admit 35,565 → bridge Burn 21,612`; execution gas: `166,475` → `165,053` (-1,422); base fee: `1,338,512` → `1,327,136` (-11,376)
- 48 / Burn 1 ETH (A) again: the burn bucket is empty: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 33,129` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 33,129`; execution gas: `89,692` → `88,270` (-1,422); base fee: `724,248` → `712,872` (-11,376)
- 49 / Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 33,129` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 33,129`; execution gas: `89,692` → `88,270` (-1,422); base fee: `724,248` → `712,872` (-11,376)
- 51 / Burn 1 ETH (A) after the Tick: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 53,151 → bridge Admit 35,566 → bridge Burn 21,612` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 53,151 → bridge Admit 35,566 → bridge Burn 21,612`; execution gas: `166,892` → `165,470` (-1,422); base fee: `1,341,848` → `1,330,472` (-11,376)
- 61 / Claim B's 1 ETH under the global mint freeze: handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 47,708 → bridge Admit 28,472` → `bridge Claim 107,702 → bridge Issue 48,756 → bridge Admit 28,472`; execution gas: `183,882` → `184,930` (+1,048); base fee: `1,487,480` → `1,495,864` (+8,384)
- 63 / Burn 1 ETH (A) while its burns are frozen: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 33,100` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 33,100`; execution gas: `89,663` → `88,241` (-1,422); base fee: `724,016` → `712,640` (-11,376)
- 66 / Burn 1 ETH (A) under the global burn freeze: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,735 → bridge Admit 28,474` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 52,735 → bridge Admit 28,474`; execution gas: `137,772` → `136,350` (-1,422); base fee: `1,108,888` → `1,097,512` (-11,376)
- 67 / Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,735 → bridge Admit 28,474` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 52,735 → bridge Admit 28,474`; execution gas: `137,772` → `136,350` (-1,422); base fee: `1,108,888` → `1,097,512` (-11,376)
- 69 / Burn 1 ETH (A) after the Tick: the freeze has lapsed: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,735 → bridge Admit 35,860 → bridge Burn 21,612` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 52,735 → bridge Admit 35,860 → bridge Burn 21,612`; execution gas: `166,770` → `165,348` (-1,422); base fee: `1,340,872` → `1,329,496` (-11,376)
- 77 / Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue: handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 47,708 → bridge Admit 35,859 → bridge Spend 30,916 → bridge Mint 24,538 → bridge Mint 22,784` → `bridge Claim 107,702 → bridge Issue 48,756 → bridge Admit 35,859 → bridge Spend 31,725 → bridge Mint 23,617 → bridge Mint 18,761`; execution gas: `269,507` → `266,420` (-3,087); base fee: `2,172,480` → `2,147,784` (-24,696)
- 78 / Burn 1 ETH (B) after Sunset: burns continue: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 33,236 → bridge Burn 49,283 → bridge Admit 35,565 → bridge Burn 21,612` → `bridge Burn 24,804 → bridge Burn 31,659 → bridge Burn 49,283 → bridge Admit 35,565 → bridge Burn 21,612`; execution gas: `164,358` → `162,923` (-1,435); base fee: `1,321,576` → `1,310,096` (-11,480)
- Register ETH (A): entry, schedules 0 and 1, count at block 100 / bridge Register: cycles: `73,920` → `73,907` (-13)
- Register ETH (A): entry, schedules 0 and 1, count at block 100 / bridge (reply): cycles: `74,476` → `74,463` (-13)
- Register ETH (A) again / bridge Register: cycles: `80,680` → `80,667` (-13)
- Register ETH (A) again / bridge (reply): cycles: `74,257` → `74,244` (-13)
- Register T2 (A): vault A holds no entry for it / bridge Register: cycles: `66,224` → `66,211` (-13)
- Register T2 (A): vault A holds no entry for it / bridge (reply): cycles: `50,017` → `50,004` (-13)
- Register ETH (B) with other relay fees than the vault's / bridge Register: cycles: `76,862` → `76,849` (-13)
- Register ETH (B) with other relay fees than the vault's / bridge (reply): cycles: `50,670` → `50,657` (-13)
- Register ETH (B) / bridge Register: cycles: `76,864` → `76,851` (-13)
- Register ETH (B) / bridge (reply): cycles: `74,476` → `74,463` (-13)
- Register T2 (B): a second token of the same configuration / bridge Register: cycles: `72,964` → `72,951` (-13)
- Register T2 (B): a second token of the same configuration / bridge (reply): cycles: `72,789` → `72,776` (-13)
- Register T3 (A) at block 101: schedule 1 present, covers nothing yet / bridge Register: cycles: `81,712` → `81,699` (-13)
- Register T3 (A) at block 101: schedule 1 present, covers nothing yet / bridge (reply): cycles: `74,257` → `74,244` (-13)
- Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) / bridge Issue: cycles: `51,585` → `52,633` (+1,048)
- Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) / bridge Spend: cycles: `37,291` → `38,243` (+952)
- Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 2 after the Tick (0.1 ETH refilled) / bridge Issue: cycles: `51,549` → `52,597` (+1,048)
- Claim leaf 2 after the Tick (0.1 ETH refilled) / bridge Spend: cycles: `30,916` → `31,725` (+809)
- Claim leaf 2 after the Tick (0.1 ETH refilled) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 2 after the Tick (0.1 ETH refilled) / bridge Mint: cycles: `22,496` → `23,096` (+600)
- Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap / bridge Issue: cycles: `47,293` → `48,341` (+1,048)
- Claim B's 1,000 T2 (1 ETH of value units) under the same global cap / bridge Issue: cycles: `45,451` → `46,499` (+1,048)
- Claim B's 1,000 T2 (1 ETH of value units) under the same global cap / bridge Spend: cycles: `24,293` → `24,959` (+666)
- Claim B's 1,000 T2 (1 ETH of value units) under the same global cap / bridge Mint: cycles: `22,784` → `18,761` (-4,023); input bytes (ReceiveInput): `373` → `299` (-74); of which pre-state: `74` → `0` (-74); journal bytes: `540` → `398` (-142); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (A) / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A) / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (A) again: the burn bucket is empty / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A) again: the burn bucket is empty / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (A) after the Tick / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A) after the Tick / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Claim B's 1 ETH under the global mint freeze / bridge Issue: cycles: `47,708` → `48,756` (+1,048)
- Burn 1 ETH (A) while its burns are frozen / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A) while its burns are frozen / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (A) under the global burn freeze / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A) under the global burn freeze / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (A) after the Tick: the freeze has lapsed / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A) after the Tick: the freeze has lapsed / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue / bridge Issue: cycles: `47,708` → `48,756` (+1,048)
- Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue / bridge Spend: cycles: `30,916` → `31,725` (+809)
- Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue / bridge Mint: cycles: `24,538` → `23,617` (-921); input bytes (ReceiveInput): `441` → `373` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `676` → `540` (-136); post-state bytes: `210` → `142` (-68)
- Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue / bridge Mint: cycles: `22,784` → `18,761` (-4,023); input bytes (ReceiveInput): `373` → `299` (-74); of which pre-state: `74` → `0` (-74); journal bytes: `540` → `398` (-142); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (B) after Sunset: burns continue / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (B) after Sunset: burns continue / bridge Burn: cycles: `33,236` → `31,659` (-1,577); input bytes (ReceiveInput): `533` → `465` (-68); of which pre-state: `210` → `142` (-68); journal bytes: `957` → `821` (-136); post-state bytes: `142` → `74` (-68)

## `bridge-gloas.md`

- 4 / Register ETH (C), Gloas record, no header: handlers run (cycles each): `bridge Register 73,647 → Attest 25,664` → `bridge Register 73,634 → Attest 25,664`; execution gas: `99,311` → `99,298` (-13); base fee: `815,872` → `815,768` (-104)
- 5 / Register ETH (C), Gloas record, with the header: handlers run (cycles each): `bridge Register 84,495 → Attest 176,262 → bridge (reply) 74,476 → bridge WriteToken 32,147` → `bridge Register 84,482 → Attest 176,262 → bridge (reply) 74,463 → bridge WriteToken 32,147`; execution gas: `367,380` → `367,354` (-26); base fee: `2,966,240` → `2,966,032` (-208)
- 8 / Claim: leaf 0 under root(C, 2): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 47,280 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 18,161` → `bridge Claim 107,702 → bridge Issue 48,328 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 18,761`; execution gas: `257,785` → `260,842` (+3,057); base fee: `2,078,704` → `2,103,160` (+24,456)
- 9 / Claim: leaf 0 again (replay): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 47,280 → bridge Admit 35,565 → bridge Spend 19,559` → `bridge Claim 107,702 → bridge Issue 48,328 → bridge Admit 35,565 → bridge Spend 20,082`; execution gas: `210,106` → `211,677` (+1,571); base fee: `1,697,272` → `1,709,840` (+12,568)
- Register ETH (C), Gloas record, no header / bridge Register: cycles: `73,647` → `73,634` (-13)
- Register ETH (C), Gloas record, with the header / bridge Register: cycles: `84,495` → `84,482` (-13)
- Register ETH (C), Gloas record, with the header / bridge (reply): cycles: `74,476` → `74,463` (-13)
- Claim: leaf 0 under root(C, 2) / bridge Issue: cycles: `47,280` → `48,328` (+1,048)
- Claim: leaf 0 under root(C, 2) / bridge Spend: cycles: `30,916` → `31,725` (+809)
- Claim: leaf 0 under root(C, 2) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim: leaf 0 under root(C, 2) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim: leaf 0 again (replay) / bridge Issue: cycles: `47,280` → `48,328` (+1,048)
- Claim: leaf 0 again (replay) / bridge Spend: cycles: `19,559` → `20,082` (+523)

## `bridge-inbound.md`

- 5 / Register ETH (A): entry and schedule 0 attested at block 100: handlers run (cycles each): `bridge Register 73,920 → Attest 151,273 → bridge (reply) 74,476 → bridge WriteToken 32,147` → `bridge Register 73,907 → Attest 151,273 → bridge (reply) 74,463 → bridge WriteToken 32,147`; execution gas: `331,816` → `331,790` (-26); base fee: `2,676,504` → `2,676,296` (-208)
- 6 / Register ETH (B): handlers run (cycles each): `bridge Register 76,864 → Attest 156,184 → bridge (reply) 74,476 → bridge WriteToken 32,147` → `bridge Register 76,851 → Attest 156,184 → bridge (reply) 74,463 → bridge WriteToken 32,147`; execution gas: `339,671` → `339,645` (-26); base fee: `2,739,528` → `2,739,320` (-208)
- 18 / Claim: the shared vector's deposit (leaf 2) under root(A, 5): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 51,166 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 18,161` → `bridge Claim 107,702 → bridge Issue 52,214 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 18,761`; execution gas: `261,671` → `264,728` (+3,057); base fee: `2,109,792` → `2,134,248` (+24,456)
- 19 / Claim: the same again (replay): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 51,166 → bridge Admit 35,565 → bridge Spend 19,559` → `bridge Claim 107,702 → bridge Issue 52,214 → bridge Admit 35,565 → bridge Spend 20,082`; execution gas: `213,992` → `215,563` (+1,571); base fee: `1,728,360` → `1,740,928` (+12,568)
- 20 / Claim: the same deposit under root(A, 8): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 51,166 → bridge Admit 35,565 → bridge Spend 19,559` → `bridge Claim 107,702 → bridge Issue 52,214 → bridge Admit 35,565 → bridge Spend 20,082`; execution gas: `213,992` → `215,563` (+1,571); base fee: `1,728,360` → `1,740,928` (+12,568)
- 30 / Mint sent directly to a holder: handlers run (cycles each): `bridge Mint 11,546` → `bridge Mint 12,072`; execution gas: `11,546` → `12,072` (+526); base fee: `97,360` → `101,568` (+4,208)
- 31 / Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 51,586 → bridge Admit 35,565 → bridge Spend 37,291 → bridge Mint 18,161 → bridge Mint 18,161 → bridge Mint 22,496` → `bridge Claim 107,702 → bridge Issue 52,634 → bridge Admit 35,565 → bridge Spend 38,243 → bridge Mint 18,761 → bridge Mint 18,761 → bridge Mint 23,096`; execution gas: `290,962` → `294,762` (+3,800); base fee: `2,344,120` → `2,374,520` (+30,400)
- 32 / Claim: vault B's deposit under root(B, 8): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 47,293 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 22,784 → bridge Mint 22,784` → `bridge Claim 107,702 → bridge Issue 48,341 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 18,761`; execution gas: `267,044` → `260,855` (-6,189); base fee: `2,152,776` → `2,103,264` (-49,512)
- Register ETH (A): entry and schedule 0 attested at block 100 / bridge Register: cycles: `73,920` → `73,907` (-13)
- Register ETH (A): entry and schedule 0 attested at block 100 / bridge (reply): cycles: `74,476` → `74,463` (-13)
- Register ETH (B) / bridge Register: cycles: `76,864` → `76,851` (-13)
- Register ETH (B) / bridge (reply): cycles: `74,476` → `74,463` (-13)
- Claim: the shared vector's deposit (leaf 2) under root(A, 5) / bridge Issue: cycles: `51,166` → `52,214` (+1,048)
- Claim: the shared vector's deposit (leaf 2) under root(A, 5) / bridge Spend: cycles: `30,916` → `31,725` (+809)
- Claim: the shared vector's deposit (leaf 2) under root(A, 5) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim: the shared vector's deposit (leaf 2) under root(A, 5) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim: the same again (replay) / bridge Issue: cycles: `51,166` → `52,214` (+1,048)
- Claim: the same again (replay) / bridge Spend: cycles: `19,559` → `20,082` (+523)
- Claim: the same deposit under root(A, 8) / bridge Issue: cycles: `51,166` → `52,214` (+1,048)
- Claim: the same deposit under root(A, 8) / bridge Spend: cycles: `19,559` → `20,082` (+523)
- Mint sent directly to a holder / bridge Mint: cycles: `11,546` → `12,072` (+526)
- Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) / bridge Issue: cycles: `51,586` → `52,634` (+1,048)
- Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) / bridge Spend: cycles: `37,291` → `38,243` (+952)
- Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) / bridge Mint: cycles: `22,496` → `23,096` (+600)
- Claim: vault B's deposit under root(B, 8) / bridge Issue: cycles: `47,293` → `48,341` (+1,048)
- Claim: vault B's deposit under root(B, 8) / bridge Spend: cycles: `30,916` → `31,725` (+809)
- Claim: vault B's deposit under root(B, 8) / bridge Mint: cycles: `22,784` → `18,761` (-4,023); input bytes (ReceiveInput): `373` → `299` (-74); of which pre-state: `74` → `0` (-74); journal bytes: `540` → `398` (-142); post-state bytes: `142` → `74` (-68)
- Claim: vault B's deposit under root(B, 8) / bridge Mint: cycles: `22,784` → `18,761` (-4,023); input bytes (ReceiveInput): `373` → `299` (-74); of which pre-state: `74` → `0` (-74); journal bytes: `540` → `398` (-142); post-state bytes: `142` → `74` (-68)

## `bridge-private-claim.md`

- 555,955 / 224,063: execution gas: `553,641` → `555,955` (+2,314); base fee: `6,221,632` → `6,240,144` (+18,512)

## `bridge-roles.md`

- 5 / Register ETH (A): handlers run (cycles each): `bridge Register 73,920 → Attest 151,273 → bridge (reply) 74,476 → bridge WriteToken 32,147` → `bridge Register 73,907 → Attest 151,273 → bridge (reply) 74,463 → bridge WriteToken 32,147`; execution gas: `331,816` → `331,790` (-26); base fee: `2,676,504` → `2,676,296` (-208)
- 6 / Register ETH (B): handlers run (cycles each): `bridge Register 76,864 → Attest 156,184 → bridge (reply) 74,476 → bridge WriteToken 32,147` → `bridge Register 76,851 → Attest 156,184 → bridge (reply) 74,463 → bridge WriteToken 32,147`; execution gas: `339,671` → `339,645` (-26); base fee: `2,739,528` → `2,739,320` (-208)
- 11 / Claim A's leaf 6: 10 ETH to the payer: handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 51,586 → bridge Admit 35,564 → bridge Spend 37,291 → bridge Mint 18,161 → bridge Mint 18,161 → bridge Mint 18,161` → `bridge Claim 107,702 → bridge Issue 52,634 → bridge Admit 35,564 → bridge Spend 38,243 → bridge Mint 18,761 → bridge Mint 18,761 → bridge Mint 18,761`; execution gas: `286,626` → `290,426` (+3,800); base fee: `2,309,432` → `2,339,832` (+30,400)
- 12 / Transfer 1 ETH (A): the payer to a holder: handlers run (cycles each): `bridge Transfer 17,578 → bridge Transfer 23,394 → bridge Transfer 36,829 → bridge Credit 16,863` → `bridge Transfer 17,722 → bridge Transfer 23,465 → bridge Transfer 36,972 → bridge Credit 16,934`; execution gas: `94,664` → `95,093` (+429); base fee: `762,840` → `766,272` (+3,432)
- 13 / Transfer 0: handlers run (cycles each): `bridge Transfer 17,578 → bridge Transfer 23,394 → bridge Transfer 26,520` → `bridge Transfer 17,722 → bridge Transfer 23,465 → bridge Transfer 26,520`; execution gas: `67,492` → `67,707` (+215); base fee: `545,464` → `547,184` (+1,720)
- 14 / Transfer 1.5 gwei (not a multiple of the LEZ unit): handlers run (cycles each): `bridge Transfer 17,578 → bridge Transfer 23,394 → bridge Transfer 26,696` → `bridge Transfer 17,722 → bridge Transfer 23,465 → bridge Transfer 26,696`; execution gas: `67,668` → `67,883` (+215); base fee: `546,872` → `548,592` (+1,720)
- 15 / Transfer 100 ETH (more than the payer holds): handlers run (cycles each): `bridge Transfer 17,578 → bridge Transfer 19,446` → `bridge Transfer 17,722 → bridge Transfer 19,517`; execution gas: `37,024` → `37,239` (+215); base fee: `301,720` → `303,440` (+1,720)
- 16 / Transfer of a token with no holding (B's ETH): handlers run (cycles each): `bridge Transfer 17,578 → bridge Transfer 19,062` → `bridge Transfer 17,722 → bridge Transfer 15,075`; execution gas: `36,640` → `32,797` (-3,843); base fee: `298,648` → `267,904` (-30,744)
- 21 / Transfer 0.1 ETH (A): the holder, signed, to the payer: handlers run (cycles each): `bridge Transfer 17,578 → bridge Transfer 23,394 → bridge Transfer 36,807 → bridge Credit 21,307` → `bridge Transfer 17,722 → bridge Transfer 23,465 → bridge Transfer 36,950 → bridge Credit 21,378`; execution gas: `99,086` → `99,515` (+429); base fee: `799,624` → `803,056` (+3,432)
- 69 / Transfer 1 ETH (A) after Sunset: balances still move: handlers run (cycles each): `bridge Transfer 17,578 → bridge Transfer 23,394 → bridge Transfer 36,829 → bridge Credit 21,307` → `bridge Transfer 17,722 → bridge Transfer 23,465 → bridge Transfer 36,972 → bridge Credit 21,378`; execution gas: `99,108` → `99,537` (+429); base fee: `798,392` → `801,824` (+3,432)
- Register ETH (A) / bridge Register: cycles: `73,920` → `73,907` (-13)
- Register ETH (A) / bridge (reply): cycles: `74,476` → `74,463` (-13)
- Register ETH (B) / bridge Register: cycles: `76,864` → `76,851` (-13)
- Register ETH (B) / bridge (reply): cycles: `74,476` → `74,463` (-13)
- Claim A's leaf 6: 10 ETH to the payer / bridge Issue: cycles: `51,586` → `52,634` (+1,048)
- Claim A's leaf 6: 10 ETH to the payer / bridge Spend: cycles: `37,291` → `38,243` (+952)
- Claim A's leaf 6: 10 ETH to the payer / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim A's leaf 6: 10 ETH to the payer / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim A's leaf 6: 10 ETH to the payer / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Transfer 1 ETH (A): the payer to a holder / bridge Transfer: cycles: `17,578` → `17,722` (+144)
- Transfer 1 ETH (A): the payer to a holder / bridge Transfer: cycles: `23,394` → `23,465` (+71)
- Transfer 1 ETH (A): the payer to a holder / bridge Transfer: cycles: `36,829` → `36,972` (+143)
- Transfer 1 ETH (A): the payer to a holder / bridge Credit: cycles: `16,863` → `16,934` (+71)
- Transfer 0 / bridge Transfer: cycles: `17,578` → `17,722` (+144)
- Transfer 0 / bridge Transfer: cycles: `23,394` → `23,465` (+71)
- Transfer 1.5 gwei (not a multiple of the LEZ unit) / bridge Transfer: cycles: `17,578` → `17,722` (+144)
- Transfer 1.5 gwei (not a multiple of the LEZ unit) / bridge Transfer: cycles: `23,394` → `23,465` (+71)
- Transfer 100 ETH (more than the payer holds) / bridge Transfer: cycles: `17,578` → `17,722` (+144)
- Transfer 100 ETH (more than the payer holds) / bridge Transfer: cycles: `19,446` → `19,517` (+71)
- Transfer of a token with no holding (B's ETH) / bridge Transfer: cycles: `17,578` → `17,722` (+144)
- Transfer of a token with no holding (B's ETH) / bridge Transfer: cycles: `19,062` → `15,075` (-3,987); input bytes (ReceiveInput): `313` → `239` (-74); of which pre-state: `74` → `0` (-74)
- Transfer 0.1 ETH (A): the holder, signed, to the payer / bridge Transfer: cycles: `17,578` → `17,722` (+144)
- Transfer 0.1 ETH (A): the holder, signed, to the payer / bridge Transfer: cycles: `23,394` → `23,465` (+71)
- Transfer 0.1 ETH (A): the holder, signed, to the payer / bridge Transfer: cycles: `36,807` → `36,950` (+143)
- Transfer 0.1 ETH (A): the holder, signed, to the payer / bridge Credit: cycles: `21,307` → `21,378` (+71)
- Transfer 1 ETH (A) after Sunset: balances still move / bridge Transfer: cycles: `17,578` → `17,722` (+144)
- Transfer 1 ETH (A) after Sunset: balances still move / bridge Transfer: cycles: `23,394` → `23,465` (+71)
- Transfer 1 ETH (A) after Sunset: balances still move / bridge Transfer: cycles: `36,829` → `36,972` (+143)
- Transfer 1 ETH (A) after Sunset: balances still move / bridge Credit: cycles: `21,307` → `21,378` (+71)

## `bridge-time.md`

- 6 / Claim B's 1 ETH in that block: the freeze holds: handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 47,293 → bridge Admit 28,473` → `bridge Claim 107,702 → bridge Issue 48,341 → bridge Admit 28,473`; execution gas: `183,468` → `184,516` (+1,048); base fee: `1,484,168` → `1,492,552` (+8,384)
- 9 / Claim leaf 6 (10.06 ETH of the 11): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 51,585 → bridge Admit 35,564 → bridge Spend 37,291 → bridge Mint 18,161 → bridge Mint 18,161 → bridge Mint 18,161` → `bridge Claim 107,702 → bridge Issue 52,633 → bridge Admit 35,564 → bridge Spend 38,243 → bridge Mint 18,761 → bridge Mint 18,761 → bridge Mint 18,761`; execution gas: `286,625` → `290,425` (+3,800); base fee: `2,309,424` → `2,339,824` (+30,400)
- 14 / Claim leaf 2 after it: 0.1 ETH refilled: handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 51,549 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 22,496` → `bridge Claim 107,702 → bridge Issue 52,597 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 23,096`; execution gas: `266,389` → `269,446` (+3,057); base fee: `2,147,536` → `2,171,992` (+24,456)
- 16 / Claim B's 1 ETH: the freeze has lapsed: handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 47,293 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 22,784 → bridge Mint 22,784` → `bridge Claim 107,702 → bridge Issue 48,341 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 18,761`; execution gas: `267,044` → `260,855` (-6,189); base fee: `2,152,776` → `2,103,264` (-49,512)
- 21 / Burn 1 ETH (A): A's burns still frozen: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 53,028 → bridge Admit 28,474` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 53,028 → bridge Admit 28,474`; execution gas: `138,065` → `136,643` (-1,422); base fee: `1,111,232` → `1,099,856` (-11,376)
- 23 / Burn 1 ETH (A): handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 53,028 → bridge Admit 35,565 → bridge Burn 21,612` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 53,028 → bridge Admit 35,565 → bridge Burn 21,612`; execution gas: `166,768` → `165,346` (-1,422); base fee: `1,340,856` → `1,329,480` (-11,376)
- 25 / Burn 1 ETH (A): its burn bucket has not refilled: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 33,129` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 33,129`; execution gas: `89,692` → `88,270` (-1,422); base fee: `724,248` → `712,872` (-11,376)
- 27 / Burn 1 ETH (A): raising the capacity refills nothing: handlers run (cycles each): `bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 33,129` → `bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 33,129`; execution gas: `89,692` → `88,270` (-1,422); base fee: `724,248` → `712,872` (-11,376)
- Claim B's 1 ETH in that block: the freeze holds / bridge Issue: cycles: `47,293` → `48,341` (+1,048)
- Claim leaf 6 (10.06 ETH of the 11) / bridge Issue: cycles: `51,585` → `52,633` (+1,048)
- Claim leaf 6 (10.06 ETH of the 11) / bridge Spend: cycles: `37,291` → `38,243` (+952)
- Claim leaf 6 (10.06 ETH of the 11) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 6 (10.06 ETH of the 11) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 6 (10.06 ETH of the 11) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 2 after it: 0.1 ETH refilled / bridge Issue: cycles: `51,549` → `52,597` (+1,048)
- Claim leaf 2 after it: 0.1 ETH refilled / bridge Spend: cycles: `30,916` → `31,725` (+809)
- Claim leaf 2 after it: 0.1 ETH refilled / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 2 after it: 0.1 ETH refilled / bridge Mint: cycles: `22,496` → `23,096` (+600)
- Claim B's 1 ETH: the freeze has lapsed / bridge Issue: cycles: `47,293` → `48,341` (+1,048)
- Claim B's 1 ETH: the freeze has lapsed / bridge Spend: cycles: `30,916` → `31,725` (+809)
- Claim B's 1 ETH: the freeze has lapsed / bridge Mint: cycles: `22,784` → `18,761` (-4,023); input bytes (ReceiveInput): `373` → `299` (-74); of which pre-state: `74` → `0` (-74); journal bytes: `540` → `398` (-142); post-state bytes: `142` → `74` (-68)
- Claim B's 1 ETH: the freeze has lapsed / bridge Mint: cycles: `22,784` → `18,761` (-4,023); input bytes (ReceiveInput): `373` → `299` (-74); of which pre-state: `74` → `0` (-74); journal bytes: `540` → `398` (-142); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (A): A's burns still frozen / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A): A's burns still frozen / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (A) / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A) / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (A): its burn bucket has not refilled / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A): its burn bucket has not refilled / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)
- Burn 1 ETH (A): raising the capacity refills nothing / bridge Burn: cycles: `24,662` → `24,804` (+142)
- Burn 1 ETH (A): raising the capacity refills nothing / bridge Burn: cycles: `31,901` → `30,337` (-1,564); input bytes (ReceiveInput): `465` → `397` (-68); of which pre-state: `142` → `74` (-68); journal bytes: `889` → `753` (-136); post-state bytes: `142` → `74` (-68)

## `bridge-transfer-private.md`

- Transfer at the private holder (debits, sends Transfer) / 249: user cycles: `19,113` → `19,114` (+1)
- e190611c58d3283ed5f11e4fa7441be6559a8b0391151cb28ee958294d83a1d1 / alias cd0ea9aa: to: `0840c609b5de392eab03c82b5a8fb7d5719e34edaa363995a53441132d2f173f` → `e190611c58d3283ed5f11e4fa7441be6559a8b0391151cb28ee958294d83a1d1`

## `bridge-vault-storage.md`

- 2 / Register ETH with lezUnit ×10 (not the vault's entry): handlers run (cycles each): `bridge Register 106,268 → Attest 220,937 → bridge (reply) 50,670` → `bridge Register 106,255 → Attest 220,937 → bridge (reply) 50,657`; execution gas: `377,875` → `377,849` (-26); base fee: `3,062,792` → `3,062,584` (-208)
- 3 / Register ETH with schedule 0 a second later: handlers run (cycles each): `bridge Register 106,076 → Attest 220,937 → bridge (reply) 53,399` → `bridge Register 106,063 → Attest 220,937 → bridge (reply) 53,386`; execution gas: `380,412` → `380,386` (-26); base fee: `3,083,088` → `3,082,880` (-208)
- 4 / Register ETH: the vault's entry and schedule 0 at P: handlers run (cycles each): `bridge Register 106,076 → Attest 220,937 → bridge (reply) 74,476 → bridge WriteToken 32,147` → `bridge Register 106,063 → Attest 220,937 → bridge (reply) 74,463 → bridge WriteToken 32,147`; execution gas: `433,636` → `433,610` (-26); base fee: `3,508,880` → `3,508,672` (-208)
- 5 / Register token 0xf62849…: the vault's entry and schedule 0 at P: handlers run (cycles each): `bridge Register 100,037 → Attest 226,956 → bridge (reply) 74,476 → bridge WriteToken 32,147` → `bridge Register 100,024 → Attest 226,956 → bridge (reply) 74,463 → bridge WriteToken 32,147`; execution gas: `433,616` → `433,590` (-26); base fee: `3,508,920` → `3,508,712` (-208)
- 6 / Register token 0x5991a2…: the vault's entry and schedule 0 at P: handlers run (cycles each): `bridge Register 101,233 → Attest 223,998 → bridge (reply) 74,476 → bridge WriteToken 32,147` → `bridge Register 101,220 → Attest 223,998 → bridge (reply) 74,463 → bridge WriteToken 32,147`; execution gas: `431,854` → `431,828` (-26); base fee: `3,493,272` → `3,493,064` (-208)
- 7 / Register token 0xc71834…: the vault's entry and schedule 0 at P: handlers run (cycles each): `bridge Register 103,090 → Attest 245,403 → bridge (reply) 74,476 → bridge WriteToken 32,147` → `bridge Register 103,077 → Attest 245,403 → bridge (reply) 74,463 → bridge WriteToken 32,147`; execution gas: `455,116` → `455,090` (-26); base fee: `3,683,040` → `3,682,832` (-208)
- 8 / Register token 0x1d1499…: the vault's entry and schedule 0 at P: handlers run (cycles each): `bridge Register 97,420 → Attest 230,329 → bridge (reply) 72,789 → bridge WriteToken 30,974` → `bridge Register 97,407 → Attest 230,329 → bridge (reply) 72,776 → bridge WriteToken 30,974`; execution gas: `431,512` → `431,486` (-26); base fee: `3,492,768` → `3,492,560` (-208)
- 10 / Claim leaf 0 (ETH, under root(n = 3)): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 47,201 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 18,161` → `bridge Claim 107,702 → bridge Issue 48,249 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 18,761`; execution gas: `257,706` → `260,763` (+3,057); base fee: `2,078,072` → `2,102,528` (+24,456)
- 11 / Claim leaf 1 (token 0x1d1499…, under root(n = 3)): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 45,889 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 23,017` → `bridge Claim 107,702 → bridge Issue 46,937 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 23,617`; execution gas: `261,250` → `264,307` (+3,057); base fee: `2,106,424` → `2,130,880` (+24,456)
- 12 / Claim leaf 2 (ETH, under root(n = 3)): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 47,166 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 23,965` → `bridge Claim 107,702 → bridge Issue 48,214 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 24,565`; execution gas: `263,475` → `266,532` (+3,057); base fee: `2,124,224` → `2,148,680` (+24,456)
- 21 / Claim leaf 3 (ETH, under root(n = 6)): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 51,535 → bridge Admit 35,565 → bridge Spend 37,291 → bridge Mint 18,161 → bridge Mint 18,161 → bridge Mint 23,965` → `bridge Claim 107,702 → bridge Issue 52,583 → bridge Admit 35,565 → bridge Spend 38,243 → bridge Mint 18,761 → bridge Mint 18,761 → bridge Mint 24,565`; execution gas: `292,380` → `296,180` (+3,800); base fee: `2,355,464` → `2,385,864` (+30,400)
- 22 / Claim leaf 4 (token 0x1d1499…, under root(n = 6)): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 45,905 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 24,212` → `bridge Claim 107,702 → bridge Issue 46,953 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 24,812`; execution gas: `262,461` → `265,518` (+3,057); base fee: `2,116,112` → `2,140,568` (+24,456)
- 23 / Claim leaf 5 (ETH, under root(n = 6)): handlers run (cycles each): `bridge Claim 107,702 → bridge Issue 51,586 → bridge Admit 35,565 → bridge Spend 37,291 → bridge Mint 18,161 → bridge Mint 22,496 → bridge Mint 23,965` → `bridge Claim 107,702 → bridge Issue 52,634 → bridge Admit 35,565 → bridge Spend 38,243 → bridge Mint 18,761 → bridge Mint 23,096 → bridge Mint 24,565`; execution gas: `296,766` → `300,566` (+3,800); base fee: `2,390,552` → `2,420,952` (+30,400)
- Register ETH with lezUnit ×10 (not the vault's entry) / bridge Register: cycles: `106,268` → `106,255` (-13)
- Register ETH with lezUnit ×10 (not the vault's entry) / bridge (reply): cycles: `50,670` → `50,657` (-13)
- Register ETH with schedule 0 a second later / bridge Register: cycles: `106,076` → `106,063` (-13)
- Register ETH with schedule 0 a second later / bridge (reply): cycles: `53,399` → `53,386` (-13)
- Register ETH: the vault's entry and schedule 0 at P / bridge Register: cycles: `106,076` → `106,063` (-13)
- Register ETH: the vault's entry and schedule 0 at P / bridge (reply): cycles: `74,476` → `74,463` (-13)
- Register token 0xf62849…: the vault's entry and schedule 0 at P / bridge Register: cycles: `100,037` → `100,024` (-13)
- Register token 0xf62849…: the vault's entry and schedule 0 at P / bridge (reply): cycles: `74,476` → `74,463` (-13)
- Register token 0x5991a2…: the vault's entry and schedule 0 at P / bridge Register: cycles: `101,233` → `101,220` (-13)
- Register token 0x5991a2…: the vault's entry and schedule 0 at P / bridge (reply): cycles: `74,476` → `74,463` (-13)
- Register token 0xc71834…: the vault's entry and schedule 0 at P / bridge Register: cycles: `103,090` → `103,077` (-13)
- Register token 0xc71834…: the vault's entry and schedule 0 at P / bridge (reply): cycles: `74,476` → `74,463` (-13)
- Register token 0x1d1499…: the vault's entry and schedule 0 at P / bridge Register: cycles: `97,420` → `97,407` (-13)
- Register token 0x1d1499…: the vault's entry and schedule 0 at P / bridge (reply): cycles: `72,789` → `72,776` (-13)
- Claim leaf 0 (ETH, under root(n = 3)) / bridge Issue: cycles: `47,201` → `48,249` (+1,048)
- Claim leaf 0 (ETH, under root(n = 3)) / bridge Spend: cycles: `30,916` → `31,725` (+809)
- Claim leaf 0 (ETH, under root(n = 3)) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 0 (ETH, under root(n = 3)) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 1 (token 0x1d1499…, under root(n = 3)) / bridge Issue: cycles: `45,889` → `46,937` (+1,048)
- Claim leaf 1 (token 0x1d1499…, under root(n = 3)) / bridge Spend: cycles: `30,916` → `31,725` (+809)
- Claim leaf 1 (token 0x1d1499…, under root(n = 3)) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 1 (token 0x1d1499…, under root(n = 3)) / bridge Mint: cycles: `23,017` → `23,617` (+600)
- Claim leaf 2 (ETH, under root(n = 3)) / bridge Issue: cycles: `47,166` → `48,214` (+1,048)
- Claim leaf 2 (ETH, under root(n = 3)) / bridge Spend: cycles: `30,916` → `31,725` (+809)
- Claim leaf 2 (ETH, under root(n = 3)) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 2 (ETH, under root(n = 3)) / bridge Mint: cycles: `23,965` → `24,565` (+600)
- Claim leaf 3 (ETH, under root(n = 6)) / bridge Issue: cycles: `51,535` → `52,583` (+1,048)
- Claim leaf 3 (ETH, under root(n = 6)) / bridge Spend: cycles: `37,291` → `38,243` (+952)
- Claim leaf 3 (ETH, under root(n = 6)) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 3 (ETH, under root(n = 6)) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 3 (ETH, under root(n = 6)) / bridge Mint: cycles: `23,965` → `24,565` (+600)
- Claim leaf 4 (token 0x1d1499…, under root(n = 6)) / bridge Issue: cycles: `45,905` → `46,953` (+1,048)
- Claim leaf 4 (token 0x1d1499…, under root(n = 6)) / bridge Spend: cycles: `30,916` → `31,725` (+809)
- Claim leaf 4 (token 0x1d1499…, under root(n = 6)) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 4 (token 0x1d1499…, under root(n = 6)) / bridge Mint: cycles: `24,212` → `24,812` (+600)
- Claim leaf 5 (ETH, under root(n = 6)) / bridge Issue: cycles: `51,586` → `52,634` (+1,048)
- Claim leaf 5 (ETH, under root(n = 6)) / bridge Spend: cycles: `37,291` → `38,243` (+952)
- Claim leaf 5 (ETH, under root(n = 6)) / bridge Mint: cycles: `18,161` → `18,761` (+600)
- Claim leaf 5 (ETH, under root(n = 6)) / bridge Mint: cycles: `22,496` → `23,096` (+600)
- Claim leaf 5 (ETH, under root(n = 6)) / bridge Mint: cycles: `23,965` → `24,565` (+600)
