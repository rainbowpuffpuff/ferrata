// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {SelectorMismatch} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "../src/FerrataVault.sol";

/// @notice The round trip of script/fork-rehearsal-31.sh on a vault that script/deploy.sh deployed on an anvil fork of
///         Sepolia (RISC Zero's Groth16 verifier 3.1.0 behind its stop, decision D1 (a) of
///         docs/design/risc0-3.1-migration.md): the recorded LEZ v0.4.0-rc3 zone session of e2e/runs/zone-bridge-rc3
///         (its deposits, its claims and burns on the zone, its rule (B) checkpoint) with the checkpoint and both releases
///         proven again under RISC Zero 3.1 (test/real/fixtures/zone-bridge-rc3.json, scripts/reprove_fixtures.py).
///         The vault is at the address the zone's configuration names, so the proofs bind it. Each function is one step;
///         the shell script runs them with the right sender (--unlocked) and warps the fork's clock between them.
///         FERRATA_OUT: DeployVault's record; FERRATA_FIXTURE: the fixture (default zone-bridge-rc3.json).
contract Rehearsal31 is Script {
    string internal constant RUN = "../e2e/runs/zone-bridge-rc3";

    function _vault() internal view returns (FerrataVault) {
        return FerrataVault(payable(vm.parseJsonAddress(vm.readFile(vm.envString("FERRATA_OUT")), ".vault")));
    }

    function _fixture() internal view returns (string memory) {
        return vm.readFile(vm.envOr("FERRATA_FIXTURE", string("test/real/fixtures/zone-bridge-rc3.json")));
    }

    /// @dev The checkpoint the journal carries (17 words after its tag, chain id, vault and configuration id).
    function _checkpoint() internal view returns (FerrataVault.Checkpoint memory c) {
        bytes memory journal = vm.parseJsonBytes(_fixture(), ".journal");
        (,,,, c) = abi.decode(journal, (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint));
    }

    function _release(uint256 i) internal view returns (FerrataVault.ReleaseRequest memory r, bytes memory seal) {
        string memory json = _fixture();
        string memory at = string.concat(".releases[", vm.toString(i), "]");
        r = FerrataVault.ReleaseRequest({
            token: vm.parseJsonAddress(json, string.concat(at, ".token")),
            denomIndex: uint8(vm.parseJsonUint(json, string.concat(at, ".denomIndex"))),
            recipient: vm.parseJsonAddress(json, string.concat(at, ".recipient")),
            relayer: vm.parseJsonAddress(json, string.concat(at, ".relayer")),
            relayFee: vm.parseJsonUint(json, string.concat(at, ".relayFee")),
            burnRoot: vm.parseJsonBytes32(json, string.concat(at, ".burnRoot")),
            nullifier: vm.parseJsonBytes32(json, string.concat(at, ".nullifier"))
        });
        seal = vm.parseJsonBytes(json, string.concat(at, ".seal"));
    }

    /// @notice The journal binds this vault, this chain and the vault's configuration: the proofs are about this vault.
    function checkBinding() external view {
        bytes memory journal = vm.parseJsonBytes(_fixture(), ".journal");
        (, uint256 chainId, address vault, bytes32 configId,) =
            abi.decode(journal, (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint));
        FerrataVault v = _vault();
        require(chainId == block.chainid, "the journal names another chain");
        require(vault == address(v), "the journal names another vault");
        require(configId == v.lezConfigId(), "the journal names another configuration");
        require(_checkpoint().parentState == v.genesisState(), "the checkpoint does not start at the vault's genesis");
        console2.log("FERRATA_OK binding: chain, vault, configuration and genesis are the journal's");
    }

    /// @notice The run's two deposits (their commitments, as the zone imported them), 0.001 ETH each plus the relay fee.
    function deposits() external {
        string memory json = _fixture();
        FerrataVault v = _vault();
        for (uint64 j = 0; j < 2; j++) {
            bytes32 commitment = vm.parseJsonBytes32(json, string.concat(".commitments[", vm.toString(j), "]"));
            vm.broadcast();
            v.depositETH{value: 0.001 ether + 2e12}(0, commitment, j);
        }
        require(v.nextLeafIndex() == 2, "two deposits");
        console2.log("FERRATA_OK deposits: 2 leaves");
    }

    /// @notice The zone's checkpoint with the seal its run made under RISC Zero 3.0.5 is refused before any pairing.
    function oldSealRefused(bytes calldata oldSeal) external {
        require(bytes4(oldSeal) == bytes4(0x73c457ba), "not a seal of risc0 3.0.5");
        FerrataVault v = _vault();
        (bool ok, bytes memory ret) = address(v).call(abi.encodeCall(FerrataVault.submitCandidate, (_checkpoint(), oldSeal)));
        require(!ok, "the vault accepted a seal of risc0 3.0.5");
        require(
            keccak256(ret) == keccak256(abi.encodeWithSelector(SelectorMismatch.selector, bytes4(0x73c457ba), bytes4(0xe71e6620))),
            "refused, but not with SelectorMismatch(0x73c457ba, 0xe71e6620)"
        );
        console2.log("FERRATA_OK the 3.0.5 seal: SelectorMismatch(0x73c457ba, 0xe71e6620)");
    }

    function submit() external {
        FerrataVault.Checkpoint memory c = _checkpoint();
        bytes memory seal = vm.parseJsonBytes(_fixture(), ".seal");
        vm.broadcast();
        _vault().submitCandidate(c, seal);
        console2.log("FERRATA_OK candidate submitted with its RISC Zero 3.1 seal");
    }

    function finalize() external {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vm.broadcast();
        _vault().finalizeAttested(c);
        require(_vault().finalState() == c.newState, "final state");
        (bool accepted,) = _vault().burnRootStatus(c.burnRoot);
        require(accepted, "burn root not accepted");
        console2.log("FERRATA_OK final; burn root accepted");
    }

    /// @notice A sender's call that must fail with `selector` (nothing is sent: a call against the fork's state).
    function refused(uint256 i, bytes4 selector) external {
        (FerrataVault.ReleaseRequest memory r, bytes memory seal) = _release(i);
        (bool ok, bytes memory ret) = address(_vault()).call(abi.encodeCall(FerrataVault.release, (r, seal)));
        require(!ok, "the release went through");
        require(ret.length >= 4 && bytes4(ret) == selector, "refused with another error");
        console2.log(string.concat("FERRATA_OK release ", vm.toString(i), " refused as expected"));
    }

    function setReleasesFrozen(bool frozen) external {
        vm.broadcast();
        _vault().setReleasesFrozen(frozen);
        console2.log(frozen ? "FERRATA_OK releases frozen" : "FERRATA_OK releases unfrozen");
    }

    function release(uint256 i) external {
        (FerrataVault.ReleaseRequest memory r, bytes memory seal) = _release(i);
        require(bytes4(seal) == bytes4(0xe71e6620), "not a seal of RISC Zero 3.1");
        uint256 before = r.recipient.balance;
        vm.broadcast();
        _vault().release(r, seal);
        uint256 paid = r.recipient.balance - before;
        console2.log(string.concat("FERRATA_OK release ", vm.toString(i), " paid (wei) ", vm.toString(paid)));
    }
}
