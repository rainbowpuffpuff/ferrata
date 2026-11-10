// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ReentrancyGuardTransient} from "@openzeppelin/contracts/utils/ReentrancyGuardTransient.sol";

import {IRiscZeroVerifier} from "./interfaces/IRiscZeroVerifier.sol";

/// @title FerrataVault
/// @notice Ethereum half of a privacy-preserving lock-and-mint bridge to LEZ (Logos RFP-022). Prototype, not audited.
/// @dev One vault per bridge configuration. Deposits of fixed denominations are appended to an incremental SHA-256
///      Merkle tree whose root is kept in storage slot 0, which the LEZ side proves with an Ethereum storage proof.
///      Releases pay out against a RISC Zero proof of a LEZ burn under a burn root that was itself accepted with a
///      proof. There is no upgradeability, no delegatecall, no settable verifier and no token rescue function.
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

    /// @notice A release. Every field except `denomIndex` is bound into the release journal; `denomIndex` is bound
    ///         through the amount it selects.
    struct ReleaseRequest {
        address token;
        uint8 denomIndex;
        address recipient;
        address feeRecipient; // address(0) means: pay the fee to msg.sender
        uint256 relayFee; // 0 or exactly standardRelayFee[token][denomIndex]
        bytes32 burnRoot;
        bytes32 nullifier;
    }

    struct TokenConfig {
        bool registered;
        bool depositsEnabled;
        uint256 lezUnit;
        uint256 unitWeight;
        uint256[] denominations;
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

    uint256 public constant TREE_DEPTH = 32;
    uint256 public constant MAX_LEAVES = 2 ** TREE_DEPTH;
    uint256 public constant MAX_DENOMINATIONS = 8;
    /// @notice Bounds that keep the cap arithmetic `(amount / lezUnit) * unitWeight` from overflowing.
    uint256 public constant MAX_DENOMINATION = type(uint128).max;
    uint256 public constant MAX_UNIT_WEIGHT = type(uint128).max;
    uint256 public constant MAX_RELEASE_DELAY = 30 days;
    uint256 public constant UNLIMITED = type(uint256).max;
    /// @notice Sentinel token address for native ETH.
    address public constant ETH = address(0);

    bytes1 public constant LEAF_PREFIX = 0x00;
    bytes1 public constant NODE_PREFIX = 0x01;
    /// @notice sha256("ferrata/vault/leaf/v1")
    bytes32 public constant LEAF_TAG = 0x084d5599858dde3bdadaec9a70979121276922b791de25071d145ac6b13c4286;
    /// @notice sha256("ferrata/vault/burn-root/v1")
    bytes32 public constant ROOT_DOMAIN = 0xba4a8683e1dc6ef6e077a4cae76d60704f4e7fcd4fc6947d6c99cae3b4195738;
    /// @notice sha256("ferrata/vault/release/v1")
    bytes32 public constant RELEASE_DOMAIN = 0x38d947b280ef2710d905d714dc6c55d9fdaa844e4769df08152f6bed4bc9f444;

    // ------------------------------------------------------------------------------------------------------------
    // Immutables: everything that defines what the vault accepts as a valid proof
    // ------------------------------------------------------------------------------------------------------------

    IRiscZeroVerifier public immutable verifier;
    bytes32 public immutable releaseImageId;
    bytes32 public immutable rootImageId;
    /// @notice Identifies the paired LEZ deployment (network id, zone id, bridge program id, configuration id).
    bytes32 public immutable lezConfigId;

    // ------------------------------------------------------------------------------------------------------------
    // Storage. The first two variables are part of the cross-chain interface: the LEZ side reads `depositRoot`
    // from slot 0 with an Ethereum storage proof. Nothing may be declared before them, and the only parent
    // contract (ReentrancyGuardTransient) uses transient storage, not storage slots.
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Root of the deposit tree. Storage slot 0.
    bytes32 public depositRoot;
    /// @notice Number of leaves in the deposit tree, i.e. the index the next leaf gets. Storage slot 1.
    uint64 public nextLeafIndex;
    /// @dev `_branch[h]` is the root of the most recently completed subtree of height h. It is read only while
    ///      bit h of `nextLeafIndex` is set, which is exactly when that subtree is the left neighbour of the
    ///      next leaf's path at height h. A static array always starts a new slot, so slot 1 holds only
    ///      `nextLeafIndex`.
    bytes32[TREE_DEPTH] private _branch;

    address public admin;
    address public pendingAdmin;
    address public freezeAuthority;
    address public pendingFreezeAuthority;

    /// @notice Minimum LEZ finality depth a burn-root proof must attest to.
    uint32 public minLezDepth;
    /// @notice Highest LEZ block id seen in an accepted burn-root proof (informational).
    uint64 public latestLezBlockId;
    bool public depositsFrozen;
    bool public releasesFrozen;

    mapping(address token => TokenConfig) private _tokens;
    mapping(address token => bool) public tokenDepositsFrozen;
    mapping(address token => bool) public tokenReleasesFrozen;
    mapping(address token => mapping(uint8 denomIndex => uint256)) public standardRelayFee;
    mapping(address token => uint256) public releaseDelay;
    mapping(address token => uint256) public escrowed;
    mapping(address depositor => uint64) public depositCount;
    mapping(bytes32 burnRoot => uint256) public burnRootAcceptedAt;
    mapping(bytes32 nullifier => bool) public nullifierUsed;

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
    event BurnRootAccepted(bytes32 indexed burnRoot, uint64 lezBlockId, uint64 burnCount, uint32 depth);
    event Release(
        bytes32 indexed nullifier,
        address indexed token,
        address indexed recipient,
        uint256 amount,
        address feeRecipient,
        uint256 relayFee,
        bytes32 burnRoot
    );

    event TokenRegistered(address indexed token, uint256[] denominations, uint256 lezUnit, uint256 unitWeight);
    event DepositsEnabledSet(address indexed token, bool enabled);
    event TokenCapSet(address indexed token, Direction indexed direction, uint256 capacity, uint256 refillPerSecond);
    event GlobalCapSet(Direction indexed direction, uint256 capacity, uint256 refillPerSecond);
    event UnitWeightSet(address indexed token, uint256 unitWeight);
    event StandardRelayFeeSet(address indexed token, uint8 indexed denomIndex, uint256 fee);
    event ReleaseDelaySet(address indexed token, uint256 delay);
    event MinLezDepthSet(uint32 depth);
    event AdminTransferStarted(address indexed currentAdmin, address indexed pendingAdmin);
    event AdminTransferred(address indexed previousAdmin, address indexed newAdmin);
    event FreezeAuthorityTransferStarted(address indexed currentAuthority, address indexed pendingAuthority);
    event FreezeAuthorityTransferred(address indexed previousAuthority, address indexed newAuthority);
    event DepositsFrozenSet(bool frozen);
    event ReleasesFrozenSet(bool frozen);
    event TokenDepositsFrozenSet(address indexed token, bool frozen);
    event TokenReleasesFrozenSet(address indexed token, bool frozen);

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

    error TokenAlreadyRegistered();
    error TokenNotRegistered();
    error TokenHasNoCode();
    error InvalidDenominations();
    error InvalidLezUnit();
    error DenominationNotMultipleOfLezUnit();
    error InvalidUnitWeight();
    error RelayFeeTooHigh();
    error ReleaseDelayTooLong();

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

    error ReleasesAreFrozen();
    error TokenReleasesAreFrozen();
    error InsufficientLezDepth();
    error UnknownBurnRoot();
    error ReleaseDelayNotElapsed();
    error NullifierAlreadyUsed();
    error InvalidRelayFee();
    error ZeroRecipient();
    error InsufficientEscrow();
    error TokenReleaseCapExceeded();
    error GlobalReleaseCapExceeded();
    error EthTransferFailed();

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

    /// @param minLezDepth_ Initial finality depth (the recommended default for this deployment).
    /// @param globalDepositCap_ Global deposit bucket, in value units. Starts full.
    /// @param globalReleaseCap_ Global release bucket, in value units. Starts full.
    constructor(
        IRiscZeroVerifier verifier_,
        bytes32 releaseImageId_,
        bytes32 rootImageId_,
        bytes32 lezConfigId_,
        address admin_,
        address freezeAuthority_,
        uint32 minLezDepth_,
        CapParams memory globalDepositCap_,
        CapParams memory globalReleaseCap_
    ) {
        if (address(verifier_).code.length == 0) revert VerifierHasNoCode();
        if (releaseImageId_ == bytes32(0) || rootImageId_ == bytes32(0) || releaseImageId_ == rootImageId_) {
            revert InvalidImageIds();
        }
        if (lezConfigId_ == bytes32(0)) revert InvalidLezConfigId();
        if (admin_ == address(0) || freezeAuthority_ == address(0)) revert ZeroAddress();
        if (admin_ == freezeAuthority_) revert RolesMustDiffer();

        verifier = verifier_;
        releaseImageId = releaseImageId_;
        rootImageId = rootImageId_;
        lezConfigId = lezConfigId_;

        // The root of the empty tree, so that `depositRoot` is always the root of the current tree.
        bytes32 emptySubtree = bytes32(0);
        for (uint256 h = 0; h < TREE_DEPTH; h++) {
            emptySubtree = _hashNode(emptySubtree, emptySubtree);
        }
        depositRoot = emptySubtree;

        admin = admin_;
        freezeAuthority = freezeAuthority_;
        minLezDepth = minLezDepth_;
        _initBucket(_globalDepositCap, globalDepositCap_.capacity, globalDepositCap_.refillPerSecond);
        _initBucket(_globalReleaseCap, globalReleaseCap_.capacity, globalReleaseCap_.refillPerSecond);

        emit AdminTransferred(address(0), admin_);
        emit FreezeAuthorityTransferred(address(0), freezeAuthority_);
        emit MinLezDepthSet(minLezDepth_);
        emit GlobalCapSet(Direction.Deposit, globalDepositCap_.capacity, globalDepositCap_.refillPerSecond);
        emit GlobalCapSet(Direction.Release, globalReleaseCap_.capacity, globalReleaseCap_.refillPerSecond);
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

    // ------------------------------------------------------------------------------------------------------------
    // Release side
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Accepts a LEZ burn-tree root proven final at `depth` blocks. Permissionless.
    /// @dev The first acceptance fixes `burnRootAcceptedAt`; a replayed proof, or a proof for the same root at a
    ///      later LEZ block, cannot move it (which would otherwise restart the release delay). Raising
    ///      `minLezDepth` later does not affect roots already accepted.
    function acceptBurnRoot(bytes32 burnRoot, uint64 lezBlockId, uint64 burnCount, uint32 depth, bytes calldata seal)
        external
    {
        if (releasesFrozen) revert ReleasesAreFrozen();
        if (depth < minLezDepth) revert InsufficientLezDepth();
        verifier.verify(seal, rootImageId, _rootJournalDigest(burnRoot, lezBlockId, burnCount, depth));

        if (lezBlockId > latestLezBlockId) latestLezBlockId = lezBlockId;
        if (burnRootAcceptedAt[burnRoot] == 0) {
            burnRootAcceptedAt[burnRoot] = block.timestamp;
            emit BurnRootAccepted(burnRoot, lezBlockId, burnCount, depth);
        }
    }

    /// @notice Releases one denomination to `r.recipient` against a proof of a LEZ burn. Permissionless.
    /// @dev Checks, then the proof, then all state changes, then the payouts. A payout that fails reverts the
    ///      whole call, so the nullifier stays unused and only this request is affected.
    function release(ReleaseRequest calldata r, bytes calldata seal) external nonReentrant {
        uint256 amount = _checkRelease(r);
        verifier.verify(seal, releaseImageId, _releaseJournalDigest(r, amount));

        nullifierUsed[r.nullifier] = true;
        _consumeReleaseCaps(r.token, amount);
        escrowed[r.token] -= amount;

        address feeRecipient = r.feeRecipient == address(0) ? msg.sender : r.feeRecipient;
        emit Release(r.nullifier, r.token, r.recipient, amount, feeRecipient, r.relayFee, r.burnRoot);

        _pay(r.token, r.recipient, amount - r.relayFee);
        if (r.relayFee != 0) _pay(r.token, feeRecipient, r.relayFee);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Admin: token registry, caps, fees, delays, finality depth
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Registers a token (ETH is `address(0)`). A token is registered once; its denominations and
    ///         `lezUnit` never change, because in-flight claims refer to them. Deposits start enabled.
    /// @param denominations_ 1 to 8 strictly increasing, non-zero amounts, each an exact multiple of `lezUnit`.
    /// @param lezUnit A power of ten: the base-unit size of one LEZ unit of the wrapped token.
    /// @param unitWeight Global-cap value units per `lezUnit` of this token.
    function registerToken(
        address token,
        uint256[] calldata denominations_,
        uint256 lezUnit,
        uint256 unitWeight,
        CapParams calldata depositCap,
        CapParams calldata releaseCap
    ) external onlyAdmin {
        TokenConfig storage cfg = _tokens[token];
        if (cfg.registered) revert TokenAlreadyRegistered();
        if (token != ETH && token.code.length == 0) revert TokenHasNoCode();
        _checkLezUnit(lezUnit);
        _checkUnitWeight(unitWeight);
        _checkDenominations(denominations_, lezUnit);

        cfg.registered = true;
        cfg.depositsEnabled = true;
        cfg.lezUnit = lezUnit;
        cfg.unitWeight = unitWeight;
        cfg.denominations = denominations_;
        _initBucket(_tokenDepositCap[token], depositCap.capacity, depositCap.refillPerSecond);
        _initBucket(_tokenReleaseCap[token], releaseCap.capacity, releaseCap.refillPerSecond);

        emit TokenRegistered(token, denominations_, lezUnit, unitWeight);
        emit TokenCapSet(token, Direction.Deposit, depositCap.capacity, depositCap.refillPerSecond);
        emit TokenCapSet(token, Direction.Release, releaseCap.capacity, releaseCap.refillPerSecond);
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

    function setUnitWeight(address token, uint256 unitWeight) external onlyAdmin {
        _requireRegistered(token);
        _checkUnitWeight(unitWeight);
        _tokens[token].unitWeight = unitWeight;
        emit UnitWeightSet(token, unitWeight);
    }

    /// @notice Sets the single non-zero relay fee a release of this denomination may carry.
    function setStandardRelayFee(address token, uint8 denomIndex, uint256 fee) external onlyAdmin {
        _requireRegistered(token);
        uint256[] storage denoms = _tokens[token].denominations;
        if (denomIndex >= denoms.length) revert InvalidDenominationIndex();
        if (fee >= denoms[denomIndex]) revert RelayFeeTooHigh();
        standardRelayFee[token][denomIndex] = fee;
        emit StandardRelayFeeSet(token, denomIndex, fee);
    }

    /// @notice Sets the per-token delay between a burn root's acceptance and releases against it.
    function setReleaseDelay(address token, uint256 delay) external onlyAdmin {
        _requireRegistered(token);
        if (delay > MAX_RELEASE_DELAY) revert ReleaseDelayTooLong();
        releaseDelay[token] = delay;
        emit ReleaseDelaySet(token, delay);
    }

    function setMinLezDepth(uint32 depth) external onlyAdmin {
        minLezDepth = depth;
        emit MinLezDepthSet(depth);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Freeze authority. It cannot touch the registry or caps; the admin cannot freeze or unfreeze.
    // ------------------------------------------------------------------------------------------------------------

    function setDepositsFrozen(bool frozen) external onlyFreezeAuthority {
        depositsFrozen = frozen;
        emit DepositsFrozenSet(frozen);
    }

    /// @notice Freezing releases also stops `acceptBurnRoot`, so a root cannot age through its release delay
    ///         while releases are frozen.
    function setReleasesFrozen(bool frozen) external onlyFreezeAuthority {
        releasesFrozen = frozen;
        emit ReleasesFrozenSet(frozen);
    }

    function setTokenDepositsFrozen(address token, bool frozen) external onlyFreezeAuthority {
        _requireRegistered(token);
        tokenDepositsFrozen[token] = frozen;
        emit TokenDepositsFrozenSet(token, frozen);
    }

    function setTokenReleasesFrozen(address token, bool frozen) external onlyFreezeAuthority {
        _requireRegistered(token);
        tokenReleasesFrozen[token] = frozen;
        emit TokenReleasesFrozenSet(token, frozen);
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

    /// @notice The deposit-tree leaf: sha256(0x00 || LEAF_TAG || uint64 leafIndex || token || uint256 amount ||
    ///         commitment), 125 bytes of preimage.
    function leafHash(uint64 leafIndex, address token, uint256 amount, bytes32 commitment)
        public
        pure
        returns (bytes32)
    {
        return sha256(abi.encodePacked(LEAF_PREFIX, LEAF_TAG, leafIndex, token, amount, commitment));
    }

    // ------------------------------------------------------------------------------------------------------------
    // Internal: deposits and the tree
    // ------------------------------------------------------------------------------------------------------------

    function _checkDeposit(address token, uint8 denomIndex, bytes32 commitment, uint64 index)
        internal
        view
        returns (uint256 amount)
    {
        if (depositsFrozen) revert DepositsAreFrozen();
        TokenConfig storage cfg = _tokens[token];
        if (!cfg.registered) revert TokenNotRegistered();
        if (tokenDepositsFrozen[token]) revert TokenDepositsAreFrozen();
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
        _insertLeaf(leafIndex, leafHash(leafIndex, token, amount, commitment));
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
    // Internal: releases and journals
    // ------------------------------------------------------------------------------------------------------------

    function _checkRelease(ReleaseRequest calldata r) internal view returns (uint256 amount) {
        if (releasesFrozen) revert ReleasesAreFrozen();
        TokenConfig storage cfg = _tokens[r.token];
        if (!cfg.registered) revert TokenNotRegistered();
        if (tokenReleasesFrozen[r.token]) revert TokenReleasesAreFrozen();
        if (r.denomIndex >= cfg.denominations.length) revert InvalidDenominationIndex();
        amount = cfg.denominations[r.denomIndex];
        if (r.recipient == address(0)) revert ZeroRecipient();

        uint256 acceptedAt = burnRootAcceptedAt[r.burnRoot];
        if (acceptedAt == 0) revert UnknownBurnRoot();
        if (block.timestamp < acceptedAt + releaseDelay[r.token]) revert ReleaseDelayNotElapsed();
        if (nullifierUsed[r.nullifier]) revert NullifierAlreadyUsed();
        // A free-form fee would fingerprint the user: only 0 or the standard fee is accepted.
        if (r.relayFee != 0 && r.relayFee != standardRelayFee[r.token][r.denomIndex]) revert InvalidRelayFee();
        if (amount > escrowed[r.token]) revert InsufficientEscrow();
    }

    /// @dev sha256(abi.encode(ROOT_DOMAIN, chainid, vault, lezConfigId, lezBlockId, burnRoot, burnCount, depth)).
    ///      Provisional layout; kept in this one function.
    function _rootJournalDigest(bytes32 burnRoot, uint64 lezBlockId, uint64 burnCount, uint32 depth)
        internal
        view
        returns (bytes32)
    {
        return sha256(
            abi.encode(ROOT_DOMAIN, block.chainid, address(this), lezConfigId, lezBlockId, burnRoot, burnCount, depth)
        );
    }

    /// @dev sha256(abi.encode(RELEASE_DOMAIN, chainid, vault, lezConfigId, burnRoot, nullifier, token, amount,
    ///      recipient, feeRecipient, relayFee)).
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
                r.feeRecipient,
                r.relayFee
            )
        );
    }

    function _pay(address token, address to, uint256 amount) internal {
        if (token == ETH) {
            (bool ok,) = to.call{value: amount}("");
            if (!ok) revert EthTransferFailed();
        } else {
            IERC20(token).safeTransfer(to, amount);
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
