'forge clean' running (wd: ./vault)
'forge config --json' running
'forge build --build-info --skip ./test/** ./script/** --force' running (wd: ./vault)
**THIS CHECKLIST IS NOT COMPLETE**. Use `--show-ignored-findings` to show all the results.
Summary
 - [encode-packed-collision](#encode-packed-collision) (2 results) (High)
 - [divide-before-multiply](#divide-before-multiply) (1 results) (Medium)
 - [incorrect-equality](#incorrect-equality) (3 results) (Medium)
 - [missing-zero-check](#missing-zero-check) (1 results) (Low)
 - [timestamp](#timestamp) (18 results) (Low)
 - [assembly](#assembly) (5 results) (Informational)
 - [cyclomatic-complexity](#cyclomatic-complexity) (2 results) (Informational)
 - [unused-state](#unused-state) (4 results) (Informational)
 - [immutable-states](#immutable-states) (1 results) (Optimization)
## encode-packed-collision
Impact: High
Confidence: High
 - [ ] ID-0
[FerrataRegistry._feeWords(FerrataVault.FeeSchedule,uint256)](src/FerrataRegistry.sol#L572-L582) calls abi.encodePacked() with multiple dynamic arguments:
	- [abi.encodePacked(mint,burn,checkpoint)](src/FerrataRegistry.sol#L581)

src/FerrataRegistry.sol#L572-L582


 - [ ] ID-1
[FerrataRegistry._publishEntry(address,FerrataVault.TokenParams)](src/FerrataRegistry.sol#L531-L547) calls abi.encodePacked() with multiple dynamic arguments:
	- [h = sha256(bytes)(abi.encodePacked(ferrata/registry/v1,token,p.lezDefinition,p.decimals,p.lezUnit,p.unitWeight,uint8(p.denominations.length),p.denominations,p.relayFees))](src/FerrataRegistry.sol#L532-L544)

src/FerrataRegistry.sol#L531-L547


## divide-before-multiply
Impact: Medium
Confidence: Medium
 - [ ] ID-2
[FerrataRegistry._checkDenominations(uint256[],uint256,uint256)](src/FerrataRegistry.sol#L459-L473) performs a multiplication on the result of a division:
	- [2 * previous / lezUnit * unitWeight > type()(uint128).max](src/FerrataRegistry.sol#L472)

src/FerrataRegistry.sol#L459-L473


## incorrect-equality
Impact: Medium
Confidence: High
 - [ ] ID-3
[FerrataVault._recordBeacon()](src/FerrataVault.sol#L1256-L1265) uses a dangerous strict equality:
	- [index != 0 && _lastBeaconBlock == block.number](src/FerrataVault.sol#L1258)

src/FerrataVault.sol#L1256-L1265


 - [ ] ID-4
[FerrataVault._finalize(FerrataVault.Checkpoint,address)](src/FerrataVault.sol#L811-L846) uses a dangerous strict equality:
	- [previous == 0 || checkpoints[previous - 1].revoked](src/FerrataVault.sol#L836)

src/FerrataVault.sol#L811-L846


 - [ ] ID-5
[FerrataVault.burnRootStatus(bytes32)](src/FerrataVault.sol#L1144-L1148) uses a dangerous strict equality:
	- [entry == 0 || checkpoints[entry - 1].revoked](src/FerrataVault.sol#L1146)

src/FerrataVault.sol#L1144-L1148


## missing-zero-check
Impact: Low
Confidence: Medium
 - [ ] ID-6
[FerrataVault.transferAdmin(address).newAdmin](src/FerrataVault.sol#L1068) lacks a zero-check on :
		- [pendingAdmin = newAdmin](src/FerrataVault.sol#L1069)

src/FerrataVault.sol#L1068


## timestamp
Impact: Low
Confidence: Medium
 - [ ] ID-7
[FerrataVault.retireOpensAt()](src/FerrataVault.sol#L1007-L1015) uses timestamp for comparisons
	Dangerous comparisons:
	- [checkpointsFrozenUntil > from](src/FerrataVault.sol#L1009)
	- [sunsetOpens < opensAt](src/FerrataVault.sol#L1013)

src/FerrataVault.sol#L1007-L1015


 - [ ] ID-8
[FerrataRegistry.revokeFrom(FerrataVault.FinalizedCheckpoint[],uint256,uint256)](src/FerrataRegistry.sol#L341-L353) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp > first.finalizedAt + revokeWindow](src/FerrataRegistry.sol#L347)

src/FerrataRegistry.sol#L341-L353


 - [ ] ID-9
[FerrataVault.retire()](src/FerrataVault.sol#L995-L1000) uses timestamp for comparisons
	Dangerous comparisons:
	- [msg.sender != freezeAuthority && block.timestamp < retireOpensAt()](src/FerrataVault.sol#L996)

src/FerrataVault.sol#L995-L1000


 - [ ] ID-10
[FerrataVault.finalizeAttested(FerrataVault.Checkpoint)](src/FerrataVault.sol#L799-L807) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp < cand.submittedAt + _wait(cand)](src/FerrataVault.sol#L802)

src/FerrataVault.sol#L799-L807


 - [ ] ID-11
[FerrataRegistry._updateBucket(FerrataVault.Bucket,uint256,uint256)](src/FerrataRegistry.sol#L410-L417) uses timestamp for comparisons
	Dangerous comparisons:
	- [level > capacity](src/FerrataRegistry.sol#L412)

src/FerrataRegistry.sol#L410-L417


 - [ ] ID-12
[FerrataRegistry.applyFees(mapping(address => FerrataVault.TokenConfig),mapping(address => FerrataVault.FeeState),mapping(address => mapping(uint256 => FerrataVault.FeeSchedule)),address)](src/FerrataRegistry.sol#L124-L143) uses timestamp for comparisons
	Dangerous comparisons:
	- [pendingAt == 0 || block.timestamp < pendingAt](src/FerrataRegistry.sol#L133)

src/FerrataRegistry.sol#L124-L143


 - [ ] ID-13
[FerrataVault.refund(uint64,address,address,uint8,bytes32,bytes32[32])](src/FerrataVault.sol#L707-L733) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp < _refundsOpenAt()](src/FerrataVault.sol#L719)

src/FerrataVault.sol#L707-L733


 - [ ] ID-14
[FerrataVault._finalize(FerrataVault.Checkpoint,address)](src/FerrataVault.sol#L811-L846) uses timestamp for comparisons
	Dangerous comparisons:
	- [previous == 0 || checkpoints[previous - 1].revoked](src/FerrataVault.sol#L836)

src/FerrataVault.sol#L811-L846


 - [ ] ID-15
[FerrataVault.revokeFrom(uint256)](src/FerrataVault.sol#L1045-L1060) uses timestamp for comparisons
	Dangerous comparisons:
	- [sunsetLeafCountSet && sunsetSetAt >= index](src/FerrataVault.sol#L1053)

src/FerrataVault.sol#L1045-L1060


 - [ ] ID-16
[FerrataVault._feesInForce(address)](src/FerrataVault.sol#L1440-L1447) uses timestamp for comparisons
	Dangerous comparisons:
	- [pendingAt != 0 && block.timestamp >= pendingAt](src/FerrataVault.sol#L1444)

src/FerrataVault.sol#L1440-L1447


 - [ ] ID-17
[FerrataVault.pauseCheckpoints()](src/FerrataVault.sol#L1034-L1039) uses timestamp for comparisons
	Dangerous comparisons:
	- [until > checkpointsFrozenUntil](src/FerrataVault.sol#L1037)

src/FerrataVault.sol#L1034-L1039


 - [ ] ID-18
[FerrataVault.setCheckpointsFrozen(bool)](src/FerrataVault.sol#L973-L978) uses timestamp for comparisons
	Dangerous comparisons:
	- [until > block.timestamp](src/FerrataVault.sol#L975)

src/FerrataVault.sol#L973-L978


 - [ ] ID-19
[FerrataVault._checkRelease(FerrataVault.ReleaseRequest)](src/FerrataVault.sol#L1357-L1378) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp < opensAt](src/FerrataVault.sol#L1373)

src/FerrataVault.sol#L1357-L1378


 - [ ] ID-20
[FerrataRegistry.currentLevel(FerrataVault.Bucket)](src/FerrataRegistry.sol#L366-L371) uses timestamp for comparisons
	Dangerous comparisons:
	- [b.refillPerSecond != 0 && elapsed > missing / b.refillPerSecond](src/FerrataRegistry.sol#L369)

src/FerrataRegistry.sol#L366-L371


 - [ ] ID-21
[FerrataVault._frozen(uint256)](src/FerrataVault.sol#L1510-L1512) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp < until](src/FerrataVault.sol#L1511)

src/FerrataVault.sol#L1510-L1512


 - [ ] ID-22
[FerrataRegistry.applyRoles(FerrataVault.RoleProposal,address[3],address,address)](src/FerrataRegistry.sol#L313-L334) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp < effectiveAt](src/FerrataRegistry.sol#L321)

src/FerrataRegistry.sol#L313-L334


 - [ ] ID-23
[FerrataVault.burnRootStatus(bytes32)](src/FerrataVault.sol#L1144-L1148) uses timestamp for comparisons
	Dangerous comparisons:
	- [entry == 0 || checkpoints[entry - 1].revoked](src/FerrataVault.sol#L1146)

src/FerrataVault.sol#L1144-L1148


 - [ ] ID-24
[FerrataVault._recordBeacon()](src/FerrataVault.sol#L1256-L1265) uses timestamp for comparisons
	Dangerous comparisons:
	- [index != 0 && _lastBeaconBlock == block.number](src/FerrataVault.sol#L1258)

src/FerrataVault.sol#L1256-L1265


## assembly
Impact: Informational
Confidence: High
 - [ ] ID-25
[FerrataRegistry.claimOwed(mapping(address => mapping(address => uint256)),mapping(address => uint256),address)](src/FerrataRegistry.sol#L276-L295) uses assembly
	- [INLINE ASM](src/FerrataRegistry.sol#L288-L290)

src/FerrataRegistry.sol#L276-L295


 - [ ] ID-26
[FerrataRegistry._leafCount()](src/FerrataRegistry.sol#L601-L605) uses assembly
	- [INLINE ASM](src/FerrataRegistry.sol#L602-L604)

src/FerrataRegistry.sol#L601-L605


 - [ ] ID-27
[FerrataRegistry._depositRoot()](src/FerrataRegistry.sol#L595-L599) uses assembly
	- [INLINE ASM](src/FerrataRegistry.sol#L596-L598)

src/FerrataRegistry.sol#L595-L599


 - [ ] ID-28
[FerrataRegistry._store(bytes32,bytes32)](src/FerrataRegistry.sol#L584-L588) uses assembly
	- [INLINE ASM](src/FerrataRegistry.sol#L585-L587)

src/FerrataRegistry.sol#L584-L588


 - [ ] ID-29
[FerrataVault._trySendEth(address,uint256)](src/FerrataVault.sol#L1452-L1456) uses assembly
	- [INLINE ASM](src/FerrataVault.sol#L1453-L1455)

src/FerrataVault.sol#L1452-L1456


## cyclomatic-complexity
Impact: Informational
Confidence: High
 - [ ] ID-30
[FerrataVault._checkRelease(FerrataVault.ReleaseRequest)](src/FerrataVault.sol#L1357-L1378) has a high cyclomatic complexity (12).

src/FerrataVault.sol#L1357-L1378


 - [ ] ID-31
[FerrataVault.constructor(FerrataVault.Deployment)](src/FerrataVault.sol#L578-L657) has a high cyclomatic complexity (18).

src/FerrataVault.sol#L578-L657


## unused-state
Impact: Informational
Confidence: High
 - [ ] ID-32
[FerrataVault.DEPOSIT_ROOT_SLOT](src/FerrataVault.sol#L212) is never used in [FerrataVault](src/FerrataVault.sol#L27-L1534)

src/FerrataVault.sol#L212


 - [ ] ID-33
[FerrataVault.ROLE_TIMELOCK](src/FerrataVault.sol#L227) is never used in [FerrataVault](src/FerrataVault.sol#L27-L1534)

src/FerrataVault.sol#L227


 - [ ] ID-34
[FerrataVault.NEXT_LEAF_INDEX_SLOT](src/FerrataVault.sol#L214) is never used in [FerrataVault](src/FerrataVault.sol#L27-L1534)

src/FerrataVault.sol#L214


 - [ ] ID-35
[FerrataVault.LEZ_CONFIG_ID_SLOT](src/FerrataVault.sol#L216) is never used in [FerrataVault](src/FerrataVault.sol#L27-L1534)

src/FerrataVault.sol#L216


## immutable-states
Impact: Optimization
Confidence: High
 - [ ] ID-36
[FerrataVault.storedLezConfigId](src/FerrataVault.sol#L286) should be immutable 

src/FerrataVault.sol#L286


