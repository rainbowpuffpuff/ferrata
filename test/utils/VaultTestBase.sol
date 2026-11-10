// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";
import {MockERC20, MockTokenBase, MockUSDT} from "../mocks/MockTokens.sol";
import {MockRiscZeroVerifier} from "../mocks/MockVerifiers.sol";

/// @notice Shared fixture: a vault with ETH, a 6-decimal token, an 18-decimal token and a USDT-style token
///         registered, all caps UNLIMITED. Journals are rebuilt here word by word from the written spec
///         (README), not with the vault's own encoding functions.
abstract contract VaultTestBase is Test {
    bytes32 internal constant RELEASE_IMAGE_ID = keccak256("test: release program image id");
    bytes32 internal constant ROOT_IMAGE_ID = keccak256("test: burn-root program image id");
    bytes32 internal constant LEZ_CONFIG_ID = keccak256("test: LEZ network / zone / bridge program / config 1");
    uint32 internal constant MIN_DEPTH = 64;
    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;
    bytes internal constant SEAL = hex"5ea15ea1";

    /// Defaults used by `_acceptRoot`.
    uint64 internal constant LEZ_BLOCK = 1000;
    uint64 internal constant BURN_COUNT = 10;

    struct RootJournal {
        uint256 chainId;
        address vault;
        bytes32 lezConfigId;
        uint64 lezBlockId;
        bytes32 burnRoot;
        uint64 burnCount;
        uint32 depth;
    }

    struct ReleaseJournal {
        uint256 chainId;
        address vault;
        bytes32 lezConfigId;
        bytes32 burnRoot;
        bytes32 nullifier;
        address token;
        uint256 amount;
        address recipient;
        address feeRecipient;
        uint256 relayFee;
    }

    FerrataVault internal vault;
    MockRiscZeroVerifier internal verifier;
    MockERC20 internal usdc; // 6 decimals
    MockERC20 internal dai; // 18 decimals
    MockUSDT internal usdt; // 6 decimals, transfer/transferFrom return nothing

    address internal admin = makeAddr("admin");
    address internal freezer = makeAddr("freezeAuthority");
    address internal alice = makeAddr("alice");
    address internal bob = makeAddr("bob");
    address internal relayer = makeAddr("relayer");
    address internal recipient = makeAddr("recipient");

    function setUp() public virtual {
        verifier = new MockRiscZeroVerifier();
        usdc = new MockERC20("USD Coin", "USDC", 6);
        dai = new MockERC20("Dai Stablecoin", "DAI", 18);
        usdt = new MockUSDT("Tether USD", "USDT", 6);
        vault = _deployVault(IRiscZeroVerifier(address(verifier)), LEZ_CONFIG_ID);
        _registerDefaultTokens(vault);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Deployment and registration
    // ------------------------------------------------------------------------------------------------------------

    function _unlimited() internal pure returns (FerrataVault.CapParams memory) {
        return FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0});
    }

    function _cap(uint256 capacity, uint256 refillPerSecond) internal pure returns (FerrataVault.CapParams memory) {
        return FerrataVault.CapParams({capacity: capacity, refillPerSecond: refillPerSecond});
    }

    function _deployVault(IRiscZeroVerifier v, bytes32 lezConfig) internal returns (FerrataVault) {
        return new FerrataVault(
            v, RELEASE_IMAGE_ID, ROOT_IMAGE_ID, lezConfig, admin, freezer, MIN_DEPTH, _unlimited(), _unlimited()
        );
    }

    /// @dev Value unit = 1e-6 USD. ETH at 3000 USD: one lezUnit (1e12 wei) is worth 3000 value units.
    function _registerDefaultTokens(FerrataVault v) internal {
        vm.startPrank(admin);
        v.registerToken(ETH, _denoms(0.1 ether, 1 ether, 10 ether), 1e12, 3000, _unlimited(), _unlimited());
        v.registerToken(address(usdc), _denoms(100e6, 1000e6, 10_000e6), 1, 1, _unlimited(), _unlimited());
        v.registerToken(address(dai), _denoms(100e18, 1000e18, 10_000e18), 1e12, 1, _unlimited(), _unlimited());
        v.registerToken(address(usdt), _denoms(100e6, 1000e6, 10_000e6), 1, 1, _unlimited(), _unlimited());
        vm.stopPrank();
    }

    function _denoms(uint256 a, uint256 b, uint256 c) internal pure returns (uint256[] memory d) {
        d = new uint256[](3);
        d[0] = a;
        d[1] = b;
        d[2] = c;
    }

    function _amount(address token, uint8 denomIndex) internal view returns (uint256) {
        return vault.denominations(token)[denomIndex];
    }

    function _balance(address token, address who) internal view returns (uint256) {
        return token == ETH ? who.balance : MockTokenBase(token).balanceOf(who);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Deposits
    // ------------------------------------------------------------------------------------------------------------

    function _deposit(address from, address token, uint8 denomIndex, bytes32 commitment)
        internal
        returns (uint64 leafIndex)
    {
        return _depositInto(vault, from, token, denomIndex, commitment);
    }

    /// @dev Funds `from`, approves if needed, and deposits with the correct per-depositor index.
    function _depositInto(FerrataVault v, address from, address token, uint8 denomIndex, bytes32 commitment)
        internal
        returns (uint64 leafIndex)
    {
        uint256 amount = v.denominations(token)[denomIndex];
        uint64 index = v.depositCount(from);
        leafIndex = v.nextLeafIndex();
        if (token == ETH) {
            vm.deal(from, from.balance + amount);
            vm.prank(from);
            v.depositETH{value: amount}(denomIndex, commitment, index);
        } else {
            MockTokenBase(token).mint(from, amount);
            vm.prank(from);
            MockTokenBase(token).approve(address(v), amount);
            vm.prank(from);
            v.deposit(token, denomIndex, commitment, index);
        }
    }

    // ------------------------------------------------------------------------------------------------------------
    // Journals: the exact byte layouts a proof must commit to
    // ------------------------------------------------------------------------------------------------------------

    /// @dev 8 words: domain, chainId, vault, lezConfigId, lezBlockId, burnRoot, burnCount, depth.
    function _rootJournalBytes(RootJournal memory j) internal pure returns (bytes memory) {
        return bytes.concat(
            sha256(bytes("ferrata/vault/burn-root/v1")),
            bytes32(j.chainId),
            bytes32(uint256(uint160(j.vault))),
            j.lezConfigId,
            bytes32(uint256(j.lezBlockId)),
            j.burnRoot,
            bytes32(uint256(j.burnCount)),
            bytes32(uint256(j.depth))
        );
    }

    /// @dev 11 words: domain, chainId, vault, lezConfigId, burnRoot, nullifier, token, amount, recipient,
    ///      feeRecipient, relayFee.
    function _releaseJournalBytes(ReleaseJournal memory j) internal pure returns (bytes memory) {
        bytes memory context = bytes.concat(
            sha256(bytes("ferrata/vault/release/v1")),
            bytes32(j.chainId),
            bytes32(uint256(uint160(j.vault))),
            j.lezConfigId,
            j.burnRoot,
            j.nullifier
        );
        bytes memory payout = bytes.concat(
            bytes32(uint256(uint160(j.token))),
            bytes32(j.amount),
            bytes32(uint256(uint160(j.recipient))),
            bytes32(uint256(uint160(j.feeRecipient))),
            bytes32(j.relayFee)
        );
        return bytes.concat(context, payout);
    }

    function _rootJournal(bytes32 burnRoot, uint64 lezBlockId, uint64 burnCount, uint32 depth)
        internal
        view
        returns (RootJournal memory)
    {
        return RootJournal({
            chainId: block.chainid,
            vault: address(vault),
            lezConfigId: LEZ_CONFIG_ID,
            lezBlockId: lezBlockId,
            burnRoot: burnRoot,
            burnCount: burnCount,
            depth: depth
        });
    }

    /// @dev The journal the vault should reconstruct for `r`.
    function _releaseJournal(FerrataVault.ReleaseRequest memory r) internal view returns (ReleaseJournal memory) {
        return ReleaseJournal({
            chainId: block.chainid,
            vault: address(vault),
            lezConfigId: LEZ_CONFIG_ID,
            burnRoot: r.burnRoot,
            nullifier: r.nullifier,
            token: r.token,
            amount: _amount(r.token, r.denomIndex),
            recipient: r.recipient,
            feeRecipient: r.feeRecipient,
            relayFee: r.relayFee
        });
    }

    /// @dev Stands in for proving: makes the mock verifier accept this journal for the root program.
    function _proveRoot(RootJournal memory j) internal {
        verifier.accept(ROOT_IMAGE_ID, sha256(_rootJournalBytes(j)));
    }

    /// @dev Stands in for proving: makes the mock verifier accept this journal for the release program.
    function _proveRelease(ReleaseJournal memory j) internal {
        verifier.accept(RELEASE_IMAGE_ID, sha256(_releaseJournalBytes(j)));
    }

    /// @dev Proves and accepts `burnRoot` at the default LEZ block and burn count, at exactly the minimum depth.
    function _acceptRoot(bytes32 burnRoot) internal {
        _proveRoot(_rootJournal(burnRoot, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH));
        vault.acceptBurnRoot(burnRoot, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Releases
    // ------------------------------------------------------------------------------------------------------------

    /// @dev A request with no relay fee and feeRecipient = address(0).
    function _request(address token, uint8 denomIndex, address to, bytes32 burnRoot, bytes32 nullifier)
        internal
        pure
        returns (FerrataVault.ReleaseRequest memory)
    {
        return FerrataVault.ReleaseRequest({
            token: token,
            denomIndex: denomIndex,
            recipient: to,
            feeRecipient: address(0),
            relayFee: 0,
            burnRoot: burnRoot,
            nullifier: nullifier
        });
    }

    /// @dev Proves `r` and submits it from the relayer.
    function _release(FerrataVault.ReleaseRequest memory r) internal {
        _proveRelease(_releaseJournal(r));
        vm.prank(relayer);
        vault.release(r, SEAL);
    }
}
