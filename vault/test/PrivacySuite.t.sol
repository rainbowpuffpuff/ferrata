// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Vm} from "forge-std/Vm.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";
import {IRiscZeroVerifier as IRiscZeroVerifierFull, Receipt} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Test only (the privacy suite with real proofs): a release's seal goes to RISC Zero's Groth16 verifier, so
///         the vault checks the real proof of the release statement; every other image (the checkpoint, whose burn
///         root this suite accepts on synthetic LEZ data) goes to the mock. `releaseOnMock` sends releases to the mock
///         too, for the naive-nullifier variant, which no real release program proves. The vault reaches it through
///         RISC Zero's emergency stop, as it reaches any verifier (its constructor requires a stop).
contract ReleaseSplitVerifier is IRiscZeroVerifierFull {
    IRiscZeroVerifier public immutable groth16;
    IRiscZeroVerifier public immutable mock;
    bytes32 public immutable releaseImageId;
    bool public releaseOnMock;

    constructor(IRiscZeroVerifier groth16_, IRiscZeroVerifier mock_, bytes32 releaseImageId_) {
        (groth16, mock, releaseImageId) = (groth16_, mock_, releaseImageId_);
    }

    function setReleaseOnMock(bool onMock) external {
        releaseOnMock = onMock;
    }

    function verify(bytes calldata seal, bytes32 imageId, bytes32 journalDigest) external view {
        if (imageId == releaseImageId && !releaseOnMock) {
            groth16.verify(seal, imageId, journalDigest);
        } else {
            mock.verify(seal, imageId, journalDigest);
        }
    }

    /// @dev Not on the vault's paths (it calls `verify` only); the mock's, for the interface the stop requires.
    function verifyIntegrity(Receipt calldata receipt) external view {
        IRiscZeroVerifierFull(address(mock)).verifyIntegrity(receipt);
    }
}

