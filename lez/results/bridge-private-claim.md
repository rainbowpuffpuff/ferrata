# The private claim (`harness/tests/bridge_private.rs`), RISC Zero dev mode

Bridge program image `25b3cc900f38579ad530335b994e3ae327ac600afe335b734bd00af78309e776`. `ClaimPrivately` at the destination's own private account, for vault A's leaf 5 (1 ETH, private kind) under `root(A, 8)`.

## The private transitions in the executor

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| ClaimPrivately (sends Read) | 1,345 | 2,719 | 41,736 | 1 | 131,072 | 0 |
| the root's reply (verifies the claim, sends Issue) | 1,535 | 1,873 | 110,862 | 1 | 262,144 | 0 |

## What LEZ charged

| execution gas | storage gas (tx bytes) | base fee | transaction bytes (dev mode: no real receipt) | proof bytes |
|---:|---:|---:|---:|---:|
| 553,641 | 224,063 | 6,221,632 | 8,937 | 4,525 |

## Public outputs (`harness::private::describe_public_outputs`)

public_context.actors (7): program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@4HvPdzuQS3tRW3dTM6oPPSouwxhH58axcZyNn3aQr47C, program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@5Xcb9cZAqwcP7JQvvKYhLk2jvP7WVQXkPeptyR2yZewz, program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@7RRVtPpN9zKk8yrt2ScgLgK8uVGqgR5XG215Ucn7v6aa, native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq, program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@8pUaUFd9oEyXYmazzjDGCcfX3uYB9KkCTAYmVB6bGdCA, program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@CPGUnKEMVbC89NvxnvHjXrXWpTTUaSoee5oFUuHuHCNv, program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@EQtygdv2PtEvVXM9bUxFqu7siyczHbD59DrwRjTaUPU7
public_context.authorized_accounts: {}
execution.public_root: None
execution.boundary (8 steps):
  - PrivateToPublic from program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@EQtygdv2PtEvVXM9bUxFqu7siyczHbD59DrwRjTaUPU7: 1 bytes (1 bytes)
  - PublicToPrivate from program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@EQtygdv2PtEvVXM9bUxFqu7siyczHbD59DrwRjTaUPU7 to program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL: StateReply { state: 117 bytes } (121 bytes)
  - PrivateToPublic from program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to program 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj@CPGUnKEMVbC89NvxnvHjXrXWpTTUaSoee5oFUuHuHCNv: 167 bytes (167 bytes)
  - EndPublicSubtree
  - EndPrivateSubtree
  - EndPublicSubtree
  - PrivateToPublic from native@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq: Some(Credit(100000000)) (17 bytes)
  - EndPublicSubtree
execution.program_image_claims (1):
  - Disclosed { program: 3YB6oVrDLwwF3ZxubaMx5qmmfxoxTLeU3kW738w9arZj, image_id: 25b3cc900f38579ad530335b994e3ae327ac600afe335b734bd00af78309e776 }
execution.private_actions (1): each a nullifier, a commitment-tree root, a new commitment and an encrypted note (1343 bytes)
execution.async_messages: 0
execution.recovery_bindings: 0
execution.validity: ValidityWindows { blocks: ValidityWindow { from: None, to: None }, timestamps: ValidityWindow { from: None, to: None } }
execution.fee: Some("refund_to Some(78Skt7EQTvVwfWVYnA1XMjghv3aUR1V5wgQAfVLKJZG5), fallback true")
nonces: 0, admission evidence: 0


## Search

The 8937 serialized bytes of the transaction were searched for the claim's private inputs:
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
