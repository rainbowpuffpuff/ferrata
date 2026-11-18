// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;
/// A stand-in for FerrataVault's deposit bookkeeping. guarded: like depositETH (index must equal depositCount, else
/// revert); open: a call that has an effect every time (like a DeployVault step that creates a contract).
contract Dep {
    mapping(address => uint64) public depositCount;
    uint256 public effects;
    function depositGuarded(uint64 index) external { require(index == depositCount[msg.sender], "WrongDepositIndex"); depositCount[msg.sender] = index + 1; effects++; }
    function depositOpen() external { depositCount[msg.sender]++; effects++; }
}
