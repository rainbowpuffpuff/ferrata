#!/usr/bin/env python3
"""Hand-written mutation spot-check for src/FerrataVault.sol (v3).

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

ETH_CALL = """        assembly ("memory-safe") {
            ok := call(gas(), to, amount, 0, 0, 0, 0)
        }"""

# (name, exact text in the original, replacement). The original text must occur exactly once.
MUTANTS = [
    # ---- deposits and the tree
    ("deposit: allowed after retire()", "        if (retired) revert VaultRetired();\n", ""),
    ("deposit: global deposit freeze ignored", "        if (_frozen(depositsFrozenUntil)) revert DepositsAreFrozen();\n", ""),
    ("deposit: per-token deposit freeze ignored",
     "        if (_frozen(tokenDepositsFrozenUntil[token])) revert TokenDepositsAreFrozen();\n", ""),
    ("deposit: delisting ignored", "        if (!cfg.depositsEnabled) revert DepositsDisabled();\n", ""),
    ("deposit: zero commitment allowed", "        if (commitment == bytes32(0)) revert ZeroCommitment();\n", ""),
    ("deposit: no index check", "        if (index != depositCount[msg.sender]) revert WrongDepositIndex();\n", ""),
    ("deposit: counter not advanced", "        depositCount[msg.sender] = index + 1;\n", ""),
    ("deposit: no exact-balance check (fee-on-transfer accepted)",
     "        if (erc20.balanceOf(address(this)) != balanceBefore + amount) revert TransferAmountMismatch();\n", ""),
    ("deposit: msg.value may exceed the denomination",
     "if (msg.value != amount) revert WrongEthAmount();", "if (msg.value < amount) revert WrongEthAmount();"),
    ("deposit: escrow not incremented", "        escrowed[token] += amount;\n", ""),
    ("leaf: binds tx.origin instead of the depositor",
     "leafHash(leafIndex, msg.sender, token, amount, commitment)", "leafHash(leafIndex, tx.origin, token, amount, commitment)"),
    ("leaf: index left out",
     "abi.encodePacked(LEAF_PREFIX, LEAF_TAG, leafIndex, depositor, token, amount, commitment)",
     "abi.encodePacked(LEAF_PREFIX, LEAF_TAG, uint64(0), depositor, token, amount, commitment)"),
    ("tree: left/right swapped where the path bit is 1",
     "node = _hashNode(_branch[h], node);", "node = _hashNode(node, _branch[h]);"),
    ("tree: branch saved at every left step (roots stay correct)",
     "                if (!branchSaved) {\n                    _branch[h] = node;\n                    branchSaved = true;\n                }",
     "                _branch[h] = node;\n                branchSaved = true;"),
    ("tree: empty-subtree hash not advanced",
     "            emptySubtree = _hashNode(emptySubtree, emptySubtree);\n        }\n        depositRoot = node;",
     "        }\n        depositRoot = node;"),
    ("tree: empty-tree root not set in the constructor", "        depositRoot = emptySubtree;\n", ""),
    ("tree: a full tree accepts one more leaf (>)",
     "if (nextLeafIndex >= MAX_LEAVES) revert TreeFull();", "if (nextLeafIndex > MAX_LEAVES) revert TreeFull();"),
    ("storage: lezConfigId not copied to slot 2", "        storedLezConfigId = d.lezConfigId;\n", ""),
    # ---- registry
    ("registry: re-registration allowed", "        if (cfg.registered) revert TokenAlreadyRegistered();\n", ""),
    ("registry: relay fee ceiling 2% instead of 1%",
     "if (fees[i] > denoms[i] / 100) revert RelayFeeTooHigh();", "if (fees[i] > denoms[i] / 50) revert RelayFeeTooHigh();"),
    ("registry: relay fee count not checked",
     "        if (fees.length != denoms.length) revert RelayFeeCountMismatch();\n", ""),
    ("registry: lezUnit need not be a power of ten", "        if (rest != 1) revert InvalidLezUnit();\n", ""),
    ("registry: denominations need not be multiples of lezUnit",
     "            if (denoms[i] % lezUnit != 0) revert DenominationNotMultipleOfLezUnit();\n", ""),
    ("registry: release delay not checked at registration", "        _checkReleaseDelay(p.releaseDelay);\n", ""),
    ("delay: below minReleaseDelay allowed", "if (delay < minReleaseDelay || delay > revokeWindow ||", "if (delay > revokeWindow ||"),
    ("delay: above revokeWindow allowed", "if (delay < minReleaseDelay || delay > revokeWindow ||", "if (delay < minReleaseDelay ||"),
    ("constructor: turnSeconds unbounded",
     "        if (d.turnSeconds == 0 || d.turnSeconds > MAX_DELTA) revert InvalidTurnSeconds();\n", ""),
    ("constructor: minReleaseDelay may exceed revokeWindow",
     "if (d.minReleaseDelay > d.revokeWindow || d.minReleaseDelay > MAX_RELEASE_DELAY)",
     "if (d.minReleaseDelay > MAX_RELEASE_DELAY)"),
    # ---- candidates, beacons, finalisation
    ("submit: checkpoint freeze ignored",
     "external {\n        if (_frozen(checkpointsFrozenUntil)) revert CheckpointsAreFrozen();\n        if (c.parentState",
     "external {\n        if (c.parentState"),
    ("submit: parent need not be final", "        if (c.parentState != finalState) revert ParentNotFinal();\n", ""),
    ("submit: echo nonce may be set", "        if (c.echoNonce != bytes32(0)) revert EchoNonceNotZero();\n", ""),
    ("submit: committee not checked",
     "        if (c.echoNonce != bytes32(0)) revert EchoNonceNotZero();\n        _checkCommittee(c);\n",
     "        if (c.echoNonce != bytes32(0)) revert EchoNonceNotZero();\n"),
    ("committee: journal floors need not be the deployed ones",
     "        if (c.nMin != nMin || c.tMin != tMin) revert WrongCommitteeFloor();\n", ""),
    ("committee: key set size not checked", "if (c.n < nMin || c.t < tMin) revert KeySetBelowFloor();",
     "if (c.t < tMin) revert KeySetBelowFloor();"),
    ("committee: threshold not checked", "if (c.n < nMin || c.t < tMin) revert KeySetBelowFloor();",
     "if (c.n < nMin) revert KeySetBelowFloor();"),
    ("submit: candidate's t not stored (floor used)", "            t: c.t,\n", "            t: tMin,\n"),
    ("submit: resubmission restarts the wait",
     "        if (existing.submittedAt != 0 && existing.revocationEpoch == revocationEpoch) {\n            revert CandidateAlreadySubmitted();\n        }\n", ""),
    ("submit: delta not recorded", "            deltaAtSubmit: delta,", "            deltaAtSubmit: 0,"),
    ("submit: no beacon recorded",
     "        _recordBeacon();\n        _emitCandidateSubmitted(c);", "        _emitCandidateSubmitted(c);"),
    ("beacon: several per block",
     "        if (index != 0 && _lastBeaconBlock == block.number) return index - 1;\n", ""),
    ("beacon: nonce without prevrandao",
     "keccak256(abi.encode(block.prevrandao, block.number, address(this)))",
     "keccak256(abi.encode(block.number, address(this)))"),
    ("finalize: revocation does not stale old candidates",
     "if (cand.revocationEpoch != revocationEpoch || cand.parentState != finalState) revert StaleCandidate();",
     "if (cand.parentState != finalState) revert StaleCandidate();"),
    ("finalize: parent need not be final",
     "if (cand.revocationEpoch != revocationEpoch || cand.parentState != finalState) revert StaleCandidate();",
     "if (cand.revocationEpoch != revocationEpoch) revert StaleCandidate();"),
    ("finalize: fields need not match the candidate",
     "                || c.importedLeafCount != cand.importedLeafCount || c.sunset != cand.sunset\n        ) revert CandidateMismatch();\n",
     "                || c.importedLeafCount != cand.importedLeafCount || c.sunset != cand.sunset\n        ) {}\n"),
    ("finalize: checkpoint freeze ignored",
     "        if (_frozen(checkpointsFrozenUntil)) revert CheckpointsAreFrozen();\n        Candidate storage cand",
     "        Candidate storage cand"),
    ("finalize: committee not checked",
     "        ) revert CandidateMismatch();\n        _checkCommittee(c);\n", "        ) revert CandidateMismatch();\n"),
    ("finalize: beacon at exactly W refused (<=)",
     "if (beacon.timestamp < cand.submittedAt + _wait(cand)) revert BeaconTooEarly();",
     "if (beacon.timestamp <= cand.submittedAt + _wait(cand)) revert BeaconTooEarly();"),
    ("wait: current delta instead of the candidate's",
     "return cand.deltaAtSubmit + (uint256(cand.t) - 1) * turnSeconds;",
     "return delta + (uint256(cand.t) - 1) * turnSeconds;"),
    ("wait: no committee term (W = delta)",
     "return cand.deltaAtSubmit + (uint256(cand.t) - 1) * turnSeconds;", "return cand.deltaAtSubmit;"),
    ("wait: t turns instead of t - 1",
     "return cand.deltaAtSubmit + (uint256(cand.t) - 1) * turnSeconds;",
     "return cand.deltaAtSubmit + uint256(cand.t) * turnSeconds;"),
    ("finalize: imported leaf count may decrease",
     "        if (c.importedLeafCount < finalImportedLeafCount) revert ImportedLeafCountDecreased();\n", ""),
    ("finalize: sunset may be dropped",
     "if (finalSunset && (!c.sunset || c.importedLeafCount != finalImportedLeafCount)) revert SunsetIsFinal();",
     "if (finalSunset && c.importedLeafCount != finalImportedLeafCount) revert SunsetIsFinal();"),
    ("finalize: count may grow after a sunset",
     "if (finalSunset && (!c.sunset || c.importedLeafCount != finalImportedLeafCount)) revert SunsetIsFinal();",
     "if (finalSunset && !c.sunset) revert SunsetIsFinal();"),
    ("finalize: parent count not recorded",
     "                parentImportedLeafCount: finalImportedLeafCount,", "                parentImportedLeafCount: 0,"),
    ("finalize: parent sunset flag not recorded", "                parentSunset: finalSunset,", "                parentSunset: false,"),
    ("finalize: imported leaf count not tracked", "        finalImportedLeafCount = c.importedLeafCount;\n", ""),
    ("finalize: sunset flag not tracked", "        finalSunset = c.sunset;\n", ""),
    ("finalize: a later sunset checkpoint moves the boundary",
     "if (c.sunset && !sunsetLeafCountSet) {", "if (c.sunset) {"),
    ("finalize: sunset finalisation time not recorded", "            sunsetFinalizedAt = block.timestamp;\n", ""),
    ("finalize: echo nonce not checked", "        if (c.echoNonce != beacon.nonce) revert EchoNonceMismatch();\n", ""),
    ("finalize: finalState not advanced",
     "        finalState = c.newState;\n        finalImportedLeafCount", "        finalImportedLeafCount"),
    ("finalize: a re-proven root stays pointing at the revoked checkpoint",
     "if (previous == 0 || checkpoints[previous - 1].revoked) burnRootCheckpoint[c.burnRoot] = index + 1;",
     "if (previous == 0) burnRootCheckpoint[c.burnRoot] = index + 1;"),
    ("finalize: sunset not recorded", "            sunsetLeafCountSet = true;\n", ""),
    ("checkpoint journal: chain id dropped",
     "abi.encode(CHECKPOINT_DOMAIN, block.chainid, address(this),", "abi.encode(CHECKPOINT_DOMAIN, uint256(0), address(this),"),
    ("checkpoint journal: sunset and echoNonce swapped",
     "c.importedLeafCount, c.sunset, c.echoNonce);", "c.importedLeafCount, c.echoNonce, c.sunset);"),
    ("checkpoint journal: tip, n and t dropped (13 words)",
     "sha256(bytes.concat(head, body, tail))", "sha256(bytes.concat(head, body))"),
    ("checkpoint journal: n and t swapped", "abi.encode(c.tipId, c.tipHash, c.n, c.t)", "abi.encode(c.tipId, c.tipHash, c.t, c.n)"),
    # ---- revocation, retirement, delta
    ("cancel: anyone may cancel",
     "function cancelCandidate(bytes32 newState) external onlyFreezeAuthority {",
     "function cancelCandidate(bytes32 newState) external {"),
    ("cancel: unknown candidate accepted",
     "        Candidate storage cand = candidates[newState];\n        if (cand.submittedAt == 0) revert UnknownCandidate();\n",
     "        Candidate storage cand = candidates[newState];\n"),
    ("cancel: candidate not deleted",
     "        delete candidates[newState];\n        emit CandidateCancelled", "        emit CandidateCancelled"),
    ("submit: tip hash not stored", "            tipHash: c.tipHash,\n", "            tipHash: bytes32(0),\n"),
    ("revoke: window not enforced",
     "        if (block.timestamp > first.finalizedAt + revokeWindow) revert RevokeWindowPassed();\n", ""),
    ("revoke: window one second short (>=)",
     "if (block.timestamp > first.finalizedAt + revokeWindow) revert RevokeWindowPassed();",
     "if (block.timestamp >= first.finalizedAt + revokeWindow) revert RevokeWindowPassed();"),
    ("revoke: only the first checkpoint marked",
     "for (uint256 i = index; i < end; i++) {", "for (uint256 i = index; i < index + 1; i++) {"),
    ("revoke: finalState not restored", "        finalState = first.parentState;\n", ""),
    ("revoke: imported leaf count not restored", "        finalImportedLeafCount = first.parentImportedLeafCount;\n", ""),
    ("revoke: sunset flag not restored", "        finalSunset = first.parentSunset;\n", ""),
    ("revoke: sunset finalisation time not cleared", "            sunsetFinalizedAt = 0;\n", ""),
    ("revoke: pending candidates stay valid", "        revocationEpoch += 1;\n", ""),
    ("revoke: sunset boundary not cleared",
     "if (sunsetLeafCountSet && sunsetSetAt >= index) {", "if (false) {"),
    ("revoke: an already revoked checkpoint can be revoked again",
     "        if (first.revoked) revert CheckpointAlreadyRevoked();\n", ""),
    ("delta: can be lowered", "        if (newDelta < delta) revert DeltaCanOnlyRise();\n", ""),
    ("delta: no ceiling", "        if (newDelta > MAX_DELTA) revert DeltaTooLong();\n", ""),
    # ---- refunds
    ("refund: allowed before retire()", "        if (!retired) revert NotRetired();\n", ""),
    ("refund: release freeze ignored",
     "        if (!retired) revert NotRetired();\n        if (_frozen(releasesFrozenUntil)) revert ReleasesAreFrozen();\n",
     "        if (!retired) revert NotRetired();\n"),
    ("refund: open before revokeWindow has passed",
     "        if (block.timestamp < sunsetFinalizedAt + revokeWindow) revert RefundNotYetOpen();\n", ""),
    ("refund: opens one second late (<=)",
     "if (block.timestamp < sunsetFinalizedAt + revokeWindow) revert RefundNotYetOpen();",
     "if (block.timestamp <= sunsetFinalizedAt + revokeWindow) revert RefundNotYetOpen();"),
    ("refund: no explicit escrow check", "        if (amount > escrowed[token]) revert InsufficientEscrow();\n", ""),
    ("refund: allowed before any sunset",
     "if (!sunsetLeafCountSet || leafIndex < sunsetLeafCount) revert NotRefundable();",
     "if (leafIndex < sunsetLeafCount) revert NotRefundable();"),
    ("refund: allowed below the sunset leaf count",
     "if (!sunsetLeafCountSet || leafIndex < sunsetLeafCount) revert NotRefundable();",
     "if (!sunsetLeafCountSet) revert NotRefundable();"),
    ("refund: unknown leaf index not refused", "        if (leafIndex >= nextLeafIndex) revert UnknownLeaf();\n", ""),
    ("refund: double refund allowed", "        if (refunded[leafIndex]) revert AlreadyRefunded();\n", ""),
    ("refund: leaf not marked refunded", "        refunded[leafIndex] = true;\n", ""),
    ("refund: path not checked",
     "        if (_rootFromPath(leafIndex, leaf, path) != depositRoot) revert InvalidMerklePath();\n", ""),
    ("refund: path direction bits inverted",
     "_hashNode(path[h], node) : _hashNode(node, path[h]);", "_hashNode(node, path[h]) : _hashNode(path[h], node);"),
    ("refund: escrow not decremented",
     "        refunded[leafIndex] = true;\n        escrowed[token] -= amount;\n", "        refunded[leafIndex] = true;\n"),
    # ---- releases
    ("release: global release freeze ignored",
     "returns (uint256 amount) {\n        if (_frozen(releasesFrozenUntil)) revert ReleasesAreFrozen();\n",
     "returns (uint256 amount) {\n"),
    ("release: per-token release freeze ignored",
     "        if (_frozen(tokenReleasesFrozenUntil[r.token])) revert TokenReleasesAreFrozen();\n", ""),
    ("release: zero recipient allowed", "        if (r.recipient == address(0)) revert ZeroRecipient();\n", ""),
    ("release: revoked root still accepted",
     "if (entry == 0 || checkpoints[entry - 1].revoked) revert BurnRootNotAccepted();",
     "if (entry == 0) revert BurnRootNotAccepted();"),
    ("release: delay off by one (<=)",
     "if (block.timestamp < checkpoints[entry - 1].finalizedAt + delay) revert ReleaseDelayNotElapsed();",
     "if (block.timestamp <= checkpoints[entry - 1].finalizedAt + delay) revert ReleaseDelayNotElapsed();"),
    ("release: current delay instead of the root's",
     "uint256 delay = _releaseDelayFor(r.token, entry - 1);",
     "uint256 delay = _releaseDelayFor(r.token, checkpoints.length);"),
    ("release: delay history search off by one",
     "if (changes[mid].fromCheckpoint <= checkpointIndex) low = mid + 1;",
     "if (changes[mid].fromCheckpoint < checkpointIndex) low = mid + 1;"),
    ("release: no nullifier check", "        if (nullifierUsed[r.nullifier]) revert NullifierAlreadyUsed();\n", ""),
    ("release: nullifier never marked used", "        nullifierUsed[r.nullifier] = true;\n", ""),
    ("release: any fee up to the registered fee",
     "if (r.relayFee != 0 && r.relayFee != cfg.relayFees[r.denomIndex])",
     "if (r.relayFee > cfg.relayFees[r.denomIndex])"),
    ("release: no explicit escrow check", "        if (amount > escrowed[r.token]) revert InsufficientEscrow();\n", ""),
    ("release: caps not charged", "        _consumeReleaseCaps(r.token, amount);\n", ""),
    ("release: escrow not decremented", "        escrowed[r.token] -= amount;\n", ""),
    ("release: unset relayer pays the recipient",
     "r.relayer == address(0) ? msg.sender : r.relayer", "r.relayer == address(0) ? r.recipient : r.relayer"),
    ("release: recipient paid the full amount",
     "_payOrCredit(r.token, r.recipient, amount - r.relayFee);", "_payOrCredit(r.token, r.recipient, amount);"),
    ("release journal: vault address dropped",
     "                RELEASE_DOMAIN,\n                block.chainid,\n                address(this),",
     "                RELEASE_DOMAIN,\n                block.chainid,\n                address(0),"),
    ("release journal: recipient and relayer swapped",
     "                r.recipient,\n                r.relayer,", "                r.relayer,\n                r.recipient,"),
    # ---- pull payments
    ("pay: a failed payment is not credited", "            owed[payee][token] += amount;\n", ""),
    ("pay: totalOwed not tracked", "            totalOwed[token] += amount;\n", ""),
    ("pay: ETH call copies the return data", ETH_CALL, '        (ok,) = to.call{value: amount}("");'),
    ("claimOwed: credit not cleared", "        owed[msg.sender][token] = 0;\n", ""),
    ("claimOwed: a failed ETH claim does not revert",
     "            if (!_trySendEth(msg.sender, amount)) revert EthTransferFailed();\n", "            _trySendEth(msg.sender, amount);\n"),
    # ---- freezes and roles
    ("freeze: never expires", "        return block.timestamp < until;", "        return until != 0;"),
    ("freeze: lasts half of maxFreeze",
     "return frozen ? block.timestamp + maxFreeze : 0;", "return frozen ? block.timestamp + maxFreeze / 2 : 0;"),
    ("retire: a second retire() is accepted", "        if (retired) revert AlreadyRetired();\n", ""),
    ("roles: freeze authority may become admin too",
     "        if (msg.sender == freezeAuthority) revert RolesMustDiffer();\n", ""),
    ("roles: admin may freeze token releases",
     "function setTokenReleasesFrozen(address token, bool frozen) external onlyFreezeAuthority",
     "function setTokenReleasesFrozen(address token, bool frozen) external onlyAdmin"),
    # ---- caps
    ("caps: global deposit bucket not charged",
     "        if (!_tryConsume(_globalDepositCap, _valueUnits(token, amount))) revert GlobalDepositCapExceeded();\n", ""),
    ("caps: consumption does not reset the refill clock",
     "        b.level = level - amount;\n        b.lastUpdate = block.timestamp;\n", "        b.level = level - amount;\n"),
    ("caps: refill saturation off by one (>=)",
     "elapsed > missing / b.refillPerSecond", "elapsed >= missing / b.refillPerSecond"),
    ("caps: lowering a cap does not clamp the level", "        if (level > capacity) level = capacity;\n", ""),
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
    os.symlink(os.path.join(ROOT, "lib"), os.path.join(work, "lib"))


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
    with open(os.path.join(ROOT, VAULT), encoding="utf-8") as f:
        original = f.read()
    for name, old, _ in MUTANTS:
        count = original.count(old)
        if count != 1:
            print(f"mutant '{name}': original text found {count} times, expected once", file=sys.stderr)
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
    for number, (name, old, new) in enumerate(MUTANTS[start:stop], start=start):
        with open(os.path.join(work, VAULT), "w", encoding="utf-8") as f:
            f.write(original.replace(old, new))
        status, first_failure = run_suite(work)
        results.append((name, status, first_failure))
        print(f"{number:3d} {status:13s} {name}   {first_failure}", flush=True)

    shutil.rmtree(work, ignore_errors=True)
    killed = sum(1 for _, status, _ in results if status == "killed")
    print(f"\n{killed}/{len(results)} mutants killed (MUTANTS[{start}:{stop}] of {len(MUTANTS)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
