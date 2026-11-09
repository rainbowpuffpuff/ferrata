// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

import {FerrataVault} from "ferrata-vault/FerrataVault.sol";
import {IRiscZeroVerifier} from "ferrata-vault/interfaces/IRiscZeroVerifier.sol";

/// @notice 6-decimal test token ("TUSD") with open minting.
contract TestToken is ERC20 {
    constructor() ERC20("Test USD", "TUSD") {}

    function decimals() public pure override returns (uint8) {
        return 6;
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}

/// @notice The vault with its internal release-journal digest exposed, so the encoding can be compared
///         with the Rust one using the vault's own code (block.chainid, address(this), lezConfigId).
contract FerrataVaultHarness is FerrataVault {
    constructor(IRiscZeroVerifier verifier_, bytes32 lezConfigId_)
        FerrataVault(
            verifier_,
            keccak256("harness: release image"),
            keccak256("harness: root image"),
            lezConfigId_,
            address(0xA11CE),
            address(0xF4EE2E),
            1,
            CapParams({capacity: type(uint256).max, refillPerSecond: 0}),
            CapParams({capacity: type(uint256).max, refillPerSecond: 0})
        )
    {}

    function releaseJournalDigest(ReleaseRequest calldata r, uint256 amount) external view returns (bytes32) {
        return _releaseJournalDigest(r, amount);
    }
}

/// @notice TEST ONLY. Stands in for the not-yet-built LEZ burn-root proof: answers the vault's root-proof
///         check from journals a test registered, and forwards every other check (the release proofs) to
///         the real RISC Zero Groth16 verifier. Used to exercise `acceptBurnRoot` itself; the gas
///         figures for `release` come from a vault wired directly to the real verifier.
contract RootMockingVerifier is IRiscZeroVerifier {
    error RootJournalNotAccepted();

    IRiscZeroVerifier public immutable real;
    bytes32 public immutable rootImageId;
    mapping(bytes32 journalDigest => bool) public acceptedRootJournal;

    constructor(IRiscZeroVerifier real_, bytes32 rootImageId_) {
        real = real_;
        rootImageId = rootImageId_;
    }

    function acceptRootJournal(bytes32 journalDigest) external {
        acceptedRootJournal[journalDigest] = true;
    }

    function verify(bytes calldata seal, bytes32 imageId, bytes32 journalDigest) external view {
        if (imageId == rootImageId) {
            if (!acceptedRootJournal[journalDigest]) revert RootJournalNotAccepted();
            return;
        }
        real.verify(seal, imageId, journalDigest);
    }
}
