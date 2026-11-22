// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";

import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice RFP-022 F.10, the vault's half of delisting: `setDepositsEnabled` publishes the listing at the SHA-256 slot
///         sha256("ferrata/vault/listing/v1" ‖ token), the word 1 while delisted and 0 otherwise, which the LEZ bridge
///         program attests (`AttestListing`; `lez/harness/tests/bridge_delist.rs` runs it on blocks holding this
///         slot). The slot of the token 0x11…11 is the bridge-wire unit test's vector, so both sides compute one slot.
contract ListingTest is VaultTestBase {
    function _listingSlot(address token) internal pure returns (bytes32) {
        return sha256(abi.encodePacked("ferrata/vault/listing/v1", token));
    }

    function test_listingSlot_isTheBridgeWiresVector() public pure {
        assertEq(
            _listingSlot(address(0x1111111111111111111111111111111111111111)),
            bytes32(0x7c3e51567f330bb2c6c9d58f6014831beef25d245cc799115462f9649a0c6fc4)
        );
        assertEq(_listingSlot(ETH), bytes32(0x04c4682ff8c82caf0fbc6c2bb47b594076f1f707d8d119512ddbbe285ed4c481));
    }

    /// @dev Releases of a delisted token go on: `Release.t.sol`, `test_release_worksForDelistedToken`.
    function test_delistAndRelist_publishTheSlot() public {
        bytes32 slot = _listingSlot(ETH);
        assertEq(vm.load(address(vault), slot), bytes32(0), "never written: listed");
        _deposit(alice, ETH, 1, bytes32(uint256(1)));

        vm.expectEmit(address(vault));
        emit FerrataVault.DepositsEnabledSet(ETH, false);
        vm.prank(admin);
        vault.setDepositsEnabled(ETH, false);
        assertEq(vm.load(address(vault), slot), bytes32(uint256(1)), "delisted: 1");
        // Deposits stop.
        uint256 total = _depositTotal(vault, ETH, 1);
        vm.deal(alice, total);
        uint64 next = vault.depositCount(alice);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.DepositsDisabled.selector);
        vault.depositETH{value: total}(1, bytes32(uint256(2)), next);
        // Another token's slot is untouched.
        assertEq(vm.load(address(vault), _listingSlot(address(usdc))), bytes32(0));

        vm.prank(admin);
        vault.setDepositsEnabled(ETH, true);
        assertEq(vm.load(address(vault), slot), bytes32(0), "relisted: 0");
        _deposit(alice, ETH, 1, bytes32(uint256(2)));
    }

    function test_onlyTheAdminDelists() public {
        vm.prank(alice);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setDepositsEnabled(ETH, false);
        assertEq(vm.load(address(vault), _listingSlot(ETH)), bytes32(0));
    }
}
