| group | check | ok | detail |
|---|---|---|---|
| devnet | two deposits to the user's private account /0 | yes | amount 100000000000000000 wei each, vault 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9 |
| import_root | import_root dry-runs the import with the programs' functions: importable, root(cid, 2) as the vault holds it | yes | record slot 9000064, message 1231 bytes |
| import_root | the call names config(cid) (root, its seed), header(id, slot) and root(cid, n) | yes |  |
| import_root | a wrong leaf count: the dry run refuses with IMPORT_MISMATCH (247) | yes | IMPORT_MISMATCH |
| import_root | submitted, the program refuses it with the same code | yes | exit 247 |
| import_root | the module's ImportRoot is applied on the zone | yes | gas_exec 231020, gas_stor 1757 |
| import_root | (stand-in) the relayer's ImportFees for the same block | yes |  |
| registry | token(cid, ETH) decoded: the deposit's amount is a denomination | yes | denominations ['100000000000000000', '1000000000000000000', '10000000000000000000'], relay fees ['100000000000000', '1000000000000000', '10000000000000000'] |
| registry | a claim of leaf 0: range from the token's schedules, mint and relay fees | yes | range {'from_leaf': 0, 'index': 0, 'until_leaf': 2}, claim {'denomination_index': 0, 'mint_fee': '0', 'relay_fee': '100000000000000', 'value': '100000000'} |
| registry | config(cid) decoded; configId recomputed from it equals the devnet's | yes | instance 0x1d1d1d1d1d1d1d1d… |
| registry | root(cid, 2) decoded | yes |  |
| claim (private) | deposit 0: the tool's claim (ferrata-wallet): claimable, ClaimPrivately, root = the private account's own actor | yes | 1239 bytes, 6 accounts, mint fee 0 |
| claim (private) | deposit 0: proven and settled privately on the zone | yes | proof dev, prove 930 ms, balance 100000000000000000 |
| claim (private) | deposit 1: the tool's claim (ferrata-wallet): claimable, ClaimPrivately, root = the private account's own actor | yes | 1239 bytes, 6 accounts, mint fee 0 |
| claim (private) | deposit 1: proven and settled privately on the zone | yes | proof dev, prove 932 ms, balance 200000000000000000 |
| claim (private) | deposit 0 again: refused from the nullifier's account (ALREADY_CLAIMED, 250) | yes |  |
| claim (private) | the private account holds both denominations | yes | balance 200000000000000000 |
| release (the wallet's rule) | two accepted roots: over burn 0 (delay passed) and over both (in its delay) | yes | now 1794524943, opens [1794524162, 1794525063] |
| release (the wallet's rule) | burn 0: the newest root whose delay has passed is the older one | yes | picked checkpoint 0, root 0x89409baec579a74d… |
| release (the wallet's rule) | burn 1: only the newer root covers it, in its delay: ReleaseDelayNotElapsed, with when it opens | yes | first open 1794525063 |
| release (the wallet's rule) | after the delay: burn 1 under the newer root | yes |  |
| prove_release | the release guest run on the module's input through r0vm: its journal is the native one | yes | image 0xb64900bcf7416685…, dev mode True, 64 ms, receipt 2344 bytes |
| vault | both releases the module planned are accepted by the vault (mock verifier on the exact journals) | yes | paid 100000000000000000 and 100000000000000000 wei |
| status | both burns released | yes |  |

24 of 24 checks passed.
