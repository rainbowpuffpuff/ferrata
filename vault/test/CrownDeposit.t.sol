// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {Vm} from "forge-std/Vm.sol";

import {Crown} from "../script/Crown.s.sol";
import {TestToken} from "../script/TestToken.sol";
import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @dev Calls Crown with the default Foundry broadcaster as msg.sender; etching this at that address aligns it with
///      the account selected by Crown's parameterless vm.broadcast(), without a prank or nested broadcast.
contract CrownCaller {
    function deposit(Crown crown, uint256 j) external {
        crown.deposit(j);
    }
}

/// @notice Executes Crown.s.sol's deposit entry point, rather than repeating its calls in a test. The broadcaster's
///         nonce proves that ETH still broadcasts one transaction and ERC-20 broadcasts mint, approve and deposit.
contract CrownDepositTest is VaultTestBase {
    uint256 internal constant ETH_DENOMINATION = 0.1 ether;
    uint256 internal constant ETH_TOTAL = ETH_DENOMINATION + ETH_DENOMINATION / 500;
    uint256 internal constant TOKEN_DENOMINATION = 100e6;
    uint256 internal constant TOKEN_TOTAL = TOKEN_DENOMINATION + TOKEN_DENOMINATION / 500;
    bytes32 internal constant COMMITMENT = keccak256("Crown.s.sol deposit test commitment");

    TestToken internal token;
    Crown internal crown;
    address internal depositor = 0x1804c8AB1F12E6bbf3894d4083f33e07309d1f38; // forge's default broadcaster

    function setUp() public override {
        super.setUp();
        CrownCaller caller = new CrownCaller();
        vm.etch(depositor, address(caller).code);
        _deployRuleB();
    }

    function _deployRuleB() internal {
        vault = new FerrataVault(_deploymentB(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID));
        token = new TestToken();
        FerrataVault.TokenParams memory tokenParams = _params(_denoms(100e6, 1000e6, 10_000e6), 1, 1);
        tokenParams.decimals = 6;
        vm.startPrank(admin);
        vault.registerToken(ETH, _params(_denoms(0.1 ether, 1 ether, 10 ether), 1e12, 3000));
        vault.registerToken(address(token), tokenParams);
        vm.stopPrank();
    }

    /// @dev Crown.setUp reads the same crown.json shape as the live driver. The generated input uses the existing
    ///      ignored deployments/crown-* output namespace and is removed as soon as setUp has read it.
    function _crownFor(address depositedToken, string memory name) internal {
        string memory path = string.concat("deployments/crown-deposit-test-", name, ".json");
        string memory tokenField =
            depositedToken == ETH ? "" : string.concat(',"token":"', vm.toString(depositedToken), '"');
        vm.writeJson(
            string.concat(
                '{"chainId":',
                vm.toString(block.chainid),
                ',"vault":"',
                vm.toString(address(vault)),
                '","deposits":[{"j":0,"denomIndex":0,"commitment":"',
                vm.toString(COMMITMENT),
                '"',
                tokenField,
                "}]}"
            ),
            path
        );
        vm.setEnv("FERRATA_CROWN", path);
        crown = new Crown();
        crown.setUp();
        vm.removeFile(path);
    }

    function test_crownDeposit_ethOneTransactionAndErc20ThreeExactTransactions() public {
        _ethBranch();
        _deployRuleB(); // a fresh vault gives the second branch its own deposit zero and first leaf
        _erc20Branch();
    }

    function _ethBranch() internal {
        _crownFor(ETH, "eth");
        vm.deal(depositor, ETH_TOTAL);
        uint64 nonceBefore = vm.getNonce(depositor);
        vm.recordLogs();
        CrownCaller(depositor).deposit(crown, 0);
        Vm.Log[] memory logs = vm.getRecordedLogs();

        assertEq(vm.getNonce(depositor) - nonceBefore, 1, "ETH deposit must remain one broadcast");
        _assertOneDeposit(logs, ETH, ETH_DENOMINATION);
        assertEq(logs.length, 1, "ETH path emitted an unexpected transaction event");
        assertEq(depositor.balance, 0);
        assertEq(address(vault).balance, ETH_TOTAL);
        assertEq(vault.escrowed(ETH), ETH_TOTAL);
        _assertFirstLeaf(ETH, ETH_DENOMINATION);
    }

    function _erc20Branch() internal {
        _crownFor(address(token), "erc20");
        uint64 nonceBefore = vm.getNonce(depositor);
        vm.recordLogs();
        CrownCaller(depositor).deposit(crown, 0);
        Vm.Log[] memory logs = vm.getRecordedLogs();

        assertEq(vm.getNonce(depositor) - nonceBefore, 3, "ERC-20 deposit must broadcast mint, approve, deposit");
        uint256 mints;
        uint256 approvals;
        for (uint256 i = 0; i < logs.length; i++) {
            Vm.Log memory log = logs[i];
            if (log.emitter != address(token)) continue;
            if (log.topics[0] == IERC20.Transfer.selector) {
                address from = address(uint160(uint256(log.topics[1])));
                address to = address(uint160(uint256(log.topics[2])));
                uint256 amount = abi.decode(log.data, (uint256));
                if (from == address(0)) {
                    mints++;
                    assertEq(to, depositor);
                    assertEq(amount, TOKEN_TOTAL, "mint must be the whole deposit total");
                } else {
                    assertEq(from, depositor);
                    assertEq(to, address(vault));
                    assertEq(amount, TOKEN_TOTAL, "escrow transfer must be the whole deposit total");
                }
            } else if (log.topics[0] == IERC20.Approval.selector) {
                approvals++;
                assertEq(address(uint160(uint256(log.topics[1]))), depositor);
                assertEq(address(uint160(uint256(log.topics[2]))), address(vault));
                assertEq(abi.decode(log.data, (uint256)), TOKEN_TOTAL, "approval must be exactly the deposit total");
            }
        }
        assertEq(mints, 1);
        assertEq(approvals, 1);
        _assertOneDeposit(logs, address(token), TOKEN_DENOMINATION);
        assertEq(logs.length, 4, "expected mint Transfer, Approval, escrow Transfer and vault Deposit");
        assertEq(token.balanceOf(depositor), 0);
        assertEq(token.balanceOf(address(vault)), TOKEN_TOTAL);
        assertEq(token.allowance(depositor, address(vault)), 0, "the exact approval is consumed by the deposit");
        assertEq(vault.escrowed(address(token)), TOKEN_TOTAL);
        _assertFirstLeaf(address(token), TOKEN_DENOMINATION);
    }

    function _assertOneDeposit(Vm.Log[] memory logs, address depositedToken, uint256 denomination) internal view {
        uint256 deposits;
        for (uint256 i = 0; i < logs.length; i++) {
            Vm.Log memory log = logs[i];
            if (log.emitter != address(vault) || log.topics[0] != FerrataVault.Deposit.selector) continue;
            deposits++;
            assertEq(address(uint160(uint256(log.topics[1]))), depositor);
            assertEq(address(uint160(uint256(log.topics[2]))), depositedToken);
            (uint64 index, uint64 leafIndex, uint256 amount, bytes32 commitment) =
                abi.decode(log.data, (uint64, uint64, uint256, bytes32));
            assertEq(index, 0);
            assertEq(leafIndex, 0);
            assertEq(amount, denomination);
            assertEq(commitment, COMMITMENT);
        }
        assertEq(deposits, 1, "Crown.deposit did not record exactly one vault deposit");
        assertEq(vault.depositCount(depositor), 1);
        assertEq(vault.nextLeafIndex(), 1);
    }

    /// @dev The root after leaf zero, recomputed from the reference leaf and empty-subtree rules.
    function _assertFirstLeaf(address depositedToken, uint256 denomination) internal view {
        bytes32 node = ReferenceMerkle.leafHash(0, depositor, depositedToken, denomination, COMMITMENT);
        bytes32 empty = bytes32(0);
        for (uint256 height = 0; height < 32; height++) {
            node = ReferenceMerkle.nodeHash(node, empty);
            empty = ReferenceMerkle.nodeHash(empty, empty);
        }
        assertEq(vault.depositRoot(), node, "the appended leaf or its Merkle root differs");
    }
}
