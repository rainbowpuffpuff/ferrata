// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import {FerrataVault} from "./FerrataVault.sol";

/// @title FerrataRegistry
/// @notice Token registration, fee schedules, cap settings, role proposals and owed claims for FerrataVault, as a
///         linked library: with the v4 and v5 features the vault's runtime would exceed EIP-170's 24,576 bytes
///         (SPEC-v3-delta D11). Its external functions run by DELEGATECALL in the vault's storage and are reached only
///         through the vault's functions of the same names (`applyFees` also from deposits and releases); the library
///         has no state. The internal functions are compiled into whichever contract calls them.
/// @dev The registry entry and every fee schedule in force are also published, hashed, at SHA-256 storage slots
///      the LEZ side attests (D13; layout of bridge-wire `registry_slot` and `fee_slot`): a slot address that does
///      not depend on the vault's Solidity layout, and a hash the LEZ guest can recompute without keccak.
library FerrataRegistry {
    using SafeERC20 for IERC20;

    // FerrataVault's constants of the same names (a contract's constants are not reachable from a library); the
    // registration and cap tests pin each against the vault's public value.
    uint256 private constant MAX_DENOMINATIONS = 8;
    /// @dev Half of 2**128 - 1: the LEZ side's cap arithmetic takes twice the largest denomination in 128 bits.
    uint256 private constant MAX_DENOMINATION = type(uint128).max / 2;
    uint256 private constant MAX_UNIT_WEIGHT = type(uint128).max;
    uint256 private constant MAX_RELEASE_DELAY = 30 days;
    uint256 private constant UNLIMITED = type(uint256).max;
    uint256 private constant ROLE_TIMELOCK = 14 days; // D13
    /// @dev The immutable fee ceiling (D13): every fee is at most 1/100 of its denomination.
    uint256 private constant FEE_CEILING_DIVISOR = 100;
    /// @dev A fee change takes effect this long after it is announced (D13), in Ethereum time.
    uint256 private constant FEE_NOTICE = 7 days;
    /// @dev FerrataVault.NEXT_LEAF_INDEX_SLOT: the leaf count, alone in its slot (part of the cross-chain interface).
    uint256 private constant LEAF_COUNT_SLOT = 1;
    /// @dev FerrataVault.DEPOSIT_ROOT_SLOT: the deposit root (part of the cross-chain interface).
    uint256 private constant DEPOSIT_ROOT_SLOT = 0;
    /// @dev FerrataVault's leaf and node encoding (refunds fold a leaf's path here).
    bytes1 private constant LEAF_PREFIX = 0x00;
    bytes1 private constant NODE_PREFIX = 0x01;
    /// @dev sha256("ferrata/vault/leaf/v2")
    bytes32 private constant LEAF_TAG = 0x17b9508a0d0cbf61207c0fcb66f06ed92a6aeec110a54860479f0bf3cd6aff0b;

    /// @notice The deployment's bounds a registration must respect.
    struct Limits {
        uint256 minReleaseDelay;
        uint256 revokeWindow;
        uint256 releaseCapFloor;
        uint256 releaseRefillFloor;
    }

    /// @notice Validates and stores a registration; the rules are documented at the vault's `registerToken`. Also
    ///         stores fee schedule 0 and publishes it and the registry entry (D13).
    function registerToken(
        mapping(address token => FerrataVault.TokenConfig) storage tokens,
        mapping(address token => FerrataVault.Bucket) storage depositCaps,
        mapping(address token => FerrataVault.Bucket) storage releaseCaps,
        mapping(address token => FerrataVault.DelayChange[]) storage delays,
        mapping(address token => mapping(uint256 index => FerrataVault.FeeSchedule)) storage schedules,
        address token,
        FerrataVault.TokenParams calldata p
    ) external {
        Limits memory limits = _limits();
        FerrataVault.TokenConfig storage cfg = tokens[token];
        if (cfg.registered) revert FerrataVault.TokenAlreadyRegistered();
        if (token != address(0) && token.code.length == 0) revert FerrataVault.TokenHasNoCode();
        _checkLezUnit(p.lezUnit, p.decimals);
        if (p.unitWeight == 0 || p.unitWeight > MAX_UNIT_WEIGHT) revert FerrataVault.InvalidUnitWeight();
        _checkDenominations(p.denominations, p.lezUnit, p.unitWeight);
        _checkRelayFees(p.relayFees, p.denominations, p.lezUnit);
        checkReleaseDelay(p.releaseDelay, limits.minReleaseDelay, limits.revokeWindow);
        _checkRegisteredFloor(p, limits);

        cfg.registered = true;
        cfg.depositsEnabled = true;
        cfg.lezUnit = p.lezUnit;
        cfg.unitWeight = p.unitWeight;
        cfg.denominations = p.denominations;
        cfg.relayFees = p.relayFees;
        initBucket(depositCaps[token], p.depositCap.capacity, p.depositCap.refillPerSecond);
        initBucket(releaseCaps[token], p.releaseCap.capacity, p.releaseCap.refillPerSecond);
        delays[token].push(FerrataVault.DelayChange({fromCheckpoint: 0, delay: p.releaseDelay}));

        emit FerrataVault.TokenRegistered(token, p.denominations, p.relayFees, p.lezUnit, p.unitWeight);
        emit FerrataVault.TokenCapSet(
            token, FerrataVault.Direction.Deposit, p.depositCap.capacity, p.depositCap.refillPerSecond
        );
        emit FerrataVault.TokenCapSet(
            token, FerrataVault.Direction.Release, p.releaseCap.capacity, p.releaseCap.refillPerSecond
        );
        emit FerrataVault.ReleaseDelaySet(token, p.releaseDelay, 0);
        _publishEntry(token, p);
        _initFees(schedules[token][0], token, p);
    }

    /// @notice Records the admin's announcement of the token's next fee schedule; the vault's `announceFees`
    ///         documents the rules. A due change takes effect first.
    function announceFees(
        mapping(address token => FerrataVault.TokenConfig) storage tokens,
        mapping(address token => FerrataVault.FeeState) storage states,
        mapping(address token => mapping(uint256 index => FerrataVault.FeeSchedule)) storage schedules,
        address token,
        uint256[] calldata mintFees,
        uint256[] calldata burnFees,
        uint256[] calldata checkpointFees
    ) external {
        FerrataVault.TokenConfig storage cfg = tokens[token];
        if (!cfg.registered) revert FerrataVault.TokenNotRegistered();
        uint256 index = applyFees(tokens, states, schedules, token) + 1;
        FerrataVault.FeeSchedule storage next = schedules[token][index];
        bool same = _storeFees(next, schedules[token][index - 1], cfg, mintFees, burnFees, checkpointFees);
        // Announcing the fees in force withdraws a pending announcement: nothing takes effect.
        uint64 effectiveAt = same ? 0 : uint64(block.timestamp + FEE_NOTICE);
        next.announcedAt = uint64(block.timestamp);
        next.effectiveAt = effectiveAt;
        states[token].pendingAt = effectiveAt;
        emit FerrataVault.FeesAnnounced(
            token, index, uint64(block.timestamp), effectiveAt, mintFees, burnFees, checkpointFees
        );
    }

    /// @notice Puts the token's announced fee schedule in force if it is due, from the current leaf count on, and
    ///         publishes it; returns the index of the schedule in force.
    function applyFees(
        mapping(address token => FerrataVault.TokenConfig) storage tokens,
        mapping(address token => FerrataVault.FeeState) storage states,
        mapping(address token => mapping(uint256 index => FerrataVault.FeeSchedule)) storage schedules,
        address token
    ) public returns (uint256 index) {
        FerrataVault.FeeState storage st = states[token];
        index = st.current;
        uint256 pendingAt = st.pendingAt;
        if (pendingAt == 0 || block.timestamp < pendingAt) return index;
        index += 1;
        FerrataVault.FeeSchedule storage s = schedules[token][index];
        uint64 fromLeaf = _leafCount();
        s.fromLeaf = fromLeaf;
        s.effectiveAt = uint64(block.timestamp);
        st.current = uint64(index);
        st.pendingAt = 0;
        bytes32 h = _publishSchedule(token, index, s, tokens[token].denominations.length);
        emit FerrataVault.FeesInForce(token, index, fromLeaf, uint64(block.timestamp), h);
    }

    /// @notice The leaf checks of the vault's `refund` (documented there) and the escrow it returns: the leaf exists,
    ///         its token and denomination are registered, and `path` folds it to `depositRoot` (direction bits = the
    ///         bits of `leafIndex`); the escrow is the denomination plus the mint and relay fees of its schedule.
    /// @dev Reads the leaf count and the root at their fixed slots (1 and 0, part of the cross-chain interface).
    function refundAmount(
        mapping(address token => FerrataVault.TokenConfig) storage tokens,
        mapping(address token => FerrataVault.FeeState) storage states,
        mapping(address token => mapping(uint256 index => FerrataVault.FeeSchedule)) storage schedules,
        uint64 leafIndex,
        address depositor,
        address token,
        uint8 denomIndex,
        bytes32 commitment,
        bytes32[32] calldata path
    ) external view returns (uint256 amount) {
        if (leafIndex >= _leafCount()) revert FerrataVault.UnknownLeaf();
        FerrataVault.TokenConfig storage cfg = tokens[token];
        if (!cfg.registered) revert FerrataVault.TokenNotRegistered();
        if (denomIndex >= cfg.denominations.length) revert FerrataVault.InvalidDenominationIndex();
        amount = cfg.denominations[denomIndex];
        bytes32 node = sha256(abi.encodePacked(LEAF_PREFIX, LEAF_TAG, leafIndex, depositor, token, amount, commitment));
        for (uint256 h = 0; h < 32; h++) {
            node = ((leafIndex >> h) & 1) == 1 ? _hashNode(path[h], node) : _hashNode(node, path[h]);
        }
        if (node != _depositRoot()) revert FerrataVault.InvalidMerklePath();
        amount += _inboundFeesOfLeaf(states, schedules, token, leafIndex, denomIndex);
    }

    /// @dev The mint and relay fees a deposit at `leafIndex` escrowed with its denomination: those of the last
    ///      schedule in force from a leaf at or below it (a deposit keeps the schedule of its leaf index, D13).
    ///      Binary search over the token's schedules 0..current, whose `fromLeaf` never decreases. Schedule 0 starts
    ///      at the leaf count of the registration, so every leaf of the token finds one.
    function _inboundFeesOfLeaf(
        mapping(address token => FerrataVault.FeeState) storage states,
        mapping(address token => mapping(uint256 index => FerrataVault.FeeSchedule)) storage schedules,
        address token,
        uint64 leafIndex,
        uint8 denomIndex
    ) private view returns (uint256) {
        mapping(uint256 index => FerrataVault.FeeSchedule) storage list = schedules[token];
        uint256 low = 0;
        uint256 high = uint256(states[token].current) + 1;
        while (low < high) {
            uint256 mid = (low + high) / 2;
            if (list[mid].fromLeaf <= leafIndex) low = mid + 1;
            else high = mid;
        }
        uint256 inbound = list[low - 1].inbound[denomIndex];
        return (inbound >> 128) + uint128(inbound);
    }

    /// @notice Updates a token's bucket (the vault's `setTokenCap` documents the rules); a release bucket stays at
    ///         or above the floor (D13).
    function setTokenCap(
        mapping(address token => FerrataVault.TokenConfig) storage tokens,
        mapping(address token => FerrataVault.Bucket) storage depositCaps,
        mapping(address token => FerrataVault.Bucket) storage releaseCaps,
        address token,
        FerrataVault.Direction direction,
        uint256 capacity,
        uint256 refillPerSecond
    ) external {
        FerrataVault.TokenConfig storage cfg = tokens[token];
        if (!cfg.registered) revert FerrataVault.TokenNotRegistered();
        if (direction == FerrataVault.Direction.Release) {
            Limits memory limits = _limits();
            checkReleaseFloor(
                capacity,
                refillPerSecond,
                cfg.unitWeight,
                cfg.lezUnit,
                limits.releaseCapFloor,
                limits.releaseRefillFloor
            );
        }
        _updateBucket(
            direction == FerrataVault.Direction.Deposit ? depositCaps[token] : releaseCaps[token],
            capacity,
            refillPerSecond
        );
        emit FerrataVault.TokenCapSet(token, direction, capacity, refillPerSecond);
    }

    /// @notice Updates a global bucket (value units), with the same rules.
    function setGlobalCap(
        FerrataVault.Bucket storage depositCap,
        FerrataVault.Bucket storage releaseCap,
        FerrataVault.Direction direction,
        uint256 capacity,
        uint256 refillPerSecond
    ) external {
        if (direction == FerrataVault.Direction.Release) {
            Limits memory limits = _limits();
            checkReleaseFloor(capacity, refillPerSecond, 1, 1, limits.releaseCapFloor, limits.releaseRefillFloor);
        }
        _updateBucket(direction == FerrataVault.Direction.Deposit ? depositCap : releaseCap, capacity, refillPerSecond);
        emit FerrataVault.GlobalCapSet(direction, capacity, refillPerSecond);
    }

    /// @notice The vault's `setReleaseDelay` (documented there): records `delay` for roots finalised from checkpoint
    ///         `fromCheckpoint` (the current count) on.
    function setReleaseDelay(
        mapping(address token => FerrataVault.TokenConfig) storage tokens,
        mapping(address token => FerrataVault.DelayChange[]) storage delays,
        address token,
        uint256 delay,
        uint256 fromCheckpoint
    ) external {
        if (!tokens[token].registered) revert FerrataVault.TokenNotRegistered();
        Limits memory limits = _limits();
        checkReleaseDelay(delay, limits.minReleaseDelay, limits.revokeWindow);
        delays[token].push(FerrataVault.DelayChange({fromCheckpoint: fromCheckpoint, delay: delay}));
        emit FerrataVault.ReleaseDelaySet(token, delay, fromCheckpoint);
    }

    /// @notice The effect of the vault's `cancelCandidate` (documented there; the vault checks the caller): deletes the
    ///         pending candidate `newState`.
    function cancelCandidate(mapping(bytes32 newState => FerrataVault.Candidate) storage candidates, bytes32 newState)
        external
    {
        FerrataVault.Candidate storage cand = candidates[newState];
        if (cand.submittedAt == 0) revert FerrataVault.UnknownCandidate();
        uint64 tipId = cand.tipId;
        bytes32 tipHash = cand.tipHash;
        delete candidates[newState];
        emit FerrataVault.CandidateCancelled(newState, tipId, tipHash);
    }

    /// @notice The vault's `claimOwed` (documented there; the vault holds the reentrancy guard): pays the caller what
    ///         it is owed in `token`, to itself only. ETH is sent without copying return data, as the vault's own
    ///         payments are.
    function claimOwed(
        mapping(address payee => mapping(address token => uint256)) storage owed,
        mapping(address token => uint256) storage totalOwed,
        address token
    ) external {
        uint256 amount = owed[msg.sender][token];
        if (amount == 0) revert FerrataVault.NothingOwed();
        owed[msg.sender][token] = 0;
        totalOwed[token] -= amount;
        emit FerrataVault.OwedClaimed(msg.sender, token, amount);
        if (token == address(0)) {
            bool ok;
            assembly ("memory-safe") {
                ok := call(gas(), caller(), amount, 0, 0, 0, 0)
            }
            if (!ok) revert FerrataVault.EthTransferFailed();
        } else {
            IERC20(token).safeTransfer(msg.sender, amount);
        }
    }

    /// @notice Records the admin's role proposal; the vault's section on timelocked role changes documents it.
    function proposeRoles(
        FerrataVault.RoleProposal storage p,
        address[3] calldata newWatchers,
        address newFreezeAuthority,
        address admin
    ) external {
        if (newFreezeAuthority == admin) revert FerrataVault.RolesMustDiffer();
        p.watchers = newWatchers;
        p.freezeAuthority = newFreezeAuthority;
        p.effectiveAt = block.timestamp + ROLE_TIMELOCK;
        emit FerrataVault.RolesProposed(newWatchers, newFreezeAuthority, p.effectiveAt);
    }

    /// @notice Installs a proposal whose timelock is over: the watcher set here, and returns the new freeze
    ///         authority (address(0): unchanged), which the vault installs. The proposal is cleared.
    function applyRoles(
        FerrataVault.RoleProposal storage p,
        address[3] storage watchers,
        address admin,
        address freezeAuthority
    ) external returns (address newAuthority) {
        uint256 effectiveAt = p.effectiveAt;
        if (effectiveAt == 0) revert FerrataVault.NoProposal();
        if (block.timestamp < effectiveAt) revert FerrataVault.TimelockNotOver();
        for (uint256 i = 0; i < 3; i++) {
            watchers[i] = p.watchers[i];
            p.watchers[i] = address(0);
        }
        emit FerrataVault.WatchersSet(watchers);
        newAuthority = p.freezeAuthority;
        if (newAuthority != address(0)) {
            if (newAuthority == admin) revert FerrataVault.RolesMustDiffer();
            emit FerrataVault.FreezeAuthorityTransferred(freezeAuthority, newAuthority);
        }
        p.freezeAuthority = address(0);
        p.effectiveAt = 0;
    }

    /// @notice The checks and the marking of `revokeFrom` (the vault documents it and restores the state): the
    ///         checkpoint exists, is not revoked yet, and was finalised at most `revokeWindow` ago; it and every
    ///         later one are marked revoked.
    /// @dev The loop is bounded: checkpoints after `index` were finalised after it, within the window, and two
    ///      finalisations are at least `deltaFloor` apart, so at most revokeWindow / deltaFloor + 1 iterations.
    function revokeFrom(FerrataVault.FinalizedCheckpoint[] storage checkpoints, uint256 index, uint256 revokeWindow)
        external
    {
        if (index >= checkpoints.length) revert FerrataVault.UnknownCheckpoint();
        FerrataVault.FinalizedCheckpoint storage first = checkpoints[index];
        if (first.revoked) revert FerrataVault.CheckpointAlreadyRevoked();
        if (block.timestamp > first.finalizedAt + revokeWindow) revert FerrataVault.RevokeWindowPassed();
        uint256 end = checkpoints.length;
        for (uint256 i = index; i < end; i++) {
            checkpoints[i].revoked = true;
        }
        emit FerrataVault.CheckpointsRevoked(index, end - 1, first.parentState);
    }

    /// @dev The deployment's bounds, read from the vault's own immutables: this library runs in the vault's context, so
    ///      `address(this)` is the vault. Read here rather than passed by the vault, which keeps the vault's runtime
    ///      under EIP-170 (admin calls only).
    function _limits() private view returns (Limits memory) {
        FerrataVault v = FerrataVault(payable(address(this)));
        return Limits(v.minReleaseDelay(), v.revokeWindow(), v.releaseCapFloor(), v.releaseRefillFloor());
    }

    /// @dev Stored level plus the refill since `lastUpdate`, saturating at capacity without overflowing:
    ///      if elapsed > missing / rate then elapsed * rate > missing, so the bucket is full; otherwise
    ///      elapsed * rate <= missing and the sum stays <= capacity.
    function currentLevel(FerrataVault.Bucket storage b) internal view returns (uint256) {
        uint256 elapsed = block.timestamp - b.lastUpdate;
        uint256 missing = b.capacity - b.level;
        if (b.refillPerSecond != 0 && elapsed > missing / b.refillPerSecond) return b.capacity;
        return b.level + elapsed * b.refillPerSecond;
    }

    /// @dev minReleaseDelay <= delay <= revokeWindow (v3 D5: a checkpoint stays revocable for as long as its
    ///      releases are delayed), and never above 30 days.
    function checkReleaseDelay(uint256 delay, uint256 minReleaseDelay, uint256 revokeWindow) internal pure {
        if (delay < minReleaseDelay || delay > revokeWindow || delay > MAX_RELEASE_DELAY) {
            revert FerrataVault.ReleaseDelayOutOfRange();
        }
    }

    /// @dev No release bucket may be set below the deployment's floor (D13), in value units: neither its capacity
    ///      nor its refill per second. An UNLIMITED bucket passes. `weight` and `unit` convert token units to value
    ///      units as the caps do (1 and 1 for the global bucket). The conversion saturates instead of overflowing,
    ///      since only `>= floor` is asked.
    function checkReleaseFloor(
        uint256 capacity,
        uint256 refillPerSecond,
        uint256 weight,
        uint256 unit,
        uint256 capacityFloor,
        uint256 refillFloor
    ) internal pure {
        if (capacity == UNLIMITED) return;
        if (_toValue(capacity, weight, unit) < capacityFloor || _toValue(refillPerSecond, weight, unit) < refillFloor) {
            revert FerrataVault.CapBelowFloor();
        }
    }

    /// @dev A newly created bucket starts full.
    function initBucket(FerrataVault.Bucket storage b, uint256 capacity, uint256 refillPerSecond) internal {
        b.capacity = capacity;
        b.refillPerSecond = refillPerSecond;
        b.level = capacity;
        b.lastUpdate = block.timestamp;
    }

    /// @dev Settles the refill earned under the old parameters, then clamps to the new capacity, so lowering a cap
    ///      takes effect at once and raising one adds no instant headroom. An UNLIMITED bucket has no tracked
    ///      level and is treated as full.
    function _updateBucket(FerrataVault.Bucket storage b, uint256 capacity, uint256 refillPerSecond) private {
        uint256 level = b.capacity == UNLIMITED ? capacity : currentLevel(b);
        if (level > capacity) level = capacity;
        b.capacity = capacity;
        b.refillPerSecond = refillPerSecond;
        b.level = level;
        b.lastUpdate = block.timestamp;
    }

    function _checkRegisteredFloor(FerrataVault.TokenParams calldata p, Limits memory limits) private pure {
        checkReleaseFloor(
            p.releaseCap.capacity,
            p.releaseCap.refillPerSecond,
            p.unitWeight,
            p.lezUnit,
            limits.releaseCapFloor,
            limits.releaseRefillFloor
        );
    }

    /// @dev amount * weight / unit, or the largest uint256 if the product would overflow.
    function _toValue(uint256 amount, uint256 weight, uint256 unit) private pure returns (uint256) {
        if (amount > type(uint256).max / weight) return type(uint256).max;
        return amount * weight / unit;
    }

    /// @dev lezUnit must be 10**k for some k >= 0, and at most 10**decimals (as the LEZ registry checks; a unit of
    ///      2**128 or more is already refused through the denominations, so decimals of 39 or more bound nothing).
    function _checkLezUnit(uint256 lezUnit, uint8 decimals) private pure {
        if (lezUnit == 0) revert FerrataVault.InvalidLezUnit();
        uint256 rest = lezUnit;
        while (rest % 10 == 0) {
            rest /= 10;
        }
        if (rest != 1 || (decimals < 39 && lezUnit > 10 ** decimals)) revert FerrataVault.InvalidLezUnit();
    }

    /// @dev One fee per denomination, each at most 1% of it (fee <= denomination / 100, i.e. fee * 100 <= it) and a
    ///      whole number of LEZ units, since the claim mints it on LEZ (D13).
    function _checkRelayFees(uint256[] calldata fees, uint256[] calldata denoms, uint256 lezUnit) private pure {
        if (fees.length != denoms.length) revert FerrataVault.RelayFeeCountMismatch();
        for (uint256 i = 0; i < fees.length; i++) {
            if (fees[i] > denoms[i] / FEE_CEILING_DIVISOR) revert FerrataVault.RelayFeeTooHigh();
            if (fees[i] % lezUnit != 0) revert FerrataVault.FeeNotMultipleOfLezUnit();
        }
    }

    /// @dev 1..8 entries, strictly increasing (so all non-zero), each <= MAX_DENOMINATION and a multiple of lezUnit;
    ///      twice the largest, in value units, fits 128 bits (the LEZ side's cap arithmetic).
    function _checkDenominations(uint256[] calldata denoms, uint256 lezUnit, uint256 unitWeight) private pure {
        if (denoms.length == 0 || denoms.length > MAX_DENOMINATIONS) {
            revert FerrataVault.InvalidDenominations();
        }
        uint256 previous = 0;
        for (uint256 i = 0; i < denoms.length; i++) {
            if (denoms[i] <= previous || denoms[i] > MAX_DENOMINATION) {
                revert FerrataVault.InvalidDenominations();
            }
            if (denoms[i] % lezUnit != 0) revert FerrataVault.DenominationNotMultipleOfLezUnit();
            previous = denoms[i];
        }
        // previous <= 2**127 - 1 and unitWeight < 2**128: no overflow.
        if (2 * previous / lezUnit * unitWeight > type(uint128).max) revert FerrataVault.InvalidUnitWeight();
    }

    /// @dev Fee schedule 0 (D13): no mint or burn fee ("protocol fees start at zero"), the registered relay fees and
    ///      the given checkpoint fees; in force from the current leaf count, announced and effective now.
    function _initFees(FerrataVault.FeeSchedule storage s, address token, FerrataVault.TokenParams calldata p)
        private
    {
        uint256 k = p.denominations.length;
        if (p.checkpointFees.length != k) revert FerrataVault.FeeCountMismatch();
        for (uint256 i = 0; i < k; i++) {
            if (p.checkpointFees[i] > p.denominations[i] / FEE_CEILING_DIVISOR) revert FerrataVault.FeeTooHigh();
            s.inbound[i] = p.relayFees[i];
            s.outbound[i] = p.checkpointFees[i];
        }
        uint64 fromLeaf = _leafCount();
        s.fromLeaf = fromLeaf;
        s.announcedAt = uint64(block.timestamp);
        s.effectiveAt = uint64(block.timestamp);
        uint256[] memory zeros = new uint256[](k);
        emit FerrataVault.FeesAnnounced(
            token, 0, uint64(block.timestamp), uint64(block.timestamp), zeros, zeros, p.checkpointFees
        );
        emit FerrataVault.FeesInForce(token, 0, fromLeaf, uint64(block.timestamp), _publishSchedule(token, 0, s, k));
    }

    /// @dev Checks an announced schedule (one fee of each kind per denomination, each at most 1% of it, mint fees in
    ///      whole LEZ units), writes it packed into `next` with the registered relay fees, and returns whether it
    ///      equals the schedule in force, `current`.
    function _storeFees(
        FerrataVault.FeeSchedule storage next,
        FerrataVault.FeeSchedule storage current,
        FerrataVault.TokenConfig storage cfg,
        uint256[] calldata mintFees,
        uint256[] calldata burnFees,
        uint256[] calldata checkpointFees
    ) private returns (bool same) {
        uint256 k = cfg.denominations.length;
        if (mintFees.length != k || burnFees.length != k || checkpointFees.length != k) {
            revert FerrataVault.FeeCountMismatch();
        }
        same = true;
        for (uint256 i = 0; i < k; i++) {
            uint256 ceiling = cfg.denominations[i] / FEE_CEILING_DIVISOR;
            if (mintFees[i] > ceiling || burnFees[i] > ceiling || checkpointFees[i] > ceiling) {
                revert FerrataVault.FeeTooHigh();
            }
            if (mintFees[i] % cfg.lezUnit != 0) revert FerrataVault.FeeNotMultipleOfLezUnit();
            uint256 inbound = mintFees[i] << 128 | cfg.relayFees[i];
            uint256 outbound = burnFees[i] << 128 | checkpointFees[i];
            if (inbound != current.inbound[i] || outbound != current.outbound[i]) same = false;
            next.inbound[i] = inbound;
            next.outbound[i] = outbound;
        }
    }

    /// @dev Publishes the registry entry: sha256("ferrata/registry/v1" ‖ token ‖ lezDefinition ‖ decimals:u8 ‖
    ///      lezUnit:u256 ‖ unitWeight:u256 ‖ k:u8 ‖ denominations[k]:u256 ‖ relayFees[k]:u256) at slot
    ///      sha256("ferrata/vault/registry/v1" ‖ token) (bridge-wire `TokenEntry::hash`, `registry_slot`).
    function _publishEntry(address token, FerrataVault.TokenParams calldata p) private {
        bytes32 h = sha256(
            abi.encodePacked(
                "ferrata/registry/v1",
                token,
                p.lezDefinition,
                p.decimals,
                p.lezUnit,
                p.unitWeight,
                uint8(p.denominations.length),
                p.denominations,
                p.relayFees
            )
        );
        _store(sha256(abi.encodePacked("ferrata/vault/registry/v1", token)), h);
        emit FerrataVault.RegistryEntryPublished(token, p.lezDefinition, p.decimals, h);
    }

    /// @dev Publishes schedule `index` in force: sha256("ferrata/fees/v1" ‖ token ‖ index:u64 ‖ fromLeaf:u64 ‖
    ///      announcedAt:u64 ‖ effectiveAt:u64 ‖ k:u8 ‖ mintFees[k] ‖ burnFees[k] ‖ checkpointFees[k]) (u256 words) at
    ///      slot sha256("ferrata/vault/fees/v1" ‖ token ‖ index:u64) (bridge-wire `FeeSchedule::hash`, `fee_slot`).
    function _publishSchedule(address token, uint256 index, FerrataVault.FeeSchedule storage s, uint256 k)
        private
        returns (bytes32 h)
    {
        h = sha256(
            abi.encodePacked(
                "ferrata/fees/v1",
                token,
                uint64(index),
                s.fromLeaf,
                s.announcedAt,
                s.effectiveAt,
                uint8(k),
                _feeWords(s, k)
            )
        );
        _store(sha256(abi.encodePacked("ferrata/vault/fees/v1", token, uint64(index))), h);
    }

    /// @dev mintFees[k] ‖ burnFees[k] ‖ checkpointFees[k] as u256 words, unpacked from `s`.
    function _feeWords(FerrataVault.FeeSchedule storage s, uint256 k) private view returns (bytes memory) {
        uint256[] memory mint = new uint256[](k);
        uint256[] memory burn = new uint256[](k);
        uint256[] memory checkpoint = new uint256[](k);
        for (uint256 i = 0; i < k; i++) {
            mint[i] = s.inbound[i] >> 128;
            burn[i] = s.outbound[i] >> 128;
            checkpoint[i] = uint128(s.outbound[i]);
        }
        return abi.encodePacked(mint, burn, checkpoint);
    }

    function _store(bytes32 slot, bytes32 value) private {
        assembly ("memory-safe") {
            sstore(slot, value)
        }
    }

    /// @dev FerrataVault's inner node: sha256(NODE_PREFIX ‖ left ‖ right).
    function _hashNode(bytes32 left, bytes32 right) private pure returns (bytes32) {
        return sha256(abi.encodePacked(NODE_PREFIX, left, right));
    }

    function _depositRoot() private view returns (bytes32 root) {
        assembly ("memory-safe") {
            root := sload(DEPOSIT_ROOT_SLOT)
        }
    }

    function _leafCount() private view returns (uint64 count) {
        assembly ("memory-safe") {
            count := and(sload(LEAF_COUNT_SLOT), 0xffffffffffffffff)
        }
    }
}
