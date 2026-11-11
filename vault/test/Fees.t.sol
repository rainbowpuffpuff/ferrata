// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Vm} from "forge-std/Vm.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";

import {ToggleRecipient} from "./mocks/MockRecipients.sol";
import {MockERC20} from "./mocks/MockTokens.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice v5, D13 fees (proposal §B4): a deposit escrows its denomination plus the mint and relay fees of the
///         schedule in force; a release pays the denomination less the relay, burn and checkpoint fees, the burn fee
///         credited to the fee recipient and the checkpoint fee to the root's prover; protocol fees start at zero
///         under a 1% ceiling and change seven days after they are announced; a deposit keeps the schedule of its leaf
///         index. Expected amounts and published hashes are worked out here from the written rules (and the hashes
///         from bridge-wire's byte layout), not read back from the vault.
contract FeesTest is VaultTestBase {
    /// A token registered at a fixed address, so the published slots have known answers (fee_vectors.py).
    address internal constant FEE_TOKEN = address(0xF00D);
    bytes32 internal constant LEZ_DEFINITION = bytes32(uint256(0xdef1));
    bytes32 internal constant REGISTRY_SLOT = 0x2d10ff07169bdd0eaf5b961ba721f69370176ca7b0ae9e13553e6760e6112231;
    bytes32 internal constant ENTRY_HASH = 0x40f0437ebe8d294824ffa71d53fa3003ffd623c865874adcd359e12100266b37;
    bytes32 internal constant FEE_SLOT_0 = 0xce22e268d8b199eaca1c11365dc0aeafff5fa8fee1acac1ad2b0f2474db2b482;
    bytes32 internal constant FEE_SLOT_1 = 0xae7eca610e386aab38649737d3fcebecaf5e474bdf7f48f1066cb82c0fad08eb;
    bytes32 internal constant SCHEDULE_0 = 0xb452725c78836908f18e1031ab3b0bb545f66a55adb1c57dcf9308320a3a4031;

    bytes32 internal constant ROOT = keccak256("fees: burn root");
    uint256 internal constant D0 = 1e18;
    uint256 internal constant D1 = 10e18;
    uint256 internal constant RELAY0 = 2e15;
    uint256 internal constant RELAY1 = 2e16;
    uint256 internal constant CHECKPOINT0 = 1e15; // schedule 0's checkpoint fees
    uint256 internal constant CHECKPOINT1 = 1e16;

    MockERC20 internal t;
    uint256 internal nonce;
    bytes32[] internal leaves;

    function setUp() public override {
        super.setUp();
        vm.etch(FEE_TOKEN, address(new MockERC20("Fee token", "FEE", 18)).code);
        t = MockERC20(FEE_TOKEN);
        vm.prank(admin);
        vault.registerToken(FEE_TOKEN, _feeTokenParams());
    }

    // ---- helpers ------------------------------------------------------------------------------------------------

    function _two(uint256 a, uint256 b) internal pure returns (uint256[] memory x) {
        x = new uint256[](2);
        (x[0], x[1]) = (a, b);
    }

    function _feeTokenParams() internal pure returns (FerrataVault.TokenParams memory) {
        return FerrataVault.TokenParams({
            denominations: _two(D0, D1),
            relayFees: _two(RELAY0, RELAY1),
            lezUnit: 1e12,
            unitWeight: 2000,
            depositCap: _unlimited(),
            releaseCap: _unlimited(),
            releaseDelay: 0,
            lezDefinition: LEZ_DEFINITION,
            decimals: 18,
            checkpointFees: _two(CHECKPOINT0, CHECKPOINT1)
        });
    }

    /// @dev bridge-wire `fee_slot`.
    function _feeSlot(address token, uint64 index) internal pure returns (bytes32) {
        return sha256(bytes.concat(bytes("ferrata/vault/fees/v1"), bytes20(token), bytes8(index)));
    }

    /// @dev bridge-wire `FeeSchedule::hash`, byte by byte.
    function _scheduleHash(
        address token,
        uint64 index,
        uint64[3] memory times, // fromLeaf, announcedAt, effectiveAt
        uint256[] memory mintFees,
        uint256[] memory burnFees,
        uint256[] memory checkpointFees
    ) internal pure returns (bytes32) {
        bytes memory m = bytes.concat(
            bytes("ferrata/fees/v1"),
            bytes20(token),
            bytes8(index),
            bytes8(times[0]),
            bytes8(times[1]),
            bytes8(times[2]),
            bytes1(uint8(mintFees.length))
        );
        for (uint256 i = 0; i < mintFees.length; i++) {
            m = bytes.concat(m, bytes32(mintFees[i]));
        }
        for (uint256 i = 0; i < burnFees.length; i++) {
            m = bytes.concat(m, bytes32(burnFees[i]));
        }
        for (uint256 i = 0; i < checkpointFees.length; i++) {
            m = bytes.concat(m, bytes32(checkpointFees[i]));
        }
        return sha256(m);
    }

    /// @dev bridge-wire `TokenEntry::hash`, byte by byte.
    function _entryHash(address token, FerrataVault.TokenParams memory p) internal pure returns (bytes32) {
        bytes memory m = bytes.concat(
            bytes("ferrata/registry/v1"),
            bytes20(token),
            p.lezDefinition,
            bytes1(p.decimals),
            bytes32(p.lezUnit),
            bytes32(p.unitWeight),
            bytes1(uint8(p.denominations.length))
        );
        for (uint256 i = 0; i < p.denominations.length; i++) {
            m = bytes.concat(m, bytes32(p.denominations[i]));
        }
        for (uint256 i = 0; i < p.relayFees.length; i++) {
            m = bytes.concat(m, bytes32(p.relayFees[i]));
        }
        return sha256(m);
    }

    function _published(uint64 index) internal view returns (bytes32) {
        return vm.load(address(vault), _feeSlot(FEE_TOKEN, index));
    }

    function _announce(uint256[] memory mintFees, uint256[] memory burnFees, uint256[] memory checkpointFees)
        internal
    {
        vm.prank(admin);
        vault.announceFees(FEE_TOKEN, mintFees, burnFees, checkpointFees);
    }

    /// @dev Deposits `amount` of the fee token as `who` (funding and approving exactly that), records the leaf, and
    ///      returns what the vault's balance gained.
    function _depositExactly(address who, uint8 denomIndex, uint256 amount) internal returns (uint256 escrowed) {
        t.mint(who, amount);
        vm.prank(who);
        t.approve(address(vault), amount);
        uint256 before = t.balanceOf(address(vault));
        uint64 leafIndex = vault.nextLeafIndex();
        bytes32 commitment = keccak256(abi.encode("fees commitment", nonce++));
        uint64 index = vault.depositCount(who); // before the prank, which the next call consumes
        vm.prank(who);
        vault.deposit(FEE_TOKEN, denomIndex, commitment, index);
        leaves.push(ReferenceMerkle.leafHash(leafIndex, who, FEE_TOKEN, denomIndex == 0 ? D0 : D1, commitment));
        _leafData.push(LeafData(who, denomIndex, commitment));
        return t.balanceOf(address(vault)) - before;
    }

    struct LeafData {
        address depositor;
        uint8 denomIndex;
        bytes32 commitment;
    }

    LeafData[] internal _leafData;

    function _refundLeaf(uint64 leafIndex) internal {
        LeafData memory l = _leafData[leafIndex];
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, leafIndex);
        bytes32[32] memory path;
        for (uint256 h = 0; h < 32; h++) {
            path[h] = siblings[h];
        }
        vault.refund(leafIndex, l.depositor, FEE_TOKEN, l.denomIndex, l.commitment, path);
    }

    function _release(uint8 denomIndex, address to, uint256 relayFee, bytes32 nullifier) internal {
        FerrataVault.ReleaseRequest memory r = _request(FEE_TOKEN, denomIndex, to, ROOT, nullifier);
        r.relayFee = relayFee;
        _release(r);
    }

    // ---- registration: schedule 0 and the published entry ----------------------------------------------------------

    /// @notice The registry entry and schedule 0 sit at bridge-wire's SHA-256 slots with bridge-wire's hashes
    ///         (known answers from fee_vectors.py); schedule 1's slot stays zero until a schedule 1 is in force.
    function test_registration_publishesTheEntryAndScheduleZeroAtTheirSlots() public view {
        assertEq(vm.load(address(vault), REGISTRY_SLOT), ENTRY_HASH);
        assertEq(vm.load(address(vault), FEE_SLOT_0), SCHEDULE_0);
        assertEq(vm.load(address(vault), FEE_SLOT_1), bytes32(0));
        // The same from the definitions, computed here.
        assertEq(sha256(bytes.concat(bytes("ferrata/vault/registry/v1"), bytes20(FEE_TOKEN))), REGISTRY_SLOT);
        assertEq(_entryHash(FEE_TOKEN, _feeTokenParams()), ENTRY_HASH);
        assertEq(_feeSlot(FEE_TOKEN, 0), FEE_SLOT_0);
        assertEq(_feeSlot(FEE_TOKEN, 1), FEE_SLOT_1);
        assertEq(
            _scheduleHash(FEE_TOKEN, 0, [uint64(0), 1, 1], _two(0, 0), _two(0, 0), _two(CHECKPOINT0, CHECKPOINT1)),
            SCHEDULE_0
        );
        (uint64 current, uint64 pendingAt) = vault.feeState(FEE_TOKEN);
        assertEq(current, 0);
        assertEq(pendingAt, 0);
        (uint256 mintFee, uint256 relayFee, uint256 burnFee, uint256 checkpointFee) = vault.fees(FEE_TOKEN, 0, 1);
        assertEq(mintFee, 0); // protocol fees start at zero
        assertEq(relayFee, RELAY1);
        assertEq(burnFee, 0);
        assertEq(checkpointFee, CHECKPOINT1);
    }

    /// @notice Registration emits what the LEZ side needs to rebuild both preimages, in force from the leaf count.
    function test_registration_emitsTheEntryAndScheduleZero() public {
        _deposit(alice, ETH, 0, keccak256("an earlier leaf")); // schedule 0 of a later token starts at leaf 1
        MockERC20 other = new MockERC20("Other", "OTH", 18);
        FerrataVault.TokenParams memory p = _feeTokenParams();
        uint64 at = uint64(block.timestamp);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.RegistryEntryPublished(address(other), LEZ_DEFINITION, 18, _entryHash(address(other), p));
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.FeesAnnounced(address(other), 0, at, at, _two(0, 0), _two(0, 0), p.checkpointFees);
        bytes32 h = _scheduleHash(address(other), 0, [uint64(1), at, at], _two(0, 0), _two(0, 0), p.checkpointFees);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.FeesInForce(address(other), 0, 1, at, h);
        vm.prank(admin);
        vault.registerToken(address(other), p);
        assertEq(vm.load(address(vault), _feeSlot(address(other), 0)), h);
    }

    function test_registration_feeRules() public {
        MockERC20 other = new MockERC20("Other", "OTH", 18);
        FerrataVault.TokenParams memory p = _feeTokenParams();
        vm.startPrank(admin);

        p.checkpointFees = new uint256[](1);
        vm.expectRevert(FerrataVault.FeeCountMismatch.selector);
        vault.registerToken(address(other), p);

        p.checkpointFees = _two(D0 / 100 + 1, 0); // 1 wei over 1%
        vm.expectRevert(FerrataVault.FeeTooHigh.selector);
        vault.registerToken(address(other), p);

        p.checkpointFees = _two(0, 0);
        p.relayFees = _two(RELAY0 + 1, RELAY1); // not a whole LEZ unit: the claim could not mint it
        vm.expectRevert(FerrataVault.FeeNotMultipleOfLezUnit.selector);
        vault.registerToken(address(other), p);

        p.relayFees = _two(RELAY0, RELAY1);
        p.decimals = 11; // lezUnit 1e12 > 10**11
        vm.expectRevert(FerrataVault.InvalidLezUnit.selector);
        vault.registerToken(address(other), p);

        p.decimals = 12; // lezUnit = 10**decimals: allowed
        p.checkpointFees = _two(D0 / 100, D1 / 100); // exactly 1%: allowed
        vault.registerToken(address(other), p);
        vm.stopPrank();
        (,,, uint256 checkpointFee) = vault.fees(address(other), 0, 1);
        assertEq(checkpointFee, D1 / 100);
    }

    /// @notice The LEZ side computes twice the largest denomination in value units in 128 bits; the vault refuses a
    ///         registration that LEZ would refuse.
    function test_registration_twiceTheLargestDenominationInValueUnitsFits128Bits() public {
        uint256[] memory d = new uint256[](1);
        d[0] = 2 ** 64;
        FerrataVault.TokenParams memory p = _params(d, 1, 2 ** 63 - 1); // 2 * 2**64 * (2**63 - 1) < 2**128
        p.relayFees = new uint256[](1);
        p.checkpointFees = new uint256[](1);
        MockERC20 ok = new MockERC20("Fits", "FIT", 18);
        MockERC20 over = new MockERC20("Over", "OVR", 18);
        vm.startPrank(admin);
        vault.registerToken(address(ok), p);
        p.unitWeight = 2 ** 63; // 2**128
        vm.expectRevert(FerrataVault.InvalidUnitWeight.selector);
        vault.registerToken(address(over), p);
        vm.stopPrank();
    }

    // ---- announcements --------------------------------------------------------------------------------------------

    function test_announce_onlyAdminAndOnlyARegisteredToken() public {
        vm.prank(alice);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.announceFees(FEE_TOKEN, _two(0, 0), _two(0, 0), _two(0, 0));
        vm.prank(admin);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.announceFees(address(0xBEEF), _two(0, 0), _two(0, 0), _two(0, 0));
    }

    /// @notice One fee of each kind per denomination; each at most 1% of its denomination (the limit and one past
    ///         it); mint fees in whole LEZ units, burn and checkpoint fees in any base unit.
    function test_announce_feeRules() public {
        uint256[] memory one = new uint256[](1);
        vm.startPrank(admin);
        vm.expectRevert(FerrataVault.FeeCountMismatch.selector);
        vault.announceFees(FEE_TOKEN, one, _two(0, 0), _two(0, 0));
        vm.expectRevert(FerrataVault.FeeCountMismatch.selector);
        vault.announceFees(FEE_TOKEN, _two(0, 0), one, _two(0, 0));
        vm.expectRevert(FerrataVault.FeeCountMismatch.selector);
        vault.announceFees(FEE_TOKEN, _two(0, 0), _two(0, 0), one);

        vm.expectRevert(FerrataVault.FeeTooHigh.selector);
        vault.announceFees(FEE_TOKEN, _two(0, D1 / 100 + 1e12), _two(0, 0), _two(0, 0));
        vm.expectRevert(FerrataVault.FeeTooHigh.selector);
        vault.announceFees(FEE_TOKEN, _two(0, 0), _two(D0 / 100 + 1, 0), _two(0, 0));
        vm.expectRevert(FerrataVault.FeeTooHigh.selector);
        vault.announceFees(FEE_TOKEN, _two(0, 0), _two(0, 0), _two(0, D1 / 100 + 1));
        vm.expectRevert(FerrataVault.FeeNotMultipleOfLezUnit.selector);
        vault.announceFees(FEE_TOKEN, _two(1e12 + 1, 0), _two(0, 0), _two(0, 0));

        uint64 at = uint64(block.timestamp);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.FeesAnnounced(
            FEE_TOKEN, 1, at, at + 7 days, _two(D0 / 100, D1 / 100), _two(1, D1 / 100), _two(3, D1 / 100)
        );
        vault.announceFees(FEE_TOKEN, _two(D0 / 100, D1 / 100), _two(1, D1 / 100), _two(3, D1 / 100));
        vm.stopPrank();
        (uint64 current, uint64 pendingAt) = vault.feeState(FEE_TOKEN);
        assertEq(current, 0);
        assertEq(pendingAt, at + FEE_NOTICE);
    }

    function testFuzz_announce_eachFeeAtMostOnePercent(uint256 mintUnits, uint256 burnFee, uint256 checkpointFee)
        public
    {
        uint256 mintFee = bound(mintUnits, 0, 2e4) * 1e12; // up to 2% of D0, in whole LEZ units
        burnFee = bound(burnFee, 0, 2e16);
        checkpointFee = bound(checkpointFee, 0, 2e16);
        bool allowed = mintFee <= D0 / 100 && burnFee <= D0 / 100 && checkpointFee <= D0 / 100;
        vm.prank(admin);
        if (!allowed) vm.expectRevert(FerrataVault.FeeTooHigh.selector);
        vault.announceFees(FEE_TOKEN, _two(mintFee, 0), _two(burnFee, 0), _two(checkpointFee, 0));
    }

    // ---- when a change takes effect -------------------------------------------------------------------------------

    /// @notice Seven days of Ethereum time after the announcement, at the first deposit from then on, from that
    ///         deposit's leaf index; published then, and not before.
    function test_feeChange_takesEffectSevenDaysAfterTheAnnouncement() public {
        uint64 at = uint64(block.timestamp);
        _announce(_two(1e16, 5e16), _two(3e15, 1e16), _two(2e15, 2e16));

        vm.warp(at + 7 days - 1);
        assertEq(_depositExactly(alice, 0, D0 + RELAY0), D0 + RELAY0); // schedule 0: no mint fee
        assertEq(_published(1), bytes32(0));
        (uint64 current,) = vault.feeState(FEE_TOKEN);
        assertEq(current, 0);

        vm.warp(at + 7 days);
        bytes32 h = _scheduleHash(
            FEE_TOKEN, 1, [uint64(1), at, at + 7 days], _two(1e16, 5e16), _two(3e15, 1e16), _two(2e15, 2e16)
        );
        t.mint(alice, D0 + 1e16 + RELAY0);
        vm.prank(alice);
        t.approve(address(vault), D0 + 1e16 + RELAY0);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.FeesInForce(FEE_TOKEN, 1, 1, at + 7 days, h);
        vm.prank(alice);
        vault.deposit(FEE_TOKEN, 0, keccak256("leaf 1"), 1); // leaf 1, the first under schedule 1
        assertEq(t.balanceOf(address(vault)), 2 * D0 + 2 * RELAY0 + 1e16);
        assertEq(_published(1), h);
        uint64 pendingAt;
        (current, pendingAt) = vault.feeState(FEE_TOKEN);
        assertEq(current, 1);
        assertEq(pendingAt, 0);
        (uint256 mintFee, uint256 relayFee, uint256 burnFee, uint256 checkpointFee) = vault.fees(FEE_TOKEN, 1, 1);
        assertEq(mintFee, 5e16);
        assertEq(relayFee, RELAY1);
        assertEq(burnFee, 1e16);
        assertEq(checkpointFee, 2e16);
    }

    /// @notice A deposit sending the old total once the change is due is refused (the denomination and the
    ///         schedule in force are what it must send).
    function test_feeChange_aDepositAfterTheChangeMustSendTheNewTotal() public {
        _announce(_two(1e16, 0), _two(0, 0), _two(0, 0));
        vm.warp(block.timestamp + FEE_NOTICE);
        t.mint(alice, D0 + RELAY0);
        vm.prank(alice);
        t.approve(address(vault), D0 + RELAY0);
        vm.prank(alice);
        vm.expectRevert(); // the token refuses: the vault pulls D0 + 1e16 + RELAY0
        vault.deposit(FEE_TOKEN, 0, keccak256("old total"), 0);

        uint256 total = D0 + 1e16 + 1 ether / 500; // the same change for ETH would not apply: schedules are per token
        vm.deal(alice, total);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.WrongEthAmount.selector);
        vault.depositETH{value: total}(1, keccak256("eth"), 0);
    }

    /// @notice A new announcement replaces a pending one and restarts the seven days.
    function test_feeChange_aNewAnnouncementReplacesThePendingOne() public {
        uint64 first = uint64(block.timestamp);
        _announce(_two(1e16, 0), _two(0, 0), _two(0, 0));
        vm.warp(first + 3 days);
        uint64 second = uint64(block.timestamp);
        _announce(_two(2e15, 0), _two(0, 0), _two(0, 0));
        (, uint64 pendingAt) = vault.feeState(FEE_TOKEN);
        assertEq(pendingAt, second + FEE_NOTICE);

        vm.warp(first + 7 days); // the first one's date: nothing changes
        assertEq(_depositExactly(alice, 0, D0 + RELAY0), D0 + RELAY0);
        vm.warp(second + 7 days);
        assertEq(_depositExactly(alice, 0, D0 + 2e15 + RELAY0), D0 + 2e15 + RELAY0);
        bytes32 h =
            _scheduleHash(FEE_TOKEN, 1, [uint64(1), second, second + 7 days], _two(2e15, 0), _two(0, 0), _two(0, 0));
        assertEq(_published(1), h);
    }

    /// @notice Announcing the fees in force withdraws a pending announcement: nothing takes effect, ever.
    function test_feeChange_announcingTheFeesInForceWithdrawsIt() public {
        _announce(_two(1e16, 0), _two(0, 0), _two(0, 0));
        uint64 at = uint64(block.timestamp);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.FeesAnnounced(FEE_TOKEN, 1, at, 0, _two(0, 0), _two(0, 0), _two(CHECKPOINT0, CHECKPOINT1));
        _announce(_two(0, 0), _two(0, 0), _two(CHECKPOINT0, CHECKPOINT1));
        (, uint64 pendingAt) = vault.feeState(FEE_TOKEN);
        assertEq(pendingAt, 0);
        vm.warp(block.timestamp + 30 days);
        assertEq(_depositExactly(alice, 0, D0 + RELAY0), D0 + RELAY0);
        vault.applyFees(FEE_TOKEN);
        (uint64 current,) = vault.feeState(FEE_TOKEN);
        assertEq(current, 0);
        assertEq(_published(1), bytes32(0));
    }

    /// @notice A due change takes effect before the next one is announced; the next one gets the next index.
    function test_feeChange_aDueChangeTakesEffectBeforeTheNextAnnouncement() public {
        uint64 at = uint64(block.timestamp);
        _announce(_two(1e16, 0), _two(0, 0), _two(0, 0));
        _depositExactly(alice, 0, D0 + RELAY0); // leaf 0
        vm.warp(at + 7 days + 5);
        _announce(_two(2e16 / 2, 0), _two(5, 0), _two(0, 0));
        (uint64 current, uint64 pendingAt) = vault.feeState(FEE_TOKEN);
        assertEq(current, 1);
        assertEq(pendingAt, at + 7 days + 5 + FEE_NOTICE);
        assertEq(
            _published(1),
            _scheduleHash(FEE_TOKEN, 1, [uint64(1), at, at + 7 days + 5], _two(1e16, 0), _two(0, 0), _two(0, 0))
        );
        assertEq(_published(2), bytes32(0));
    }

    /// @notice Anyone puts a due change in force; before it is due, and once it is in force, the call does nothing.
    function test_applyFees_isPermissionlessAndActsOnlyWhenDue() public {
        uint64 at = uint64(block.timestamp);
        _announce(_two(0, 0), _two(7, 0), _two(0, 0));
        vm.warp(at + 7 days - 1);
        vm.prank(bob);
        vault.applyFees(FEE_TOKEN);
        (uint64 current,) = vault.feeState(FEE_TOKEN);
        assertEq(current, 0);

        vm.warp(at + 7 days);
        vm.recordLogs();
        vm.prank(bob);
        vault.applyFees(FEE_TOKEN);
        assertEq(vm.getRecordedLogs().length, 1); // FeesInForce
        (current,) = vault.feeState(FEE_TOKEN);
        assertEq(current, 1);
        assertEq(
            _published(1), _scheduleHash(FEE_TOKEN, 1, [uint64(0), at, at + 7 days], _two(0, 0), _two(7, 0), _two(0, 0))
        );
        vm.recordLogs();
        vault.applyFees(FEE_TOKEN);
        assertEq(vm.getRecordedLogs().length, 0);
        (current,) = vault.feeState(FEE_TOKEN);
        assertEq(current, 1);
    }

    // ---- a deposit keeps the schedule of its leaf index -----------------------------------------------------------

    /// @notice Deposits under schedules 0, 1 and 2, among other tokens' leaves, are refunded exactly what each
    ///         escrowed: the denomination, the relay fee and the mint fee of the schedule of its leaf index.
    function test_feeChange_aRefundReturnsTheEscrowOfItsLeafsSchedule() public {
        uint64 at = uint64(block.timestamp);
        _depositExactly(alice, 1, D1 + RELAY1); // leaf 0: schedule 0
        _announce(_two(1e16, 1e17), _two(0, 0), _two(0, 0));
        vm.warp(at + 7 days);
        _depositExactly(bob, 1, D1 + 1e17 + RELAY1); // leaf 1: schedule 1 from here
        _deposit(alice, ETH, 0, keccak256("another token's leaf")); // leaf 2
        leaves.push(_leafHash(2, alice, ETH, 0.1 ether, keccak256("another token's leaf")));
        _leafData.push(LeafData(alice, 0, bytes32(0)));
        _depositExactly(bob, 0, D0 + 1e16 + RELAY0); // leaf 3: schedule 1
        _announce(_two(5e15, 3e16), _two(0, 0), _two(0, 0));
        vm.warp(at + 14 days);
        vault.applyFees(FEE_TOKEN); // schedule 2 from leaf 4
        _depositExactly(alice, 1, D1 + 3e16 + RELAY1); // leaf 4: schedule 2

        vm.prank(freezer);
        vault.retire();
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT, 0, true); // LEZ imported nothing
        _submit(c);
        _finalizeAfterDelta(c);
        vm.warp(block.timestamp + REVOKE_WINDOW + 1);

        uint256[5] memory expected = [D1 + RELAY1, D1 + 1e17 + RELAY1, 0, D0 + 1e16 + RELAY0, D1 + 3e16 + RELAY1];
        for (uint64 i = 0; i < 5; i++) {
            if (i == 2) continue;
            address who = _leafData[i].depositor;
            uint256 before = t.balanceOf(who);
            _refundLeaf(i);
            assertEq(t.balanceOf(who) - before, expected[i]);
        }
        assertEq(vault.escrowed(FEE_TOKEN), 0);
    }

    // ---- releases -------------------------------------------------------------------------------------------------

    /// @dev Schedule 1 with burn and checkpoint fees on denomination 0, in force from the next deposit.
    function _scheduleOneWithReleaseFees() internal {
        _announce(_two(0, 0), _two(3e15, 0), _two(4e15, 0));
        vm.warp(block.timestamp + FEE_NOTICE);
    }

    /// @notice Rule (A): the recipient gets the denomination less the relay, burn and checkpoint fees; the relayer is
    ///         paid the relay fee; the burn fee is credited to the fee recipient and the checkpoint fee to the account
    ///         that finalised the root (here this contract), each claims it.
    function test_release_paysTheDenominationLessItsFees() public {
        _scheduleOneWithReleaseFees();
        _depositExactly(alice, 0, D0 + RELAY0);
        _acceptRoot(ROOT); // finalised by this contract

        FerrataVault.ReleaseRequest memory r = _request(FEE_TOKEN, 0, recipient, ROOT, keccak256("n1"));
        r.relayFee = RELAY0;
        _proveRelease(_releaseJournal(r));
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.OwedCredited(feeRecipient, FEE_TOKEN, 3e15);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.OwedCredited(address(this), FEE_TOKEN, 4e15);
        vm.prank(relayer);
        vault.release(r, SEAL);

        assertEq(t.balanceOf(recipient), D0 - RELAY0 - 3e15 - 4e15);
        assertEq(t.balanceOf(relayer), RELAY0);
        assertEq(vault.owed(feeRecipient, FEE_TOKEN), 3e15);
        assertEq(vault.owed(address(this), FEE_TOKEN), 4e15);
        assertEq(vault.totalOwed(FEE_TOKEN), 7e15);
        assertEq(vault.escrowed(FEE_TOKEN), RELAY0); // the deposit's relay fee, for its claim
        assertEq(t.balanceOf(address(vault)), RELAY0 + 7e15);

        vm.prank(feeRecipient);
        vault.claimOwed(FEE_TOKEN);
        vault.claimOwed(FEE_TOKEN);
        assertEq(t.balanceOf(feeRecipient), 3e15);
        assertEq(t.balanceOf(address(this)), 4e15);
        assertEq(vault.totalOwed(FEE_TOKEN), 0);
    }

    /// @notice Without a relay fee (the user submits), only the burn and checkpoint fees are taken.
    function test_release_withoutRelayFee() public {
        _scheduleOneWithReleaseFees();
        _depositExactly(alice, 0, D0 + RELAY0);
        _acceptRoot(ROOT);
        _release(0, recipient, 0, keccak256("n1"));
        assertEq(t.balanceOf(recipient), D0 - 3e15 - 4e15);
        assertEq(t.balanceOf(relayer), 0);
    }

    /// @notice A release puts a due change in force first: its fees apply, and the schedule starts at the leaf count.
    function test_release_putsADueChangeInForce() public {
        _depositExactly(alice, 0, D0 + RELAY0);
        _acceptRoot(ROOT);
        uint64 at = uint64(block.timestamp);
        _announce(_two(0, 0), _two(3e15, 0), _two(4e15, 0));
        vm.warp(at + 7 days);
        _release(0, recipient, 0, keccak256("n1"));
        assertEq(t.balanceOf(recipient), D0 - 3e15 - 4e15);
        assertEq(
            _published(1),
            _scheduleHash(FEE_TOKEN, 1, [uint64(1), at, at + 7 days], _two(0, 0), _two(3e15, 0), _two(4e15, 0))
        );
    }

    /// @notice With no burn fee and no checkpoint fee in force, a release credits nothing.
    function test_release_zeroFeesCreditNothing() public {
        _announce(_two(0, 0), _two(0, 0), _two(0, 0));
        vm.warp(block.timestamp + FEE_NOTICE);
        _depositExactly(alice, 0, D0 + RELAY0);
        _acceptRoot(ROOT);
        vm.recordLogs();
        _release(0, recipient, 0, keccak256("n1"));
        Vm.Log[] memory logs = vm.getRecordedLogs();
        for (uint256 i = 0; i < logs.length; i++) {
            assertTrue(logs[i].topics[0] != FerrataVault.OwedCredited.selector);
        }
        assertEq(vault.totalOwed(FEE_TOKEN), 0);
        assertEq(t.balanceOf(recipient), D0);
    }

    /// @notice The checkpoint fee goes to the checkpoint's prover (the sender of its candidate, review M-2), never
    ///         pushed: a prover that refuses every payment (here a contract) is credited and cannot block or tax a release.
    function test_release_checkpointFeeIsCreditedToTheProverNeverPushed() public {
        ToggleRecipient prover = new ToggleRecipient(); // refuses ETH while closed
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _proveCheckpoint(_checkpointJournal(c));
        vm.prank(address(prover));
        vault.submitCandidate(c, SEAL);
        (uint256 submittedAt, uint256 wait) = _waitOf(vault, c.newState);
        vm.warp(submittedAt + wait);
        vm.roll(block.number + 1);
        uint256 beaconIndex = vault.recordBeacon();
        (, bytes32 beaconNonce) = vault.beacons(beaconIndex);
        c.echoNonce = beaconNonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.prank(relayer); // the finalisation's sender is not paid
        vault.finalize(beaconIndex, c, SEAL);

        _deposit(alice, ETH, 1, keccak256("eth"));
        uint256 checkpointFee = 1 ether / 200; // 0.5%
        vm.prank(admin);
        vault.announceFees(ETH, _three(0), _three(0), _threeAt1(checkpointFee));
        vm.warp(block.timestamp + FEE_NOTICE);
        _release(_request(ETH, 1, recipient, ROOT, keccak256("n1")));
        assertEq(recipient.balance, 1 ether - checkpointFee);
        assertEq(vault.owed(address(prover), ETH), checkpointFee);
        assertEq(vault.owed(relayer, ETH), 0);
        assertEq(address(prover).balance, 0);
        prover.setOpen(true);
        prover.claim(address(vault), ETH);
        assertEq(address(prover).balance, checkpointFee);
    }

    function _three(uint256 v) internal pure returns (uint256[] memory x) {
        x = new uint256[](3);
        (x[0], x[1], x[2]) = (v, v, v);
    }

    function _threeAt1(uint256 v) internal pure returns (uint256[] memory x) {
        x = new uint256[](3);
        x[1] = v;
    }

    /// @notice Rule (B): the checkpoint fee goes to the candidate's prover, not to the sender of finalizeAttested, which
    ///         needs no proof (review M-2).
    function test_release_ruleB_checkpointFeeToTheProverNotToTheSenderOfFinalizeAttested() public {
        address prover = makeAddr("prover");
        FerrataVault vb = new FerrataVault(_deploymentB(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID));
        vm.prank(admin);
        vb.registerToken(FEE_TOKEN, _feeTokenParams());
        _depositInto(vb, alice, FEE_TOKEN, 1, keccak256("b"));
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vb, ROOT, 0, false);
        _proveCheckpoint(_checkpointJournal(vb, LEZ_CONFIG_ID, c));
        vm.prank(prover);
        vb.submitCandidate(c, SEAL);
        (uint256 submittedAt, uint256 wait) = _waitOf(vb, c.newState);
        vm.warp(submittedAt + wait);
        vm.prank(bob);
        vb.finalizeAttested(c);

        FerrataVault.ReleaseRequest memory r = _request(FEE_TOKEN, 1, recipient, ROOT, keccak256("nb"));
        ReleaseJournal memory j = _releaseJournal(r);
        j.vault = address(vb);
        _proveRelease(j);
        vm.prank(relayer);
        vb.release(r, SEAL);
        assertEq(t.balanceOf(recipient), D1 - CHECKPOINT1); // schedule 0: no burn fee, its checkpoint fee
        assertEq(vb.owed(prover, FEE_TOKEN), CHECKPOINT1);
        assertEq(vb.owed(bob, FEE_TOKEN), 0);
        assertEq(vb.owed(feeRecipient, FEE_TOKEN), 0);
    }

    // ---- caps count the whole escrow movement ---------------------------------------------------------------------

    /// @notice The deposit buckets count what the deposit escrows (denomination and fees), which is what LEZ mints.
    function test_caps_depositBucketsCountTheWholeEscrow() public {
        uint256 total = D0 + RELAY0;
        uint256 units = total / 1e12 * 2000; // value units: per lezUnit, the unit weight
        t.mint(alice, total);
        vm.prank(alice);
        t.approve(address(vault), total);
        vm.prank(admin);
        vault.setTokenCap(FEE_TOKEN, FerrataVault.Direction.Deposit, total - 1, 0);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.TokenDepositCapExceeded.selector);
        vault.deposit(FEE_TOKEN, 0, keccak256("c"), 0);
        vm.startPrank(admin);
        vault.setTokenCap(FEE_TOKEN, FerrataVault.Direction.Deposit, UNLIMITED, 0);
        vault.setTokenCap(FEE_TOKEN, FerrataVault.Direction.Deposit, total, 0); // from UNLIMITED: starts full
        vault.setGlobalCap(FerrataVault.Direction.Deposit, units - 1, 0);
        vm.stopPrank();
        vm.prank(alice);
        vm.expectRevert(FerrataVault.GlobalDepositCapExceeded.selector);
        vault.deposit(FEE_TOKEN, 0, keccak256("c"), 0);
        vm.startPrank(admin);
        vault.setGlobalCap(FerrataVault.Direction.Deposit, UNLIMITED, 0);
        vault.setGlobalCap(FerrataVault.Direction.Deposit, units, 0);
        vm.stopPrank();
        vm.prank(alice);
        vault.deposit(FEE_TOKEN, 0, keccak256("c"), 0);
        (,, uint256 available) = vault.tokenCap(FEE_TOKEN, FerrataVault.Direction.Deposit);
        (,, uint256 globalLeft) = vault.globalCap(FerrataVault.Direction.Deposit);
        assertEq(available, 0);
        assertEq(globalLeft, 0);
    }

    // ---- the arithmetic over the whole fee range ------------------------------------------------------------------

    /// @notice Any schedule within the ceiling: the deposit escrows denomination + mint + relay, and the release
    ///         pays the denomination less burn, checkpoint and (if named) relay, the parts adding up to it.
    function testFuzz_feeArithmetic(uint256 mintUnits, uint256 burnFee, uint256 checkpointFee, bool withRelay) public {
        uint256 mintFee = bound(mintUnits, 0, D1 / 100 / 1e12) * 1e12;
        burnFee = bound(burnFee, 0, D1 / 100);
        checkpointFee = bound(checkpointFee, 0, D1 / 100);
        _announce(_two(0, mintFee), _two(0, burnFee), _two(0, checkpointFee));
        vm.warp(block.timestamp + FEE_NOTICE);
        assertEq(_depositExactly(alice, 1, D1 + mintFee + RELAY1), D1 + mintFee + RELAY1);
        _acceptRoot(ROOT);
        _release(1, recipient, withRelay ? RELAY1 : 0, keccak256("n"));
        uint256 relayPaid = withRelay ? RELAY1 : 0;
        assertEq(t.balanceOf(recipient), D1 - burnFee - checkpointFee - relayPaid);
        assertEq(t.balanceOf(relayer), relayPaid);
        assertEq(vault.owed(feeRecipient, FEE_TOKEN), burnFee);
        assertEq(vault.owed(address(this), FEE_TOKEN), checkpointFee);
        assertEq(vault.escrowed(FEE_TOKEN), mintFee + RELAY1); // backs what the claim minted besides the denomination
    }
}
