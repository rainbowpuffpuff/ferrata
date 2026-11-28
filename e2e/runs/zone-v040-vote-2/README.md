# LEZ v0.4.0's committee vote again, then an unstake under L1 finality: the stake paid 69 node blocks after its key's removal

**Run:** from `checkpoint/`, `attestor/zone-v040.sh vote <bundle> OUT` on a Modal build box, 28 November 2026,
02:40–02:57 UTC; the same bundle, binaries and set-up as `../zone-v040-vote-1/` (LEZ v0.4.0 `a255b42c`, unmodified;
Logos Blockchain node 0.4.0-rc.2 with LEZ's V2 deployment settings; four sequencers, three of their keys staked from the
run's wallet), with the script's fix for the unstake's destination (it is funded first). Nothing was sent to any
public network.

**The vote (as in vote-1):** the channel accredits four keys (configuration threshold 3, posting timeframe 60:
[`channel-four-keys.json`](channel-four-keys.json)); `vote-params --posting-timeframe 90` by the three owners: "1 of the
3 accredited keys …", "2 of the 3 …", then (block 64) "The channel params are now ChannelParams { minimum_sequencer_stake:
149, posting_timeframe: 90, posting_timeout: 25, max_block_size: 1048576, finality_depth: 35 }"
([`logs/vote.log`](logs/vote.log)); the Bedrock channel's posting timeframe is 90 at the first read after the third vote
([`channel-after-vote.json`](channel-after-vote.json)).

**L1 finality in place of `exit_delay` (LEZ PR 1026):** one of the three keys (`12b87983…`) unstaked, its whole stake to
a fresh wallet account funded with 1,000 first ([`logs/unstake.log`](logs/unstake.log), [`facts.json`](facts.json)):

| | node height (`/cryptarchia/info`) | seconds after the unstake was sent |
|---|---|---|
| the unstake request lands ("Releasing 149 to …") | 185 | 30 (the command returns once it is included) |
| the channel drops the key (three accredited keys) | 185 (seen at the first poll) | 30 |
| the stake is paid (the destination's balance 1,000 → 1,149) | 254 | 306 |

The release came 69 node blocks after the key's removal was seen. With this genesis's `finality_depth` 35, that matches
2 × `finality_depth` − 1 = 69, the depth under the tip at which PR 1026's producer includes a full release
(`producible_releases`, release_finality.rs). The match holds only to within the polling: each height was read at a poll
(every 10 s, about 2–3 node blocks apart), so each is at or after the event it marks, and the true gap may differ by a few
blocks.

Checked by `python3 -I checkpoint/attestor/check_zone_v040.py e2e/runs/zone-v040-vote-2` (`facts.json: ok`). It checks
the votes, Bedrock's new posting timeframe, and that the release was paid no sooner than 2 × `finality_depth` − 1 blocks
after the key's removal as seen, less 3 blocks for the polls.

**A gap in the record:** [`run.log`](run.log) holds only the run's first five lines. The build box's copy of the run
directory was brought back to the laptop while the run was writing, and the next sync sent that early copy back over
the box's growing file. The files written when the run ended were not affected and hold every step: `facts.json` (each
step's outcome, heights and times), `logs/` (the stake, vote and unstake commands' own outputs, whole; the node's and
the sequencers' logs as the script
keeps them: at most their last 400 lines, each line cut at 2,000 characters) and the two channel files.
(`../zone-v040-vote-1/run.log` was cut the same way.)
