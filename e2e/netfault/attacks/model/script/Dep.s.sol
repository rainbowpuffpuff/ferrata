// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;
import {Dep} from "../src/Dep.sol";
interface Vm { function broadcast() external; function envAddress(string calldata) external view returns (address); }
/// Crown.s.sol's deposit(j), state-checked the same way: a deposit already on chain is skipped.
contract DepScript {
    bool public IS_SCRIPT = true;
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function guarded(uint256 j) external {
        Dep d = Dep(vm.envAddress("DEP"));
        address sender = msg.sender;
        if (d.depositCount(sender) > j) return; // already made (an interrupted stage started again)
        require(d.depositCount(sender) == j, "an earlier deposit of the run is missing");
        vm.broadcast();
        d.depositGuarded(uint64(j));
    }
    function open(uint256 j) external {
        Dep d = Dep(vm.envAddress("DEP"));
        if (d.depositCount(msg.sender) > j) return;
        vm.broadcast();
        d.depositOpen();
    }
}
