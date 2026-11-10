// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ReentrancyGuardTransient} from "@openzeppelin/contracts/utils/ReentrancyGuardTransient.sol";

import {IRiscZeroVerifier} from "./interfaces/IRiscZeroVerifier.sol";

/// @title FerrataVault (v2)
/// @notice Ethereum half of a privacy-preserving lock-and-mint bridge to LEZ (Logos RFP-022). Prototype, not audited.
///         Implements notes/design/SPEC-v2.md.
/// @dev One vault per bridge configuration. Deposits of fixed denominations are appended to an incremental SHA-256
///      Merkle tree whose root is kept in storage slot 0, which the LEZ side proves with an Ethereum storage proof.
///      LEZ burn roots become accepted only through finalised checkpoints (candidate, then a beacon at least `delta`
///      later, then finalisation). Releases pay out against a RISC Zero proof of a burn under an accepted root.
///      There is no upgradeability, no delegatecall, no settable verifier and no token rescue function.
contract FerrataVault is ReentrancyGuardTransient {
    using SafeERC20 for IERC20;

    // ------------------------------------------------------------------------------------------------------------
    // Types
    // ------------------------------------------------------------------------------------------------------------

    enum Direction {
        Deposit,
        Release
    }

    /// @notice Token-bucket parameters. `capacity == UNLIMITED` disables the bucket.
    struct CapParams {
        uint256 capacity;
        uint256 refillPerSecond;
    }

    /// @notice Everything fixed at deployment.
    struct Deployment {
        IRiscZeroVerifier verifier; // a RiscZeroVerifierEmergencyStop around the Groth16 verifier (spec §6)
        bytes32 checkpointImageId;
        bytes32 releaseImageId;
        bytes32 lezConfigId;
        bytes32 genesisState; // the LEZ checkpoint state the first candidate must build on
        uint32 nMin; // smallest accredited key set the checkpoint program may accept
        uint32 tMin; // smallest signing threshold the checkpoint program may accept
        uint256 deltaFloor; // seconds; `delta` can never go below it
        uint256 initialDelta;
        uint256 revokeWindow; // seconds after finalisation during which a checkpoint can be revoked
        uint256 maxFreeze; // seconds a freeze lasts unless renewed (spec §7)
        address admin;
        address freezeAuthority;
        CapParams globalDepositCap; // value units; starts full
        CapParams globalReleaseCap; // value units; starts full
    }

    /// @notice The checkpoint-journal fields the caller supplies, in journal order. The vault adds
    ///         CHECKPOINT_DOMAIN, the chain id, its own address and `lezConfigId` in front (spec §4.2 item 5).
    struct Checkpoint {
        uint32 nMin;
        uint32 tMin;
        bytes32 parentState;
        bytes32 newState;
        bytes32 burnRoot;
        uint64 burnCount;
        uint64 importedLeafCount;
        bool sunset;
        bytes32 echoNonce;
    }

    /// @dev A submitted, not yet finalised checkpoint, keyed by its `newState`. It is valid only in the epoch it was
    ///      submitted in: every finalisation and every revocation starts a new epoch.
    struct Candidate {
        bytes32 parentState;
        bytes32 burnRoot;
        uint64 burnCount;
        uint64 importedLeafCount;
        bool sunset;
        uint256 submittedAt; // 0 means no candidate
        uint256 deltaAtSubmit;
        uint256 epoch;
    }

    /// @dev History entry of a finalised checkpoint (spec §4.3, needed by §4.4).
    struct FinalizedCheckpoint {
        bytes32 parentState;
        bytes32 newState;
        bytes32 burnRoot;
        uint64 burnCount;
        uint64 importedLeafCount;
        bool sunset;
        bool revoked;
        uint256 finalizedAt;
    }

    struct Beacon {
        uint256 timestamp;
        bytes32 nonce;
    }

    /// @notice A release. Every field except `denomIndex` is bound into the release journal; `denomIndex` is bound
    ///         through the amount it selects.
    struct ReleaseRequest {
        address token;
        uint8 denomIndex;
        address recipient;
        address relayer; // receives the relay fee; address(0) means msg.sender
        uint256 relayFee; // 0 or exactly the registered relay fee of this denomination
        bytes32 burnRoot;
        bytes32 nullifier;
    }

    /// @notice A registration. Everything here except the caps is write-once.
    struct TokenParams {
        uint256[] denominations; // 1..8, strictly increasing, non-zero, each a multiple of lezUnit
        uint256[] relayFees; // one per denomination, each at most 1% of it
        uint256 lezUnit; // a power of ten: base units per LEZ unit of the wrapped token
        uint256 unitWeight; // global-cap value units per lezUnit
        CapParams depositCap;
        CapParams releaseCap;
        uint256 releaseDelay; // initial release delay (adjustable later, at most 30 days)
    }

    struct TokenConfig {
        bool registered;
        bool depositsEnabled;
        uint256 lezUnit;
        uint256 unitWeight;
        uint256[] denominations;
        uint256[] relayFees;
    }

    /// @dev A release-delay setting and the first checkpoint index it applies to.
    struct DelayChange {
        uint256 fromCheckpoint;
        uint256 delay;
    }

    /// @dev Token bucket. Invariant: `level <= capacity` whenever `capacity != UNLIMITED`.
    struct Bucket {
        uint256 capacity;
        uint256 refillPerSecond;
        uint256 level; // available amount at `lastUpdate`
        uint256 lastUpdate;
    }

    // ------------------------------------------------------------------------------------------------------------
    // Constants
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Storage slot of `depositRoot`; part of the cross-chain interface.
    uint256 public constant DEPOSIT_ROOT_SLOT = 0;
    /// @notice Storage slot of `nextLeafIndex`; nothing else is packed into it.
    uint256 public constant NEXT_LEAF_INDEX_SLOT = 1;
    /// @notice Storage slot of `storedLezConfigId`, which the LEZ configuration's `Init` attests (reciprocal pairing).
    uint256 public constant LEZ_CONFIG_ID_SLOT = 2;

    uint256 public constant TREE_DEPTH = 32;
    uint256 public constant MAX_LEAVES = 2 ** TREE_DEPTH;
    uint256 public constant MAX_DENOMINATIONS = 8;
    /// @notice Bounds that keep the cap arithmetic `amount * unitWeight / lezUnit` from overflowing.
    uint256 public constant MAX_DENOMINATION = type(uint128).max;
    uint256 public constant MAX_UNIT_WEIGHT = type(uint128).max;
    uint256 public constant MAX_RELEASE_DELAY = 30 days;
    /// @notice Ceiling for `delta`. Not in the spec: without it a raise-only delta could be pushed out of reach for
    ///         good (and `submittedAt + delta` could overflow).
    uint256 public constant MAX_DELTA = 30 days;
    uint256 public constant UNLIMITED = type(uint256).max;
    /// @notice Sentinel token address for native ETH.
    address public constant ETH = address(0);

    bytes1 public constant LEAF_PREFIX = 0x00;
    bytes1 public constant NODE_PREFIX = 0x01;
    /// @notice sha256("ferrata/vault/leaf/v2")
    bytes32 public constant LEAF_TAG = 0x17b9508a0d0cbf61207c0fcb66f06ed92a6aeec110a54860479f0bf3cd6aff0b;
    /// @notice sha256("ferrata/vault/checkpoint/v2")
    bytes32 public constant CHECKPOINT_DOMAIN = 0xe99926553e520475aa7b6407f28efcca58fa91851d8476395f692eb871457ce1;
    /// @notice sha256("ferrata/vault/release/v2")
    bytes32 public constant RELEASE_DOMAIN = 0x6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e51;

    // ------------------------------------------------------------------------------------------------------------
    // Immutables: everything that defines what the vault accepts as a valid proof
    // ------------------------------------------------------------------------------------------------------------

    IRiscZeroVerifier public immutable verifier;
    bytes32 public immutable checkpointImageId;
    bytes32 public immutable releaseImageId;
    /// @notice Identifies the paired LEZ configuration (spec §1).
    bytes32 public immutable lezConfigId;
    bytes32 public immutable genesisState;
    uint32 public immutable nMin;
    uint32 public immutable tMin;
    uint256 public immutable deltaFloor;
    uint256 public immutable revokeWindow;
    uint256 public immutable maxFreeze;

    // ------------------------------------------------------------------------------------------------------------
    // Storage. The first three variables are part of the cross-chain interface: the LEZ side reads `depositRoot`
    // from slot 0 and `storedLezConfigId` from slot 2 with Ethereum storage proofs. Nothing may be declared before
    // them, and the only parent contract (ReentrancyGuardTransient) uses transient storage, not storage slots.
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Root of the deposit tree. Storage slot 0.
    bytes32 public depositRoot;
    /// @notice Number of leaves in the deposit tree, i.e. the index the next leaf gets. Storage slot 1.
    uint64 public nextLeafIndex;
    /// @notice Copy of the immutable `lezConfigId`, storage slot 2. A bytes32 starts a new slot, so slot 1 holds
    ///         only `nextLeafIndex`.
    bytes32 public storedLezConfigId;
    /// @dev `_branch[h]` is the root of the most recently completed subtree of height h. It is read only while
    ///      bit h of `nextLeafIndex` is set, which is exactly when that subtree is the left neighbour of the
    ///      next leaf's path at height h.
    bytes32[TREE_DEPTH] private _branch;

    address public admin;
    address public pendingAdmin;
    address public freezeAuthority;
    address public pendingFreezeAuthority;
    /// @notice Set once by `retire()`; deposits revert forever afterwards.
    bool public retired;
    /// @notice Freezes are stored as the time they lapse (0: not frozen); see `depositsFrozen()` etc.
    uint256 public depositsFrozenUntil;
    uint256 public releasesFrozenUntil;
    uint256 public candidatesFrozenUntil;

    /// @notice The latest finalised LEZ checkpoint state (starts at `genesisState`).
    bytes32 public finalState;
    /// @notice Seconds a candidate must wait for its finalisation beacon; applies to candidates submitted later.
    uint256 public delta;
    /// @notice Incremented by every finalisation and every revocation; a candidate is valid only in its epoch.
    uint256 public stateEpoch;
    uint256 private _lastBeaconBlock;
    Beacon[] public beacons;
    mapping(bytes32 newState => Candidate) public candidates;
    FinalizedCheckpoint[] public checkpoints;
    /// @notice For an accepted burn root: 1 + the index of the checkpoint that made it accepted; 0 if none.
    mapping(bytes32 burnRoot => uint256) public burnRootCheckpoint;
    bool public sunsetLeafCountSet;
    /// @notice Leaves with an index at or above this were never imported on LEZ and are refundable (spec §4.5).
    uint64 public sunsetLeafCount;
    /// @notice Index of the checkpoint that last set `sunsetLeafCount`.
    uint256 public sunsetSetAt;

    mapping(address token => TokenConfig) private _tokens;
    mapping(address token => uint256) public tokenDepositsFrozenUntil;
    mapping(address token => uint256) public tokenReleasesFrozenUntil;
    /// @dev Per token, every release-delay setting with the first checkpoint index it applies to (spec §5.3).
    mapping(address token => DelayChange[]) private _releaseDelays;
    mapping(address token => uint256) public escrowed;
    /// @notice Payments that could not be pushed (spec §5.4); only the payee can claim them.
    mapping(address payee => mapping(address token => uint256)) public owed;
    mapping(address token => uint256) public totalOwed;
    mapping(address depositor => uint64) public depositCount;
    mapping(bytes32 nullifier => bool) public nullifierUsed;
    mapping(uint64 leafIndex => bool) public refunded;

    mapping(address token => Bucket) private _tokenDepositCap;
    mapping(address token => Bucket) private _tokenReleaseCap;
    Bucket private _globalDepositCap;
    Bucket private _globalReleaseCap;

    // ------------------------------------------------------------------------------------------------------------
    // Events
    // ------------------------------------------------------------------------------------------------------------

    event Deposit(
        address indexed depositor,
        uint64 index,
        uint64 leafIndex,
        address indexed token,
        uint256 amount,
        bytes32 commitment
    );
    event BeaconRecorded(uint256 indexed index, uint256 timestamp, bytes32 nonce);
    event CandidateSubmitted(
        bytes32 indexed newState,
        bytes32 indexed parentState,
        bytes32 burnRoot,
        uint64 burnCount,
        uint64 importedLeafCount,
        bool sunset,
        uint256 deltaAtSubmit
    );
    event CheckpointFinalized(
        uint256 indexed index,
        bytes32 indexed newState,
        bytes32 parentState,
        bytes32 burnRoot,
        uint64 burnCount,
        uint64 importedLeafCount,
        bool sunset
    );
    event CheckpointsRevoked(uint256 indexed fromIndex, uint256 toIndex, bytes32 restoredState);
    event SunsetLeafCountSet(uint256 indexed checkpointIndex, uint64 sunsetLeafCount);
    event SunsetLeafCountCleared();
    event Release(
        bytes32 indexed nullifier,
        address indexed token,
        address indexed recipient,
        uint256 amount,
        address relayer,
        uint256 relayFee,
        bytes32 burnRoot
    );
    event Refund(uint64 indexed leafIndex, address indexed depositor, address indexed token, uint256 amount);
    event OwedCredited(address indexed payee, address indexed token, uint256 amount);
    event OwedClaimed(address indexed payee, address indexed token, uint256 amount);
    event Retired();

    event TokenRegistered(
        address indexed token, uint256[] denominations, uint256[] relayFees, uint256 lezUnit, uint256 unitWeight
    );
    event DepositsEnabledSet(address indexed token, bool enabled);
    event TokenCapSet(address indexed token, Direction indexed direction, uint256 capacity, uint256 refillPerSecond);
    event GlobalCapSet(Direction indexed direction, uint256 capacity, uint256 refillPerSecond);
    event ReleaseDelaySet(address indexed token, uint256 delay, uint256 fromCheckpoint);
    event DeltaSet(uint256 delta);
    event AdminTransferStarted(address indexed currentAdmin, address indexed pendingAdmin);
    event AdminTransferred(address indexed previousAdmin, address indexed newAdmin);
    event FreezeAuthorityTransferStarted(address indexed currentAuthority, address indexed pendingAuthority);
    event FreezeAuthorityTransferred(address indexed previousAuthority, address indexed newAuthority);
    event DepositsFrozenSet(bool frozen, uint256 until);
    event ReleasesFrozenSet(bool frozen, uint256 until);
    event CandidatesFrozenSet(bool frozen, uint256 until);
    event TokenDepositsFrozenSet(address indexed token, bool frozen, uint256 until);
    event TokenReleasesFrozenSet(address indexed token, bool frozen, uint256 until);

    // ------------------------------------------------------------------------------------------------------------
    // Errors. None carries parameters, so no error echoes user data.
    // ------------------------------------------------------------------------------------------------------------

    error NotAdmin();
    error NotFreezeAuthority();
    error NotPendingAdmin();
    error NotPendingFreezeAuthority();
    error ZeroAddress();
    error RolesMustDiffer();
    error VerifierHasNoCode();
    error InvalidImageIds();
    error InvalidLezConfigId();
    error InvalidGenesisState();
    error InvalidCommitteeFloor();
    error InvalidDelta();
    error InvalidRevokeWindow();
    error InvalidMaxFreeze();

    error TokenAlreadyRegistered();
    error TokenNotRegistered();
    error TokenHasNoCode();
    error InvalidDenominations();
    error InvalidLezUnit();
    error DenominationNotMultipleOfLezUnit();
    error InvalidUnitWeight();
    error RelayFeeTooHigh();
    error RelayFeeCountMismatch();
    error ReleaseDelayTooLong();
    error DeltaCanOnlyRise();
    error DeltaTooLong();

    error VaultRetired();
    error AlreadyRetired();
    error DepositsAreFrozen();
    error TokenDepositsAreFrozen();
    error DepositsDisabled();
    error UseDepositETH();
    error InvalidDenominationIndex();
    error ZeroCommitment();
    error WrongDepositIndex();
    error WrongEthAmount();
    error TransferAmountMismatch();
    error TreeFull();
    error TokenDepositCapExceeded();
    error GlobalDepositCapExceeded();

    error CandidatesAreFrozen();
    error ParentNotFinal();
    error EchoNonceNotZero();
    error WrongCommitteeFloor();
    error CandidateAlreadySubmitted();
    error UnknownCandidate();
    error StaleCandidate();
    error CandidateMismatch();
    error UnknownBeacon();
    error BeaconTooEarly();
    error EchoNonceMismatch();
    error UnknownCheckpoint();
    error CheckpointAlreadyRevoked();
    error RevokeWindowPassed();

    error NotRefundable();
    error UnknownLeaf();
    error AlreadyRefunded();
    error InvalidMerklePath();

    error ReleasesAreFrozen();
    error TokenReleasesAreFrozen();
    error BurnRootNotAccepted();
    error ReleaseDelayNotElapsed();
    error NullifierAlreadyUsed();
    error InvalidRelayFee();
    error ZeroRecipient();
    error InsufficientEscrow();
    error TokenReleaseCapExceeded();
    error GlobalReleaseCapExceeded();
    error EthTransferFailed();
    error NothingOwed();

    error DirectEthNotAccepted();
    error FunctionNotFound();

    // ------------------------------------------------------------------------------------------------------------
    // Modifiers
    // ------------------------------------------------------------------------------------------------------------

    modifier onlyAdmin() {
        _checkAdmin();
        _;
    }

    modifier onlyFreezeAuthority() {
        _checkFreezeAuthority();
        _;
    }

    // ------------------------------------------------------------------------------------------------------------
    // Constructor, receive, fallback
    // ------------------------------------------------------------------------------------------------------------

    constructor(Deployment memory d) {
        if (address(d.verifier).code.length == 0) revert VerifierHasNoCode();
        if (
            d.checkpointImageId == bytes32(0) || d.releaseImageId == bytes32(0)
                || d.checkpointImageId == d.releaseImageId
        ) revert InvalidImageIds();
        if (d.lezConfigId == bytes32(0)) revert InvalidLezConfigId();
        if (d.genesisState == bytes32(0)) revert InvalidGenesisState();
        if (d.nMin == 0 || d.tMin == 0) revert InvalidCommitteeFloor();
        if (d.deltaFloor == 0 || d.initialDelta < d.deltaFloor || d.initialDelta > MAX_DELTA) revert InvalidDelta();
        if (d.revokeWindow == 0) revert InvalidRevokeWindow();
        if (d.maxFreeze == 0) revert InvalidMaxFreeze();
        if (d.admin == address(0) || d.freezeAuthority == address(0)) revert ZeroAddress();
        if (d.admin == d.freezeAuthority) revert RolesMustDiffer();

        verifier = d.verifier;
        checkpointImageId = d.checkpointImageId;
        releaseImageId = d.releaseImageId;
        lezConfigId = d.lezConfigId;
        storedLezConfigId = d.lezConfigId;
        genesisState = d.genesisState;
        nMin = d.nMin;
        tMin = d.tMin;
        deltaFloor = d.deltaFloor;
        revokeWindow = d.revokeWindow;
        maxFreeze = d.maxFreeze;

        // The root of the empty tree, so that `depositRoot` is always the root of the current tree.
        bytes32 emptySubtree = bytes32(0);
        for (uint256 h = 0; h < TREE_DEPTH; h++) {
            emptySubtree = _hashNode(emptySubtree, emptySubtree);
        }
        depositRoot = emptySubtree;

        finalState = d.genesisState;
        delta = d.initialDelta;
        admin = d.admin;
        freezeAuthority = d.freezeAuthority;
        _initBucket(_globalDepositCap, d.globalDepositCap.capacity, d.globalDepositCap.refillPerSecond);
        _initBucket(_globalReleaseCap, d.globalReleaseCap.capacity, d.globalReleaseCap.refillPerSecond);

        emit AdminTransferred(address(0), d.admin);
        emit FreezeAuthorityTransferred(address(0), d.freezeAuthority);
        emit DeltaSet(d.initialDelta);
        emit GlobalCapSet(Direction.Deposit, d.globalDepositCap.capacity, d.globalDepositCap.refillPerSecond);
        emit GlobalCapSet(Direction.Release, d.globalReleaseCap.capacity, d.globalReleaseCap.refillPerSecond);
    }

    /// @notice Plain ETH transfers are refused; ETH enters only through `depositETH`.
    receive() external payable {
        revert DirectEthNotAccepted();
    }

    fallback() external payable {
        revert FunctionNotFound();
    }

    // ------------------------------------------------------------------------------------------------------------
    // Deposit side
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Escrows `denominations(token)[denomIndex]` of an ERC-20 and appends a leaf to the deposit tree.
    /// @param commitment Hiding commitment to the LEZ destination. Opaque to the vault, non-zero, may repeat.
    /// @param index Must equal `depositCount[msg.sender]`, so every deposit has a unique (depositor, index) pair.
    function deposit(address token, uint8 denomIndex, bytes32 commitment, uint64 index) external nonReentrant {
        if (token == ETH) revert UseDepositETH();
        uint256 amount = _checkDeposit(token, denomIndex, commitment, index);

        // Deposits pull the tokens before changing any state, so the vault never books an escrow it has not
        // received and its balance covers `escrowed` at every external call. The exact-delta check rejects
        // fee-on-transfer tokens. SafeERC20 accepts tokens whose transferFrom returns nothing (USDT).
        IERC20 erc20 = IERC20(token);
        uint256 balanceBefore = erc20.balanceOf(address(this));
        erc20.safeTransferFrom(msg.sender, address(this), amount);
        if (erc20.balanceOf(address(this)) != balanceBefore + amount) revert TransferAmountMismatch();

        _recordDeposit(token, amount, commitment, index);
    }

    /// @notice Escrows native ETH; `msg.value` must equal `denominations(ETH)[denomIndex]` exactly.
    function depositETH(uint8 denomIndex, bytes32 commitment, uint64 index) external payable nonReentrant {
        uint256 amount = _checkDeposit(ETH, denomIndex, commitment, index);
        if (msg.value != amount) revert WrongEthAmount();
        _recordDeposit(ETH, amount, commitment, index);
    }

    /// @notice Returns a deposit that LEZ never imported, once the configuration has been sunset (spec §3.4, §4.5).
    ///         Permissionless; pays the depositor named in the leaf. The leaf is proven against the current
    ///         `depositRoot` with a Merkle path whose direction bits are the bits of `leafIndex`.
    function refund(
        uint64 leafIndex,
        address depositor,
        address token,
        uint8 denomIndex,
        bytes32 commitment,
        bytes32[TREE_DEPTH] calldata path
    ) external nonReentrant {
        if (!sunsetLeafCountSet || leafIndex < sunsetLeafCount) revert NotRefundable();
        if (leafIndex >= nextLeafIndex) revert UnknownLeaf();
        if (refunded[leafIndex]) revert AlreadyRefunded();
        TokenConfig storage cfg = _tokens[token];
        if (!cfg.registered) revert TokenNotRegistered();
        if (denomIndex >= cfg.denominations.length) revert InvalidDenominationIndex();
        uint256 amount = cfg.denominations[denomIndex];
        bytes32 leaf = leafHash(leafIndex, depositor, token, amount, commitment);
        if (_rootFromPath(leafIndex, leaf, path) != depositRoot) revert InvalidMerklePath();
        if (amount > escrowed[token]) revert InsufficientEscrow();

        refunded[leafIndex] = true;
        escrowed[token] -= amount;
        emit Refund(leafIndex, depositor, token, amount);
        _payOrCredit(token, depositor, amount);
    }

    // ------------------------------------------------------------------------------------------------------------
    // LEZ checkpoints (spec §4.3): candidate, beacon, finalisation
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Records a beacon: the block time and a nonce nobody could know before this block. Permissionless,
    ///         at most one per block (a second call in the same block returns the existing one).
    function recordBeacon() external returns (uint256 index) {
        return _recordBeacon();
    }

    /// @notice Submits a candidate checkpoint built on `finalState`. Permissionless; starts its `delta` wait.
    function submitCandidate(Checkpoint calldata c, bytes calldata seal) external {
        if (_frozen(candidatesFrozenUntil)) revert CandidatesAreFrozen();
        if (c.parentState != finalState) revert ParentNotFinal();
        if (c.echoNonce != bytes32(0)) revert EchoNonceNotZero();
        if (c.nMin != nMin || c.tMin != tMin) revert WrongCommitteeFloor();
        Candidate storage existing = candidates[c.newState];
        // Resubmitting a live candidate must not restart its wait.
        if (existing.submittedAt != 0 && existing.epoch == stateEpoch) revert CandidateAlreadySubmitted();
        verifier.verify(seal, checkpointImageId, _checkpointDigest(c));

        candidates[c.newState] = Candidate({
            parentState: c.parentState,
            burnRoot: c.burnRoot,
            burnCount: c.burnCount,
            importedLeafCount: c.importedLeafCount,
            sunset: c.sunset,
            submittedAt: block.timestamp,
            deltaAtSubmit: delta,
            epoch: stateEpoch
        });
        _recordBeacon();
        emit CandidateSubmitted(
            c.newState, c.parentState, c.burnRoot, c.burnCount, c.importedLeafCount, c.sunset, delta
        );
    }

    /// @notice Finalises a candidate with a proof that echoes a beacon recorded at least `deltaAtSubmit` after the
    ///         candidate. Permissionless. Its burn root becomes accepted; a sunset checkpoint enables refunds.
    function finalize(uint256 beaconIndex, Checkpoint calldata c, bytes calldata seal) external {
        Candidate storage cand = candidates[c.newState];
        if (cand.submittedAt == 0) revert UnknownCandidate();
        if (cand.epoch != stateEpoch || cand.parentState != finalState) revert StaleCandidate();
        if (
            c.parentState != cand.parentState || c.burnRoot != cand.burnRoot || c.burnCount != cand.burnCount
                || c.importedLeafCount != cand.importedLeafCount || c.sunset != cand.sunset
        ) revert CandidateMismatch();
        if (c.nMin != nMin || c.tMin != tMin) revert WrongCommitteeFloor();
        if (beaconIndex >= beacons.length) revert UnknownBeacon();
        Beacon storage beacon = beacons[beaconIndex];
        if (beacon.timestamp < cand.submittedAt + cand.deltaAtSubmit) revert BeaconTooEarly();
        if (c.echoNonce != beacon.nonce) revert EchoNonceMismatch();
        verifier.verify(seal, checkpointImageId, _checkpointDigest(c));

        uint256 index = checkpoints.length;
        checkpoints.push(
            FinalizedCheckpoint({
                parentState: c.parentState,
                newState: c.newState,
                burnRoot: c.burnRoot,
                burnCount: c.burnCount,
                importedLeafCount: c.importedLeafCount,
                sunset: c.sunset,
                revoked: false,
                finalizedAt: block.timestamp
            })
        );
        finalState = c.newState;
        stateEpoch += 1;
        delete candidates[c.newState];

        uint256 previous = burnRootCheckpoint[c.burnRoot];
        if (previous == 0 || checkpoints[previous - 1].revoked) burnRootCheckpoint[c.burnRoot] = index + 1;
        if (c.sunset) {
            sunsetLeafCountSet = true;
            sunsetLeafCount = c.importedLeafCount;
            sunsetSetAt = index;
            emit SunsetLeafCountSet(index, c.importedLeafCount);
        }
        emit CheckpointFinalized(
            index, c.newState, c.parentState, c.burnRoot, c.burnCount, c.importedLeafCount, c.sunset
        );
    }

    // ------------------------------------------------------------------------------------------------------------
    // Release side
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Releases one denomination to `r.recipient` against a proof of a LEZ burn. Permissionless.
    /// @dev Checks, then the proof, then all state changes, then the payouts. A payout that fails is credited to
    ///      `owed` instead (spec §5.4): the release completes, the nullifier is used, and only the payee can claim.
    function release(ReleaseRequest calldata r, bytes calldata seal) external nonReentrant {
        uint256 amount = _checkRelease(r);
        verifier.verify(seal, releaseImageId, _releaseJournalDigest(r, amount));

        nullifierUsed[r.nullifier] = true;
        _consumeReleaseCaps(r.token, amount);
        escrowed[r.token] -= amount;

        address feePayee = r.relayer == address(0) ? msg.sender : r.relayer;
        emit Release(r.nullifier, r.token, r.recipient, amount, feePayee, r.relayFee, r.burnRoot);

        _payOrCredit(r.token, r.recipient, amount - r.relayFee);
        if (r.relayFee != 0) _payOrCredit(r.token, feePayee, r.relayFee);
    }

    /// @notice Withdraws what the vault owes the caller in `token` after a failed push payment. Only the payee can
    ///         withdraw, and only to itself.
    function claimOwed(address token) external nonReentrant {
        uint256 amount = owed[msg.sender][token];
        if (amount == 0) revert NothingOwed();
        owed[msg.sender][token] = 0;
        totalOwed[token] -= amount;
        emit OwedClaimed(msg.sender, token, amount);
        if (token == ETH) {
            if (!_trySendEth(msg.sender, amount)) revert EthTransferFailed();
        } else {
            IERC20(token).safeTransfer(msg.sender, amount);
        }
    }

    // ------------------------------------------------------------------------------------------------------------
    // Admin: token registry, caps, delays, delta
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Registers a token (ETH is `address(0)`), once. Denominations, relay fees, `lezUnit` and `unitWeight`
    ///         are write-once, because in-flight deposits and burns refer to them; only the caps, `depositsEnabled`
    ///         and the release delay stay adjustable. Deposits start enabled.
    function registerToken(address token, TokenParams calldata p) external onlyAdmin {
        TokenConfig storage cfg = _tokens[token];
        if (cfg.registered) revert TokenAlreadyRegistered();
        if (token != ETH && token.code.length == 0) revert TokenHasNoCode();
        _checkLezUnit(p.lezUnit);
        _checkUnitWeight(p.unitWeight);
        _checkDenominations(p.denominations, p.lezUnit);
        _checkRelayFees(p.relayFees, p.denominations);
        if (p.releaseDelay > MAX_RELEASE_DELAY) revert ReleaseDelayTooLong();

        cfg.registered = true;
        cfg.depositsEnabled = true;
        cfg.lezUnit = p.lezUnit;
        cfg.unitWeight = p.unitWeight;
        cfg.denominations = p.denominations;
        cfg.relayFees = p.relayFees;
        _initBucket(_tokenDepositCap[token], p.depositCap.capacity, p.depositCap.refillPerSecond);
        _initBucket(_tokenReleaseCap[token], p.releaseCap.capacity, p.releaseCap.refillPerSecond);
        _releaseDelays[token].push(DelayChange({fromCheckpoint: 0, delay: p.releaseDelay}));

        emit TokenRegistered(token, p.denominations, p.relayFees, p.lezUnit, p.unitWeight);
        emit TokenCapSet(token, Direction.Deposit, p.depositCap.capacity, p.depositCap.refillPerSecond);
        emit TokenCapSet(token, Direction.Release, p.releaseCap.capacity, p.releaseCap.refillPerSecond);
        emit ReleaseDelaySet(token, p.releaseDelay, 0);
    }

    /// @notice Enables or disables deposits of a token (delisting). Releases are unaffected.
    function setDepositsEnabled(address token, bool enabled) external onlyAdmin {
        _requireRegistered(token);
        _tokens[token].depositsEnabled = enabled;
        emit DepositsEnabledSet(token, enabled);
    }

    /// @notice Updates a per-token bucket. The level earned under the old parameters is kept, clamped to the new
    ///         capacity; a bucket that was UNLIMITED starts full.
    function setTokenCap(address token, Direction direction, uint256 capacity, uint256 refillPerSecond)
        external
        onlyAdmin
    {
        _requireRegistered(token);
        _updateBucket(_tokenBucket(token, direction), capacity, refillPerSecond);
        emit TokenCapSet(token, direction, capacity, refillPerSecond);
    }

    /// @notice Updates a global bucket (value units), with the same rules as `setTokenCap`.
    function setGlobalCap(Direction direction, uint256 capacity, uint256 refillPerSecond) external onlyAdmin {
        _updateBucket(_globalBucket(direction), capacity, refillPerSecond);
        emit GlobalCapSet(direction, capacity, refillPerSecond);
    }

    /// @notice Sets the per-token delay between a burn root's finalisation and releases against it, for roots
    ///         finalised after this call; roots already finalised keep the delay they were finalised under (§5.3).
    function setReleaseDelay(address token, uint256 delay) external onlyAdmin {
        _requireRegistered(token);
        if (delay > MAX_RELEASE_DELAY) revert ReleaseDelayTooLong();
        uint256 fromCheckpoint = checkpoints.length;
        _releaseDelays[token].push(DelayChange({fromCheckpoint: fromCheckpoint, delay: delay}));
        emit ReleaseDelaySet(token, delay, fromCheckpoint);
    }

    /// @notice Raises `delta`. It can never go down; candidates already submitted keep the delta they started with.
    function raiseDelta(uint256 newDelta) external onlyAdmin {
        if (newDelta < delta) revert DeltaCanOnlyRise();
        if (newDelta > MAX_DELTA) revert DeltaTooLong();
        delta = newDelta;
        emit DeltaSet(newDelta);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Freeze authority. It cannot touch the registry, caps or escrow; the admin cannot freeze or unfreeze.
    // A freeze lasts `maxFreeze` unless renewed (freezing again restarts it), so a lost key cannot lock the vault
    // forever (spec §7). Unfreezing ends it at once.
    // ------------------------------------------------------------------------------------------------------------

    function setDepositsFrozen(bool frozen) external onlyFreezeAuthority {
        depositsFrozenUntil = _freezeUntil(frozen);
        emit DepositsFrozenSet(frozen, depositsFrozenUntil);
    }

    function setReleasesFrozen(bool frozen) external onlyFreezeAuthority {
        releasesFrozenUntil = _freezeUntil(frozen);
        emit ReleasesFrozenSet(frozen, releasesFrozenUntil);
    }

    /// @notice Stops new candidates. Candidates already submitted can still be finalised (spec §4.3).
    function setCandidatesFrozen(bool frozen) external onlyFreezeAuthority {
        candidatesFrozenUntil = _freezeUntil(frozen);
        emit CandidatesFrozenSet(frozen, candidatesFrozenUntil);
    }

    function setTokenDepositsFrozen(address token, bool frozen) external onlyFreezeAuthority {
        _requireRegistered(token);
        tokenDepositsFrozenUntil[token] = _freezeUntil(frozen);
        emit TokenDepositsFrozenSet(token, frozen, tokenDepositsFrozenUntil[token]);
    }

    function setTokenReleasesFrozen(address token, bool frozen) external onlyFreezeAuthority {
        _requireRegistered(token);
        tokenReleasesFrozenUntil[token] = _freezeUntil(frozen);
        emit TokenReleasesFrozenSet(token, frozen, tokenReleasesFrozenUntil[token]);
    }

    /// @notice Stops deposits forever (spec §4.5). It cannot touch escrow, checkpoints or releases.
    function retire() external onlyFreezeAuthority {
        if (retired) revert AlreadyRetired();
        retired = true;
        emit Retired();
    }

    /// @notice Reject-only revocation (spec §4.4): checkpoint `index` and every later one stop counting, and
    ///         `finalState` returns to the state `index` was built on. Allowed within `revokeWindow` of `index`'s
    ///         finalisation. Used nullifiers stay used; nothing is accepted by this call. Pending candidates are
    ///         invalidated too (new epoch), so the restored state needs a fresh candidate and a full `delta` wait.
    /// @dev The loop is bounded: checkpoints after `index` were finalised after it, within the window, and two
    ///      finalisations are at least `deltaFloor` apart, so at most revokeWindow / deltaFloor + 1 iterations.
    function revokeFrom(uint256 index) external onlyFreezeAuthority {
        if (index >= checkpoints.length) revert UnknownCheckpoint();
        FinalizedCheckpoint storage first = checkpoints[index];
        if (first.revoked) revert CheckpointAlreadyRevoked();
        if (block.timestamp > first.finalizedAt + revokeWindow) revert RevokeWindowPassed();

        uint256 end = checkpoints.length;
        for (uint256 i = index; i < end; i++) {
            checkpoints[i].revoked = true;
        }
        finalState = first.parentState;
        stateEpoch += 1;
        if (sunsetLeafCountSet && sunsetSetAt >= index) {
            sunsetLeafCountSet = false;
            sunsetLeafCount = 0;
            sunsetSetAt = 0;
            emit SunsetLeafCountCleared();
        }
        emit CheckpointsRevoked(index, end - 1, first.parentState);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Two-step role transfers. Each role is handed over only by its current holder, and no address may hold both.
    // Proposing address(0) cancels a pending transfer.
    // ------------------------------------------------------------------------------------------------------------

    function transferAdmin(address newAdmin) external onlyAdmin {
        pendingAdmin = newAdmin;
        emit AdminTransferStarted(admin, newAdmin);
    }

    function acceptAdmin() external {
        if (msg.sender != pendingAdmin) revert NotPendingAdmin();
        if (msg.sender == freezeAuthority) revert RolesMustDiffer();
        emit AdminTransferred(admin, msg.sender);
        admin = msg.sender;
        pendingAdmin = address(0);
    }

    function transferFreezeAuthority(address newAuthority) external onlyFreezeAuthority {
        pendingFreezeAuthority = newAuthority;
        emit FreezeAuthorityTransferStarted(freezeAuthority, newAuthority);
    }

    function acceptFreezeAuthority() external {
        if (msg.sender != pendingFreezeAuthority) revert NotPendingFreezeAuthority();
        if (msg.sender == admin) revert RolesMustDiffer();
        emit FreezeAuthorityTransferred(freezeAuthority, msg.sender);
        freezeAuthority = msg.sender;
        pendingFreezeAuthority = address(0);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Views
    // ------------------------------------------------------------------------------------------------------------

    function tokenConfig(address token)
        external
        view
        returns (
            bool registered,
            bool depositsEnabled,
            uint256 lezUnit,
            uint256 unitWeight,
            uint256[] memory denominations_
        )
    {
        TokenConfig storage cfg = _tokens[token];
        return (cfg.registered, cfg.depositsEnabled, cfg.lezUnit, cfg.unitWeight, cfg.denominations);
    }

    function denominations(address token) external view returns (uint256[] memory) {
        return _tokens[token].denominations;
    }

    /// @notice The registered relay fees, one per denomination (write-once).
    function relayFees(address token) external view returns (uint256[] memory) {
        return _tokens[token].relayFees;
    }

    function depositsFrozen() external view returns (bool) {
        return _frozen(depositsFrozenUntil);
    }

    function releasesFrozen() external view returns (bool) {
        return _frozen(releasesFrozenUntil);
    }

    function candidatesFrozen() external view returns (bool) {
        return _frozen(candidatesFrozenUntil);
    }

    function tokenDepositsFrozen(address token) external view returns (bool) {
        return _frozen(tokenDepositsFrozenUntil[token]);
    }

    function tokenReleasesFrozen(address token) external view returns (bool) {
        return _frozen(tokenReleasesFrozenUntil[token]);
    }

    /// @notice The release delay that applies to roots finalised from now on.
    function releaseDelay(address token) external view returns (uint256) {
        return _releaseDelayFor(token, checkpoints.length);
    }

    /// @notice The release delay that applies to a root finalised as checkpoint `checkpointIndex`.
    function releaseDelayFor(address token, uint256 checkpointIndex) external view returns (uint256) {
        return _releaseDelayFor(token, checkpointIndex);
    }

    function beaconCount() external view returns (uint256) {
        return beacons.length;
    }

    function checkpointCount() external view returns (uint256) {
        return checkpoints.length;
    }

    /// @notice Whether `burnRoot` is accepted (finalised and not revoked), and when it was finalised.
    function burnRootStatus(bytes32 burnRoot) external view returns (bool accepted, uint256 finalizedAt) {
        uint256 entry = burnRootCheckpoint[burnRoot];
        if (entry == 0 || checkpoints[entry - 1].revoked) return (false, 0);
        return (true, checkpoints[entry - 1].finalizedAt);
    }

    /// @return capacity Bucket capacity (UNLIMITED when disabled).
    /// @return refillPerSecond Refill rate.
    /// @return available Amount that could be consumed now (UNLIMITED when disabled).
    function tokenCap(address token, Direction direction)
        external
        view
        returns (uint256 capacity, uint256 refillPerSecond, uint256 available)
    {
        Bucket storage b = _tokenBucket(token, direction);
        return (b.capacity, b.refillPerSecond, _available(b));
    }

    /// @return capacity Bucket capacity in value units (UNLIMITED when disabled).
    /// @return refillPerSecond Refill rate in value units per second.
    /// @return available Value units that could be consumed now (UNLIMITED when disabled).
    function globalCap(Direction direction)
        external
        view
        returns (uint256 capacity, uint256 refillPerSecond, uint256 available)
    {
        Bucket storage b = _globalBucket(direction);
        return (b.capacity, b.refillPerSecond, _available(b));
    }

    /// @notice The deposit-tree leaf: sha256(0x00 || LEAF_TAG || uint64 leafIndex || depositor || token ||
    ///         uint256 amount || commitment), 145 bytes of preimage.
    function leafHash(uint64 leafIndex, address depositor, address token, uint256 amount, bytes32 commitment)
        public
        pure
        returns (bytes32)
    {
        return sha256(abi.encodePacked(LEAF_PREFIX, LEAF_TAG, leafIndex, depositor, token, amount, commitment));
    }

    // ------------------------------------------------------------------------------------------------------------
    // Internal: deposits and the tree
    // ------------------------------------------------------------------------------------------------------------

    function _checkDeposit(address token, uint8 denomIndex, bytes32 commitment, uint64 index)
        internal
        view
        returns (uint256 amount)
    {
        if (retired) revert VaultRetired();
        if (_frozen(depositsFrozenUntil)) revert DepositsAreFrozen();
        TokenConfig storage cfg = _tokens[token];
        if (!cfg.registered) revert TokenNotRegistered();
        if (_frozen(tokenDepositsFrozenUntil[token])) revert TokenDepositsAreFrozen();
        if (!cfg.depositsEnabled) revert DepositsDisabled();
        if (denomIndex >= cfg.denominations.length) revert InvalidDenominationIndex();
        if (commitment == bytes32(0)) revert ZeroCommitment();
        if (index != depositCount[msg.sender]) revert WrongDepositIndex();
        if (nextLeafIndex >= MAX_LEAVES) revert TreeFull();
        return cfg.denominations[denomIndex];
    }

    function _recordDeposit(address token, uint256 amount, bytes32 commitment, uint64 index) internal {
        _consumeDepositCaps(token, amount);
        depositCount[msg.sender] = index + 1;
        escrowed[token] += amount;

        uint64 leafIndex = nextLeafIndex;
        _insertLeaf(leafIndex, leafHash(leafIndex, msg.sender, token, amount, commitment));
        emit Deposit(msg.sender, index, leafIndex, token, amount, commitment);
    }

    /// @dev Appends `leaf` at `leafIndex` (== nextLeafIndex) and recomputes `depositRoot` along its path.
    ///      At height h the path bit `(leafIndex >> h) & 1` says which side the running node is on:
    ///        bit 1: the left sibling is the completed subtree `_branch[h]`;
    ///        bit 0: the right sibling is still empty, i.e. the empty subtree of height h.
    ///      At the lowest 0 bit the running node is a subtree the new leaf has just completed, so it is saved
    ///      as `_branch[h]` (exactly one branch write per insertion; none when the tree becomes full).
    function _insertLeaf(uint64 leafIndex, bytes32 leaf) internal {
        bytes32 node = leaf;
        bytes32 emptySubtree = bytes32(0);
        bool branchSaved = false;
        for (uint256 h = 0; h < TREE_DEPTH; h++) {
            if (((leafIndex >> h) & 1) == 1) {
                node = _hashNode(_branch[h], node);
            } else {
                if (!branchSaved) {
                    _branch[h] = node;
                    branchSaved = true;
                }
                node = _hashNode(node, emptySubtree);
            }
            emptySubtree = _hashNode(emptySubtree, emptySubtree);
        }
        depositRoot = node;
        nextLeafIndex = leafIndex + 1;
    }

    /// @dev Folds a Merkle path: at height h the running node is the right child where bit h of `leafIndex` is 1.
    function _rootFromPath(uint64 leafIndex, bytes32 leaf, bytes32[TREE_DEPTH] calldata path)
        internal
        pure
        returns (bytes32 node)
    {
        node = leaf;
        for (uint256 h = 0; h < TREE_DEPTH; h++) {
            node = ((leafIndex >> h) & 1) == 1 ? _hashNode(path[h], node) : _hashNode(node, path[h]);
        }
    }

    /// @dev Inner node: sha256(0x01 || left || right). The empty leaf is bytes32(0).
    function _hashNode(bytes32 left, bytes32 right) internal pure returns (bytes32) {
        return sha256(abi.encodePacked(NODE_PREFIX, left, right));
    }

    // ------------------------------------------------------------------------------------------------------------
    // Internal: checkpoints
    // ------------------------------------------------------------------------------------------------------------

    function _recordBeacon() internal returns (uint256 index) {
        index = beacons.length;
        if (index != 0 && _lastBeaconBlock == block.number) return index - 1;
        _lastBeaconBlock = block.number;
        // The expression spec §4.3 states; kept readable rather than hand-optimised.
        // forge-lint: disable-next-line(asm-keccak256)
        bytes32 nonce = keccak256(abi.encode(block.prevrandao, block.number, address(this)));
        beacons.push(Beacon({timestamp: block.timestamp, nonce: nonce}));
        emit BeaconRecorded(index, block.timestamp, nonce);
    }

    /// @dev sha256(abi.encode(CHECKPOINT_DOMAIN, chainid, vault, lezConfigId, nMin, tMin, parentState, newState,
    ///      burnRoot, burnCount, importedLeafCount, sunset, echoNonce)): 13 words. Encoded as two halves, which for
    ///      static types is the same byte string as one abi.encode.
    function _checkpointDigest(Checkpoint calldata c) internal view returns (bytes32) {
        bytes memory head = abi.encode(CHECKPOINT_DOMAIN, block.chainid, address(this), lezConfigId, c.nMin, c.tMin);
        bytes memory body =
            abi.encode(c.parentState, c.newState, c.burnRoot, c.burnCount, c.importedLeafCount, c.sunset, c.echoNonce);
        return sha256(bytes.concat(head, body));
    }

    // ------------------------------------------------------------------------------------------------------------
    // Internal: releases and journals
    // ------------------------------------------------------------------------------------------------------------

    function _checkRelease(ReleaseRequest calldata r) internal view returns (uint256 amount) {
        if (_frozen(releasesFrozenUntil)) revert ReleasesAreFrozen();
        TokenConfig storage cfg = _tokens[r.token];
        if (!cfg.registered) revert TokenNotRegistered();
        if (_frozen(tokenReleasesFrozenUntil[r.token])) revert TokenReleasesAreFrozen();
        if (r.denomIndex >= cfg.denominations.length) revert InvalidDenominationIndex();
        amount = cfg.denominations[r.denomIndex];
        if (r.recipient == address(0)) revert ZeroRecipient();

        uint256 entry = burnRootCheckpoint[r.burnRoot];
        if (entry == 0 || checkpoints[entry - 1].revoked) revert BurnRootNotAccepted();
        uint256 delay = _releaseDelayFor(r.token, entry - 1);
        if (block.timestamp < checkpoints[entry - 1].finalizedAt + delay) revert ReleaseDelayNotElapsed();
        if (nullifierUsed[r.nullifier]) revert NullifierAlreadyUsed();
        // A free-form fee would fingerprint the user: only 0 or the registered fee is accepted.
        if (r.relayFee != 0 && r.relayFee != cfg.relayFees[r.denomIndex]) revert InvalidRelayFee();
        if (amount > escrowed[r.token]) revert InsufficientEscrow();
    }

    /// @dev sha256(abi.encode(RELEASE_DOMAIN, chainid, vault, lezConfigId, burnRoot, NOut, token, amount,
    ///      recipient, relayer, relayFee)) (spec §5.2).
    function _releaseJournalDigest(ReleaseRequest calldata r, uint256 amount) internal view returns (bytes32) {
        return sha256(
            abi.encode(
                RELEASE_DOMAIN,
                block.chainid,
                address(this),
                lezConfigId,
                r.burnRoot,
                r.nullifier,
                r.token,
                amount,
                r.recipient,
                r.relayer,
                r.relayFee
            )
        );
    }

    /// @dev The delay in force for roots finalised as checkpoint `checkpointIndex`: the last setting whose
    ///      `fromCheckpoint` is at most that index. Settings are appended in non-decreasing `fromCheckpoint` order,
    ///      so a binary search finds how many of them precede the checkpoint.
    function _releaseDelayFor(address token, uint256 checkpointIndex) internal view returns (uint256) {
        DelayChange[] storage changes = _releaseDelays[token];
        uint256 low = 0;
        uint256 high = changes.length;
        while (low < high) {
            uint256 mid = (low + high) / 2;
            if (changes[mid].fromCheckpoint <= checkpointIndex) low = mid + 1;
            else high = mid;
        }
        return low == 0 ? 0 : changes[low - 1].delay;
    }

    /// @dev Pays `amount` of `token` to `payee`, or, if the transfer fails, credits it to `owed[payee][token]`
    ///      (spec §5.4), so a payee that cannot receive never blocks or loses the payment and nobody can redirect it.
    function _payOrCredit(address token, address payee, uint256 amount) internal {
        bool paid = token == ETH ? _trySendEth(payee, amount) : IERC20(token).trySafeTransfer(payee, amount);
        if (!paid) {
            owed[payee][token] += amount;
            totalOwed[token] += amount;
            emit OwedCredited(payee, token, amount);
        }
    }

    /// @dev Sends ETH without copying return data, so a payee cannot exhaust the vault's gas by returning or
    ///      reverting with a large buffer. All gas is forwarded; the 1/64 the vault keeps pays for the fallback,
    ///      so a payee that burns its gas only forces the submitter to send a larger gas limit.
    function _trySendEth(address to, uint256 amount) internal returns (bool ok) {
        assembly ("memory-safe") {
            ok := call(gas(), to, amount, 0, 0, 0, 0)
        }
    }

    // ------------------------------------------------------------------------------------------------------------
    // Internal: rate limits
    // ------------------------------------------------------------------------------------------------------------

    function _consumeDepositCaps(address token, uint256 amount) internal {
        if (!_tryConsume(_tokenDepositCap[token], amount)) revert TokenDepositCapExceeded();
        if (!_tryConsume(_globalDepositCap, _valueUnits(token, amount))) revert GlobalDepositCapExceeded();
    }

    function _consumeReleaseCaps(address token, uint256 amount) internal {
        if (!_tryConsume(_tokenReleaseCap[token], amount)) revert TokenReleaseCapExceeded();
        if (!_tryConsume(_globalReleaseCap, _valueUnits(token, amount))) revert GlobalReleaseCapExceeded();
    }

    /// @dev Value units of `amount`: (amount / lezUnit) * unitWeight. The division is exact because every
    ///      denomination is a multiple of lezUnit. The product is formed first and cannot overflow, because
    ///      amount <= MAX_DENOMINATION and unitWeight <= MAX_UNIT_WEIGHT, both 2**128 - 1.
    function _valueUnits(address token, uint256 amount) internal view returns (uint256) {
        TokenConfig storage cfg = _tokens[token];
        return amount * cfg.unitWeight / cfg.lezUnit;
    }

    /// @dev Consumes `amount` if available. Returns false, changing nothing, if it is not.
    function _tryConsume(Bucket storage b, uint256 amount) internal returns (bool) {
        if (b.capacity == UNLIMITED) return true;
        uint256 level = _currentLevel(b);
        if (amount > level) return false;
        b.level = level - amount;
        b.lastUpdate = block.timestamp;
        return true;
    }

    /// @dev Stored level plus the refill since `lastUpdate`, saturating at capacity without overflowing:
    ///      if elapsed > missing / rate then elapsed * rate > missing, so the bucket is full; otherwise
    ///      elapsed * rate <= missing and the sum stays <= capacity.
    function _currentLevel(Bucket storage b) internal view returns (uint256) {
        uint256 elapsed = block.timestamp - b.lastUpdate;
        uint256 missing = b.capacity - b.level;
        if (b.refillPerSecond != 0 && elapsed > missing / b.refillPerSecond) return b.capacity;
        return b.level + elapsed * b.refillPerSecond;
    }

    function _available(Bucket storage b) internal view returns (uint256) {
        return b.capacity == UNLIMITED ? UNLIMITED : _currentLevel(b);
    }

    /// @dev A newly created bucket starts full.
    function _initBucket(Bucket storage b, uint256 capacity, uint256 refillPerSecond) internal {
        b.capacity = capacity;
        b.refillPerSecond = refillPerSecond;
        b.level = capacity;
        b.lastUpdate = block.timestamp;
    }

    /// @dev Settles the refill earned under the old parameters, then clamps to the new capacity, so lowering a cap
    ///      takes effect at once and raising one adds no instant headroom. An UNLIMITED bucket has no tracked
    ///      level and is treated as full.
    function _updateBucket(Bucket storage b, uint256 capacity, uint256 refillPerSecond) internal {
        uint256 level = b.capacity == UNLIMITED ? capacity : _currentLevel(b);
        if (level > capacity) level = capacity;
        b.capacity = capacity;
        b.refillPerSecond = refillPerSecond;
        b.level = level;
        b.lastUpdate = block.timestamp;
    }

    function _tokenBucket(address token, Direction direction) internal view returns (Bucket storage) {
        return direction == Direction.Deposit ? _tokenDepositCap[token] : _tokenReleaseCap[token];
    }

    function _globalBucket(Direction direction) internal view returns (Bucket storage) {
        return direction == Direction.Deposit ? _globalDepositCap : _globalReleaseCap;
    }

    // ------------------------------------------------------------------------------------------------------------
    // Internal: access control and registry validation
    // ------------------------------------------------------------------------------------------------------------

    function _freezeUntil(bool frozen) internal view returns (uint256) {
        return frozen ? block.timestamp + maxFreeze : 0;
    }

    function _frozen(uint256 until) internal view returns (bool) {
        return block.timestamp < until;
    }

    function _checkAdmin() internal view {
        if (msg.sender != admin) revert NotAdmin();
    }

    function _checkFreezeAuthority() internal view {
        if (msg.sender != freezeAuthority) revert NotFreezeAuthority();
    }

    function _requireRegistered(address token) internal view {
        if (!_tokens[token].registered) revert TokenNotRegistered();
    }

    /// @dev lezUnit must be 10**k for some k >= 0.
    function _checkLezUnit(uint256 lezUnit) internal pure {
        if (lezUnit == 0) revert InvalidLezUnit();
        uint256 rest = lezUnit;
        while (rest % 10 == 0) {
            rest /= 10;
        }
        if (rest != 1) revert InvalidLezUnit();
    }

    function _checkUnitWeight(uint256 unitWeight) internal pure {
        if (unitWeight == 0 || unitWeight > MAX_UNIT_WEIGHT) revert InvalidUnitWeight();
    }

    /// @dev One fee per denomination, each at most 1% of it (fee <= denomination / 100, i.e. fee * 100 <= it).
    function _checkRelayFees(uint256[] calldata fees, uint256[] calldata denoms) internal pure {
        if (fees.length != denoms.length) revert RelayFeeCountMismatch();
        for (uint256 i = 0; i < fees.length; i++) {
            if (fees[i] > denoms[i] / 100) revert RelayFeeTooHigh();
        }
    }

    /// @dev 1..8 entries, strictly increasing (so all non-zero), each <= MAX_DENOMINATION and a multiple of lezUnit.
    function _checkDenominations(uint256[] calldata denoms, uint256 lezUnit) internal pure {
        if (denoms.length == 0 || denoms.length > MAX_DENOMINATIONS) revert InvalidDenominations();
        uint256 previous = 0;
        for (uint256 i = 0; i < denoms.length; i++) {
            if (denoms[i] <= previous || denoms[i] > MAX_DENOMINATION) revert InvalidDenominations();
            if (denoms[i] % lezUnit != 0) revert DenominationNotMultipleOfLezUnit();
            previous = denoms[i];
        }
    }
}
