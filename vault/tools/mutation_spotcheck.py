#!/usr/bin/env python3
"""Hand-written mutation spot-check for src/FerrataVault.sol and its library src/FerrataRegistry.sol (v5: v4's list
retargeted where the code moved, the D13 fee mutants, and the fixes of the security review of 11 November 2026).

Each mutant is one deliberate bug (a single textual replacement). For each one the script writes the mutated
contract into a scratch copy of the project (notes/mutation-workspace-<start>, with lib/ symlinked), runs every test
suite except the invariant campaign (so each failing test is listed with its reason), and reports whether the
suite failed ("killed") or still passed. A mutant that survives those suites is then run against the invariant
campaign as well, and reported "SURVIVED" only if that passes too. The real src/ is never modified.

The scratch copy compiles without the optimizer (about 3x faster; the unmutated suite passes there as well).

Usage (from the project root): python3 -I tools/mutation_spotcheck.py [start [stop]]
The optional arguments select MUTANTS[start:stop], so a long run can be split or run in parallel slices (each slice
uses its own workspace). MUTATION_SKIP_BASELINE=1 skips the per-slice check that the unmutated suite passes.
Needs forge on PATH. Honours TMPDIR.
"""

import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VAULT = "src/FerrataVault.sol"
REGISTRY = "src/FerrataRegistry.sol"  # v4: the linked library

ETH_CALL = """        assembly ("memory-safe") {
            ok := call(gas(), to, amount, 0, 0, 0, 0)
        }"""

