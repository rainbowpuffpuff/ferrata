#!/usr/bin/env python3
"""Hand-written mutation spot-check for src/FerrataVault.sol.

Each mutant is one deliberate bug (a single textual replacement). For each one the script writes the mutated
contract into a scratch copy of the project (notes/mutation-workspace, with lib/ symlinked), runs the whole
`forge test` suite there with --fail-fast, and reports whether the suite failed ("killed") or still passed
("SURVIVED"). The real src/ is never modified.

Usage (from the project root): python3 -I tools/mutation_spotcheck.py [start [stop]]
The optional arguments select MUTANTS[start:stop], so a long run can be split. Needs forge on PATH. Honours TMPDIR.
"""

import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORK = os.path.join(ROOT, "notes", "mutation-workspace")
VAULT = "src/FerrataVault.sol"

# (name, exact text in the original, replacement). The original text must occur exactly once.
MUTANTS = [
    ("release: no nullifier check",
     "        if (nullifierUsed[r.nullifier]) revert NullifierAlreadyUsed();\n", ""),
    ("release: nullifier never marked used",
     "        nullifierUsed[r.nullifier] = true;\n", ""),
    ("release: no accepted-root check",
     "        if (acceptedAt == 0) revert UnknownBurnRoot();\n", ""),
    ("release: delay off by one (<=)",
     "if (block.timestamp < acceptedAt + releaseDelay[r.token])",
     "if (block.timestamp <= acceptedAt + releaseDelay[r.token])"),
    ("release: any fee up to the standard fee",
     "if (r.relayFee != 0 && r.relayFee != standardRelayFee[r.token][r.denomIndex])",
     "if (r.relayFee > standardRelayFee[r.token][r.denomIndex])"),
    ("release: no explicit escrow check",
     "        if (amount > escrowed[r.token]) revert InsufficientEscrow();\n", ""),
    ("release: zero recipient allowed",
     "        if (r.recipient == address(0)) revert ZeroRecipient();\n", ""),
    ("release: unset fee recipient pays the recipient",
     "r.feeRecipient == address(0) ? msg.sender : r.feeRecipient",
     "r.feeRecipient == address(0) ? r.recipient : r.feeRecipient"),
    ("release: recipient paid the full amount",
     "_pay(r.token, r.recipient, amount - r.relayFee);", "_pay(r.token, r.recipient, amount);"),
    ("release: failed ETH payout ignored",
     "            if (!ok) revert EthTransferFailed();\n", ""),
    ("release: escrow not decremented",
     "        escrowed[r.token] -= amount;\n", ""),
    ("release journal: vault address dropped",
     "                RELEASE_DOMAIN,\n                block.chainid,\n                address(this),",
     "                RELEASE_DOMAIN,\n                block.chainid,\n                address(0),"),
    ("release journal: recipient and fee recipient swapped",
     "                r.recipient,\n                r.feeRecipient,",
     "                r.feeRecipient,\n                r.recipient,"),
    ("root journal: chain id dropped",
     "abi.encode(ROOT_DOMAIN, block.chainid, address(this),", "abi.encode(ROOT_DOMAIN, uint256(0), address(this),"),
    ("root: depth check off by one (<=)",
     "if (depth < minLezDepth) revert InsufficientLezDepth();",
     "if (depth <= minLezDepth) revert InsufficientLezDepth();"),
    ("root: re-acceptance moves the timestamp",
     "if (burnRootAcceptedAt[burnRoot] == 0) {", "if (true) {"),
    ("root: not stopped by the release freeze",
     "        if (releasesFrozen) revert ReleasesAreFrozen();\n        if (depth < minLezDepth)",
     "        if (depth < minLezDepth)"),
    ("tree: left/right swapped where the path bit is 1",
     "node = _hashNode(_branch[h], node);", "node = _hashNode(node, _branch[h]);"),
    ("tree: branch saved at every left step (roots stay correct)",
     "                if (!branchSaved) {\n                    _branch[h] = node;\n                    branchSaved = true;\n                }",
     "                _branch[h] = node;\n                branchSaved = true;"),
    ("tree: empty-subtree hash not advanced",
     "            emptySubtree = _hashNode(emptySubtree, emptySubtree);\n        }\n        depositRoot = node;",
     "        }\n        depositRoot = node;"),
    ("tree: empty-tree root not set in the constructor",
     "        depositRoot = emptySubtree;\n", ""),
    ("leaf: index left out",
     "abi.encodePacked(LEAF_PREFIX, LEAF_TAG, leafIndex, token, amount, commitment)",
     "abi.encodePacked(LEAF_PREFIX, LEAF_TAG, uint64(0), token, amount, commitment)"),
    ("deposit: no index check",
     "        if (index != depositCount[msg.sender]) revert WrongDepositIndex();\n", ""),
    ("deposit: counter not advanced",
     "        depositCount[msg.sender] = index + 1;\n", ""),
    ("deposit: zero commitment allowed",
     "        if (commitment == bytes32(0)) revert ZeroCommitment();\n", ""),
    ("deposit: no exact-balance check (fee-on-transfer accepted)",
     "        if (erc20.balanceOf(address(this)) != balanceBefore + amount) revert TransferAmountMismatch();\n", ""),
    ("deposit: msg.value may exceed the denomination",
     "if (msg.value != amount) revert WrongEthAmount();", "if (msg.value < amount) revert WrongEthAmount();"),
    ("deposit: escrow not incremented",
     "        escrowed[token] += amount;\n", ""),
    ("deposit: per-token freeze ignored",
     "        if (tokenDepositsFrozen[token]) revert TokenDepositsAreFrozen();\n", ""),
    ("deposit: delisting ignored",
     "        if (!cfg.depositsEnabled) revert DepositsDisabled();\n", ""),
    ("caps: global deposit bucket not charged",
     "        if (!_tryConsume(_globalDepositCap, _valueUnits(token, amount))) revert GlobalDepositCapExceeded();\n",
     ""),
    ("caps: consumption does not reset the refill clock",
     "        b.level = level - amount;\n        b.lastUpdate = block.timestamp;\n",
     "        b.level = level - amount;\n"),
    ("caps: refill saturation off by one (>=)",
     "elapsed > missing / b.refillPerSecond", "elapsed >= missing / b.refillPerSecond"),
    ("caps: lowering a cap does not clamp the level",
     "        if (level > capacity) level = capacity;\n", ""),
    ("registry: lezUnit need not be a power of ten",
     "        if (rest != 1) revert InvalidLezUnit();\n", ""),
    ("registry: denominations need not be multiples of lezUnit",
     "            if (denoms[i] % lezUnit != 0) revert DenominationNotMultipleOfLezUnit();\n", ""),
    ("roles: freeze authority may become admin too",
     "        if (msg.sender == freezeAuthority) revert RolesMustDiffer();\n", ""),
    ("roles: admin may freeze token releases",
     "function setTokenReleasesFrozen(address token, bool frozen) external onlyFreezeAuthority",
     "function setTokenReleasesFrozen(address token, bool frozen) external onlyAdmin"),
]


