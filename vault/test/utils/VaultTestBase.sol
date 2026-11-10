// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";
import {IRiscZeroVerifier as IRiscZeroVerifierFull} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";
import {MockERC20, MockTokenBase, MockUSDT} from "../mocks/MockTokens.sol";
import {MockRiscZeroVerifier} from "../mocks/MockVerifiers.sol";
import {ReferenceMerkle} from "./ReferenceMerkle.sol";

/// @notice Shared fixture: a vault with ETH, a 6-decimal token, an 18-decimal token and a USDT-style token
///         registered, all caps UNLIMITED, verifying through RISC Zero's emergency-stop wrapper. Journals are rebuilt
///         here word by word from the written spec (README, SPEC-v2), not with the vault's own encoding functions.
abstract contract VaultTestBase is Test {
    bytes32 internal constant RELEASE_IMAGE_ID = keccak256("test: release program image id");
    bytes32 internal constant CHECKPOINT_IMAGE_ID = keccak256("test: checkpoint program image id");
    bytes32 internal constant LEZ_CONFIG_ID = keccak256("test: LEZ configuration id");
    bytes32 internal constant GENESIS_STATE = keccak256("test: LEZ genesis checkpoint state");
    uint32 internal constant N_MIN = 3;
    uint32 internal constant T_MIN = 2;
    /// Default key set of the test checkpoints: n = 4 keys, threshold t(4) = 3.
    uint32 internal constant N = 4;
    uint32 internal constant T = 3;
    uint256 internal constant DELTA_FLOOR = 3 hours + 1 minutes; // v3 D4 default floor
    uint256 internal constant DELTA = 4 hours;
    uint256 internal constant TURN_SECONDS = 900; // posting timeframe on the repository's testnet values
    uint256 internal constant REVOKE_WINDOW = 3 days;
    uint256 internal constant MAX_FREEZE = 30 days;
    uint256 internal constant MIN_RELEASE_DELAY = 0; // tests of the bound deploy their own vault
    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;
    bytes internal constant SEAL = hex"5ea15ea1";
    // The vault's internal constants (v5 made them internal for EIP-170), from the written spec.
    uint256 internal constant DEPOSIT_ROOT_SLOT_ = 0;
    uint256 internal constant NEXT_LEAF_INDEX_SLOT_ = 1;
    uint256 internal constant LEZ_CONFIG_ID_SLOT_ = 2;
    uint256 internal constant MAX_DELTA_ = 30 days;
    /// The fee ceiling and notice (D13).
    uint256 internal constant FEE_CEILING_DIVISOR = 100;
    uint256 internal constant FEE_NOTICE = 7 days;

    struct CheckpointJournal {
        uint256 chainId;
        address vault;
        bytes32 lezConfigId;
        uint32 nMin;
        uint32 tMin;
        bytes32 parentState;
        bytes32 newState;
        bytes32 burnRoot;
        uint64 burnCount;
        uint64 importedLeafCount;
        bool sunset;
        bytes32 echoNonce;
        uint64 tipId;
        bytes32 tipHash;
        uint32 n;
        uint32 t;
    }

    struct ReleaseJournal {
        uint256 chainId;
        address vault;
        bytes32 lezConfigId;
        bytes32 burnRoot;
        bytes32 nullifier;
        address token;
        uint256 amount;
        address recipient;
        address relayer;
        uint256 relayFee;
    }

    FerrataVault internal vault;
    /// The base verifier: accepts exactly the journals a test registers.
    MockRiscZeroVerifier internal verifier;
    /// The vault's verifier: RISC Zero's unmodified emergency-stop wrapper around `verifier`, guardian = freezer.
    RiscZeroVerifierEmergencyStop internal estop;
    MockERC20 internal usdc; // 6 decimals
    MockERC20 internal dai; // 18 decimals
    MockUSDT internal usdt; // 6 decimals, transfer/transferFrom return nothing

    address internal admin = makeAddr("admin");
    address internal freezer = makeAddr("freezeAuthority");
    address internal alice = makeAddr("alice");
    address internal bob = makeAddr("bob");
    address internal relayer = makeAddr("relayer");
    address internal recipient = makeAddr("recipient");
    address internal watcher = makeAddr("watcher");
    address internal feeRecipient = makeAddr("feeRecipient");

    /// Rule (B) test deployment: the wait is the reaction window only (D13 default 1 h), no turn term.
    uint256 internal constant DELTA_B = 1 hours;

    /// Counters that make every test checkpoint's `newState` and `burnCount` distinct.
    uint256 internal stateNonce;
    uint64 internal burnCounter;

    function setUp() public virtual {
        verifier = new MockRiscZeroVerifier();
        estop = new RiscZeroVerifierEmergencyStop(IRiscZeroVerifierFull(address(verifier)), freezer);
        usdc = new MockERC20("USD Coin", "USDC", 6);
        dai = new MockERC20("Dai Stablecoin", "DAI", 18);
        usdt = new MockUSDT("Tether USD", "USDT", 6);
        vault = _deployVault(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        _registerDefaultTokens(vault);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Deployment and registration
    // ------------------------------------------------------------------------------------------------------------

    function _unlimited() internal pure returns (FerrataVault.CapParams memory) {
        return FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0});
    }

    function _cap(uint256 capacity, uint256 refillPerSecond) internal pure returns (FerrataVault.CapParams memory) {
        return FerrataVault.CapParams({capacity: capacity, refillPerSecond: refillPerSecond});
    }

    function _deployment(IRiscZeroVerifier v, bytes32 lezConfig)
        internal
        view
        returns (FerrataVault.Deployment memory)
    {
        return FerrataVault.Deployment({
            verifier: v,
            checkpointImageId: CHECKPOINT_IMAGE_ID,
            releaseImageId: RELEASE_IMAGE_ID,
            lezConfigId: lezConfig,
            genesisState: GENESIS_STATE,
            nMin: N_MIN,
            tMin: T_MIN,
            deltaFloor: DELTA_FLOOR,
            initialDelta: DELTA,
            revokeWindow: REVOKE_WINDOW,
            maxFreeze: MAX_FREEZE,
            turnSeconds: TURN_SECONDS,
            minReleaseDelay: MIN_RELEASE_DELAY,
            admin: admin,
            freezeAuthority: freezer,
            globalDepositCap: _unlimited(),
            globalReleaseCap: _unlimited(),
            mode: FerrataVault.Mode.A,
            watchers: [watcher, address(0), address(0)],
            releaseCapFloor: _cap(0, 0),
            feeRecipient: feeRecipient
        });
    }

    /// @dev The same deployment under rule (B): turnSeconds 0, delta and its floor 1 h.
    function _deploymentB(IRiscZeroVerifier v, bytes32 lezConfig)
        internal
        view
        returns (FerrataVault.Deployment memory d)
    {
        d = _deployment(v, lezConfig);
        d.mode = FerrataVault.Mode.B;
        d.turnSeconds = 0;
        d.deltaFloor = DELTA_B;
        d.initialDelta = DELTA_B;
    }

    function _deployVault(IRiscZeroVerifier v, bytes32 lezConfig) internal returns (FerrataVault) {
        return new FerrataVault(_deployment(v, lezConfig));
    }

    /// @dev Value unit = 1e-6 USD. ETH at 3000 USD: one lezUnit (1e12 wei) is worth 3000 value units.
    ///      Relay fees are 0.2% of each denomination.
    function _registerDefaultTokens(FerrataVault v) internal {
        vm.startPrank(admin);
        v.registerToken(ETH, _params(_denoms(0.1 ether, 1 ether, 10 ether), 1e12, 3000));
        v.registerToken(address(usdc), _params(_denoms(100e6, 1000e6, 10_000e6), 1, 1));
        v.registerToken(address(dai), _params(_denoms(100e18, 1000e18, 10_000e18), 1e12, 1));
        v.registerToken(address(usdt), _params(_denoms(100e6, 1000e6, 10_000e6), 1, 1));
        vm.stopPrank();
    }

    /// @dev Registration parameters with relay fees of 0.2% per denomination, no checkpoint fee and UNLIMITED caps;
    ///      decimals 18 (every test unit is at most 10**18).
    function _params(uint256[] memory denoms, uint256 lezUnit, uint256 unitWeight)
        internal
        pure
        returns (FerrataVault.TokenParams memory p)
    {
        uint256[] memory fees = new uint256[](denoms.length);
        for (uint256 i = 0; i < denoms.length; i++) {
            fees[i] = denoms[i] / 500;
        }
        p = FerrataVault.TokenParams({
            denominations: denoms,
            relayFees: fees,
            lezUnit: lezUnit,
            unitWeight: unitWeight,
            depositCap: _unlimited(),
            releaseCap: _unlimited(),
            releaseDelay: 0,
            lezDefinition: keccak256(abi.encode("test LEZ definition", denoms[0])),
            decimals: 18,
            checkpointFees: new uint256[](denoms.length)
        });
    }

    function _denoms(uint256 a, uint256 b, uint256 c) internal pure returns (uint256[] memory d) {
        d = new uint256[](3);
        d[0] = a;
        d[1] = b;
        d[2] = c;
    }

    function _amount(address token, uint8 denomIndex) internal view returns (uint256) {
        return _denomsOf(vault, token)[denomIndex];
    }

    function _relayFee(address token, uint8 denomIndex) internal view returns (uint256) {
        (, uint256 relayFee,,) = vault.fees(token, 0, denomIndex);
        return relayFee;
    }

    function _denomsOf(FerrataVault v, address token) internal view returns (uint256[] memory denoms) {
        (,,,, denoms) = v.tokenConfig(token);
    }

    /// @dev The registered relay fees (the same in every fee schedule).
    function _relayFeesOf(FerrataVault v, address token) internal view returns (uint256[] memory fees) {
        fees = new uint256[](_denomsOf(v, token).length);
        for (uint8 i = 0; i < fees.length; i++) {
            (, fees[i],,) = v.fees(token, 0, i);
        }
    }

    /// @dev The fee schedule a deposit or release of `token` gets now: the announced one once due, else the current.
    function _scheduleNow(FerrataVault v, address token) internal view returns (uint256) {
        (uint64 current, uint64 pendingAt) = v.feeState(token);
        return pendingAt != 0 && block.timestamp >= pendingAt ? uint256(current) + 1 : current;
    }

    /// @dev What a deposit of `denomIndex` escrows now: the denomination, its mint fee and its relay fee (D13).
    function _depositTotal(FerrataVault v, address token, uint8 denomIndex) internal view returns (uint256) {
        (uint256 mintFee, uint256 relayFee,,) = v.fees(token, _scheduleNow(v, token), denomIndex);
        return _denomsOf(v, token)[denomIndex] + mintFee + relayFee;
    }

    /// @dev The deposit-tree leaf, from the reference implementation (spec §3.1).
    function _leafHash(uint64 leafIndex, address depositor, address token, uint256 amount, bytes32 commitment)
        internal
        pure
        returns (bytes32)
    {
        return ReferenceMerkle.leafHash(leafIndex, depositor, token, amount, commitment);
    }

    /// @dev Checkpoint record `i` of `v`, decoded from the getter's return data as one struct (eleven return values do
    ///      not fit on the stack next to a caller's locals).
    function _checkpointRecord(FerrataVault v, uint256 i) internal view returns (FerrataVault.FinalizedCheckpoint memory) {
        (bool ok, bytes memory data) = address(v).staticcall(abi.encodeCall(v.checkpoints, (i)));
        require(ok, "checkpoint getter");
        return abi.decode(data, (FerrataVault.FinalizedCheckpoint));
    }

    function _isFrozen(uint256 until) internal view returns (bool) {
        return block.timestamp < until;
    }

    function _balance(address token, address who) internal view returns (uint256) {
        return token == ETH ? who.balance : MockTokenBase(token).balanceOf(who);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Deposits
    // ------------------------------------------------------------------------------------------------------------

    function _deposit(address from, address token, uint8 denomIndex, bytes32 commitment)
        internal
        returns (uint64 leafIndex)
    {
        return _depositInto(vault, from, token, denomIndex, commitment);
    }

    /// @dev Funds `from` with the deposit's total (denomination and fees), approves if needed, and deposits with the
    ///      correct per-depositor index.
    function _depositInto(FerrataVault v, address from, address token, uint8 denomIndex, bytes32 commitment)
        internal
        returns (uint64 leafIndex)
    {
        uint256 amount = _depositTotal(v, token, denomIndex);
        uint64 index = v.depositCount(from);
        leafIndex = v.nextLeafIndex();
        if (token == ETH) {
            vm.deal(from, from.balance + amount);
            vm.prank(from);
            v.depositETH{value: amount}(denomIndex, commitment, index);
        } else {
            MockTokenBase(token).mint(from, amount);
            vm.prank(from);
            MockTokenBase(token).approve(address(v), amount);
            vm.prank(from);
            v.deposit(token, denomIndex, commitment, index);
        }
    }

    // ------------------------------------------------------------------------------------------------------------
    // Journals: the exact byte layouts a proof must commit to
    // ------------------------------------------------------------------------------------------------------------

    /// @dev 17 words: domain, chainId, vault, lezConfigId, nMin, tMin, parentState, newState, burnRoot, burnCount,
    ///      importedLeafCount, sunset, echoNonce, tipId, tipHash, n, t (SPEC-v3 D3).
    function _checkpointJournalBytes(CheckpointJournal memory j) internal pure returns (bytes memory) {
        bytes memory head = bytes.concat(
            sha256(bytes("ferrata/vault/checkpoint/v3")),
            bytes32(j.chainId),
            bytes32(uint256(uint160(j.vault))),
            j.lezConfigId,
            bytes32(uint256(j.nMin)),
            bytes32(uint256(j.tMin))
        );
        bytes memory body = bytes.concat(
            j.parentState,
            j.newState,
            j.burnRoot,
            bytes32(uint256(j.burnCount)),
            bytes32(uint256(j.importedLeafCount)),
            bytes32(uint256(j.sunset ? 1 : 0)),
            j.echoNonce
        );
        bytes memory tail =
            bytes.concat(bytes32(uint256(j.tipId)), j.tipHash, bytes32(uint256(j.n)), bytes32(uint256(j.t)));
        return bytes.concat(head, body, tail);
    }

    /// @dev 11 words: domain, chainId, vault, lezConfigId, burnRoot, nullifier, token, amount, recipient,
    ///      relayer, relayFee.
    function _releaseJournalBytes(ReleaseJournal memory j) internal pure returns (bytes memory) {
        bytes memory context = bytes.concat(
            sha256(bytes("ferrata/vault/release/v2")),
            bytes32(j.chainId),
            bytes32(uint256(uint160(j.vault))),
            j.lezConfigId,
            j.burnRoot,
            j.nullifier
        );
        bytes memory payout = bytes.concat(
            bytes32(uint256(uint160(j.token))),
            bytes32(j.amount),
            bytes32(uint256(uint160(j.recipient))),
            bytes32(uint256(uint160(j.relayer))),
            bytes32(j.relayFee)
        );
        return bytes.concat(context, payout);
    }

    /// @dev The journal a checkpoint proof for vault `v` (configured with `lezConfig`) must commit to for `c`.
    function _checkpointJournal(FerrataVault v, bytes32 lezConfig, FerrataVault.Checkpoint memory c)
        internal
        view
        returns (CheckpointJournal memory)
    {
        return CheckpointJournal({
            chainId: block.chainid,
            vault: address(v),
            lezConfigId: lezConfig,
            nMin: c.nMin,
            tMin: c.tMin,
            parentState: c.parentState,
            newState: c.newState,
            burnRoot: c.burnRoot,
            burnCount: c.burnCount,
            importedLeafCount: c.importedLeafCount,
            sunset: c.sunset,
            echoNonce: c.echoNonce,
            tipId: c.tipId,
            tipHash: c.tipHash,
            n: c.n,
            t: c.t
        });
    }

    function _checkpointJournal(FerrataVault.Checkpoint memory c) internal view returns (CheckpointJournal memory) {
        return _checkpointJournal(vault, LEZ_CONFIG_ID, c);
    }

    /// @dev The journal the vault should reconstruct for `r`.
    function _releaseJournal(FerrataVault.ReleaseRequest memory r) internal view returns (ReleaseJournal memory) {
        return ReleaseJournal({
            chainId: block.chainid,
            vault: address(vault),
            lezConfigId: LEZ_CONFIG_ID,
            burnRoot: r.burnRoot,
            nullifier: r.nullifier,
            token: r.token,
            amount: _denomsOf(vault, r.token)[r.denomIndex],
            recipient: r.recipient,
            relayer: r.relayer,
            relayFee: r.relayFee
        });
    }

    /// @dev Stands in for proving: makes the mock verifier accept this journal for the checkpoint program.
    function _proveCheckpoint(CheckpointJournal memory j) internal {
        verifier.accept(CHECKPOINT_IMAGE_ID, sha256(_checkpointJournalBytes(j)));
    }

    /// @dev Stands in for proving: makes the mock verifier accept this journal for the release program.
    function _proveRelease(ReleaseJournal memory j) internal {
        verifier.accept(RELEASE_IMAGE_ID, sha256(_releaseJournalBytes(j)));
    }

    // ------------------------------------------------------------------------------------------------------------
    // Checkpoints: candidate, beacon, finalisation
    // ------------------------------------------------------------------------------------------------------------

    /// @dev A fresh checkpoint built on `v.finalState()`, with a new state id, tip and burn count, and the default
    ///      key set (n = 4, t = 3).
    function _nextCheckpoint(FerrataVault v, bytes32 burnRoot, uint64 importedLeafCount, bool sunset)
        internal
        returns (FerrataVault.Checkpoint memory)
    {
        stateNonce += 1;
        burnCounter += 1;
        return FerrataVault.Checkpoint({
            nMin: N_MIN,
            tMin: T_MIN,
            parentState: v.finalState(),
            newState: keccak256(abi.encode("test LEZ checkpoint state", stateNonce)),
            burnRoot: burnRoot,
            burnCount: burnCounter,
            importedLeafCount: importedLeafCount,
            sunset: sunset,
            echoNonce: bytes32(0),
            tipId: 1000 + burnCounter,
            tipHash: keccak256(abi.encode("test LEZ tip", stateNonce)),
            n: N,
            t: T
        });
    }

    /// @dev As above, carrying the vault's current imported leaf count and sunset flag forward (monotonicity, v3 D4).
    function _nextCheckpoint(bytes32 burnRoot) internal returns (FerrataVault.Checkpoint memory) {
        return _nextCheckpoint(vault, burnRoot, vault.finalImportedLeafCount(), vault.finalSunset());
    }

    /// @dev Adds `k` ETH deposits from a fresh depositor, so that checkpoints may claim up to that many imported
    ///      leaves (v4, D11: a finalised imported leaf count never exceeds the tree's leaf count).
    function _fillTree(uint256 k) internal {
        address filler = makeAddr("tree filler");
        for (uint256 i = 0; i < k; i++) {
            _deposit(filler, ETH, 0, keccak256(abi.encode("filler leaf", i)));
        }
    }

    /// @dev W = deltaAtSubmit + (t - 1) * turnSeconds, computed here independently of the vault.
    function _waitOf(FerrataVault v, bytes32 newState) internal view returns (uint256 submittedAt, uint256 wait) {
        uint32 t;
        uint32 deltaAtSubmit;
        uint48 at;
        (,,, t, at, deltaAtSubmit,) = v.candidates(newState);
        submittedAt = at;
        wait = uint256(deltaAtSubmit) + (uint256(t) - 1) * v.turnSeconds();
    }

    /// @dev Proves `c` as a candidate (echoNonce 0) for vault `v` and submits it.
    function _submit(FerrataVault v, bytes32 lezConfig, FerrataVault.Checkpoint memory c) internal {
        _proveCheckpoint(_checkpointJournal(v, lezConfig, c));
        v.submitCandidate(c, SEAL);
    }

    function _submit(FerrataVault.Checkpoint memory c) internal {
        _submit(vault, LEZ_CONFIG_ID, c);
    }

    /// @dev Waits out the candidate's wait W, records a beacon in a new block, sets `c.echoNonce` to that beacon's
    ///      nonce (so `c` is modified), proves the finalisation journal and finalises. Returns the checkpoint index.
    function _finalizeAfterDelta(FerrataVault v, bytes32 lezConfig, FerrataVault.Checkpoint memory c)
        internal
        returns (uint256 index)
    {
        (uint256 submittedAt, uint256 wait) = _waitOf(v, c.newState);
        if (block.timestamp < submittedAt + wait) vm.warp(submittedAt + wait);
        if (v.mode() == FerrataVault.Mode.B) {
            // Rule (B): no beacon, no second proof; the candidate's own journal, once W has passed.
            v.finalizeAttested(c);
            return v.checkpointCount() - 1;
        }
        vm.roll(block.number + 1);
        uint256 beaconIndex = v.recordBeacon();
        (, bytes32 nonce) = v.beacons(beaconIndex);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(v, lezConfig, c));
        v.finalize(beaconIndex, c, SEAL);
        return v.checkpointCount() - 1;
    }

    function _finalizeAfterDelta(FerrataVault.Checkpoint memory c) internal returns (uint256) {
        return _finalizeAfterDelta(vault, LEZ_CONFIG_ID, c);
    }

    /// @dev Makes `burnRoot` accepted on `v` the only way the vault allows: a candidate, a beacon `delta` later,
    ///      and a finalisation. Returns the checkpoint index.
    function _acceptRootOn(FerrataVault v, bytes32 lezConfig, bytes32 burnRoot) internal returns (uint256) {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(v, burnRoot, v.finalImportedLeafCount(), v.finalSunset());
        _submit(v, lezConfig, c);
        return _finalizeAfterDelta(v, lezConfig, c);
    }

    function _acceptRoot(bytes32 burnRoot) internal returns (uint256) {
        return _acceptRootOn(vault, LEZ_CONFIG_ID, burnRoot);
    }

    function _finalizedAt(bytes32 burnRoot) internal view returns (uint256 finalizedAt) {
        (, finalizedAt) = vault.burnRootStatus(burnRoot);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Releases
    // ------------------------------------------------------------------------------------------------------------

    /// @dev A request with no relay fee and relayer = address(0) (the fee, if any, would go to msg.sender).
    function _request(address token, uint8 denomIndex, address to, bytes32 burnRoot, bytes32 nullifier)
        internal
        pure
        returns (FerrataVault.ReleaseRequest memory)
    {
        return FerrataVault.ReleaseRequest({
            token: token,
            denomIndex: denomIndex,
            recipient: to,
            relayer: address(0),
            relayFee: 0,
            burnRoot: burnRoot,
            nullifier: nullifier
        });
    }

    /// @dev Proves `r` and submits it from the relayer.
    function _release(FerrataVault.ReleaseRequest memory r) internal {
        _proveRelease(_releaseJournal(r));
        vm.prank(relayer);
        vault.release(r, SEAL);
    }
}
