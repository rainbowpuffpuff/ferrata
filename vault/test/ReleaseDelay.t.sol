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
        assertEq(vault.releaseDelayFor(address(token), vault.checkpointCount()), 2 days);
        assertEq(vault.releaseDelayFor(address(token), 0), 2 days);

        p.releaseDelay = REVOKE_WINDOW + 1;
        MockERC20 other = new MockERC20("Other", "OTH", 18);
        vm.prank(admin);
        vm.expectRevert(FerrataVault.ReleaseDelayOutOfRange.selector);
        vault.registerToken(address(other), p);
    }

    /// @notice v3 D5: every delay setting, registration included, satisfies minReleaseDelay <= delay <= revokeWindow.
    function test_everyDelaySettingIsBetweenTheMinimumAndTheRevokeWindow() public {
        FerrataVault.Deployment memory d = _deployment(vault.verifier(), LEZ_CONFIG_ID);
        d.minReleaseDelay = 1 hours;
        FerrataVault strict = new FerrataVault(d);
        MockERC20 token = new MockERC20("Token", "TKN", 18);
        FerrataVault.TokenParams memory p = _params(_denoms(1e18, 2e18, 3e18), 1e12, 1);

        p.releaseDelay = 1 hours - 1;
        vm.prank(admin);
        vm.expectRevert(FerrataVault.ReleaseDelayOutOfRange.selector);
        strict.registerToken(address(token), p);
        p.releaseDelay = 1 hours;
        vm.prank(admin);
        strict.registerToken(address(token), p);

        vm.startPrank(admin);
        vm.expectRevert(FerrataVault.ReleaseDelayOutOfRange.selector);
        strict.setReleaseDelay(address(token), 0);
        vm.expectRevert(FerrataVault.ReleaseDelayOutOfRange.selector);
        strict.setReleaseDelay(address(token), REVOKE_WINDOW + 1);
        strict.setReleaseDelay(address(token), REVOKE_WINDOW);
        vm.stopPrank();
        assertEq(strict.releaseDelayFor(address(token), strict.checkpointCount()), REVOKE_WINDOW);
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
                uint256 delay = (seed >> 8) % (REVOKE_WINDOW + 1);
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

    /// @notice v4 (D11): a release whose delay equals revokeWindow opens only after the checkpoint's last revocable
    ///         second; a shorter delay still opens at its own second.
    function test_releaseDelay_equalToRevokeWindowOpensAfterTheLastRevocableSecond() public {
        _deposit(alice, ETH, 1, keccak256("d11 a"));
        _deposit(alice, ETH, 1, keccak256("d11 b"));
        vm.prank(admin);
        vault.setReleaseDelay(ETH, REVOKE_WINDOW);
        bytes32 root = keccak256("d11 root");
        uint256 index = _acceptRoot(root);
        uint256 finalizedAt = block.timestamp;
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, root, keccak256("d11 nullifier"));
        _proveRelease(_releaseJournal(r));
        vm.warp(finalizedAt + REVOKE_WINDOW);
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(r, SEAL);
        uint256 snapshot = vm.snapshotState();
        vm.prank(freezer);
        vault.revokeFrom(index); // that second still belongs to the revocation window
        vm.revertToState(snapshot);
        vm.warp(finalizedAt + REVOKE_WINDOW + 1);
        vm.prank(relayer);
        vault.release(r, SEAL);
        assertEq(recipient.balance, 1 ether);

        vm.prank(admin);
        vault.setReleaseDelay(ETH, REVOKE_WINDOW - 1);
        bytes32 root2 = keccak256("d11 root 2");
        _acceptRoot(root2);
        uint256 at2 = block.timestamp;
        FerrataVault.ReleaseRequest memory r2 = _request(ETH, 1, recipient, root2, keccak256("d11 nullifier 2"));
        _proveRelease(_releaseJournal(r2));
        vm.warp(at2 + REVOKE_WINDOW - 2);
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(r2, SEAL);
        vm.warp(at2 + REVOKE_WINDOW - 1);
        vm.prank(relayer);
        vault.release(r2, SEAL);
        assertEq(recipient.balance, 2 ether);
    }
}
