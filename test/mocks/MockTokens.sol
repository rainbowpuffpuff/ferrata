// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

/// @notice Balances, allowances and minting shared by the token mocks. Each mock defines its own
///         `transfer`/`transferFrom` so it can differ in return type and behaviour.
abstract contract MockTokenBase {
    error InsufficientBalance();
    error InsufficientAllowance();

    string public name;
    string public symbol;
    uint8 public immutable decimals;
    uint256 public totalSupply;
    mapping(address account => uint256) public balanceOf;
    mapping(address owner => mapping(address spender => uint256)) public allowance;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    constructor(string memory name_, string memory symbol_, uint8 decimals_) {
        name = name_;
        symbol = symbol_;
        decimals = decimals_;
    }

    function mint(address to, uint256 amount) external {
        totalSupply += amount;
        balanceOf[to] += amount;
        emit Transfer(address(0), to, amount);
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    function _spendAllowance(address owner, address spender, uint256 amount) internal {
        uint256 current = allowance[owner][spender];
        if (current < amount) revert InsufficientAllowance();
        if (current != type(uint256).max) allowance[owner][spender] = current - amount;
    }

    function _move(address from, address to, uint256 sent, uint256 received) internal {
        if (balanceOf[from] < sent) revert InsufficientBalance();
        balanceOf[from] -= sent;
        balanceOf[to] += received;
        totalSupply -= sent - received;
        emit Transfer(from, to, received);
    }
}

/// @notice Well-behaved ERC-20: transfer and transferFrom return true.
contract MockERC20 is MockTokenBase {
    constructor(string memory name_, string memory symbol_, uint8 decimals_) MockTokenBase(name_, symbol_, decimals_) {}

    function transfer(address to, uint256 amount) external returns (bool) {
        _move(msg.sender, to, amount, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        _spendAllowance(from, msg.sender, amount);
        _move(from, to, amount, amount);
        return true;
    }
}

/// @notice USDT-style token: transfer and transferFrom return nothing.
contract MockUSDT is MockTokenBase {
    constructor(string memory name_, string memory symbol_, uint8 decimals_) MockTokenBase(name_, symbol_, decimals_) {}

    function transfer(address to, uint256 amount) external {
        _move(msg.sender, to, amount, amount);
    }

    function transferFrom(address from, address to, uint256 amount) external {
        _spendAllowance(from, msg.sender, amount);
        _move(from, to, amount, amount);
    }
}

/// @notice Burns 1% of every transfer, so the receiver gets less than `amount`.
contract MockFeeOnTransferToken is MockTokenBase {
    constructor() MockTokenBase("Fee Token", "FEE", 18) {}

    function transfer(address to, uint256 amount) external returns (bool) {
        _move(msg.sender, to, amount, amount - amount / 100);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        _spendAllowance(from, msg.sender, amount);
        _move(from, to, amount, amount - amount / 100);
        return true;
    }
}

/// @notice Returns false from transfer and transferFrom without moving anything (and without reverting).
contract MockFalseReturnToken is MockTokenBase {
    constructor() MockTokenBase("False Token", "FALSE", 18) {}

    function transfer(address, uint256) external pure returns (bool) {
        return false;
    }

    function transferFrom(address, address, uint256) external pure returns (bool) {
        return false;
    }
}

/// @notice Calls an arbitrary hook before moving funds in transfer/transferFrom, records how the hook ended, and
///         then completes the transfer normally. Used to attempt re-entry into the vault.
contract MockReentrantToken is MockTokenBase {
    address public hookTarget;
    bytes public hookData;
    bool public hookCalled;
    bool public hookSucceeded;
    bytes public hookReturnData;

    constructor() MockTokenBase("Reentrant Token", "REENTER", 18) {}

    function setHook(address target, bytes calldata data) external {
        hookTarget = target;
        hookData = data;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        _runHook();
        _move(msg.sender, to, amount, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        _runHook();
        _spendAllowance(from, msg.sender, amount);
        _move(from, to, amount, amount);
        return true;
    }

    function _runHook() internal {
        if (hookTarget == address(0)) return;
        address target = hookTarget;
        hookTarget = address(0); // one attempt per arming
        hookCalled = true;
        (hookSucceeded, hookReturnData) = target.call(hookData);
    }
}

/// @notice USDC-style token whose issuer can blacklist addresses: transfers from or to them revert.
contract MockBlacklistToken is MockTokenBase {
    error Blacklisted();

    mapping(address account => bool) public blacklisted;

    constructor() MockTokenBase("Blacklisting USD", "BUSD", 6) {}

    function setBlacklisted(address account, bool value) external {
        blacklisted[account] = value;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        if (blacklisted[msg.sender] || blacklisted[to]) revert Blacklisted();
        _move(msg.sender, to, amount, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        if (blacklisted[from] || blacklisted[to]) revert Blacklisted();
        _spendAllowance(from, msg.sender, amount);
        _move(from, to, amount, amount);
        return true;
    }
}
