// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {console} from "forge-std/Test.sol";
import {Pausable} from "openzeppelin/contracts/utils/Pausable.sol";

import {IRiscZeroVerifier as R0Verifier, VerificationFailed} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "ferrata-vault/FerrataVault.sol";
import {IRiscZeroVerifier} from "ferrata-vault/interfaces/IRiscZeroVerifier.sol";

import {FixturesV2} from "./utils/FixturesV2.sol";
import {RejectingRecipient, TestToken} from "./utils/TestContractsV2.sol";

/// @notice The v2 vault (vendor/vault-v2: spike/bridge-evm branch v2) wired as SPEC-v2 §6 says: its immutable
///         verifier is RISC Zero's RiscZeroVerifierEmergencyStop (unmodified, guardian = the freeze authority)
///         around the real RiscZeroGroth16Verifier. The deployer's nonces 0..3 give exactly the addresses the
///         release proofs bind (chain 31337).
///
///         The LEZ checkpoint program does not exist yet. TEST ONLY: its proof is mocked with vm.mockCall on
///         the stop, for exactly the call `verify(CHECKPOINT_SEAL, CHECKPOINT_IMAGE_ID, digest)` of each
///         checkpoint journal; the checkpoint itself goes through the vault's real submitCandidate,
///         recordBeacon and finalize. Every release proof reaches the real verifier.
abstract contract VaultV2Fixture is FixturesV2 {
    address internal constant ETH = address(0);
    bytes32 internal constant CHECKPOINT_IMAGE_ID =
        keccak256("ferrata-bridge evm-verify: LEZ checkpoint program (not built here; its proof is mocked)");
    bytes internal constant CHECKPOINT_SEAL = hex"c0ffee";
    uint256 internal constant DELTA = 1 hours;
    uint256 internal constant UNLIMITED = type(uint256).max;

    Release internal eth;
    Release internal erc20;
    Release internal ethpull;
    CheckpointFixture internal cp;
    RiscZeroGroth16Verifier internal groth16;
    RiscZeroVerifierEmergencyStop internal estop;
    TestToken internal token;
    FerrataVault internal vault;
    address internal freezer = makeAddr("freeze authority (estop guardian)");
    address internal alice = makeAddr("alice (depositor)");
    address internal submitter = makeAddr("submitter (relayer EOA)");

    function _deploy() internal {
        eth = _load("eth");
        erc20 = _load("erc20");
        ethpull = _load("ethpull");
        cp = _loadCheckpoint();
        vm.chainId(eth.chainId);
        address deployer = eth.deployer;
        FerrataVault.CapParams memory unlimited = FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0});

        vm.startPrank(deployer);
        groth16 = new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID); // nonce 0
        estop = new RiscZeroVerifierEmergencyStop(groth16, freezer); // nonce 1
        token = new TestToken(); // nonce 2
        vault = new FerrataVault( // nonce 3
            FerrataVault.Deployment({
                verifier: IRiscZeroVerifier(address(estop)),
                checkpointImageId: CHECKPOINT_IMAGE_ID,
                releaseImageId: eth.releaseImageId,
                lezConfigId: eth.lezConfigId,
                genesisState: cp.genesisState,
                nMin: cp.nMin,
                tMin: cp.tMin,
                deltaFloor: DELTA,
                initialDelta: DELTA,
                revokeWindow: 1 days,
                maxFreeze: 30 days,
                admin: deployer,
                freezeAuthority: freezer,
                globalDepositCap: unlimited,
                globalReleaseCap: unlimited
            })
        );
        vault.registerToken(
            ETH,
            _params(_three(0.1 ether, 1 ether, 10 ether), _three(0.0005 ether, 0.005 ether, 0.05 ether), 1e12, 3000)
        );
        vault.registerToken(
            address(token), _params(_three(100e6, 1000e6, 10_000e6), _three(250_000, 2_500_000, 25_000_000), 1, 1)
        );
        vm.stopPrank();

        assertEq(address(groth16), eth.groth16Verifier);
        assertEq(address(estop), eth.emergencyStop);
        assertEq(address(token), eth.testToken);
        assertEq(address(token), erc20.token);
        assertEq(address(vault), eth.vault, "vault address differs from the one the proofs bind");
        assertEq(vault.lezConfigId(), eth.lezConfigId);
        assertEq(vm.load(address(vault), bytes32(uint256(2))), eth.lezConfigId, "slot 2 holds lezConfigId (spec 3.1)");

        _depositETH(0, keccak256("someone's commitment 1"));
        _depositETH(0, keccak256("someone's commitment 2"));
        _depositETH(1, keccak256("someone's commitment 3")); // funds the denomination-change negative
        _depositToken(0, keccak256("someone's commitment 4"));
        _depositToken(1, keccak256("someone's commitment 5"));

        // The proof binds this recipient; it is a contract that refuses ETH until told otherwise.
        vm.etch(ethpull.recipient, type(RejectingRecipient).runtimeCode);
    }

    function _three(uint256 a, uint256 b, uint256 c) internal pure returns (uint256[] memory d) {
        d = new uint256[](3);
        (d[0], d[1], d[2]) = (a, b, c);
    }

    function _params(uint256[] memory denoms, uint256[] memory fees, uint256 lezUnit, uint256 unitWeight)
        internal
        pure
        returns (FerrataVault.TokenParams memory)
    {
        FerrataVault.CapParams memory unlimited = FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0});
        return FerrataVault.TokenParams({
            denominations: denoms,
            relayFees: fees,
            lezUnit: lezUnit,
            unitWeight: unitWeight,
            depositCap: unlimited,
            releaseCap: unlimited,
            releaseDelay: 0
        });
    }

    function _depositETH(uint8 denomIndex, bytes32 commitment) internal {
        uint256 amount = vault.denominations(ETH)[denomIndex];
        uint64 index = vault.depositCount(alice);
        vm.deal(alice, amount);
        vm.prank(alice);
        vault.depositETH{value: amount}(denomIndex, commitment, index);
    }

    function _depositToken(uint8 denomIndex, bytes32 commitment) internal {
        uint256 amount = vault.denominations(address(token))[denomIndex];
        token.mint(alice, amount);
        uint64 index = vault.depositCount(alice);
        vm.startPrank(alice);
        token.approve(address(vault), amount);
        vault.deposit(address(token), denomIndex, commitment, index);
        vm.stopPrank();
    }

    // ---- checkpoints (spec 4.3), with only the checkpoint proof mocked --------------------------------------

    /// The vault's checkpoint journal digest, restated (the Rust encoding equals this: JournalEquality.t.sol).
    function _checkpointDigest(FerrataVault.Checkpoint memory c) internal view returns (bytes32) {
        bytes memory head = abi.encode(
            sha256("ferrata/vault/checkpoint/v2"), block.chainid, address(vault), vault.lezConfigId(), c.nMin, c.tMin
        );
        bytes memory body =
            abi.encode(c.parentState, c.newState, c.burnRoot, c.burnCount, c.importedLeafCount, c.sunset, c.echoNonce);
        return sha256(bytes.concat(head, body));
    }

    /// TEST ONLY: makes the stop answer `verify(CHECKPOINT_SEAL, CHECKPOINT_IMAGE_ID, digest(c))` without
    /// running it. A vault that encoded the journal differently would call with another digest, reach the
    /// real verifier, and revert.
    function _mockCheckpointProof(FerrataVault.Checkpoint memory c) internal {
        vm.mockCall(
            address(estop),
            abi.encodeCall(R0Verifier.verify, (CHECKPOINT_SEAL, CHECKPOINT_IMAGE_ID, _checkpointDigest(c))),
            ""
        );
    }

    function _candidate(bytes32 parent, bytes32 newState, bytes32 burnRoot, uint64 burnCount, uint64 imported)
        internal
        view
        returns (FerrataVault.Checkpoint memory)
    {
        return FerrataVault.Checkpoint({
            nMin: cp.nMin,
            tMin: cp.tMin,
            parentState: parent,
            newState: newState,
            burnRoot: burnRoot,
            burnCount: burnCount,
            importedLeafCount: imported,
            sunset: false,
            echoNonce: bytes32(0)
        });
    }

    /// submitCandidate, wait delta, record a beacon, finalize with the beacon's nonce echoed.
    function _finalize(bytes32 parent, bytes32 newState, bytes32 burnRoot, uint64 burnCount, uint64 imported)
        internal
    {
        FerrataVault.Checkpoint memory c = _candidate(parent, newState, burnRoot, burnCount, imported);
        _mockCheckpointProof(c);
        vault.submitCandidate(c, CHECKPOINT_SEAL);
        vm.warp(block.timestamp + DELTA);
        vm.roll(block.number + 1);
        uint256 beaconIndex = vault.recordBeacon();
        (, bytes32 nonce) = vault.beacons(beaconIndex);
        c.echoNonce = nonce;
        _mockCheckpointProof(c);
        vault.finalize(beaconIndex, c, CHECKPOINT_SEAL);
    }

    // ---- releases ----------------------------------------------------------------------------------------

    function _request(Release memory f, uint8 denomIndex) internal pure returns (FerrataVault.ReleaseRequest memory) {
        return FerrataVault.ReleaseRequest({
            token: f.token,
            denomIndex: denomIndex,
            recipient: f.recipient,
            relayer: f.relayer,
            relayFee: f.relayFee,
            burnRoot: f.burnRoot,
            nullifier: f.nullifier
        });
    }

    function _balance(address tok, address who) internal view returns (uint256) {
        return tok == ETH ? who.balance : token.balanceOf(who);
    }

    /// Releases `f` with its real seal from `submitter`; checks the payouts (or the pull-payment credit when
    /// `pull`), the escrow and the nullifier; then a replay fails. With a non-empty `snapshotName` it also
    /// logs the release's gas (call it as the first vault call of a test, so storage is cold as in a fresh
    /// transaction).
    function _release(Release memory f, bool pull, string memory snapshotName) internal {
        FerrataVault.ReleaseRequest memory r = _request(f, 0);
        address feePayee = f.relayer == address(0) ? submitter : f.relayer;
        uint256 recipientBefore = _balance(f.token, f.recipient);
        uint256 feeBefore = _balance(f.token, feePayee);
        uint256 escrowBefore = vault.escrowed(f.token);

        vm.prank(submitter);
        uint256 before = gasleft();
        vault.release(r, f.seal);
        uint256 delta = before - gasleft();
        uint256 frame = bytes(snapshotName).length != 0 ? vm.snapshotGasLastCall(snapshotName) : 0;

        assertTrue(vault.nullifierUsed(f.nullifier));
        assertEq(escrowBefore - vault.escrowed(f.token), f.amount, "escrow");
        assertEq(_balance(f.token, feePayee) - feeBefore, f.relayFee, "relay fee");
        if (pull) {
            assertEq(_balance(f.token, f.recipient), recipientBefore, "the rejecting recipient received nothing");
            assertEq(vault.owed(f.recipient, f.token), f.amount - f.relayFee, "credited to owed");
        } else {
            assertEq(_balance(f.token, f.recipient) - recipientBefore, f.amount - f.relayFee, "recipient amount");
        }
        if (bytes(snapshotName).length != 0) {
            bytes memory data = abi.encodeCall(vault.release, (r, f.seal));
            console.log(snapshotName);
            console.log("  snapshotGasLastCall (vault.release frame, incl. stop + Groth16):", frame);
            console.log("  gasleft() delta (frame + CALL from the test contract)          :", delta);
            console.log(
                "  calldata bytes / EIP-2028 gas                                  :", data.length, _calldataGas(data)
            );
            console.log(
                "  as a transaction, estimate 21000 + calldata + frame            :",
                21_000 + _calldataGas(data) + frame
            );
        }

        vm.prank(submitter);
        vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
        vault.release(r, f.seal);
    }

    function _expectVerifierRejects(FerrataVault.ReleaseRequest memory r, bytes memory seal) internal {
        vm.prank(submitter);
        vm.expectRevert(VerificationFailed.selector);
        vault.release(r, seal);
        assertFalse(vault.nullifierUsed(r.nullifier));
    }
}

