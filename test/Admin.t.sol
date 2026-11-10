// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockERC20} from "./mocks/MockTokens.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

contract ConstructorTest is VaultTestBase {
    function _construct(
        address v,
        bytes32 releaseId,
        bytes32 rootId,
        bytes32 lezConfig,
        address admin_,
        address freezer_
    ) internal returns (FerrataVault) {
        return new FerrataVault(
            IRiscZeroVerifier(v), releaseId, rootId, lezConfig, admin_, freezer_, MIN_DEPTH, _unlimited(), _unlimited()
        );
    }

    function test_constructor_setsImmutablesRolesAndEmptyRoot() public view {
        assertEq(address(vault.verifier()), address(estop));
        assertEq(address(estop.verifier()), address(verifier));
        assertEq(estop.owner(), freezer); // the freeze authority is the stop's guardian
        assertEq(vault.releaseImageId(), RELEASE_IMAGE_ID);
        assertEq(vault.rootImageId(), ROOT_IMAGE_ID);
        assertEq(vault.lezConfigId(), LEZ_CONFIG_ID);
        assertEq(vault.admin(), admin);
        assertEq(vault.freezeAuthority(), freezer);
        assertEq(vault.minLezDepth(), MIN_DEPTH);
        assertEq(vault.nextLeafIndex(), 0);
        assertEq(vault.depositRoot(), ReferenceMerkle.zeroHashes()[32]);
    }

    function test_constructor_domainTagsMatchTheirDefinitions() public view {
        assertEq(vault.LEAF_TAG(), sha256(bytes("ferrata/vault/leaf/v1")));
        assertEq(vault.ROOT_DOMAIN(), sha256(bytes("ferrata/vault/burn-root/v1")));
        assertEq(vault.RELEASE_DOMAIN(), sha256(bytes("ferrata/vault/release/v1")));
        assertEq(vault.LEAF_PREFIX(), bytes1(0x00));
        assertEq(vault.NODE_PREFIX(), bytes1(0x01));
    }

    function test_constructor_rejectsVerifierWithoutCode() public {
        vm.expectRevert(FerrataVault.VerifierHasNoCode.selector);
        _construct(makeAddr("eoa"), RELEASE_IMAGE_ID, ROOT_IMAGE_ID, LEZ_CONFIG_ID, admin, freezer);
    }

    function test_constructor_rejectsBadImageIds() public {
        vm.expectRevert(FerrataVault.InvalidImageIds.selector);
        _construct(address(verifier), bytes32(0), ROOT_IMAGE_ID, LEZ_CONFIG_ID, admin, freezer);
        vm.expectRevert(FerrataVault.InvalidImageIds.selector);
        _construct(address(verifier), RELEASE_IMAGE_ID, bytes32(0), LEZ_CONFIG_ID, admin, freezer);
        vm.expectRevert(FerrataVault.InvalidImageIds.selector);
        _construct(address(verifier), RELEASE_IMAGE_ID, RELEASE_IMAGE_ID, LEZ_CONFIG_ID, admin, freezer);
    }

    function test_constructor_rejectsZeroLezConfigId() public {
        vm.expectRevert(FerrataVault.InvalidLezConfigId.selector);
        _construct(address(verifier), RELEASE_IMAGE_ID, ROOT_IMAGE_ID, bytes32(0), admin, freezer);
    }

    function test_constructor_rejectsZeroOrSharedRoles() public {
        vm.expectRevert(FerrataVault.ZeroAddress.selector);
        _construct(address(verifier), RELEASE_IMAGE_ID, ROOT_IMAGE_ID, LEZ_CONFIG_ID, address(0), freezer);
        vm.expectRevert(FerrataVault.ZeroAddress.selector);
        _construct(address(verifier), RELEASE_IMAGE_ID, ROOT_IMAGE_ID, LEZ_CONFIG_ID, admin, address(0));
        vm.expectRevert(FerrataVault.RolesMustDiffer.selector);
        _construct(address(verifier), RELEASE_IMAGE_ID, ROOT_IMAGE_ID, LEZ_CONFIG_ID, admin, admin);
    }

    function test_constructor_globalCapsStartFull() public {
        FerrataVault v = new FerrataVault(
            IRiscZeroVerifier(address(verifier)),
            RELEASE_IMAGE_ID,
            ROOT_IMAGE_ID,
            LEZ_CONFIG_ID,
            admin,
            freezer,
            MIN_DEPTH,
            _cap(500, 1),
            _cap(700, 2)
        );
        (uint256 capacity, uint256 refill, uint256 available) = v.globalCap(FerrataVault.Direction.Deposit);
        assertEq(capacity, 500);
        assertEq(refill, 1);
        assertEq(available, 500);
        (capacity, refill, available) = v.globalCap(FerrataVault.Direction.Release);
        assertEq(capacity, 700);
        assertEq(refill, 2);
        assertEq(available, 700);
    }
}

