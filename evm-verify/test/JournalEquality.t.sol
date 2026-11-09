// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";

import {FerrataVault} from "ferrata-vault/FerrataVault.sol";
import {IRiscZeroVerifier} from "ferrata-vault/interfaces/IRiscZeroVerifier.sol";
import {FerrataVaultHarness} from "./utils/TestContracts.sol";

/// @notice The Rust release journal (ferrata-bridge-core `ReleaseJournal::encode`, vectors/release-journal.json)
///         equals, byte for byte, what Solidity computes: (1) `abi.encode` of the eleven fields, as written in
///         `FerrataVault._releaseJournalDigest`; (2) for every deployable case, the vault's own
///         `_releaseJournalDigest`, run from the case's vault address on the case's chain id.
contract JournalEqualityTest is Test {
    struct Case {
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
        bytes journal;
        bytes32 digest;
    }

    string internal json;

    function setUp() public {
        json = vm.readFile("../vectors/release-journal.json");
    }

    function _case(uint256 i) internal view returns (Case memory c) {
        string memory p = string.concat(".cases[", vm.toString(i), "].");
        c.chainId = vm.parseJsonUint(json, string.concat(p, "chainId"));
        c.vault = vm.parseJsonAddress(json, string.concat(p, "vault"));
        c.lezConfigId = vm.parseJsonBytes32(json, string.concat(p, "lezConfigId"));
        c.burnRoot = vm.parseJsonBytes32(json, string.concat(p, "burnRoot"));
        c.nullifier = vm.parseJsonBytes32(json, string.concat(p, "nullifier"));
        c.token = vm.parseJsonAddress(json, string.concat(p, "token"));
        c.amount = uint256(vm.parseJsonBytes32(json, string.concat(p, "amount")));
        c.recipient = vm.parseJsonAddress(json, string.concat(p, "recipient"));
        c.feeRecipient = vm.parseJsonAddress(json, string.concat(p, "feeRecipient"));
        c.relayFee = uint256(vm.parseJsonBytes32(json, string.concat(p, "relayFee")));
        c.journal = vm.parseJsonBytes(json, string.concat(p, "journal"));
        c.digest = vm.parseJsonBytes32(json, string.concat(p, "digest"));
    }

    function _count() internal view returns (uint256) {
        return vm.parseJsonUint(json, ".count");
    }

    function test_rustJournal_equals_abiEncode_forEveryVector() public view {
        uint256 n = _count();
        assertGe(n, 6);
        for (uint256 i = 0; i < n; i++) {
            Case memory c = _case(i);
            bytes memory solidity = abi.encode(
                sha256("ferrata/vault/release/v1"),
                c.chainId,
                c.vault,
                c.lezConfigId,
                c.burnRoot,
                c.nullifier,
                c.token,
                c.amount,
                c.recipient,
                c.feeRecipient,
                c.relayFee
            );
            assertEq(solidity.length, 352);
            assertEq(solidity, c.journal, string.concat("journal bytes, case ", vm.toString(i)));
            assertEq(sha256(solidity), c.digest, string.concat("digest, case ", vm.toString(i)));
        }
    }

    function test_rustJournal_equals_vaultsOwnDigest_forEveryDeployableVector() public {
        uint256 n = _count();
        uint256 checked;
        IRiscZeroVerifier anyCode = IRiscZeroVerifier(address(new FerrataVaultHarnessStub()));
        for (uint256 i = 0; i < n; i++) {
            Case memory c = _case(i);
            // The vault constructor rejects a zero lezConfigId; address(0) cannot hold the vault.
            if (c.lezConfigId == bytes32(0) || c.vault == address(0)) continue;
            vm.chainId(c.chainId);
            deployCodeTo("TestContracts.sol:FerrataVaultHarness", abi.encode(anyCode, c.lezConfigId), c.vault);
            FerrataVault.ReleaseRequest memory r = FerrataVault.ReleaseRequest({
                token: c.token,
                denomIndex: 0,
                recipient: c.recipient,
                feeRecipient: c.feeRecipient,
                relayFee: c.relayFee,
                burnRoot: c.burnRoot,
                nullifier: c.nullifier
            });
            assertEq(
                FerrataVaultHarness(payable(c.vault)).releaseJournalDigest(r, c.amount),
                c.digest,
                string.concat("vault digest, case ", vm.toString(i))
            );
            checked++;
        }
        assertEq(checked, n - 1); // all but the all-zero case
    }
}

/// @dev Any contract with code satisfies the vault constructor's verifier check; it is never called here.
contract FerrataVaultHarnessStub {}
