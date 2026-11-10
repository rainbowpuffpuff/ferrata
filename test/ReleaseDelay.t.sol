// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {MockERC20} from "./mocks/MockTokens.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Spec §5.3: a release-delay change applies only to burn roots finalised after the change.
contract ReleaseDelayTest is VaultTestBase {
    bytes32 internal constant ROOT_1 = keccak256("burn root 1");
    bytes32 internal constant ROOT_2 = keccak256("burn root 2");

    function setUp() public override {
        super.setUp();
        for (uint256 i = 0; i < 4; i++) {
            _deposit(alice, ETH, 1, keccak256(abi.encode("c", i)));
        }
    }

    function _proven(bytes32 root, bytes32 nullifier) internal returns (FerrataVault.ReleaseRequest memory r) {
        r = _request(ETH, 1, recipient, root, nullifier);
        _proveRelease(_releaseJournal(r));
    }

    function test_raisingTheDelayAppliesOnlyToLaterRoots() public {
        _acceptRoot(ROOT_1); // finalised under delay 0
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.ReleaseDelaySet(ETH, 1 days, 1);
        vm.prank(admin);
        vault.setReleaseDelay(ETH, 1 days);
        _acceptRoot(ROOT_2); // finalised under 1 day

        vault.release(_proven(ROOT_1, keccak256("n1")), SEAL); // no wait for the older root

        FerrataVault.ReleaseRequest memory r2 = _proven(ROOT_2, keccak256("n2"));
        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(r2, SEAL);
        vm.warp(_finalizedAt(ROOT_2) + 1 days - 1);
        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(r2, SEAL);
        vm.warp(_finalizedAt(ROOT_2) + 1 days);
        vault.release(r2, SEAL);
    }

    function test_loweringTheDelayDoesNotShortenExistingRoots() public {
        vm.prank(admin);
        vault.setReleaseDelay(ETH, 1 days);
        _acceptRoot(ROOT_1); // under 1 day
        vm.prank(admin);
        vault.setReleaseDelay(ETH, 0);
        _acceptRoot(ROOT_2); // under 0

        FerrataVault.ReleaseRequest memory r1 = _proven(ROOT_1, keccak256("n1"));
        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(r1, SEAL);
        vault.release(_proven(ROOT_2, keccak256("n2")), SEAL);
        vm.warp(_finalizedAt(ROOT_1) + 1 days);
        vault.release(r1, SEAL);
    }

    function test_delayIsPerToken() public {
        vm.prank(admin);
        vault.setReleaseDelay(address(dai), 1 days);
        _acceptRoot(ROOT_1);
        vault.release(_proven(ROOT_1, keccak256("n1")), SEAL); // ETH has no delay
    }

    function test_registrationSetsTheInitialDelay() public {
        MockERC20 token = new MockERC20("Token", "TKN", 18);
        FerrataVault.TokenParams memory p = _params(_denoms(1e18, 2e18, 3e18), 1e12, 1);
        p.releaseDelay = 2 days;
        vm.prank(admin);
        vault.registerToken(address(token), p);
        assertEq(vault.releaseDelay(address(token)), 2 days);
        assertEq(vault.releaseDelayFor(address(token), 0), 2 days);

        p.releaseDelay = 30 days + 1;
        MockERC20 other = new MockERC20("Other", "OTH", 18);
        vm.prank(admin);
        vm.expectRevert(FerrataVault.ReleaseDelayTooLong.selector);
        vault.registerToken(address(other), p);
    }

    function test_severalChangesBeforeOneCheckpoint_lastOneWins() public {
        vm.startPrank(admin);
        vault.setReleaseDelay(ETH, 1 hours);
        vault.setReleaseDelay(ETH, 3 hours);
        vm.stopPrank();
        _acceptRoot(ROOT_1);
        assertEq(vault.releaseDelayFor(ETH, 0), 3 hours);
    }

    /// @notice The binary search over the delay history agrees with a linear scan, for any sequence of changes
    ///         and finalisations.
    function testFuzz_delayLookupMatchesALinearScan(uint256 seed) public {
        uint256[] memory fromCheckpoint = new uint256[](40);
        uint256[] memory delays = new uint256[](40);
        uint256 changes = 1; // registration: delay 0 from checkpoint 0
        uint256 checkpointsMade;
        for (uint256 step = 0; step < 24; step++) {
            seed = uint256(keccak256(abi.encode(seed, step)));
            if (seed % 3 == 0) {
                uint256 delay = (seed >> 8) % 7 days;
                vm.prank(admin);
                vault.setReleaseDelay(ETH, delay);
                fromCheckpoint[changes] = vault.checkpointCount();
                delays[changes] = delay;
                changes++;
            } else {
                _acceptRoot(keccak256(abi.encode("root", step)));
                checkpointsMade++;
            }
        }
        for (uint256 index = 0; index <= checkpointsMade; index++) {
            uint256 expected = 0;
            for (uint256 k = 0; k < changes; k++) {
                if (fromCheckpoint[k] <= index) expected = delays[k];
            }
            assertEq(vault.releaseDelayFor(ETH, index), expected);
        }
    }
}