/// @notice The privacy suite, Ethereum half (proposal B3; RFP-022 Privacy #1–#7). It replays the scenario the LEZ
///         half (`lez/harness/tests/privacy_suite.rs`) wrote to `target/privacy/scenario.json` at the repository root
///         (`FERRATA_PRIVACY_DIR` overrides) on a vault deployed at the scenario's address on chain 1, with the mock
///         verifier, and writes everything public about each deposit and release to `eth.jsonl` there: sender,
///         value, calldata, events, every storage write and every balance change. It skips when no scenario exists.
///
///         Universe A: 20 deposits whose commitments hide private LEZ destinations; the burn root of the 20 LEZ burns
///         accepted the way the vault allows (candidate, wait, finalisation); then the 20 releases in the scenario's
///         order with the nullifier the release statement computes, and, from the same state, the same releases with
///         the naive nullifier (the burn's statement identifier). Universe B, from the fresh vault: the deposits of the
///         plaintext variant (public LEZ destinations). Cross-checks: the vault's deposit root equals the LEZ
///         harness's tree root in both universes, and the vault accepts each release journal the bridge core wrote
///         (the mock verifier accepts exactly the registered digest, so a byte of difference reverts).
///
///         With real proofs (`release-seals.json` beside the scenario: `scripts/privacy/real.sh`), the vault is deployed
///         with the real release image id behind `ReleaseSplitVerifier`, and each private release carries the Groth16
///         seal of its release statement's real proof, checked by RISC Zero's Groth16 verifier; before the first, the
///         same release with one seal byte flipped must revert. The naive variant stays on the mock.
contract PrivacySuiteTest is VaultTestBase {
    string internal dir;
    string internal scenario;
    string internal outPath;
    address internal vaultAddress;
    bytes32 internal configId;
    address internal ethRelayer;
    uint256 internal users;
    string internal seals; // release-seals.json with real proofs, else empty
    bytes32 internal releaseId; // the release image id the vault was deployed with
    ReleaseSplitVerifier internal split;

    function test_privacySuite_ethereumHalf() public {
        dir = vm.envOr("FERRATA_PRIVACY_DIR", string("../target/privacy"));
        string memory path = string.concat(dir, "/scenario.json");
        if (!vm.exists(path)) {
            vm.skip(true);
            return;
        }
        scenario = vm.readFile(path);
        vaultAddress = vm.parseJsonAddress(scenario, ".vault");
        configId = vm.parseJsonBytes32(scenario, ".config_id");
        ethRelayer = vm.parseJsonAddress(scenario, ".relayer");
        users = vm.parseJsonUint(scenario, ".users");
        outPath = string.concat(dir, "/eth.jsonl");
        vm.writeFile(outPath, "");

        vm.chainId(vm.parseJsonUint(scenario, ".chain_id"));
        string memory sealsPath = string.concat(dir, "/release-seals.json");
        if (vm.exists(sealsPath)) {
            seals = vm.readFile(sealsPath);
            releaseId = vm.parseJsonBytes32(seals, ".release_image_id");
            RiscZeroGroth16Verifier groth16 =
                new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
            split = new ReleaseSplitVerifier(
                IRiscZeroVerifier(address(groth16)), IRiscZeroVerifier(address(verifier)), releaseId
            );
            RiscZeroVerifierEmergencyStop stop = new RiscZeroVerifierEmergencyStop(IRiscZeroVerifierFull(address(split)), freezer);
            FerrataVault.Deployment memory d = _deployment(IRiscZeroVerifier(address(stop)), configId);
            d.releaseImageId = releaseId;
            _deployAt(vaultAddress, abi.encode(d));
        } else {
            releaseId = RELEASE_IMAGE_ID;
            _deployAt(vaultAddress, abi.encode(_deployment(IRiscZeroVerifier(address(estop)), configId)));
        }
        vault = FerrataVault(payable(vaultAddress));
        _registerDefaultTokens(vault);
        assertEq(_relayFee(ETH, 1), vm.parseUint(vm.parseJsonString(scenario, ".relay_fee_wei")), "relay fee");
        assertEq(_amount(ETH, 1), vm.parseUint(vm.parseJsonString(scenario, ".amount_wei")), "denomination");
        uint256 fresh = vm.snapshotState();

        // Universe A: the default private path.
        _deposits("A", "private");
        bytes32 burnRoot = vm.parseJsonBytes32(scenario, ".burn_root");
        _acceptRootOn(vault, configId, burnRoot);
        uint256 accepted = vm.snapshotState();
        _releases("private", burnRoot);
        vm.revertToState(accepted);
        vm.chainId(vm.parseJsonUint(scenario, ".chain_id"));
        _releases("naive", burnRoot);

        // Universe B: the plaintext variant's deposits.
        vm.revertToState(fresh);
        vm.chainId(vm.parseJsonUint(scenario, ".chain_id"));
        _deposits("B", "plaintext");
    }

    /// @dev The vault's own creation code run at `where` (constructor storage and immutables land there).
    function _deployAt(address where, bytes memory args) internal {
        vm.etch(where, abi.encodePacked(type(FerrataVault).creationCode, args));
        (bool ok, bytes memory runtime) = where.call("");
        require(ok, "deploy");
        vm.etch(where, runtime);
    }

    function _deposits(string memory universe, string memory variant) internal {
        string memory key = string.concat(".", universe, ".deposits");
        for (uint256 u = 0; u < users; u++) {
            string memory at = string.concat(key, "[", vm.toString(u), "]");
            address depositor = vm.parseJsonAddress(scenario, string.concat(at, ".depositor"));
            bytes32 commitment = vm.parseJsonBytes32(scenario, string.concat(at, ".commitment"));
            // v5 (D13): the denomination plus the mint and relay fees in force, exactly.
            uint256 value = _depositTotal(vault, ETH, 1);
            vm.deal(depositor, value);
            bytes memory callData = abi.encodeCall(FerrataVault.depositETH, (1, commitment, 0));
            _record(
                string.concat("eth:deposit:", variant, ":", _two(u)), "deposit", variant, u, depositor, value, callData
            );
        }
        assertEq(vault.nextLeafIndex(), users, "leaf count");
        assertEq(
            vm.load(address(vault), bytes32(0)),
            vm.parseJsonBytes32(scenario, string.concat(".", universe, ".root")),
            "the vault's deposit root is the LEZ harness's tree root"
        );
    }

    function _releases(string memory variant, bytes32 burnRoot) internal {
        string memory key = string.concat(".releases.", variant);
        bool real = bytes(seals).length > 0 && keccak256(bytes(variant)) == keccak256("private");
        if (bytes(seals).length > 0 && !real) split.setReleaseOnMock(true);
        for (uint256 k = 0; k < users; k++) {
            string memory at = string.concat(key, "[", vm.toString(k), "]");
            FerrataVault.ReleaseRequest memory r = FerrataVault.ReleaseRequest({
                token: ETH,
                denomIndex: 1,
                recipient: vm.parseJsonAddress(scenario, string.concat(at, ".recipient")),
                relayer: ethRelayer,
                relayFee: _relayFee(ETH, 1),
                burnRoot: burnRoot,
                nullifier: vm.parseJsonBytes32(scenario, string.concat(at, ".nullifier"))
            });
            bytes memory seal = SEAL;
            if (real) {
                // The Groth16 seal of the release statement's real proof: its journal must be the one the vault rebuilds.
                seal = vm.parseJsonBytes(seals, string.concat(".seals[", vm.toString(k), "]"));
                if (k == 0) {
                    bytes memory bad = bytes.concat(seal);
                    bad[100] = bad[100] ^ 0x01;
                    vm.prank(ethRelayer);
                    // Bounded gas, as test/real/ does: a byte flipped inside a curve point makes the pairing
                    // precompile fail, which consumes all the gas it was given.
                    (bool accepted,) = address(vault).call{gas: 5_000_000}(abi.encodeCall(FerrataVault.release, (r, bad)));
                    assertFalse(accepted, "a seal with one byte flipped is refused");
                }
            } else {
                // The journal the bridge core's release statement committed to: the vault must rebuild it exactly.
                verifier.accept(releaseId, sha256(vm.parseJsonBytes(scenario, string.concat(at, ".journal"))));
            }
            bytes memory callData = abi.encodeCall(FerrataVault.release, (r, seal));
            _record(
                string.concat("eth:release:", variant, ":", _two(k)), "release", variant, k, ethRelayer, 0, callData
            );
            assertTrue(vault.nullifierUsed(r.nullifier), "released");
        }
    }

    /// @dev One transaction in a block of its own, with everything it makes public written as one JSON line.
    function _record(
        string memory id,
        string memory kind,
        string memory variant,
        uint256 seq,
        address from,
        uint256 value,
        bytes memory callData
    ) internal {
        vm.roll(block.number + 1);
        vm.warp(block.timestamp + 12);
        vm.recordLogs();
        vm.getRecordedLogs();
        vm.startStateDiffRecording();
        vm.prank(from);
        (bool ok,) = address(vault).call{value: value}(callData);
        uint256 gasUsed = vm.lastCallGas().gasTotalUsed;
        Vm.AccountAccess[] memory accesses = vm.stopAndReturnStateDiff();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        assertTrue(ok, id);
        string memory line = string.concat('{"id":"', id, '","chain":"eth","kind":"', kind, '","variant":"', variant);
        line = string.concat(line, '","seq":', vm.toString(seq), ',"block":', vm.toString(block.number));
        line = string.concat(line, ',"time":', vm.toString(block.timestamp), ',"gas":', vm.toString(gasUsed));
        line = string.concat(line, ',"from":"', vm.toString(from));
        line = string.concat(line, '","to":"', vm.toString(address(vault)), '","value":"', vm.toString(value));
        line = string.concat(line, '","calldata":"', vm.toString(callData), '","logs":', _logs(logs));
        line = string.concat(line, ',"writes":', _writes(accesses), ',"balances":', _balances(accesses), "}");
        vm.writeLine(outPath, line);
    }

    function _logs(Vm.Log[] memory logs) internal pure returns (string memory out) {
        out = "[";
        for (uint256 i = 0; i < logs.length; i++) {
            string memory topics = "[";
            for (uint256 t = 0; t < logs[i].topics.length; t++) {
                topics = string.concat(topics, t == 0 ? '"' : ',"', vm.toString(logs[i].topics[t]), '"');
            }
            out = string.concat(out, i == 0 ? "" : ",", '{"emitter":"', vm.toString(logs[i].emitter), '","topics":');
            out = string.concat(out, topics, '],"data":"', vm.toString(logs[i].data), '"}');
        }
        out = string.concat(out, "]");
    }

    function _writes(Vm.AccountAccess[] memory accesses) internal pure returns (string memory out) {
        out = "[";
        bool first = true;
        for (uint256 i = 0; i < accesses.length; i++) {
            for (uint256 s = 0; s < accesses[i].storageAccesses.length; s++) {
                Vm.StorageAccess memory a = accesses[i].storageAccesses[s];
                if (!a.isWrite || a.reverted) continue;
                out = string.concat(out, first ? "" : ",", '{"account":"', vm.toString(a.account), '","slot":"');
                out = string.concat(out, vm.toString(a.slot), '","old":"', vm.toString(a.previousValue), '","new":"');
                out = string.concat(out, vm.toString(a.newValue), '"}');
                first = false;
            }
        }
        out = string.concat(out, "]");
    }

    function _balances(Vm.AccountAccess[] memory accesses) internal pure returns (string memory out) {
        out = "[";
        bool first = true;
        for (uint256 i = 0; i < accesses.length; i++) {
            if (accesses[i].oldBalance == accesses[i].newBalance || accesses[i].reverted) continue;
            out = string.concat(out, first ? "" : ",", '{"account":"', vm.toString(accesses[i].account), '","old":"');
            out = string.concat(
                out, vm.toString(accesses[i].oldBalance), '","new":"', vm.toString(accesses[i].newBalance)
            );
            out = string.concat(out, '"}');
            first = false;
        }
        out = string.concat(out, "]");
    }

    function _two(uint256 k) internal pure returns (string memory) {
        return k < 10 ? string.concat("0", vm.toString(k)) : vm.toString(k);
    }
}
