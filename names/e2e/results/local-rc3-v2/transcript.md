## rules (exit 0, expected 0)
```
$ ferrata-names rules
{
  "minLen": 3,
  "maxLen": 32,
  "charset": "a-z 0-9 -",
  "noLeadingTrailingHyphen": true,
  "noDoubleHyphen": true,
  "reserved": [
    "admin",
    "administrator",
    "bridge",
    "explorer",
    "faucet",
    "ferrata",
    "help",
    "lez",
    "logos",
    "null",
    "official",
    "root",
    "security",
    "sequencer",
    "support",
    "system",
    "treasury",
    "validator",
    "wallet"
  ],
  "reservedVariants": "any -separated part with a reserved skeleton, as it is or without its edge digits",
  "skeleton": "0→o, 1→l, then rn→m, then vv→w: one name per skeleton",
  "maxNamesPerAccount": 5,
  "termDays": 180,
  "graceDays": 30,
  "fee": "none on the test network",
  "clockSlackMs": 600000,
  "commitSlackMs": 120000,
  "revealFromMs": 240000,
  "revealWindowMs": 3600000,
  "contestMs": 3600000,
  "priority": "the oldest commitment wins"
}
```
## check Alice (normalised to alice) (exit 0, expected 0)
```
$ ferrata-names check Alice
{
  "name": "alice",
  "ok": true
}
```
## check a--b (refused offline) (exit 1, expected 1)
```
$ ferrata-names check a--b
{
  "name": "a--b",
  "ok": false,
  "code": 245,
  "reason": "names are 3-32 characters of a-z, 0-9 and -, not starting or ending with -, without --"
}
```
## check faucet1 (a variant of a reserved name, refused offline) (exit 1, expected 1)
```
$ ferrata-names check faucet1
{
  "name": "faucet1",
  "ok": false,
  "code": 246,
  "reason": "this name is reserved, or a variant of a reserved name"
}
```
## resolve alice before it exists (exit 1, expected 1)
```
$ ferrata-names resolve alice
null
```
## alice: commit alice (a hash; the name stays hidden) (exit 0, expected 0)
```
$ ferrata-names commit alice --key <run>/keys/alice.key --state <run>/alice.commit.json
(node:51323) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
commitment for alice kept in <run>/alice.commit.json: reveal from 2026-11-23T21:42:01.555Z until 2026-11-23T22:35:31.555Z
{
  "signer": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "action": "Commit",
  "name": null,
  "nowMs": 1795469851555,
  "hash": "cef96c6453d90a763568e5d864b22d5cfa39f28f4c7c133913d7ec7b4456223f",
  "bytes": 568,
  "gasLimit": 60000,
  "maxFee": "3876352",
  "block": 111,
  "result": "applied"
}
```
## alice: reveal at once (refused by the CLI: too early; nothing sent) (exit 1, expected 1)
```
$ ferrata-names reveal alice --key <run>/keys/alice.key --state <run>/alice.commit.json
ferrata-names: too early: the reveal window opens in 238 s; nothing sent
```
## alice: reveal at once with --force (ignored; nothing sent) (exit 1, expected 1)
```
$ ferrata-names reveal alice --force --key <run>/keys/alice.key --state <run>/alice.commit.json
ferrata-names: --force is ignored: a reveal that is refused or reverts publishes the name
ferrata-names: too early: the reveal window opens in 238 s; nothing sent
```
## alice: reveal sent early anyway (test-only switch): TOO_EARLY, reverted, the name is public (exit 1, expected 1)
```
$ ferrata-names reveal alice --key <run>/keys/alice.key --state <run>/alice.commit.json
ferrata-names: FERRATA_NAMES_UNSAFE_REVEAL=1: sending without checks (test only)
(node:51431) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "action": "Register",
  "name": "alice",
  "nowMs": 1795469854113,
  "hash": "5014d344a6911dcb0a1620be9d2ff93c6b3272d88e9299b0f4a29370e5288057",
  "bytes": 611,
  "gasLimit": 190000,
  "maxFee": "12199104",
  "block": 113,
  "result": "reverted (refused; the fee was charged)"
}
```
## mallory: commit alice with now_ms 105 s back (the copier) (exit 0, expected 0)
```
$ ferrata-names commit alice --key <run>/keys/mallory.key --state <run>/mallory-alice.commit.json
(node:51515) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
commitment for alice kept in <run>/mallory-alice.commit.json: reveal from 2026-11-23T21:40:51.758Z until 2026-11-23T22:34:21.758Z
{
  "signer": "CBax6DP6eQfnLCw8RwbQ2eNnrehs2RDjT6k3A6ZVPoLQ",
  "action": "Commit",
  "name": null,
  "nowMs": 1795469781758,
  "hash": "5a5428cadc94c27507cc1a45b11816da48ca407d687e999057dd8c9ce4ce4752",
  "bytes": 503,
  "gasLimit": 60000,
  "maxFee": "3872192",
  "block": 114,
  "result": "applied"
}
```
## mallory: reveal alice first (registered, provisional) (exit 0, expected 0)
```
$ ferrata-names reveal alice --key <run>/keys/mallory.key --state <run>/mallory-alice.commit.json
(node:53568) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "CBax6DP6eQfnLCw8RwbQ2eNnrehs2RDjT6k3A6ZVPoLQ",
  "action": "Register",
  "name": "alice",
  "nowMs": 1795470024376,
  "hash": "e9ed3a5081f912cb68b7df81bae1e0e22efe2eb843a6e4fb47ce0cf103e842c9",
  "bytes": 611,
  "gasLimit": 190000,
  "maxFee": "12199104",
  "block": 198,
  "result": "applied"
}
```
## after mallory's reveal, alice is mallory's: whois alice → alice CBax6DP6eQfnLCw8RwbQ2eNnrehs2RDjT6k3A6ZVPoLQ
## alice: reveal alice in her window (older commitment: Displaced) (exit 0, expected 0)
```
$ ferrata-names reveal alice --key <run>/keys/alice.key --state <run>/alice.commit.json
(node:54506) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "action": "Register",
  "name": "alice",
  "nowMs": 1795470094566,
  "hash": "0ff517def2d4703ddc388f803b10c1145439f4a7b23ad6a78db4516a33ace228",
  "bytes": 546,
  "gasLimit": 190000,
  "maxFee": "12194944",
  "block": 233,
  "result": "applied"
}
```
## after alice's reveal, alice is alice's: whois alice → alice GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH
## account mallory (alice still in her list) (exit 0, expected 0)
```
$ ferrata-names account CBax6DP6eQfnLCw8RwbQ2eNnrehs2RDjT6k3A6ZVPoLQ
{
  "account": "CBax6DP6eQfnLCw8RwbQ2eNnrehs2RDjT6k3A6ZVPoLQ",
  "names": [
    "alice"
  ],
  "primary": null,
  "commit": null
}
```
## mallory: forget (the names that left her) (exit 0, expected 0)
```
$ ferrata-names forget --key <run>/keys/mallory.key
(node:54617) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "CBax6DP6eQfnLCw8RwbQ2eNnrehs2RDjT6k3A6ZVPoLQ",
  "action": "Forget",
  "name": null,
  "nowMs": null,
  "hash": "56e892cede0f50acde372d00c1f1bd1ac8c8679fb74147f6e775b09174590201",
  "bytes": 476,
  "gasLimit": 92000,
  "maxFee": "5918464",
  "block": 234,
  "result": "applied"
}
```
## account mallory (empty list) (exit 0, expected 0)
```
$ ferrata-names account CBax6DP6eQfnLCw8RwbQ2eNnrehs2RDjT6k3A6ZVPoLQ
{
  "account": "CBax6DP6eQfnLCw8RwbQ2eNnrehs2RDjT6k3A6ZVPoLQ",
  "names": [],
  "primary": null,
  "commit": null
}
```
## resolve alice (exit 0, expected 0)
```
$ ferrata-names resolve alice
{
  "name": "alice",
  "owner": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "target": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "receiver": null,
  "registeredMs": 1795470094566,
  "expiresMs": 1811022094566,
  "offer": null,
  "commitSeq": 1,
  "status": "live",
  "expires": "2027-05-22T21:41:34.566Z",
  "provisional": true,
  "final": "2026-11-23T22:41:34.566Z"
}
```
## whois alice (exit 0, expected 0)
```
$ ferrata-names whois alice
{
  "name": "alice",
  "skeleton": "alice",
  "account": "272vw3Xq9BNrGCzcK62WxpVCMZjockERCNtu5P6tMtfP",
  "record": {
    "name": "alice",
    "owner": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
    "target": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
    "receiver": null,
    "registeredMs": 1795470094566,
    "expiresMs": 1811022094566,
    "offer": null,
    "commitSeq": 1,
    "status": "live",
    "expires": "2027-05-22T21:41:34.566Z",
    "provisional": true,
    "final": "2026-11-23T22:41:34.566Z"
  },
  "status": "live",
  "lookAlike": null
}
```
## alice: set-primary alice (exit 0, expected 0)
```
$ ferrata-names set-primary alice --key <run>/keys/alice.key
(node:54745) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "action": "SetPrimary",
  "name": "alice",
  "nowMs": 1795470099900,
  "hash": "a4011683e1ed99ef7d223649b2f63c2c93cdf8c6ece40d8cb730aadc0882ba77",
  "bytes": 481,
  "gasLimit": 90000,
  "maxFee": "5790784",
  "block": 235,
  "result": "applied"
}
```
## reverse alice's account (exit 0, expected 0)
```
$ ferrata-names reverse GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH
{
  "account": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "name": "alice"
}
```
## bob reveals alice's commitment with his key (refused by the CLI: no commitment; nothing sent) (exit 1, expected 1)
```
$ ferrata-names reveal alice --key <run>/keys/bob.key --state <run>/alice.commit.json
(node:54861) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
ferrata-names: not revealing (nothing sent): 230 commit first: this account has no pending commitment; 247 this name is taken
```
## bob registers alice: commit, wait, reveal (refused by the CLI: taken; the reveal not sent) (exit 1, expected 1)
```
$ ferrata-names register alice --key <run>/keys/bob.key --state <run>/bob-alice.commit.json
(node:54877) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
commitment for alice kept in <run>/bob-alice.commit.json: reveal from 2026-11-23T21:46:12.714Z until 2026-11-23T22:39:42.714Z
{
  "signer": "2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566",
  "action": "Commit",
  "name": null,
  "nowMs": 1795470102714,
  "hash": "6daeb18c17a013ee2d4d9bdfc9fb4bcffd8a53f06b0260d33b69fada3fce5903",
  "bytes": 503,
  "gasLimit": 60000,
  "maxFee": "3872192",
  "block": 237,
  "result": "applied"
}
waiting 239 s for the reveal window (front-running protection)
ferrata-names: not revealing (nothing sent): 247 this name is taken
```
## bob offers alice to himself (not owner: reverted on chain) (exit 1, expected 1)
```
$ ferrata-names transfer alice 2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566 --key <run>/keys/bob.key
(node:57805) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566",
  "action": "Transfer",
  "name": "alice",
  "nowMs": 1795470343954,
  "hash": "81c502cb34b340745031e8c622e84e1b973207d6ba70da0c9240414ee0d8490d",
  "bytes": 512,
  "gasLimit": 70000,
  "maxFee": "4512768",
  "block": 357,
  "result": "reverted (refused; the fee was charged)"
}
```
## alice: set-target alice, add a private receiver (exit 0, expected 0)
```
$ ferrata-names set-target alice --receiver-keys <run>/receiver.keys --key <run>/keys/alice.key
(node:57898) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "action": "SetTarget",
  "name": "alice",
  "nowMs": 1795470346580,
  "hash": "590b70cabc309c5025362ac98160017bd8160024a4c72f4879f304fe42771bc0",
  "bytes": 1734,
  "gasLimit": 100000,
  "maxFee": "6510976",
  "block": 359,
  "result": "applied"
}
```
## resolve alice (with receiver) (exit 0, expected 0)
```
$ ferrata-names resolve alice
{
  "name": "alice",
  "owner": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "target": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "receiver": {
    "npk": "1111111111111111111111111111111111111111111111111111111111111111",
    "vpk": "2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"
  },
  "registeredMs": 1795470094566,
  "expiresMs": 1811022094566,
  "offer": null,
  "commitSeq": 1,
  "status": "live",
  "expires": "2027-05-22T21:41:34.566Z",
  "provisional": true,
  "final": "2026-11-23T22:41:34.566Z"
}
```
## alice: renew alice (exit 0, expected 0)
```
$ ferrata-names renew alice --key <run>/keys/alice.key
(node:57986) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "action": "Renew",
  "name": "alice",
  "nowMs": 1795470349046,
  "hash": "8930ccea3787dd8f18fac18770b15e0e0a32a451e1d12ee63c877158a15289f6",
  "bytes": 480,
  "gasLimit": 70000,
  "maxFee": "4510720",
  "block": 360,
  "result": "applied"
}
```
## alice: offer alice to bob (transfer) (exit 0, expected 0)
```
$ ferrata-names transfer alice 2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566 --key <run>/keys/alice.key
(node:58058) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "action": "Transfer",
  "name": "alice",
  "nowMs": 1795470351336,
  "hash": "47f4f0dcffa7bdbb46b92d5b612f73c5fb645f494a2fa3c9fd835b831615da30",
  "bytes": 512,
  "gasLimit": 70000,
  "maxFee": "4512768",
  "block": 361,
  "result": "applied"
}
```
## resolve alice (still alice's, offer pending) (exit 0, expected 0)
```
$ ferrata-names resolve alice
{
  "name": "alice",
  "owner": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "target": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "receiver": {
    "npk": "1111111111111111111111111111111111111111111111111111111111111111",
    "vpk": "2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"
  },
  "registeredMs": 1795470094566,
  "expiresMs": 1811022349046,
  "offer": "2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566",
  "commitSeq": 1,
  "status": "live",
  "expires": "2027-05-22T21:45:49.046Z",
  "provisional": true,
  "final": "2026-11-23T22:41:34.566Z"
}
```
## bob: accept alice (exit 0, expected 0)
```
$ ferrata-names accept alice --key <run>/keys/bob.key
(node:58146) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566",
  "action": "Accept",
  "name": "alice",
  "nowMs": 1795470353738,
  "hash": "876a0ca971a854e0d248550d865e51d4b7bda6ea7d6541ecde0c8b2896b33a49",
  "bytes": 480,
  "gasLimit": 90000,
  "maxFee": "5790720",
  "block": 362,
  "result": "applied"
}
```
## after bob's accept, alice is bob's: whois alice → alice 2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566
## reverse alice's account (her stored primary no longer points at her) (exit 1, expected 1)
```
$ ferrata-names reverse GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH
{
  "account": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "name": null
}
```
## account alice (alice still in her list) (exit 0, expected 0)
```
$ ferrata-names account GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH
{
  "account": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "names": [
    "alice"
  ],
  "primary": "alice",
  "commit": null
}
```
## alice: forget (the names that left her) (exit 0, expected 0)
```
$ ferrata-names forget --key <run>/keys/alice.key
(node:58297) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "GdC4CYSrueW2oXTBSPr8bA6s3jWinFcdBXA7kQqPp9NH",
  "action": "Forget",
  "name": null,
  "nowMs": null,
  "hash": "1f6091fb15ec51ac2c82a5b078dac90147636b4fc1fa8f95d97ed528738e63e2",
  "bytes": 476,
  "gasLimit": 92000,
  "maxFee": "5918464",
  "block": 364,
  "result": "applied"
}
```
## bob: set-primary alice (exit 0, expected 0)
```
$ ferrata-names set-primary alice --key <run>/keys/bob.key
(node:58377) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566",
  "action": "SetPrimary",
  "name": "alice",
  "nowMs": 1795470358931,
  "hash": "efd8340560353671a1f9f1ed318aee55fafe57f549732678990988b78adc9667",
  "bytes": 481,
  "gasLimit": 90000,
  "maxFee": "5790784",
  "block": 365,
  "result": "applied"
}
```
## reverse bob's account (exit 0, expected 0)
```
$ ferrata-names reverse 2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566
{
  "account": "2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566",
  "name": "alice"
}
```
## bob: release alice (exit 0, expected 0)
```
$ ferrata-names release alice --key <run>/keys/bob.key
(node:58473) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
{
  "signer": "2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566",
  "action": "Release",
  "name": "alice",
  "nowMs": null,
  "hash": "ab9de68c9d05d968a6c89a235f439fadba1e137ae1666c63c6fc821db60c6ea4",
  "bytes": 472,
  "gasLimit": 90000,
  "maxFee": "5790208",
  "block": 366,
  "result": "applied"
}
```
## resolve alice after release (exit 1, expected 1)
```
$ ferrata-names resolve alice
null
```
## bob: register bob (commit, wait, reveal) (exit 0, expected 0)
```
$ ferrata-names register bob --key <run>/keys/bob.key --state <run>/bob.commit.json
(node:58565) [MODULE_TYPELESS_PACKAGE_JSON] Warning: Module type of file://<repo>/deploy/testnet/web/wallet/vendor/noble.js is not specified and it doesn't parse as CommonJS.
Reparsing as ES module because module syntax was detected. This incurs a performance overhead.
To eliminate this warning, add "type": "module" to <repo>/deploy/testnet/web/wallet/vendor/package.json.
(Use `node --trace-warnings ...` to show where the warning was created)
commitment for bob kept in <run>/bob.commit.json: reveal from 2026-11-23T21:50:33.687Z until 2026-11-23T22:44:03.687Z
{
  "signer": "2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566",
  "action": "Commit",
  "name": null,
  "nowMs": 1795470363687,
  "hash": "28987c08b2a0aab62d05bc83d713d49b7dac05d23efb9ff0d0695027be5fbd6c",
  "bytes": 503,
  "gasLimit": 60000,
  "maxFee": "3872192",
  "block": 367,
  "result": "applied"
}
waiting 239 s for the reveal window (front-running protection)
{
  "signer": "2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566",
  "action": "Register",
  "name": "bob",
  "nowMs": 1795470604700,
  "hash": "abde150498c3a7ef9610151722dbcdbc251e6fe2eb069ab30b21c1a13ee16dd1",
  "bytes": 609,
  "gasLimit": 190000,
  "maxFee": "12198976",
  "block": 488,
  "result": "applied"
}
```
## whois b0b (digit zero: a look-alike of bob is registered) (exit 0, expected 0)
```
$ ferrata-names whois b0b
{
  "name": "b0b",
  "skeleton": "bob",
  "account": "A6kPUj7ky26Fdo78CBEPU7kY9kvoYwwPkw9ZHbnCHK5r",
  "record": {
    "name": "bob",
    "owner": "2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566",
    "target": "2bp4KXq3u6LyeXnHB99nfdCSyU5NByBaeRxnvoDxi566",
    "receiver": null,
    "registeredMs": 1795470604700,
    "expiresMs": 1811022604700,
    "offer": null,
    "commitSeq": 4,
    "status": "live",
    "expires": "2027-05-22T21:50:04.700Z",
    "provisional": true,
    "final": "2026-11-23T22:50:04.700Z"
  },
  "status": "look-alike registered",
  "lookAlike": "bob"
}
```
## resolve b0b (not registered: a look-alike holds its address) (exit 1, expected 1)
```
$ ferrata-names resolve b0b
null
```
## faucet is reserved (refused before sending) (exit 1, expected 1)
```
$ ferrata-names register faucet --key <run>/keys/bob.key
ferrata-names: faucet: this name is reserved, or a variant of a reserved name
```
