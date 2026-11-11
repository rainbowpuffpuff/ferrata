// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ReentrancyGuardTransient} from "@openzeppelin/contracts/utils/ReentrancyGuardTransient.sol";

import {FerrataRegistry} from "./FerrataRegistry.sol";
import {IEmergencyStop} from "./interfaces/IEmergencyStop.sol";
import {IRiscZeroVerifier} from "./interfaces/IRiscZeroVerifier.sol";

/// @title FerrataVault (v5)
/// @notice Ethereum half of a privacy-preserving lock-and-mint bridge to LEZ (Logos RFP-022). Prototype, not audited.
///         Implements notes/design/SPEC-v2.md as amended by notes/design/SPEC-v3-delta.md: D3-D5, D9, and the vault
///         items of D11 and D13 including fees, with both finality rules behind an immutable `mode`.
/// @dev One vault per bridge configuration. Deposits of fixed denominations are appended to an incremental SHA-256
///      Merkle tree whose root is kept in storage slot 0, which the LEZ side proves with an Ethereum storage proof.
///      LEZ burn roots become accepted only through finalised checkpoints. Rule (A): a candidate, a beacon at least
///      its wait W = delta + (t - 1) * turnSeconds later, and a second proof that echoes the beacon. Rule (B): one
///      proof of t final-tier attestations, final once W = delta has passed unless a watcher cancels it. Releases
///      pay out against a RISC Zero proof of a burn under an accepted root.
///      Fees (D13): a deposit escrows its denomination plus the mint and relay fees of the fee schedule in force; a
///      release pays the denomination less the relay, burn and checkpoint fees in force. Fee schedules and the registry
///      entry are published at SHA-256 storage slots that the LEZ side attests.
///      There is no upgradeability, no settable verifier and no token rescue function. The only delegatecall is into
///      the linked library FerrataRegistry (registration, fees, caps, roles), whose address is fixed at link time.
contract FerrataVault is ReentrancyGuardTransient {
    using SafeERC20 for IERC20;

    // ------------------------------------------------------------------------------------------------------------
    // Types
    // ------------------------------------------------------------------------------------------------------------

    enum Direction {
        Deposit,
        Release
    }

    /// @notice The finality rule, fixed at deployment (v3 D13). A: candidate, beacon, a second proof that echoes it.
    ///         B: one proof of final-tier attestations, final after the wait unless cancelled.
    enum Mode {
        A,
        B
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
        uint256 turnSeconds; // the channel's posting timeframe; each signer beyond the first adds a turn (v3 D4); 0 in B
        uint256 minReleaseDelay; // every release delay is at least this and at most `revokeWindow` (v3 D5)
        address admin;
        address freezeAuthority;
        CapParams globalDepositCap; // value units; starts full
        CapParams globalReleaseCap; // value units; starts full
        Mode mode; // the finality rule; checkpointImageId must be that rule's program (deployment check)
        address[3] watchers; // may cancel candidates and pause checkpoints for 12 h (D13); zero entries are unused
        CapParams releaseCapFloor; // value units; no release bucket can be set below it (D13); zeros: no floor
        address feeRecipient; // credited every burn fee (D13)
    }

    /// @notice The checkpoint-journal fields the caller supplies, in journal order. The vault adds
    ///         CHECKPOINT_DOMAIN, the chain id, its own address and `lezConfigId` in front (v3 D3: 17 words).
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
        uint64 tipId; // the LEZ block T the checkpoint was cut at
        bytes32 tipHash;
        uint32 n; // size of the key set in newState
        uint32 t; // its signing threshold
    }

    /// @dev A submitted, not yet finalised checkpoint, keyed by its `newState`. It is valid only while `finalState` has
    ///      not changed and nothing was revoked since it was submitted (`epoch` unchanged), i.e. while its parent is
    ///      still `finalState`; a stale one no longer blocks its `newState` (review L-1). Finalisation re-presents the journal and must
    ///      match `journalDigest` word for word, so the other journal fields need not be stored.
    struct Candidate {
        bytes32 journalDigest; // sha256 of the 17-word journal the candidate's proof was verified against
        bytes32 tipHash; // the candidate's tip, reported if it is cancelled
        uint64 tipId;
        uint32 t; // from the candidate's journal; sizes its wait (v3 D4)
        uint48 submittedAt; // 0 means no candidate
        uint32 deltaAtSubmit; // delta <= MAX_DELTA < 2**32
        uint64 epoch; // _epoch() at submission
        address prover; // sent the candidate with its verified proof; credited the checkpoint fee once it is final
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
        uint64 parentImportedLeafCount; // of the state it was built on, restored if it is revoked
        bool parentSunset;
        uint64 finalizedAt;
        address prover; // submitted its candidate; credited the checkpoint fee of every release against it (D13)
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

    /// @notice A registration. Everything here except the caps, the release delay and the checkpoint fees (fee schedule
    ///         0, changed later by announcement) is write-once.
    struct TokenParams {
        uint256[] denominations; // 1..8, strictly increasing, non-zero, each a multiple of lezUnit
        uint256[] relayFees; // one per denomination, each at most 1% of it and a multiple of lezUnit
        uint256 lezUnit; // a power of ten, at most 10**decimals: base units per LEZ unit of the wrapped token
        uint256 unitWeight; // global-cap value units per lezUnit
        CapParams depositCap;
        CapParams releaseCap;
        uint256 releaseDelay; // initial release delay; adjustable later, always within [minReleaseDelay, revokeWindow]
        bytes32 lezDefinition; // the wrapped token's definition account on LEZ (published in the registry entry)
        uint8 decimals; // the token's decimals on Ethereum (published in the registry entry)
        uint256[] checkpointFees; // fee schedule 0, one per denomination, each at most 1% of it (D13)
    }

    /// @dev A token's fee schedule `index` (D13). Fees are per denomination, packed two to a word (each is at most 1%
    ///      of a denomination <= 2**128 - 1, so below 2**128): `inbound` = mintFee << 128 | relayFee (the relay fee is
    ///      the registered one, copied), `outbound` = burnFee << 128 | checkpointFee. Pending: `effectiveAt` is the
    ///      earliest time it takes effect; in force: `fromLeaf` and `effectiveAt` record when it did.
    struct FeeSchedule {
        uint64 fromLeaf; // the first leaf index it applies to
        uint64 announcedAt;
        uint64 effectiveAt;
        uint256[8] inbound;
        uint256[8] outbound;
    }

    /// @dev Per token: the index of the schedule in force, and when the announced schedule `current + 1` takes
    ///      effect (0: none announced).
    struct FeeState {
        uint64 current;
        uint64 pendingAt;
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

    /// @dev A role change the admin proposed (D13): a watcher set and, unless address(0), a new freeze authority.
    ///      Anyone applies it from `effectiveAt` on (0: nothing pending).
    struct RoleProposal {
        address[3] watchers;
        address freezeAuthority;
        uint256 effectiveAt;
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
    uint256 internal constant DEPOSIT_ROOT_SLOT = 0;
    /// @notice Storage slot of `nextLeafIndex`; nothing else is packed into it.
    uint256 internal constant NEXT_LEAF_INDEX_SLOT = 1;
    /// @notice Storage slot of `storedLezConfigId`, which the LEZ configuration's `Init` attests (reciprocal pairing).
    uint256 internal constant LEZ_CONFIG_ID_SLOT = 2;

    uint256 internal constant TREE_DEPTH = 32;
    uint256 internal constant MAX_LEAVES = 2 ** TREE_DEPTH;
    // The registry's bounds (at most 8 denominations, each and every unitWeight at most 2**128 - 1, which keeps the
    // cap arithmetic `amount * unitWeight / lezUnit` from overflowing) are constants of FerrataRegistry.
    uint256 internal constant MAX_RELEASE_DELAY = 30 days;
    /// @notice Ceiling for `delta` (v3 D4): without it a raise-only delta could be pushed out of reach for good. Also
    ///         bounds `turnSeconds`.
    uint256 internal constant MAX_DELTA = 30 days;
    /// @notice Delay between the admin's proposal and a change of the watchers or of the freeze authority (D13).
    uint256 internal constant ROLE_TIMELOCK = 14 days;
    /// @notice The longest checkpoint pause a watcher can set (D13).
    uint256 internal constant WATCHER_PAUSE = 12 hours;
    /// @notice After this long without checkpoint progress anyone may retire the vault (D13); see `retireOpensAt`.
    uint256 internal constant KEYLESS_RETIREMENT = 30 days;
    /// @notice `revokeWindow / deltaFloor` is at most this, which bounds the loop of `revokeFrom` (two finalisations
    ///         are at least `deltaFloor` apart): at most 1,001 checkpoints marked, about 5 M gas.
    uint256 internal constant MAX_REVOKE_SPAN = 1000;
    uint256 internal constant UNLIMITED = type(uint256).max;
    /// @notice Sentinel token address for native ETH.
    address internal constant ETH = address(0);

    bytes1 internal constant LEAF_PREFIX = 0x00;
    bytes1 internal constant NODE_PREFIX = 0x01;
    /// @notice sha256("ferrata/vault/leaf/v2")
    bytes32 internal constant LEAF_TAG = 0x17b9508a0d0cbf61207c0fcb66f06ed92a6aeec110a54860479f0bf3cd6aff0b;
    /// @notice sha256("ferrata/vault/checkpoint/v3")
    bytes32 internal constant CHECKPOINT_DOMAIN = 0xbdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7;
    /// @notice sha256("ferrata/vault/release/v2")
    bytes32 internal constant RELEASE_DOMAIN = 0x6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e51;

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
    uint256 public immutable turnSeconds;
    uint256 public immutable minReleaseDelay;
    Mode public immutable mode;
    /// @notice Floor under every release bucket, in value units (D13): its capacity and its refill per second.
    uint256 public immutable releaseCapFloor;
    uint256 public immutable releaseRefillFloor;
    /// @notice Credited the burn fee of every release (D13).
    address public immutable feeRecipient;
    /// @dev (tMin - 1) * turnSeconds + KEYLESS_RETIREMENT: the keyless period after `delta` (see `retireOpensAt`).
    uint256 internal immutable _keylessSpan;

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
    /// @notice Set once by `retire()`; deposits revert forever afterwards.
    bool public retired;
    /// @notice Freezes are stored as the time they lapse (0: not frozen); see `depositsFrozen()` etc.
    uint256 public depositsFrozenUntil;
    uint256 public releasesFrozenUntil;
    uint256 public checkpointsFrozenUntil;

    /// @notice The latest finalised LEZ checkpoint state (starts at `genesisState`).
    bytes32 public finalState;
    /// @notice The base of a candidate's wait W = delta + (t - 1) * turnSeconds; a change applies to candidates
    ///         submitted later.
    uint256 public delta;
    /// @notice Incremented by every revocation. A candidate submitted before a revocation can no longer be finalised,
    ///         so the state a revocation restores needs a fresh candidate and a full wait (spec §4.4).
    uint256 public revocationEpoch;
    uint256 private _lastBeaconBlock;
    Beacon[] public beacons;
    mapping(bytes32 newState => Candidate) public candidates;
    FinalizedCheckpoint[] public checkpoints;
    /// @notice For an accepted burn root: 1 + the index of the checkpoint that made it accepted; 0 if none.
    mapping(bytes32 burnRoot => uint256) public burnRootCheckpoint;
    bool public sunsetLeafCountSet;
    /// @notice Leaves with an index at or above this were never imported on LEZ and are refundable (spec §4.5).
    uint64 public sunsetLeafCount;
    /// @notice Index of the checkpoint that set `sunsetLeafCount`: the first non-revoked sunset checkpoint.
    uint256 public sunsetSetAt;
    /// @notice When that checkpoint was finalised; refunds open `revokeWindow` later (v3 D5).
    uint256 public sunsetFinalizedAt;
    /// @notice `importedLeafCount` and `sunset` of `finalState` (0 and false at genesis), for the monotonicity
    ///         checks of v3 D4.
    uint64 public finalImportedLeafCount;
    bool public finalSunset;
    /// @notice Start of the keyless-retirement clock (D13): the deployment, the last candidate submission or
    ///         finalisation, or the end of a checkpoint freeze that was lifted early; see `retireOpensAt`. Shares a slot
    ///         with the two fields above, which finalisation writes too.
    uint64 public keylessFrom;
    /// @dev How many finalisations changed `finalState` (a no-op checkpoint, newState == parentState, does not); with
    ///      `revocationEpoch` it dates candidates (`_epoch`). In the same slot.
    uint64 internal _stateChanges;

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

    /// @notice Up to three watchers (D13); zero entries are unused.
    address[3] public watchers;
    /// @notice The pending role proposal (D13); `effectiveAt` 0 means none. The getter returns the proposed freeze
    ///         authority and `effectiveAt`; `RolesProposed` carries the proposed watchers.
    RoleProposal public roleProposal;

    /// @notice Per token: the fee schedule in force and the announced one's start (D13).
    mapping(address token => FeeState) public feeState;
    /// @dev Per token, every fee schedule by index (`fees` shows the fees). Schedule 0 is the registration's. Each
    ///      schedule in force is published, hashed, at its SHA-256 slot; `FeesAnnounced` and `FeesInForce` carry the
    ///      preimage.
    mapping(address token => mapping(uint256 index => FeeSchedule)) internal feeSchedules;

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
        uint256 deltaAtSubmit,
        uint64 tipId,
        bytes32 tipHash,
        uint32 n,
        uint32 t
    );
    event CheckpointFinalized(
        uint256 indexed index,
        bytes32 indexed newState,
        bytes32 parentState,
        bytes32 burnRoot,
        uint64 burnCount,
        uint64 importedLeafCount,
        bool sunset,
        uint64 tipId,
        bytes32 tipHash,
        uint32 n,
        uint32 t
    );
    event CandidateCancelled(bytes32 indexed newState, uint64 tipId, bytes32 tipHash);
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
    event FreezeAuthorityTransferred(address indexed previousAuthority, address indexed newAuthority);
    event DepositsFrozenSet(bool frozen, uint256 until);
    event ReleasesFrozenSet(bool frozen, uint256 until);
    event CheckpointsFrozenSet(bool frozen, uint256 until);
    event TokenDepositsFrozenSet(address indexed token, bool frozen, uint256 until);
    event TokenReleasesFrozenSet(address indexed token, bool frozen, uint256 until);
    event CheckpointsPaused(address indexed watcher, uint256 until);
    event RolesProposed(address[3] watchers, address indexed freezeAuthority, uint256 effectiveAt);
    event WatchersSet(address[3] watchers);
    /// @notice The registry entry published for the LEZ side, with the fields `TokenRegistered` does not carry.
    event RegistryEntryPublished(address indexed token, bytes32 lezDefinition, uint8 decimals, bytes32 entryHash);
    /// @notice Fee schedule `index` announced; it takes effect from `effectiveAt` (0: the announcement is withdrawn).
    event FeesAnnounced(
        address indexed token,
        uint256 indexed index,
        uint64 announcedAt,
        uint64 effectiveAt,
        uint256[] mintFees,
        uint256[] burnFees,
        uint256[] checkpointFees
    );
    /// @notice Fee schedule `index` in force from leaf `fromLeaf`; `scheduleHash` is published at its slot.
    event FeesInForce(
        address indexed token, uint256 indexed index, uint64 fromLeaf, uint64 effectiveAt, bytes32 scheduleHash
    );

    // ------------------------------------------------------------------------------------------------------------
    // Errors. None carries parameters, so no error echoes user data.
    // ------------------------------------------------------------------------------------------------------------

    error NotAdmin();
    error NotFreezeAuthority();
    error NotPendingAdmin();
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
    error InvalidTurnSeconds();
    error InvalidMinReleaseDelay();
    error VerifierNotStop();

    error TokenAlreadyRegistered();
    error TokenNotRegistered();
    error TokenHasNoCode();
    error InvalidDenominations();
    error InvalidLezUnit();
    error DenominationNotMultipleOfLezUnit();
    error InvalidUnitWeight();
    error RelayFeeTooHigh();
    error RelayFeeCountMismatch();
    error FeeTooHigh();
    error FeeCountMismatch();
    error FeeNotMultipleOfLezUnit();
    error ReleaseDelayOutOfRange();
    error CapBelowFloor();
    error DeltaCanOnlyRise();
    error DeltaTooLong();

    error VaultRetired();
    error AlreadyRetired();
    error RetireNotOpen();
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

    error CheckpointsAreFrozen();
    error WrongMode();
    error WaitNotOver();
    error ImportedLeafCountTooHigh();
    error VerifierStopped();
    error NotWatcher();
    error NoProposal();
    error TimelockNotOver();
    error ParentNotFinal();
    error EchoNonceNotZero();
    error WrongCommitteeFloor();
    error KeySetBelowFloor();
    error ImportedLeafCountDecreased();
    error SunsetIsFinal();
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

    error NotRetired();
    error NotRefundable();
    error RefundNotYetOpen();
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
        if (d.revokeWindow == 0 || d.revokeWindow / d.deltaFloor > MAX_REVOKE_SPAN) revert InvalidRevokeWindow();
        if (d.maxFreeze == 0) revert InvalidMaxFreeze();
        // Rule (A) waits (t - 1) turns more; rule (B) has no turn term, so its turnSeconds is 0 (the one wait formula).
        if (d.mode == Mode.A ? d.turnSeconds == 0 || d.turnSeconds > MAX_DELTA : d.turnSeconds != 0) {
            revert InvalidTurnSeconds();
        }
        if (d.minReleaseDelay > d.revokeWindow || d.minReleaseDelay > MAX_RELEASE_DELAY) {
            revert InvalidMinReleaseDelay();
        }
        if (d.admin == address(0) || d.freezeAuthority == address(0) || d.feeRecipient == address(0)) {
            revert ZeroAddress();
        }
        if (d.admin == d.freezeAuthority) revert RolesMustDiffer();
        // Refunds and rule (B) finalisations read the stop (D13): the verifier must be a stop that is not tripped.
        try IEmergencyStop(address(d.verifier)).paused() returns (bool stopped) {
            if (stopped) revert VerifierNotStop();
        } catch {
            revert VerifierNotStop();
        }
        FerrataRegistry.checkReleaseFloor(
            d.globalReleaseCap.capacity,
            d.globalReleaseCap.refillPerSecond,
            1,
            1,
            d.releaseCapFloor.capacity,
            d.releaseCapFloor.refillPerSecond
        );

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
        turnSeconds = d.turnSeconds;
        minReleaseDelay = d.minReleaseDelay;
        mode = d.mode;
        releaseCapFloor = d.releaseCapFloor.capacity;
        releaseRefillFloor = d.releaseCapFloor.refillPerSecond;
        feeRecipient = d.feeRecipient;
        _keylessSpan = (uint256(d.tMin) - 1) * d.turnSeconds + KEYLESS_RETIREMENT;

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
        watchers = d.watchers;
        keylessFrom = uint64(block.timestamp);
        FerrataRegistry.initBucket(_globalDepositCap, d.globalDepositCap.capacity, d.globalDepositCap.refillPerSecond);
        FerrataRegistry.initBucket(_globalReleaseCap, d.globalReleaseCap.capacity, d.globalReleaseCap.refillPerSecond);

        emit AdminTransferred(address(0), d.admin);
        emit FreezeAuthorityTransferred(address(0), d.freezeAuthority);
        emit DeltaSet(d.initialDelta);
        emit GlobalCapSet(Direction.Deposit, d.globalDepositCap.capacity, d.globalDepositCap.refillPerSecond);
        emit GlobalCapSet(Direction.Release, d.globalReleaseCap.capacity, d.globalReleaseCap.refillPerSecond);
        emit WatchersSet(d.watchers);
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

    /// @notice Escrows `denominations(token)[denomIndex]` of an ERC-20 plus the mint and relay fees of the fee schedule
    ///         in force (D13), and appends a leaf (which binds the denomination) to the deposit tree.
    /// @param commitment Hiding commitment to the LEZ destination. Opaque to the vault, non-zero, may repeat.
    /// @param index Must equal `depositCount[msg.sender]`, so every deposit has a unique (depositor, index) pair.
    function deposit(address token, uint8 denomIndex, bytes32 commitment, uint64 index) external nonReentrant {
        if (token == ETH) revert UseDepositETH();
        (uint256 amount, uint256 total) = _checkDeposit(token, denomIndex, commitment, index);

        // Deposits pull the tokens before changing any state other than a due fee change, so the vault never
        // books an escrow it has not received and its balance covers `escrowed` at every external call. The
        // exact-delta check rejects fee-on-transfer tokens. SafeERC20 accepts tokens whose transferFrom returns
        // nothing (USDT).
        IERC20 erc20 = IERC20(token);
        uint256 balanceBefore = erc20.balanceOf(address(this));
        erc20.safeTransferFrom(msg.sender, address(this), total);
        if (erc20.balanceOf(address(this)) != balanceBefore + total) revert TransferAmountMismatch();

        _recordDeposit(token, amount, total, commitment, index);
    }

    /// @notice Escrows native ETH; `msg.value` must equal `denominations(ETH)[denomIndex]` plus the mint and relay
    ///         fees in force, exactly.
    function depositETH(uint8 denomIndex, bytes32 commitment, uint64 index) external payable nonReentrant {
        (uint256 amount, uint256 total) = _checkDeposit(ETH, denomIndex, commitment, index);
        if (msg.value != total) revert WrongEthAmount();
        _recordDeposit(ETH, amount, total, commitment, index);
    }

    /// @notice Returns a deposit that LEZ never imported, once the configuration has been sunset (spec §3.4, §4.5).
    ///         Permissionless; pays the depositor named in the leaf. The leaf is proven against `depositRoot`, which
    ///         no longer moves because the vault must be retired, with a Merkle path whose direction bits are the
    ///         bits of `leafIndex`. Refunds open after the last second in which the sunset checkpoint can be revoked
    ///         (v3 D5, D11), the global release freeze stops them, they draw on the release buckets, and nothing is
    ///         refunded after the verifier stop (D13). A refund returns the leaf's whole escrow: the denomination and
    ///         the mint and relay fees of the fee schedule of its leaf index (D13).
    function refund(
        uint64 leafIndex,
        address depositor,
        address token,
        uint8 denomIndex,
        bytes32 commitment,
        bytes32[TREE_DEPTH] calldata path
    ) external nonReentrant {
        if (!retired) revert NotRetired();
        if (_frozen(releasesFrozenUntil)) revert ReleasesAreFrozen();
        if (_stopped()) revert VerifierStopped();
        if (!sunsetLeafCountSet || leafIndex < sunsetLeafCount) revert NotRefundable();
        if (block.timestamp < _refundsOpenAt()) revert RefundNotYetOpen();
        if (refunded[leafIndex]) revert AlreadyRefunded();
        // The leaf's checks (known leaf, registered token and denomination, Merkle path to `depositRoot`) and its
        // escrow, in the library.
        uint256 amount = FerrataRegistry.refundAmount(
            _tokens, feeState, feeSchedules, leafIndex, depositor, token, denomIndex, commitment, path
        );
        if (amount > escrowed[token]) revert InsufficientEscrow();

        _consumeReleaseCaps(token, amount);
        refunded[leafIndex] = true;
        escrowed[token] -= amount;
        emit Refund(leafIndex, depositor, token, amount);
        _payOrCredit(token, depositor, amount);
    }

    // ------------------------------------------------------------------------------------------------------------
    // LEZ checkpoints (spec §4.3): candidate, then beacon and finalisation (A) or the wait alone (B)
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Records a beacon: the block time and a nonce nobody could know before this block. Permissionless,
    ///         at most one per block (a second call in the same block returns the existing one). Rule (A) only.
    function recordBeacon() external returns (uint256 index) {
        if (mode != Mode.A) revert WrongMode();
        return _recordBeacon();
    }

    /// @notice Submits a candidate checkpoint built on `finalState`. Permissionless; starts its wait
    ///         W = delta + (t - 1) * turnSeconds (turnSeconds is 0 under rule (B)). The proof is of the deployment's
    ///         rule; under (A) the candidate also records a beacon. The sender is the candidate's prover, credited the
    ///         checkpoint fee if it is finalised; the submission restarts the keyless-retirement clock.
    function submitCandidate(Checkpoint calldata c, bytes calldata seal) external {
        if (_frozen(checkpointsFrozenUntil)) revert CheckpointsAreFrozen();
        if (c.parentState != finalState) revert ParentNotFinal();
        if (c.echoNonce != bytes32(0)) revert EchoNonceNotZero();
        _checkCommittee(c);
        uint256 epoch = _epoch();
        // Resubmitting a live candidate must not restart its wait; a stale one (built on an earlier finalState) is
        // replaced.
        Candidate storage existing = candidates[c.newState];
        if (existing.submittedAt != 0 && existing.epoch == epoch) revert CandidateAlreadySubmitted();
        bytes32 digest = _checkpointDigest(c, bytes32(0));
        verifier.verify(seal, checkpointImageId, digest);

        candidates[c.newState] = Candidate({
            journalDigest: digest,
            tipHash: c.tipHash,
            tipId: c.tipId,
            t: c.t,
            submittedAt: uint48(block.timestamp),
            deltaAtSubmit: uint32(delta),
            epoch: uint64(epoch),
            prover: msg.sender
        });
        keylessFrom = uint64(block.timestamp);
        if (mode == Mode.A) _recordBeacon();
        _emitCandidateSubmitted(c);
    }

    /// @notice Rule (A): finalises a candidate with a proof that echoes a beacon recorded at least its wait W after
    ///         the candidate. Permissionless. Its burn root becomes accepted; the first sunset checkpoint sets the
    ///         refund boundary. `importedLeafCount` never decreases, and after a sunset checkpoint every later one is
    ///         a sunset checkpoint with the same count (v3 D4).
    /// @dev Every journal word but `echoNonce` must equal the candidate's (its digest), so the tip, n and t are
    ///      compared too (D11), and the floors checked at submission hold.
    function finalize(uint256 beaconIndex, Checkpoint calldata c, bytes calldata seal) external {
        if (mode != Mode.A) revert WrongMode();
        Candidate storage cand = _liveCandidate(c, bytes32(0));
        if (beaconIndex >= beacons.length) revert UnknownBeacon();
        Beacon storage beacon = beacons[beaconIndex];
        if (beacon.timestamp < cand.submittedAt + _wait(cand)) revert BeaconTooEarly();
        if (c.echoNonce != beacon.nonce) revert EchoNonceMismatch();
        _checkMonotone(c);
        verifier.verify(seal, checkpointImageId, _checkpointDigest(c, c.echoNonce));
        _finalize(c, cand.prover);
    }

    /// @notice Rule (B): finalises a candidate once its wait W = deltaAtSubmit has passed (D13), unless it was
    ///         cancelled. Permissionless. No second proof and no beacon: `c` must be, word for word, the journal the
    ///         candidate's one proof was verified against. Same effects and monotonicity as rule (A).
    function finalizeAttested(Checkpoint calldata c) external {
        if (mode != Mode.B) revert WrongMode();
        Candidate storage cand = _liveCandidate(c, c.echoNonce);
        if (block.timestamp < cand.submittedAt + _wait(cand)) revert WaitNotOver();
        _checkMonotone(c);
        // Nothing proven is accepted after a stop, under either rule (under (A) the second proof cannot verify).
        if (_stopped()) revert VerifierStopped();
        _finalize(c, cand.prover);
    }

    /// @dev The effects of a finalisation, shared by both rules. `prover` submitted the candidate (M-2: the fee goes
    ///      to the proof, not to whoever sends the finalisation).
    function _finalize(Checkpoint calldata c, address prover) internal {
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
                parentImportedLeafCount: finalImportedLeafCount,
                parentSunset: finalSunset,
                finalizedAt: uint64(block.timestamp),
                prover: prover
            })
        );
        finalState = c.newState;
        finalImportedLeafCount = c.importedLeafCount;
        finalSunset = c.sunset;
        keylessFrom = uint64(block.timestamp);
        if (c.newState != c.parentState) _stateChanges += 1;
        delete candidates[c.newState];

        uint256 previous = burnRootCheckpoint[c.burnRoot];
        if (previous == 0 || checkpoints[previous - 1].revoked) burnRootCheckpoint[c.burnRoot] = index + 1;
        // The first sunset checkpoint fixes the boundary; later ones carry the same count (checked above).
        if (c.sunset && !sunsetLeafCountSet) {
            sunsetLeafCountSet = true;
            sunsetLeafCount = c.importedLeafCount;
            sunsetSetAt = index;
            sunsetFinalizedAt = block.timestamp;
            emit SunsetLeafCountSet(index, c.importedLeafCount);
        }
        _emitCheckpointFinalized(index, c);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Release side
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Releases one denomination against a proof of a LEZ burn. Permissionless. `r.recipient` receives it less
    ///         the relay fee (if the proof names one) and the burn and checkpoint fees of the fee schedule in force; the
    ///         burn fee is credited to `feeRecipient` and the checkpoint fee to the prover of `r.burnRoot`'s checkpoint
    ///         (D13), the sender of its candidate.
    /// @dev Checks, then the proof, then all state changes, then the payouts. A payout that fails is credited to
    ///      `owed` instead (spec §5.4): the release completes, the nullifier is used, and only the payee can claim.
    ///      The burn and checkpoint fees are always credited, never pushed, so no prover can make releases costly.
    function release(ReleaseRequest calldata r, bytes calldata seal) external nonReentrant {
        (uint256 amount, address prover) = _checkRelease(r);
        verifier.verify(seal, releaseImageId, _releaseJournalDigest(r, amount));

        nullifierUsed[r.nullifier] = true;
        _consumeReleaseCaps(r.token, amount);
        escrowed[r.token] -= amount;

        address feePayee = r.relayer == address(0) ? msg.sender : r.relayer;
        emit Release(r.nullifier, r.token, r.recipient, amount, feePayee, r.relayFee, r.burnRoot);

        uint256 deducted = _creditReleaseFees(r.token, r.denomIndex, prover);
        _payOrCredit(r.token, r.recipient, amount - r.relayFee - deducted);
        if (r.relayFee != 0) _payOrCredit(r.token, feePayee, r.relayFee);
    }

    /// @notice Withdraws what the vault owes the caller in `token` after a failed push payment. Only the payee can
    ///         withdraw, and only to itself.
    function claimOwed(address token) external nonReentrant {
        FerrataRegistry.claimOwed(owed, totalOwed, token);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Admin: token registry, caps, delays, delta
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Registers a token (ETH is `address(0)`), once. Denominations, relay fees, `lezUnit` and `unitWeight`
    ///         are write-once, because in-flight deposits and burns refer to them; only the caps, `depositsEnabled`
    ///         and the release delay stay adjustable. Deposits start enabled.
    ///         The release bucket may not be below the deployment's floor (D13). The checks and writes run in the
    ///         linked library FerrataRegistry.
    ///         Registration also sets fee schedule 0 (no mint or burn fee, the given checkpoint fees) and publishes the
    ///         registry entry and the schedule for the LEZ side (D13).
    function registerToken(address token, TokenParams calldata p) external onlyAdmin {
        FerrataRegistry.registerToken(
            _tokens, _tokenDepositCap, _tokenReleaseCap, _releaseDelays, feeSchedules, token, p
        );
    }

    /// @notice Announces fee schedule `feeState(token).current + 1`: per denomination a mint, a burn and a checkpoint
    ///         fee, each at most 1% of it (mint fees in whole LEZ units). It takes effect seven days later, at the first
    ///         deposit, release or `applyFees` of the token from then on, and applies to the deposits from that leaf
    ///         index on (D13). A new announcement replaces a pending one and restarts the seven days; announcing the
    ///         fees in force withdraws it. A due change takes effect first.
    function announceFees(
        address token,
        uint256[] calldata mintFees,
        uint256[] calldata burnFees,
        uint256[] calldata checkpointFees
    ) external onlyAdmin {
        FerrataRegistry.announceFees(_tokens, feeState, feeSchedules, token, mintFees, burnFees, checkpointFees);
    }

    /// @notice Puts the token's announced fee schedule in force if its seven days have passed; otherwise does nothing.
    ///         Permissionless: deposits and releases of the token do the same.
    function applyFees(address token) external {
        _feesInForce(token);
    }

    /// @notice Enables or disables deposits of a token (delisting). Releases are unaffected.
    function setDepositsEnabled(address token, bool enabled) external onlyAdmin {
        _requireRegistered(token);
        _tokens[token].depositsEnabled = enabled;
        emit DepositsEnabledSet(token, enabled);
    }

    /// @notice Updates a per-token bucket. The level earned under the old parameters is kept, clamped to the new
    ///         capacity; a bucket that was UNLIMITED starts full. A release bucket stays above the floor (D13).
    function setTokenCap(address token, Direction direction, uint256 capacity, uint256 refillPerSecond)
        external
        onlyAdmin
    {
        FerrataRegistry.setTokenCap(
            _tokens, _tokenDepositCap, _tokenReleaseCap, token, direction, capacity, refillPerSecond
        );
    }

    /// @notice Updates a global bucket (value units), with the same rules as `setTokenCap`.
    function setGlobalCap(Direction direction, uint256 capacity, uint256 refillPerSecond) external onlyAdmin {
        FerrataRegistry.setGlobalCap(_globalDepositCap, _globalReleaseCap, direction, capacity, refillPerSecond);
    }

    /// @notice Sets the per-token delay between a burn root's finalisation and releases against it, for roots
    ///         finalised after this call; roots already finalised keep the delay they were finalised under (§5.3).
    function setReleaseDelay(address token, uint256 delay) external onlyAdmin {
        FerrataRegistry.setReleaseDelay(_tokens, _releaseDelays, token, delay, checkpoints.length);
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

    /// @notice Stops both `submitCandidate` and `finalize` (v3 D4); lapses like every other freeze. Lifting it keeps the
    ///         time the freeze or pause ended (now, or when it lapsed), from which keyless retirement counts (M-1).
    function setCheckpointsFrozen(bool frozen) external onlyFreezeAuthority {
        uint256 until = checkpointsFrozenUntil;
        if (until > block.timestamp) until = block.timestamp;
        checkpointsFrozenUntil = frozen ? block.timestamp + maxFreeze : until;
        emit CheckpointsFrozenSet(frozen, checkpointsFrozenUntil);
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

    /// @notice Stops deposits forever (spec §4.5). It cannot touch escrow, checkpoints or releases. The freeze
    ///         authority may retire at any time; anyone may from `retireOpensAt()` on (D13), so refunds never depend on
    ///         the freeze key.
    function retire() external {
        if (msg.sender != freezeAuthority && block.timestamp < retireOpensAt()) revert RetireNotOpen();
        if (retired) revert AlreadyRetired();
        retired = true;
        emit Retired();
    }

    /// @notice When anyone may retire the vault: KEYLESS_RETIREMENT after the first moment a checkpoint could be
    ///         finalised since the clock last started (`keylessFrom`, or the end of a checkpoint freeze or watcher
    ///         pause in force: no freeze time counts), that is after the shortest wait delta + (tMin - 1) * turnSeconds;
    ///         or, if earlier, once the first sunset checkpoint can no longer be revoked (D11; refunds open then too).
    ///         A checkpoint must therefore be submitted or finalised at least every 30 days, from deployment on.
    function retireOpensAt() public view returns (uint256 opensAt) {
        uint256 from = keylessFrom;
        if (checkpointsFrozenUntil > from) from = checkpointsFrozenUntil;
        opensAt = from + delta + _keylessSpan;
        if (sunsetLeafCountSet) {
            uint256 sunsetOpens = _refundsOpenAt();
            if (sunsetOpens < opensAt) opensAt = sunsetOpens;
        }
    }

    /// @dev The second after the last one in which the sunset checkpoint can be revoked (v3 D5, D11).
    function _refundsOpenAt() internal view returns (uint256) {
        return sunsetFinalizedAt + revokeWindow + 1;
    }

    /// @notice Reject-only: deletes a pending (not yet finalised) candidate, so it can never be finalised. Finalised
    ///         checkpoints are untouched, and the same state can be submitted again later as a fresh candidate with a
    ///         new wait, so a cancellation costs an honest prover one wait. Any watcher or the freeze authority (D13).
    ///         Rule (A): when the tip is not in an honest node's final tier by submittedAt + Δ_g, or the key set is not
    ///         the live one; rule (B): when the tip is not already final at submission.
    function cancelCandidate(bytes32 newState) external {
        if (msg.sender != freezeAuthority && !_isWatcher(msg.sender)) revert NotWatcher();
        FerrataRegistry.cancelCandidate(candidates, newState);
    }

    /// @notice A watcher's checkpoint pause (D13): `submitCandidate` and finalisation stop until 12 h from now. It
    ///         never shortens a longer freeze, and a watcher cannot lift it (the freeze authority can).
    function pauseCheckpoints() external {
        if (!_isWatcher(msg.sender)) revert NotWatcher();
        uint256 until = block.timestamp + WATCHER_PAUSE;
        if (until > checkpointsFrozenUntil) checkpointsFrozenUntil = until;
        emit CheckpointsPaused(msg.sender, checkpointsFrozenUntil);
    }

    /// @notice Reject-only revocation (spec §4.4): checkpoint `index` and every later one stop counting, and
    ///         `finalState` returns to the state `index` was built on. Allowed within `revokeWindow` of `index`'s
    ///         finalisation. Used nullifiers stay used; nothing is accepted by this call. Pending candidates are
    ///         invalidated too (`revocationEpoch`), so the restored state needs a fresh candidate and a full wait.
    function revokeFrom(uint256 index) external onlyFreezeAuthority {
        // The checks, the marking loop and the event run in the library; the restored state is set here.
        FerrataRegistry.revokeFrom(checkpoints, index, revokeWindow);
        FinalizedCheckpoint storage first = checkpoints[index];
        finalState = first.parentState;
        finalImportedLeafCount = first.parentImportedLeafCount;
        finalSunset = first.parentSunset;
        revocationEpoch += 1;
        if (sunsetLeafCountSet && sunsetSetAt >= index) {
            sunsetLeafCountSet = false;
            sunsetLeafCount = 0;
            sunsetSetAt = 0;
            sunsetFinalizedAt = 0;
            emit SunsetLeafCountCleared();
        }
    }

    // ------------------------------------------------------------------------------------------------------------
    // The admin hands its role over in two steps; proposing address(0) cancels a pending transfer. No address may hold
    // both roles. The freeze authority changes only through the admin's timelocked proposal below (D13); a multisig
    // rotates its own signers.
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

    // ------------------------------------------------------------------------------------------------------------
    // Timelocked role change by the admin (D13): the watcher set and, when the freeze key is lost or compromised, the
    // freeze authority. One proposal at a time; it takes effect ROLE_TIMELOCK after it was made, applied by anyone. A
    // new proposal replaces a pending one (proposing the current roles withdraws it).
    // ------------------------------------------------------------------------------------------------------------

    function proposeRoles(address[3] calldata newWatchers, address newFreezeAuthority) external onlyAdmin {
        FerrataRegistry.proposeRoles(roleProposal, newWatchers, newFreezeAuthority, admin);
    }

    /// @notice Applies the proposed roles once the timelock is over.
    function applyRoles() external {
        address newAuthority = FerrataRegistry.applyRoles(roleProposal, watchers, admin, freezeAuthority);
        if (newAuthority != address(0)) freezeAuthority = newAuthority;
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

    /// @notice The fees of denomination `denomIndex` under fee schedule `index` of `token` (D13). A deposit under it
    ///         escrows the denomination + mintFee + relayFee; a release under it pays the denomination less burnFee,
    ///         checkpointFee and the relay fee its proof names (0 or relayFee).
    function fees(address token, uint256 index, uint8 denomIndex)
        external
        view
        returns (uint256 mintFee, uint256 relayFee, uint256 burnFee, uint256 checkpointFee)
    {
        FeeSchedule storage s = feeSchedules[token][index];
        uint256 inbound = s.inbound[denomIndex];
        uint256 outbound = s.outbound[denomIndex];
        return (inbound >> 128, uint128(inbound), outbound >> 128, uint128(outbound));
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
    function _leafHash(uint64 leafIndex, address depositor, address token, uint256 amount, bytes32 commitment)
        internal
        pure
        returns (bytes32)
    {
        return sha256(abi.encodePacked(LEAF_PREFIX, LEAF_TAG, leafIndex, depositor, token, amount, commitment));
    }

    // ------------------------------------------------------------------------------------------------------------
    // Internal: deposits and the tree
    // ------------------------------------------------------------------------------------------------------------

    /// @dev The deposit's checks; returns the denomination and the total to escrow under the fee schedule in force,
    ///      which a due fee change becomes first (so every deposit from its due time on pays it).
    function _checkDeposit(address token, uint8 denomIndex, bytes32 commitment, uint64 index)
        internal
        returns (uint256 amount, uint256 total)
    {
        // A finalised sunset checkpoint ends deposits like retirement: LEZ imports nothing after `Sunset` (L-3).
        if (retired || finalSunset) revert VaultRetired();
        if (_frozen(depositsFrozenUntil)) revert DepositsAreFrozen();
        TokenConfig storage cfg = _tokens[token];
        if (!cfg.registered) revert TokenNotRegistered();
        if (_frozen(tokenDepositsFrozenUntil[token])) revert TokenDepositsAreFrozen();
        if (!cfg.depositsEnabled) revert DepositsDisabled();
        if (denomIndex >= cfg.denominations.length) revert InvalidDenominationIndex();
        if (commitment == bytes32(0)) revert ZeroCommitment();
        if (index != depositCount[msg.sender]) revert WrongDepositIndex();
        if (nextLeafIndex >= MAX_LEAVES) revert TreeFull();
        amount = cfg.denominations[denomIndex];
        uint256 inbound = feeSchedules[token][_feesInForce(token)].inbound[denomIndex];
        total = amount + (inbound >> 128) + uint128(inbound);
    }

    /// @dev Escrows `total` and counts it against the deposit caps; the leaf binds the denomination `amount`.
    function _recordDeposit(address token, uint256 amount, uint256 total, bytes32 commitment, uint64 index) internal {
        _consumeDepositCaps(token, total);
        depositCount[msg.sender] = index + 1;
        escrowed[token] += total;

        uint64 leafIndex = nextLeafIndex;
        _insertLeaf(leafIndex, _leafHash(leafIndex, msg.sender, token, amount, commitment));
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
    ///      burnRoot, burnCount, importedLeafCount, sunset, echoNonce, tipId, tipHash, n, t)): 17 words (v3 D3), with
    ///      `echoNonce` given separately (0 for a candidate's journal). Encoded in three parts, which for static types
    ///      is the same byte string as one abi.encode.
    function _checkpointDigest(Checkpoint calldata c, bytes32 echoNonce) internal view returns (bytes32) {
        bytes memory head = abi.encode(CHECKPOINT_DOMAIN, block.chainid, address(this), lezConfigId, c.nMin, c.tMin);
        bytes memory body =
            abi.encode(c.parentState, c.newState, c.burnRoot, c.burnCount, c.importedLeafCount, c.sunset, echoNonce);
        bytes memory tail = abi.encode(c.tipId, c.tipHash, c.n, c.t);
        return sha256(bytes.concat(head, body, tail));
    }

    /// @dev Emitted from their own frames: eleven event fields do not fit next to the callers' locals on the stack.
    function _emitCandidateSubmitted(Checkpoint calldata c) internal {
        emit CandidateSubmitted(
            c.newState,
            c.parentState,
            c.burnRoot,
            c.burnCount,
            c.importedLeafCount,
            c.sunset,
            delta,
            c.tipId,
            c.tipHash,
            c.n,
            c.t
        );
    }

    function _emitCheckpointFinalized(uint256 index, Checkpoint calldata c) internal {
        emit CheckpointFinalized(
            index,
            c.newState,
            c.parentState,
            c.burnRoot,
            c.burnCount,
            c.importedLeafCount,
            c.sunset,
            c.tipId,
            c.tipHash,
            c.n,
            c.t
        );
    }

    /// @dev The vault's own committee checks (v3 D4): the journal names the deployed floors, and the key set in
    ///      `newState` meets them.
    function _checkCommittee(Checkpoint calldata c) internal view {
        if (c.nMin != nMin || c.tMin != tMin) revert WrongCommitteeFloor();
        if (c.n < nMin || c.t < tMin) revert KeySetBelowFloor();
    }

    /// @dev A candidate's wait W = deltaAtSubmit + (t - 1) * turnSeconds (v3 D4): t - 1 corrupted keys can each hold
    ///      a full turn with empty inscriptions. t >= tMin >= 1 was checked at submission. Under rule (B)
    ///      turnSeconds is 0 and W = deltaAtSubmit, a reaction window only (D13).
    function _wait(Candidate storage cand) internal view returns (uint256) {
        return uint256(cand.deltaAtSubmit) + (uint256(cand.t) - 1) * turnSeconds;
    }

    /// @dev The candidate `c` names, if it can be finalised now under either rule: no checkpoint freeze, submitted,
    ///      `finalState` unchanged and nothing revoked since (so it still builds on `finalState`), and `c` is
    ///      word for word its journal. `echoNonce` is the word hashed in place of c.echoNonce: 0 under (A), whose
    ///      second proof echoes a beacon; c.echoNonce under (B), so that it too must be the candidate's 0.
    function _liveCandidate(Checkpoint calldata c, bytes32 echoNonce) internal view returns (Candidate storage cand) {
        if (_frozen(checkpointsFrozenUntil)) revert CheckpointsAreFrozen();
        cand = candidates[c.newState];
        if (cand.submittedAt == 0) revert UnknownCandidate();
        if (cand.epoch != _epoch()) revert StaleCandidate();
        if (_checkpointDigest(c, echoNonce) != cand.journalDigest) revert CandidateMismatch();
    }

    /// @dev Changes with every finalisation that moves `finalState` and with every revocation (review L-1): a candidate
    ///      is live only in the epoch it was submitted in. A no-op checkpoint leaves pending candidates live.
    function _epoch() internal view returns (uint256) {
        return revocationEpoch + _stateChanges;
    }

    /// @dev v3 D4: the imported leaf count never decreases, and after a sunset checkpoint every later one is a sunset
    ///      checkpoint with the same count. D11: LEZ cannot have imported more leaves than the tree has.
    function _checkMonotone(Checkpoint calldata c) internal view {
        if (c.importedLeafCount < finalImportedLeafCount) revert ImportedLeafCountDecreased();
        if (finalSunset && (!c.sunset || c.importedLeafCount != finalImportedLeafCount)) revert SunsetIsFinal();
        if (c.importedLeafCount > nextLeafIndex) revert ImportedLeafCountTooHigh();
    }

    // ------------------------------------------------------------------------------------------------------------
    // Internal: releases and journals
    // ------------------------------------------------------------------------------------------------------------

    /// @dev Returns the denomination and the prover of the root's checkpoint (credited the checkpoint fee).
    function _checkRelease(ReleaseRequest calldata r) internal view returns (uint256 amount, address prover) {
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
        prover = checkpoints[entry - 1].prover;
        uint256 opensAt = checkpoints[entry - 1].finalizedAt + delay;
        // D11: the last second in which the root's checkpoint can be revoked is never a release second.
        if (delay == revokeWindow) opensAt += 1;
        if (block.timestamp < opensAt) revert ReleaseDelayNotElapsed();
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
        if (!paid) _credit(payee, token, amount);
    }

    function _credit(address payee, address token, uint256 amount) internal {
        owed[payee][token] += amount;
        totalOwed[token] += amount;
        emit OwedCredited(payee, token, amount);
    }

    /// @dev Credits a release's burn fee to `feeRecipient` and its checkpoint fee to the root's prover, under the fee
    ///      schedule in force (a due change first), and returns their sum. Each is at most 1% of the denomination.
    function _creditReleaseFees(address token, uint8 denomIndex, address prover) internal returns (uint256) {
        uint256 outbound = feeSchedules[token][_feesInForce(token)].outbound[denomIndex];
        uint256 burnFee = outbound >> 128;
        uint256 checkpointFee = uint128(outbound);
        if (burnFee != 0) _credit(feeRecipient, token, burnFee);
        if (checkpointFee != 0) _credit(prover, token, checkpointFee);
        return burnFee + checkpointFee;
    }

    /// @dev The index of the token's fee schedule in force, after putting an announced one in force if it is due.
    function _feesInForce(address token) internal returns (uint256 index) {
        FeeState storage st = feeState[token];
        index = st.current;
        uint256 pendingAt = st.pendingAt;
        if (pendingAt != 0 && block.timestamp >= pendingAt) {
            index = FerrataRegistry.applyFees(_tokens, feeState, feeSchedules, token);
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
        uint256 level = FerrataRegistry.currentLevel(b);
        if (amount > level) return false;
        b.level = level - amount;
        b.lastUpdate = block.timestamp;
        return true;
    }

    function _available(Bucket storage b) internal view returns (uint256) {
        return b.capacity == UNLIMITED ? UNLIMITED : FerrataRegistry.currentLevel(b);
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

    function _isWatcher(address who) internal view returns (bool) {
        return who == watchers[0] || who == watchers[1] || who == watchers[2]; // msg.sender is never address(0)
    }

    /// @dev Whether RISC Zero's emergency stop in front of the verifier has been tripped (never undone).
    function _stopped() internal view returns (bool) {
        return IEmergencyStop(address(verifier)).paused();
    }

    function _requireRegistered(address token) internal view {
        if (!_tokens[token].registered) revert TokenNotRegistered();
    }
}
