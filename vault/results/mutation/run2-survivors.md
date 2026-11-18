## registry-115 survived src/FerrataRegistry.sol:112 DeleteExpressionMutation

```diff
@@ -112,7 +112,8 @@
         // Announcing the fees in force withdraws a pending announcement: nothing takes effect.
         uint64 effectiveAt = same ? 0 : uint64(block.timestamp + FEE_NOTICE);
         next.announcedAt = uint64(block.timestamp);
-        next.effectiveAt = effectiveAt;
+        /// DeleteExpressionMutation(`next.effectiveAt = effectiveAt` |==> `assert(true)`) of: `next.effectiveAt = effectiveAt;`
+        assert(true);
         states[token].pendingAt = effectiveAt;
         emit FerrataVault.FeesAnnounced(
             token, index, uint64(block.timestamp), effectiveAt, mintFees, burnFees, checkpointFees
```

## registry-116 survived src/FerrataRegistry.sol:112 AssignmentMutation

```diff
@@ -112,7 +112,8 @@
         // Announcing the fees in force withdraws a pending announcement: nothing takes effect.
         uint64 effectiveAt = same ? 0 : uint64(block.timestamp + FEE_NOTICE);
         next.announcedAt = uint64(block.timestamp);
-        next.effectiveAt = effectiveAt;
+        /// AssignmentMutation(`effectiveAt` |==> `0`) of: `next.effectiveAt = effectiveAt;`
+        next.effectiveAt = 0;
         states[token].pendingAt = effectiveAt;
         emit FerrataVault.FeesAnnounced(
             token, index, uint64(block.timestamp), effectiveAt, mintFees, burnFees, checkpointFees
```

## registry-118 survived src/FerrataRegistry.sol:112 AssignmentMutation

```diff
@@ -112,7 +112,8 @@
         // Announcing the fees in force withdraws a pending announcement: nothing takes effect.
         uint64 effectiveAt = same ? 0 : uint64(block.timestamp + FEE_NOTICE);
         next.announcedAt = uint64(block.timestamp);
-        next.effectiveAt = effectiveAt;
+        /// AssignmentMutation(`effectiveAt` |==> `1`) of: `next.effectiveAt = effectiveAt;`
+        next.effectiveAt = 1;
         states[token].pendingAt = effectiveAt;
         emit FerrataVault.FeesAnnounced(
             token, index, uint64(block.timestamp), effectiveAt, mintFees, burnFees, checkpointFees
```

## registry-283 survived src/FerrataRegistry.sol:187 AssignmentMutation

```diff
@@ -187,7 +187,8 @@
         while (low < high) {
             uint256 mid = (low + high) / 2;
             if (list[mid].fromLeaf <= leafIndex) low = mid + 1;
-            else high = mid;
+            /// AssignmentMutation(`mid` |==> `1`) of: `else high = mid;`
+            else high = 1;
         }
         uint256 inbound = list[low - 1].inbound[denomIndex];
         return (inbound >> 128) + uint128(inbound);
```

## registry-463 survived src/FerrataRegistry.sol:321 DeleteExpressionMutation

```diff
@@ -321,7 +321,8 @@
         if (block.timestamp < effectiveAt) revert FerrataVault.TimelockNotOver();
         for (uint256 i = 0; i < 3; i++) {
             watchers[i] = p.watchers[i];
-            p.watchers[i] = address(0);
+            /// DeleteExpressionMutation(`p.watchers[i] = address(0)` |==> `assert(true)`) of: `p.watchers[i] = address(0);`
+            assert(true);
         }
         emit FerrataVault.WatchersSet(watchers);
         newAuthority = p.freezeAuthority;
```

## registry-484 survived src/FerrataRegistry.sol:325 IfStatementMutation

