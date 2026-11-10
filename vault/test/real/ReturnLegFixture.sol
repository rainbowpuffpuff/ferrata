// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {TestToken} from "../../script/TestToken.sol";
import {FerrataVault} from "../../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";

/// @notice The end-to-end return-leg fixture (repository `e2e/`, copied to test/real/fixtures/return-leg.json) and
///         the deployment its proofs bind: RISC Zero's unmodified Groth16 verifier, RISC Zero's unmodified
///         emergency stop around it, a test token and the v3 vault, created by the fixture's deployer at its first
///         four nonces on the fixture's chain id. Nothing here is mocked: every proof reaches the real verifier.
///         The vault takes the design's default timers (docs/design/bridge.md §13); the journals bind none of them.
abstract contract ReturnLegFixture is Test {
    struct Proof {
        bytes journal;
        bytes seal;
    }

    struct ReleaseCase {
        string name;
        address token;
        uint256 amount;
        address recipient;
        address relayer;
        uint256 relayFee;
        bytes32 burnRoot;
        bytes32 nullifier;
        bytes journal;
        bytes seal;
    }

    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;
    uint256 internal constant DELTA = 3 hours + 1 minutes; // deltaFloor's default: Δ_g + S + ε
    uint256 internal constant REVOKE_WINDOW = 7 days;
    uint256 internal constant MAX_FREEZE = 30 days;
    uint256 internal constant RELEASE_DELAY = 24 hours; // minReleaseDelay outside testnets
    uint256 internal constant SLOT_SECONDS = 1; // Logos Blockchain's slot on the repository's testnet values

    // The fixture.
    uint256 internal chainId;
    address internal deployer;
    address internal depositor;
    bytes32 internal checkpointImageId;
    bytes32 internal releaseImageId;
    bytes32 internal lezConfigId;
    bytes32 internal genesisState;
    uint256 internal postingTimeframe;
    bytes32 internal depositRoot;
    bytes32[] internal commitments;
    Proof internal candidate;
    Proof internal finalization;
    uint256 internal beaconBlockNumber;
    bytes32 internal beaconPrevrandao;
    bytes32 internal beaconNonce;
    ReleaseCase[] internal releases;
    string internal fixtureJson;

    // The deployment.
    RiscZeroGroth16Verifier internal groth16;
    RiscZeroVerifierEmergencyStop internal estop;
    TestToken internal token;
    FerrataVault internal vault;
    address internal freezer = makeAddr("freeze authority (guardian of the emergency stop)");
    address internal prover = makeAddr("checkpoint prover");
    address internal submitter = makeAddr("release submitter");

    function _loadFixture() internal {
        string memory json = vm.readFile("test/real/fixtures/return-leg.json");
        fixtureJson = json;
        chainId = vm.parseJsonUint(json, ".chainId");
        deployer = vm.parseJsonAddress(json, ".deployer");
        depositor = vm.parseJsonAddress(json, ".depositor");
        checkpointImageId = vm.parseJsonBytes32(json, ".checkpointImageId");
        releaseImageId = vm.parseJsonBytes32(json, ".releaseImageId");
        lezConfigId = vm.parseJsonBytes32(json, ".lezConfigId");
        genesisState = vm.parseJsonBytes32(json, ".genesisState");
        postingTimeframe = vm.parseJsonUint(json, ".postingTimeframe");
        depositRoot = vm.parseJsonBytes32(json, ".depositRoot");
        uint256 depositCount = vm.parseJsonUint(json, ".depositCount");
        for (uint256 i = 0; i < depositCount; i++) {
            commitments.push(vm.parseJsonBytes32(json, string.concat(".deposits[", vm.toString(i), "].commitment")));
        }
        candidate = Proof(vm.parseJsonBytes(json, ".candidate.journal"), _seal(json, ".candidate.seal"));
        finalization = Proof(vm.parseJsonBytes(json, ".finalize.journal"), _seal(json, ".finalize.seal"));
        beaconBlockNumber = vm.parseJsonUint(json, ".beacon.blockNumber");
        beaconPrevrandao = vm.parseJsonBytes32(json, ".beacon.prevrandao");
        beaconNonce = vm.parseJsonBytes32(json, ".beacon.nonce");
        uint256 count = vm.parseJsonUint(json, ".releaseCount");
        for (uint256 i = 0; i < count; i++) {
            releases.push(_loadRelease(json, string.concat(".releases[", vm.toString(i), "]")));
        }
    }

    function _loadRelease(string memory json, string memory at) internal view returns (ReleaseCase memory r) {
        r.name = vm.parseJsonString(json, string.concat(at, ".name"));
        r.token = vm.parseJsonAddress(json, string.concat(at, ".token"));
        r.amount = uint256(vm.parseJsonBytes32(json, string.concat(at, ".amount")));
        r.recipient = vm.parseJsonAddress(json, string.concat(at, ".recipient"));
        r.relayer = vm.parseJsonAddress(json, string.concat(at, ".relayer"));
        r.relayFee = uint256(vm.parseJsonBytes32(json, string.concat(at, ".relayFee")));
        r.burnRoot = vm.parseJsonBytes32(json, string.concat(at, ".burnRoot"));
        r.nullifier = vm.parseJsonBytes32(json, string.concat(at, ".nullifier"));
        r.journal = vm.parseJsonBytes(json, string.concat(at, ".journal"));
        r.seal = _seal(json, string.concat(at, ".seal"));
    }

    /// @dev A fixture written before its proofs exist has journals only; the tests that need a seal then fail in
    ///      the verifier, the journal tests still run.
    function _seal(string memory json, string memory key) internal view returns (bytes memory) {
        return vm.keyExistsJson(json, key) ? vm.parseJsonBytes(json, key) : bytes("");
    }

    function _three(uint256 a, uint256 b, uint256 c) internal pure returns (uint256[] memory d) {
        d = new uint256[](3);
        (d[0], d[1], d[2]) = (a, b, c);
    }

    function _unlimited() internal pure returns (FerrataVault.CapParams memory) {
        return FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0});
    }

    function _tokenParams(uint256[] memory denoms, uint256[] memory fees, uint256 lezUnit, uint256 unitWeight)
        internal
        pure
        returns (FerrataVault.TokenParams memory)
    {
        return FerrataVault.TokenParams({
            denominations: denoms,
            relayFees: fees,
            lezUnit: lezUnit,
            unitWeight: unitWeight,
            depositCap: _unlimited(),
            releaseCap: _unlimited(),
            releaseDelay: RELEASE_DELAY
        });
    }

    function _vaultConfig() internal view returns (FerrataVault.Deployment memory) {
        return FerrataVault.Deployment({
            verifier: IRiscZeroVerifier(address(estop)),
            checkpointImageId: checkpointImageId,
            releaseImageId: releaseImageId,
            lezConfigId: lezConfigId,
            genesisState: genesisState,
            nMin: uint32(vm.parseJsonUint(fixtureJson, ".nMin")),
            tMin: uint32(vm.parseJsonUint(fixtureJson, ".tMin")),
            deltaFloor: DELTA,
            initialDelta: DELTA,
            revokeWindow: REVOKE_WINDOW,
            maxFreeze: MAX_FREEZE,
            turnSeconds: postingTimeframe * SLOT_SECONDS,
            minReleaseDelay: RELEASE_DELAY,
            admin: deployer,
            freezeAuthority: freezer,
            globalDepositCap: _unlimited(),
            globalReleaseCap: _unlimited()
        });
    }

    // The four creations, in the order the proofs' addresses assume, then the two registrations. One function per
    // transaction, so that the gas test can measure each.

    function _deployVerifier() internal {
        vm.chainId(chainId);
        vm.prank(deployer);
        groth16 = new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
    }

    function _deployStop() internal {
        vm.prank(deployer);
        estop = new RiscZeroVerifierEmergencyStop(groth16, freezer);
    }

    function _deployToken() internal {
        vm.prank(deployer);
        token = new TestToken();
    }

    function _deployVault() internal {
        FerrataVault.Deployment memory config = _vaultConfig();
        vm.prank(deployer);
        vault = new FerrataVault(config);
    }

    function _registerEth() internal {
        FerrataVault.TokenParams memory p =
            _tokenParams(_three(0.001 ether, 0.01 ether, 0.1 ether), _three(2e12, 2e13, 2e14), 1e12, 3000);
        vm.prank(deployer);
        vault.registerToken(ETH, p);
    }

    function _registerToken() internal {
        FerrataVault.TokenParams memory p =
            _tokenParams(_three(100e6, 1000e6, 10_000e6), _three(250_000, 2_500_000, 25_000_000), 1, 1);
        vm.prank(deployer);
        vault.registerToken(address(token), p);
    }

    function _deploy() internal {
        _deployVerifier();
        _deployStop();
        _deployToken();
        _deployVault();
        _registerEth();
        _registerToken();
        _assertAddresses();
    }

    function _assertAddresses() internal view {
        assertEq(address(groth16), vm.parseJsonAddress(fixtureJson, ".groth16Verifier"), "verifier address");
        assertEq(address(estop), vm.parseJsonAddress(fixtureJson, ".emergencyStop"), "emergency stop address");
        assertEq(address(token), vm.parseJsonAddress(fixtureJson, ".token"), "token address");
        assertEq(
            address(vault), vm.parseJsonAddress(fixtureJson, ".vault"), "the vault is not where the proofs bind it"
        );
    }

    // The depositor's two deposits: one ETH denomination, then one token denomination.

    function _depositEth() internal {
        vm.deal(depositor, 1 ether);
        vm.prank(depositor);
        vault.depositETH{value: 0.001 ether}(0, commitments[0], 0);
    }

    function _fundToken() internal {
        token.mint(depositor, 100e6);
        vm.prank(depositor);
        token.approve(address(vault), 100e6);
    }

    function _depositToken() internal {
        vm.prank(depositor);
        vault.deposit(address(token), 0, commitments[1], 1);
    }

    function _deposit() internal {
        _depositEth();
        _fundToken();
        _depositToken();
    }

    /// @dev The journal's last thirteen words are the vault's `Checkpoint` argument, in order.
    function _checkpoint(bytes memory journal) internal pure returns (FerrataVault.Checkpoint memory c) {
        (,,,, c) = abi.decode(journal, (bytes32, uint256, address, bytes32, FerrataVault.Checkpoint));
    }

    function _wait() internal view returns (uint256) {
        return DELTA + (uint256(_checkpoint(candidate.journal).t) - 1) * postingTimeframe * SLOT_SECONDS;
    }

    function _submit() internal returns (uint256 submittedAt) {
        vm.prank(prover);
        vault.submitCandidate(_checkpoint(candidate.journal), candidate.seal);
        return block.timestamp;
    }

    /// @dev Moves to the Ethereum block the finalisation proof echoes (number and prevrandao from the fixture), at
    ///      `timestamp`, and records its beacon.
    function _recordBeaconAt(uint256 timestamp) internal returns (uint256 index) {
        vm.warp(timestamp);
        vm.roll(beaconBlockNumber);
        vm.prevrandao(beaconPrevrandao);
        return vault.recordBeacon();
    }

    function _finalize(uint256 beaconIndex) internal {
        vm.prank(prover);
        vault.finalize(beaconIndex, _checkpoint(finalization.journal), finalization.seal);
    }

    /// @dev Candidate, the full wait, the beacon, finalisation, then the release delay.
    function _acceptRoot() internal {
        uint256 submittedAt = _submit();
        _finalize(_recordBeaconAt(submittedAt + _wait()));
        vm.warp(block.timestamp + RELEASE_DELAY);
    }

    function _request(ReleaseCase memory r) internal pure returns (FerrataVault.ReleaseRequest memory) {
        return FerrataVault.ReleaseRequest({
            token: r.token,
            denomIndex: 0,
            recipient: r.recipient,
            relayer: r.relayer,
            relayFee: r.relayFee,
            burnRoot: r.burnRoot,
            nullifier: r.nullifier
        });
    }

    function _flip(bytes memory data, uint256 at) internal pure returns (bytes memory out) {
        out = bytes.concat(data);
        out[at] = bytes1(uint8(out[at]) ^ 0x01);
    }
}
