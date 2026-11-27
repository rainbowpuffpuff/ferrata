// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {IRiscZeroVerifier as IRiscZeroVerifierFull, Receipt} from "risc0/IRiscZeroVerifier.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {ReferenceMerkle} from "../test/utils/ReferenceMerkle.sol";
import {TestToken} from "./TestToken.sol";

/// @notice FOR A LOCAL FORK ONLY: accepts every proof. The refund rehearsal puts it behind the emergency stop, because
///         no checkpoint proof exists for a vault at a fresh address without a GPU prover.
contract RehearsalAcceptAllVerifier is IRiscZeroVerifierFull {
    function verify(bytes calldata, bytes32, bytes32) external pure {}

    function verifyIntegrity(Receipt calldata) external pure {}
}

/// @notice The deposit and refund paths on a local fork, against a deployment of script/deploy.sh (FERRATA_OUT names
///         its record). script/fork-rehearsal.sh runs the steps and moves the fork's clock between them:
///         deposits()           msg.sender deposits 0.001 ETH (leaf 0) and 100 test tokens (leaf 1), with their fees
///         checkDeposits(d)     escrow = the whole payment (denomination + mint + relay, ADR 0048); lezFees = mint;
///                              nothing owed; balance = escrow; root is the reference's
///         sunset()             rule (B): the freeze authority retires the vault and submits a sunset candidate that
///                              imported no leaf
///         finalizeSunset()     after the wait: finalizeAttested
///         refunds(d)           after the revoke window: anyone refunds both leaves to their depositor
///         checkRefunds(d, b)   each refund returned denomination + relay; only the mint fees stay escrowed, counted in
///                              lezFees for the fee recipient on LEZ
contract ForkRehearsal is Script {
    address internal constant ETH = address(0);
    bytes32 internal constant COMMITMENT_ETH = keccak256("ferrata fork rehearsal: ETH deposit");
    bytes32 internal constant COMMITMENT_TOKEN = keccak256("ferrata fork rehearsal: token deposit");

    FerrataVault internal vault;
    TestToken internal token;

    function _load() internal {
        string memory record = vm.readFile(vm.envString("FERRATA_OUT"));
        require(vm.parseJsonBool(record, ".complete"), "rehearsal: the deployment is not complete");
        vault = FerrataVault(payable(vm.parseJsonAddress(record, ".vault")));
        token = TestToken(vm.parseJsonAddress(record, ".token"));
    }

    /// @dev Denomination 0 of `t`, the whole amount its deposit pays, and its mint fee (escrowed and counted in `lezFees`).
    function _total(address t) internal view returns (uint256 denom, uint256 total, uint256 mintFee) {
        (,,,, uint256[] memory denoms) = vault.tokenConfig(t);
        (uint64 current,) = vault.feeState(t);
        uint256 relayFee;
        (mintFee, relayFee,,) = vault.fees(t, current, 0);
        denom = denoms[0];
        total = denom + mintFee + relayFee;
    }

    function _leaves(address depositor) internal view returns (bytes32[] memory leaves) {
        (uint256 ethDenom,,) = _total(ETH);
        (uint256 tokenDenom,,) = _total(address(token));
        leaves = new bytes32[](2);
        leaves[0] = ReferenceMerkle.leafHash(0, depositor, ETH, ethDenom, COMMITMENT_ETH);
        leaves[1] = ReferenceMerkle.leafHash(1, depositor, address(token), tokenDenom, COMMITMENT_TOKEN);
    }

    function deposits() external {
        _load();
        require(vault.nextLeafIndex() == 0, "rehearsal: the vault is not fresh");
        (, uint256 ethTotal,) = _total(ETH);
        (, uint256 tokenTotal,) = _total(address(token));
        vm.startBroadcast(msg.sender);
        vault.depositETH{value: ethTotal}(0, COMMITMENT_ETH, 0);
        token.mint(msg.sender, tokenTotal);
        token.approve(address(vault), tokenTotal);
        vault.deposit(address(token), 0, COMMITMENT_TOKEN, 1);
        vm.stopBroadcast();
    }

    function checkDeposits(address depositor) external {
        _load();
        (uint256 ethDenom, uint256 ethTotal, uint256 ethMintFee) = _total(ETH);
        (uint256 tokenDenom, uint256 tokenTotal, uint256 tokenMintFee) = _total(address(token));
        // ADR 0048: the whole payment is escrowed; the mint fee is counted in lezFees and credited to no one.
        require(ethTotal - ethMintFee > ethDenom && tokenTotal - tokenMintFee > tokenDenom, "rehearsal: no relay fee escrowed");
        require(vault.escrowed(ETH) == ethTotal && address(vault).balance == ethTotal, "rehearsal: ETH accounting");
        require(vault.lezFees(ETH) == ethMintFee && vault.totalOwed(ETH) == 0, "rehearsal: ETH mint fee for LEZ");
        require(
            vault.escrowed(address(token)) == tokenTotal && token.balanceOf(address(vault)) == tokenTotal,
            "rehearsal: token accounting"
        );
        require(
            vault.lezFees(address(token)) == tokenMintFee && vault.totalOwed(address(token)) == 0,
            "rehearsal: token mint fee for LEZ"
        );
        require(vault.nextLeafIndex() == 2, "rehearsal: leaf count");
        require(vault.depositRoot() == ReferenceMerkle.root(_leaves(depositor)), "rehearsal: deposit root");
        console2.log("FERRATA_OK deposits, ETH denomination and escrow (wei):", ethDenom, ethTotal);
        console2.log("FERRATA_OK deposits, token denomination and escrow (base units):", tokenDenom, tokenTotal);
    }

    function _sunsetCheckpoint() internal view returns (FerrataVault.Checkpoint memory c) {
        c.nMin = vault.nMin();
        c.tMin = vault.tMin();
        c.parentState = vault.finalState();
        c.newState = keccak256("ferrata fork rehearsal: sunset state");
        c.burnRoot = keccak256("ferrata fork rehearsal: empty burn tree");
        c.importedLeafCount = 0;
        c.sunset = true;
        c.tipId = 1;
        c.tipHash = keccak256("ferrata fork rehearsal: tip");
        c.n = c.nMin + 1;
        c.t = c.tMin + 1;
    }

    function sunset() external {
        _load();
        require(vault.mode() == FerrataVault.Mode.B, "rehearsal: the refund path runs on the rule (B) vault");
        FerrataVault.Checkpoint memory c = _sunsetCheckpoint();
        vm.startBroadcast(msg.sender);
        vault.retire();
        vault.submitCandidate(c, "");
        vm.stopBroadcast();
    }

    function finalizeSunset() external {
        _load();
        FerrataVault.Checkpoint memory c = _sunsetCheckpoint();
        vm.broadcast(msg.sender);
        vault.finalizeAttested(c);
    }

    function _path(bytes32[] memory leaves, uint256 index) internal pure returns (bytes32[32] memory p) {
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, index);
        for (uint256 h = 0; h < 32; h++) {
            p[h] = siblings[h];
        }
    }

    function refunds(address depositor) external {
        _load();
        bytes32[] memory leaves = _leaves(depositor);
        vm.startBroadcast(msg.sender);
        vault.refund(0, depositor, ETH, 0, COMMITMENT_ETH, _path(leaves, 0));
        vault.refund(1, depositor, address(token), 0, COMMITMENT_TOKEN, _path(leaves, 1));
        vm.stopBroadcast();
    }

    function checkRefunds(address depositor, uint256 ethBalanceBefore) external {
        _load();
        (, uint256 ethTotal, uint256 ethMintFee) = _total(ETH);
        (, uint256 tokenTotal, uint256 tokenMintFee) = _total(address(token));
        uint256 ethEscrow = ethTotal - ethMintFee;
        uint256 tokenEscrow = tokenTotal - tokenMintFee;
        require(vault.refunded(0) && vault.refunded(1), "rehearsal: not refunded");
        // ADR 0048: a refund returns denomination + relay fee; the mint fee stays escrowed for the fee recipient on LEZ.
        require(
            vault.escrowed(ETH) == ethMintFee && vault.escrowed(address(token)) == tokenMintFee, "rehearsal: escrow left"
        );
        require(depositor.balance == ethBalanceBefore + ethEscrow, "rehearsal: ETH refund amount");
        require(token.balanceOf(depositor) == tokenEscrow, "rehearsal: token refund amount");
        require(vault.lezFees(ETH) == ethMintFee && vault.totalOwed(ETH) == 0, "rehearsal: ETH mint fee is not refunded");
        require(
            vault.lezFees(address(token)) == tokenMintFee && vault.totalOwed(address(token)) == 0,
            "rehearsal: token mint fee is not refunded"
        );
        console2.log("FERRATA_OK refunds, ETH returned to the depositor (wei):", ethEscrow);
        console2.log("FERRATA_OK refunds, tokens returned to the depositor (base units):", tokenEscrow);
    }
}
