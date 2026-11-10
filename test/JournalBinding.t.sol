// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockRiscZeroVerifier} from "./mocks/MockVerifiers.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Every field of the release journal is bound: a proof whose journal differs from the vault's
///         reconstruction in exactly one field is rejected. Each test also runs a control in which the same
///         request succeeds once its own journal is proven, so the rejection can only come from the mismatch.
contract ReleaseJournalBindingTest is VaultTestBase {
    bytes32 internal constant ROOT_A = keccak256("burn root A");
    bytes32 internal constant ROOT_B = keccak256("burn root B");
    bytes32 internal constant NULLIFIER = keccak256("nullifier");
    uint256 internal constant FEE = 0.002 ether;
    address internal feeTaker = makeAddr("feeTaker");

    function setUp() public override {
        super.setUp();
        _deposit(alice, ETH, 1, keccak256("c1"));
        _deposit(alice, ETH, 1, keccak256("c2"));
        _acceptRoot(ROOT_A);
        _acceptRoot(ROOT_B);
        vm.prank(admin);
        vault.setStandardRelayFee(ETH, 1, FEE);
    }

    /// @dev 1 ETH to `recipient`, standard fee bound to `feeTaker`, under ROOT_A.
    function _honestRequest() internal view returns (FerrataVault.ReleaseRequest memory) {
        return FerrataVault.ReleaseRequest({
            token: ETH,
            denomIndex: 1,
            recipient: recipient,
            feeRecipient: feeTaker,
            relayFee: FEE,
            burnRoot: ROOT_A,
            nullifier: NULLIFIER
        });
    }

    function _honestJournal() internal view returns (ReleaseJournal memory) {
        return _releaseJournal(_honestRequest());
    }

    function _assertMismatchRejected(ReleaseJournal memory proven) internal {
        FerrataVault.ReleaseRequest memory r = _honestRequest();
        _proveRelease(proven);
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(r, SEAL);
        assertFalse(vault.nullifierUsed(NULLIFIER));

        _proveRelease(_releaseJournal(r)); // control
        vm.prank(relayer);
        vault.release(r, SEAL);
        assertTrue(vault.nullifierUsed(NULLIFIER));
    }

    // ---- one test per journal field -----------------------------------------------------------------------------

    function test_releaseJournal_bindsChainId() public {
        ReleaseJournal memory j = _honestJournal();
        j.chainId = block.chainid + 1;
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsVault() public {
        ReleaseJournal memory j = _honestJournal();
        j.vault = makeAddr("another vault");
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsLezConfigId() public {
        ReleaseJournal memory j = _honestJournal();
        j.lezConfigId = keccak256("another LEZ configuration");
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsBurnRoot() public {
        ReleaseJournal memory j = _honestJournal();
        j.burnRoot = ROOT_B;
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsNullifier() public {
        ReleaseJournal memory j = _honestJournal();
        j.nullifier = keccak256("another nullifier");
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsToken() public {
        ReleaseJournal memory j = _honestJournal();
        j.token = address(dai);
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsAmount() public {
        ReleaseJournal memory j = _honestJournal();
        j.amount = 0.1 ether;
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsRecipient() public {
        ReleaseJournal memory j = _honestJournal();
        j.recipient = bob;
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsFeeRecipient() public {
        ReleaseJournal memory j = _honestJournal();
        j.feeRecipient = address(0);
        _assertMismatchRejected(j);
    }

    function test_releaseJournal_bindsRelayFee() public {
        ReleaseJournal memory j = _honestJournal();
        j.relayFee = 0;
        _assertMismatchRejected(j);
    }

    // ---- the same property as concrete attacks (Security #2, #3) --------------------------------------------------

    function test_attack_relayerCannotRedirectFunds() public {
        _proveRelease(_honestJournal()); // the user's proof
        FerrataVault.ReleaseRequest memory forged = _honestRequest();
        forged.recipient = relayer;
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(forged, SEAL);
    }

    function test_attack_relayerCannotAddAFee() public {
        FerrataVault.ReleaseRequest memory honest = _honestRequest();
        honest.relayFee = 0;
        honest.feeRecipient = address(0);
        _proveRelease(_releaseJournal(honest)); // the user chose to pay no fee
        FerrataVault.ReleaseRequest memory forged = _honestRequest();
        forged.feeRecipient = address(0);
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(forged, SEAL);
    }

    function test_attack_relayerCannotRedirectTheFee() public {
        _proveRelease(_honestJournal()); // fee bound to feeTaker
        FerrataVault.ReleaseRequest memory forged = _honestRequest();
        forged.feeRecipient = relayer;
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(forged, SEAL);
    }

    function test_attack_proofCannotBeReplayedOnAnotherVault() public {
        FerrataVault other = _deployVault(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        _registerDefaultTokens(other);
        _depositInto(other, alice, ETH, 1, keccak256("c3"));
        RootJournal memory rj = _rootJournal(ROOT_A, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH);
        rj.vault = address(other);
        _proveRoot(rj);
        other.acceptBurnRoot(ROOT_A, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL);
        vm.prank(admin);
        other.setStandardRelayFee(ETH, 1, FEE);

        _proveRelease(_honestJournal()); // valid for `vault` only
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        other.release(_honestRequest(), SEAL);

        vm.prank(relayer);
        vault.release(_honestRequest(), SEAL);
    }

    function test_attack_proofCannotBeReplayedOnAnotherChain() public {
        _proveRelease(_honestJournal());
        uint256 home = block.chainid;
        vm.chainId(home + 1); // e.g. a fork that kept the state but changed the chain id
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.release(_honestRequest(), SEAL);

        vm.chainId(home);
        vm.prank(relayer);
        vault.release(_honestRequest(), SEAL);
    }

    function test_attack_proofCannotBeReplayedAcrossLezConfigurations() public {
        FerrataVault other = _deployVault(IRiscZeroVerifier(address(estop)), keccak256("another LEZ configuration"));
        _registerDefaultTokens(other);
        _depositInto(other, alice, ETH, 1, keccak256("c3"));
        RootJournal memory rj = _rootJournal(ROOT_A, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH);
        rj.vault = address(other);
        rj.lezConfigId = keccak256("another LEZ configuration");
        _proveRoot(rj);
        other.acceptBurnRoot(ROOT_A, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL);
        vm.prank(admin);
        other.setStandardRelayFee(ETH, 1, FEE);

        _proveRelease(_honestJournal());
        vm.prank(relayer);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        other.release(_honestRequest(), SEAL);
    }
}

/// @notice Every field of the (provisional) burn-root journal is bound, with the same control as above.
contract RootJournalBindingTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("burn root");
    uint32 internal constant DEPTH = MIN_DEPTH + 1;

    function _honestJournal() internal view returns (RootJournal memory) {
        return _rootJournal(ROOT, LEZ_BLOCK, BURN_COUNT, DEPTH);
    }

    function _assertMismatchRejected(RootJournal memory proven) internal {
        _proveRoot(proven);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.acceptBurnRoot(ROOT, LEZ_BLOCK, BURN_COUNT, DEPTH, SEAL);
        assertEq(vault.burnRootAcceptedAt(ROOT), 0);

        _proveRoot(_honestJournal()); // control
        vault.acceptBurnRoot(ROOT, LEZ_BLOCK, BURN_COUNT, DEPTH, SEAL);
        assertEq(vault.burnRootAcceptedAt(ROOT), block.timestamp);
    }

    function test_rootJournal_bindsChainId() public {
        RootJournal memory j = _honestJournal();
        j.chainId = block.chainid + 1;
        _assertMismatchRejected(j);
    }

    function test_rootJournal_bindsVault() public {
        RootJournal memory j = _honestJournal();
        j.vault = makeAddr("another vault");
        _assertMismatchRejected(j);
    }

    function test_rootJournal_bindsLezConfigId() public {
        RootJournal memory j = _honestJournal();
        j.lezConfigId = keccak256("another LEZ configuration");
        _assertMismatchRejected(j);
    }

    function test_rootJournal_bindsLezBlockId() public {
        RootJournal memory j = _honestJournal();
        j.lezBlockId = LEZ_BLOCK + 1;
        _assertMismatchRejected(j);
    }

    function test_rootJournal_bindsBurnRoot() public {
        RootJournal memory j = _honestJournal();
        j.burnRoot = keccak256("another burn root");
        _assertMismatchRejected(j);
    }

    function test_rootJournal_bindsBurnCount() public {
        RootJournal memory j = _honestJournal();
        j.burnCount = BURN_COUNT + 1;
        _assertMismatchRejected(j);
    }

    function test_rootJournal_bindsDepth() public {
        // The proof attests a shallower depth than the caller claims.
        RootJournal memory j = _honestJournal();
        j.depth = MIN_DEPTH;
        _assertMismatchRejected(j);
    }
}
