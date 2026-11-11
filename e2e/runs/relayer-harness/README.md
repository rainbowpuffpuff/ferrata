# relayer-harness: the relayer against the LEZ harness and a local vault

`cli/tests/e2e.rs` with `FERRATA_GASDROP_ELF` (run by `cli/tests/relayer.sh`, Modal build box, CPU), 11 Nov 2026:
anvil (chain id 1, local) with the v5 vault and the vault tests' mock verifier; the zone in the LEZ harness (bridge
guest in dev mode). On top of the CLI's own end-to-end test:
- **gas drop**: the user proves, for real (not dev mode, about 15 s), a gas-drop receipt for deposit leaf 0 under the
  root imported on the zone, bound to a one-time address and the relayer's LEZ account; the relayer checks it against
  the zone's imported roots and claimed nullifiers and the vault's verified deposit list, and records one credit
  (the harness does not model LEZ's native token program: the credit is a recorded stand-in); the receipt replayed
  to another relayer and a second drop are refused;
- the claim names the relayer: the zone mints it the relay fee (10¹⁶ wei for the 10 ETH denomination); afterwards
  a relayer with no record refuses the receipt (the deposit is claimed);
- **burn watch**: 11 burns on the zone and none accepted: a checkpoint is needed; once the root is accepted: none;
- **paid release**: burn 1's release names the relayer with the registered fee; the relayer sends it and is paid
  10¹⁴ wei (about 135,000 gas with the mock verifier); its copy with the recipient redirected is refused by the vault,
  and another relayer refuses the job.