contract RegistryTest is VaultTestBase {
    MockERC20 internal token;

    function setUp() public override {
        super.setUp();
        token = new MockERC20("Token", "TKN", 18);
    }

    function _register(uint256[] memory denoms, uint256 lezUnit, uint256 unitWeight) internal {
        vm.prank(admin);
        vault.registerToken(address(token), denoms, lezUnit, unitWeight, _cap(1000e18, 1e18), _cap(2000e18, 2e18));
    }

    function _one(uint256 a) internal pure returns (uint256[] memory d) {
        d = new uint256[](1);
        d[0] = a;
    }

    function test_registerToken_storesConfigAndEmits() public {
        uint256[] memory denoms = _denoms(1e18, 5e18, 25e18);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.TokenRegistered(address(token), denoms, 1e12, 7);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.TokenCapSet(address(token), FerrataVault.Direction.Deposit, 1000e18, 1e18);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.TokenCapSet(address(token), FerrataVault.Direction.Release, 2000e18, 2e18);
        _register(denoms, 1e12, 7);

        (bool registered, bool enabled, uint256 lezUnit, uint256 weight, uint256[] memory stored) =
            vault.tokenConfig(address(token));
        assertTrue(registered);
        assertTrue(enabled);
        assertEq(lezUnit, 1e12);
        assertEq(weight, 7);
        assertEq(stored, denoms);
        (uint256 capacity,, uint256 available) = vault.tokenCap(address(token), FerrataVault.Direction.Release);
        assertEq(capacity, 2000e18);
        assertEq(available, 2000e18); // new buckets start full
    }

    function test_registerToken_onlyAdmin() public {
        uint256[] memory denoms = _denoms(1e18, 5e18, 25e18);
        address[2] memory notAdmin = [freezer, alice];
        for (uint256 i = 0; i < notAdmin.length; i++) {
            vm.prank(notAdmin[i]);
            vm.expectRevert(FerrataVault.NotAdmin.selector);
            vault.registerToken(address(token), denoms, 1e12, 1, _unlimited(), _unlimited());
        }
    }

    function test_registerToken_onlyOnce() public {
        _register(_denoms(1e18, 5e18, 25e18), 1e12, 1);
        vm.prank(admin);
        vm.expectRevert(FerrataVault.TokenAlreadyRegistered.selector);
        vault.registerToken(address(token), _denoms(2e18, 5e18, 25e18), 1e12, 1, _unlimited(), _unlimited());
    }

    function test_registerToken_rejectsAddressWithoutCode() public {
        vm.prank(admin);
        vm.expectRevert(FerrataVault.TokenHasNoCode.selector);
        vault.registerToken(makeAddr("not a token"), _one(1e18), 1e12, 1, _unlimited(), _unlimited());
    }

    function test_registerToken_denominationRules() public {
        vm.startPrank(admin);
        uint256[] memory empty = new uint256[](0);
        vm.expectRevert(FerrataVault.InvalidDenominations.selector);
        vault.registerToken(address(token), empty, 1, 1, _unlimited(), _unlimited());

        uint256[] memory nine = new uint256[](9);
        for (uint256 i = 0; i < 9; i++) {
            nine[i] = i + 1;
        }
        vm.expectRevert(FerrataVault.InvalidDenominations.selector);
        vault.registerToken(address(token), nine, 1, 1, _unlimited(), _unlimited());

        vm.expectRevert(FerrataVault.InvalidDenominations.selector); // zero
        vault.registerToken(address(token), _denoms(0, 1, 2), 1, 1, _unlimited(), _unlimited());
        vm.expectRevert(FerrataVault.InvalidDenominations.selector); // repeated
        vault.registerToken(address(token), _denoms(1, 2, 2), 1, 1, _unlimited(), _unlimited());
        vm.expectRevert(FerrataVault.InvalidDenominations.selector); // decreasing
        vault.registerToken(address(token), _denoms(1, 3, 2), 1, 1, _unlimited(), _unlimited());
        vm.expectRevert(FerrataVault.InvalidDenominations.selector); // above MAX_DENOMINATION
        vault.registerToken(address(token), _one(uint256(type(uint128).max) + 1), 1, 1, _unlimited(), _unlimited());

        uint256[] memory eight = new uint256[](8);
        for (uint256 i = 0; i < 8; i++) {
            eight[i] = i + 1;
        }
        vault.registerToken(address(token), eight, 1, 1, _unlimited(), _unlimited());
        vm.stopPrank();
        assertEq(vault.denominations(address(token)).length, 8);
    }

    function test_registerToken_lezUnitMustBePowerOfTen() public {
        uint256[6] memory bad = [uint256(0), 2, 20, 15, 1e4 + 1, 3e4];
        vm.startPrank(admin);
        for (uint256 i = 0; i < bad.length; i++) {
            vm.expectRevert(FerrataVault.InvalidLezUnit.selector);
            vault.registerToken(address(token), _one(9e4), bad[i], 1, _unlimited(), _unlimited());
        }
        vm.stopPrank();
    }

    function testFuzz_registerToken_lezUnitPowersOfTenAccepted(uint8 exponent) public {
        uint256 k = bound(exponent, 0, 38); // 10**38 is the largest power of ten below MAX_DENOMINATION
        uint256 unit = 10 ** k;
        _register(_one(unit), unit, 1);
        (,, uint256 lezUnit,,) = vault.tokenConfig(address(token));
        assertEq(lezUnit, unit);
    }

    function testFuzz_registerToken_nonPowersOfTenRejected(uint256 unit) public {
        vm.assume(unit != 0);
        uint256 rest = unit;
        while (rest % 10 == 0) {
            rest /= 10;
        }
        vm.assume(rest != 1);
        vm.prank(admin);
        vm.expectRevert(FerrataVault.InvalidLezUnit.selector);
        vault.registerToken(address(token), _one(1e18), unit, 1, _unlimited(), _unlimited());
    }

    function test_registerToken_denominationsMustBeMultiplesOfLezUnit() public {
        MockERC20 sixDecimals = new MockERC20("Six", "SIX", 6);
        vm.startPrank(admin);
        // 1.234567 units of a 6-decimal token is not representable with 2 decimals on LEZ (lezUnit 1e4).
        vm.expectRevert(FerrataVault.DenominationNotMultipleOfLezUnit.selector);
        vault.registerToken(address(sixDecimals), _denoms(1e6, 1_234_567, 10e6), 1e4, 1, _unlimited(), _unlimited());
        // 1 ether + 1 wei is not representable with 6 decimals on LEZ (lezUnit 1e12).
        vm.expectRevert(FerrataVault.DenominationNotMultipleOfLezUnit.selector);
        vault.registerToken(address(token), _denoms(1e17, 1e18 + 1, 1e19), 1e12, 1, _unlimited(), _unlimited());
        // The same tokens with representable amounts are accepted.
        vault.registerToken(address(sixDecimals), _denoms(1e6, 1_230_000, 10e6), 1e4, 1, _unlimited(), _unlimited());
        vault.registerToken(address(token), _denoms(1e17, 1e18, 1e19), 1e12, 1, _unlimited(), _unlimited());
        vm.stopPrank();
    }

    function test_registerToken_unitWeightBounds() public {
        vm.startPrank(admin);
        vm.expectRevert(FerrataVault.InvalidUnitWeight.selector);
        vault.registerToken(address(token), _one(1e18), 1e12, 0, _unlimited(), _unlimited());
        vm.expectRevert(FerrataVault.InvalidUnitWeight.selector);
        vault.registerToken(
            address(token), _one(1e18), 1e12, uint256(type(uint128).max) + 1, _unlimited(), _unlimited()
        );
        vm.stopPrank();
    }

    // ---- setters --------------------------------------------------------------------------------------------------

    function test_setDepositsEnabled() public {
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.DepositsEnabledSet(address(usdc), false);
        vm.prank(admin);
        vault.setDepositsEnabled(address(usdc), false);
        (, bool enabled,,,) = vault.tokenConfig(address(usdc));
        assertFalse(enabled);

        vm.prank(admin);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.setDepositsEnabled(address(token), false);
    }

    function test_setUnitWeight() public {
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.UnitWeightSet(ETH, 3500);
        vm.prank(admin);
        vault.setUnitWeight(ETH, 3500);
        (,,, uint256 weight,) = vault.tokenConfig(ETH);
        assertEq(weight, 3500);

        vm.startPrank(admin);
        vm.expectRevert(FerrataVault.InvalidUnitWeight.selector);
        vault.setUnitWeight(ETH, 0);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.setUnitWeight(address(token), 1);
        vm.stopPrank();
    }

    function test_setStandardRelayFee() public {
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.StandardRelayFeeSet(address(usdc), 2, 5e6);
        vm.prank(admin);
        vault.setStandardRelayFee(address(usdc), 2, 5e6);
        assertEq(vault.standardRelayFee(address(usdc), 2), 5e6);

        vm.startPrank(admin);
        vm.expectRevert(FerrataVault.RelayFeeTooHigh.selector);
        vault.setStandardRelayFee(address(usdc), 0, 100e6); // fee must be below the denomination
        vm.expectRevert(FerrataVault.InvalidDenominationIndex.selector);
        vault.setStandardRelayFee(address(usdc), 3, 1);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.setStandardRelayFee(address(token), 0, 1);
        vm.stopPrank();
    }

    function test_setReleaseDelay() public {
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.ReleaseDelaySet(address(dai), 30 days);
        vm.prank(admin);
        vault.setReleaseDelay(address(dai), 30 days);
        assertEq(vault.releaseDelay(address(dai)), 30 days);

        vm.startPrank(admin);
        vm.expectRevert(FerrataVault.ReleaseDelayTooLong.selector);
        vault.setReleaseDelay(address(dai), 30 days + 1);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.setReleaseDelay(address(token), 1);
        vm.stopPrank();
    }

    function test_setMinLezDepth() public {
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.MinLezDepthSet(128);
        vm.prank(admin);
        vault.setMinLezDepth(128);
        assertEq(vault.minLezDepth(), 128);
    }

    function test_setCaps_emitAndRequireRegistration() public {
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.TokenCapSet(address(dai), FerrataVault.Direction.Release, 5e18, 1);
        vm.prank(admin);
        vault.setTokenCap(address(dai), FerrataVault.Direction.Release, 5e18, 1);

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.GlobalCapSet(FerrataVault.Direction.Deposit, 9, 3);
        vm.prank(admin);
        vault.setGlobalCap(FerrataVault.Direction.Deposit, 9, 3);

        vm.prank(admin);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.setTokenCap(address(token), FerrataVault.Direction.Deposit, 1, 1);
    }
}

