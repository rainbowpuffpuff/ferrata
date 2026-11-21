# crown-fork-3r: the Sepolia stages of the crown run on a fork (STAND-IN, rehearsal)

11 Nov 2026 00:17Z, build box (mrun), anvil 1.3.6 fork of Sepolia block 12,108,134 with the real
deployer impersonated (nonce 10 there). Script: `box-3r.sh` (outside the repository), driving
`e2e/crown/sepolia.sh` (deploy | deposit 0 | deposit 1 | submit | finalize | release 0 | release 1).

Stand-in, labelled: configId, genesisState (identity.py over a label tip and key), the checkpoint and release image
ids are label hashes, and the Groth16 verifier is `RehearsalAcceptAllVerifier` (FORK ONLY) behind a real emergency
stop, so seals are not checked; the vault's other rules are. Gas is Foundry's (no EIP-8037 state gas on a fork).

Result: deploy.sh mode B complete (13 transactions in all, nonces 10..22); the vault at CREATE(deployer, nonce+2)
as predicted, storedLezConfigId and genesisState equal identity.json; deposits escrowed 1,002,000,000,000,000 wei each
(denomination + relay fee 0.2 %); finalizeAttested after evm_increaseTime 3601; both releases paid 999,000,000,000,000
wei (denomination - checkpoint fee 0.1 %); 6,000,000,000,000 wei stay escrowed (2 relay fees + 2 checkpoint fees).
Gas used (manifest.json): deposit 198,427 / 149,249; submitCandidate 119,764; finalizeAttested 215,073; releases
164,607 / 130,407. Logs are not committed (they name machine paths).
