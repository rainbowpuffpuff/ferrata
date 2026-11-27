// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

// RFP-022 F.7 for the deployment's 6-decimal TestToken, on the same four-sequencer LEZ v0.4.0-rc3 return leg as
// ZoneBridgeRc3Multi.t.sol. The fixture does not exist until checkpoint/attestor/zone-bridge.sh is run with
// ZB_TOKEN=test-token: the suite then skips cleanly instead of pretending that a synthetic burn is a real-proof run.

import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";

import {TestToken} from "../../script/TestToken.sol";
import {FerrataVault} from "../../src/FerrataVault.sol";
import {RuleBMultiFixture} from "./RuleBMulti.t.sol";

contract ZoneBridgeRc3Erc20Test is RuleBMultiFixture {
    bytes32 internal constant CHECKPOINT_B_0_3_5_IMAGE_ID =
        0x8917f58e76aa805dedc04fc5e2b70404f5078b66b271ec44aaa0c78d9f5eaf04;
    bytes32 internal constant RELEASE_IMAGE_ID = 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd;
    bytes4 internal constant SELECTOR_31 = 0xe71e6620;
    uint256 internal constant DENOMINATION = 100e6;
    uint256 internal constant RELAY_FEE = 250_000;
    uint256 internal constant CHECKPOINT_FEE = 100_000;
    uint256 internal constant DEPOSIT_TOTAL = DENOMINATION + RELAY_FEE;

    TestToken internal token;
    address internal depositor;

    function _path() internal pure override returns (string memory) {
        return "test/real/fixtures/zone-bridge-rc3-erc20.json";
    }

    function _releaseImageId() internal pure override returns (bytes32) {
        return RELEASE_IMAGE_ID;
    }

    function setUp() public {
        if (!vm.exists(_path())) {
            vm.skip(true, "zone-bridge-rc3-erc20.json absent: run ZB_TOKEN=test-token zone-bridge stages, then fixture");
            return;
        }
        _load();
        require(imageId == CHECKPOINT_B_0_3_5_IMAGE_ID, "not the rc3 multi-key checkpoint image");
        _deploy(FerrataVault.Mode.B, imageId, _state(keys), 4, 3);

        string memory json = vm.readFile(_path());
        address tokenAt = vm.parseJsonAddress(json, ".token");
        vm.etch(tokenAt, type(TestToken).creationCode);
        (bool ok, bytes memory runtime) = tokenAt.call("");
        require(ok, "TestToken constructor reverted");
        vm.etch(tokenAt, runtime);
        token = TestToken(tokenAt);
        _register(token);
        _twoDeposits(token);
    }

    function _params() internal pure returns (FerrataVault.TokenParams memory p) {
        p.denominations = new uint256[](3);
        (p.denominations[0], p.denominations[1], p.denominations[2]) = (100e6, 1000e6, 10_000e6);
        p.relayFees = new uint256[](3);
        (p.relayFees[0], p.relayFees[1], p.relayFees[2]) = (250_000, 2_500_000, 25_000_000);
        p.checkpointFees = new uint256[](3);
        (p.checkpointFees[0], p.checkpointFees[1], p.checkpointFees[2]) = (100_000, 1_000_000, 10_000_000);
        p.lezUnit = 1;
        p.unitWeight = 1;
        p.lezDefinition = bytes32(0);
        p.decimals = 6;
        p.releaseDelay = 120;
        p.depositCap = FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
        p.releaseCap = FerrataVault.CapParams({capacity: type(uint256).max, refillPerSecond: 0});
    }

    /// @dev The vault publication rule reconstructed from literals, independently of `_params`, exactly as
    ///      crown-driver's `test_token_entry_and_schedule_zero_match_the_vault_publication_rule` does.
    function _publicationEntryHash(address t) internal pure returns (bytes32) {
        return sha256(
            abi.encodePacked(
                "ferrata/registry/v1",
                t,
                bytes32(0), // lezDefinition
                uint8(6), // decimals
                uint256(1), // lezUnit
                uint256(1), // unitWeight
                uint8(3), // denomination count
                uint256(100e6),
                uint256(1000e6),
                uint256(10_000e6),
                uint256(250_000),
                uint256(2_500_000),
                uint256(25_000_000)
            )
        );
    }

    function _register(TestToken t) internal {
        bytes32 entryHash = _publicationEntryHash(address(t));
        if (t == token) {
            string memory session = vm.readFile("../e2e/runs/zone-bridge-rc3-erc20/lez/session-a/session-a.json");
            assertEq(entryHash, vm.parseJsonBytes32(session, ".token.entryHash"), "not the entry imported by the run");
        }
        // Bind the registry-only metadata as well as the release fields: neither decimals nor the entry hash is in a
        // release journal, so the releases can pass even when this replay publishes a different registry entry.
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.RegistryEntryPublished(address(t), bytes32(0), 6, entryHash);
        vm.prank(admin);
        vault.registerToken(address(t), _params());
        assertEq(
            vm.load(address(vault), sha256(abi.encodePacked("ferrata/vault/registry/v1", address(t)))),
            entryHash,
            "the vault stored a different registry entry"
        );
    }

    function _twoDeposits(TestToken t) internal {
        string memory json = vm.readFile(_path());
        depositor = vm.parseJsonAddress(json, ".depositor");
        for (uint64 j = 0; j < 2; j++) {
            bytes32 commitment = vm.parseJsonBytes32(json, string.concat(".commitments[", vm.toString(j), "]"));
            token.mint(depositor, DEPOSIT_TOTAL);
            vm.startPrank(depositor);
            token.approve(address(vault), DEPOSIT_TOTAL);
            vault.deposit(address(token), 0, commitment, j);
            vm.stopPrank();
        }
        assertEq(vault.nextLeafIndex(), 2);
        assertEq(vault.escrowed(address(token)), 2 * DEPOSIT_TOTAL);
        // The helper is only used with the fixture token; make accidental reuse with another token conspicuous.
        assertEq(address(t), address(token));
    }

    function _final() internal returns (FerrataVault.Checkpoint memory c) {
        c = _checkpoint();
        require(bytes4(seal) == SELECTOR_31, "checkpoint seal is not RISC Zero 3.1");
        vault.submitCandidate(c, seal); // this contract is the checkpoint prover/fee payee
        vm.warp(block.timestamp + WAIT);
        vault.finalizeAttested(c);
        vm.warp(block.timestamp + 120);
    }

    function _releaseAt(uint256 i) internal view returns (FerrataVault.ReleaseRequest memory r, bytes memory s) {
        string memory json = vm.readFile(_path());
        string memory p = string.concat(".releases[", vm.toString(i), "]");
        r = FerrataVault.ReleaseRequest({
            token: vm.parseJsonAddress(json, string.concat(p, ".token")),
            denomIndex: uint8(vm.parseJsonUint(json, string.concat(p, ".denomIndex"))),
            recipient: vm.parseJsonAddress(json, string.concat(p, ".recipient")),
            relayer: vm.parseJsonAddress(json, string.concat(p, ".relayer")),
            relayFee: vm.parseJsonUint(json, string.concat(p, ".relayFee")),
            burnRoot: vm.parseJsonBytes32(json, string.concat(p, ".burnRoot")),
            nullifier: vm.parseJsonBytes32(json, string.concat(p, ".nullifier"))
        });
        s = vm.parseJsonBytes(json, string.concat(p, ".seal"));
    }

    function _amountAndFees(FerrataVault.ReleaseRequest memory r)
        internal
        view
        returns (uint256 denomination, uint256 burnFee, uint256 checkpointFee, uint256 recipientNet)
    {
        (,,,, uint256[] memory denominations) = vault.tokenConfig(r.token);
        (, uint256 registeredRelay, uint256 burn, uint256 checkpoint) = vault.fees(r.token, 0, r.denomIndex);
        assertTrue(r.relayFee == 0 || r.relayFee == registeredRelay);
        denomination = denominations[r.denomIndex];
        burnFee = burn;
        checkpointFee = checkpoint;
        recipientNet = denomination - r.relayFee - burn - checkpoint;
    }

    function test_rc3Erc20_checkpointThenBothRealReleasesPayExactAmounts() public {
        FerrataVault.Checkpoint memory c = _final();
        assertEq(c.burnCount, 2);
        assertEq(c.importedLeafCount, 2);
        (bool accepted,) = vault.burnRootStatus(c.burnRoot);
        assertTrue(accepted);
        string memory json = vm.readFile(_path());
        assertEq(vm.parseJsonUint(json, ".releaseCount"), 2);

        for (uint256 i = 0; i < 2; i++) {
            _releaseOne(i, c.burnRoot);
        }
        assertEq(vault.escrowed(address(token)), 2 * RELAY_FEE, "only the two deposits' relay escrows remain");
    }

    function _releaseOne(uint256 i, bytes32 burnRoot) internal {
        (FerrataVault.ReleaseRequest memory r, bytes memory s) = _releaseAt(i);
        assertEq(r.token, address(token));
        assertEq(r.denomIndex, 0);
        assertEq(r.burnRoot, burnRoot);
        // Casting is safe: the first four bytes of a RISC Zero Groth16 seal are its verifier selector.
        // forge-lint: disable-next-line(unsafe-typecast)
        assertEq(bytes4(s), SELECTOR_31);
        (uint256 denomination,, uint256 checkpointFee, uint256 recipientNet) = _amountAndFees(r);
        assertEq(denomination, DENOMINATION);
        assertEq(checkpointFee, CHECKPOINT_FEE);

        uint256 recipientBefore = token.balanceOf(r.recipient);
        address feePayee = r.relayer == address(0) ? address(this) : r.relayer;
        uint256 relayerBefore = token.balanceOf(feePayee);
        uint256 escrowBefore = vault.escrowed(address(token));
        uint256 proverFeesBefore = vault.owed(address(this), address(token));
        vault.release(r, s);

        // If recipient == feePayee this is one balance delta: add the fee exactly once, not a second reading.
        if (feePayee == r.recipient) {
            assertEq(token.balanceOf(r.recipient) - recipientBefore, recipientNet + r.relayFee);
        } else {
            assertEq(token.balanceOf(r.recipient) - recipientBefore, recipientNet);
            assertEq(token.balanceOf(feePayee) - relayerBefore, r.relayFee);
        }
        assertEq(vault.owed(address(this), address(token)) - proverFeesBefore, CHECKPOINT_FEE);
        assertEq(escrowBefore - vault.escrowed(address(token)), DENOMINATION);
        uint256 observed = feePayee == r.recipient ? recipientNet + r.relayFee : recipientNet;
        assertEq(observed, _paidOnAnvil(i), "different payment from the run's anvil");
        assertTrue(vault.nullifierUsed(r.nullifier));
        vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
        vault.release(r, s);
    }

    function _paidOnAnvil(uint256 i) internal view returns (uint256) {
        string memory json = vm.readFile(_path());
        return vm.parseJsonUint(json, string.concat(".releases[", vm.toString(i), "].paidOnAnvil"));
    }

    function test_rc3Erc20_anotherRegisteredTokenOrAmountIsRefusedByTheRealVerifier() public {
        _final();
        TestToken other = new TestToken();
        _register(other);
        other.mint(depositor, DEPOSIT_TOTAL);
        vm.startPrank(depositor);
        other.approve(address(vault), DEPOSIT_TOTAL);
        vault.deposit(address(other), 0, keccak256("other token escrow"), 2);
        vm.stopPrank();

        (FerrataVault.ReleaseRequest memory wrongToken, bytes memory s) = _releaseAt(0);
        wrongToken.token = address(other);
        vm.expectRevert(VerificationFailed.selector);
        vault.release(wrongToken, s);

        // denomIndex is absent from the proof journal by design: the vault selects its amount and includes that amount
        // in the journal digest. Give the vault denomination-1 escrow as well, so the altered amount reaches the real
        // verifier rather than stopping earlier at InsufficientEscrow.
        token.mint(depositor, 1000e6 + 2_500_000);
        vm.startPrank(depositor);
        token.approve(address(vault), 1000e6 + 2_500_000);
        vault.deposit(address(token), 1, keccak256("larger denomination escrow"), 3);
        vm.stopPrank();
        // Choosing denomination 1 changes 100 TUSD to 1,000 TUSD and so the journal digest no longer matches.
        (FerrataVault.ReleaseRequest memory wrongAmount,) = _releaseAt(0);
        wrongAmount.denomIndex = 1;
        vm.expectRevert(VerificationFailed.selector);
        vault.release(wrongAmount, s);
    }
}
