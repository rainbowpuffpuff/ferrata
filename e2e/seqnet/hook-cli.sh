#!/usr/bin/env bash
# e2e/seqnet's hook for the `ferrata` tool: lez/crown/session.sh x runs it while the zone (the LEZ sequencer and the
# Logos Blockchain node) runs; drive.sh has made the vault on anvil, the user's deposit and capture-1. Steps:
#   a. the operator and the relayer: crown-driver crown-b --operator-only over capture-1 (the stand-in committee's
#      finality update for the deposit's block, bridge Init, Register ETH, ImportRoot n)
#   b. the gas drop's stand-in (seqnet-fund): native LEZ to the user's private account /0 for its fees
#   c. the user, with the tool on its defaults: scan, the claim held by its delay, the delay let pass on anvil's clock,
#      the private claim, a private burn to a fresh address, recover
#   d. the checkpoint (common.sh: the node's final blocks, realblocks-crown, ferrata-e2e exec, the stand-in prover,
#      the relayer's submitCandidate, Delta let pass on anvil's clock, finalizeAttested)
#   e. the user's release through a relayer (the release delay let pass), recover and status; the zone's transactions
set -uo pipefail
. "$SEQNET_ENV"
. "$REPO/e2e/seqnet/common.sh"

F=("$BIN/ferrata" --seed "$U/seed.txt" --eth-rpc "$RPC" --chain-id 11155111 --vault "$VAULT" --from-block "$FROM"
   --sequencer "$SEQ" --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE")

operator_import                                                            # a
fund_private                                                               # b

# c. the user's LEZ side
u 06-scan "${F[@]}" scan || die "scan"
u 07-status "${F[@]}" status || die "status"
u 08-claim-held "${F[@]}" claim --bridge-program "$BRIDGE_BIN" && die "the claim was not held by its delay"
let_claim_delay_pass "$RUN/user/08-claim-held.txt"
u 09-claim-dry-run "${F[@]}" claim --bridge-program "$BRIDGE_BIN" || die "claim dry run"
u 10-claim "${PROVE[@]}" "${F[@]}" claim --bridge-program "$BRIDGE_BIN" --send --yes || die "the private claim"
u 11-recover "${F[@]}" recover || die "recover"
FRESH="$(cast wallet new --json | jq -r '.[0].address')"
echo "FRESH=$FRESH" >> "$SEQNET_ENV"
say "the user's fresh Ethereum address for the release: $FRESH (cast wallet new; its key is not kept)"
u 12-burn-dry-run "${F[@]}" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" || die "burn dry run"
u 13-burn "${PROVE[@]}" "${F[@]}" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" --send --yes || die "the private burn"
u 14-recover "${F[@]}" recover || die "recover"

checkpoint                                                                 # d

# e. the release through a relayer
printf '0x5ea15ea1\n' > "$W/seal.hex"   # the mock verifier's: the stand-in prover accepts the journal below first
u 15-release-held "${F[@]}" release --seal "$W/seal.hex" && say "note: the release was not held (its delay had passed)"
pass_time 121 "the release delay (120 s after finalizeAttested)"
u 16-release "${F[@]}" release --seal "$W/seal.hex" --job-out "$W/release-job.json" || die "release"
relay_release "$W/release-job.json"
u 17-recover "${F[@]}" recover || die "recover"
u 18-status "${F[@]}" status || die "status"
records
say "hook-cli: done"
