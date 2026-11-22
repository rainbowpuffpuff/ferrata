.pragma library
// What each step makes public, and what the user's current choices would leak (RFP-022 U.05-U.07).
// Sources: the proposal's §B6 field table and docs/privacy/RESULTS.md (the privacy suite measured the
// default path); the residual correlations are those docs/trust.md lists.

var STEPS = {
    deposit: {
        title: "Deposit (Ethereum)",
        publicEth: ["your depositor address", "the token", "the denomination and its fees", "the time", "the commitment C (random-looking)"],
        publicLez: [],
        hidden: ["the LEZ account that will receive the tokens (C commits to it)", "which later claim spends it"]
    },
    claim: {
        title: "Claim (LEZ)",
        publicEth: [],
        publicLez: ["the nullifier N (fresh randomness)", "the denomination", "which imported root was read", "a public destination and its balance, if you chose one"],
        hidden: ["which deposit is claimed (no leaf index, no depositor)", "a private destination and its balance"]
    },
    burn: {
        title: "Burn (LEZ)",
        publicEth: [],
        publicLez: ["the burn note: token, denomination, r, the recipient encrypted (c_out)", "the holder account, if it is public"],
        hidden: ["the Ethereum recipient (encrypted to you)", "a private holder and its balance"]
    },
    release: {
        title: "Release (Ethereum)",
        publicEth: ["the recipient address", "the token and amount", "NOut (fresh randomness)", "the burn root used", "the time", "the relayer that sent it (it pays the gas)"],
        publicLez: [],
        hidden: ["which burn is released (no burn index)", "the LEZ holder"]
    }
};

// A step's table for the path the view is on. `ownRelease`: the release is sent from the user's own release account
// (the module's path through Logos Core's send stack), not by a relayer: that account, not a relayer, is public.
function step(name, ownRelease) {
    var s = STEPS[name];
    if (name !== "release" || !ownRelease) return s;
    return {
        title: s.title,
        publicEth: ["the recipient address", "the token and amount", "NOut (fresh randomness)", "the burn root used", "the time",
                    "the account that sent it: on this path your own release account (it pays the gas; your releases from it are linked to each other, but not to your depositor address unless you fund it from there, or from an account linked to it)"],
        publicLez: s.publicLez,
        hidden: s.hidden
    };
}

// level: 0 fine, 1 a correlation the user accepts, 2 a direct link.
function leaks(c) {
    var out = [];
    out.push({ level: 0, text: "Fixed denomination: your deposit looks like every other deposit of " + c.denominationText + "." });
    if (c.kind === "private")
        out.push({ level: 0, text: "Private destination: the receiving account and its balance stay private on LEZ." });
    else
        out.push({ level: 2, text: "Public destination: the account and its balance become public on LEZ when claimed; anyone can see it received " + c.denominationText + " at that time." });
    if (c.delay)
        out.push({ level: 0, text: "Delayed claim: held for " + c.delayHours + " h, so the claim's time does not point at your deposit." });
    else
        out.push({ level: 1, text: "No delay: a claim minutes after a deposit narrows the deposits it could be to those made just before." });
    if (c.relayer)
        out.push({ level: 0, text: "Permissionless relayer: a relayer submits and pays the LEZ fee; no fee payment ties the claim to you." });
    else
        out.push({ level: 1, text: "Self-submitted: the account paying the LEZ fee is linked to the claim." });
    if (c.account >= 8)
        out.push({ level: 2, text: "Account /" + c.account + " is outside the recovery window (8): a restored wallet would not find it." });
    return out;
}

function exposure(list) {
    var worst = 0;
    for (var i = 0; i < list.length; i++) worst = Math.max(worst, list[i].level);
    return worst === 0 ? "Minimal: only the residual correlations (token, denomination, time)"
         : worst === 1 ? "Elevated: a timing or fee-payer correlation"
         : "High: a direct link is public";
}

function burnLeaks(c) {
    var out = [];
    if (c.kind === "private")
        out.push({ level: 0, text: "Burn from a private account: the holder stays private." });
    else
        out.push({ level: 2, text: "Burn from a public account: the holder, its balance and the burn are public on LEZ." });
    if (c.freshRecipient)
        out.push({ level: 0, text: "Fresh recipient address: nothing on Ethereum ties the release to your depositor address" +
                                   (c.relayer ? "." : ", as long as the account that sends it is not funded from your depositor address (or an account linked to it).") });
    else
        out.push({ level: 1, text: "The recipient is an address you used before: the release is linked to it." });
    if (c.relayer)
        out.push({ level: 0, text: "Released through a relayer: the recipient needs no ETH for gas." });
    else
        out.push({ level: 1, text: "You send the release yourself: your gas payer is linked to the recipient." });
    if (c.delay === undefined || c.delay)
        out.push({ level: 0, text: "Delayed release: held for " + (c.delayHours || "a random number of") + " h after the burn, so the release's time does not point at your burn." });
    else
        out.push({ level: 1, text: "No delay: a release right after a burn narrows the burns it could be to those made just before." });
    return out;
}

function claimLeaks(c) {
    var out = [];
    if (c.mode === "private")
        out.push({ level: 0, text: "Private claim: proven without saying which deposit it spends (no leaf index, no depositor)." });
    else
        out.push({ level: 2, text: "Plaintext claim: names on LEZ which deposit paid which account." });
    if (c.kind === "public")
        out.push({ level: 2, text: "Public destination: the account and its new balance are public." });
    else if (c.kind === "private")
        out.push({ level: 0, text: "Private destination: the account and its balance stay private." });
    if (c.held && c.skipDelay)
        out.push({ level: 1, text: "Claiming before your random delay ends: the claim's time narrows which deposits it could be." });
    else if (c.held)
        out.push({ level: 0, text: "Held by your random delay: the claim will not follow the deposit closely in time." });
    if (c.relayer)
        out.push({ level: 0, text: "Submitted by a permissionless relayer: no fee payment ties the claim to you." });
    return out;
}
