// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Script} from "forge-std/Script.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {TestToken} from "./TestToken.sol";

/// @notice The Sepolia stages of the crown run (e2e/crown/README.md) after the deployment (script/deploy.sh,
///         FERRATA_MODE=B): one function per stage (an ERC-20 deposit broadcasts mint, approve and deposit), so every
///         transaction's limit is the node's estimate (e2e/crown/sepolia.sh uses --skip-simulation and price margin).
///
///         deposit(j)          deposit j of the run: absent deposits[j].token means ETH and msg.value is the
///                             denomination + mint fee + relay fee of schedule 0. For the test ERC-20, mint that total,
///                             approve it exactly, then deposit(token, ...). Skipped if done; ERC-20 takes three txs.
///         submit()            submitCandidate with the rule (B) checkpoint journal and its Groth16 seal
///         finalizeAttested()  once the candidate's wait W = deltaAtSubmit is over (rule B: turnSeconds = 0)
///         release(i)          release i, once its burn root is final and the release delay is over; skipped if paid
///
///         Input: FERRATA_CROWN, the run's crown.json (chainId, vault, deposits[], candidate{journal, seal},
///         releases[]). Keys: none here; forge signs with ETH_KEYSTORE/ETH_PASSWORD, or --unlocked on a fork.
contract Crown is Script {
    address internal constant ETH = address(0);

    string internal json;
    FerrataVault internal vault;

    function setUp() public {
        json = vm.readFile(vm.envString("FERRATA_CROWN"));
        require(block.chainid == vm.parseJsonUint(json, ".chainId"), "crown.json is for another chain id");
        vault = FerrataVault(payable(vm.parseJsonAddress(json, ".vault")));
        require(address(vault).code.length > 0, "the vault has no code");
        require(vault.mode() == FerrataVault.Mode.B, "the vault is not a rule (B) vault");
    }

    function _at(string memory list, uint256 i, string memory field) internal pure returns (string memory) {
        return string.concat(".", list, "[", vm.toString(i), "].", field);
    }

    /// @notice What deposit j pays: the denomination plus the mint and relay fees of schedule 0 (the vault escrows the
    ///         denomination and the relay fee and credits the mint fee to the fee recipient: ADR 0046). An absent token
    ///         is native ETH, as in every crown.json written before ERC-20 selection was added.
    function depositValue(uint256 j) public view returns (uint8 denom, uint256 total) {
        address token = _depositToken(j);
        denom = uint8(vm.parseJsonUint(json, _at("deposits", j, "denomIndex")));
        (,,,, uint256[] memory denoms) = vault.tokenConfig(token);
        (uint256 mintFee, uint256 relayFee,,) = vault.fees(token, 0, denom);
        total = denoms[denom] + mintFee + relayFee;
    }

    function _depositToken(uint256 j) internal view returns (address) {
        string memory key = _at("deposits", j, "token");
        return vm.keyExistsJson(json, key) ? vm.parseJsonAddress(json, key) : ETH;
    }

    function deposit(uint256 j) external {
        require(vm.parseJsonUint(json, _at("deposits", j, "j")) == j, "deposits[] is not in j order");
        address sender = msg.sender;
        if (vault.depositCount(sender) > j) return; // already made (an interrupted stage started again)
        require(vault.depositCount(sender) == j, "an earlier deposit of the run is missing");
        address token = _depositToken(j);
        (uint8 denom, uint256 total) = depositValue(j);
        bytes32 commitment = vm.parseJsonBytes32(json, _at("deposits", j, "commitment"));
        uint256 leaf = vault.nextLeafIndex();
        if (token == ETH) {
            vm.broadcast();
            vault.depositETH{value: total}(denom, commitment, uint64(j));
        } else {
            TestToken erc20 = TestToken(token);
            vm.broadcast();
            erc20.mint(sender, total);
            vm.broadcast();
            erc20.approve(address(vault), total);
            vm.broadcast();
            vault.deposit(token, denom, commitment, uint64(j));
        }
        require(vault.nextLeafIndex() == leaf + 1, "no leaf appended");
    }

    function _checkpoint() internal view returns (FerrataVault.Checkpoint memory c) {
        (,,,, c) = abi.decode(
            vm.parseJsonBytes(json, ".candidate.journal"), (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint)
        );
    }

    /// @notice When the candidate's wait is over (0 if it was not submitted).
    function waitOver() public view returns (uint256) {
        (,,, uint32 t, uint48 submittedAt, uint32 deltaAtSubmit,,) = vault.candidates(_checkpoint().newState);
        if (submittedAt == 0) return 0;
        return uint256(submittedAt) + deltaAtSubmit + (uint256(t) - 1) * vault.turnSeconds();
    }

    function submit() external {
        FerrataVault.Checkpoint memory c = _checkpoint();
        require(c.parentState == vault.finalState(), "the candidate does not build on the vault's final state");
        if (waitOver() != 0) return; // already submitted
        vm.broadcast();
        vault.submitCandidate(c, vm.parseJsonBytes(json, ".candidate.seal"));
    }

    function finalizeAttested() external {
        FerrataVault.Checkpoint memory c = _checkpoint();
        if (vault.finalState() == c.newState) return; // already final
        uint256 over = waitOver();
        require(over != 0, "the candidate was not submitted");
        require(block.timestamp >= over, "the candidate's wait is not over");
        vm.broadcast();
        vault.finalizeAttested(c);
    }

    function release(uint256 i) external {
        FerrataVault.ReleaseRequest memory r = FerrataVault.ReleaseRequest({
            token: vm.parseJsonAddress(json, _at("releases", i, "token")),
            denomIndex: uint8(vm.parseJsonUint(json, _at("releases", i, "denomIndex"))),
            recipient: vm.parseJsonAddress(json, _at("releases", i, "recipient")),
            relayer: vm.parseJsonAddress(json, _at("releases", i, "relayer")),
            relayFee: vm.parseJsonUint(json, _at("releases", i, "relayFee")),
            burnRoot: vm.parseJsonBytes32(json, _at("releases", i, "burnRoot")),
            nullifier: vm.parseJsonBytes32(json, _at("releases", i, "nullifier"))
        });
        if (vault.nullifierUsed(r.nullifier)) return; // already paid
        (bool accepted, uint256 finalizedAt) = vault.burnRootStatus(r.burnRoot);
        require(accepted, "the burn root is not final");
        require(
            block.timestamp >= finalizedAt + vault.releaseDelayFor(r.token, vault.checkpointCount() - 1),
            "the release delay is not over"
        );
        vm.broadcast();
        vault.release(r, vm.parseJsonBytes(json, _at("releases", i, "seal")));
    }
}
