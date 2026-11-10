// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {ReentrancyGuardTransient} from "@openzeppelin/contracts/utils/ReentrancyGuardTransient.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {
    GasBurnerRecipient, ReentrantRecipient, RevertBombRecipient, ToggleRecipient
} from "./mocks/MockRecipients.sol";
import {MockBlacklistToken} from "./mocks/MockTokens.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Spec §5.4: when a push payment fails, the vault credits `owed[payee][token]`; the release (or refund)
///         still completes, and only that payee can withdraw later with `claimOwed`.
contract PullPaymentTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("burn root");
    MockBlacklistToken internal busd;
    uint256 internal nonce;

    function setUp() public override {
        super.setUp();
        busd = new MockBlacklistToken();
        vm.prank(admin);
        vault.registerToken(address(busd), _params(_denoms(100e6, 1000e6, 10_000e6), 1, 1));
        for (uint256 i = 0; i < 4; i++) {
            _deposit(alice, ETH, 1, keccak256(abi.encode("eth", i)));
            _deposit(alice, address(busd), 1, keccak256(abi.encode("busd", i)));
        }
        _acceptRoot(ROOT);
    }

    function _releaseTo(address token, address to, address relayer_, bool withFee)
        internal
        returns (FerrataVault.ReleaseRequest memory r)
    {
        r = _request(token, 1, to, ROOT, keccak256(abi.encode("nullifier", nonce++)));
        r.relayer = relayer_;
        if (withFee) r.relayFee = _relayFee(token, 1);
        _release(r);
    }

    function test_ethRecipientThatRejects_isCreditedAndClaimsLater() public {
        ToggleRecipient payee = new ToggleRecipient(); // closed
        uint256 vaultBalance = address(vault).balance;

        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, address(payee), ROOT, keccak256("n"));
        _proveRelease(_releaseJournal(r));
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.OwedCredited(address(payee), ETH, 1 ether);
        vm.prank(relayer);
        vault.release(r, SEAL);

        assertTrue(vault.nullifierUsed(r.nullifier)); // the release happened
        assertEq(vault.escrowed(ETH), 3 ether);
        assertEq(vault.owed(address(payee), ETH), 1 ether);
        assertEq(vault.totalOwed(ETH), 1 ether);
        assertEq(address(vault).balance, vaultBalance); // the ETH is still in the vault, now owed

        vm.expectRevert(FerrataVault.EthTransferFailed.selector);
        payee.claim(address(vault), ETH); // still closed: the claim reverts and the credit stays

        payee.setOpen(true);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.OwedClaimed(address(payee), ETH, 1 ether);
        payee.claim(address(vault), ETH);
        assertEq(address(payee).balance, 1 ether);
        assertEq(vault.owed(address(payee), ETH), 0);
        assertEq(vault.totalOwed(ETH), 0);
    }

    function test_blacklistedRecipient_isCreditedAndClaimsOnceUnblacklisted() public {
        busd.setBlacklisted(recipient, true);
        _releaseTo(address(busd), recipient, address(0), true);
        uint256 net = 1000e6 - _relayFee(address(busd), 1);
        assertEq(vault.owed(recipient, address(busd)), net);
        assertEq(busd.balanceOf(relayer), _relayFee(address(busd), 1)); // the relayer was paid normally

        vm.prank(recipient);
        vm.expectRevert(MockBlacklistToken.Blacklisted.selector);
        vault.claimOwed(address(busd));

        busd.setBlacklisted(recipient, false);
        vm.prank(recipient);
        vault.claimOwed(address(busd));
        assertEq(busd.balanceOf(recipient), net);
        assertEq(vault.totalOwed(address(busd)), 0);
    }

    function test_relayerThatCannotReceive_isCredited() public {
        ToggleRecipient relayerContract = new ToggleRecipient(); // closed
        _releaseTo(ETH, recipient, address(relayerContract), true);
        uint256 fee = _relayFee(ETH, 1);
        assertEq(recipient.balance, 1 ether - fee);
        assertEq(vault.owed(address(relayerContract), ETH), fee);

        relayerContract.setOpen(true);
        relayerContract.claim(address(vault), ETH);
        assertEq(address(relayerContract).balance, fee);
    }

    function test_aRecipientThatCannotReceiveBlocksNobody() public {
        ToggleRecipient stuck = new ToggleRecipient();
        _releaseTo(ETH, address(stuck), address(0), false);
        _releaseTo(ETH, recipient, address(0), false);
        _releaseTo(ETH, address(stuck), address(0), false);
        _releaseTo(ETH, recipient, address(0), false);
        assertEq(recipient.balance, 2 ether);
        assertEq(vault.owed(address(stuck), ETH), 2 ether);
        assertEq(vault.escrowed(ETH), 0);
        assertEq(address(vault).balance, vault.totalOwed(ETH));
    }

    function test_revertBombRecipient_isCreditedWithinAnOrdinaryGasLimit() public {
        RevertBombRecipient bomb = new RevertBombRecipient();
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, address(bomb), ROOT, keccak256("bomb"));
        _proveRelease(_releaseJournal(r));
        vm.prank(relayer);
        vault.release{gas: 1_000_000}(r, SEAL); // the 100 kB revert data is never copied
        assertEq(vault.owed(address(bomb), ETH), 1 ether);
    }

    function test_gasBurningRecipient_isCreditedGivenEnoughGas() public {
        GasBurnerRecipient burner = new GasBurnerRecipient();
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, address(burner), ROOT, keccak256("burner"));
        _proveRelease(_releaseJournal(r));
        // The burner consumes the 63/64 it is forwarded; the 1/64 the vault keeps must pay for the credit.
        vm.prank(relayer);
        vault.release{gas: 8_000_000}(r, SEAL);
        assertEq(vault.owed(address(burner), ETH), 1 ether);
        assertTrue(vault.nullifierUsed(r.nullifier));
    }

    function test_claimOwed_onlyByThePayeeAndOnlyWhatIsOwed() public {
        ToggleRecipient payee = new ToggleRecipient();
        _releaseTo(ETH, address(payee), address(0), false);

        vm.prank(alice);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(ETH); // alice is owed nothing, and cannot take the payee's credit

        payee.setOpen(true);
        payee.claim(address(vault), ETH);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        payee.claim(address(vault), ETH); // nothing left
    }

    function test_claimOwed_cannotBeReenteredFromAPayout() public {
        ToggleRecipient payee = new ToggleRecipient();
        _releaseTo(ETH, address(payee), address(0), false); // the vault now owes `payee`
        // A recipient whose receive() calls claimOwed during the release payout: the guard blocks the re-entry.
        ReentrantRecipient attacker = new ReentrantRecipient();
        attacker.setHook(address(vault), abi.encodeCall(FerrataVault.claimOwed, (ETH)));
        _releaseTo(ETH, address(attacker), address(0), false);
        assertTrue(attacker.hookCalled());
        assertEq(
            attacker.hookReturnData(),
            abi.encodeWithSelector(ReentrancyGuardTransient.ReentrancyGuardReentrantCall.selector)
        );
        assertEq(vault.owed(address(payee), ETH), 1 ether);
    }

    function test_refundToADepositorThatCannotReceive_isCredited() public {
        ToggleRecipient depositorContract = new ToggleRecipient();
        uint64 leafIndex = vault.nextLeafIndex();
        vm.deal(address(this), 1 ether);
        depositorContract.depositETH{value: 1 ether}(address(vault), 1, keccak256("contract deposit"), 0);

        vm.prank(freezer);
        vault.retire();
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, keccak256("sunset root"), leafIndex, true);
        _submit(c);
        _finalizeAfterDelta(c);

        bytes32[] memory leaves = new bytes32[](leafIndex + 1);
        for (uint64 i = 0; i < leafIndex; i++) {
            bool isEth = i % 2 == 0;
            leaves[i] = ReferenceMerkle.leafHash(
                i,
                alice,
                isEth ? ETH : address(busd),
                isEth ? 1 ether : 1000e6,
                keccak256(abi.encode(isEth ? "eth" : "busd", uint256(i / 2)))
            );
        }
        leaves[leafIndex] =
            ReferenceMerkle.leafHash(leafIndex, address(depositorContract), ETH, 1 ether, keccak256("contract deposit"));
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, leafIndex);
        bytes32[32] memory path;
        for (uint256 h = 0; h < 32; h++) {
            path[h] = siblings[h];
        }
        vault.refund(leafIndex, address(depositorContract), ETH, 1, keccak256("contract deposit"), path);
        assertEq(vault.owed(address(depositorContract), ETH), 1 ether);

        depositorContract.setOpen(true);
        depositorContract.claim(address(vault), ETH);
        assertEq(address(depositorContract).balance, 1 ether);
    }
}