def prepare_workspace() -> None:
    shutil.rmtree(WORK, ignore_errors=True)
    os.makedirs(WORK)
    shutil.copy(os.path.join(ROOT, "foundry.toml"), WORK)
    shutil.copytree(os.path.join(ROOT, "src"), os.path.join(WORK, "src"))
    shutil.copytree(os.path.join(ROOT, "test"), os.path.join(WORK, "test"))
    os.symlink(os.path.join(ROOT, "lib"), os.path.join(WORK, "lib"))


def _forge_test(extra: list) -> tuple:
    env = dict(os.environ, FOUNDRY_DISABLE_NIGHTLY_WARNING="1")
    proc = subprocess.run(["forge", "test", "--root", WORK] + extra, capture_output=True, text=True, env=env)
    output = proc.stdout + proc.stderr
    # Each failing test appears twice (in its suite and in the final summary); count distinct ones.
    failing = sorted({line.strip() for line in output.splitlines() if line.startswith("[FAIL")})
    compile_error = "Compiler run failed" in output or "Error (" in output
    return proc.returncode, failing, compile_error


def run_suite() -> tuple:
    """All suites except the invariant campaign run in full, so every failing test is listed with its reason
    (no --fail-fast, which hides them). The invariant campaign runs only if everything else passed."""
    code, failing, compile_error = _forge_test(["--no-match-path", "test/invariant/*"])
    if compile_error:
        return "compile error", ""
    if code != 0:
        return "killed", f"{len(failing)} failing, e.g. {failing[0][:120]}" if failing else "(no [FAIL] line)"
    code, failing, _ = _forge_test(["--match-path", "test/invariant/*"])
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

    prepare_workspace()
    status, _ = run_suite()
    if status != "SURVIVED":
        print(f"unmutated suite did not pass in the workspace ({status})", file=sys.stderr)
        return 2

    start = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    stop = int(sys.argv[2]) if len(sys.argv) > 2 else len(MUTANTS)
    results = []
    for name, old, new in MUTANTS[start:stop]:
        with open(os.path.join(WORK, VAULT), "w", encoding="utf-8") as f:
            f.write(original.replace(old, new))
        status, first_failure = run_suite()
        results.append((name, status, first_failure))
        print(f"{status:13s} {name}   {first_failure}", flush=True)

    with open(os.path.join(WORK, VAULT), "w", encoding="utf-8") as f:
        f.write(original)
    killed = sum(1 for _, status, _ in results if status == "killed")
    print(f"\n{killed}/{len(results)} mutants killed (MUTANTS[{start}:{stop}] of {len(MUTANTS)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
