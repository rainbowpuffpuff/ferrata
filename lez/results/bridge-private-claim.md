# The private claim (`harness/tests/bridge_private.rs`), RISC Zero dev mode

Bridge program image `25dfd9b283ba14b0b83a522d7d307de57288c4730f538d8862b53ad73a4a8c49`. `ClaimPrivately` at the destination's own private account, for vault A's leaf 5 (1 ETH, private kind) under `root(A, 8)`.

## The private transitions in the executor

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| ClaimPrivately (sends Read) | 1,288 | 2,605 | 39,314 | 1 | 131,072 | 0 |
| the root's reply (verifies the claim, sends Spend) | 1,546 | 2,011 | 113,417 | 1 | 262,144 | 0 |

## What LEZ charged

| execution gas | storage gas (tx bytes) | base fee | transaction bytes (dev mode: no real receipt) | proof bytes |
|---:|---:|---:|---:|---:|
| 439,815 | 224,063 | 5,311,024 | 8,815 | 4,464 |

## Public outputs (`harness::private::describe_public_outputs`)

public_context.actors (3): program 3Yr4SF8Kf9rAMWQ74g7Jq2U9PgrFSFqjwWbnRez4A3Vr@xbgTUuZj5NL5M5WoTVXhqmx468hoa5vYEeWya41WuWm, native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq, program 3Yr4SF8Kf9rAMWQ74g7Jq2U9PgrFSFqjwWbnRez4A3Vr@BtY4phRTr2qUsMd5wycWmtvHHQGtGWvUUfDyPQHsXg2k
public_context.authorized_accounts: {}
execution.public_root: None
execution.boundary (8 steps):
  - PrivateToPublic from program 3Yr4SF8Kf9rAMWQ74g7Jq2U9PgrFSFqjwWbnRez4A3Vr@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to program 3Yr4SF8Kf9rAMWQ74g7Jq2U9PgrFSFqjwWbnRez4A3Vr@xbgTUuZj5NL5M5WoTVXhqmx468hoa5vYEeWya41WuWm: 1 bytes (1 bytes)
  - PublicToPrivate from program 3Yr4SF8Kf9rAMWQ74g7Jq2U9PgrFSFqjwWbnRez4A3Vr@xbgTUuZj5NL5M5WoTVXhqmx468hoa5vYEeWya41WuWm to program 3Yr4SF8Kf9rAMWQ74g7Jq2U9PgrFSFqjwWbnRez4A3Vr@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL: StateReply { state: 185 bytes } (189 bytes)
  - PrivateToPublic from program 3Yr4SF8Kf9rAMWQ74g7Jq2U9PgrFSFqjwWbnRez4A3Vr@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to program 3Yr4SF8Kf9rAMWQ74g7Jq2U9PgrFSFqjwWbnRez4A3Vr@BtY4phRTr2qUsMd5wycWmtvHHQGtGWvUUfDyPQHsXg2k: 186 bytes (186 bytes, 1 PDA seed(s))
  - EndPublicSubtree
  - EndPrivateSubtree
  - EndPublicSubtree
  - PrivateToPublic from native@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq: Some(Credit(100000000)) (17 bytes)
  - EndPublicSubtree
execution.program_image_claims (1):
  - Disclosed { program: 3Yr4SF8Kf9rAMWQ74g7Jq2U9PgrFSFqjwWbnRez4A3Vr, image_id: 25dfd9b283ba14b0b83a522d7d307de57288c4730f538d8862b53ad73a4a8c49 }
execution.private_actions (1): each a nullifier, a commitment-tree root, a new commitment and an encrypted note (1419 bytes)
execution.async_messages: 0
execution.recovery_bindings: 0
execution.validity: ValidityWindows { blocks: ValidityWindow { from: None, to: None }, timestamps: ValidityWindow { from: None, to: None } }
execution.fee: Some("refund_to Some(78Skt7EQTvVwfWVYnA1XMjghv3aUR1V5wgQAfVLKJZG5), fallback true")
nonces: 0, admission evidence: 0


## Search

The 8815 serialized bytes of the transaction were searched for the claim's private inputs:
- the destination (the private account id) (32 bytes): absent
- the deposit secret s (32 bytes): absent
- the commitment C (32 bytes): absent
- the deposit leaf (32 bytes): absent
- the depositor (20 bytes): absent
- the borsh witness (1173 bytes): absent
- path sibling 0 (32 bytes): absent
- path sibling 1 (32 bytes): absent
- path sibling 2 (32 bytes): absent
- path sibling 3 (32 bytes): absent
- path sibling 4 (32 bytes): absent
- path sibling 5 (32 bytes): absent
- path sibling 6 (32 bytes): absent
- path sibling 7 (32 bytes): absent
- path sibling 8 (32 bytes): absent
- path sibling 9 (32 bytes): absent
- path sibling 10 (32 bytes): absent
- path sibling 11 (32 bytes): absent
- path sibling 12 (32 bytes): absent
- path sibling 13 (32 bytes): absent
- path sibling 14 (32 bytes): absent
- path sibling 15 (32 bytes): absent
- path sibling 16 (32 bytes): absent
- path sibling 17 (32 bytes): absent
- path sibling 18 (32 bytes): absent
- path sibling 19 (32 bytes): absent
- path sibling 20 (32 bytes): absent
- path sibling 21 (32 bytes): absent
- path sibling 22 (32 bytes): absent
- path sibling 23 (32 bytes): absent
- path sibling 24 (32 bytes): absent
- path sibling 25 (32 bytes): absent
- path sibling 26 (32 bytes): absent
- path sibling 27 (32 bytes): absent
- path sibling 28 (32 bytes): absent
- path sibling 29 (32 bytes): absent
- path sibling 30 (32 bytes): absent
- path sibling 31 (32 bytes): absent
- N (public by design): present
- configId (public by design): present
- the root read (public by design): present
