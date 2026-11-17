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
    [/ALREADY_CLAIMED|250 /, "Already claimed: this position was claimed before. Nothing was spent; refresh your positions."],

    [/INSUFFICIENT_BALANCE|254 /, "Insufficient balance: the account holds less than this denomination. Choose a smaller one or another account."],
    [/FROZEN|frozen/i, "Frozen: the bridge or this token is frozen by its freeze authority until the time shown in the registry. Try again after it."],
    [/CAP|cap exceeded|CapExceeded/i, "Cap exceeded: the rate limit for this token is used up. It refills over time (see the registry); try later or a smaller amount."],
    [/NOT_REGISTERED|not registered|unsupported/i, "Unsupported token or amount: only the registered tokens and their fixed denominations can be bridged (see the registry)."],
    [/LEAF_NOT_IMPORTED|245 |248 |no imported root/, "Not final yet: the deposit is not covered by an imported root. Wait for the relayer's next import, then claim."],
    [/no accepted burn root|wait for the next checkpoint/, "Not final yet: no checkpoint covers this burn. Wait for the next checkpoint, then release."],
    [/WrongDepositIndex/, "Out of date: your deposit counter changed (another deposit was sent). Review the deposit again."],
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