contract RolesTest is VaultTestBase {
    address internal newAdmin = makeAddr("newAdmin");
    address internal newFreezer = makeAddr("newFreezer");

    /// @dev Calls every admin-only function as `caller` and expects each to revert with NotAdmin.
    function _assertNoAdminPowers(address caller) internal {
        vm.startPrank(caller);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.registerToken(address(0xbeef), _denoms(1, 2, 3), 1, 1, _unlimited(), _unlimited());
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setDepositsEnabled(ETH, false);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setTokenCap(ETH, FerrataVault.Direction.Deposit, 0, 0);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setGlobalCap(FerrataVault.Direction.Release, 0, 0);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setUnitWeight(ETH, 1);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setStandardRelayFee(ETH, 0, 1);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setReleaseDelay(ETH, 1);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setMinLezDepth(1);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.transferAdmin(caller);
        vm.stopPrank();
    }

    /// @dev Calls every freeze-authority-only function as `caller` and expects each to revert.
    function _assertNoFreezePowers(address caller) internal {
        vm.startPrank(caller);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setDepositsFrozen(true);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setDepositsFrozen(false);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setReleasesFrozen(true);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setReleasesFrozen(false);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setTokenDepositsFrozen(ETH, true);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setTokenDepositsFrozen(ETH, false);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setTokenReleasesFrozen(ETH, true);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setTokenReleasesFrozen(ETH, false);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.transferFreezeAuthority(caller);
        vm.stopPrank();
    }

    function test_roles_adminCannotFreezeOrUnfreeze() public {
        vm.prank(freezer);
        vault.setReleasesFrozen(true);
        _assertNoFreezePowers(admin);
        assertTrue(vault.releasesFrozen());
    }

    function test_roles_freezeAuthorityCannotAdminister() public {
        _assertNoAdminPowers(freezer);
    }

    function test_roles_strangerHasNoPowers() public {
        _assertNoAdminPowers(alice);
        _assertNoFreezePowers(alice);
    }

    function test_transferAdmin_isTwoStep() public {
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.AdminTransferStarted(admin, newAdmin);
        vm.prank(admin);
        vault.transferAdmin(newAdmin);
        assertEq(vault.admin(), admin); // nothing changes until accepted
        assertEq(vault.pendingAdmin(), newAdmin);

        vm.prank(alice);
        vm.expectRevert(FerrataVault.NotPendingAdmin.selector);
        vault.acceptAdmin();

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.AdminTransferred(admin, newAdmin);
        vm.prank(newAdmin);
        vault.acceptAdmin();
        assertEq(vault.admin(), newAdmin);
        assertEq(vault.pendingAdmin(), address(0));

        _assertNoAdminPowers(admin); // the old admin is out
        vm.prank(newAdmin);
        vault.setMinLezDepth(7);
        assertEq(vault.minLezDepth(), 7);

        vm.prank(newAdmin);
        vm.expectRevert(FerrataVault.NotPendingAdmin.selector); // cannot be accepted twice
        vault.acceptAdmin();
    }

    function test_transferAdmin_canBeCancelledOrReplaced() public {
        vm.startPrank(admin);
        vault.transferAdmin(newAdmin);
        vault.transferAdmin(alice); // replaced
        vm.stopPrank();
        vm.prank(newAdmin);
        vm.expectRevert(FerrataVault.NotPendingAdmin.selector);
        vault.acceptAdmin();

        vm.prank(admin);
        vault.transferAdmin(address(0)); // cancelled
        vm.prank(alice);
        vm.expectRevert(FerrataVault.NotPendingAdmin.selector);
        vault.acceptAdmin();
        assertEq(vault.admin(), admin);
    }

    function test_transferAdmin_cannotBeAcceptedByFreezeAuthority() public {
        vm.prank(admin);
        vault.transferAdmin(freezer);
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.RolesMustDiffer.selector);
        vault.acceptAdmin();
    }

    function test_transferFreezeAuthority_isTwoStepAndOnlyByHolder() public {
        vm.prank(admin); // the admin cannot appoint a freeze authority (it could then unfreeze)
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.transferFreezeAuthority(admin);

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.FreezeAuthorityTransferStarted(freezer, newFreezer);
        vm.prank(freezer);
        vault.transferFreezeAuthority(newFreezer);
        assertEq(vault.freezeAuthority(), freezer);
        assertEq(vault.pendingFreezeAuthority(), newFreezer);

        vm.prank(alice);
        vm.expectRevert(FerrataVault.NotPendingFreezeAuthority.selector);
        vault.acceptFreezeAuthority();

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.FreezeAuthorityTransferred(freezer, newFreezer);
        vm.prank(newFreezer);
        vault.acceptFreezeAuthority();
        assertEq(vault.freezeAuthority(), newFreezer);
        assertEq(vault.pendingFreezeAuthority(), address(0));

        _assertNoFreezePowers(freezer);
        vm.prank(newFreezer);
        vault.setDepositsFrozen(true);
        assertTrue(vault.depositsFrozen());
    }

    function test_transferFreezeAuthority_cannotBeAcceptedByAdmin() public {
        vm.prank(freezer);
        vault.transferFreezeAuthority(admin);
        vm.prank(admin);
        vm.expectRevert(FerrataVault.RolesMustDiffer.selector);
        vault.acceptFreezeAuthority();
    }
}
