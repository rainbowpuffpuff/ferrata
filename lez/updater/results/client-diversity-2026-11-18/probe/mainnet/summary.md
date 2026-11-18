# Client diversity of public light-client data: probe

Started 2026-11-18T16:06:09Z; 20 rounds, 12.0 s apart; User-Agent `ferrata-updater`; read-only GETs.

## mainnet

| endpoint | `/eth/v1/node/version` | client | `finality_update` | layout |
|---|---|---|---|---|
| `http://testing.mainnet.beacon-api.nimbus.team` | 200: `Nimbus/v26.10.2-0abc0d-stateofus` | nimbus | 200 (2097 bytes) | electra |
| `http://unstable.mainnet.beacon-api.nimbus.team` | 200: `Nimbus/v26.10.2-3a36af-stateofus` | nimbus | 200 (2097 bytes) | electra |
| `https://lodestar-mainnet.chainsafe.io` | 200: `Lodestar/v1.51.0/7923280` | lodestar | 200 (2097 bytes) | electra |
| `https://ethereum-beacon-api.publicnode.com` | 200: `Lighthouse/v8.3.3-57c2abc/x86_64-linux` | lighthouse | 200 | None |

Finality updates asked of every endpoint at the same moment:

| round | `testing.mainnet.beacon-api.nimbus.team` (nimbus) | `unstable.mainnet.beacon-api.nimbus.team` (nimbus) | `lodestar-mainnet.chainsafe.io` (lodestar) | `ethereum-beacon-api.publicnode.com` (lighthouse) | verdict |
|---|---|---|---|---|---|
| 0 | 46c0afdf4148 att 15682827 sig 15682828 fin 15682752 (510) | 46c0afdf4148 att 15682827 sig 15682828 fin 15682752 (510) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 1 | 64c951e56cf4 att 15682828 sig 15682829 fin 15682752 (511) | 64c951e56cf4 att 15682828 sig 15682829 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 2 | 7ad21e1f518c att 15682829 sig 15682830 fin 15682752 (511) | 7ad21e1f518c att 15682829 sig 15682830 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 3 | 2ee7a83bc27b att 15682831 sig 15682832 fin 15682752 (511) | 2ee7a83bc27b att 15682831 sig 15682832 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 4 | 498b6fbb506f att 15682832 sig 15682833 fin 15682752 (511) | 498b6fbb506f att 15682832 sig 15682833 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 5 | f26f9488c3e9 att 15682833 sig 15682834 fin 15682752 (511) | f26f9488c3e9 att 15682833 sig 15682834 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 6 | 61d53d45f51c att 15682834 sig 15682835 fin 15682752 (510) | 61d53d45f51c att 15682834 sig 15682835 fin 15682752 (510) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 7 | e7007434a54d att 15682835 sig 15682836 fin 15682752 (511) | e7007434a54d att 15682835 sig 15682836 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 8 | e76f16b50e52 att 15682836 sig 15682837 fin 15682752 (511) | e76f16b50e52 att 15682836 sig 15682837 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 9 | bdd34eb70aec att 15682837 sig 15682838 fin 15682752 (511) | bdd34eb70aec att 15682837 sig 15682838 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 10 | 47cf2c701f7f att 15682838 sig 15682839 fin 15682752 (509) | 47cf2c701f7f att 15682838 sig 15682839 fin 15682752 (509) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 11 | 8a4aeff2e025 att 15682839 sig 15682840 fin 15682752 (511) | 8a4aeff2e025 att 15682839 sig 15682840 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 12 | 705021ba1707 att 15682840 sig 15682841 fin 15682752 (510) | 705021ba1707 att 15682840 sig 15682841 fin 15682752 (510) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 13 | 0500c02c0ef9 att 15682841 sig 15682842 fin 15682752 (511) | ae403a0deb15 att 15682842 sig 15682843 fin 15682752 (509) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 14 | c6dead4978bf att 15682843 sig 15682844 fin 15682752 (511) | c6dead4978bf att 15682843 sig 15682844 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 15 | 84ab438c23a4 att 15682844 sig 15682845 fin 15682752 (511) | 84ab438c23a4 att 15682844 sig 15682845 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 16 | f3e188d05df3 att 15682845 sig 15682846 fin 15682752 (511) | f3e188d05df3 att 15682845 sig 15682846 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 17 | 0fd3391baf7b att 15682846 sig 15682847 fin 15682752 (510) | 0fd3391baf7b att 15682846 sig 15682847 fin 15682752 (510) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 18 | 789e4e83c468 att 15682848 sig 15682849 fin 15682784 (510) | e15c6bcdb813 att 15682847 sig 15682848 fin 15682752 (511) | be04b506eb57 att 15682817 sig 15682818 fin 15682752 (511) | ? att None sig None fin None (None) | different moment |
| 19 | 59a25c90c9ce att 15682849 sig 15682850 fin 15682784 (510) | 59a25c90c9ce att 15682849 sig 15682850 fin 15682784 (510) | 789e4e83c468 att 15682848 sig 15682849 fin 15682784 (510) | ? att None sig None fin None (None) | different moment |

Rounds: identical 0, different moment 20, DISAGREE 0, incomplete 0

Period updates `updates?start_period=1910&count=5` (head period 1914; periods below it are finished):

| period | `testing.mainnet.beacon-api.nimbus.team` | `unstable.mainnet.beacon-api.nimbus.team` | `lodestar-mainnet.chainsafe.io` | `ethereum-beacon-api.publicnode.com` | verdict |
|---|---|---|---|---|---|
| 1910 | 161200b83044 att 15646786 sig 15646787 (512) | 161200b83044 att 15646786 sig 15646787 (512) | 161200b83044 att 15646786 sig 15646787 (512) | — | identical |
| 1911 | a4fa59c1bdbd att 15655016 sig 15655017 (509) | a4fa59c1bdbd att 15655016 sig 15655017 (509) | a4fa59c1bdbd att 15655016 sig 15655017 (509) | — | identical |
| 1912 | c9814c110332 att 15663315 sig 15663316 (511) | c9814c110332 att 15663315 sig 15663316 (511) | c9814c110332 att 15663315 sig 15663316 (511) | — | identical |
| 1913 | 3edba55a7cba att 15672590 sig 15672591 (511) | 3edba55a7cba att 15672590 sig 15672591 (511) | 3edba55a7cba att 15672590 sig 15672591 (511) | — | identical |
| 1914 | 514a3f8b89de att 15679593 sig 15679594 (511) | 514a3f8b89de att 15679593 sig 15679594 (511) | 514a3f8b89de att 15679593 sig 15679594 (511) | — | identical |

Ended 2026-11-18T16:10:41Z.
