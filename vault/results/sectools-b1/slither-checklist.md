'forge clean' running (wd: ./vault)
'forge config --json' running
'forge build --build-info --skip ./test/** ./script/** --force' running (wd: ./vault)
**THIS CHECKLIST IS NOT COMPLETE**. Use `--show-ignored-findings` to show all the results.
Summary
 - [encode-packed-collision](#encode-packed-collision) (2 results) (High)
 - [divide-before-multiply](#divide-before-multiply) (1 results) (Medium)
 - [incorrect-equality](#incorrect-equality) (3 results) (Medium)
 - [unused-return](#unused-return) (1 results) (Medium)
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
[FerrataVault._recordBeacon()](src/FerrataVault.sol#L1259-L1268) uses a dangerous strict equality:
	- [index != 0 && _lastBeaconBlock == block.number](src/FerrataVault.sol#L1261)

src/FerrataVault.sol#L1259-L1268


 - [ ] ID-4
[FerrataVault.burnRootStatus(bytes32)](src/FerrataVault.sol#L1147-L1151) uses a dangerous strict equality:
	- [entry == 0 || checkpoints[entry - 1].revoked](src/FerrataVault.sol#L1149)

src/FerrataVault.sol#L1147-L1151


 - [ ] ID-5
[FerrataVault._finalize(FerrataVault.Checkpoint,address)](src/FerrataVault.sol#L811-L846) uses a dangerous strict equality:
	- [previous == 0 || checkpoints[previous - 1].revoked](src/FerrataVault.sol#L836)

src/FerrataVault.sol#L811-L846


## unused-return
Impact: Medium
Confidence: Medium
 - [ ] ID-6
[StandInCheckpointVerifier.verify(bytes,bytes32,bytes32)](src/standin/StandInCheckpointVerifier.sol#L138-L162) ignores return value by [(signer,err,None) = ECDSA.tryRecover(digest,v,r,s)](src/standin/StandInCheckpointVerifier.sol#L156)

src/standin/StandInCheckpointVerifier.sol#L138-L162


## missing-zero-check
Impact: Low
Confidence: Medium
 - [ ] ID-7
[FerrataVault.transferAdmin(address).newAdmin](src/FerrataVault.sol#L1071) lacks a zero-check on :
		- [pendingAdmin = newAdmin](src/FerrataVault.sol#L1072)

src/FerrataVault.sol#L1071


## timestamp
Impact: Low
Confidence: Medium
 - [ ] ID-8
[FerrataRegistry.revokeFrom(FerrataVault.FinalizedCheckpoint[],uint256,uint256)](src/FerrataRegistry.sol#L341-L353) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp > first.finalizedAt + revokeWindow](src/FerrataRegistry.sol#L347)

src/FerrataRegistry.sol#L341-L353


 - [ ] ID-9
[FerrataVault._recordBeacon()](src/FerrataVault.sol#L1259-L1268) uses timestamp for comparisons
	Dangerous comparisons:
	- [index != 0 && _lastBeaconBlock == block.number](src/FerrataVault.sol#L1261)

src/FerrataVault.sol#L1259-L1268


 - [ ] ID-10
[FerrataVault._feesInForce(address)](src/FerrataVault.sol#L1443-L1450) uses timestamp for comparisons
	Dangerous comparisons:
	- [pendingAt != 0 && block.timestamp >= pendingAt](src/FerrataVault.sol#L1447)

src/FerrataVault.sol#L1443-L1450


 - [ ] ID-11
[FerrataVault.pauseCheckpoints()](src/FerrataVault.sol#L1037-L1042) uses timestamp for comparisons
	Dangerous comparisons:
	- [until > checkpointsFrozenUntil](src/FerrataVault.sol#L1040)

src/FerrataVault.sol#L1037-L1042


 - [ ] ID-12
[FerrataVault.retireOpensAt()](src/FerrataVault.sol#L1010-L1018) uses timestamp for comparisons
	Dangerous comparisons:
	- [checkpointsFrozenUntil > from](src/FerrataVault.sol#L1012)
	- [sunsetOpens < opensAt](src/FerrataVault.sol#L1016)

src/FerrataVault.sol#L1010-L1018


 - [ ] ID-13
[FerrataVault.finalizeAttested(FerrataVault.Checkpoint)](src/FerrataVault.sol#L799-L807) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp < cand.submittedAt + _wait(cand)](src/FerrataVault.sol#L802)

src/FerrataVault.sol#L799-L807


 - [ ] ID-14
[FerrataVault.revokeFrom(uint256)](src/FerrataVault.sol#L1048-L1063) uses timestamp for comparisons
	Dangerous comparisons:
	- [sunsetLeafCountSet && sunsetSetAt >= index](src/FerrataVault.sol#L1056)

src/FerrataVault.sol#L1048-L1063


 - [ ] ID-15
[FerrataVault.retire()](src/FerrataVault.sol#L998-L1003) uses timestamp for comparisons
	Dangerous comparisons:
	- [msg.sender != freezeAuthority && block.timestamp < retireOpensAt()](src/FerrataVault.sol#L999)

src/FerrataVault.sol#L998-L1003


 - [ ] ID-16
[FerrataVault._checkRelease(FerrataVault.ReleaseRequest)](src/FerrataVault.sol#L1360-L1381) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp < opensAt](src/FerrataVault.sol#L1376)

src/FerrataVault.sol#L1360-L1381


 - [ ] ID-17
[FerrataRegistry._updateBucket(FerrataVault.Bucket,uint256,uint256)](src/FerrataRegistry.sol#L410-L417) uses timestamp for comparisons
	Dangerous comparisons:
	- [level > capacity](src/FerrataRegistry.sol#L412)

src/FerrataRegistry.sol#L410-L417


 - [ ] ID-18
[FerrataRegistry.applyFees(mapping(address => FerrataVault.TokenConfig),mapping(address => FerrataVault.FeeState),mapping(address => mapping(uint256 => FerrataVault.FeeSchedule)),address)](src/FerrataRegistry.sol#L124-L143) uses timestamp for comparisons
	Dangerous comparisons:
	- [pendingAt == 0 || block.timestamp < pendingAt](src/FerrataRegistry.sol#L133)

src/FerrataRegistry.sol#L124-L143


 - [ ] ID-19
[FerrataVault.refund(uint64,address,address,uint8,bytes32,bytes32[32])](src/FerrataVault.sol#L707-L733) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp < _refundsOpenAt()](src/FerrataVault.sol#L719)

src/FerrataVault.sol#L707-L733


 - [ ] ID-20
[FerrataVault._finalize(FerrataVault.Checkpoint,address)](src/FerrataVault.sol#L811-L846) uses timestamp for comparisons
	Dangerous comparisons:
	- [previous == 0 || checkpoints[previous - 1].revoked](src/FerrataVault.sol#L836)

src/FerrataVault.sol#L811-L846


 - [ ] ID-21
[FerrataVault.setCheckpointsFrozen(bool)](src/FerrataVault.sol#L976-L981) uses timestamp for comparisons
	Dangerous comparisons:
	- [until > block.timestamp](src/FerrataVault.sol#L978)

src/FerrataVault.sol#L976-L981


 - [ ] ID-22
[FerrataVault.burnRootStatus(bytes32)](src/FerrataVault.sol#L1147-L1151) uses timestamp for comparisons
	Dangerous comparisons:
	- [entry == 0 || checkpoints[entry - 1].revoked](src/FerrataVault.sol#L1149)

src/FerrataVault.sol#L1147-L1151


 - [ ] ID-23
[FerrataRegistry.currentLevel(FerrataVault.Bucket)](src/FerrataRegistry.sol#L366-L371) uses timestamp for comparisons
	Dangerous comparisons:
	- [b.refillPerSecond != 0 && elapsed > missing / b.refillPerSecond](src/FerrataRegistry.sol#L369)

src/FerrataRegistry.sol#L366-L371


 - [ ] ID-24
[FerrataVault._frozen(uint256)](src/FerrataVault.sol#L1513-L1515) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp < until](src/FerrataVault.sol#L1514)

src/FerrataVault.sol#L1513-L1515


 - [ ] ID-25
[FerrataRegistry.applyRoles(FerrataVault.RoleProposal,address[3],address,address)](src/FerrataRegistry.sol#L313-L334) uses timestamp for comparisons
	Dangerous comparisons:
	- [block.timestamp < effectiveAt](src/FerrataRegistry.sol#L321)

src/FerrataRegistry.sol#L313-L334


## assembly
Impact: Informational
Confidence: High
 - [ ] ID-26
[FerrataRegistry._depositRoot()](src/FerrataRegistry.sol#L602-L606) uses assembly
	- [INLINE ASM](src/FerrataRegistry.sol#L603-L605)

src/FerrataRegistry.sol#L602-L606


 - [ ] ID-27
[FerrataRegistry.claimOwed(mapping(address => mapping(address => uint256)),mapping(address => uint256),address)](src/FerrataRegistry.sol#L276-L295) uses assembly
	- [INLINE ASM](src/FerrataRegistry.sol#L288-L290)

src/FerrataRegistry.sol#L276-L295


 - [ ] ID-28
[FerrataRegistry._leafCount()](src/FerrataRegistry.sol#L608-L612) uses assembly
	- [INLINE ASM](src/FerrataRegistry.sol#L609-L611)

src/FerrataRegistry.sol#L608-L612


 - [ ] ID-29
[FerrataRegistry._store(bytes32,bytes32)](src/FerrataRegistry.sol#L591-L595) uses assembly
	- [INLINE ASM](src/FerrataRegistry.sol#L592-L594)

src/FerrataRegistry.sol#L591-L595


 - [ ] ID-30
[FerrataVault._trySendEth(address,uint256)](src/FerrataVault.sol#L1455-L1459) uses assembly
	- [INLINE ASM](src/FerrataVault.sol#L1456-L1458)

src/FerrataVault.sol#L1455-L1459


## cyclomatic-complexity
Impact: Informational
Confidence: High
 - [ ] ID-31
[FerrataVault._checkRelease(FerrataVault.ReleaseRequest)](src/FerrataVault.sol#L1360-L1381) has a high cyclomatic complexity (12).

src/FerrataVault.sol#L1360-L1381


 - [ ] ID-32
[FerrataVault.constructor(FerrataVault.Deployment)](src/FerrataVault.sol#L578-L657) has a high cyclomatic complexity (18).

src/FerrataVault.sol#L578-L657


## unused-state
Impact: Informational
Confidence: High
 - [ ] ID-33
[FerrataVault.DEPOSIT_ROOT_SLOT](src/FerrataVault.sol#L212) is never used in [FerrataVault](src/FerrataVault.sol#L27-L1537)

src/FerrataVault.sol#L212


 - [ ] ID-34
[FerrataVault.ROLE_TIMELOCK](src/FerrataVault.sol#L227) is never used in [FerrataVault](src/FerrataVault.sol#L27-L1537)

src/FerrataVault.sol#L227


 - [ ] ID-35
[FerrataVault.NEXT_LEAF_INDEX_SLOT](src/FerrataVault.sol#L214) is never used in [FerrataVault](src/FerrataVault.sol#L27-L1537)

src/FerrataVault.sol#L214


 - [ ] ID-36
[FerrataVault.LEZ_CONFIG_ID_SLOT](src/FerrataVault.sol#L216) is never used in [FerrataVault](src/FerrataVault.sol#L27-L1537)

src/FerrataVault.sol#L216


## immutable-states
Impact: Optimization
Confidence: High
 - [ ] ID-37
[FerrataVault.storedLezConfigId](src/FerrataVault.sol#L286) should be immutable 

src/FerrataVault.sol#L286


