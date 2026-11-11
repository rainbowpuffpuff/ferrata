// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../../src/FerrataVault.sol";
import {VaultTestBase} from "../utils/VaultTestBase.sol";

/// @dev Halmos's cheat codes (halmos-cheatcodes' SVM), declared here so the suite needs no extra library.
interface SVM {
    function createCalldata(string memory contractName) external returns (bytes memory);
}

/// @notice Symbolic checks of the vault's core properties, run by Halmos (`check_*` functions; `forge test` does not
///         run them). Every argument of a `check_*` function is symbolic: Halmos proves the assertions for all values,
///         on every path from the state `setUp` builds. That state is the shared fixture's vault (rule (A), RISC Zero's
///         emergency-stop wrapper around an accept-what-is-registered verifier, ETH, USDC, DAI and USDT registered),
///         with three deposits, a finalised checkpoint with root R1, a finalised and revoked one with root R2, and a
///         release of nullifier N0 on R1 to a payee that refuses ETH (so the vault owes it). After `setUp` the verifier
///         accepts no release statement: a check makes it accept one only where it says so.
///         The verifier stands for RISC Zero's: `verify` returns only for a statement it was given, the soundness
///         assumption every proof system check rests on. Run: scripts/security-tools.sh halmos.
contract VaultHalmos is VaultTestBase {
    SVM internal constant SYM = SVM(address(uint160(uint256(keccak256("svm cheat code")))));

    bytes32 internal constant R1 = keccak256("halmos: accepted burn root");
    bytes32 internal constant R2 = keccak256("halmos: revoked burn root");
    bytes32 internal constant R3 = keccak256("halmos: sunset burn root");
    bytes32 internal constant N0 = keccak256("halmos: used nullifier");

    struct Books {
        uint256 ethBalance;
        uint256 usdcBalance;
        uint256 ethEscrow;
        uint256 usdcEscrow;
        uint256 ethOwed;
        uint256 usdcOwed;
        uint256 payeeOwed;
    }

    function setUp() public override {
        super.setUp();
        _deposit(alice, ETH, 1, keccak256("halmos: c1"));
        _deposit(bob, ETH, 1, keccak256("halmos: c2"));
        _deposit(alice, address(usdc), 1, keccak256("halmos: c3"));
        _acceptRoot(R1); // checkpoint 0
        _acceptRoot(R2); // checkpoint 1
        vm.prank(freezer);
        vault.revokeFrom(1);
        // N0 spent on R1; its payee (the USDT mock, no receive function) refuses ETH, so it is owed instead.
        FerrataVault.ReleaseRequest memory r0 = _request(ETH, 1, address(usdt), R1, N0);
        _release(r0);
        _forgetRelease(r0);
    }

    // --------------------------------------------------------------------------------------------------------
    // 1. A release needs a proof of exactly its statement, an accepted unrevoked root and a fresh nullifier;
    //    what leaves equals what the escrow gives up; the same request never pays twice.
    // --------------------------------------------------------------------------------------------------------

    function check_release_needs_proof_root_and_fresh_nullifier(
        bool useEth,
        uint8 denomIndex,
        address to,
        address relayerAddr,
        uint256 relayFee,
        bytes32 burnRoot,
        bytes32 nullifier,
        bool proven,
        address caller
    ) public {
        address token = useEth ? ETH : address(usdc);
        vm.assume(denomIndex < 3);
        denomIndex = _index(denomIndex);
        vm.assume(to != address(vault) && relayerAddr != address(vault) && caller != address(vault));
        FerrataVault.ReleaseRequest memory r = FerrataVault.ReleaseRequest({
            token: token,
            denomIndex: denomIndex,
            recipient: to,
            relayer: relayerAddr,
            relayFee: relayFee,
            burnRoot: burnRoot,
            nullifier: nullifier
        });
        if (proven) _proveRelease(_releaseJournal(r));
        bool usedBefore = vault.nullifierUsed(nullifier);
        uint256 amount = _amount(token, denomIndex);
        uint256[3] memory before = _tokenBooks(token); // escrowed, totalOwed, balance

        vm.prank(caller);
        (bool ok,) = address(vault).call(abi.encodeCall(FerrataVault.release, (r, SEAL)));
        if (!ok) return;

        assert(proven); // no release without the verifier's acceptance of this very statement
        assert(burnRoot == R1); // the only finalised root that is not revoked
        assert(!usedBefore && vault.nullifierUsed(nullifier));
        uint256[3] memory b1 = _tokenBooks(token);
        assert(before[0] - b1[0] == amount);
        assert(b1[2] <= before[2] && b1[1] >= before[1]);
        // paid out plus newly owed: the denomination, no more
        assert((before[2] - b1[2]) + (b1[1] - before[1]) == amount);
        assert(b1[2] >= b1[0] + b1[1]); // still solvent for this token

        vm.prank(caller);
        (bool again,) = address(vault).call(abi.encodeCall(FerrataVault.release, (r, SEAL)));
        assert(!again);
    }

    // --------------------------------------------------------------------------------------------------------
    // 2. Fees within their bounds: what the admin can announce or register.
    // --------------------------------------------------------------------------------------------------------

    function check_announced_fees_within_bounds(
        bool useEth,
        uint256 m0,
        uint256 m1,
        uint256 m2,
        uint256 b0,
        uint256 b1,
        uint256 b2,
        uint256 c0,
        uint256 c1,
        uint256 c2
    ) public {
        address token = useEth ? ETH : address(usdc);
        uint256 lezUnit = useEth ? 1e12 : 1;
        uint256[] memory mintF = _three(m0, m1, m2);
        uint256[] memory burnF = _three(b0, b1, b2);
        uint256[] memory cpF = _three(c0, c1, c2);
        vm.prank(admin);
        (bool ok,) = address(vault).call(abi.encodeCall(FerrataVault.announceFees, (token, mintF, burnF, cpF)));
        if (!ok) return;
        for (uint8 i = 0; i < 3; i++) {
            uint256 ceiling = _amount(token, i) / 100;
            assert(mintF[i] <= ceiling && burnF[i] <= ceiling && cpF[i] <= ceiling);
            assert(mintF[i] % lezUnit == 0);
        }
    }

    function check_registered_fees_within_bounds(uint256 d0, uint256 d1, uint256 f0, uint256 f1, uint256 k0, uint256 k1)
        public
    {
        address token = address(new FreshToken());
        uint256[] memory denoms = new uint256[](2);
        denoms[0] = d0;
        denoms[1] = d1;
        FerrataVault.TokenParams memory p = _params(denoms, 1e12, 1);
        p.relayFees[0] = f0;
        p.relayFees[1] = f1;
        p.checkpointFees[0] = k0;
        p.checkpointFees[1] = k1;
        vm.prank(admin);
        (bool ok,) = address(vault).call(abi.encodeCall(FerrataVault.registerToken, (token, p)));
        if (!ok) return;
        assert(f0 <= d0 / 100 && f1 <= d1 / 100 && k0 <= d0 / 100 && k1 <= d1 / 100);
        assert(f0 % 1e12 == 0 && f1 % 1e12 == 0);
    }

    // --------------------------------------------------------------------------------------------------------
    // 3. No admin, freeze-authority or watcher call moves a user's funds: any function, any arguments.
    // --------------------------------------------------------------------------------------------------------

    function check_no_role_call_moves_funds(uint8 role) public {
        address caller = _role(role);
        bytes memory data = SYM.createCalldata("FerrataVault");
        Books memory s0 = _books();
        vm.prank(caller);
        (bool ok,) = address(vault).call(data);
        ok;
        _assertNothingLeft(s0);
    }

    /// The freeze authority may retire the vault at any time; even then no role's next call moves funds.
    function check_no_role_call_moves_funds_after_retire(uint8 role) public {
        vm.prank(freezer);
        vault.retire();
        address caller = _role(role);
        bytes memory data = SYM.createCalldata("FerrataVault");
        Books memory s0 = _books();
        vm.prank(caller);
        (bool ok,) = address(vault).call(data);
        ok;
        _assertNothingLeft(s0);
    }

    /// The same property, function by function: each admin, freeze-authority and watcher entry point with symbolic
    /// arguments (tokens ETH, USDC or a new one), called by any of the three roles, twice in a row (so a role can
    /// first set something up, e.g. retire, freeze, revoke or re-point the freeze authority, then act again).
    function check_role_entry_points_move_no_funds(
        uint8 role1,
        uint8 fn1,
        uint8 role2,
        uint8 fn2,
        uint8 t,
        uint256 a,
        uint256 b,
        bool flag,
        address who
    ) public {
        address token = t == 0 ? ETH : t == 1 ? address(usdc) : address(new FreshToken());
        Books memory s0 = _books();
        _roleCall(_role(role1), fn1, token, a, b, flag, who);
        _roleCall(_role(role2), fn2, token, a, b, flag, who);
        _assertNothingLeft(s0);
    }

    function _roleCall(address caller, uint8 fn, address token, uint256 a, uint256 b, bool flag, address who)
        internal
    {
        bytes memory data;
        if (fn == 0) data = abi.encodeCall(FerrataVault.setDepositsEnabled, (token, flag));
        else if (fn == 1) data = abi.encodeCall(FerrataVault.setTokenCap, (token, FerrataVault.Direction(a % 2), a, b));
        else if (fn == 2) data = abi.encodeCall(FerrataVault.setGlobalCap, (FerrataVault.Direction(b % 2), a, b));
        else if (fn == 3) data = abi.encodeCall(FerrataVault.setReleaseDelay, (token, a));
        else if (fn == 4) data = abi.encodeCall(FerrataVault.raiseDelta, (a));
        else if (fn == 5) data = abi.encodeCall(FerrataVault.setDepositsFrozen, (flag));
        else if (fn == 6) data = abi.encodeCall(FerrataVault.setReleasesFrozen, (flag));
        else if (fn == 7) data = abi.encodeCall(FerrataVault.setCheckpointsFrozen, (flag));
        else if (fn == 8) data = abi.encodeCall(FerrataVault.setTokenDepositsFrozen, (token, flag));
        else if (fn == 9) data = abi.encodeCall(FerrataVault.setTokenReleasesFrozen, (token, flag));
        else if (fn == 10) data = abi.encodeCall(FerrataVault.retire, ());
        else if (fn == 11) data = abi.encodeCall(FerrataVault.cancelCandidate, (bytes32(a)));
        else if (fn == 12) data = abi.encodeCall(FerrataVault.pauseCheckpoints, ());
        else if (fn == 13) data = abi.encodeCall(FerrataVault.revokeFrom, (a % 2));
        else if (fn == 14) data = abi.encodeCall(FerrataVault.transferAdmin, (who));
        else if (fn == 15) data = abi.encodeCall(FerrataVault.acceptAdmin, ());
        else if (fn == 16) data = abi.encodeCall(FerrataVault.proposeRoles, ([who, address(0), address(0)], who));
        else if (fn == 17) data = abi.encodeCall(FerrataVault.applyRoles, ());
        else if (fn == 18) data = abi.encodeCall(FerrataVault.announceFees, (token, _three(0, a, 0), _three(b, 0, 0), _three(0, 0, a)));
        else if (fn == 19) data = abi.encodeCall(FerrataVault.applyFees, (token));
        else if (fn == 20) data = abi.encodeCall(FerrataVault.claimOwed, (token));
        else data = abi.encodeCall(FerrataVault.registerToken, (token, _params(_three(1e18, 2e18, 3e18), 1e12, a % 1e6 + 1)));
        vm.prank(caller);
        (bool ok,) = address(vault).call(data);
        ok;
    }

    // --------------------------------------------------------------------------------------------------------
    // 4. The verifier stop: afterwards nothing is released, refunded or finalised, whoever calls what; only owed
    //    balances can still be withdrawn (README, "Known limits").
    // --------------------------------------------------------------------------------------------------------

    function check_after_stop_only_owed_leaves(address caller) public {
        vm.prank(freezer);
        estop.estop();
        bytes memory data = SYM.createCalldata("FerrataVault");
        Books memory s0 = _books();
        uint256 count0 = vault.checkpointCount();
        vm.prank(caller);
        (bool ok,) = address(vault).call(data);
        ok;
        Books memory s1 = _books();
        assert(s1.ethEscrow >= s0.ethEscrow && s1.usdcEscrow >= s0.usdcEscrow);
        assert(vault.checkpointCount() == count0);
        // the balance falls only by a withdrawal of what is owed, by exactly that
        if (s1.ethBalance < s0.ethBalance) assert(s0.ethBalance - s1.ethBalance == s0.ethOwed - s1.ethOwed);
        if (s1.usdcBalance < s0.usdcBalance) assert(s0.usdcBalance - s1.usdcBalance == s0.usdcOwed - s1.usdcOwed);
    }

    function check_after_stop_no_release_even_with_proof(
        bool useEth,
        uint8 denomIndex,
        address to,
        bytes32 nullifier,
        address caller
    ) public {
        vm.assume(denomIndex < 3);
        denomIndex = _index(denomIndex);
        vm.prank(freezer);
        estop.estop();
        FerrataVault.ReleaseRequest memory r = _request(useEth ? ETH : address(usdc), denomIndex, to, R1, nullifier);
        _proveRelease(_releaseJournal(r));
        vm.prank(caller);
        (bool ok,) = address(vault).call(abi.encodeCall(FerrataVault.release, (r, SEAL)));
        assert(!ok);
    }

    // --------------------------------------------------------------------------------------------------------
    // 5. Sunset: deposits close, every later checkpoint is a sunset one with the same count, and refunds pay only
    //    unimported leaves, once, after the revoke window, never after a stop, and exactly their escrow.
    // --------------------------------------------------------------------------------------------------------

    /// Finalises a sunset checkpoint that imported leaves 0 and 1 (leaf 2, Alice's USDC, was not imported).
    function _sunset() internal {
        FerrataVault.Checkpoint memory s = _nextCheckpoint(vault, R3, 2, true);
        _submit(s);
        _finalizeAfterDelta(s);
    }

    function check_sunset_closes_deposits(address from, bool useEth, uint8 denomIndex, bytes32 commitment, uint256 v)
        public
    {
        _sunset();
        vm.assume(from != address(vault));
        bool ok;
        if (useEth) {
            vm.deal(from, v);
            vm.prank(from);
            (ok,) = address(vault).call{value: v}(
                abi.encodeCall(FerrataVault.depositETH, (denomIndex, commitment, vault.depositCount(from)))
            );
        } else {
            usdc.mint(from, 1e12);
            vm.prank(from);
            usdc.approve(address(vault), type(uint256).max);
            vm.prank(from);
            (ok,) = address(vault).call(
                abi.encodeCall(FerrataVault.deposit, (address(usdc), denomIndex, commitment, vault.depositCount(from)))
            );
        }
        assert(!ok);
    }

    function check_sunset_is_final(
        bytes32 newState,
        bytes32 burnRoot,
        uint64 burnCount,
        uint64 importedLeafCount,
        bool sunset,
        uint64 tipId,
        bytes32 tipHash
    ) public {
        _sunset();
        FerrataVault.Checkpoint memory c = FerrataVault.Checkpoint({
            nMin: N_MIN,
            tMin: T_MIN,
            parentState: vault.finalState(),
            newState: newState,
            burnRoot: burnRoot,
            burnCount: burnCount,
            importedLeafCount: importedLeafCount,
            sunset: sunset,
            echoNonce: bytes32(0),
            tipId: tipId,
            tipHash: tipHash,
            n: N,
            t: T
        });
        uint256 count0 = vault.checkpointCount();
        _submit(c); // a revert here ends the path: nothing to check
        _finalizeAfterDelta(c);
        assert(vault.checkpointCount() == count0 + 1);
        assert(c.sunset && c.importedLeafCount == 2);
        assert(vault.sunsetLeafCount() == 2);
    }

    function check_refund_rules(
        uint64 leafIndex,
        address depositor,
        bool useEth,
        uint8 denomIndex,
        bytes32 commitment,
        bytes32[32] calldata path,
        uint256 wait,
        bool stop,
        address caller
    ) public {
        uint256 opens = _sunsetRetireAndWait(wait, stop);
        RefundArgs memory a = RefundArgs(leafIndex, depositor, useEth ? ETH : address(usdc), denomIndex, commitment);
        vm.assume(depositor != address(vault) && caller != address(vault));
        bool refundedBefore = vault.refunded(leafIndex);
        uint256[3] memory before = _tokenBooks(a.token);
        vm.prank(caller);
        bool ok = _callRefund(a, path);
        if (!ok) return;
        assert(!stop);
        assert(block.timestamp >= opens);
        assert(leafIndex >= 2 && leafIndex < vault.nextLeafIndex());
        assert(!refundedBefore && vault.refunded(leafIndex));
        // escrow gives up the leaf's amount; paid out plus newly owed is exactly that; still solvent
        uint256[3] memory b1 = _tokenBooks(a.token);
        assert(b1[0] < before[0] && b1[2] <= before[2] && b1[1] >= before[1]);
        assert((before[2] - b1[2]) + (b1[1] - before[1]) == before[0] - b1[0]);
        assert(b1[2] >= b1[0] + b1[1]);
    }

    struct RefundArgs {
        uint64 leafIndex;
        address depositor;
        address token;
        uint8 denomIndex;
        bytes32 commitment;
    }

    function _sunsetRetireAndWait(uint256 wait, bool stop) internal returns (uint256 opens) {
        _sunset();
        vm.prank(freezer);
        vault.retire();
        (, uint256 finalizedAt) = vault.burnRootStatus(R3);
        opens = finalizedAt + REVOKE_WINDOW + 1;
        vm.assume(wait <= 400 days);
        vm.warp(finalizedAt + wait);
        if (stop) {
            vm.prank(freezer);
            estop.estop();
        }
    }

    function _callRefund(RefundArgs memory a, bytes32[32] calldata path) internal returns (bool ok) {
        (ok,) = address(vault).call(
            abi.encodeCall(FerrataVault.refund, (a.leafIndex, a.depositor, a.token, a.denomIndex, a.commitment, path))
        );
    }

    /// @dev [escrowed, totalOwed, the vault's balance] of `token`.
    function _tokenBooks(address token) internal view returns (uint256[3] memory b) {
        b[0] = vault.escrowed(token);
        b[1] = vault.totalOwed(token);
        b[2] = _balance(token, address(vault));
    }

    // --------------------------------------------------------------------------------------------------------
    // 6. Reachability checks (`check_reach_*`): each asserts that a success the checks above rely on is
    //    impossible, and Halmos must refute it with a counterexample (an expected FAIL). They show that the PASS
    //    results above are not vacuous: the guarded calls do succeed on some paths.
    // --------------------------------------------------------------------------------------------------------

    function check_reach_release(bool useEth, uint8 denomIndex, address to, bytes32 nullifier) public {
        vm.assume(denomIndex < 3);
        denomIndex = _index(denomIndex);
        FerrataVault.ReleaseRequest memory r = _request(useEth ? ETH : address(usdc), denomIndex, to, R1, nullifier);
        _proveRelease(_releaseJournal(r));
        (bool ok,) = address(vault).call(abi.encodeCall(FerrataVault.release, (r, SEAL)));
        assert(!ok); // expected FAIL: a proven release on R1 with a fresh nullifier pays
    }

    function check_reach_role_call(uint8 role) public {
        bytes memory data = SYM.createCalldata("FerrataVault");
        vm.prank(_role(role));
        (bool ok,) = address(vault).call(data);
        assert(!ok); // expected FAIL: some role call succeeds
    }

    function check_reach_after_stop(address caller) public {
        vm.prank(freezer);
        estop.estop();
        bytes memory data = SYM.createCalldata("FerrataVault");
        vm.prank(caller);
        (bool ok,) = address(vault).call(data);
        assert(!ok); // expected FAIL: some call still succeeds after the stop
    }

    function check_reach_announce(uint256 m1) public {
        uint256[] memory z = _three(0, 0, 0);
        vm.prank(admin);
        (bool ok,) = address(vault).call(abi.encodeCall(FerrataVault.announceFees, (ETH, _three(0, m1, 0), z, z)));
        assert(!ok); // expected FAIL
    }

    function check_reach_register(uint256 d0) public {
        address token = address(new FreshToken());
        uint256[] memory denoms = new uint256[](1);
        denoms[0] = d0;
        FerrataVault.TokenParams memory p = _params(denoms, 1e12, 1);
        vm.prank(admin);
        (bool ok,) = address(vault).call(abi.encodeCall(FerrataVault.registerToken, (token, p)));
        assert(!ok); // expected FAIL
    }

    function check_reach_deposit(bytes32 commitment) public {
        vm.assume(commitment != bytes32(0));
        vm.deal(bob, 10 ether);
        uint256 total = _depositTotal(vault, ETH, 0);
        vm.prank(bob);
        (bool ok,) = address(vault).call{value: total}(
            abi.encodeCall(FerrataVault.depositETH, (0, commitment, vault.depositCount(bob)))
        );
        assert(!ok); // expected FAIL: before the sunset, deposits are open
    }

    function check_reach_sunset_checkpoint(bytes32 newState) public {
        _sunset();
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, R1, 2, true);
        c.newState = newState;
        uint256 count0 = vault.checkpointCount();
        _submit(c);
        _finalizeAfterDelta(c);
        assert(vault.checkpointCount() == count0); // expected FAIL: a later sunset checkpoint is finalised
    }

    function check_reach_refund(bytes32[32] calldata path, address caller) public {
        _sunsetRetireAndWait(REVOKE_WINDOW + 1, false);
        vm.assume(caller != address(vault));
        vm.prank(caller);
        bool ok = _callRefund(RefundArgs(2, alice, address(usdc), 1, keccak256("halmos: c3")), path);
        assert(!ok); // expected FAIL: leaf 2 (not imported) is refunded once the window has passed
    }

    // --------------------------------------------------------------------------------------------------------
    // Helpers
    // --------------------------------------------------------------------------------------------------------

    /// @dev The same index, made concrete on each path (the fixture's helpers index memory arrays with it, and
    ///      Halmos needs concrete memory offsets). Callers assume i < 3.
    function _index(uint8 i) internal pure returns (uint8) {
        if (i == 0) return 0;
        if (i == 1) return 1;
        return 2;
    }

    function _role(uint8 role) internal view returns (address) {
        return role == 0 ? admin : role == 1 ? freezer : watcher;
    }

    function _books() internal view returns (Books memory s) {
        s.ethBalance = address(vault).balance;
        s.usdcBalance = usdc.balanceOf(address(vault));
        s.ethEscrow = vault.escrowed(ETH);
        s.usdcEscrow = vault.escrowed(address(usdc));
        s.ethOwed = vault.totalOwed(ETH);
        s.usdcOwed = vault.totalOwed(address(usdc));
        s.payeeOwed = vault.owed(address(usdt), ETH);
    }

    function _assertNothingLeft(Books memory s0) internal view {
        Books memory s1 = _books();
        assert(s1.ethBalance >= s0.ethBalance && s1.usdcBalance >= s0.usdcBalance);
        assert(s1.ethEscrow >= s0.ethEscrow && s1.usdcEscrow >= s0.usdcEscrow);
        assert(s1.payeeOwed == s0.payeeOwed);
    }

    function _three(uint256 a, uint256 b, uint256 c) internal pure returns (uint256[] memory x) {
        x = new uint256[](3);
        x[0] = a;
        x[1] = b;
        x[2] = c;
    }

    /// @dev Withdraws the mock verifier's acceptance of a release journal (its `accepted` mapping is slot 0).
    function _forgetRelease(FerrataVault.ReleaseRequest memory r) internal {
        bytes32 digest = sha256(_releaseJournalBytes(_releaseJournal(r)));
        bytes32 slot = keccak256(abi.encode(digest, keccak256(abi.encode(RELEASE_IMAGE_ID, uint256(0)))));
        vm.store(address(verifier), slot, bytes32(0));
    }
}

/// @dev A contract with code, for registering a new token.
contract FreshToken {
    function balanceOf(address) external pure returns (uint256) {
        return 0;
    }
}
