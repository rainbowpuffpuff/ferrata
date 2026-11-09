// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {console, stdStorage, StdStorage} from "forge-std/Test.sol";

import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "ferrata-vault/FerrataVault.sol";
import {IRiscZeroVerifier} from "ferrata-vault/interfaces/IRiscZeroVerifier.sol";

import {Fixtures} from "./utils/Fixtures.sol";
import {RootMockingVerifier, TestToken} from "./utils/TestContracts.sol";

/// @notice Shared deployment. The proofs bind chain id 31337 and the vault and token addresses, so the
///         deployer from the fixtures deploys at nonces 0, 1 and 2, as `host` assumed when proving.
abstract contract VaultFixture is Fixtures {
    address internal constant ETH = address(0);
    /// The burn-root program is not built yet; this id only has to be non-zero and differ from the release id.
    bytes32 internal constant ROOT_IMAGE_ID = keccak256("ferrata-bridge evm-verify: burn-root program (mocked)");
    uint32 internal constant MIN_DEPTH = 64;
    uint256 internal constant TUSD_RELAY_FEE = 250_000;

    Release internal eth;
    Release internal erc20;
    TestToken internal token;
    FerrataVault internal vault;
    address internal freezer = makeAddr("freezeAuthority");
    address internal alice = makeAddr("alice (depositor)");
    address internal submitter = makeAddr("submitter (relayer EOA)");

    function _loadFixtures() internal {
        eth = _load("eth");
        erc20 = _load("erc20");
        vm.chainId(eth.chainId);
    }

    /// Deploys token (nonce 1) and vault (nonce 2) from the fixture deployer, whose nonce-0 contract the
    /// caller has just deployed, then registers ETH and TUSD and makes deposits that fund the releases.
    function _deployTokenAndVault(IRiscZeroVerifier verifier) internal {
        address deployer = eth.deployer;
        vm.startPrank(deployer);
        token = new TestToken();
        FerrataVault.CapParams memory unlimited =
            FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
        vault = new FerrataVault(
            verifier,
            eth.releaseImageId,
            ROOT_IMAGE_ID,
            eth.lezConfigId,
            deployer,
            freezer,
            MIN_DEPTH,
            unlimited,
            unlimited
        );
        vault.registerToken(ETH, _denoms(0.1 ether, 1 ether, 10 ether), 1e12, 3000, unlimited, unlimited);
        vault.registerToken(address(token), _denoms(100e6, 1000e6, 10_000e6), 1, 1, unlimited, unlimited);
        vault.setStandardRelayFee(address(token), 0, TUSD_RELAY_FEE);
        // Same standard fee for denomination 1, so that a request changing only the denomination passes
        // the vault's fee check and is rejected by the proof.
        vault.setStandardRelayFee(address(token), 1, TUSD_RELAY_FEE);
        vm.stopPrank();

        assertEq(address(token), eth.testToken, "token address differs from the one bound in the proof");
        assertEq(address(token), erc20.token);
        assertEq(address(vault), eth.vault, "vault address differs from the one bound in the proof");
        assertEq(address(vault), erc20.vault);

        _depositETH(0, keccak256("someone's commitment 1"));
        _depositETH(1, keccak256("someone's commitment 2")); // funds the denomination-change negative
        _depositToken(0, keccak256("someone's commitment 3"));
        _depositToken(1, keccak256("someone's commitment 4"));
    }

    function _denoms(uint256 a, uint256 b, uint256 c) internal pure returns (uint256[] memory d) {
        d = new uint256[](3);
        (d[0], d[1], d[2]) = (a, b, c);
    }

    function _depositETH(uint8 denomIndex, bytes32 commitment) internal {
        uint256 amount = vault.denominations(ETH)[denomIndex];
        uint64 index = vault.depositCount(alice); // read first: vm.prank applies to the next call only
        vm.deal(alice, amount);
        vm.prank(alice);
        vault.depositETH{value: amount}(denomIndex, commitment, index);
    }

    function _depositToken(uint8 denomIndex, bytes32 commitment) internal {
        uint256 amount = vault.denominations(address(token))[denomIndex];
        token.mint(alice, amount);
        vm.startPrank(alice);
        token.approve(address(vault), amount);
        vault.deposit(address(token), denomIndex, commitment, vault.depositCount(alice));
        vm.stopPrank();
    }

    function _request(Release memory f) internal pure returns (FerrataVault.ReleaseRequest memory) {
        return FerrataVault.ReleaseRequest({
            token: f.token,
            denomIndex: 0,
            recipient: f.recipient,
            feeRecipient: f.feeRecipient,
            relayFee: f.relayFee,
            burnRoot: f.burnRoot,
            nullifier: f.nullifier
        });
    }

    function _balance(address tok, address who) internal view returns (uint256) {
        return tok == ETH ? who.balance : token.balanceOf(who);
    }

    /// Releases `f` with its real seal from `submitter` and checks every balance and the nullifier. With a
    /// non-empty `snapshotName` it also records the release call's gas (forge writes snapshots/).
    function _releaseAndCheck(Release memory f, string memory snapshotName)
        internal
        returns (uint256 frame, uint256 delta)
    {
        FerrataVault.ReleaseRequest memory r = _request(f);
        address feeTo = f.feeRecipient == address(0) ? submitter : f.feeRecipient;
        uint256 recipientBefore = _balance(f.token, f.recipient);
        uint256 feeBefore = _balance(f.token, feeTo);
        uint256 escrowBefore = vault.escrowed(f.token);
        assertFalse(vault.nullifierUsed(f.nullifier));

        vm.prank(submitter);
        uint256 before = gasleft();
        vault.release(r, f.seal);
        delta = before - gasleft();
        if (bytes(snapshotName).length != 0) frame = vm.snapshotGasLastCall(snapshotName);

        assertTrue(vault.nullifierUsed(f.nullifier));
        assertEq(_balance(f.token, f.recipient) - recipientBefore, f.amount - f.relayFee, "recipient amount");
        assertEq(_balance(f.token, feeTo) - feeBefore, f.relayFee, "relay fee");
        assertEq(escrowBefore - vault.escrowed(f.token), f.amount, "escrow");

        // Replaying the same proof fails on the nullifier.
        vm.prank(submitter);
        vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
        vault.release(r, f.seal);
    }

    /// Gas of one release (call it as the first vault call of a test, so storage is cold as in a fresh
    /// transaction: setUp runs as a separate transaction).
    function _releaseAndLogGas(Release memory f, string memory name) internal {
        (uint256 frame, uint256 delta) = _releaseAndCheck(f, name);
        bytes memory data = abi.encodeCall(vault.release, (_request(f), f.seal));
        console.log(name);
        console.log("  snapshotGasLastCall (vault.release frame, incl. Groth16 verify):", frame);
        console.log("  gasleft() delta (frame + CALL from the test contract)          :", delta);
        console.log(
            "  calldata bytes / EIP-2028 gas                                  :", data.length, _calldataGas(data)
        );
        console.log(
            "  as a transaction, estimate 21000 + calldata + frame            :", 21_000 + _calldataGas(data) + frame
        );
    }
}

