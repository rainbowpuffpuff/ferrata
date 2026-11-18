// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {IEmergencyStop} from "../../src/interfaces/IEmergencyStop.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";
import {StandInCheckpointVerifier} from "../../src/standin/StandInCheckpointVerifier.sol";

/// @notice Deploys the TESTNET STAND-IN arrangement for the public LEZ testnet's return leg (src/standin/README.md):
///         the stand-in checkpoint verifier in front of RISC Zero's emergency stop, and an unchanged FerrataVault in
///         rule (B) whose `verifier` is that stand-in and whose `checkpointImageId` is the stand-in's id. ONE
///         TRANSACTION PER RUN, like script/DeployVault.s.sol, whose gas rule and record it follows; the loop is
///         e2e/testnet-return.sh's.
///
///         standIn      StandInCheckpointVerifier(riscZero, signers, threshold)
///         registry     the library FerrataRegistry (skipped when the parameters name a deployed one whose code is
///                      exactly this tree's library)
///         vault        FerrataVault linked to it; verifier = the stand-in, mode B, the release program's real id
///         registerETH  ETH with crown-live-3's denominations and fees
///         done         the checks; the record gets `complete: true`
///
///         Parameters: FERRATA_STANDIN_PARAMS, a JSON file under e2e/runs/ (e2e/testnet-return.sh plan writes it).
///         Record: FERRATA_OUT under deployments/. Gas: FERRATA_GAS_RULE and FERRATA_GAS_MARGIN_PCT as DeployVault.
///         Keys: none is read here; forge signs with ETH_KEYSTORE/ETH_PASSWORD, or --unlocked on a local fork.
///         Never for production: anvil, Sepolia, Hoodi and Holesky only (the stand-in's constructor refuses the rest).
contract DeployStandIn is Script {
    enum Step {
        StandIn,
        Registry,
        Vault,
        RegisterETH,
        Done
    }

    struct Record {
        address standIn;
        address registry;
        address vault;
        uint256[4] estimate;
        uint256[4] limit;
    }

    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;
    uint256 internal constant EIP7825_TX_GAS_CAP = 1 << 24;
    uint256 internal constant EIP8037_TX_TOTAL_GAS_CAP = (1 << 32) - 1;
    string internal constant LIBRARY_ID = "src/FerrataRegistry.sol:FerrataRegistry";
    string internal constant LIBRARY_ARTIFACT = "out/FerrataRegistry.sol/FerrataRegistry.json";
    string internal constant VAULT_ARTIFACT = "out/FerrataVault.sol/FerrataVault.json";

    address internal deployer;
    string internal params;
    string internal outPath;
    uint256 internal marginPct;
    string internal gasRule;
    uint256 internal gasCap;

    function run() external {
        _init();
        Record memory r = _read();
        address reused = vm.keyExistsJson(params, ".registry") ? vm.parseJsonAddress(params, ".registry") : address(0);
        if (r.registry == address(0) && reused != address(0)) {
            _checkLibraryCode(reused);
            r.registry = reused;
        }
        Step s = _next(r);
        console2.log(string.concat("FERRATA_STEP ", _name(s)));
        if (s == Step.StandIn) {
            address riscZero = vm.parseJsonAddress(params, ".standIn.riscZero");
            address[] memory signers = vm.parseJsonAddressArray(params, ".standIn.signers");
            uint256 threshold = vm.parseJsonUint(params, ".standIn.threshold");
            r.standIn = _create(
                r,
                s,
                abi.encodePacked(type(StandInCheckpointVerifier).creationCode, abi.encode(riscZero, signers, threshold))
            );
            _expect(".expect.standIn", r.standIn);
        } else if (s == Step.Registry) {
            r.registry = _create(r, s, vm.parseJsonBytes(vm.readFile(LIBRARY_ARTIFACT), ".bytecode.object"));
        } else if (s == Step.Vault) {
            _checkLibraryCode(r.registry);
            r.vault = _create(r, s, abi.encodePacked(_linkedVaultCode(r.registry), abi.encode(_deployment(r.standIn))));
            _expect(".expect.vault", r.vault);
        } else if (s == Step.RegisterETH) {
            _call(r, s, r.vault, abi.encodeCall(FerrataVault.registerToken, (ETH, _ethParams())));
        } else {
            _check(r);
            console2.log("FERRATA_COMPLETE");
        }
        _write(r, s);
    }

    // ---- configuration --------------------------------------------------------------------------------------------

    function _init() internal {
        uint256 id = block.chainid;
        require(
            id == 31_337 || id == 11_155_111 || id == 560_048 || id == 17_000,
            "DeployStandIn: a testnet stand-in: anvil, Sepolia, Hoodi or Holesky only"
        );
        deployer = msg.sender;
        require(
            deployer != DEFAULT_SENDER, "DeployStandIn: no deployer: --sender with a keystore, or --unlocked on a fork"
        );
        if (block.chainid != 31_337) {
            string memory mnemonic = "test test test test test test test test test test test junk";
            for (uint32 i = 0; i < 10; i++) {
                require(deployer != vm.addr(vm.deriveKey(mnemonic, i)), "DeployStandIn: a public anvil test account");
            }
        }
        params = vm.readFile(vm.envString("FERRATA_STANDIN_PARAMS"));
        require(vm.parseJsonUint(params, ".chainId") == block.chainid, "DeployStandIn: parameters of another chain");
        require(vm.parseJsonAddress(params, ".deployer") == deployer, "DeployStandIn: parameters of another deployer");
        outPath = vm.envString("FERRATA_OUT");
        marginPct = vm.envOr("FERRATA_GAS_MARGIN_PCT", uint256(125));
        require(marginPct >= 100 && marginPct <= 300, "DeployStandIn: FERRATA_GAS_MARGIN_PCT must be 100..300");
        gasRule = vm.envOr("FERRATA_GAS_RULE", string("eip7825"));
        if (_eq(gasRule, "eip7825")) {
            gasCap = EIP7825_TX_GAS_CAP;
        } else if (_eq(gasRule, "eip8037")) {
            gasCap = block.gaslimit < EIP8037_TX_TOTAL_GAS_CAP ? block.gaslimit : EIP8037_TX_TOTAL_GAS_CAP;
        } else {
            revert("DeployStandIn: FERRATA_GAS_RULE must be eip7825 or eip8037");
        }
    }

    function _freezeAuthority() internal pure returns (address) {
        // script/DeployVault.s.sol's default: an address nobody holds a key for.
        return address(uint160(uint256(keccak256("ferrata testnet: freeze authority (no key)"))));
    }

    function _unlimited() internal pure returns (FerrataVault.CapParams memory) {
        return FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0});
    }

    /// @dev crown-live-3's testnet configuration (script/DeployVault.s.sol under rule (B)) with three differences: the
    ///      verifier is the stand-in, the checkpoint id is the stand-in's, and the floors nMin / tMin are the stand-in
    ///      key set's size and threshold. Delta comes from the parameters.
    function _deployment(address standIn) internal view returns (FerrataVault.Deployment memory d) {
        StandInCheckpointVerifier v = StandInCheckpointVerifier(standIn);
        uint256 delta = vm.parseJsonUint(params, ".delta");
        d.verifier = IRiscZeroVerifier(standIn);
        d.checkpointImageId = v.STAND_IN_IMAGE_ID();
        d.releaseImageId = vm.parseJsonBytes32(params, ".releaseImageId");
        d.lezConfigId = vm.parseJsonBytes32(params, ".lezConfigId");
        d.genesisState = vm.parseJsonBytes32(params, ".genesisState");
        d.nMin = uint32(v.signers().length);
        d.tMin = uint32(v.threshold());
        d.deltaFloor = delta;
        d.initialDelta = delta;
        d.revokeWindow = vm.parseJsonUint(params, ".revokeWindow");
        d.maxFreeze = 30 days;
        d.turnSeconds = 0;
        d.minReleaseDelay = 0;
        d.admin = deployer;
        d.freezeAuthority = _freezeAuthority();
        d.globalDepositCap = _unlimited();
        d.globalReleaseCap = _unlimited();
        d.mode = FerrataVault.Mode.B;
        d.releaseCapFloor = FerrataVault.CapParams({capacity: 0, refillPerSecond: 0});
        d.feeRecipient = deployer;
    }

    function _three(uint256 a, uint256 b, uint256 c) internal pure returns (uint256[] memory d) {
        d = new uint256[](3);
        (d[0], d[1], d[2]) = (a, b, c);
    }

    /// @dev ETH as script/DeployVault.s.sol registers it: 0.001, 0.01, 0.1 ETH; relay fee 0.2% and checkpoint fee 0.1%.
    function _ethParams() internal view returns (FerrataVault.TokenParams memory) {
        return FerrataVault.TokenParams({
            denominations: _three(0.001 ether, 0.01 ether, 0.1 ether),
            relayFees: _three(2e12, 2e13, 2e14),
            lezUnit: 1e12,
            unitWeight: 3000,
            depositCap: _unlimited(),
            releaseCap: _unlimited(),
            releaseDelay: vm.parseJsonUint(params, ".releaseDelay"),
            lezDefinition: vm.parseJsonBytes32(params, ".ethLezDefinition"),
            decimals: 18,
            checkpointFees: _three(1e12, 1e13, 1e14)
        });
    }

    // ---- steps ----------------------------------------------------------------------------------------------------

    function _next(Record memory r) internal view returns (Step) {
        if (r.standIn == address(0)) return Step.StandIn;
        if (r.registry == address(0)) return Step.Registry;
        if (r.vault == address(0)) return Step.Vault;
        (bool registered,,,,) = FerrataVault(payable(r.vault)).tokenConfig(ETH);
        if (!registered) return Step.RegisterETH;
        return Step.Done;
    }

    /// @dev The addresses the plan computed from the deployer's nonce: the release proofs and the LEZ configuration
    ///      bind the vault's, the attestations the stand-in's.
    function _expect(string memory key, address got) internal view {
        if (vm.keyExistsJson(params, key)) {
            require(vm.parseJsonAddress(params, key) == got, string.concat("DeployStandIn: not the planned ", key));
        }
    }

    function _gas(Step s, address to, bytes memory data) internal returns (uint256 estimate, uint256 limit) {
        string memory call = string.concat(
            '{"from":"',
            vm.toString(deployer),
            '",',
            to == address(0) ? "" : string.concat('"to":"', vm.toString(to), '",'),
            '"data":"',
            vm.toString(data),
            '"}'
        );
        estimate = _quantity(vm.rpc("eth_estimateGas", string.concat("[", call, ',"latest"]')));
        limit = estimate * marginPct / 100;
        console2.log(
            string.concat(
                "FERRATA_GAS ",
                _name(s),
                " estimate ",
                vm.toString(estimate),
                " limit ",
                vm.toString(limit),
                " cap ",
                vm.toString(gasCap)
            )
        );
        require(limit <= gasCap, string.concat("DeployStandIn: ", _name(s), ": above the gas cap; nothing sent"));
    }

    function _create(Record memory r, Step s, bytes memory initcode) internal returns (address created) {
        (r.estimate[uint256(s)], r.limit[uint256(s)]) = _gas(s, address(0), initcode);
        vm.broadcast(deployer);
        assembly ("memory-safe") {
            created := create(0, add(initcode, 0x20), mload(initcode))
        }
        require(created != address(0) && created.code.length > 0, string.concat("DeployStandIn: ", _name(s), " failed"));
        console2.log(string.concat("FERRATA_CREATED ", _name(s), " ", vm.toString(created)));
    }

    function _call(Record memory r, Step s, address to, bytes memory data) internal {
        (r.estimate[uint256(s)], r.limit[uint256(s)]) = _gas(s, to, data);
        vm.broadcast(deployer);
        (bool ok, bytes memory returned) = to.call(data);
        if (!ok) {
            assembly ("memory-safe") {
                revert(add(returned, 0x20), mload(returned))
            }
        }
    }

    /// @dev As script/DeployVault.s.sol: the vault's creation code with every call site of the library linked.
    function _linkedVaultCode(address registry) internal view returns (bytes memory) {
        string memory code = vm.parseJsonString(vm.readFile(VAULT_ARTIFACT), ".bytecode.object");
        string memory placeholder = _placeholder();
        require(
            vm.indexOf(code, placeholder) != type(uint256).max, "DeployStandIn: the vault does not link the library"
        );
        code = vm.replace(code, placeholder, vm.replace(vm.toString(registry), "0x", ""));
        require(vm.indexOf(code, "__$") == type(uint256).max, "DeployStandIn: the vault links another library");
        return vm.parseBytes(code);
    }

    /// @dev The address must hold exactly this tree's FerrataRegistry (the artifact's runtime code with solc's call
    ///      protection, PUSH20 <its own address>): the vault DELEGATECALLs it.
    function _checkLibraryCode(address registry) internal view {
        bytes memory expected = vm.parseJsonBytes(vm.readFile(LIBRARY_ARTIFACT), ".deployedBytecode.object");
        require(expected.length > 21 && uint8(expected[0]) == 0x73, "DeployStandIn: the library has no call guard");
        bytes20 self = bytes20(registry);
        for (uint256 i = 0; i < 20; i++) {
            expected[1 + i] = self[i];
        }
        require(keccak256(registry.code) == keccak256(expected), "DeployStandIn: not this tree's FerrataRegistry");
    }

    function _placeholder() internal pure returns (string memory) {
        bytes32 h = keccak256(bytes(LIBRARY_ID));
        bytes16 digits = "0123456789abcdef";
        bytes memory p = new bytes(40);
        p[0] = "_";
        p[1] = "_";
        p[2] = "$";
        p[37] = "$";
        p[38] = "_";
        p[39] = "_";
        for (uint256 i = 0; i < 17; i++) {
            p[3 + 2 * i] = digits[uint8(h[i]) >> 4];
            p[4 + 2 * i] = digits[uint8(h[i]) & 15];
        }
        return string(p);
    }

    /// @dev What the arrangement claims, read back from the chain.
    function _check(Record memory r) internal view {
        StandInCheckpointVerifier v = StandInCheckpointVerifier(r.standIn);
        FerrataVault vault = FerrataVault(payable(r.vault));
        address riscZero = vm.parseJsonAddress(params, ".standIn.riscZero");
        require(address(v.riscZero()) == riscZero, "check: the stand-in forwards elsewhere");
        require(!IEmergencyStop(riscZero).paused() && !v.paused(), "check: RISC Zero's stop is tripped");
        address[] memory want = vm.parseJsonAddressArray(params, ".standIn.signers");
        address[] memory got = v.signers();
        require(got.length == want.length, "check: signer count");
        for (uint256 i = 0; i < got.length; i++) {
            require(got[i] == want[i], "check: signer");
        }
        require(v.threshold() == vm.parseJsonUint(params, ".standIn.threshold"), "check: threshold");
        require(address(vault.verifier()) == r.standIn, "check: the vault's verifier is not the stand-in");
        require(vault.checkpointImageId() == v.STAND_IN_IMAGE_ID(), "check: checkpoint id");
        require(vault.releaseImageId() == vm.parseJsonBytes32(params, ".releaseImageId"), "check: release id");
        require(vault.lezConfigId() == vm.parseJsonBytes32(params, ".lezConfigId"), "check: lezConfigId");
        require(vault.storedLezConfigId() == vault.lezConfigId(), "check: slot 2");
        require(vault.genesisState() == vm.parseJsonBytes32(params, ".genesisState"), "check: genesisState");
        require(vault.mode() == FerrataVault.Mode.B, "check: rule (B)");
        require(vault.nMin() == got.length && vault.tMin() == v.threshold(), "check: floors");
        require(vault.delta() == vm.parseJsonUint(params, ".delta"), "check: delta");
        require(vault.admin() == deployer, "check: admin");
        (bool registered, bool enabled,,, uint256[] memory denoms) = vault.tokenConfig(ETH);
        require(registered && enabled && denoms.length == 3 && denoms[0] == 0.001 ether, "check: ETH registration");
        require(vault.releaseDelayFor(ETH, 0) == vm.parseJsonUint(params, ".releaseDelay"), "check: release delay");
    }

    // ---- the record -----------------------------------------------------------------------------------------------

    function _read() internal view returns (Record memory r) {
        if (!vm.exists(outPath)) return r;
        string memory j = vm.readFile(outPath);
        require(vm.parseJsonUint(j, ".chainId") == block.chainid, "DeployStandIn: the record is of another chain");
        require(vm.parseJsonAddress(j, ".deployer") == deployer, "DeployStandIn: the record is of another deployer");
        r.standIn = _onChain(j, ".standIn");
        r.registry = _onChain(j, ".registry");
        r.vault = _onChain(j, ".vault");
        for (uint256 i = 0; i < 4; i++) {
            string memory key = string.concat(".gas.", _name(Step(i)));
            if (vm.keyExistsJson(j, key)) {
                r.estimate[i] = vm.parseJsonUint(j, string.concat(key, ".estimate"));
                r.limit[i] = vm.parseJsonUint(j, string.concat(key, ".limit"));
            }
        }
    }

    function _onChain(string memory j, string memory key) internal view returns (address a) {
        if (!vm.keyExistsJson(j, key)) return address(0);
        a = vm.parseJsonAddress(j, key);
        if (a.code.length == 0) a = address(0);
    }

    function _write(Record memory r, Step s) internal {
        string memory o = "record";
        vm.serializeString(o, "what", "TESTNET STAND-IN deployment (vault/src/standin/README.md); not Ferrata's design");
        vm.serializeUint(o, "chainId", block.chainid);
        vm.serializeString(o, "mode", "B");
        vm.serializeAddress(o, "deployer", deployer);
        vm.serializeAddress(o, "freezeAuthority", _freezeAuthority());
        vm.serializeString(o, "gasRule", gasRule);
        vm.serializeUint(o, "gasMarginPct", marginPct);
        vm.serializeAddress(o, "riscZero", vm.parseJsonAddress(params, ".standIn.riscZero"));
        vm.serializeAddress(o, "standIn", r.standIn);
        vm.serializeAddress(o, "registry", r.registry);
        vm.serializeAddress(o, "vault", r.vault);
        string memory gas;
        for (uint256 i = 0; i < 4; i++) {
            if (r.estimate[i] == 0) continue;
            string memory name = _name(Step(i));
            vm.serializeUint(name, "estimate", r.estimate[i]);
            gas = vm.serializeString("gas", name, vm.serializeUint(name, "limit", r.limit[i]));
        }
        if (bytes(gas).length != 0) vm.serializeString(o, "gas", gas);
        vm.serializeString(o, "lastStep", _name(s));
        vm.writeJson(vm.serializeBool(o, "complete", s == Step.Done), outPath);
    }

    // ---- helpers --------------------------------------------------------------------------------------------------

    function _name(Step s) internal pure returns (string memory) {
        if (s == Step.StandIn) return "standIn";
        if (s == Step.Registry) return "registry";
        if (s == Step.Vault) return "vault";
        if (s == Step.RegisterETH) return "registerETH";
        return "done";
    }

    function _quantity(bytes memory b) internal pure returns (uint256 v) {
        require(b.length <= 32, "DeployStandIn: the node's estimate is not a quantity");
        for (uint256 i = 0; i < b.length; i++) {
            v = (v << 8) | uint8(b[i]);
        }
    }

    function _eq(string memory a, string memory b) internal pure returns (bool) {
        return keccak256(bytes(a)) == keccak256(bytes(b));
    }
}
