// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {console2} from "forge-std/console2.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {ToggleRecipient} from "./mocks/MockRecipients.sol";
import {MockBlacklistToken, MockTokenBase} from "./mocks/MockTokens.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Gas per scenario for GAS.md. Run with
///             forge test --isolate --match-contract GasScenarios -vv
///         so every top-level call is its own transaction (cold storage, 21000 base cost and calldata included).
///         Without --isolate the logged numbers are execution-only and not comparable. The first row, a plain ETH
///         transfer, must read 21000 in isolated mode. Numbers for calls that verify a proof include the mock
///         verifier's own execution, which `test_gas_mockVerifierExecution` estimates separately.
contract GasScenarios is VaultTestBase {
    address internal warmup = makeAddr("warm-up depositor");
    address internal fresh = makeAddr("fresh depositor");
    bytes32 internal constant C = keccak256("commitment");
    bytes32 internal constant ROOT = keccak256("burn root");
    MockBlacklistToken internal busd;
    bytes32[] internal leaves;

    function setUp() public override {
        super.setUp();
        // A deployed-looking configuration: every bucket finite, relay fees registered, a non-empty tree, escrow on
        // every token, one accepted root.
        address[4] memory tokens = [ETH, address(usdc), address(dai), address(usdt)];
        vm.startPrank(admin);
        for (uint256 i = 0; i < tokens.length; i++) {
            uint256 big = _amount(tokens[i], 2) * 1000;
            vault.setTokenCap(tokens[i], FerrataVault.Direction.Deposit, big, big / 86_400);
            vault.setTokenCap(tokens[i], FerrataVault.Direction.Release, big, big / 86_400);
        }
        vault.setGlobalCap(FerrataVault.Direction.Deposit, 1e15, 1e10);
        vault.setGlobalCap(FerrataVault.Direction.Release, 1e15, 1e10);
        vm.stopPrank();

        for (uint8 i = 0; i < 37; i++) {
            _recordedDeposit(tokens[i % 4], i % 3, keccak256(abi.encode(i)));
        }
        for (uint256 i = 0; i < 4; i++) {
            _recordedDeposit(tokens[i], 2, keccak256(abi.encode("escrow", i)));
        }
        busd = new MockBlacklistToken();
        vm.prank(admin);
        vault.registerToken(address(busd), _params(_denoms(100e6, 1000e6, 10_000e6), 1, 1));
        _recordedDeposit(address(busd), 1, keccak256("busd escrow"));
        _acceptRoot(ROOT);
    }

    function _recordedDeposit(address token, uint8 denomIndex, bytes32 commitment) internal {
        uint64 leafIndex = _deposit(warmup, token, denomIndex, commitment);
        leaves.push(ReferenceMerkle.leafHash(leafIndex, warmup, token, _amount(token, denomIndex), commitment));
    }

    function _path(uint64 leafIndex) internal view returns (bytes32[32] memory path) {
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, leafIndex);
        for (uint256 h = 0; h < 32; h++) {
            path[h] = siblings[h];
        }
    }

    /// @dev Finalises a sunset checkpoint below which 37 leaves were imported: leaves 37..41 become refundable.
    function _sunsetAt37() internal {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, keccak256("sunset root"), 37, true);
        _submit(c);
        _finalizeAfterDelta(c);
    }

    function _log(string memory scenario) internal view returns (uint256 gasUsed) {
        gasUsed = vm.lastCallGas().gasTotalUsed;
        console2.log(scenario, gasUsed);
    }

    function _calldataGas(bytes memory data) internal pure returns (uint256 g) {
        for (uint256 i = 0; i < data.length; i++) {
            g += data[i] == 0 ? 4 : 16;
        }
    }

    function _fundAndApprove(address token, address who, uint256 amount) internal {
        MockTokenBase(token).mint(who, amount);
        vm.prank(who);
        MockTokenBase(token).approve(address(vault), type(uint256).max);
    }

    // ---- baseline -------------------------------------------------------------------------------------------------

    function test_gas_baselinePlainTransfer() public {
        vm.deal(fresh, 1 ether);
        vm.prank(fresh);
        (bool ok,) = recipient.call{value: 1}("");
        assertTrue(ok);
        _log("baseline: plain ETH transfer (must be 21000 under --isolate)");
    }

    // ---- deposits -------------------------------------------------------------------------------------------------

    function test_gas_depositETH_firstByAddress() public {
        vm.deal(fresh, 1 ether);
        vm.prank(fresh);
        vault.depositETH{value: 1 ether}(1, C, 0);
        _log("depositETH, first deposit by the address");
    }

    function test_gas_depositETH_laterByAddress() public {
        vm.deal(fresh, 2 ether);
        vm.prank(fresh);
        vault.depositETH{value: 1 ether}(1, C, 0);
        vm.prank(fresh);
        vault.depositETH{value: 1 ether}(1, C, 1);
        _log("depositETH, later deposit by the address");
    }

    function test_gas_depositETH_laterByAddress_capsUnlimited() public {
        vm.startPrank(admin);
        vault.setTokenCap(ETH, FerrataVault.Direction.Deposit, UNLIMITED, 0);
        vault.setGlobalCap(FerrataVault.Direction.Deposit, UNLIMITED, 0);
        vm.stopPrank();
        vm.deal(fresh, 2 ether);
        vm.prank(fresh);
        vault.depositETH{value: 1 ether}(1, C, 0);
        vm.prank(fresh);
        vault.depositETH{value: 1 ether}(1, C, 1);
        _log("depositETH, later deposit by the address, caps UNLIMITED");
    }

    function test_gas_depositERC20_firstByAddress() public {
        _fundAndApprove(address(dai), fresh, 1000e18);
        vm.prank(fresh);
        vault.deposit(address(dai), 1, C, 0);
        _log("deposit ERC-20 (18 dec, returns bool), first deposit by the address");
    }

    function test_gas_depositERC20_laterByAddress() public {
        _fundAndApprove(address(dai), fresh, 2000e18);
        vm.prank(fresh);
        vault.deposit(address(dai), 1, C, 0);
        vm.prank(fresh);
        vault.deposit(address(dai), 1, C, 1);
        _log("deposit ERC-20 (18 dec, returns bool), later deposit by the address");
    }

    function test_gas_depositUsdtStyle_firstByAddress() public {
        _fundAndApprove(address(usdt), fresh, 1000e6);
        vm.prank(fresh);
        vault.deposit(address(usdt), 1, C, 0);
        _log("deposit USDT-style (no return value), first deposit by the address");
    }

    /// @notice Deposit cost depends on the leaf index (one cold read per set bit of the index), so this sweeps
    ///         128 consecutive later deposits.
    function test_gas_depositETH_rangeOver128Leaves() public {
        vm.deal(fresh, 200 ether);
        vm.prank(fresh);
        vault.depositETH{value: 1 ether}(1, C, 0);
        uint256 lo = type(uint256).max;
        uint256 hi;
        uint256 sum;
        for (uint64 i = 1; i <= 128; i++) {
            vm.prank(fresh);
            vault.depositETH{value: 1 ether}(1, C, i);
            uint256 g = vm.lastCallGas().gasTotalUsed;
            lo = g < lo ? g : lo;
            hi = g > hi ? g : hi;
            sum += g;
        }
        console2.log("depositETH later, 128 consecutive leaves: min", lo);
        console2.log("depositETH later, 128 consecutive leaves: max", hi);
        console2.log("depositETH later, 128 consecutive leaves: mean", sum / 128);
    }

    // ---- release side ---------------------------------------------------------------------------------------------

    function test_gas_submitCandidate() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(keccak256("a new burn root"));
        _proveCheckpoint(_checkpointJournal(c));
        vm.roll(block.number + 1); // so the call records its own beacon
        vm.prank(relayer);
        vault.submitCandidate(c, SEAL);
        _log("submitCandidate, incl. its beacon (incl. mock verifier)");
    }

    function test_gas_recordBeacon() public {
        vm.roll(block.number + 1);
        vm.prank(relayer);
        vault.recordBeacon();
        _log("recordBeacon");
    }

    function test_gas_finalize() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(keccak256("a new burn root"));
        _submit(c);
        vm.warp(block.timestamp + DELTA);
        vm.roll(block.number + 1);
        uint256 beaconIndex = vault.recordBeacon();
        (, bytes32 nonce) = vault.beacons(beaconIndex);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.prank(relayer);
        vault.finalize(beaconIndex, c, SEAL);
        _log("finalize (incl. mock verifier)");
    }

    function test_gas_releaseETH_noFee() public {
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, keccak256("n1"));
        _proveRelease(_releaseJournal(r));
        vm.prank(relayer);
        vault.release(r, SEAL);
        _log("release ETH, no relay fee (incl. mock verifier)");
    }

    function test_gas_releaseETH_withFee() public {
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, keccak256("n2"));
        r.relayFee = _relayFee(ETH, 1);
        _proveRelease(_releaseJournal(r));
        vm.prank(relayer);
        vault.release(r, SEAL);
        _log("release ETH, standard fee to msg.sender (incl. mock verifier)");
    }

    function test_gas_releaseERC20_withFee() public {
        FerrataVault.ReleaseRequest memory r = _request(address(dai), 1, recipient, ROOT, keccak256("n3"));
        r.relayFee = _relayFee(address(dai), 1);
        _proveRelease(_releaseJournal(r));
        vm.prank(relayer);
        vault.release(r, SEAL);
        _log("release ERC-20 (18 dec), standard fee to msg.sender (incl. mock verifier)");
    }

    function test_gas_releaseETH_recipientCannotReceive_credited() public {
        ToggleRecipient payee = new ToggleRecipient(); // closed
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, address(payee), ROOT, keccak256("n5"));
        _proveRelease(_releaseJournal(r));
        vm.prank(relayer);
        vault.release(r, SEAL);
        _log("release ETH, recipient cannot receive: credited to owed (incl. mock verifier)");
    }

    function test_gas_refundETH() public {
        _sunsetAt37();
        bytes32[32] memory path = _path(37); // 10 ETH, deposit "escrow" 0
        vm.prank(relayer);
        vault.refund(37, warmup, ETH, 2, keccak256(abi.encode("escrow", uint256(0))), path);
        _log("refund ETH (32-step path check against depositRoot)");
    }

    function test_gas_refundERC20() public {
        _sunsetAt37();
        bytes32[32] memory path = _path(39); // 10,000 DAI, deposit "escrow" 2
        vm.prank(relayer);
        vault.refund(39, warmup, address(dai), 2, keccak256(abi.encode("escrow", uint256(2))), path);
        _log("refund ERC-20 (18 dec)");
    }

    function test_gas_claimOwedERC20() public {
        busd.setBlacklisted(recipient, true);
        FerrataVault.ReleaseRequest memory r = _request(address(busd), 1, recipient, ROOT, keccak256("n6"));
        _release(r); // credited: the token refuses the transfer
        busd.setBlacklisted(recipient, false);
        vm.prank(recipient);
        vault.claimOwed(address(busd));
        _log("claimOwed ERC-20 by the payee");
    }

    function test_gas_claimOwedETH_viaContractPayee() public {
        ToggleRecipient payee = new ToggleRecipient();
        _release(_request(ETH, 1, address(payee), ROOT, keccak256("n7")));
        payee.setOpen(true);
        payee.claim(address(vault), ETH);
        _log("claimOwed ETH, called through the payee contract");
    }

    /// @notice The mock verifier's own execution inside the measured releases: one cold storage read plus
    ///         call overhead. Estimated as (transaction gas of a direct verify call) - 21000 - calldata gas.
    function test_gas_mockVerifierExecution() public {
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, keccak256("n4"));
        bytes32 digest = sha256(_releaseJournalBytes(_releaseJournal(r)));
        verifier.accept(RELEASE_IMAGE_ID, digest);
        bytes memory callData = abi.encodeCall(verifier.verify, (SEAL, RELEASE_IMAGE_ID, digest));
        // A plain call, not staticcall: --isolate turns top-level calls into transactions.
        (bool ok,) = address(verifier).call(callData);
        assertTrue(ok);
        uint256 total = vm.lastCallGas().gasTotalUsed;
        // Meaningful under --isolate only, where `total` includes the 21000 base cost and calldata.
        if (total > 21_000 + _calldataGas(callData)) {
            console2.log("mock verifier execution (estimate)", total - 21_000 - _calldataGas(callData));
        }
    }
}
