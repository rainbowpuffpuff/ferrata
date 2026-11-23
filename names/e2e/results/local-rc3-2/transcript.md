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
  "maxNamesPerAccount": 5,
  "termDays": 180,
  "graceDays": 30,
  "fee": "none on the test network",
  "clockSlackMs": 600000
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
## resolve alice before it exists (exit 1, expected 1)
```
$ ferrata-names resolve alice
null
```
## commit alice (a hash; the name stays hidden) (exit 0, expected 0)
```
$ ferrata-names commit alice --key <run>/keys/alice.key
commitment for alice kept in alice.commit.json: reveal from 2026-11-23T10:49:01.124Z until 2026-11-24T10:44:31.124Z
{
  "signer": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "action": "Commit",
  "name": null,
  "nowMs": 1795430671124,
  "hash": "483616d98b0bea0368f28854d7bf8e529f1105d378d2835009f58109aba6571d",
  "bytes": 439,
  "gasLimit": 200000,
  "maxFee": "12828096",
  "block": 97,
  "result": "applied"
}
```
## reveal alice at once (refused locally: too early) (exit 1, expected 1)
```
$ ferrata-names reveal alice --key <run>/keys/alice.key
ferrata-names: too early: the reveal window opens in 238 s
```
## reveal alice at once, sent anyway (TOO_EARLY: reverted) (exit 1, expected 1)
```
$ ferrata-names reveal alice --force --key <run>/keys/alice.key
{
  "signer": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "action": "Register",
  "name": "alice",
  "nowMs": 1795430673864,
  "hash": "ff2726d6ebc8996c5d72c54b2110e4e484897a2a52f5e6d7f5f6fbf8b93b037e",
  "bytes": 611,
  "gasLimit": 200000,
  "maxFee": "12839104",
  "block": 99,
  "result": "reverted (refused; the fee was charged)"
}
```
## resolve alice after the refused reveal (exit 1, expected 1)
```
$ ferrata-names resolve alice
null
```
## reveal alice (registers it) (exit 0, expected 0)
```
$ ferrata-names reveal alice --key <run>/keys/alice.key
{
  "signer": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "action": "Register",
  "name": "alice",
  "nowMs": 1795430927263,
  "hash": "860b62106d26b63fa11618b96bcb57898aecf4e894ab317dffe7f1334abe317d",
  "bytes": 611,
  "gasLimit": 200000,
  "maxFee": "12839104",
  "block": 226,
  "result": "applied"
}
```
## resolve alice (exit 0, expected 0)
```
$ ferrata-names resolve alice
{
  "name": "alice",
  "owner": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "target": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "receiver": null,
  "registeredMs": 1795430927263,
  "expiresMs": 1810982927263,
  "offer": null,
  "status": "live",
  "expires": "2027-05-22T10:48:47.263Z"
}
```
## whois alice (exit 0, expected 0)
```
$ ferrata-names whois alice
{
  "name": "alice",
  "account": "5n51tZJvTAD9ZDzjKoEmN8S2DHt58Yt8b45TuiWB7GnP",
  "record": {
    "name": "alice",
    "owner": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
    "target": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
    "receiver": null,
    "registeredMs": 1795430927263,
    "expiresMs": 1810982927263,
    "offer": null,
    "status": "live",
    "expires": "2027-05-22T10:48:47.263Z"
  },
  "status": "live"
}
```
## set-primary alice (exit 0, expected 0)
```
$ ferrata-names set-primary alice --key <run>/keys/alice.key
{
  "signer": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "action": "SetPrimary",
  "name": "alice",
  "nowMs": 1795430932248,
  "hash": "8a7dafb3dd566af218ebbce9c974bc3cf04e26b02c1d5e16f53f75f9bc9ad140",
  "bytes": 481,
  "gasLimit": 200000,
  "maxFee": "12830784",
  "block": 228,
  "result": "applied"
}
```
## reverse alice's account (exit 0, expected 0)
```
$ ferrata-names reverse 3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q
{
  "account": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "name": "alice"
}
```
## bob reveals alice's commitment with his key (NO_COMMIT: reverted) (exit 1, expected 1)
```
$ ferrata-names reveal alice --force --key <run>/keys/bob.key --state <run>/alice.commit.json
{
  "signer": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "action": "Register",
  "name": "alice",
  "nowMs": 1795430935360,
  "hash": "c0911a5b0d7b11b68992ac51a5816ef878eec8123640d5c95395785f3dce34ce",
  "bytes": 610,
  "gasLimit": 200000,
  "maxFee": "12839040",
  "block": 230,
  "result": "reverted (refused; the fee was charged)"
}
```
## bob registers alice: commit, wait, reveal (TAKEN: reverted) (exit 1, expected 1)
```
$ ferrata-names register alice --key <run>/keys/bob.key --state <run>/bob-alice.commit.json
commitment for alice kept in <run>/bob-alice.commit.json: reveal from 2026-11-23T10:53:28.595Z until 2026-11-24T10:48:58.595Z
{
  "signer": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "action": "Commit",
  "name": null,
  "nowMs": 1795430938595,
  "hash": "e5323dd1c80b9ed31922a70180e5e68946a37cb0fd0d06b894ce2de4a4581100",
  "bytes": 439,
  "gasLimit": 200000,
  "maxFee": "12828096",
  "block": 231,
  "result": "applied"
}
waiting 239 s for the reveal window (front-running protection)
{
  "signer": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "action": "Register",
  "name": "alice",
  "nowMs": 1795431179598,
  "hash": "e1fdb164742f9a0ab5028abd64cc7f2e39f1d34d7f8fe64e445b0aba4edc4242",
  "bytes": 610,
  "gasLimit": 200000,
  "maxFee": "12839040",
  "block": 352,
  "result": "reverted (refused; the fee was charged)"
}
```
## bob transfers alice (not owner: reverted) (exit 1, expected 1)
```
$ ferrata-names transfer alice 4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1 --key <run>/keys/bob.key
{
  "signer": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "action": "Transfer",
  "name": "alice",
  "nowMs": 1795431182695,
  "hash": "28078a6f2d1b95b0818e78d68675d068ae87e844e92dfc782b487f54f367d82a",
  "bytes": 512,
  "gasLimit": 200000,
  "maxFee": "12832768",
  "block": 353,
  "result": "reverted (refused; the fee was charged)"
}
```
## set-target alice: add a private receiver (exit 0, expected 0)
```
$ ferrata-names set-target alice --receiver-keys <run>/receiver.keys --key <run>/keys/alice.key
{
  "signer": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "action": "SetTarget",
  "name": "alice",
  "nowMs": 1795431185512,
  "hash": "6eed29935e69a5bd22b1c1a3ca588137f843e2b1a56c08951bd8a735f6252ca9",
  "bytes": 1734,
  "gasLimit": 200000,
  "maxFee": "12910976",
  "block": 355,
  "result": "applied"
}
```
## resolve alice (with receiver) (exit 0, expected 0)
```
$ ferrata-names resolve alice
{
  "name": "alice",
  "owner": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "target": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "receiver": {
    "npk": "1111111111111111111111111111111111111111111111111111111111111111",
    "vpk": "2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"
  },
  "registeredMs": 1795430927263,
  "expiresMs": 1810982927263,
  "offer": null,
  "status": "live",
  "expires": "2027-05-22T10:48:47.263Z"
}
```
## renew alice (exit 0, expected 0)
```
$ ferrata-names renew alice --key <run>/keys/alice.key
{
  "signer": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "action": "Renew",
  "name": "alice",
  "nowMs": 1795431189400,
  "hash": "4f4638f1cc8eff03db45f002f15a65548f7deed04a21bcb45bb5156ecdffc95b",
  "bytes": 480,
  "gasLimit": 200000,
  "maxFee": "12830720",
  "block": 357,
  "result": "applied"
}
```
## offer alice to bob (transfer) (exit 0, expected 0)
```
$ ferrata-names transfer alice 4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1 --key <run>/keys/alice.key
{
  "signer": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "action": "Transfer",
  "name": "alice",
  "nowMs": 1795431192756,
  "hash": "a1b1874801e8d7695c7feb61dae5615721e33eb4e51801be0622a6625d8dadee",
  "bytes": 512,
  "gasLimit": 200000,
  "maxFee": "12832768",
  "block": 358,
  "result": "applied"
}
```
## resolve alice (still alice's, offer pending) (exit 0, expected 0)
```
$ ferrata-names resolve alice
{
  "name": "alice",
  "owner": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "target": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "receiver": {
    "npk": "1111111111111111111111111111111111111111111111111111111111111111",
    "vpk": "2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"
  },
  "registeredMs": 1795430927263,
  "expiresMs": 1810983189400,
  "offer": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "status": "live",
  "expires": "2027-05-22T10:53:09.400Z"
}
```
## bob: accept alice (exit 0, expected 0)
```
$ ferrata-names accept alice --key <run>/keys/bob.key
{
  "signer": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "action": "Accept",
  "name": "alice",
  "nowMs": 1795431196249,
  "hash": "a19a1429bf7dcf4dad9220b16c9d38a952f33b16f78839cb766f53e8dc88eab3",
  "bytes": 544,
  "gasLimit": 200000,
  "maxFee": "12834816",
  "block": 360,
  "result": "applied"
}
```
## resolve alice (bob's now) (exit 0, expected 0)
```
$ ferrata-names resolve alice
{
  "name": "alice",
  "owner": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "target": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "receiver": null,
  "registeredMs": 1795430927263,
  "expiresMs": 1810983189400,
  "offer": null,
  "status": "live",
  "expires": "2027-05-22T10:53:09.400Z"
}
```
## reverse alice's account (cleared by the transfer) (exit 1, expected 1)
```
$ ferrata-names reverse 3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q
{
  "account": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "name": null
}
```
## account alice (exit 0, expected 0)
```
$ ferrata-names account 3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q
{
  "account": "3HHKTdnvuHG9NZfoBcspLaaa5QKuePFnAaZqpgMLYb9Q",
  "owned": 0,
  "primary": null,
  "commit": null
}
```
## bob: set-primary alice (exit 0, expected 0)
```
$ ferrata-names set-primary alice --key <run>/keys/bob.key
{
  "signer": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "action": "SetPrimary",
  "name": "alice",
  "nowMs": 1795431201206,
  "hash": "02d5b7d3aa7725184ebeca3a103a83da9157efd2895a4318ee469a9de553630e",
  "bytes": 481,
  "gasLimit": 200000,
  "maxFee": "12830784",
  "block": 362,
  "result": "applied"
}
```
## reverse bob's account (exit 0, expected 0)
```
$ ferrata-names reverse 4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1
{
  "account": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "name": "alice"
}
```
## bob: release alice (exit 0, expected 0)
```
$ ferrata-names release alice --key <run>/keys/bob.key
{
  "signer": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "action": "Release",
  "name": "alice",
  "nowMs": null,
  "hash": "57509eaecb254e94acdfd8fa24b92427b3cfe5adb6175262c6f95f9dd4d8b53b",
  "bytes": 472,
  "gasLimit": 200000,
  "maxFee": "12830208",
  "block": 364,
  "result": "applied"
}
```
## resolve alice after release (exit 1, expected 1)
```
$ ferrata-names resolve alice
null
```
## bob: register bob (exit 0, expected 0)
```
$ ferrata-names register bob --key <run>/keys/bob.key
commitment for bob kept in bob.commit.json: reveal from 2026-11-23T10:57:58.272Z until 2026-11-24T10:53:28.272Z
{
  "signer": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "action": "Commit",
  "name": null,
  "nowMs": 1795431208272,
  "hash": "f3fe0d42cc71e387ab446155dcfb30a2c4156fbc7b0425cf6a0f052eb67d9c70",
  "bytes": 439,
  "gasLimit": 200000,
  "maxFee": "12828096",
  "block": 366,
  "result": "applied"
}
waiting 239 s for the reveal window (front-running protection)
{
  "signer": "4Rb5Aawr1ZjLtFcZR7cMrFwcuap5bwoZ42pByzTTfEd1",
  "action": "Register",
  "name": "bob",
  "nowMs": 1795431449275,
  "hash": "c547e77e9e926a8408f5c805608ae72549cc96e64d0453f3fe856e38f0e3037d",
  "bytes": 609,
  "gasLimit": 200000,
  "maxFee": "12838976",
  "block": 486,
  "result": "applied"
}
```
## faucet is reserved (refused before sending) (exit 1, expected 1)
```
$ ferrata-names register faucet --key <run>/keys/bob.key
ferrata-names: faucet: this name is reserved
```