# (name, exact text in the original, replacement[, file]). The file defaults to VAULT; the original text must occur
# exactly once in it.
MUTANTS = [
    ('deposit: allowed after retire()', '        if (retired || finalSunset) revert VaultRetired();\n', '        if (finalSunset) revert VaultRetired();\n'),
    ('L-3: deposits allowed after a finalised sunset checkpoint', 'if (retired || finalSunset) revert VaultRetired();', 'if (retired) revert VaultRetired();'),
    ('deposit: global deposit freeze ignored', '        if (_frozen(depositsFrozenUntil)) revert DepositsAreFrozen();\n', ''),
    ('deposit: per-token deposit freeze ignored', '        if (_frozen(tokenDepositsFrozenUntil[token])) revert TokenDepositsAreFrozen();\n', ''),
    ('deposit: delisting ignored', '        if (!cfg.depositsEnabled) revert DepositsDisabled();\n', ''),
    ('deposit: zero commitment allowed', '        if (commitment == bytes32(0)) revert ZeroCommitment();\n', ''),
    ('deposit: no index check', '        if (index != depositCount[msg.sender]) revert WrongDepositIndex();\n', ''),
    ('deposit: counter not advanced', '        depositCount[msg.sender] = index + 1;\n', ''),
    ('deposit: no exact-balance check (fee-on-transfer accepted)', '        if (erc20.balanceOf(address(this)) != balanceBefore + total) revert TransferAmountMismatch();\n', ''),
    ('deposit: msg.value may exceed the denomination', 'if (msg.value != total) revert WrongEthAmount();', 'if (msg.value < total) revert WrongEthAmount();'),
    ('deposit: escrow not incremented', '        escrowed[token] += total;\n', '', 'src/FerrataRegistry.sol'),
    ('leaf: binds tx.origin instead of the depositor', 'leafHash(leafIndex, msg.sender, token, amount, commitment)', 'leafHash(leafIndex, tx.origin, token, amount, commitment)'),
    ('leaf: index left out', 'abi.encodePacked(LEAF_PREFIX, LEAF_TAG, leafIndex, depositor, token, amount, commitment)', 'abi.encodePacked(LEAF_PREFIX, LEAF_TAG, uint64(0), depositor, token, amount, commitment)'),
    ('tree: left/right swapped where the path bit is 1', 'node = _hashNode(_branch[h], node);', 'node = _hashNode(node, _branch[h]);'),
    ('tree: branch saved at every left step (roots stay correct)', '                if (!branchSaved) {\n                    _branch[h] = node;\n                    branchSaved = true;\n                }', '                _branch[h] = node;\n                branchSaved = true;'),
    ('tree: empty-subtree hash not advanced', '            emptySubtree = _hashNode(emptySubtree, emptySubtree);\n        }\n        depositRoot = node;', '        }\n        depositRoot = node;'),
    ('tree: empty-tree root not set in the constructor', '        depositRoot = emptySubtree;\n', ''),
    ('tree: a full tree accepts one more leaf (>)', 'if (nextLeafIndex >= MAX_LEAVES) revert TreeFull();', 'if (nextLeafIndex > MAX_LEAVES) revert TreeFull();'),
    ('storage: lezConfigId not copied to slot 2', '        storedLezConfigId = d.lezConfigId;\n', ''),
    ('registry: re-registration allowed', '        if (cfg.registered) revert FerrataVault.TokenAlreadyRegistered();\n', '', 'src/FerrataRegistry.sol'),
    ('registry: relay fee ceiling 2% instead of 1%', 'if (fees[i] > denoms[i] / FEE_CEILING_DIVISOR) revert FerrataVault.RelayFeeTooHigh();', 'if (fees[i] > denoms[i] / 50) revert FerrataVault.RelayFeeTooHigh();', 'src/FerrataRegistry.sol'),
    ('registry: relay fee count not checked', '        if (fees.length != denoms.length) revert FerrataVault.RelayFeeCountMismatch();\n', '', 'src/FerrataRegistry.sol'),
    ('registry: lezUnit need not be a power of ten', 'if (rest != 1 || (decimals < 39 && lezUnit > 10 ** decimals)) revert', 'if ((decimals < 39 && lezUnit > 10 ** decimals)) revert', 'src/FerrataRegistry.sol'),
    ('registry: denominations need not be multiples of lezUnit', '            if (denoms[i] % lezUnit != 0) revert FerrataVault.DenominationNotMultipleOfLezUnit();\n', '', 'src/FerrataRegistry.sol'),
    ('registry: release delay not checked at registration', '        checkReleaseDelay(p.releaseDelay, limits.minReleaseDelay, limits.revokeWindow);\n', '', 'src/FerrataRegistry.sol'),
    ('delay: below minReleaseDelay allowed', 'if (delay < minReleaseDelay || delay > revokeWindow ||', 'if (delay > revokeWindow ||', 'src/FerrataRegistry.sol'),
    ('delay: above revokeWindow allowed', 'if (delay < minReleaseDelay || delay > revokeWindow ||', 'if (delay < minReleaseDelay ||', 'src/FerrataRegistry.sol'),
    ('constructor: turnSeconds unbounded under A', 'd.mode == Mode.A ? d.turnSeconds == 0 || d.turnSeconds > MAX_DELTA : d.turnSeconds != 0', 'd.mode == Mode.A ? d.turnSeconds == 0 : d.turnSeconds != 0'),
    ('constructor: minReleaseDelay may exceed revokeWindow', 'if (d.minReleaseDelay > d.revokeWindow || d.minReleaseDelay > MAX_RELEASE_DELAY)', 'if (d.minReleaseDelay > MAX_RELEASE_DELAY)'),
    ('submit: checkpoint freeze ignored', 'external {\n        if (_frozen(checkpointsFrozenUntil)) revert CheckpointsAreFrozen();\n        if (c.parentState', 'external {\n        if (c.parentState'),
    ('submit: parent need not be final', '        if (c.parentState != finalState) revert ParentNotFinal();\n', ''),
    ('submit: echo nonce may be set', '        if (c.echoNonce != bytes32(0)) revert EchoNonceNotZero();\n', ''),
    ('submit: committee not checked', '        if (c.echoNonce != bytes32(0)) revert EchoNonceNotZero();\n        _checkCommittee(c);\n', '        if (c.echoNonce != bytes32(0)) revert EchoNonceNotZero();\n'),
    ('committee: journal floors need not be the deployed ones', '        if (c.nMin != nMin || c.tMin != tMin) revert WrongCommitteeFloor();\n', ''),
    ('committee: key set size not checked', 'if (c.n < nMin || c.t < tMin) revert KeySetBelowFloor();', 'if (c.t < tMin) revert KeySetBelowFloor();'),
    ('committee: threshold not checked', 'if (c.n < nMin || c.t < tMin) revert KeySetBelowFloor();', 'if (c.n < nMin) revert KeySetBelowFloor();'),
    ("submit: candidate's t not stored (floor used)", '            t: c.t,\n', '            t: tMin,\n'),
    ('submit: resubmission restarts the wait', '        if (existing.submittedAt != 0 && existing.epoch == epoch) revert CandidateAlreadySubmitted();\n', ''),
    ('L-1: a stale candidate still blocks its state', 'existing.epoch == epoch) revert CandidateAlreadySubmitted();', 'existing.epoch == revocationEpoch) revert CandidateAlreadySubmitted();'),
    ('submit: delta not recorded', '            deltaAtSubmit: uint32(delta),', '            deltaAtSubmit: uint32(deltaFloor),'),
    ('submit: no beacon recorded under A', '        if (mode == Mode.A) _recordBeacon();\n', ''),
    ('beacon: several per block', '        if (index != 0 && _lastBeaconBlock == block.number) return index - 1;\n', ''),
    ('beacon: nonce without prevrandao', 'keccak256(abi.encode(block.prevrandao, block.number, address(this)))', 'keccak256(abi.encode(block.number, address(this)))'),
    ('finalize: stale candidates (revoked or superseded) accepted', '        if (cand.epoch != _epoch()) revert StaleCandidate();\n', ''),
    ('L-1: a finalisation does not stale the other candidates (parent no longer final)', 'return revocationEpoch + _stateChanges;', 'return revocationEpoch;'),
    ('L-1: a revocation does not stale pending candidates', 'return revocationEpoch + _stateChanges;', 'return _stateChanges;'),
    ('L-1: a state change not counted', '        if (c.newState != c.parentState) _stateChanges += 1;\n', ''),
    ('L-1: a no-op checkpoint stales pending candidates', '        if (c.newState != c.parentState) _stateChanges += 1;', '        _stateChanges += 1;'),
    ("finalize: journal need not match the candidate's digest", '        if (_checkpointDigest(c, echoNonce) != cand.journalDigest) revert CandidateMismatch();\n', ''),
    ('finalize: checkpoint freeze ignored', '        if (_frozen(checkpointsFrozenUntil)) revert CheckpointsAreFrozen();\n        cand = candidates[c.newState];', '        cand = candidates[c.newState];'),
    ("finalize (A): the candidate digest takes the beacon's echo instead of 0", 'Candidate storage cand = _liveCandidate(c, bytes32(0));', 'Candidate storage cand = _liveCandidate(c, c.echoNonce);'),
    ('finalize: beacon at exactly W refused (<=)', 'if (beacon.timestamp < cand.submittedAt + _wait(cand)) revert BeaconTooEarly();', 'if (beacon.timestamp <= cand.submittedAt + _wait(cand)) revert BeaconTooEarly();'),
    ("wait: current delta instead of the candidate's", 'return uint256(cand.deltaAtSubmit) + (uint256(cand.t) - 1) * turnSeconds;', 'return delta + (uint256(cand.t) - 1) * turnSeconds;'),
    ('wait: no committee term (W = delta)', 'return uint256(cand.deltaAtSubmit) + (uint256(cand.t) - 1) * turnSeconds;', 'return uint256(cand.deltaAtSubmit);'),
    ('wait: t turns instead of t - 1', 'return uint256(cand.deltaAtSubmit) + (uint256(cand.t) - 1) * turnSeconds;', 'return uint256(cand.deltaAtSubmit) + uint256(cand.t) * turnSeconds;'),
    ('finalize: imported leaf count may decrease', '        if (c.importedLeafCount < finalImportedLeafCount) revert ImportedLeafCountDecreased();\n', ''),
    ('finalize: sunset may be dropped', 'if (finalSunset && (!c.sunset || c.importedLeafCount != finalImportedLeafCount)) revert SunsetIsFinal();', 'if (finalSunset && c.importedLeafCount != finalImportedLeafCount) revert SunsetIsFinal();'),
    ('finalize: count may grow after a sunset', 'if (finalSunset && (!c.sunset || c.importedLeafCount != finalImportedLeafCount)) revert SunsetIsFinal();', 'if (finalSunset && !c.sunset) revert SunsetIsFinal();'),
    ('finalize: parent count not recorded', '                parentImportedLeafCount: finalImportedLeafCount,', '                parentImportedLeafCount: 0,'),
    ('finalize: parent sunset flag not recorded', '                parentSunset: finalSunset,', '                parentSunset: false,'),
    ('finalize: imported leaf count not tracked', '        finalImportedLeafCount = c.importedLeafCount;\n', ''),
    ('finalize: sunset flag not tracked', '        finalSunset = c.sunset;\n', ''),
    ('finalize: a later sunset checkpoint moves the boundary', 'if (c.sunset && !sunsetLeafCountSet) {', 'if (c.sunset) {'),
    ('finalize: sunset finalisation time not recorded', '            sunsetFinalizedAt = block.timestamp;\n', ''),
    ('finalize: echo nonce not checked', '        if (c.echoNonce != beacon.nonce) revert EchoNonceMismatch();\n', ''),
    ('finalize: finalState not advanced', '        finalState = c.newState;\n        finalImportedLeafCount', '        finalImportedLeafCount'),
    ('finalize: a re-proven root stays pointing at the revoked checkpoint', 'if (previous == 0 || checkpoints[previous - 1].revoked) burnRootCheckpoint[c.burnRoot] = index + 1;', 'if (previous == 0) burnRootCheckpoint[c.burnRoot] = index + 1;'),
    ('finalize: sunset not recorded', '            sunsetLeafCountSet = true;\n', ''),
    ('checkpoint journal: chain id dropped', 'abi.encode(CHECKPOINT_DOMAIN, block.chainid, address(this),', 'abi.encode(CHECKPOINT_DOMAIN, uint256(0), address(this),'),
    ('checkpoint journal: sunset and echoNonce swapped', 'c.importedLeafCount, c.sunset, echoNonce);', 'c.importedLeafCount, echoNonce, c.sunset);'),
    ('checkpoint journal: tip, n and t dropped (13 words)', 'sha256(bytes.concat(head, body, tail))', 'sha256(bytes.concat(head, body))'),
    ('checkpoint journal: n and t swapped', 'abi.encode(c.tipId, c.tipHash, c.n, c.t)', 'abi.encode(c.tipId, c.tipHash, c.t, c.n)'),
    ('cancel: anyone may cancel', '        if (msg.sender != freezeAuthority && !_isWatcher(msg.sender)) revert NotWatcher();\n', ''),
    ('cancel: unknown candidate accepted', '        FerrataVault.Candidate storage cand = candidates[newState];\n        if (cand.submittedAt == 0) revert FerrataVault.UnknownCandidate();\n', '        FerrataVault.Candidate storage cand = candidates[newState];\n', 'src/FerrataRegistry.sol'),
    ('cancel: candidate not deleted', '        delete candidates[newState];\n        emit FerrataVault.CandidateCancelled', '        emit FerrataVault.CandidateCancelled', 'src/FerrataRegistry.sol'),
    ('submit: tip hash not stored', '            tipHash: c.tipHash,\n', '            tipHash: bytes32(0),\n'),
    ('revoke: window not enforced', '        if (block.timestamp > first.finalizedAt + revokeWindow) revert FerrataVault.RevokeWindowPassed();\n', '', 'src/FerrataRegistry.sol'),
    ('revoke: window one second short (>=)', 'if (block.timestamp > first.finalizedAt + revokeWindow) revert FerrataVault.RevokeWindowPassed();', 'if (block.timestamp >= first.finalizedAt + revokeWindow) revert FerrataVault.RevokeWindowPassed();', 'src/FerrataRegistry.sol'),
    ('revoke: only the first checkpoint marked', 'for (uint256 i = index; i < end; i++) {', 'for (uint256 i = index; i < index + 1; i++) {', 'src/FerrataRegistry.sol'),
    ('revoke: finalState not restored', '        finalState = first.parentState;\n', ''),
    ('revoke: imported leaf count not restored', '        finalImportedLeafCount = first.parentImportedLeafCount;\n', ''),
    ('revoke: sunset flag not restored', '        finalSunset = first.parentSunset;\n', ''),
    ('revoke: sunset finalisation time not cleared', '            sunsetFinalizedAt = 0;\n', ''),
    ('revoke: pending candidates stay valid', '        revocationEpoch += 1;\n', ''),
    ('revoke: sunset boundary not cleared', 'if (sunsetLeafCountSet && sunsetSetAt >= index) {', 'if (false) {'),
    ('revoke: an already revoked checkpoint can be revoked again', '        if (first.revoked) revert FerrataVault.CheckpointAlreadyRevoked();\n', '', 'src/FerrataRegistry.sol'),
    ('delta: can be lowered', '        if (newDelta < delta) revert DeltaCanOnlyRise();\n', ''),
    ('delta: no ceiling', '        if (newDelta > MAX_DELTA) revert DeltaTooLong();\n', ''),
    ('refund: allowed before retire()', '        if (!retired) revert NotRetired();\n', ''),
    ('refund: release freeze ignored', '        if (!retired) revert NotRetired();\n        if (_frozen(releasesFrozenUntil)) revert ReleasesAreFrozen();\n', '        if (!retired) revert NotRetired();\n'),
    ('refund: open before revokeWindow has passed', 'if (block.timestamp < _refundsOpenAt()) revert RefundNotYetOpen();', 'if (block.timestamp < sunsetFinalizedAt) revert RefundNotYetOpen();'),
    ('refund: opens at the last revocable second (<, D11)', 'return sunsetFinalizedAt + revokeWindow + 1;', 'return sunsetFinalizedAt + revokeWindow;'),
    ('refund: no explicit escrow check', '        if (amount > escrowed[token]) revert InsufficientEscrow();\n', ''),
    ('refund: allowed before any sunset', 'if (!sunsetLeafCountSet || leafIndex < sunsetLeafCount) revert NotRefundable();', 'if (leafIndex < sunsetLeafCount) revert NotRefundable();'),
    ('refund: allowed below the sunset leaf count', 'if (!sunsetLeafCountSet || leafIndex < sunsetLeafCount) revert NotRefundable();', 'if (!sunsetLeafCountSet) revert NotRefundable();'),
    ('refund: unknown leaf index not refused', '        if (leafIndex >= _leafCount()) revert FerrataVault.UnknownLeaf();\n', '', 'src/FerrataRegistry.sol'),
    ('refund: double refund allowed', '        if (refunded[leafIndex]) revert AlreadyRefunded();\n', ''),
    ('refund: leaf not marked refunded', '        refunded[leafIndex] = true;\n', ''),
    ('refund: path not checked', '        if (node != _depositRoot()) revert FerrataVault.InvalidMerklePath();\n', '', 'src/FerrataRegistry.sol'),
    ('refund: path direction bits inverted', '_hashNode(path[h], node) : _hashNode(node, path[h]);', '_hashNode(node, path[h]) : _hashNode(path[h], node);', 'src/FerrataRegistry.sol'),
    ('refund: escrow not decremented', '        refunded[leafIndex] = true;\n        escrowed[token] -= amount;\n', '        refunded[leafIndex] = true;\n'),
    ('release: global release freeze ignored', 'returns (uint256 amount, address prover) {\n        if (_frozen(releasesFrozenUntil)) revert ReleasesAreFrozen();\n', 'returns (uint256 amount, address prover) {\n'),
    ('release: per-token release freeze ignored', '        if (_frozen(tokenReleasesFrozenUntil[r.token])) revert TokenReleasesAreFrozen();\n', ''),
    ('release: zero recipient allowed', '        if (r.recipient == address(0)) revert ZeroRecipient();\n', ''),
    ('release: revoked root still accepted', 'if (entry == 0 || checkpoints[entry - 1].revoked) revert BurnRootNotAccepted();', 'if (entry == 0) revert BurnRootNotAccepted();'),
    ('release: delay off by one (<=)', 'if (block.timestamp < opensAt) revert ReleaseDelayNotElapsed();', 'if (block.timestamp <= opensAt) revert ReleaseDelayNotElapsed();'),
    ("release: current delay instead of the root's", 'uint256 delay = _releaseDelayFor(r.token, entry - 1);', 'uint256 delay = _releaseDelayFor(r.token, checkpoints.length);'),
    ('release: delay history search off by one', 'if (changes[mid].fromCheckpoint <= checkpointIndex) low = mid + 1;', 'if (changes[mid].fromCheckpoint < checkpointIndex) low = mid + 1;'),
    ('release: no nullifier check', '        if (nullifierUsed[r.nullifier]) revert NullifierAlreadyUsed();\n', ''),
    ('release: nullifier never marked used', '        nullifierUsed[r.nullifier] = true;\n', ''),
    ('release: any fee up to the registered fee', 'if (r.relayFee != 0 && r.relayFee != cfg.relayFees[r.denomIndex])', 'if (r.relayFee > cfg.relayFees[r.denomIndex])'),
    ('release: no explicit escrow check', '        if (amount > escrowed[r.token]) revert InsufficientEscrow();\n', ''),
    ('release: caps not charged', '        _consumeReleaseCaps(r.token, amount);\n', ''),
    ('release: escrow not decremented', '        escrowed[r.token] -= amount - burnFee;\n', '', 'src/FerrataRegistry.sol'),
    ('release: unset relayer pays the recipient', 'r.relayer == address(0) ? msg.sender : r.relayer', 'r.relayer == address(0) ? r.recipient : r.relayer'),
    ('release: recipient paid the full amount', '_payOrCredit(owed, totalOwed, r.token, r.recipient, amount - r.relayFee - burnFee - checkpointFee);', '_payOrCredit(owed, totalOwed, r.token, r.recipient, amount);', 'src/FerrataRegistry.sol'),
    ('release journal: vault address dropped', '                RELEASE_DOMAIN,\n                block.chainid,\n                address(this),', '                RELEASE_DOMAIN,\n                block.chainid,\n                address(0),'),
    ('release journal: recipient and relayer swapped', '                r.recipient,\n                r.relayer,', '                r.relayer,\n                r.recipient,'),
    ('pay: a failed payment is not credited', '        owed[payee][token] += amount;\n', ''),
    ('pay: totalOwed not tracked', '        totalOwed[token] += amount;\n', ''),
    ('pay: ETH call copies the return data', '        assembly ("memory-safe") {\n            ok := call(gas(), to, amount, 0, 0, 0, 0)\n        }', '        (ok,) = to.call{value: amount}("");'),
    ('claimOwed: credit not cleared', '        owed[msg.sender][token] = 0;\n', '', 'src/FerrataRegistry.sol'),
    ('claimOwed: a failed ETH claim does not revert', '            if (!ok) revert FerrataVault.EthTransferFailed();\n', '', 'src/FerrataRegistry.sol'),
    ('claimOwed: total owed not decremented', '        totalOwed[token] -= amount;\n', '', 'src/FerrataRegistry.sol'),
    ('claimOwed: ERC-20 claim not paid', '            IERC20(token).safeTransfer(msg.sender, amount);\n', '', 'src/FerrataRegistry.sol'),
    ('claimOwed: nothing owed accepted', '        if (amount == 0) revert FerrataVault.NothingOwed();\n', '', 'src/FerrataRegistry.sol'),
    ('freeze: never expires', '        return block.timestamp < until;', '        return until != 0;'),
    ('freeze: lasts half of maxFreeze', 'return frozen ? block.timestamp + maxFreeze : 0;', 'return frozen ? block.timestamp + maxFreeze / 2 : 0;'),
    ('retire: a second retire() is accepted', '        if (retired) revert AlreadyRetired();\n', ''),
    ('roles: freeze authority may become admin too', '        if (msg.sender == freezeAuthority) revert RolesMustDiffer();\n', ''),
    ('roles: admin may freeze token releases', 'function setTokenReleasesFrozen(address token, bool frozen) external onlyFreezeAuthority', 'function setTokenReleasesFrozen(address token, bool frozen) external onlyAdmin'),
    ('caps: global deposit bucket not charged', '        if (!_tryConsume(_globalDepositCap, _valueUnits(token, amount))) revert GlobalDepositCapExceeded();\n', ''),
    ('caps: consumption does not reset the refill clock', '        b.level = level - amount;\n        b.lastUpdate = block.timestamp;\n', '        b.level = level - amount;\n'),
    ('caps: refill saturation off by one (>=)', 'elapsed > missing / b.refillPerSecond', 'elapsed >= missing / b.refillPerSecond', 'src/FerrataRegistry.sol'),
    ('caps: lowering a cap does not clamp the level', '        if (level > capacity) level = capacity;\n', '', 'src/FerrataRegistry.sol'),
    ('mode: B accepts a turn term', 'd.mode == Mode.A ? d.turnSeconds == 0 || d.turnSeconds > MAX_DELTA : d.turnSeconds != 0', 'd.mode == Mode.A ? d.turnSeconds == 0 || d.turnSeconds > MAX_DELTA : false'),
    ('mode: recordBeacon allowed under B', '        if (mode != Mode.A) revert WrongMode();\n        return _recordBeacon();', '        return _recordBeacon();'),
    ('mode: finalize (A) allowed under B', '        if (mode != Mode.A) revert WrongMode();\n        Candidate storage cand = _liveCandidate(c, bytes32(0));', '        Candidate storage cand = _liveCandidate(c, bytes32(0));'),
    ('mode: finalizeAttested allowed under A', '        if (mode != Mode.B) revert WrongMode();\n', ''),
    ('B: wait not enforced', '        if (block.timestamp < cand.submittedAt + _wait(cand)) revert WaitNotOver();\n', ''),
    ('B: final one second before the wait', 'if (block.timestamp < cand.submittedAt + _wait(cand)) revert WaitNotOver();', 'if (block.timestamp + 1 < cand.submittedAt + _wait(cand)) revert WaitNotOver();'),
    ('B: echoNonce left out of the comparison', 'Candidate storage cand = _liveCandidate(c, c.echoNonce);', 'Candidate storage cand = _liveCandidate(c, bytes32(0));'),
    ('B: monotonicity not checked', '        if (block.timestamp < cand.submittedAt + _wait(cand)) revert WaitNotOver();\n        _checkMonotone(c);\n', '        if (block.timestamp < cand.submittedAt + _wait(cand)) revert WaitNotOver();\n'),
    ('B: finalised after the verifier stop', '        if (_stopped()) revert VerifierStopped();\n        _finalize(c, cand.prover);', '        _finalize(c, cand.prover);'),
    ('submit: journal digest not kept', '            journalDigest: digest,', '            journalDigest: bytes32(0),'),
    ('submit: a beacon recorded under B too', '        if (mode == Mode.A) _recordBeacon();', '        _recordBeacon();'),
    ('submit: tip hash not kept (cancellation event)', '            tipHash: c.tipHash,', '            tipHash: bytes32(0),'),
    ('submit: t not kept (wait)', '            t: c.t,', '            t: 1,'),
    ('submit: submission time not kept', '            submittedAt: uint48(block.timestamp),', '            submittedAt: 1,'),
    ('submit: epoch not kept', '            epoch: uint64(epoch),', '            epoch: 0,'),
    ('D11: more imported leaves than the tree accepted', '        if (c.importedLeafCount > nextLeafIndex) revert ImportedLeafCountTooHigh();\n', ''),
    ('D11: tree bound off by one (>=)', 'if (c.importedLeafCount > nextLeafIndex) revert ImportedLeafCountTooHigh();', 'if (c.importedLeafCount >= nextLeafIndex) revert ImportedLeafCountTooHigh();'),
    ('D11: a release with delay == revokeWindow opens at the last revocable second', '        if (delay == revokeWindow) opensAt += 1;\n', ''),
    ('D11: every release one second late', '        if (delay == revokeWindow) opensAt += 1;\n', '        opensAt += 1;\n'),
    ('finalisation: keyless clock not restarted', '        keylessFrom = uint64(block.timestamp);\n        if (c.newState != c.parentState)', '        if (c.newState != c.parentState)'),
    ('retire: anyone at any time', 'if (msg.sender != freezeAuthority && block.timestamp < retireOpensAt()) revert', 'if (msg.sender != freezeAuthority && block.timestamp < 0) revert'),
    ('retire: keyless one second early', 'block.timestamp < retireOpensAt()', 'block.timestamp + 1 < retireOpensAt()'),
    ('retire: keyless clock not started at deployment', '        keylessFrom = uint64(block.timestamp);\n        FerrataRegistry.initBucket', '        FerrataRegistry.initBucket'),
    ('retire: only the freeze authority (D11/D13 keyless path gone)', 'if (msg.sender != freezeAuthority && block.timestamp < retireOpensAt()) revert', 'if (msg.sender != freezeAuthority) revert'),
    ('M-1: a submission does not restart the keyless clock', '        keylessFrom = uint64(block.timestamp);\n        if (mode == Mode.A) _recordBeacon();', '        if (mode == Mode.A) _recordBeacon();'),
    ('M-1: lifting a checkpoint freeze forgets when it ended (0)', 'checkpointsFrozenUntil = frozen ? block.timestamp + maxFreeze : until;', 'checkpointsFrozenUntil = frozen ? block.timestamp + maxFreeze : 0;'),
    ('M-1: lifting a checkpoint freeze keeps its scheduled end (still frozen)', '        if (until > block.timestamp) until = block.timestamp;\n        checkpointsFrozenUntil = frozen', '        checkpointsFrozenUntil = frozen'),
    ('M-1: a checkpoint freeze shorter than maxFreeze', 'checkpointsFrozenUntil = frozen ? block.timestamp + maxFreeze : until;', 'checkpointsFrozenUntil = frozen ? block.timestamp + maxFreeze / 2 : until;'),
    ('M-1: a freeze or pause in force does not delay the opening', '        if (checkpointsFrozenUntil > from) from = checkpointsFrozenUntil;\n', ''),
    ('M-1: no turn term in the keyless period', '_keylessSpan = (uint256(d.tMin) - 1) * d.turnSeconds + KEYLESS_RETIREMENT;', '_keylessSpan = KEYLESS_RETIREMENT;'),
    ('M-1: the keyless period counts tMin turns', '_keylessSpan = (uint256(d.tMin) - 1) * d.turnSeconds + KEYLESS_RETIREMENT;', '_keylessSpan = uint256(d.tMin) * d.turnSeconds + KEYLESS_RETIREMENT;'),
    ('M-1: no 30 days in the keyless period', '_keylessSpan = (uint256(d.tMin) - 1) * d.turnSeconds + KEYLESS_RETIREMENT;', '_keylessSpan = (uint256(d.tMin) - 1) * d.turnSeconds;'),
    ('M-1: the wait leaves out delta', 'opensAt = from + delta + _keylessSpan;', 'opensAt = from + _keylessSpan;'),
    ('L-3: no keyless retirement after an irrevocable sunset checkpoint', '        if (sunsetLeafCountSet) {\n            uint256 sunsetOpens = _refundsOpenAt();\n            if (sunsetOpens < opensAt) opensAt = sunsetOpens;\n        }\n', ''),
    ('L-3: retirement at the last revocable second of the sunset checkpoint', 'uint256 sunsetOpens = _refundsOpenAt();', 'uint256 sunsetOpens = _refundsOpenAt() - 1;'),
    ('L-3: the sunset opening applies even when later than the keyless one', '            if (sunsetOpens < opensAt) opensAt = sunsetOpens;', '            opensAt = sunsetOpens;'),
    ('L-3: the sunset opening without a sunset checkpoint', '        if (sunsetLeafCountSet) {\n            uint256 sunsetOpens', '        if (true) {\n            uint256 sunsetOpens'),
    ('M-2: the sender of finalize credited under (A)', '        _finalize(c, cand.prover);\n    }\n\n    /// @notice Rule (B)', '        _finalize(c, msg.sender);\n    }\n\n    /// @notice Rule (B)'),
    ('M-2: the sender of finalizeAttested credited under (B)', '        if (_stopped()) revert VerifierStopped();\n        _finalize(c, cand.prover);', '        if (_stopped()) revert VerifierStopped();\n        _finalize(c, msg.sender);'),
    ('I-7: revokeWindow / deltaFloor unbounded', 'if (d.revokeWindow == 0 || d.revokeWindow / d.deltaFloor > MAX_REVOKE_SPAN) revert', 'if (d.revokeWindow == 0) revert'),
    ('I-7: the bound off by one (>=)', 'd.revokeWindow / d.deltaFloor > MAX_REVOKE_SPAN', 'd.revokeWindow / d.deltaFloor >= MAX_REVOKE_SPAN'),
    ('constructor: zero revokeWindow allowed', 'if (d.revokeWindow == 0 || d.revokeWindow', 'if (d.revokeWindow'),
    ('watcher: pause longer than 12 h', 'uint256 until = block.timestamp + WATCHER_PAUSE;', 'uint256 until = block.timestamp + 2 * WATCHER_PAUSE;'),
    ('watcher: pause shortens a longer freeze', '        if (until > checkpointsFrozenUntil) checkpointsFrozenUntil = until;', '        checkpointsFrozenUntil = until;'),
    ('watcher: anyone may pause', '        if (!_isWatcher(msg.sender)) revert NotWatcher();\n', ''),
    ('watcher: third slot ignored', ' || who == watchers[2]', ''),
    ('watcher: first slot ignored', 'who == watchers[0] || ', ''),
    ('roles: timelock not enforced', '        if (block.timestamp < effectiveAt) revert FerrataVault.TimelockNotOver();\n', '', 'src/FerrataRegistry.sol'),
    ('roles: timelock one second short (<=)', 'if (block.timestamp < effectiveAt) revert FerrataVault.TimelockNotOver();', 'if (block.timestamp + 1 < effectiveAt) revert FerrataVault.TimelockNotOver();', 'src/FerrataRegistry.sol'),
    ('roles: applied with nothing proposed', '        if (effectiveAt == 0) revert FerrataVault.NoProposal();\n', '', 'src/FerrataRegistry.sol'),
    ('roles: admin proposed as freeze authority', '        if (newFreezeAuthority == admin) revert FerrataVault.RolesMustDiffer();\n', '', 'src/FerrataRegistry.sol'),
    ('roles: admin installed as freeze authority', '            if (newAuthority == admin) revert FerrataVault.RolesMustDiffer();\n', '', 'src/FerrataRegistry.sol'),
    ('roles: proposal not cleared once applied', '        p.effectiveAt = 0;\n', '', 'src/FerrataRegistry.sol'),
    ('roles: timelock 1 day', 'ROLE_TIMELOCK = 14 days; // D13', 'ROLE_TIMELOCK = 1 days; // D13', 'src/FerrataRegistry.sol'),
    ('roles: the new freeze authority not installed', '        if (newAuthority != address(0)) freezeAuthority = newAuthority;\n', ''),
    ('floor: global release bucket not floored', '            checkReleaseFloor(capacity, refillPerSecond, 1, 1, limits.releaseCapFloor, limits.releaseRefillFloor);\n', '', 'src/FerrataRegistry.sol'),
    ('floor: token release bucket not floored', '        if (direction == FerrataVault.Direction.Release) {\n            Limits memory limits = _limits();\n            checkReleaseFloor(\n                capacity,', '        if (direction != direction) {\n            Limits memory limits = _limits();\n            checkReleaseFloor(\n                capacity,', 'src/FerrataRegistry.sol'),
    ('limits: the release floor read as zero', 'return Limits(v.minReleaseDelay(), v.revokeWindow(), v.releaseCapFloor(), v.releaseRefillFloor());', 'return Limits(v.minReleaseDelay(), v.revokeWindow(), 0, v.releaseRefillFloor());', 'src/FerrataRegistry.sol'),
    ('limits: the refill floor read as zero', 'return Limits(v.minReleaseDelay(), v.revokeWindow(), v.releaseCapFloor(), v.releaseRefillFloor());', 'return Limits(v.minReleaseDelay(), v.revokeWindow(), v.releaseCapFloor(), 0);', 'src/FerrataRegistry.sol'),
    ('limits: minReleaseDelay read as zero', 'return Limits(v.minReleaseDelay(), v.revokeWindow(), v.releaseCapFloor(), v.releaseRefillFloor());', 'return Limits(0, v.revokeWindow(), v.releaseCapFloor(), v.releaseRefillFloor());', 'src/FerrataRegistry.sol'),
    ('limits: revokeWindow read as 30 days', 'return Limits(v.minReleaseDelay(), v.revokeWindow(), v.releaseCapFloor(), v.releaseRefillFloor());', 'return Limits(v.minReleaseDelay(), 30 days, v.releaseCapFloor(), v.releaseRefillFloor());', 'src/FerrataRegistry.sol'),
    ('floor: refill not floored', ' || _toValue(refillPerSecond, weight, unit) < refillFloor', '', 'src/FerrataRegistry.sol'),
    ('floor: registration not floored', '        _checkRegisteredFloor(p, limits);\n', '', 'src/FerrataRegistry.sol'),
    ('floor: overflow instead of saturation', '        if (amount > type(uint256).max / weight) return type(uint256).max;\n', '', 'src/FerrataRegistry.sol'),
    ('floor: UNLIMITED not exempt', '        if (capacity == UNLIMITED) return;\n', '', 'src/FerrataRegistry.sol'),
    ("floor: the deployment's global release bucket not floored", '        FerrataRegistry.checkReleaseFloor(\n            d.globalReleaseCap.capacity,', '        if (false) FerrataRegistry.checkReleaseFloor(\n            d.globalReleaseCap.capacity,'),
    ('refund: release buckets not drawn on', '        _consumeReleaseCaps(token, amount);\n        refunded[leafIndex] = true;', '        refunded[leafIndex] = true;'),
    ('refund: paid after the verifier stop', '        if (_stopped()) revert VerifierStopped();\n        if (!sunsetLeafCountSet', '        if (!sunsetLeafCountSet'),
    ('constructor: a tripped stop accepted', '            if (stopped) revert VerifierNotStop();\n', ''),
    ('constructor: a verifier without paused() accepted', '        } catch {\n            revert VerifierNotStop();\n        }', '        } catch {}'),
    ('fees: deposit leaves out the mint fee', 'total = amount + mintFee + uint128(inbound);', 'total = amount + uint128(inbound);'),
    ('fees: deposit leaves out the relay fee', 'total = amount + mintFee + uint128(inbound);', 'total = amount + mintFee;'),
    ('fees: deposit caps count the denomination only', '        _consumeDepositCaps(token, total);\n', '        _consumeDepositCaps(token, amount);\n'),
    ('fees: the leaf binds the total instead of the denomination', '_insertLeaf(leafIndex, _leafHash(leafIndex, msg.sender, token, amount, commitment));', '_insertLeaf(leafIndex, _leafHash(leafIndex, msg.sender, token, total, commitment));'),
    ('fees: a deposit does not put a due change in force', 'uint256 inbound = feeSchedules[token][_feesInForce(token)].inbound[denomIndex];', 'uint256 inbound = feeSchedules[token][feeState[token].current].inbound[denomIndex];'),
    ('fees: a release does not put a due change in force', 'uint256 outbound = schedules[r.token][applyFees(tokens, states, schedules, r.token)].outbound[r.denomIndex];', 'uint256 outbound = schedules[r.token][states[r.token].current].outbound[r.denomIndex];', 'src/FerrataRegistry.sol'),
    ('fees: release leaves out the burn fee', '        uint256 burnFee = outbound >> 128;\n', '        uint256 burnFee = 0;\n', 'src/FerrataRegistry.sol'),
    ('fees: release leaves out the checkpoint fee', '        uint256 checkpointFee = uint128(outbound);\n', '        uint256 checkpointFee = 0;\n', 'src/FerrataRegistry.sol'),
    ('fees: burn fee credited to the prover, not accrued for LEZ (ADR 0048)', 'if (burnFee != 0) accrueLezFees(r.token, burnFee);', 'if (burnFee != 0) { owed[prover][r.token] += burnFee; totalOwed[r.token] += burnFee; }', 'src/FerrataRegistry.sol'),
    ("fees: checkpoint fee credited to the release's sender, not the prover", '            owed[prover][r.token] += checkpointFee;\n', '            owed[msg.sender][r.token] += checkpointFee;\n', 'src/FerrataRegistry.sol'),
    ('fees: zero fees credited anyway', 'if (checkpointFee != 0) {', 'if (true) {', 'src/FerrataRegistry.sol'),
    ('fees: release fees not deducted from the recipient', 'r.recipient, amount - r.relayFee - burnFee - checkpointFee);', 'r.recipient, amount - r.relayFee);', 'src/FerrataRegistry.sol'),
    ('fees: a change applies one second late (>)', 'if (pendingAt != 0 && block.timestamp >= pendingAt) {', 'if (pendingAt != 0 && block.timestamp > pendingAt) {'),
    ('M-2: the prover recorded is tx.origin', '            prover: msg.sender\n', '            prover: tx.origin\n'),
    ('M-2: the prover not carried into the checkpoint record', '                prover: prover\n', '                prover: msg.sender\n'),
    ("fees: checkpoint fee to the release's sender", '        prover = checkpoints[entry - 1].prover;\n', '        prover = msg.sender;\n'),
    ('constructor: zero fee recipient allowed', ' || d.feeRecipient == address(0)) {', ') {'),
    ('fees: refund returns the denomination only', '        amount += cfg.relayFees[denomIndex];\n', '', 'src/FerrataRegistry.sol'),
    # ADR 0048 (the two refund-schedule mutants of ADR 0046's code replaced: a refund reads no schedule since then).
    ('fees: a deposit does not grow lezFees (ADR 0048)', '        if (mintFee != 0) accrueLezFees(token, mintFee);\n', '', 'src/FerrataRegistry.sol'),
    ('fees: a release does not grow lezFees by its burn fee (ADR 0048)', 'if (burnFee != 0) accrueLezFees(r.token, burnFee);', 'if (burnFee != 0) {}', 'src/FerrataRegistry.sol'),
    ('fees: notice of six days', 'uint256 private constant FEE_NOTICE = 7 days;', 'uint256 private constant FEE_NOTICE = 6 days;', 'src/FerrataRegistry.sol'),
    ('fees: ceiling 2%', 'uint256 private constant FEE_CEILING_DIVISOR = 100;', 'uint256 private constant FEE_CEILING_DIVISOR = 50;', 'src/FerrataRegistry.sol'),
    ('fees: mint fee ceiling not checked', 'if (mintFees[i] > ceiling || burnFees[i] > ceiling || checkpointFees[i] > ceiling) {', 'if (burnFees[i] > ceiling || checkpointFees[i] > ceiling) {', 'src/FerrataRegistry.sol'),
    ('fees: burn fee ceiling not checked', 'if (mintFees[i] > ceiling || burnFees[i] > ceiling || checkpointFees[i] > ceiling) {', 'if (mintFees[i] > ceiling || checkpointFees[i] > ceiling) {', 'src/FerrataRegistry.sol'),
    ('fees: checkpoint fee ceiling not checked', 'if (mintFees[i] > ceiling || burnFees[i] > ceiling || checkpointFees[i] > ceiling) {', 'if (mintFees[i] > ceiling || burnFees[i] > ceiling) {', 'src/FerrataRegistry.sol'),
    ('fees: mint fee need not be whole LEZ units', 'if (mintFees[i] % cfg.lezUnit != 0 || burnFees[i] % cfg.lezUnit != 0) {', 'if (burnFees[i] % cfg.lezUnit != 0) {', 'src/FerrataRegistry.sol'),
    ('fees: announced fee counts not checked', 'if (mintFees.length != k || burnFees.length != k || checkpointFees.length != k) {', 'if (mintFees.length != k) {', 'src/FerrataRegistry.sol'),
    ('fees: relay fee not carried into a schedule', 'uint256 inbound = mintFees[i] << 128 | cfg.relayFees[i];', 'uint256 inbound = mintFees[i] << 128;', 'src/FerrataRegistry.sol'),
    ('fees: announcing the fees in force does not withdraw', 'uint64 effectiveAt = same ? 0 : uint64(block.timestamp + FEE_NOTICE);', 'uint64 effectiveAt = uint64(block.timestamp + FEE_NOTICE);', 'src/FerrataRegistry.sol'),
    ("fees: the announcement's date not stored", '        states[token].pendingAt = effectiveAt;\n', '', 'src/FerrataRegistry.sol'),
    ('fees: a due change not put in force before an announcement', 'uint256 index = applyFees(tokens, states, schedules, token) + 1;', 'uint256 index = states[token].current + 1;', 'src/FerrataRegistry.sol'),
    ('fees: fromLeaf not recorded', '        s.fromLeaf = fromLeaf;\n        s.effectiveAt = uint64(block.timestamp);\n        st.current', '        s.effectiveAt = uint64(block.timestamp);\n        st.current', 'src/FerrataRegistry.sol'),
    ('fees: the effective time kept as the due time', '        s.effectiveAt = uint64(block.timestamp);\n        st.current', '        st.current', 'src/FerrataRegistry.sol'),
    ('fees: pending not cleared once in force', '        st.pendingAt = 0;\n', '', 'src/FerrataRegistry.sol'),
    ('fees: a schedule in force not published', 'bytes32 h = _publishSchedule(token, index, s, tokens[token].denominations.length);', 'bytes32 h = bytes32(0);', 'src/FerrataRegistry.sol'),
    ('fees: published hash leaves out fromLeaf', '                s.fromLeaf,\n', '                uint64(0),\n', 'src/FerrataRegistry.sol'),
    ('fees: fee slot leaves out the index', '_store(sha256(abi.encodePacked("ferrata/vault/fees/v1", token, uint64(index))), h);', '_store(sha256(abi.encodePacked("ferrata/vault/fees/v1", token)), h);', 'src/FerrataRegistry.sol'),
    ('fees: the announcement time not stored', '        next.announcedAt = uint64(block.timestamp);\n', '', 'src/FerrataRegistry.sol'),
    ('fees: schedule 0 checkpoint fee ceiling not checked', '            if (p.checkpointFees[i] > p.denominations[i] / FEE_CEILING_DIVISOR) revert FerrataVault.FeeTooHigh();\n', '', 'src/FerrataRegistry.sol'),
    ('fees: schedule 0 fee count not checked', '        if (p.checkpointFees.length != k) revert FerrataVault.FeeCountMismatch();\n', '', 'src/FerrataRegistry.sol'),
    ('fees: schedule 0 carries no relay fee', '            s.inbound[i] = p.relayFees[i];\n', '', 'src/FerrataRegistry.sol'),
    ('registry: entry not published', '        _store(sha256(abi.encodePacked("ferrata/vault/registry/v1", token)), h);\n', '', 'src/FerrataRegistry.sol'),
    ('registry: entry hash leaves out the relay fees', '                p.denominations,\n                p.relayFees\n', '                p.denominations\n', 'src/FerrataRegistry.sol'),
    ('registry: relay fee need not be whole LEZ units', '            if (fees[i] % lezUnit != 0) revert FerrataVault.FeeNotMultipleOfLezUnit();\n', '', 'src/FerrataRegistry.sol'),
    ('registry: lezUnit may exceed 10**decimals', '(decimals < 39 && lezUnit > 10 ** decimals)', '(decimals < 39 && lezUnit > 10 ** (decimals + 1))', 'src/FerrataRegistry.sol'),
    ('registry: twice the largest denomination unbounded in value units', '        if (2 * previous / lezUnit * unitWeight > type(uint128).max) revert FerrataVault.InvalidUnitWeight();\n', '', 'src/FerrataRegistry.sol'),
    ('registry: denomination bound back to 2**128 - 1', 'uint256 private constant MAX_DENOMINATION = type(uint128).max / 2;', 'uint256 private constant MAX_DENOMINATION = type(uint128).max;', 'src/FerrataRegistry.sol'),
    ('library: leaf count read from slot 2', 'count := and(sload(LEAF_COUNT_SLOT), 0xffffffffffffffff)', 'count := and(sload(2), 0xffffffffffffffff)', 'src/FerrataRegistry.sol'),
    ('library: deposit root read from slot 1', 'root := sload(DEPOSIT_ROOT_SLOT)', 'root := sload(1)', 'src/FerrataRegistry.sol'),
]