/// @notice The vault wired directly to the real RiscZeroGroth16Verifier (as deployed in production). The
///         burn root is marked accepted by writing `burnRootAcceptedAt[root]` directly: a TEST CHEAT
///         standing in for the LEZ burn-root proof, which does not exist yet. Everything after that is
///         real: the release image id, the Groth16 seal, the journal the vault rebuilds, the payout.
contract VaultReleaseTest is VaultFixture {
    using stdStorage for StdStorage;

    RiscZeroGroth16Verifier internal groth16;

    function setUp() public {
        _loadFixtures();
        vm.prank(eth.deployer);
        groth16 = new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        assertEq(address(groth16), eth.verifier);
        _deployTokenAndVault(IRiscZeroVerifier(address(groth16)));
        _markRootAccepted(eth.burnRoot);
        assertEq(erc20.burnRoot, eth.burnRoot); // both burns are in the same LEZ burn tree
    }

    function _markRootAccepted(bytes32 root) internal {
        stdstore.target(address(vault)).sig("burnRootAcceptedAt(bytes32)").with_key(root).checked_write(block.timestamp);
        assertEq(vault.burnRootAcceptedAt(root), block.timestamp);
    }

    function test_release_eth_withRealProof() public {
        _releaseAndLogGas(eth, "release-eth");
    }

    function test_release_erc20_withRealProof() public {
        _releaseAndLogGas(erc20, "release-erc20");
    }

    // ---- negative: each must revert in the verifier, and leave the nullifier unused -----------------------

    function _expectVerifierRejects(FerrataVault.ReleaseRequest memory r, bytes memory seal) internal {
        vm.prank(submitter);
        vm.expectRevert(VerificationFailed.selector);
        vault.release(r, seal);
        assertFalse(vault.nullifierUsed(r.nullifier));
    }

    function test_reject_sealForADifferentJournal() public {
        _expectVerifierRejects(_request(eth), erc20.seal);
        _expectVerifierRejects(_request(erc20), eth.seal);
    }

    function test_reject_tamperedSeal() public {
        bytes memory seal = bytes.concat(eth.seal);
        seal[4 + 32 * 7 + 31] ^= 0x01; // last bit of c.y
        _expectVerifierRejects(_request(eth), seal);
    }

    function test_reject_rightSeal_oneJournalFieldChanged() public {
        FerrataVault.ReleaseRequest memory r;

        r = _request(erc20);
        r.recipient = makeAddr("thief");
        _expectVerifierRejects(r, erc20.seal);

        r = _request(erc20);
        r.feeRecipient = makeAddr("another relayer");
        _expectVerifierRejects(r, erc20.seal);

        r = _request(erc20);
        r.relayFee = 0; // the other value the vault accepts
        _expectVerifierRejects(r, erc20.seal);

        r = _request(erc20);
        r.nullifier = keccak256("a fresh nullifier");
        _expectVerifierRejects(r, erc20.seal);

        r = _request(erc20);
        r.denomIndex = 1; // 1000 TUSD instead of 100: escrow covers it, the proof does not
        _expectVerifierRejects(r, erc20.seal);

        r = _request(eth);
        r.token = address(token); // same denomination index, other token
        _expectVerifierRejects(r, eth.seal);

        bytes32 otherRoot = keccak256("another accepted burn root");
        _markRootAccepted(otherRoot);
        r = _request(eth);
        r.burnRoot = otherRoot;
        _expectVerifierRejects(r, eth.seal);

        // Another chain: the vault puts block.chainid into the journal.
        vm.chainId(1);
        _expectVerifierRejects(_request(eth), eth.seal);
        vm.chainId(eth.chainId);

        // Control: the unmodified requests still release (gas not reported: storage is warm by now).
        _releaseAndCheck(eth, "");
        _releaseAndCheck(erc20, "");
    }
}

