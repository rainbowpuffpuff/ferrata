// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";

import {MockRiscZeroVerifier} from "./mocks/MockVerifiers.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice RFP-022 Priv.5 on Ethereum: "Failure and error paths must not reveal which deposit or burn was involved: a
///         rejected claim, a repeat claim, and a cap rejection must be indistinguishable in that respect." The vault's
///         side of a burn is the release. A rejected release (a journal no proof committed), a repeat and a cap
///         rejection are each refused with an error that carries no argument: the revert data is the four bytes of the
///         error's selector and nothing else, so it names no burn. Their calldata is the release request a successful
///         release sends, byte for byte (`ReleaseRequest`: token, denomination index, recipient, relayer, relay fee,
///         burn root, NOut, and the seal), so a refusal publishes nothing a release would not; and a release request
///         holds no field of the LEZ burn (its leaf index, tag `r`, `COut` or holder), since the release proof hides
///         which leaf of the burn root it opens. The same for the three refusals of a deposit. (The LEZ half:
///         `lez/harness/tests/bridge_refusals.rs`.)
contract RefusalPrivacyTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("burn root");
    bytes32 internal constant N1 = keccak256("NOut 1");
    bytes32 internal constant N2 = keccak256("NOut 2");

    /// @dev Calls the vault with `data` from the relayer; returns the revert data (empty if it applied).
    function _try(bytes memory data) internal returns (bool ok, bytes memory ret) {
        vm.prank(relayer);
        (ok, ret) = address(vault).call(data);
    }

    function _releaseCall(FerrataVault.ReleaseRequest memory r) internal pure returns (bytes memory) {
        return abi.encodeCall(FerrataVault.release, (r, SEAL));
    }

    function test_releaseRefusals_revealNoBurn() public {
        _deposit(alice, ETH, 1, keccak256("c1"));
        _deposit(alice, ETH, 1, keccak256("c2"));
        _acceptRoot(ROOT);
        vm.warp(block.timestamp + 1 days);
        FerrataVault.ReleaseRequest memory r1 = _request(ETH, 1, recipient, ROOT, N1);
        FerrataVault.ReleaseRequest memory r2 = _request(ETH, 1, recipient, ROOT, N2);

        // Rejected: no proof committed this journal.
        (bool ok, bytes memory rejected) = _try(_releaseCall(r1));
        assertFalse(ok);
        assertEq(rejected, abi.encodeWithSelector(MockRiscZeroVerifier.VerificationFailed.selector));

        // Applied, then repeated.
        _proveRelease(_releaseJournal(r1));
        bytes memory call1 = _releaseCall(r1);
        (ok,) = _try(call1);
        assertTrue(ok, "the proven release applies");
        (bool again, bytes memory repeated) = _try(call1);
        assertFalse(again);
        assertEq(repeated, abi.encodeWithSelector(FerrataVault.NullifierAlreadyUsed.selector));

        // Capped: the admin sets ETH's release bucket at one denomination less one wei a day.
        vm.prank(admin);
        vault.setTokenCap(ETH, FerrataVault.Direction.Release, 1 ether - 1, 1);
        _proveRelease(_releaseJournal(r2));
        bytes memory call2 = _releaseCall(r2);
        bytes memory capped;
        (ok, capped) = _try(call2);
        assertFalse(ok);
        assertEq(capped, abi.encodeWithSelector(FerrataVault.TokenReleaseCapExceeded.selector));

        // Every refusal: four bytes, the selector alone. Every call: the request's encoding, the same length.
        assertEq(rejected.length, 4);
        assertEq(repeated.length, 4);
        assertEq(capped.length, 4);
        assertEq(call1.length, call2.length);
        assertEq(_releaseCall(r1).length, call1.length);
    }

    function test_depositRefusals_revealNothingBeyondTheirCalldata() public {
        uint256 total = _depositTotal(vault, ETH, 1);
        vm.deal(alice, 10 * total);
        // Wrong denomination index, delisted, capped: argless errors.
        uint64 j = vault.depositCount(alice);
        vm.prank(alice);
        (bool ok, bytes memory a) = address(vault).call{value: total}(
            abi.encodeCall(FerrataVault.depositETH, (9, keccak256("c"), j))
        );
        assertFalse(ok);
        assertEq(a, abi.encodeWithSelector(FerrataVault.InvalidDenominationIndex.selector));
        vm.prank(admin);
        vault.setDepositsEnabled(ETH, false);
        vm.prank(alice);
        (ok, a) = address(vault).call{value: total}(abi.encodeCall(FerrataVault.depositETH, (1, keccak256("c"), j)));
        assertFalse(ok);
        assertEq(a, abi.encodeWithSelector(FerrataVault.DepositsDisabled.selector));
        vm.prank(admin);
        vault.setDepositsEnabled(ETH, true);
        vm.prank(admin);
        vault.setTokenCap(ETH, FerrataVault.Direction.Deposit, 1 ether - 1, 1);
        vm.prank(alice);
        (ok, a) = address(vault).call{value: total}(abi.encodeCall(FerrataVault.depositETH, (1, keccak256("c"), j)));
        assertFalse(ok);
        assertEq(a, abi.encodeWithSelector(FerrataVault.TokenDepositCapExceeded.selector));
        assertEq(a.length, 4);
        assertEq(vault.depositCount(alice), j, "nothing deposited");
    }
}