def workspace(start: int) -> str:
    return os.path.join(ROOT, "notes", f"mutation-workspace-{start}")


def prepare_workspace(work: str) -> None:
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work)
    with open(os.path.join(ROOT, "foundry.toml"), encoding="utf-8") as f:
        config = f.read()
    assert config.count("optimizer = true") == 1
    with open(os.path.join(work, "foundry.toml"), "w", encoding="utf-8") as f:
        f.write(config.replace("optimizer = true", "optimizer = false"))
    shutil.copytree(os.path.join(ROOT, "src"), os.path.join(work, "src"))
    shutil.copytree(os.path.join(ROOT, "test"), os.path.join(work, "test"))
    shutil.copytree(os.path.join(ROOT, "script"), os.path.join(work, "script"))  # test/real imports script/TestToken.sol
    os.symlink(os.path.join(ROOT, "lib"), os.path.join(work, "lib"))
    # test/PrivacySuite.t.sol reads ../target/privacy (relative to the workspace) and skips when no scenario exists
    # there; the directory must exist for the read permission, or the suite fails and every mutant looks killed.
    os.makedirs(os.path.join(os.path.dirname(work), "target", "privacy"), exist_ok=True)


def _forge_test(work: str, extra: list) -> tuple:
    env = dict(os.environ, FOUNDRY_DISABLE_NIGHTLY_WARNING="1")
    proc = subprocess.run(["forge", "test", "--root", work] + extra, capture_output=True, text=True, env=env)
    output = proc.stdout + proc.stderr
    # Each failing test appears twice (in its suite and in the final summary); count distinct ones.
    failing = sorted({line.strip() for line in output.splitlines() if line.startswith("[FAIL")})
    compile_error = "Compiler run failed" in output or "Error (" in output
    return proc.returncode, failing, compile_error


