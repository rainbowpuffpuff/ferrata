.pragma library
// Amounts and messages (RFP-022 U.08: clear, actionable, and never naming the deposit or burn).

function units(base, decimals, symbol) {
    var s = String(base || "0");
    var neg = s.charAt(0) === "-";
    if (neg) s = s.substring(1);
    while (s.length <= decimals) s = "0" + s;
    var whole = s.substring(0, s.length - decimals);
    var frac = s.substring(s.length - decimals).replace(/0+$/, "");
    return (neg ? "-" : "") + whole + (frac.length ? "." + frac : "") + (symbol ? " " + symbol : "");
}

function eth(wei) { return units(wei, 18, "ETH"); }

// A cap: "unlimited" for the vault's no-cap value (2^256 - 1 and its neighbours), else the amount.
function cap(base, decimals, symbol) {
    return String(base || "0").length > 40 ? "unlimited" : units(base, decimals, symbol);
}

function short(h) {
    var s = String(h || "");
    return s.length > 14 ? s.substring(0, 8) + "…" + s.substring(s.length - 6) : s;
}

function pct(used, capacity) {
    if (String(capacity || "0").length > 40) return "0 %";
    var c = Number(capacity), u = Number(used);
    if (!(c > 0)) return "–";
    return (100 * u / c).toFixed(1) + " %";
}

