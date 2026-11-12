#!/usr/bin/env python3
"""Extra mutants of the agent review of 12 November 2026 (docs/REVIEW-vault.md), VR01-VR25.

Each one is a plausible bug in a function of the review's table that none of mutation_spotcheck.py's 254 mutants
plants. The runner is mutation_spotcheck.py's own (same scratch workspace under notes/, optimizer off, every suite
but the invariant campaign, then the campaign for a survivor); only the list differs. A mutant that survives is a
behaviour no test pins.

Usage (from the project root): python3 -I tools/review_mutants.py [start [stop]]
"""

import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("mutation_spotcheck", os.path.join(HERE, "mutation_spotcheck.py"))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

R = m.REGISTRY

# (name, exact text in the original, replacement[, file]); the file defaults to the vault.
REVIEW_MUTANTS = [
    ("VR01 fees(): mint and relay fee swapped in the view",
     "return (inbound >> 128, uint128(inbound), outbound >> 128, uint128(outbound));",
     "return (uint128(inbound), inbound >> 128, outbound >> 128, uint128(outbound));"),
    ("VR02 burnRootStatus(): a revoked root still reported accepted",
     "if (entry == 0 || checkpoints[entry - 1].revoked) return (false, 0);",
     "if (entry == 0) return (false, 0);"),
    ("VR03 beacon: nonce without the vault's address",
     "keccak256(abi.encode(block.prevrandao, block.number, address(this)))",
     "keccak256(abi.encode(block.prevrandao, block.number))"),
    ("VR04 _finalize: the candidate is not deleted",
     "        delete candidates[c.newState];\n",
     ""),
    ("VR05 revokeFrom: revoking the sunset checkpoint itself keeps the boundary (>= -> >)",
     "if (sunsetLeafCountSet && sunsetSetAt >= index) {",
     "if (sunsetLeafCountSet && sunsetSetAt > index) {"),
    ("VR06 refundAmount: the leaf binds the caller instead of the named depositor",
     "bytes32 node = sha256(abi.encodePacked(LEAF_PREFIX, LEAF_TAG, leafIndex, depositor, token, amount, commitment));",
     "bytes32 node = sha256(abi.encodePacked(LEAF_PREFIX, LEAF_TAG, leafIndex, msg.sender, token, amount, commitment));",
     R),
    ("VR07 _inboundFeesOfLeaf: the schedule in force left out of the search",
     "uint256 high = uint256(states[token].current) + 1;",
     "uint256 high = uint256(states[token].current);",
     R),
    ("VR08 acceptAdmin: pendingAdmin not cleared",
     "        pendingAdmin = address(0);\n",
     ""),
    ("VR09 release: the relay fee is never paid",
     "        if (r.relayFee != 0) _payOrCredit(r.token, feePayee, r.relayFee);\n",
     ""),
    ("VR10 constructor: minReleaseDelay may exceed 30 days",
     "if (d.minReleaseDelay > d.revokeWindow || d.minReleaseDelay > MAX_RELEASE_DELAY) {",
     "if (d.minReleaseDelay > d.revokeWindow) {"),
    ("VR11 constructor: initialDelta above the 30-day ceiling",
     "|| d.initialDelta > MAX_DELTA) revert InvalidDelta();",
     ") revert InvalidDelta();"),
    ("VR12 constructor: a zero maxFreeze accepted",
     "        if (d.maxFreeze == 0) revert InvalidMaxFreeze();\n",
     ""),
    ("VR13 _trySendEth: only 2300 gas forwarded",
     "ok := call(gas(), to, amount, 0, 0, 0, 0)",
     "ok := call(2300, to, amount, 0, 0, 0, 0)"),
    ("VR14 _valueUnits: no division by lezUnit",
     "return amount * cfg.unitWeight / cfg.lezUnit;",
     "return amount * cfg.unitWeight;"),
    ("VR15 library revokeFrom: the first checkpoint not marked",
     "for (uint256 i = index; i < end; i++) {",
     "for (uint256 i = index + 1; i < end; i++) {",
     R),
    ("VR16 finalize (A): the second proof's journal carries echoNonce 0",
     "verifier.verify(seal, checkpointImageId, _checkpointDigest(c, c.echoNonce));",
     "verifier.verify(seal, checkpointImageId, _checkpointDigest(c, bytes32(0)));"),
    ("VR17 setReleaseDelay: a new delay applies to every root, finalised or not",
     "FerrataRegistry.setReleaseDelay(_tokens, _releaseDelays, token, delay, checkpoints.length);",
     "FerrataRegistry.setReleaseDelay(_tokens, _releaseDelays, token, delay, 0);"),
    ("VR18 _isWatcher: the middle slot ignored",
     "who == watchers[0] || who == watchers[1] || who == watchers[2]",
     "who == watchers[0] || who == watchers[0] || who == watchers[2]"),
    ("VR19 CandidateSubmitted: newState in place of tipHash (what monitor compares)",
     "            delta,\n            c.tipId,\n            c.tipHash,\n",
     "            delta,\n            c.tipId,\n            c.newState,\n"),
    ("VR20 Release event: the bound relayer field instead of the fee's payee",
     "emit Release(r.nullifier, r.token, r.recipient, amount, feePayee, r.relayFee, r.burnRoot);",
     "emit Release(r.nullifier, r.token, r.recipient, amount, r.relayer, r.relayFee, r.burnRoot);"),
    ("VR21 tokenCap(): the stored level without its refill",
     "        Bucket storage b = _tokenBucket(token, direction);\n        return (b.capacity, b.refillPerSecond, _available(b));",
     "        Bucket storage b = _tokenBucket(token, direction);\n        return (b.capacity, b.refillPerSecond, b.level);"),
    ("VR22 registry: a relay fee of exactly 1% refused (> -> >=)",
     "if (fees[i] > denoms[i] / FEE_CEILING_DIVISOR) revert FerrataVault.RelayFeeTooHigh();",
     "if (fees[i] >= denoms[i] / FEE_CEILING_DIVISOR) revert FerrataVault.RelayFeeTooHigh();",
     R),
    ("VR23 announceFees: a mint fee of exactly 1% refused (> -> >=)",
     "if (mintFees[i] > ceiling || burnFees[i] > ceiling || checkpointFees[i] > ceiling) {",
     "if (mintFees[i] >= ceiling || burnFees[i] > ceiling || checkpointFees[i] > ceiling) {",
     R),
    ("VR24 refund: paid to the caller instead of the leaf's depositor",
     "_payOrCredit(token, depositor, amount);",
     "_payOrCredit(token, msg.sender, amount);"),
    ("VR25 layout: retired declared before freezeAuthority (LEZ reads byte 11 of slot 37)",
     "    address public freezeAuthority;\n    /// @notice Set once by `retire()`; deposits revert forever afterwards.\n    bool public retired;\n",
     "    /// @notice Set once by `retire()`; deposits revert forever afterwards.\n    bool public retired;\n    address public freezeAuthority;\n"),
]

if __name__ == "__main__":
    m.MUTANTS = REVIEW_MUTANTS
    # mutation_spotcheck.workspace() names the scratch copy by the slice start; keep the review's apart.
    m.workspace = lambda start: os.path.join(m.ROOT, "notes", f"review-mutation-workspace-{start}")
    # REVIEW_WITHOUT_NEW_TESTS=1: the first run, against the suite as it was (without test/ReviewVault.t.sol).
    if os.environ.get("REVIEW_WITHOUT_NEW_TESTS") == "1":
        prepare = m.prepare_workspace

        def prepare_without_review_tests(work: str) -> None:
            prepare(work)
            os.remove(os.path.join(work, "test", "ReviewVault.t.sol"))

        m.prepare_workspace = prepare_without_review_tests
    sys.exit(m.main())
