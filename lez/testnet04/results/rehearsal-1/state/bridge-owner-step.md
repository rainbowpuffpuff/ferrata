# The owner-side Sepolia step for the bridge on this network (prepared by day1.sh; not sent)

The bridge program is deployed at `9Qaf4Mnt7E8jTWfP3PzUH4ghTo9y49QXQxCKsVeDomSD` (image `0xccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd`), beside the attestation program
`3bVFfqdLnQwN5vbn5vB59ZDSwLWjeqeN9qGHP5aSAR8S` (image `0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df`), whose Sepolia instance is `0x11d23518f276148be8ef4448980c9edc682414f12e46eb69ff9e603610cc43cb` (checkpoint
`0x430bcdd1ea31d4b02a45bd48e4916db1c2b8cd8c87b97282a8cdd00d3f238d1d`, slot 11607488).

A bridge configuration pairs the bridge with one vault: its `configId` commits to the bridge's image id, the
instance and the vault's address, and the vault pins it (docs/design/bridge.md, docs/sepolia.md). The vault is deployed
by the owner on Sepolia with the release's deployment script, in its own step:

    scripts/deploy-testnet.sh   # plan first (no --send); its LEZ side reads the program accounts above

then the configuration transaction is signed here against the deployed vault's address (`day1.sh` stage (e) stops at
this point: the vault does not exist yet). Nothing in this file has been sent anywhere.