```diff
@@ -325,7 +325,8 @@
         }
         emit FerrataVault.WatchersSet(watchers);
         newAuthority = p.freezeAuthority;
-        if (newAuthority != address(0)) {
+        /// IfStatementMutation(`newAuthority != address(0)` |==> `true`) of: `if (newAuthority != address(0)) {`
+        if (true) {
             if (newAuthority == admin) revert FerrataVault.RolesMustDiffer();
             emit FerrataVault.FreezeAuthorityTransferred(freezeAuthority, newAuthority);
         }
```

## registry-686 survived src/FerrataRegistry.sol:401 AssignmentMutation

```diff
@@ -401,7 +401,8 @@
         b.capacity = capacity;
         b.refillPerSecond = refillPerSecond;
         b.level = capacity;
-        b.lastUpdate = block.timestamp;
+        /// AssignmentMutation(`block.timestamp` |==> `1`) of: `b.lastUpdate = block.timestamp;`
+        b.lastUpdate = 1;
     }
 
     /// @dev Settles the refill earned under the old parameters, then clamps to the new capacity, so lowering a cap
```

## registry-728 survived src/FerrataRegistry.sol:413 DeleteExpressionMutation

```diff
@@ -413,7 +413,8 @@
         b.capacity = capacity;
         b.refillPerSecond = refillPerSecond;
         b.level = level;
-        b.lastUpdate = block.timestamp;
+        /// DeleteExpressionMutation(`b.lastUpdate = block.timestamp` |==> `assert(true)`) of: `b.lastUpdate = block.timestamp;`
+        assert(true);
     }
 
     function _checkRegisteredFloor(FerrataVault.TokenParams calldata p, Limits memory limits) private pure {
```

## registry-731 survived src/FerrataRegistry.sol:413 AssignmentMutation

```diff
@@ -413,7 +413,8 @@
         b.capacity = capacity;
         b.refillPerSecond = refillPerSecond;
         b.level = level;
-        b.lastUpdate = block.timestamp;
+        /// AssignmentMutation(`block.timestamp` |==> `1`) of: `b.lastUpdate = block.timestamp;`
+        b.lastUpdate = 1;
     }
 
     function _checkRegisteredFloor(FerrataVault.TokenParams calldata p, Limits memory limits) private pure {
```

## registry-1021 survived src/FerrataRegistry.sol:486 AssignmentMutation

```diff
@@ -486,7 +486,8 @@
         }
         uint64 fromLeaf = _leafCount();
         s.fromLeaf = fromLeaf;
-        s.announcedAt = uint64(block.timestamp);
+        /// AssignmentMutation(`uint64(block.timestamp)` |==> `1`) of: `s.announcedAt = uint64(block.timestamp);`
+        s.announcedAt = 1;
         s.effectiveAt = uint64(block.timestamp);
         uint256[] memory zeros = new uint256[](k);
         emit FerrataVault.FeesAnnounced(
```

## registry-1027 survived src/FerrataRegistry.sol:487 AssignmentMutation

```diff
@@ -487,7 +487,8 @@
         uint64 fromLeaf = _leafCount();
         s.fromLeaf = fromLeaf;
         s.announcedAt = uint64(block.timestamp);
-        s.effectiveAt = uint64(block.timestamp);
+        /// AssignmentMutation(`uint64(block.timestamp)` |==> `1`) of: `s.effectiveAt = uint64(block.timestamp);`
+        s.effectiveAt = 1;
         uint256[] memory zeros = new uint256[](k);
         emit FerrataVault.FeesAnnounced(
             token, 0, uint64(block.timestamp), uint64(block.timestamp), zeros, zeros, p.checkpointFees
```

## vault-430 survived src/FerrataVault.sol:644 AssignmentMutation

```diff
@@ -644,7 +644,8 @@
         admin = d.admin;
         freezeAuthority = d.freezeAuthority;
         watchers = d.watchers;
-        keylessFrom = uint64(block.timestamp);
+        /// AssignmentMutation(`uint64(block.timestamp)` |==> `1`) of: `keylessFrom = uint64(block.timestamp);`
+        keylessFrom = 1;
         FerrataRegistry.initBucket(_globalDepositCap, d.globalDepositCap.capacity, d.globalDepositCap.refillPerSecond);
         FerrataRegistry.initBucket(_globalReleaseCap, d.globalReleaseCap.capacity, d.globalReleaseCap.refillPerSecond);
 
```

## vault-719 survived src/FerrataVault.sol:833 BinaryOpMutation

```diff
@@ -833,7 +833,8 @@
         delete candidates[c.newState];
 
         uint256 previous = burnRootCheckpoint[c.burnRoot];
-        if (previous == 0 || checkpoints[previous - 1].revoked) burnRootCheckpoint[c.burnRoot] = index + 1;
+        /// BinaryOpMutation(`-` |==> `%`) of: `if (previous == 0 || checkpoints[previous - 1].revoked) burnRootCheckpoint[c.burnRoot] = index + 1;`
+        if (previous == 0 || checkpoints[previous%1].revoked) burnRootCheckpoint[c.burnRoot] = index + 1;
         // The first sunset checkpoint fixes the boundary; later ones carry the same count (checked above).
         if (c.sunset && !sunsetLeafCountSet) {
             sunsetLeafCountSet = true;
```

## vault-794 survived src/FerrataVault.sol:869 IfStatementMutation

```diff
@@ -869,7 +869,8 @@
 
         uint256 deducted = _creditReleaseFees(r.token, r.denomIndex, prover);
         _payOrCredit(r.token, r.recipient, amount - r.relayFee - deducted);
-        if (r.relayFee != 0) _payOrCredit(r.token, feePayee, r.relayFee);
+        /// IfStatementMutation(`r.relayFee != 0` |==> `true`) of: `if (r.relayFee != 0) _payOrCredit(r.token, feePayee, r.relayFee);`
+        if (true) _payOrCredit(r.token, feePayee, r.relayFee);
     }
 
     /// @notice Withdraws what the vault owes the caller in `token` after a failed push payment. Only the payee can
```

## vault-1061 survived src/FerrataVault.sol:1053 DeleteExpressionMutation

```diff
@@ -1053,7 +1053,8 @@
         if (sunsetLeafCountSet && sunsetSetAt >= index) {
             sunsetLeafCountSet = false;
             sunsetLeafCount = 0;
-            sunsetSetAt = 0;
+            /// DeleteExpressionMutation(`sunsetSetAt = 0` |==> `assert(true)`) of: `sunsetSetAt = 0;`
+            assert(true);
             sunsetFinalizedAt = 0;
             emit SunsetLeafCountCleared();
         }
```

## vault-1063 survived src/FerrataVault.sol:1053 AssignmentMutation

```diff
@@ -1053,7 +1053,8 @@
         if (sunsetLeafCountSet && sunsetSetAt >= index) {
             sunsetLeafCountSet = false;
             sunsetLeafCount = 0;
-            sunsetSetAt = 0;
+            /// AssignmentMutation(`0` |==> `1`) of: `sunsetSetAt = 0;`
+            sunsetSetAt = 1;
             sunsetFinalizedAt = 0;
             emit SunsetLeafCountCleared();
         }
```

## vault-1367 survived src/FerrataVault.sol:1255 BinaryOpMutation

```diff
@@ -1255,7 +1255,8 @@
 
     function _recordBeacon() internal returns (uint256 index) {
         index = beacons.length;
-        if (index != 0 && _lastBeaconBlock == block.number) return index - 1;
+        /// BinaryOpMutation(`-` |==> `%`) of: `if (index != 0 && _lastBeaconBlock == block.number) return index - 1;`
+        if (index != 0 && _lastBeaconBlock == block.number) return index%1;
         _lastBeaconBlock = block.number;
         // The expression spec §4.3 states; kept readable rather than hand-optimised.
         // forge-lint: disable-next-line(asm-keccak256)
```

## vault-1373 survived src/FerrataVault.sol:1256 AssignmentMutation

```diff
@@ -1256,7 +1256,8 @@
     function _recordBeacon() internal returns (uint256 index) {
         index = beacons.length;
         if (index != 0 && _lastBeaconBlock == block.number) return index - 1;
-        _lastBeaconBlock = block.number;
+        /// AssignmentMutation(`block.number` |==> `1`) of: `_lastBeaconBlock = block.number;`
+        _lastBeaconBlock = 1;
         // The expression spec §4.3 states; kept readable rather than hand-optimised.
         // forge-lint: disable-next-line(asm-keccak256)
         bytes32 nonce = keccak256(abi.encode(block.prevrandao, block.number, address(this)));
```

## vault-1580 survived src/FerrataVault.sol:1366 BinaryOpMutation

```diff
@@ -1366,7 +1366,8 @@
         uint256 entry = burnRootCheckpoint[r.burnRoot];
         if (entry == 0 || checkpoints[entry - 1].revoked) revert BurnRootNotAccepted();
         uint256 delay = _releaseDelayFor(r.token, entry - 1);
-        prover = checkpoints[entry - 1].prover;
+        /// BinaryOpMutation(`-` |==> `%`) of: `prover = checkpoints[entry - 1].prover;`
+        prover = checkpoints[entry%1].prover;
         uint256 opensAt = checkpoints[entry - 1].finalizedAt + delay;
         // D11: the last second in which the root's checkpoint can be revoked is never a release second.
         if (delay == revokeWindow) opensAt += 1;
```

## vault-1761 survived src/FerrataVault.sol:1441 IfStatementMutation

```diff
@@ -1441,7 +1441,8 @@
         FeeState storage st = feeState[token];
         index = st.current;
         uint256 pendingAt = st.pendingAt;
-        if (pendingAt != 0 && block.timestamp >= pendingAt) {
+        /// IfStatementMutation(`pendingAt != 0 && block.timestamp >= pendingAt` |==> `true`) of: `if (pendingAt != 0 && block.timestamp >= pendingAt) {`
+        if (true) {
             index = FerrataRegistry.applyFees(_tokens, feeState, feeSchedules, token);
         }
     }
```