/// @notice The same releases, with the root accepted through the vault's own `acceptBurnRoot`. The vault's
///         verifier is `RootMockingVerifier` (TEST ONLY): it answers the root-proof check from a
///         registered journal digest and forwards release proofs to the real Groth16 verifier, so the
///         release gas here includes one extra CALL hop.
contract VaultAcceptRootTest is VaultFixture {
    RootMockingVerifier internal mocking;

    uint64 internal constant LEZ_BLOCK = 4242;
    uint64 internal constant BURN_COUNT = 8;

    function setUp() public {
        _loadFixtures();
        vm.prank(makeAddr("risc0 verifier deployer"));
        RiscZeroGroth16Verifier groth16 =
            new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        vm.prank(eth.deployer);
        mocking = new RootMockingVerifier(IRiscZeroVerifier(address(groth16)), ROOT_IMAGE_ID);
        _deployTokenAndVault(IRiscZeroVerifier(address(mocking)));

        // The ordinary acceptance path: a (mocked) root proof for this vault's journal.
        mocking.acceptRootJournal(_rootJournalDigest(eth.burnRoot, MIN_DEPTH));
        vault.acceptBurnRoot(eth.burnRoot, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, "");
        assertEq(vault.burnRootAcceptedAt(eth.burnRoot), block.timestamp);
    }

    function _rootJournalDigest(bytes32 root, uint32 depth) internal view returns (bytes32) {
        return sha256(
            abi.encode(
                vault.ROOT_DOMAIN(), block.chainid, address(vault), eth.lezConfigId, LEZ_BLOCK, root, BURN_COUNT, depth
            )
        );
    }

    function test_acceptBurnRoot_refusesARootWithoutProof() public {
        bytes32 other = keccak256("a root nobody proved");
        vm.expectRevert(RootMockingVerifier.RootJournalNotAccepted.selector);
        vault.acceptBurnRoot(other, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, "");
        FerrataVault.ReleaseRequest memory r = _request(eth);
        r.burnRoot = other;
        vm.prank(submitter);
        vm.expectRevert(FerrataVault.UnknownBurnRoot.selector);
        vault.release(r, eth.seal);
    }

    function test_release_eth_viaAcceptBurnRoot() public {
        _releaseAndLogGas(eth, "release-eth-via-wrapper");
    }

    function test_release_erc20_viaAcceptBurnRoot() public {
        _releaseAndLogGas(erc20, "release-erc20-via-wrapper");
    }
}