// The program's and the vault's codes, by name, mapped to what the user can do.
var MESSAGES = [
    // Inside Logos the bridge module's wallet session is opened with a chain service (the stand-in for Logos's Ethereum
    // and LEZ modules), which the view does not open: in Basecamp a person meets this on the first call.
    [/no wallet session|no network session/, "Not connected to a network: open a session above (the test network's address and your recovery phrase). Nothing was read or sent."],
    // The network session (ferrata_bridge_module's open_network): what it does not do, and what a step needs first.
    [/needs a proof of LEZ's privacy circuit/, "A private claim or burn needs a proof made on your machine with your keys: run it with the ferrata tool from the same phrase, or choose the plaintext claim or the public burn here (each after its acknowledgement). Nothing was sent."],
    [/use the faucet first/, "Not enough LGO for the LEZ fee: get test LGO with the faucet button above, then try again. Nothing was sent."],
    // The session's own refusals (review round 1): who may use it, what a step needs, how long a call may take.
    [/not the one this view opened/, "Refused: the module's session is not the one this view opened. Close the session and open it again. Nothing was sent."],
    [/network session ended/, "The module's session ended (15 minutes without a call, or the module restarted): open it again above. Nothing was sent."],
    [/belongs to the Ferrata Bridge view|opened by the Ferrata Bridge view|MAC does not hold|\(a replay\)/, "Refused: only the view that opened the session can use it. Nothing was read or sent."],
    [/a network session is open in this module; close it first/, "A session is already open in the module: close it first (or quit Basecamp; it also ends after 15 minutes without a call)."],
    [/deposits are made in the network's web wallet/, "Deposits are made in the network's web wallet with this phrase (Basecamp has no Logos signer for Sepolia yet); claim and burn them here afterwards."],
    [/release address|enter the Ethereum address the release pays|checksummed form|fails its EIP-55/, "Enter the address the release pays: yours, in its checksummed form (upper and lower case as your wallet shows it). Nothing was signed."],
    [/did not answer in time|more than this call has left/, "The network did not answer in time: try again. Nothing was signed."],
    [/above this module's ceiling/, "Refused: the network asks for a LEZ fee above the module's ceiling. Nothing was signed."],
    [/no transaction waits for approval/, "That transaction no longer waits for approval (sent, replaced or expired): start the step again."],
    [/a deposit needs your Sepolia address/, "A deposit needs your Sepolia address (the account Logos's signer sends it from): close the session and open it again with that address. Nothing was sent."],
    [/this session reads and sends through the network|this session's endpoints are the network's|admin calls are its operator's/, "Not in a network session: it uses the network's own endpoints, and the bridge's admin calls are its operator's. Nothing was changed."],
    // The module's jobs (its private claim and burn run for minutes): busy while one is proven, one at a time, a job the
    // module does not know (it was restarted).
    [/^busy: job \d+/, "Busy: a private transaction is being proven. This can be done once it is finished."],
    [/^refused: job \d+ .*one private transaction at a time/, "One private transaction at a time: the one being proven must finish first."],
    [/no job \d+ in this module/, "Unknown job: the module does not have it (it may have been restarted). Refresh your positions to see what reached the chain."],
    // The host stopped waiting (Basecamp cuts a ui module's call to a core module at 20 s), the module did not: the
    // step may still complete, so the user must not be told that nothing changed.
    [/Logos host:.*(timed out|timeout)/i, "No answer in time: the step may still complete. Refresh your positions before repeating it."],
    [/Rejected in the wallet/, "Rejected in the wallet: nothing was signed or sent."],
    [/private burn|harness settles public/i, "Not available on this devnet: private transactions are proven by the LEZ wallet on a sequencer."],
    [/NullifierAlreadyUsed|already released/, "Already released: this burn was paid out before. Nothing was sent."],
    [/no burn to checkpoint/, "Nothing to checkpoint yet: a checkpoint covers burns, and there is none."],
    [/ALREADY_CLAIMED|code 250 /, "Already claimed: this position was claimed before. Nothing was spent; refresh your positions."],

    [/INSUFFICIENT_BALANCE|code 254 /, "Insufficient balance: the account holds less than this denomination. Choose a smaller one or another account."],
    [/FROZEN|frozen/i, "Frozen: the bridge or this token is frozen by its freeze authority until the time shown in the registry. Try again after it."],
    [/\bCAPPED\b|code 255 |CapExceeded|cap exceeded/i, "Cap exceeded: the rate limit for this token is used up. It refills over time (see the registry); try later or a smaller amount."],
    [/BAD_MESSAGE|code 240 /, "Refused: a value is outside the bridge's bounds (a cap below its floor, for example). Nothing was changed; check the values."],
    [/InvalidDenominationIndex|not a denomination|the amount is not one of the token's denominations/, "Invalid amount: only this token's fixed denominations can be bridged. Choose one of the amounts the registry lists."],
    [/TokenNotRegistered|no token\(cid, token\) account|the token is not registered|unsupported token/i, "Unsupported token: this token is not registered with the bridge. Only the tokens the registry lists can be bridged."],
    // 251 without the tool's check: the program's code covers both causes.
    [/NOT_REGISTERED|code 251 /, "Unsupported token or amount: only the registered tokens and their fixed denominations can be bridged (see the registry)."],
    [/LEAF_NOT_IMPORTED|code 245 |code 248 |no imported root/, "Not final yet: the deposit is not covered by an imported root. Wait for the relayer's next import, then claim."],
    [/BurnRootNotAccepted|no accepted burn root|wait for the next checkpoint/, "Not final yet: no checkpoint covers this burn. Wait for the next checkpoint, then release."],
    [/CLAIM_INVALID|code 249 /, "Verification failed: the claim does not check against the imported root (its destination or path is not the deposit's). Nothing was spent; refresh your positions and claim to the account the deposit names."],
    [/WRONG_DESTINATION|code 252 /, "Verification failed: a deposit to a private account can only be claimed privately. Nothing was spent; choose Private claim."],
    [/WrongDepositIndex/, "Out of date: your deposit counter changed (another deposit was sent). Review the deposit again."],
    [/VerificationFailed|0x439cc0cd/, "Verification failed: the release's proof does not check against the vault. Nothing was sent; prove the release again, or ask the relayer for a new proof."],
    [/verif|proof|seal/i, "Verification failed: a proof or record did not check. Nothing was changed; retry, or switch endpoints under Endpoints."],
    [/error sending request|connect|Connection refused|unreachable|tcp/i, "Endpoint unreachable: the Ethereum RPC or the backend under Endpoints does not answer. Check the address there."]
];

function friendly(raw) {
    var s = String(raw || "");
    for (var i = 0; i < MESSAGES.length; i++)
        if (MESSAGES[i][0].test(s)) return MESSAGES[i][1];
    return "The operation failed. Nothing was changed.";
}

// The technical line shown under the friendly message: the code and name only, with leaf and
// burn indices, hashes and amounts removed, so a failure does not say which position it concerned.
function technical(raw) {
    return String(raw || "").replace(/0x[0-9a-fA-F]{8,}/g, "0x…").replace(/\b(leaf|burn|index|j)\s*[=:]?\s*\d+/gi, "$1 …").substring(0, 220);
}
