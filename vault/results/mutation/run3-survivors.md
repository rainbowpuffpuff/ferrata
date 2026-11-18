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

