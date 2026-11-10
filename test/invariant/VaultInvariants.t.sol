// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {StdInvariant} from "forge-std/StdInvariant.sol";
import {console2} from "forge-std/console2.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {ReferenceMerkle} from "../utils/ReferenceMerkle.sol";
import {VaultTestBase} from "../utils/VaultTestBase.sol";
import {VaultHandler} from "./VaultHandler.sol";

contract VaultInvariants is VaultTestBase {
    VaultHandler internal handler;

    function setUp() public override {
        super.setUp();

        vm.startPrank(admin);
        // Finite caps, so the rate limiter takes part (the handler also changes them).
        vault.setTokenCap(ETH, FerrataVault.Direction.Deposit, 40 ether, 0.01 ether);
        vault.setTokenCap(address(dai), FerrataVault.Direction.Release, 40_000e18, 10e18);
        vault.setGlobalCap(FerrataVault.Direction.Deposit, 500_000e6, 100e6);
        vault.setGlobalCap(FerrataVault.Direction.Release, 500_000e6, 100e6);
        vm.stopPrank();

        address[] memory tokens = new address[](4);
        tokens[0] = ETH;
        tokens[1] = address(usdc);
        tokens[2] = address(dai);
        tokens[3] = address(usdt);
        handler = new VaultHandler(vault, verifier, admin, freezer, tokens);

        bytes4[] memory selectors = new bytes4[](9);
        selectors[0] = VaultHandler.deposit.selector;
        selectors[1] = VaultHandler.acceptRoot.selector;
        selectors[2] = VaultHandler.release.selector;
        selectors[3] = VaultHandler.replayNullifier.selector;
        selectors[4] = VaultHandler.donate.selector;
        selectors[5] = VaultHandler.warp.selector;
        selectors[6] = VaultHandler.setFreeze.selector;
        selectors[7] = VaultHandler.setCap.selector;
        selectors[8] = VaultHandler.setReleaseDelay.selector;
        targetContract(address(handler));
        targetSelector(StdInvariant.FuzzSelector({addr: address(handler), selectors: selectors}));
    }

    function invariant_escrowEqualsDepositsMinusReleases() public view {
        address[] memory tokens = handler.tokens();
        for (uint256 i = 0; i < tokens.length; i++) {
            assertEq(vault.escrowed(tokens[i]), handler.ghostDeposited(tokens[i]) - handler.ghostReleased(tokens[i]));
        }
    }

    function invariant_vaultBalanceCoversEscrow() public view {
        address[] memory tokens = handler.tokens();
        for (uint256 i = 0; i < tokens.length; i++) {
            assertGe(_balance(tokens[i], address(vault)), vault.escrowed(tokens[i]));
        }
    }

    function invariant_leafCountEqualsDepositCount() public view {
        assertEq(vault.nextLeafIndex(), handler.ghostDepositCount());
        assertEq(vm.load(address(vault), bytes32(uint256(1))), bytes32(handler.ghostDepositCount()));
    }

    function invariant_depositRootMatchesReference() public view {
        bytes32 expected = ReferenceMerkle.root(handler.leaves());
        assertEq(vault.depositRoot(), expected);
        assertEq(vm.load(address(vault), bytes32(uint256(0))), expected);
    }

    function invariant_usedNullifierIsNeverAcceptedAgain() public view {
        assertEq(handler.ghostDoubleSpends(), 0);
        bytes32[] memory used = handler.usedNullifiers();
        for (uint256 i = 0; i < used.length; i++) {
            assertTrue(vault.nullifierUsed(used[i]));
        }
        assertEq(used.length, handler.ghostReleaseCount());
    }

    /// @dev Run summary, shown with -vv: shows the campaign reached successful releases and replay attempts that
    ///      only the nullifier check stopped, i.e. that the invariants above are not passing vacuously.
    function afterInvariant() public view {
        console2.log("deposits", handler.ghostDepositCount());
        console2.log("releases", handler.ghostReleaseCount());
        console2.log("replays stopped by nullifier check", handler.ghostReplaysStoppedByNullifier());
    }
}