contract VaultV2Test is VaultV2Fixture {
    function setUp() public {
        _deploy();
        _finalize(cp.genesisState, cp.newState, cp.burnRoot, cp.burnCount, cp.importedLeafCount);
        (bool accepted,) = vault.burnRootStatus(cp.burnRoot);
        assertTrue(accepted);
        assertEq(vault.finalState(), cp.newState);
        assertEq(eth.burnRoot, cp.burnRoot);
    }

    function test_release_eth_withRealProof() public {
        _release(eth, false, "release-eth");
    }

    function test_release_erc20_withRealProof() public {
        _release(erc20, false, "release-erc20");
    }

    /// Spec 5.4: the recipient (a contract bound by the proof) rejects ETH. The release completes, the relayer
    /// is paid, the recipient's share is credited to `owed`, and only the recipient can collect it later.
    function test_release_ethpull_fallsBackToPullPayment() public {
        _release(ethpull, true, "release-eth-pull-payment");
        uint256 owedAmount = vault.owed(ethpull.recipient, ETH);
        vm.prank(makeAddr("someone else"));
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        vault.claimOwed(ETH);
        RejectingRecipient(payable(ethpull.recipient)).setAccepting(true);
        RejectingRecipient(payable(ethpull.recipient)).collect(address(vault), ETH);
        assertEq(ethpull.recipient.balance, owedAmount);
        assertEq(vault.owed(ethpull.recipient, ETH), 0);
        vm.expectRevert(FerrataVault.NothingOwed.selector);
        RejectingRecipient(payable(ethpull.recipient)).collect(address(vault), ETH);
    }

    // ---- negative: each must revert, and leave the nullifier unused ----------------------------------------

    function test_reject_sealForADifferentJournal() public {
        _expectVerifierRejects(_request(eth, 0), erc20.seal);
        _expectVerifierRejects(_request(erc20, 0), ethpull.seal);
    }

    function test_reject_tamperedSeal() public {
        bytes memory seal = bytes.concat(eth.seal);
        seal[4 + 32 * 7 + 31] ^= 0x01; // last bit of c.y
        _expectVerifierRejects(_request(eth, 0), seal);
    }

    function test_reject_rightSeal_oneJournalFieldChanged() public {
        FerrataVault.ReleaseRequest memory r;

        r = _request(eth, 0);
        r.recipient = makeAddr("thief");
        _expectVerifierRejects(r, eth.seal);

        r = _request(erc20, 0);
        r.relayer = makeAddr("another relayer");
        _expectVerifierRejects(r, erc20.seal);

        r = _request(erc20, 0);
        r.relayFee = 0; // the other value the vault accepts
        _expectVerifierRejects(r, erc20.seal);

        r = _request(erc20, 0);
        r.nullifier = keccak256("a fresh nullifier");
        _expectVerifierRejects(r, erc20.seal);

        r = _request(eth, 1); // 1 ETH instead of 0.1: escrow covers it, the fee (0) is valid, the proof is not
        _expectVerifierRejects(r, eth.seal);

        r = _request(eth, 0);
        r.token = address(token); // the same denomination index of another token
        _expectVerifierRejects(r, eth.seal);

        // Another accepted burn root: a second checkpoint on top of the first.
        bytes32 otherRoot = keccak256("another accepted burn root");
        _finalize(cp.newState, keccak256("a later LEZ state"), otherRoot, cp.burnCount + 1, cp.importedLeafCount);
        r = _request(eth, 0);
        r.burnRoot = otherRoot;
        _expectVerifierRejects(r, eth.seal);

        // Another chain: the vault puts block.chainid into the journal.
        vm.chainId(1);
        _expectVerifierRejects(_request(eth, 0), eth.seal);
        vm.chainId(eth.chainId);

        // Control: the unmodified requests still release.
        _release(eth, false, "");
        _release(erc20, false, "");
    }

    function test_reject_unacceptedBurnRoot() public {
        FerrataVault.ReleaseRequest memory r = _request(eth, 0);
        r.burnRoot = keccak256("a root no checkpoint carried");
        vm.prank(submitter);
        vm.expectRevert(FerrataVault.BurnRootNotAccepted.selector);
        vault.release(r, eth.seal);
    }

    /// The checkpoint path is the vault's real one: a finalisation that echoes a beacon recorded before the
    /// candidate's delta has passed is refused.
    function test_checkpoint_finalizeBeforeDelta_isRefused() public {
        FerrataVault.Checkpoint memory c = _candidate(
            cp.newState, keccak256("next state"), keccak256("next root"), cp.burnCount + 1, cp.importedLeafCount
        );
        _mockCheckpointProof(c);
        vault.submitCandidate(c, CHECKPOINT_SEAL);
        vm.warp(block.timestamp + DELTA - 1);
        vm.roll(block.number + 1);
        uint256 beaconIndex = vault.recordBeacon();
        (, bytes32 nonce) = vault.beacons(beaconIndex);
        c.echoNonce = nonce;
        _mockCheckpointProof(c);
        vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
        vault.finalize(beaconIndex, c, CHECKPOINT_SEAL);
    }

    /// Spec 6: once the freeze authority (the stop's guardian) stops the verifier, no release goes through.
    function test_emergencyStop_blocksRelease() public {
        vm.prank(freezer);
        estop.estop();
        vm.prank(submitter);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.release(_request(eth, 0), eth.seal);
        assertFalse(vault.nullifierUsed(eth.nullifier));
        // Nor any checkpoint: the stop also blocks the (here mocked) checkpoint proofs' real path.
        assertTrue(estop.paused());
    }
}
