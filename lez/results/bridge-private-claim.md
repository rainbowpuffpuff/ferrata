# The private claim (`harness/tests/bridge_private.rs`), RISC Zero dev mode

Bridge program image `b5d177bb06a625166e8d39d10113655e113b768d3ebb101d496689b5d6691132`. `ClaimPrivately` at the destination's own private account, for vault A's leaf 5 (1 ETH, private kind) under `root(A, 8)`.

## The private transitions in the executor

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| ClaimPrivately (sends Read) | 1,353 | 2,735 | 41,133 | 1 | 131,072 | 0 |
| the root's reply (verifies the claim, sends Issue) | 1,543 | 1,889 | 111,838 | 1 | 262,144 | 0 |

## What LEZ charged

| execution gas | storage gas (tx bytes) | base fee | transaction bytes (dev mode: no real receipt) | proof bytes |
|---:|---:|---:|---:|---:|
| 539,060 | 224,063 | 6,104,984 | 8,953 | 4,533 |

## Public outputs (`harness::private::describe_public_outputs`)

public_context.actors (7): program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@3zaQWkQdXuLX8TAeDAN4PTUqLpaY6eNT9hjoJPUK2MXj, program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@62Ckz2u6AVpeY1MYAKjJK3gxkNvVUWdNCVWbqyz1Btrk, program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@7nds2VbmjyQLrRS5DE5BcyeXKPTHQ9tvMLAgXJoPyFPs, native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq, program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@9zXQfcPrmwrbWhEt7vVpsmUJVEchCmbVgP2AQx6TG21T, program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@Gwrkn4uQZP6TvN2mwVyqfb92cUeVx12WsJHrkhaPKd7q, program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@JAQ5MVHbCkSYRzXunsrNuM2m1LS859PGveHfoYPAmcvZ
public_context.authorized_accounts: {}
execution.public_root: None
execution.boundary (8 steps):
  - PrivateToPublic from program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@9zXQfcPrmwrbWhEt7vVpsmUJVEchCmbVgP2AQx6TG21T: 1 bytes (1 bytes)
  - PublicToPrivate from program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@9zXQfcPrmwrbWhEt7vVpsmUJVEchCmbVgP2AQx6TG21T to program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL: StateReply { state: 117 bytes } (121 bytes)
  - PrivateToPublic from program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to program DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty@62Ckz2u6AVpeY1MYAKjJK3gxkNvVUWdNCVWbqyz1Btrk: 175 bytes (175 bytes)
  - EndPublicSubtree
  - EndPrivateSubtree
  - EndPublicSubtree
  - PrivateToPublic from native@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq: Some(Credit(100000000)) (17 bytes)
  - EndPublicSubtree
execution.program_image_claims (1):
  - Disclosed { program: DEk3m64WAKZCjYNbrMHrehhherTq12PFzSHySLyc5Yty, image_id: b5d177bb06a625166e8d39d10113655e113b768d3ebb101d496689b5d6691132 }
execution.private_actions (1): each a nullifier, a commitment-tree root, a new commitment and an encrypted note (1343 bytes)
execution.async_messages: 0
execution.recovery_bindings: 0
execution.validity: ValidityWindows { blocks: ValidityWindow { from: None, to: None }, timestamps: ValidityWindow { from: None, to: None } }
execution.fee: Some("refund_to Some(78Skt7EQTvVwfWVYnA1XMjghv3aUR1V5wgQAfVLKJZG5), fallback true")
nonces: 0, admission evidence: 0


## Search

The 8953 serialized bytes of the transaction were searched for the claim's private inputs:
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
