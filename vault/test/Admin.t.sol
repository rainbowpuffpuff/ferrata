// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockERC20} from "./mocks/MockTokens.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

contract ConstructorTest is VaultTestBase {
    function _expectConstructorRevert(FerrataVault.Deployment memory d, bytes4 expectedError) internal {
        vm.expectRevert(expectedError);
        new FerrataVault(d);
    }

    function _d() internal view returns (FerrataVault.Deployment memory) {
        return _deployment(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
    }

    function test_constructor_setsImmutablesStateAndRoles() public view {
        assertEq(address(vault.verifier()), address(estop));
        assertEq(address(estop.verifier()), address(verifier));
        assertEq(estop.owner(), freezer); // the freeze authority is the stop's guardian
        assertEq(vault.checkpointImageId(), CHECKPOINT_IMAGE_ID);
        assertEq(vault.releaseImageId(), RELEASE_IMAGE_ID);
        assertEq(vault.lezConfigId(), LEZ_CONFIG_ID);
        assertEq(vault.storedLezConfigId(), LEZ_CONFIG_ID);
        assertEq(vault.genesisState(), GENESIS_STATE);
        assertEq(vault.finalState(), GENESIS_STATE);
        assertEq(vault.nMin(), N_MIN);
        assertEq(vault.tMin(), T_MIN);
        assertEq(vault.deltaFloor(), DELTA_FLOOR);
        assertEq(vault.delta(), DELTA);
        assertEq(vault.revokeWindow(), REVOKE_WINDOW);
        assertEq(vault.maxFreeze(), MAX_FREEZE);
        assertEq(vault.turnSeconds(), TURN_SECONDS);
        assertEq(vault.minReleaseDelay(), MIN_RELEASE_DELAY);
        assertEq(vault.finalImportedLeafCount(), 0);
        assertFalse(vault.finalSunset());
        assertEq(vault.admin(), admin);
        assertEq(vault.freezeAuthority(), freezer);
        assertEq(vault.feeRecipient(), feeRecipient);
        assertEq(vault.nextLeafIndex(), 0);
        assertEq(vault.depositRoot(), ReferenceMerkle.zeroHashes()[32]);
        assertEq(vault.checkpointCount(), 0);
        assertFalse(vault.retired());
    }

    function test_constructor_rejectsVerifierWithoutCode() public {
        FerrataVault.Deployment memory d = _d();
        d.verifier = IRiscZeroVerifier(makeAddr("eoa"));
        _expectConstructorRevert(d, FerrataVault.VerifierHasNoCode.selector);
    }

    function test_constructor_rejectsBadImageIds() public {
        FerrataVault.Deployment memory d = _d();
        d.checkpointImageId = bytes32(0);
        _expectConstructorRevert(d, FerrataVault.InvalidImageIds.selector);
        d = _d();
        d.releaseImageId = bytes32(0);
        _expectConstructorRevert(d, FerrataVault.InvalidImageIds.selector);
        d = _d();
        d.releaseImageId = d.checkpointImageId;
        _expectConstructorRevert(d, FerrataVault.InvalidImageIds.selector);
    }

    function test_constructor_rejectsZeroConfigOrGenesis() public {
        FerrataVault.Deployment memory d = _d();
        d.lezConfigId = bytes32(0);
        _expectConstructorRevert(d, FerrataVault.InvalidLezConfigId.selector);
        d = _d();
        d.genesisState = bytes32(0);
        _expectConstructorRevert(d, FerrataVault.InvalidGenesisState.selector);
    }

    function test_constructor_rejectsZeroCommitteeFloor() public {
        FerrataVault.Deployment memory d = _d();
        d.nMin = 0;
        _expectConstructorRevert(d, FerrataVault.InvalidCommitteeFloor.selector);
        d = _d();
        d.tMin = 0;
        _expectConstructorRevert(d, FerrataVault.InvalidCommitteeFloor.selector);
    }

    function test_constructor_deltaRules() public {
        FerrataVault.Deployment memory d = _d();
        d.deltaFloor = 0;
        d.initialDelta = 0;
        _expectConstructorRevert(d, FerrataVault.InvalidDelta.selector);
        d = _d();
        d.initialDelta = DELTA_FLOOR - 1;
        _expectConstructorRevert(d, FerrataVault.InvalidDelta.selector);
        d = _d();
        d.initialDelta = 30 days + 1;
        _expectConstructorRevert(d, FerrataVault.InvalidDelta.selector);
        d = _d();
        d.revokeWindow = 0;
        _expectConstructorRevert(d, FerrataVault.InvalidRevokeWindow.selector);
        d = _d();
        d.maxFreeze = 0;
        _expectConstructorRevert(d, FerrataVault.InvalidMaxFreeze.selector);
    }

    function test_constructor_turnSecondsAndMinReleaseDelay() public {
        FerrataVault.Deployment memory d = _d();
        d.turnSeconds = 0;
        _expectConstructorRevert(d, FerrataVault.InvalidTurnSeconds.selector);
        d = _d();
        d.turnSeconds = 30 days + 1;
        _expectConstructorRevert(d, FerrataVault.InvalidTurnSeconds.selector);
        d = _d();
        d.minReleaseDelay = REVOKE_WINDOW + 1; // no delay could satisfy minReleaseDelay <= delay <= revokeWindow
        _expectConstructorRevert(d, FerrataVault.InvalidMinReleaseDelay.selector);
        d = _d();
        d.revokeWindow = 40 days;
        d.minReleaseDelay = 31 days; // within the window, but above the 30-day delay ceiling
        _expectConstructorRevert(d, FerrataVault.InvalidMinReleaseDelay.selector);
        d = _d();
        d.minReleaseDelay = REVOKE_WINDOW; // the tightest valid setting
        assertEq(new FerrataVault(d).minReleaseDelay(), REVOKE_WINDOW);
    }

    function test_constructor_rejectsZeroOrSharedRoles() public {
        FerrataVault.Deployment memory d = _d();
        d.admin = address(0);
        _expectConstructorRevert(d, FerrataVault.ZeroAddress.selector);
        d = _d();
        d.freezeAuthority = address(0);
        _expectConstructorRevert(d, FerrataVault.ZeroAddress.selector);
        d = _d();
        d.feeRecipient = address(0); // D13: burn fees need a recipient
        _expectConstructorRevert(d, FerrataVault.ZeroAddress.selector);
        d = _d();
        d.freezeAuthority = d.admin;
        _expectConstructorRevert(d, FerrataVault.RolesMustDiffer.selector);
    }

    function test_constructor_globalCapsStartFull() public {
        FerrataVault.Deployment memory d = _d();
        d.globalDepositCap = _cap(500, 1);
        d.globalReleaseCap = _cap(700, 2);
        FerrataVault v = new FerrataVault(d);
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

    /// @dev Registration parameters for `token` with finite caps.
    function _p(uint256[] memory denoms, uint256 lezUnit, uint256 unitWeight)
        internal
        pure
        returns (FerrataVault.TokenParams memory p)
    {
        p = _params(denoms, lezUnit, unitWeight);
        p.depositCap = _cap(1000e18, 1e18);
        p.releaseCap = _cap(2000e18, 2e18);
    }

    function _register(FerrataVault.TokenParams memory p) internal {
        vm.prank(admin);
        vault.registerToken(address(token), p);
    }

    function _expectRegisterRevert(FerrataVault.TokenParams memory p, bytes4 expectedError) internal {
        vm.prank(admin);
        vm.expectRevert(expectedError);
        vault.registerToken(address(token), p);
    }

    function _one(uint256 a) internal pure returns (uint256[] memory d) {
        d = new uint256[](1);
        d[0] = a;
    }

    function test_registerToken_storesConfigAndEmits() public {
        FerrataVault.TokenParams memory p = _p(_denoms(1e18, 5e18, 25e18), 1e12, 7);
        p.relayFees = _denoms(0.01e18, 0.05e18, 0.25e18); // exactly 1% each, the ceiling
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.TokenRegistered(address(token), p.denominations, p.relayFees, 1e12, 7);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.TokenCapSet(address(token), FerrataVault.Direction.Deposit, 1000e18, 1e18);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.TokenCapSet(address(token), FerrataVault.Direction.Release, 2000e18, 2e18);
        _register(p);

        (bool registered, bool enabled, uint256 lezUnit, uint256 weight, uint256[] memory stored) =
            vault.tokenConfig(address(token));
        assertTrue(registered);
        assertTrue(enabled);
        assertEq(lezUnit, 1e12);
        assertEq(weight, 7);
        assertEq(stored, p.denominations);
        assertEq(_relayFeesOf(vault, address(token)), p.relayFees);
        (uint256 capacity,, uint256 available) = vault.tokenCap(address(token), FerrataVault.Direction.Release);
        assertEq(capacity, 2000e18);
        assertEq(available, 2000e18); // new buckets start full
    }

    function test_registerToken_onlyAdmin() public {
        FerrataVault.TokenParams memory p = _p(_denoms(1e18, 5e18, 25e18), 1e12, 1);
        address[2] memory notAdmin = [freezer, alice];
        for (uint256 i = 0; i < notAdmin.length; i++) {
            vm.prank(notAdmin[i]);
            vm.expectRevert(FerrataVault.NotAdmin.selector);
            vault.registerToken(address(token), p);
        }
    }

    /// @notice Spec §3.4: (token, lezUnit, denominations, relay fees, unitWeight) are write-once.
    function test_registerToken_isWriteOnce() public {
        _register(_p(_denoms(1e18, 5e18, 25e18), 1e12, 1));
        _expectRegisterRevert(_p(_denoms(2e18, 5e18, 25e18), 1e12, 1), FerrataVault.TokenAlreadyRegistered.selector);

        // The v1 setters that could change a registered token are gone: calling them hits the fallback.
        bytes[3] memory removed = [
            abi.encodeWithSignature("setUnitWeight(address,uint256)", address(token), 2),
            abi.encodeWithSignature("setStandardRelayFee(address,uint8,uint256)", address(token), 0, 1),
            abi.encodeWithSignature(
                "registerToken(address,uint256[],uint256,uint256,(uint256,uint256),(uint256,uint256))",
                address(token),
                _one(1e18),
                1e12,
                1,
                _unlimited(),
                _unlimited()
            )
        ];
        for (uint256 i = 0; i < removed.length; i++) {
            vm.prank(admin);
            (bool ok, bytes memory ret) = address(vault).call(removed[i]);
            assertFalse(ok);
            assertEq(ret, abi.encodeWithSelector(FerrataVault.FunctionNotFound.selector));
        }
        assertEq(_denomsOf(vault, address(token)), _denoms(1e18, 5e18, 25e18));
    }

    function test_registerToken_rejectsAddressWithoutCode() public {
        vm.prank(admin);
        vm.expectRevert(FerrataVault.TokenHasNoCode.selector);
        vault.registerToken(makeAddr("not a token"), _p(_one(1e18), 1e12, 1));
    }

    function test_registerToken_denominationRules() public {
        _expectRegisterRevert(_p(new uint256[](0), 1, 1), FerrataVault.InvalidDenominations.selector);

        uint256[] memory nine = new uint256[](9);
        for (uint256 i = 0; i < 9; i++) {
            nine[i] = (i + 1) * 100;
        }
        _expectRegisterRevert(_p(nine, 1, 1), FerrataVault.InvalidDenominations.selector);
        _expectRegisterRevert(_p(_denoms(0, 100, 200), 1, 1), FerrataVault.InvalidDenominations.selector); // zero
        _expectRegisterRevert(_p(_denoms(100, 200, 200), 1, 1), FerrataVault.InvalidDenominations.selector); // repeated
        _expectRegisterRevert(_p(_denoms(100, 300, 200), 1, 1), FerrataVault.InvalidDenominations.selector); // decreasing
        _expectRegisterRevert(
            _p(_one(uint256(type(uint128).max) + 1), 1, 1), FerrataVault.InvalidDenominations.selector
        );

        uint256[] memory eight = new uint256[](8);
        for (uint256 i = 0; i < 8; i++) {
            eight[i] = (i + 1) * 100;
        }
        _register(_p(eight, 1, 1));
        assertEq(_denomsOf(vault, address(token)).length, 8);
    }

    function test_registerToken_lezUnitMustBePowerOfTen() public {
        uint256[6] memory bad = [uint256(0), 2, 20, 15, 1e4 + 1, 3e4];
        for (uint256 i = 0; i < bad.length; i++) {
            _expectRegisterRevert(_p(_one(9e4), bad[i], 1), FerrataVault.InvalidLezUnit.selector);
        }
    }

    function testFuzz_registerToken_lezUnitPowersOfTenAccepted(uint8 exponent) public {
        uint256 k = bound(exponent, 0, 38); // 10**38 is the largest power of ten below MAX_DENOMINATION
        uint256 unit = 10 ** k;
        _register(_p(_one(unit), unit, 1));
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
        _expectRegisterRevert(_p(_one(1e18), unit, 1), FerrataVault.InvalidLezUnit.selector);
    }

    function test_registerToken_denominationsMustBeMultiplesOfLezUnit() public {
        MockERC20 sixDecimals = new MockERC20("Six", "SIX", 6);
        vm.startPrank(admin);
        // 1.234567 units of a 6-decimal token is not representable with 2 decimals on LEZ (lezUnit 1e4).
        vm.expectRevert(FerrataVault.DenominationNotMultipleOfLezUnit.selector);
        vault.registerToken(address(sixDecimals), _p(_denoms(1e6, 1_234_567, 10e6), 1e4, 1));
        // 1 ether + 1 wei is not representable with 6 decimals on LEZ (lezUnit 1e12).
        vm.expectRevert(FerrataVault.DenominationNotMultipleOfLezUnit.selector);
        vault.registerToken(address(token), _p(_denoms(1e17, 1e18 + 1, 1e19), 1e12, 1));
        // The same tokens with representable amounts are accepted.
        vault.registerToken(address(sixDecimals), _p(_denoms(1e6, 1_230_000, 10e6), 1e4, 1));
        vault.registerToken(address(token), _p(_denoms(1e17, 1e18, 1e19), 1e12, 1));
        vm.stopPrank();
    }

    function test_registerToken_unitWeightBounds() public {
        _expectRegisterRevert(_p(_one(1e18), 1e12, 0), FerrataVault.InvalidUnitWeight.selector);
        _expectRegisterRevert(
            _p(_one(1e18), 1e12, uint256(type(uint128).max) + 1), FerrataVault.InvalidUnitWeight.selector
        );
    }

    // ---- relay fees: one per denomination, at most 1%, fixed at registration (spec §3.4) ------------------------

    function test_registerToken_relayFeeAtMostOnePercent() public {
        FerrataVault.TokenParams memory p = _p(_denoms(1e18, 5e18, 25e18), 1e12, 1);
        p.relayFees = _denoms(0.01e18, 0.05e18 + 1, 0); // the second is 1 wei over 1%
        _expectRegisterRevert(p, FerrataVault.RelayFeeTooHigh.selector);

        // 1% of 150 is 1.5: a fee of 1 is allowed, 2 is not.
        FerrataVault.TokenParams memory odd = _p(_denoms(150, 300, 450), 1, 1);
        odd.relayFees = _denoms(2, 0, 0);
        _expectRegisterRevert(odd, FerrataVault.RelayFeeTooHigh.selector);
        odd.relayFees = _denoms(1, 3, 4);
        _register(odd);
        assertEq(_relayFeesOf(vault, address(token)), _denoms(1, 3, 4));
    }

    function testFuzz_registerToken_relayFeeCeiling(uint256 denomination, uint256 fee) public {
        denomination = bound(denomination, 1, type(uint128).max);
        FerrataVault.TokenParams memory p = _p(_one(denomination), 1, 1);
        p.relayFees = _one(fee);
        bool withinOnePercent = fee <= denomination / 100; // fee * 100 <= denomination, without overflow
        vm.prank(admin);
        if (!withinOnePercent) vm.expectRevert(FerrataVault.RelayFeeTooHigh.selector);
        vault.registerToken(address(token), p);
    }

    function test_registerToken_oneRelayFeePerDenomination() public {
        FerrataVault.TokenParams memory p = _p(_denoms(1e18, 5e18, 25e18), 1e12, 1);
        p.relayFees = _one(0);
        _expectRegisterRevert(p, FerrataVault.RelayFeeCountMismatch.selector);
        p.relayFees = new uint256[](4);
        _expectRegisterRevert(p, FerrataVault.RelayFeeCountMismatch.selector);
        p.relayFees = new uint256[](3); // all zero: relaying without a fee
        _register(p);
    }

    // ---- what stays adjustable: caps, depositsEnabled, release delays ---------------------------------------------

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

    function test_setReleaseDelay() public {
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.ReleaseDelaySet(address(dai), REVOKE_WINDOW, 0);
        vm.prank(admin);
        vault.setReleaseDelay(address(dai), REVOKE_WINDOW);
        assertEq(vault.releaseDelayFor(address(dai), vault.checkpointCount()), REVOKE_WINDOW);

        vm.startPrank(admin);
        vm.expectRevert(FerrataVault.ReleaseDelayOutOfRange.selector);
        vault.setReleaseDelay(address(dai), REVOKE_WINDOW + 1); // a root must stay revocable while it is delayed
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.setReleaseDelay(address(token), 1);
        vm.stopPrank();
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
        vault.registerToken(address(0xbeef), _params(_denoms(100, 200, 300), 1, 1));
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setDepositsEnabled(ETH, false);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setTokenCap(ETH, FerrataVault.Direction.Deposit, 0, 0);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setGlobalCap(FerrataVault.Direction.Release, 0, 0);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setReleaseDelay(ETH, 1);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.raiseDelta(DELTA + 1);
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
        vault.setCheckpointsFrozen(true);
        // v4 (D13): anyone may retire, but only 30 days after the last finalisation.
        vm.expectRevert(FerrataVault.RetireNotOpen.selector);
        vault.retire();
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.revokeFrom(0);
        vm.stopPrank();
    }

    function test_roles_adminCannotFreezeOrUnfreeze() public {
        vm.prank(freezer);
        vault.setReleasesFrozen(true);
        _assertNoFreezePowers(admin);
        assertTrue(_isFrozen(vault.releasesFrozenUntil()));
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
        vault.raiseDelta(DELTA + 7);
        assertEq(vault.delta(), DELTA + 7);

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

    // v4 (D13): the freeze authority has no two-step handover of its own any more; the admin replaces it through a
    // 14-day public proposal (test/Roles.t.sol).
}
