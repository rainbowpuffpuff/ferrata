# LEZ v0.4.0's committee-voted parameters on a zone of four: two votes change nothing, the third does, and Bedrock follows

**Run:** from `checkpoint/`, `attestor/zone-v040.sh vote <bundle> OUT` on a Modal build box, 28 November 2026,
02:30–02:38 UTC ([`run.log`](run.log), [`facts.json`](facts.json)); the bundle and binaries are those of
`../zone-v040-standalone-1/` (LEZ v0.4.0 `a255b42c`, unmodified; the Logos Blockchain node 0.4.0-rc.2 LEZ v0.4.0 pins,
with LEZ's V2 deployment settings; dev-mode leader proofs as LEZ's container runs them). Nothing was sent to any public
network.

**Why not upstream's devnet keys:** upstream's four-key devnet stakes its keys at genesis with ownership accounts whose
secrets its generator (`tools/devnet_configs`) makes at random and discards, so no one can vote with them. This run
starts from the same devnet configuration without its founding stakes: sequencer 0 creates the channel and stakes its
own key at genesis (LEZ v0.4.0's `setup` says so: "Creating the channel stakes that key at genesis"), and three more
keys are staked by this run's wallet with `sequencer_service stake` (149 each, the minimum, paid by LEZ's published
local test accounts), which makes each stake's ownership account in that wallet.

**What it shows** ([`logs/stake.log`](logs/stake.log), [`logs/vote.log`](logs/vote.log), the channel before and after):

| | |
|---|---|
| The channel created by sequencer 0 alone (one key) | 02:31:08 |
| Three keys staked from the run's wallet; the channel accredits four ([`channel-four-keys.json`](channel-four-keys.json): four keys, configuration threshold 3, posting timeframe 60 slots) | 02:31:39, 02:32:10, 02:36:40 |
| `vote-params --posting-timeframe 90` by key 1's owner (LEZ PR 1089) | block 36: "1 of the 3 accredited keys these params need have voted for them"; the Bedrock channel still 60 |
| the same by key 2's owner | block 39: "2 of the 3 …"; the Bedrock channel still 60 |
| the same by key 3's owner | block 42: "The channel params are now ChannelParams { minimum_sequencer_stake: 149, posting_timeframe: 90, posting_timeout: 25, max_block_size: 1048576, finality_depth: 35 }" |
| The Bedrock channel takes the voted posting timeframe through the committee's channel configuration update | 90 at the first read after the third vote ([`channel-after-vote.json`](channel-after-vote.json): four keys, threshold 3) |

Checked by `python3 -I checkpoint/attestor/check_zone_v040.py e2e/runs/zone-v040-vote-1`. The vote checks pass, and the
checker reports `the unstake under L1 finality (run by default) has no record: it did not complete`. That is this run's
refused unstake (above); `../zone-v040-vote-2/` has the unstake that completed.

**Where it stopped:** the next step, an unstake of key 3 to show LEZ PR 1026's L1 finality, was refused by LEZ's own
transaction builder before anything was sent: `FinalizeUnstake cannot pay <the destination>: it is neither a public
account nor a bound private address` ([`logs/unstake.log`](logs/unstake.log)). The destination was a fresh wallet
account that did not yet exist on chain. The script now funds the destination first; the unstake and its release
under L1 finality are in `../zone-v040-vote-2/`.

**A gap in the record:** [`run.log`](run.log) stops at the first stake. The build box's copy of the run directory was
brought back while the run was writing, and a later sync sent that early copy back over the box's file. The files
written when the run ended were not affected: [`facts.json`](facts.json), [`logs/`](logs/) (the stake, vote and unstake
commands' outputs, whole, which the quotes above come from; the node's and the sequencers' logs as the script
keeps them: at most their last 400 lines, each line cut at 2,000 characters) and the two channel files.
