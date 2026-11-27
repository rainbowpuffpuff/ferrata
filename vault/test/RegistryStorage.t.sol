// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Vm} from "forge-std/Vm.sol";

import {FerrataVault} from "../src/FerrataVault.sol";

import {MockERC20} from "./mocks/MockTokens.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice The vault's own storage, for the LEZ side: every slot FerrataVault (and its linked library) wrote, at two
///         moments, and what an importer learns from the vault's events (registry entries, fee schedules, deposits),
///         with the depositors' secrets. `lez/harness/tests/bridge_vault_storage.rs` builds Ethereum blocks holding
///         exactly this storage and runs the bridge program on it: `Register`, `ImportFees` and `ImportRoot` attest
///         their slots, while ADR 0048's new `lezFees` slots are present for the separate LEZ fee collector to attest.
///         Nothing here computes a LEZ-side slot or hash.
///         Moment P: the default tokens and a six-decimal token with checkpoint fees registered, three deposits.
///         Moment Q: ETH's schedule 1 (mint, burn and checkpoint fees) in force from the next deposit, three more.
///         The committed file is test/vectors/registry-storage.json; FERRATA_WRITE_VECTORS=1 rewrites it first.
contract RegistryStorageTest is VaultTestBase {
    string internal constant PATH = "test/vectors/registry-storage.json";
    address internal constant DEPOSITOR = address(0xD0D0);
    uint8 internal constant PUBLIC_KIND = 0x02; // bridge core `DestKind::Public`

    MockERC20 internal six;

    // What the events say, collected as JSON fragments in the order the vault emitted them.
    mapping(address => string) internal entryAmounts; // TokenRegistered
    mapping(address => string) internal entryMeta; // RegistryEntryPublished
    address[] internal registered;
    mapping(address => mapping(uint256 => string)) internal announced; // FeesAnnounced
    string internal schedules;
    string internal deposits;
    string internal snapshots;
    // The wallet's side of each deposit: commitment => (s, dest).
    mapping(bytes32 => bytes32) internal secretOf;
    mapping(bytes32 => bytes32) internal destOf;
    uint256 internal deposited;

    function setUp() public override {
        vm.recordLogs();
        vm.record();
        super.setUp();
        six = new MockERC20("Six", "SIX", 6);
        uint256[] memory denoms = new uint256[](2);
        (denoms[0], denoms[1]) = (100e6, 1000e6);
        uint256[] memory relay = new uint256[](2);
        (relay[0], relay[1]) = (25e4, 25e5); // 0.25%
        uint256[] memory checkpoint = new uint256[](2);
        (checkpoint[0], checkpoint[1]) = (1e5, 1e6); // 0.1%
        vm.prank(admin);
        vault.registerToken(
            address(six),
            FerrataVault.TokenParams({
                denominations: denoms,
                relayFees: relay,
                lezUnit: 1,
                unitWeight: 1,
                depositCap: _unlimited(),
                releaseCap: _unlimited(),
                releaseDelay: 0,
                lezDefinition: keccak256("test: SIX on LEZ"),
                decimals: 6,
                checkpointFees: checkpoint
            })
        );
    }

    function test_registryStorage_matchesTheCommittedVectors() public {
        _depositFor(ETH, 1);
        _depositFor(address(six), 0);
        _depositFor(ETH, 0);
        _snapshot("P");

        uint256[] memory mintFees = new uint256[](3);
        uint256[] memory burnFees = new uint256[](3);
        uint256[] memory checkpointFees = new uint256[](3);
        uint256[3] memory d = [uint256(0.1 ether), 1 ether, 10 ether];
        for (uint256 i = 0; i < 3; i++) {
            mintFees[i] = d[i] / 200; // 0.5%, whole LEZ units (1e12)
            burnFees[i] = d[i] / 1000;
            checkpointFees[i] = d[i] / 2000;
        }
        vm.prank(admin);
        vault.announceFees(ETH, mintFees, burnFees, checkpointFees);
        vm.warp(block.timestamp + FEE_NOTICE);
        _depositFor(ETH, 1); // puts schedule 1 in force first: it applies from this leaf
        _depositFor(address(six), 1);
        _depositFor(ETH, 2);
        _snapshot("Q");

        string memory json = _json();
        if (vm.envOr("FERRATA_WRITE_VECTORS", false)) vm.writeFile(PATH, json);
        assertEq(
            vm.readFile(PATH), json, "test/vectors/registry-storage.json is stale: rerun with FERRATA_WRITE_VECTORS=1"
        );
    }

    function _json() internal view returns (string memory) {
        string memory head = string.concat(
            "{\n  \"description\": \"FerrataVault v5's own storage and events, written by vault/test/RegistryStorage.t.sol",
            " and read by lez/harness/tests/bridge_vault_storage.rs\",\n  \"vault\": \"",
            vm.toString(address(vault)),
            "\",\n  \"tokens\": [",
            _tokens()
        );
        string memory body =
            string.concat("\n  ],\n  \"schedules\": [", schedules, "\n  ],\n  \"deposits\": [", deposits);
        return string.concat(head, body, "\n  ],\n  \"snapshots\": [", snapshots, "\n  ]\n}\n");
    }

    // ---- the scenario ------------------------------------------------------------------------------------------

    /// A deposit to a public LEZ destination: the wallet's secret `s` and destination, and the commitment
    /// sha256("ferrata/C/v1" ‖ s ‖ kind ‖ dest) (bridge core `commitment`).
    function _depositFor(address token, uint8 denomIndex) internal {
        bytes32 s = sha256(abi.encodePacked("integration: registry storage, s", deposited));
        bytes32 dest = sha256(abi.encodePacked("integration: registry storage, LEZ destination", deposited));
        bytes32 c = sha256(abi.encodePacked("ferrata/C/v1", s, PUBLIC_KIND, dest));
        (secretOf[c], destOf[c]) = (s, dest);
        deposited++;
        _deposit(DEPOSITOR, token, denomIndex, c);
    }

    /// Reads the vault's events since the last snapshot, then every slot the vault has written so far with its value
    /// now (zero values left out, as in Ethereum's state).
    function _snapshot(string memory name) internal {
        _readLogs();
        (, bytes32[] memory writes) = vm.accesses(address(vault));
        string memory slots;
        uint256 n;
        for (uint256 i = 0; i < writes.length; i++) {
            if (_seenBefore(writes, i)) continue;
            bytes32 value = vm.load(address(vault), writes[i]);
            if (value == bytes32(0)) continue;
            slots = string.concat(
                slots, n == 0 ? "" : ",", "\n        [\"", vm.toString(writes[i]), "\", \"", vm.toString(value), "\"]"
            );
            n++;
        }
        string memory head = string.concat(
            "\n    {\n      \"name\": \"",
            name,
            "\",\n      \"leaf_count\": ",
            vm.toString(uint256(vault.nextLeafIndex()))
        );
        snapshots = string.concat(
            snapshots,
            bytes(snapshots).length == 0 ? "" : ",",
            head,
            ",\n      \"storage\": [",
            slots,
            "\n      ]\n    }"
        );
    }

    function _seenBefore(bytes32[] memory xs, uint256 i) internal pure returns (bool) {
        for (uint256 j = 0; j < i; j++) {
            if (xs[j] == xs[i]) return true;
        }
        return false;
    }

    // ---- the vault's events, as an importer reads them ---------------------------------------------------------

    function _readLogs() internal {
        Vm.Log[] memory logs = vm.getRecordedLogs();
        for (uint256 i = 0; i < logs.length; i++) {
            if (logs[i].emitter != address(vault)) continue;
            bytes32 topic = logs[i].topics[0];
            if (topic == FerrataVault.TokenRegistered.selector) _onRegistered(logs[i]);
            else if (topic == FerrataVault.RegistryEntryPublished.selector) _onEntry(logs[i]);
            else if (topic == FerrataVault.FeesAnnounced.selector) _onAnnounced(logs[i]);
            else if (topic == FerrataVault.FeesInForce.selector) _onInForce(logs[i]);
            else if (topic == FerrataVault.Deposit.selector) _onDeposit(logs[i]);
        }
    }

    function _onRegistered(Vm.Log memory log) internal {
        address token = address(uint160(uint256(log.topics[1])));
        (uint256[] memory denominations, uint256[] memory relayFees, uint256 lezUnit, uint256 unitWeight) =
            abi.decode(log.data, (uint256[], uint256[], uint256, uint256));
        registered.push(token);
        string memory units =
            string.concat("\"lez_unit\": \"", vm.toString(lezUnit), "\", \"unit_weight\": \"", vm.toString(unitWeight));
        entryAmounts[token] = string.concat(
            units, "\",\n      \"denominations\": ", _list(denominations), ", \"relay_fees\": ", _list(relayFees)
        );
    }

    function _onEntry(Vm.Log memory log) internal {
        address token = address(uint160(uint256(log.topics[1])));
        (bytes32 lezDefinition, uint8 decimals, bytes32 entryHash) = abi.decode(log.data, (bytes32, uint8, bytes32));
        entryMeta[token] = string.concat(
            "\"lez_definition\": \"",
            vm.toString(lezDefinition),
            "\", \"decimals\": ",
            vm.toString(uint256(decimals)),
            ",\n      \"entry_hash\": \"",
            vm.toString(entryHash),
            "\", "
        );
    }

    function _onAnnounced(Vm.Log memory log) internal {
        address token = address(uint160(uint256(log.topics[1])));
        uint256 index = uint256(log.topics[2]);
        (uint64 announcedAt,, uint256[] memory mintFees, uint256[] memory burnFees, uint256[] memory checkpointFees) =
            abi.decode(log.data, (uint64, uint64, uint256[], uint256[], uint256[]));
        string memory head = string.concat(
            "\"announced_at\": ", vm.toString(uint256(announcedAt)), ",\n      \"mint_fees\": ", _list(mintFees)
        );
        announced[token][index] = string.concat(
            head, ",\n      \"burn_fees\": ", _list(burnFees), ",\n      \"checkpoint_fees\": ", _list(checkpointFees)
        );
    }

    function _onInForce(Vm.Log memory log) internal {
        address token = address(uint160(uint256(log.topics[1])));
        uint256 index = uint256(log.topics[2]);
        (uint64 fromLeaf, uint64 effectiveAt, bytes32 scheduleHash) = abi.decode(log.data, (uint64, uint64, bytes32));
        string memory head =
            string.concat("\n    {\"token\": \"", vm.toString(token), "\", \"index\": ", vm.toString(index));
        string memory leaf = string.concat(
            ", \"from_leaf\": ",
            vm.toString(uint256(fromLeaf)),
            ", \"effective_at\": ",
            vm.toString(uint256(effectiveAt))
        );
        string memory hash = string.concat(",\n      \"schedule_hash\": \"", vm.toString(scheduleHash), "\", ");
        schedules = string.concat(
            schedules, bytes(schedules).length == 0 ? "" : ",", head, leaf, hash, announced[token][index], "}"
        );
    }

    function _onDeposit(Vm.Log memory log) internal {
        address depositor = address(uint160(uint256(log.topics[1])));
        address token = address(uint160(uint256(log.topics[2])));
        (, uint64 leafIndex, uint256 amount, bytes32 commitment) =
            abi.decode(log.data, (uint64, uint64, uint256, bytes32));
        string memory head = string.concat(
            "\n    {\"leaf_index\": ", vm.toString(uint256(leafIndex)), ", \"depositor\": \"", vm.toString(depositor)
        );
        string memory what = string.concat(
            "\", \"token\": \"",
            vm.toString(token),
            "\", \"amount\": \"",
            vm.toString(amount),
            "\",\n      \"commitment\": \""
        );
        string memory wallet = string.concat(
            vm.toString(commitment),
            "\", \"kind\": 2,\n      \"s\": \"",
            vm.toString(secretOf[commitment]),
            "\", \"dest\": \"",
            vm.toString(destOf[commitment]),
            "\"}"
        );
        deposits = string.concat(deposits, bytes(deposits).length == 0 ? "" : ",", head, what, wallet);
    }

    function _tokens() internal view returns (string memory out) {
        for (uint256 i = 0; i < registered.length; i++) {
            address token = registered[i];
            out = string.concat(
                out,
                i == 0 ? "" : ",",
                "\n    {\"token\": \"",
                vm.toString(token),
                "\", ",
                entryMeta[token],
                entryAmounts[token],
                "}"
            );
        }
    }

    function _list(uint256[] memory xs) internal pure returns (string memory out) {
        out = "[";
        for (uint256 i = 0; i < xs.length; i++) {
            out = string.concat(out, i == 0 ? "\"" : ", \"", vm.toString(xs[i]), "\"");
        }
        out = string.concat(out, "]");
    }
}
