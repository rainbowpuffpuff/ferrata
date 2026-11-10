// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";
import {VaultInvariants} from "./VaultInvariants.t.sol";

/// @notice The same handler and the same invariants on a rule (B) vault (v4): candidates finalise with their own
///         journal once W = deltaAtSubmit (1 h) has passed, without beacons or a second proof.
contract VaultInvariantsB is VaultInvariants {
    function _useVault() internal override {
        vault = new FerrataVault(_deploymentB(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID));
        _registerDefaultTokens(vault);
    }
}
