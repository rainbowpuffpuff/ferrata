// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {TestToken} from "./TestToken.sol";

/// @notice Deploys a test FerrataVault under rule (A) or (B) with its linked library FerrataRegistry, ONE TRANSACTION
///         PER RUN: each run reads the record (FERRATA_OUT), checks on chain what is already there, and does the next
///         step. script/deploy.sh runs it until the record says `complete`.
///
///         groth16Verifier    RISC Zero's Groth16 verifier (skipped if FERRATA_GROTH16_VERIFIER names one)
///         emergencyStop      RiscZeroVerifierEmergencyStop around it, owned by the deployer until the last step
///         registry           the library FerrataRegistry
///         vault              FerrataVault, its bytecode linked to that library (never `new`: forge would deploy a
///                            library of its own)
///         token              a test ERC-20 (TestToken, open minting)
///         registerETH        ETH, with relay and checkpoint fees (schedule 0; mint and burn fees start at zero)
///         registerToken      the test token, the same
///         renounceStopOwner  the stop's owner renounced (proposal §B5: nobody holds the guardian key)
///         done               the checks below; the record gets `complete: true`
///
///         Gas: before it sends, a run asks the node (eth_estimateGas, against the chain as the previous runs left it)
///         and refuses unless estimate x FERRATA_GAS_MARGIN_PCT / 100 is within the cap of FERRATA_GAS_RULE:
///           eip7825  the limit is at most 2^24 (EIP-7825);
///           eip8037  the limit is at most min(2^32 - 1, block gas limit) (EIP-8037: the 2^24 cap applies to
///                    execution gas only, and the node's estimate fails if that part exceeds it). Sepolia since
///                    6 Oct 2026 (Glamsterdam): the vault's creation needs about 39.5 M gas, the library's 18.3 M.
///         FERRATA_TX_GAS_CAP, if lower, caps it further.
///         forge sends it with --skip-simulation --gas-estimate-multiplier = the same margin, so the limit it uses
///         is the node's estimate times the margin (script/deploy.sh passes both).
///
///         Caps (RFP-022 Sec.4, F.11; docs/guides/deployer-guide.md §2): capped by default. Per token and per day,
///         deposits and releases each at most a ceiling (the params' `ethDailyCap` and `tokenDailyCap`, in base units;
///         by default 100 times the token's largest denomination: 10 ETH and 1,000,000 test-token units a day);
///         globally, deposits and releases each at most the params' `globalDailyValue` in value units (by default the
///         sum of the two ceilings' values); and an immutable release floor (`releaseFloorDailyValue`, by default one
///         largest denomination of the token whose largest is worth least, a day), so that no admin can throttle
///         releases below it. Each bucket refills its capacity over a day. FERRATA_UNCAPPED=1 deploys without caps, as
///         the live testnet runs of November 2026 did; the record says which (`uncapped`).
///
///         Keys: none is read here. forge signs with Foundry's own keystore variables (ETH_KEYSTORE, ETH_PASSWORD:
///         files) or, on a local fork, with --unlocked. Refused: chain 1, forge's default sender (no key given), and,
///         except on chain 31337, the public anvil test accounts.
contract DeployVault is Script {
    enum Step {
        Groth16Verifier,
        EmergencyStop,
        Registry,
        Vault,
        Token,
        RegisterETH,
        RegisterToken,
        RenounceStopOwner,
        Done
    }

    struct Record {
        address groth16;
        address stop;
        address registry;
        address vault;
        address token;
        uint256[8] estimate; // per step that sent a transaction: the node's estimate
        uint256[8] limit; // and the gas limit it was sent with
    }

    address internal constant ETH = address(0);
    uint256 internal constant UNLIMITED = type(uint256).max;
    uint256 internal constant EIP7825_TX_GAS_CAP = 1 << 24;
    uint256 internal constant EIP8037_TX_TOTAL_GAS_CAP = (1 << 32) - 1;
    string internal constant LIBRARY_ID = "src/FerrataRegistry.sol:FerrataRegistry";
    string internal constant LIBRARY_ARTIFACT = "out/FerrataRegistry.sol/FerrataRegistry.json";
    string internal constant VAULT_ARTIFACT = "out/FerrataVault.sol/FerrataVault.json";

    string internal modeName;
    FerrataVault.Mode internal mode;
    address internal deployer;
    string internal params;
    string internal outPath;
    uint256 internal marginPct;
    string internal gasRule;
    uint256 internal gasCap;
    bool internal uncapped;

    function run() external {
        _init();
        Record memory r = _read();
        address reused = vm.envOr("FERRATA_GROTH16_VERIFIER", address(0));
        if (reused != address(0)) {
            require(reused.code.length > 0, "DeployVault: FERRATA_GROTH16_VERIFIER has no code");
            r.groth16 = reused;
        }
        Step s = _next(r);
        console2.log(string.concat("FERRATA_STEP ", _name(s)));
        if (s == Step.Groth16Verifier) {
            r.groth16 = _create(
                r,
                s,
                abi.encodePacked(
                    type(RiscZeroGroth16Verifier).creationCode,
                    abi.encode(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID)
                )
            );
        } else if (s == Step.EmergencyStop) {
            r.stop = _create(
                r,
                s,
                abi.encodePacked(type(RiscZeroVerifierEmergencyStop).creationCode, abi.encode(r.groth16, deployer))
            );
        } else if (s == Step.Registry) {
            r.registry = _create(r, s, vm.parseJsonBytes(vm.readFile(LIBRARY_ARTIFACT), ".bytecode.object"));
        } else if (s == Step.Vault) {
            require(
                address(RiscZeroVerifierEmergencyStop(r.stop).verifier()) == r.groth16,
                "DeployVault: the recorded stop wraps another verifier"
            );
            _checkLibraryCode(r.registry);
            r.vault = _create(r, s, abi.encodePacked(_linkedVaultCode(r.registry), abi.encode(_deployment(r.stop))));
        } else if (s == Step.Token) {
            r.token = _create(r, s, type(TestToken).creationCode);
        } else if (s == Step.RegisterETH) {
            _warnIfNoLezDefinition(_ethParams());
            _call(r, s, r.vault, abi.encodeCall(FerrataVault.registerToken, (ETH, _ethParams())));
        } else if (s == Step.RegisterToken) {
            _warnIfNoLezDefinition(_tokenParams());
            _call(r, s, r.vault, abi.encodeCall(FerrataVault.registerToken, (r.token, _tokenParams())));
        } else if (s == Step.RenounceStopOwner) {
            _call(r, s, r.stop, abi.encodeWithSignature("renounceOwnership()"));
        } else {
            _check(r);
            console2.log("FERRATA_COMPLETE");
        }
        _write(r, s);
    }

    // ---- configuration --------------------------------------------------------------------------------------------

    function _init() internal {
        // A test deployment (open-minting token, short timers, single-key committees allowed): testnets only, not
        // chain 1 nor any other mainnet (review I-2).
        uint256 id = block.chainid;
        require(
            id == 31_337 || id == 11_155_111 || id == 560_048 || id == 17_000,
            "DeployVault: a test deployment: anvil, Sepolia, Hoodi or Holesky only"
        );
        deployer = msg.sender;
        require(
            deployer != DEFAULT_SENDER, "DeployVault: no deployer: --sender with a keystore, or --unlocked on a fork"
        );
        if (block.chainid != 31_337) {
            // The anvil accounts' keys are public: compare addresses only.
            string memory mnemonic = "test test test test test test test test test test test junk";
            for (uint32 i = 0; i < 10; i++) {
                require(deployer != vm.addr(vm.deriveKey(mnemonic, i)), "DeployVault: a public anvil test account");
            }
        }
        modeName = vm.envString("FERRATA_MODE");
        if (_eq(modeName, "A")) mode = FerrataVault.Mode.A;
        else if (_eq(modeName, "B")) mode = FerrataVault.Mode.B;
        else revert("DeployVault: FERRATA_MODE must be A or B");
        params = vm.readFile(vm.envOr("FERRATA_PARAMS", string("script/params/sepolia.json")));
        uncapped = vm.envOr("FERRATA_UNCAPPED", false);
        if (uncapped) console2.log("warning: FERRATA_UNCAPPED=1: no deposit or release caps (a test deployment only)");
        outPath =
            vm.envOr("FERRATA_OUT", string.concat("deployments/", vm.toString(block.chainid), "-", modeName, ".json"));
        marginPct = vm.envOr("FERRATA_GAS_MARGIN_PCT", uint256(125));
        require(marginPct >= 100 && marginPct <= 300, "DeployVault: FERRATA_GAS_MARGIN_PCT must be 100..300");
        gasRule = vm.envOr("FERRATA_GAS_RULE", string("eip7825"));
        if (_eq(gasRule, "eip7825")) {
            gasCap = EIP7825_TX_GAS_CAP;
        } else if (_eq(gasRule, "eip8037")) {
            gasCap = block.gaslimit < EIP8037_TX_TOTAL_GAS_CAP ? block.gaslimit : EIP8037_TX_TOTAL_GAS_CAP;
        } else {
            revert("DeployVault: FERRATA_GAS_RULE must be eip7825 or eip8037");
        }
        // An operator's own lower cap, if any (the fork rehearsal uses it to see a refusal).
        uint256 ownCap = vm.envOr("FERRATA_TX_GAS_CAP", type(uint256).max);
        if (ownCap < gasCap) gasCap = ownCap;
        console2.log(
            string.concat(
                "deployer ",
                vm.toString(deployer),
                ", rule ",
                modeName,
                ", gas rule ",
                gasRule,
                ", cap ",
                vm.toString(gasCap),
                ", block gas limit ",
                vm.toString(block.gaslimit)
            )
        );
    }

    function _freezeAuthority() internal view returns (address) {
        return vm.envOr(
            "FERRATA_FREEZE_AUTHORITY",
            address(uint160(uint256(keccak256("ferrata testnet: freeze authority (no key)"))))
        );
    }

    function _watchers() internal view returns (address[3] memory w) {
        address[] memory given = vm.envOr("FERRATA_WATCHERS", ",", new address[](0));
        require(given.length <= 3, "DeployVault: at most three watchers");
        for (uint256 i = 0; i < given.length; i++) {
            w[i] = given[i];
        }
    }

    function _unlimited() internal pure returns (FerrataVault.CapParams memory) {
        return FerrataVault.CapParams({capacity: UNLIMITED, refillPerSecond: 0});
    }

    /// @dev A bucket of `perDay` that refills over a day (the refill rounded up, so a full day refills it whole).
    function _daily(uint256 perDay) internal pure returns (FerrataVault.CapParams memory) {
        return FerrataVault.CapParams({capacity: perDay, refillPerSecond: (perDay + 1 days - 1) / 1 days});
    }

    /// @dev `key` of the params if present, else `fallbackValue`.
    function _paramOr(string memory key, uint256 fallbackValue) internal view returns (uint256) {
        return vm.keyExistsJson(params, key) ? vm.parseJsonUint(params, key) : fallbackValue;
    }

    /// @dev A token's daily ceiling, in its base units: the params' `key`, else 100 times its largest denomination.
    function _dailyCeiling(string memory key, uint256[] memory denoms) internal view returns (uint256) {
        return _paramOr(key, 100 * denoms[denoms.length - 1]);
    }

    /// @dev `amount` in value units, as the vault counts the global caps: amount / lezUnit * unitWeight.
    function _value(uint256 amount, uint256 lezUnit, uint256 unitWeight) internal pure returns (uint256) {
        return amount / lezUnit * unitWeight;
    }

    /// @dev The global daily ceiling in value units: the params' `globalDailyValue`, else the sum of both tokens'.
    function _globalDaily() internal view returns (uint256) {
        FerrataVault.TokenParams memory e = _ethParams();
        FerrataVault.TokenParams memory t = _tokenParams();
        return _paramOr(
            ".globalDailyValue",
            _value(e.depositCap.capacity, e.lezUnit, e.unitWeight) + _value(t.depositCap.capacity, t.lezUnit, t.unitWeight)
        );
    }

    /// @dev The immutable release floor in value units a day: the params' `releaseFloorDailyValue`, else one largest
    ///      denomination of the token whose largest is worth least.
    function _releaseFloorDaily() internal view returns (uint256) {
        FerrataVault.TokenParams memory e = _ethParams();
        FerrataVault.TokenParams memory t = _tokenParams();
        uint256 ethLargest = _value(e.denominations[e.denominations.length - 1], e.lezUnit, e.unitWeight);
        uint256 tokenLargest = _value(t.denominations[t.denominations.length - 1], t.lezUnit, t.unitWeight);
        return _paramOr(".releaseFloorDailyValue", ethLargest < tokenLargest ? ethLargest : tokenLargest);
    }

    /// @dev The testnet configuration of script/RoundTrip.s.sol, under either rule. Rule (B) has no turn term.
    function _deployment(address stop) internal view returns (FerrataVault.Deployment memory d) {
        bool ruleA = mode == FerrataVault.Mode.A;
        uint256 delta = vm.envOr("FERRATA_DELTA", uint256(600));
        d.verifier = IRiscZeroVerifier(stop);
        d.checkpointImageId = vm.parseJsonBytes32(params, ruleA ? ".checkpointImageIdA" : ".checkpointImageIdB");
        d.releaseImageId = vm.parseJsonBytes32(params, ".releaseImageId");
        d.lezConfigId = vm.parseJsonBytes32(params, ".lezConfigId");
        d.genesisState = vm.parseJsonBytes32(params, ".genesisState");
        d.nMin = uint32(vm.parseJsonUint(params, ".nMin"));
        d.tMin = uint32(vm.parseJsonUint(params, ".tMin"));
        d.deltaFloor = delta;
        d.initialDelta = delta;
        d.revokeWindow = vm.envOr("FERRATA_REVOKE_WINDOW", uint256(1 days));
        d.maxFreeze = vm.envOr("FERRATA_MAX_FREEZE", uint256(30 days));
        d.turnSeconds = ruleA ? vm.parseJsonUint(params, ".postingTimeframe") : 0;
        d.minReleaseDelay = vm.envOr("FERRATA_MIN_RELEASE_DELAY", uint256(0));
        d.admin = deployer;
        d.freezeAuthority = _freezeAuthority();
        d.globalDepositCap = uncapped ? _unlimited() : _daily(_globalDaily());
        d.globalReleaseCap = uncapped ? _unlimited() : _daily(_globalDaily());
        d.mode = mode;
        d.watchers = _watchers();
        d.releaseCapFloor = uncapped
            ? FerrataVault.CapParams({capacity: 0, refillPerSecond: 0})
            : FerrataVault.CapParams({capacity: _releaseFloorDaily(), refillPerSecond: _releaseFloorDaily() / 1 days});
        d.feeRecipient = vm.envOr("FERRATA_FEE_RECIPIENT", deployer);
    }

    function _three(uint256 a, uint256 b, uint256 c) internal pure returns (uint256[] memory d) {
        d = new uint256[](3);
        (d[0], d[1], d[2]) = (a, b, c);
    }

    function _token(
        uint256[] memory denoms,
        uint256[] memory relayFees,
        uint256[] memory checkpointFees,
        uint256 lezUnit,
        uint256 unitWeight,
        uint8 decimals,
        bytes32 lezDefinition,
        string memory ceilingKey
    ) internal view returns (FerrataVault.TokenParams memory) {
        FerrataVault.CapParams memory cap = uncapped ? _unlimited() : _daily(_dailyCeiling(ceilingKey, denoms));
        return FerrataVault.TokenParams({
            denominations: denoms,
            relayFees: relayFees,
            lezUnit: lezUnit,
            unitWeight: unitWeight,
            depositCap: cap,
            releaseCap: cap,
            releaseDelay: vm.envOr("FERRATA_RELEASE_DELAY", uint256(120)),
            lezDefinition: lezDefinition,
            decimals: decimals,
            checkpointFees: checkpointFees
        });
    }

    /// @dev ETH: 0.001, 0.01, 0.1 ETH; relay fee 0.2% and checkpoint fee 0.1% of each; LEZ unit 10^12 wei.
    function _ethParams() internal view returns (FerrataVault.TokenParams memory) {
        return _token(
            _three(0.001 ether, 0.01 ether, 0.1 ether),
            _three(2e12, 2e13, 2e14),
            _three(1e12, 1e13, 1e14),
            1e12,
            3000,
            18,
            vm.parseJsonBytes32(params, ".ethLezDefinition"),
            ".ethDailyCap"
        );
    }

    /// @dev The test token (6 decimals): 100, 1,000, 10,000; relay fee 0.25% and checkpoint fee 0.1% of each.
    function _tokenParams() internal view returns (FerrataVault.TokenParams memory) {
        return _token(
            _three(100e6, 1000e6, 10_000e6),
            _three(250_000, 2_500_000, 25_000_000),
            _three(100_000, 1_000_000, 10_000_000),
            1,
            1,
            6,
            vm.parseJsonBytes32(params, ".tokenLezDefinition"),
            ".tokenDailyCap"
        );
    }

    function _warnIfNoLezDefinition(FerrataVault.TokenParams memory p) internal pure {
        if (p.lezDefinition == bytes32(0)) {
            console2.log("warning: a zero LEZ definition (registration is write-once; set it in FERRATA_PARAMS)");
        }
    }

    // ---- steps ----------------------------------------------------------------------------------------------------

    function _next(Record memory r) internal view returns (Step) {
        if (r.groth16 == address(0)) return Step.Groth16Verifier;
        if (r.stop == address(0)) return Step.EmergencyStop;
        if (r.registry == address(0)) return Step.Registry;
        if (r.vault == address(0)) return Step.Vault;
        if (r.token == address(0)) return Step.Token;
        if (!_registered(r.vault, ETH)) return Step.RegisterETH;
        if (!_registered(r.vault, r.token)) return Step.RegisterToken;
        if (RiscZeroVerifierEmergencyStop(r.stop).owner() != address(0)) return Step.RenounceStopOwner;
        return Step.Done;
    }

    function _registered(address vault, address token) internal view returns (bool registered) {
        (registered,,,,) = FerrataVault(payable(vault)).tokenConfig(token);
    }

    /// @dev The node's estimate for this transaction now, and the limit forge will send it with; refuses above the cap.
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
        require(
            limit <= gasCap,
            string.concat(
                "DeployVault: ",
                _name(s),
                ": the node's estimate x margin, ",
                vm.toString(limit),
                ", is above the cap ",
                vm.toString(gasCap),
                " of ",
                gasRule,
                "; nothing sent"
            )
        );
    }

    function _create(Record memory r, Step s, bytes memory initcode) internal returns (address created) {
        (r.estimate[uint256(s)], r.limit[uint256(s)]) = _gas(s, address(0), initcode);
        vm.broadcast(deployer);
        assembly ("memory-safe") {
            created := create(0, add(initcode, 0x20), mload(initcode))
        }
        require(created != address(0) && created.code.length > 0, string.concat("DeployVault: ", _name(s), " failed"));
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

    /// @dev The vault's creation code with every call site of the library linked to `registry`. The placeholder is
    ///      solc's: "__$" + the first 17 bytes of keccak256("<source path>:<library>") in hex + "$__".
    function _linkedVaultCode(address registry) internal view returns (bytes memory) {
        string memory code = vm.parseJsonString(vm.readFile(VAULT_ARTIFACT), ".bytecode.object");
        string memory placeholder = _placeholder();
        require(vm.indexOf(code, placeholder) != type(uint256).max, "DeployVault: the vault does not link the library");
        code = vm.replace(code, placeholder, vm.replace(vm.toString(registry), "0x", ""));
        require(vm.indexOf(code, "__$") == type(uint256).max, "DeployVault: the vault links another library");
        return vm.parseBytes(code);
    }

    /// @dev The vault DELEGATECALLs its library, so the recorded address must hold exactly FerrataRegistry's runtime code:
    ///      the artifact's, with solc's call protection (PUSH20 <the library's own address>, written at deployment).
    function _checkLibraryCode(address registry) internal view {
        bytes memory expected = vm.parseJsonBytes(vm.readFile(LIBRARY_ARTIFACT), ".deployedBytecode.object");
        require(
            expected.length > 21 && uint8(expected[0]) == 0x73, "DeployVault: the library artifact has no call guard"
        );
        bytes20 self = bytes20(registry);
        for (uint256 i = 0; i < 20; i++) {
            expected[1 + i] = self[i];
        }
        require(
            keccak256(registry.code) == keccak256(expected), "DeployVault: the recorded library is not FerrataRegistry"
        );
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

    /// @dev Everything the proposal and the registrations promise, read back from the chain.
    function _check(Record memory r) internal view {
        RiscZeroVerifierEmergencyStop stop = RiscZeroVerifierEmergencyStop(r.stop);
        FerrataVault vault = FerrataVault(payable(r.vault));
        require(stop.owner() == address(0), "check: the stop still has an owner");
        require(!stop.paused(), "check: the stop is tripped");
        require(address(stop.verifier()) == r.groth16, "check: the stop wraps another verifier");
        require(address(vault.verifier()) == r.stop, "check: the vault's verifier is not the stop");
        require(vault.mode() == mode, "check: the vault's rule");
        require(vault.admin() == deployer, "check: admin");
        require(vault.freezeAuthority() == _freezeAuthority(), "check: freeze authority");
        _checkToken(vault, ETH, _ethParams());
        _checkToken(vault, r.token, _tokenParams());
        _checkCaps(vault, r.token);
    }

    /// @dev The caps the deployment chose, read back: finite unless FERRATA_UNCAPPED=1, and the floor immutable.
    function _checkCaps(FerrataVault vault, address token) internal view {
        FerrataVault.Deployment memory d = _deployment(address(0));
        for (uint256 i = 0; i < 2; i++) {
            FerrataVault.Direction dir = FerrataVault.Direction(i);
            (uint256 capacity, uint256 refill,) = vault.globalCap(dir);
            FerrataVault.CapParams memory want = i == 0 ? d.globalDepositCap : d.globalReleaseCap;
            require(capacity == want.capacity && refill == want.refillPerSecond, "check: global cap");
            (capacity, refill,) = vault.tokenCap(ETH, dir);
            require(capacity == _ethParams().depositCap.capacity && refill == _ethParams().depositCap.refillPerSecond, "check: ETH cap");
            (capacity, refill,) = vault.tokenCap(token, dir);
            require(capacity == _tokenParams().depositCap.capacity, "check: token cap");
            require(uncapped == (capacity == UNLIMITED), "check: capped unless FERRATA_UNCAPPED=1");
        }
        require(vault.releaseCapFloor() == d.releaseCapFloor.capacity, "check: release floor");
    }

    function _checkToken(FerrataVault vault, address token, FerrataVault.TokenParams memory p) internal view {
        (bool registered, bool enabled,,, uint256[] memory denoms) = vault.tokenConfig(token);
        require(registered && enabled && denoms.length == p.denominations.length, "check: registration");
        for (uint256 i = 0; i < denoms.length; i++) {
            (uint256 mintFee, uint256 relayFee, uint256 burnFee, uint256 checkpointFee) = vault.fees(token, 0, uint8(i));
            require(denoms[i] == p.denominations[i], "check: denomination");
            require(mintFee == 0 && burnFee == 0, "check: protocol fees start at zero");
            require(relayFee == p.relayFees[i] && checkpointFee == p.checkpointFees[i], "check: schedule 0");
        }
        require(
            vm.load(address(vault), sha256(abi.encodePacked("ferrata/vault/registry/v1", token))) != 0,
            "check: registry entry not published"
        );
        require(
            vm.load(address(vault), sha256(abi.encodePacked("ferrata/vault/fees/v1", token, uint64(0)))) != 0,
            "check: fee schedule 0 not published"
        );
    }

    // ---- the record -----------------------------------------------------------------------------------------------

    function _read() internal view returns (Record memory r) {
        if (!vm.exists(outPath)) return r;
        string memory j = vm.readFile(outPath);
        require(vm.parseJsonUint(j, ".chainId") == block.chainid, "DeployVault: the record is of another chain");
        require(_eq(vm.parseJsonString(j, ".mode"), modeName), "DeployVault: the record is of the other rule");
        require(vm.parseJsonAddress(j, ".deployer") == deployer, "DeployVault: the record is of another deployer");
        r.groth16 = _onChain(j, ".groth16Verifier");
        r.stop = _onChain(j, ".emergencyStop");
        r.registry = _onChain(j, ".registry");
        r.vault = _onChain(j, ".vault");
        r.token = _onChain(j, ".token");
        for (uint256 i = 0; i < 8; i++) {
            string memory key = string.concat(".gas.", _name(Step(i)));
            if (vm.keyExistsJson(j, key)) {
                r.estimate[i] = vm.parseJsonUint(j, string.concat(key, ".estimate"));
                r.limit[i] = vm.parseJsonUint(j, string.concat(key, ".limit"));
            }
        }
    }

    /// @dev A recorded address counts only if it has code (a creation that failed is done again).
    function _onChain(string memory j, string memory key) internal view returns (address a) {
        if (!vm.keyExistsJson(j, key)) return address(0);
        a = vm.parseJsonAddress(j, key);
        if (a.code.length == 0) a = address(0);
    }

    function _write(Record memory r, Step s) internal {
        string memory o = "record";
        vm.serializeUint(o, "chainId", block.chainid);
        vm.serializeString(o, "mode", modeName);
        vm.serializeAddress(o, "deployer", deployer);
        vm.serializeAddress(o, "freezeAuthority", _freezeAuthority());
        vm.serializeAddress(o, "feeRecipient", vm.envOr("FERRATA_FEE_RECIPIENT", deployer));
        vm.serializeString(o, "gasRule", gasRule);
        vm.serializeBool(o, "uncapped", uncapped);
        if (!uncapped) {
            vm.serializeUint(o, "ethDailyCap", _ethParams().depositCap.capacity);
            vm.serializeUint(o, "tokenDailyCap", _tokenParams().depositCap.capacity);
            vm.serializeUint(o, "globalDailyValue", _globalDaily());
            vm.serializeUint(o, "releaseFloorDailyValue", _releaseFloorDaily());
        }
        vm.serializeUint(o, "gasMarginPct", marginPct);
        vm.serializeAddress(o, "groth16Verifier", r.groth16);
        vm.serializeAddress(o, "emergencyStop", r.stop);
        vm.serializeAddress(o, "registry", r.registry);
        vm.serializeAddress(o, "vault", r.vault);
        vm.serializeAddress(o, "token", r.token);
        string memory gas;
        for (uint256 i = 0; i < 8; i++) {
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
        if (s == Step.Groth16Verifier) return "groth16Verifier";
        if (s == Step.EmergencyStop) return "emergencyStop";
        if (s == Step.Registry) return "registry";
        if (s == Step.Vault) return "vault";
        if (s == Step.Token) return "token";
        if (s == Step.RegisterETH) return "registerETH";
        if (s == Step.RegisterToken) return "registerToken";
        if (s == Step.RenounceStopOwner) return "renounceStopOwner";
        return "done";
    }

    /// @dev A JSON-RPC quantity as vm.rpc returns it (big-endian bytes).
    function _quantity(bytes memory b) internal pure returns (uint256 v) {
        require(b.length <= 32, "DeployVault: the node's estimate is not a quantity");
        for (uint256 i = 0; i < b.length; i++) {
            v = (v << 8) | uint8(b[i]);
        }
    }

    function _eq(string memory a, string memory b) internal pure returns (bool) {
        return keccak256(bytes(a)) == keccak256(bytes(b));
    }
}