def run_suite(work: str) -> tuple:
    """All suites except the invariant campaign run in full, so every failing test is listed with its reason
    (no --fail-fast, which hides them). The invariant campaign runs only if everything else passed."""
    code, failing, compile_error = _forge_test(work, ["--no-match-path", "test/invariant/*"])
    if compile_error:
        return "compile error", ""
    if code != 0:
        return "killed", f"{len(failing)} failing, e.g. {failing[0][:120]}" if failing else "(no [FAIL] line)"
    code, failing, _ = _forge_test(work, ["--match-path", "test/invariant/*"])
    if code != 0:
        return "killed", f"only by invariants: {failing[0][:120]}" if failing else "(no [FAIL] line)"
    return "SURVIVED", ""


def main() -> int:
    originals = {}
    for path in (VAULT, REGISTRY):
        with open(os.path.join(ROOT, path), encoding="utf-8") as f:
            originals[path] = f.read()
    for mutant in MUTANTS:
        name, old, path = mutant[0], mutant[1], mutant[3] if len(mutant) > 3 else VAULT
        count = originals[path].count(old)
        if count != 1:
            print(f"mutant '{name}': original text found {count} times in {path}, expected once", file=sys.stderr)
            return 2

    start = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    stop = int(sys.argv[2]) if len(sys.argv) > 2 else len(MUTANTS)
    work = workspace(start)
    prepare_workspace(work)
    if os.environ.get("MUTATION_SKIP_BASELINE") != "1":
        code, failing, compile_error = _forge_test(work, ["--no-match-path", "test/invariant/*"])
        if code != 0 or compile_error:
            print(f"unmutated suite did not pass in the workspace: {failing[:2]}", file=sys.stderr)
            return 2

    results = []
    for number, mutant in enumerate(MUTANTS[start:stop], start=start):
        name, old, new = mutant[:3]
        target = mutant[3] if len(mutant) > 3 else VAULT
        for path, text in originals.items():
            with open(os.path.join(work, path), "w", encoding="utf-8") as f:
                f.write(text.replace(old, new) if path == target else text)
        status, first_failure = run_suite(work)
        results.append((name, status, first_failure))
        print(f"{number:3d} {status:13s} {name}   {first_failure}", flush=True)

    shutil.rmtree(work, ignore_errors=True)
    killed = sum(1 for _, status, _ in results if status == "killed")
    print(f"\n{killed}/{len(results)} mutants killed (MUTANTS[{start}:{stop}] of {len(MUTANTS)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
