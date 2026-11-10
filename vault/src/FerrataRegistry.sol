// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "./FerrataVault.sol";

/// @title FerrataRegistry
/// @notice Token registration, cap settings and role proposals for FerrataVault, as a linked library: with the v4 features the vault's runtime would
///         exceed EIP-170's 24,576 bytes (SPEC-v3-delta D11). Its external functions run by DELEGATECALL in the
///         vault's storage and are reached only through the vault's onlyAdmin functions of the same names; the
///         library has no state. The
///         internal functions are compiled into whichever contract calls them.
library FerrataRegistry {
    // FerrataVault's constants of the same names (a contract's constants are not reachable from a library); the
    // registration and cap tests pin each against the vault's public value.
    uint256 private constant MAX_DENOMINATIONS = 8;
    uint256 private constant MAX_DENOMINATION = type(uint128).max;
    uint256 private constant MAX_UNIT_WEIGHT = type(uint128).max;
    uint256 private constant MAX_RELEASE_DELAY = 30 days;
    uint256 private constant UNLIMITED = type(uint256).max;
    uint256 private constant ROLE_TIMELOCK = 14 days; // D13

    /// @notice The deployment's bounds a registration must respect.
    struct Limits {
        uint256 minReleaseDelay;
        uint256 revokeWindow;
        uint256 releaseCapFloor;
        uint256 releaseRefillFloor;
    }

    /// @notice Validates and stores a registration; the rules are documented at the vault's `registerToken`.
    function registerToken(
        mapping(address token => FerrataVault.TokenConfig) storage tokens,
        mapping(address token => FerrataVault.Bucket) storage depositCaps,
        mapping(address token => FerrataVault.Bucket) storage releaseCaps,
        mapping(address token => FerrataVault.DelayChange[]) storage delays,
        address token,
        FerrataVault.TokenParams calldata p,
        Limits memory limits
    ) external {
        FerrataVault.TokenConfig storage cfg = tokens[token];
        if (cfg.registered) revert FerrataVault.TokenAlreadyRegistered();
        if (token != address(0) && token.code.length == 0) revert FerrataVault.TokenHasNoCode();
        _checkLezUnit(p.lezUnit);
        if (p.unitWeight == 0 || p.unitWeight > MAX_UNIT_WEIGHT) revert FerrataVault.InvalidUnitWeight();
        _checkDenominations(p.denominations, p.lezUnit);
        _checkRelayFees(p.relayFees, p.denominations);
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
        uint256 refillPerSecond,
        Limits memory limits
    ) external {
        FerrataVault.TokenConfig storage cfg = tokens[token];
        if (!cfg.registered) revert FerrataVault.TokenNotRegistered();
        if (direction == FerrataVault.Direction.Release) {
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
        uint256 refillPerSecond,
        Limits memory limits
    ) external {
        if (direction == FerrataVault.Direction.Release) {
            checkReleaseFloor(capacity, refillPerSecond, 1, 1, limits.releaseCapFloor, limits.releaseRefillFloor);
        }
        _updateBucket(direction == FerrataVault.Direction.Deposit ? depositCap : releaseCap, capacity, refillPerSecond);
        emit FerrataVault.GlobalCapSet(direction, capacity, refillPerSecond);
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

    /// @dev lezUnit must be 10**k for some k >= 0.
    function _checkLezUnit(uint256 lezUnit) private pure {
        if (lezUnit == 0) revert FerrataVault.InvalidLezUnit();
        uint256 rest = lezUnit;
        while (rest % 10 == 0) {
            rest /= 10;
        }
        if (rest != 1) revert FerrataVault.InvalidLezUnit();
    }

    /// @dev One fee per denomination, each at most 1% of it (fee <= denomination / 100, i.e. fee * 100 <= it).
    function _checkRelayFees(uint256[] calldata fees, uint256[] calldata denoms) private pure {
        if (fees.length != denoms.length) revert FerrataVault.RelayFeeCountMismatch();
        for (uint256 i = 0; i < fees.length; i++) {
            if (fees[i] > denoms[i] / 100) revert FerrataVault.RelayFeeTooHigh();
        }
    }

    /// @dev 1..8 entries, strictly increasing (so all non-zero), each <= MAX_DENOMINATION and a multiple of lezUnit.
    function _checkDenominations(uint256[] calldata denoms, uint256 lezUnit) private pure {
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
    }
}
