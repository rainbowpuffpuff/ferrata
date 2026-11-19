// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;
/// A stand-in for FerrataVault's deposit bookkeeping. guarded: like depositETH (index must equal depositCount, else
/// revert); open: a call that has an effect every time (like a DeployVault step that creates a contract); tagged: the
/// same with calldata that carries a value of the plan (the review's round-2 case v1b).
contract Dep {
    mapping(address => uint64) public depositCount;
    uint256 public effects;
    function depositGuarded(uint64 index) external { require(index == depositCount[msg.sender], "WrongDepositIndex"); depositCount[msg.sender] = index + 1; effects++; }
    function depositOpen() external { depositCount[msg.sender]++; effects++; }
    function depositTagged(uint256 tag) external { tag; depositCount[msg.sender]++; effects++; }
}
