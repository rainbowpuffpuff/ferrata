// The mini-app driven headless (Qt Quick Test, offscreen) against the devnet backend
// (cli/examples/miniapp_devnet.rs: anvil + the LEZ harness), screen by screen, with a screenshot of
// each state in results/screenshots/. Run by scripts/box-test.sh from the module's directory.
// The B3 done gate is the private round trip on the defaults: private deposit (04), the claim's
// random delay waited out and a private claim (07), a private burn to a fresh address (10), the
// release's random delay waited out and the release by a relayer to that address (11). No default is
// changed and no delay is overridden on that path; "waiting" is the devnet's clock moved on.
import QtQuick
import QtTest
import "../qml/js/format.js" as Fmt

Item {
    id: host
    width: 1180
    height: 1500
    property var app: null
    property string privateRecipient: ""
    property string publicRecipient: ""

    TestCase {
        name: "FerrataMiniApp"
        when: windowShown

        function initTestCase() {
            var c = Qt.createComponent(Qt.resolvedUrl("../qml/Main.qml"));
            if (c.status === Component.Error) fail(c.errorString());
            host.app = c.createObject(host);
            verify(host.app !== null, "Main.qml created");
            tryVerify(function () { return host.app.info !== null && host.app.registry !== null; }, 120000, "info and registry loaded");
        }
        function el(name) { var it = findChild(host.app, name); verify(it !== null, "found " + name); return it; }
        // Long enough for a private transaction proven for real on a CPU (FERRATA_DEVNET_PROOFS=real).
        function idle() { tryVerify(function () { return host.app.busy === 0; }, 1800000, "calls finished"); }
        function click(name) { wait(80); var it = el(name); verify(it.visible, name + " visible"); verify(it.enabled, name + " enabled"); mouseClick(it); }
        function shot(name) { idle(); wait(200); var img = grabImage(host.app); img.save("results/screenshots/" + name + ".png"); }
        function tab(name) { click("tab." + name); wait(150); }
        function has(text, part) { return String(text).indexOf(part) >= 0; }
        function approve(titlePart, shotName) {
            tryVerify(function () { return host.app.approval !== null; }, 30000, "the wallet's approval prompt");
            verify(has(host.app.approval.title, titlePart), "prompt: " + host.app.approval.title);
            if (shotName) shot(shotName);
            click("approval.approve");
            idle();
        }

        // RFP-022 U.08's failure modes as the user sees them: the banner's message (what to do) and its technical line
        // (the code), and the raw refusal behind both names no deposit or burn: no 8 hex digits in a row (no hash,
        // commitment, nullifier, tag or address, nor a prefix of one), no leaf, burn or counter named by number, and not
        // the position's own index as a number.
        function namesNothing(raw, index) {
            var s = String(raw);
            // A hash, commitment, nullifier, tag or address, or 8 hex digits of one: a run of 8 or more hex digits with a
            // letter in it, or 40 or more (a decimal number, such as an Ethereum time, is neither).
            var runs = s.match(/[0-9a-fA-F]{8,}/g) || [];
            for (var r = 0; r < runs.length; r++)
                if (/[a-fA-F]/.test(runs[r]) || runs[r].length >= 40) return false;
            if (/\b(leaf|leaf index|leaf_index|burn|burn index|counter|j)\s*[:=#]?\s*\d/i.test(s)) return false;
            return s.split(/[^0-9A-Za-z]+/).indexOf(String(index)) < 0;
        }
        // `before`: where the refusal was found, before anything was signed ("checked before sending", the vault's
        // "eth_call before sending").
        function refused(message, code, before, index) {
            verify(!host.app.lastOk, "refused: " + host.app.lastError);
            verify(has(host.app.lastError, before), "refused before anything is signed: " + host.app.lastError);
            verify(has(el("error.message").text, message), "the message shown: " + el("error.message").text + " / " + host.app.lastError);
            verify(has(el("error.detail").text, code), "the code shown: " + el("error.detail").text);
            verify(namesNothing(host.app.lastError, index), "names no deposit or burn: " + host.app.lastError);
            console.log("U8 " + code + " | " + el("error.message").text + " | " + el("error.detail").text);
        }

        // The message table on its own (an internal review's L3, L4, M1): a 251 the program returned names both causes, a number
        // such as "0.249 ETH" is not a code, a SetCap below the floor (240) is not "Cap exceeded", and the verifier's
        // VerificationFailed, named or raw, has its own message.
        function test_00_messages() {
            var cases = [
                ["refused: LEZ bridge program exit code 251 NOT_REGISTERED (returned by the program in the harness): the token has no registry entry in this configuration, or the amount is not one of its denominations", "Unsupported token or amount:"],
                ["refused: LEZ bridge program exit code 251 NOT_REGISTERED (checked before sending: the amount is not one of the token's denominations): the token has no registry entry in this configuration, or the amount is not one of its denominations", "Invalid amount:"],
                ["refused: LEZ bridge program exit code 251 NOT_REGISTERED (checked before sending: no token(cid, token) account): the token has no registry entry in this configuration, or the amount is not one of its denominations", "Unsupported token: "],
                ["a deposit of 0.249 ETH and one of 0.252 ETH: no reply", "The operation failed. Nothing was changed."],
                ["refused: LEZ bridge program exit code 240 BAD_MESSAGE (SetCap, returned by the program): a message that does not decode or that this actor does not take, or values outside the specified bounds", "Refused: a value is outside the bridge's bounds"],
                ["refused: LEZ bridge program exit code 255 CAPPED (returned by the program): the token's or the global mint or burn bucket holds less than the amount; retry later", "Cap exceeded:"],
                ["refused: vault error VerificationFailed() (eth_call before sending): the RISC Zero verifier rejects the proof", "Verification failed: the release's proof does not check against the vault."],
                ["the vault would revert: server returned an error response: error code 3: execution reverted: custom error 0x439cc0cd, data: \"0x439cc0cd\"", "Verification failed: the release's proof does not check against the vault."]
            ];
            for (var i = 0; i < cases.length; i++)
                verify(has(Fmt.friendly(cases[i][0]), cases[i][1]), cases[i][0] + " -> " + Fmt.friendly(cases[i][0]));
        }

        function test_01_connect() {
            shot("01-start");
            verify(!el("deposit.review").enabled, "no deposit before the wallet is connected");
            click("wallet.connect");
            tryVerify(function () { return host.app.selecting; }, 5000);
            idle();
            shot("02-wallet-account-selection");
            click("wallet.account.0");
            tryVerify(function () { return host.app.connectedAccount !== ""; }, 5000);
            idle();
        }

        function test_02_endpoints() {
            tab("settings");
            el("settings.ethRpc").text = "http://127.0.0.1:18999";
            click("settings.apply");
            idle();
            tryVerify(function () { return has(el("settings.status").text, "does not answer"); }, 60000, "an unreachable RPC is reported");
            verify(has(el("header.endpoints").text, "18999"), "the header names the new RPC");
            shot("03-settings-rpc-unreachable");
            el("settings.ethRpc").text = "http://127.0.0.1:18545";
            el("settings.sequencer").text = "http://127.0.0.1:3040";
            click("settings.apply");
            idle();
            verify(has(el("settings.status").text, "Not switched"), "a sequencer the devnet does not have is refused: " + el("settings.status").text);
            shot("04-settings-sequencer-refused");
            el("settings.sequencer").text = "harness";
            click("settings.apply");
            idle();
            tryVerify(function () { return has(el("settings.status").text, "Connected: chain 1"); }, 60000, "back on the devnet RPC");
            shot("05-settings-connected");
        }

        function test_03_registry() {
            tab("registry");
            click("registry.refresh");
            idle();
            verify(has(el("registry.token.0").text, "ETH  registered"), el("registry.token.0").text);
            verify(el("registry.lez.0").visible, "the LEZ side of the token is shown");
            shot("06-registry");
        }

        function test_04_deposit_private_by_default() {
            tab("deposit");
            verify(host.app.destKind === "private" && el("deposit.kind.private").checked, "private destination pre-selected");
            verify(host.app.delayOn && host.app.relayerOn, "delay and relayer on by default");
            verify(has(el("deposit.leaks.summary").text, "Minimal"), el("deposit.leaks.summary").text);
            shot("07-deposit-defaults");
            click("deposit.review");
            approve("Deposit", "08-deposit-approval");
            tryVerify(function () { return has(host.app.depositText, "Deposited"); }, 60000, "deposit sent");
            verify(has(host.app.depositText, "private account"), host.app.depositText);
            shot("09-deposit-private-sent");
        }

        function test_05_public_needs_an_explicit_action() {
            click("deposit.kind.public");
            verify(!el("deposit.review").enabled, "a public destination needs the acknowledgement");
            verify(has(el("deposit.leaks.summary").text, "High"), el("deposit.leaks.summary").text);
            shot("10-deposit-public-needs-ack");
            click("deposit.public.ack");
            click("deposit.delay");
            verify(!host.app.delayOn, "delay off");
            click("deposit.review");
            tryVerify(function () { return host.app.approval !== null; }, 30000);
            click("approval.reject");
            verify(has(el("error.message").text, "Rejected in the wallet"), el("error.message").text);
            shot("11-deposit-rejected-in-wallet");
            click("deposit.review");
            approve("Deposit", null);
            tryVerify(function () { return has(host.app.depositText, "public account"); }, 60000, "public deposit sent");
            shot("12-deposit-public-sent");
        }

        function test_06_positions_from_the_seed() {
            tab("positions");
            click("positions.recover");
            idle();
            compare(host.app.positions.deposits.length, 2, "two deposits found from the seed");
            verify(has(el("positions.summary").text, "Found 2 deposits"), el("positions.summary").text);
            shot("13-positions-before-import");
        }

        // Insufficient finality (U.08): a claim before its root is imported is refused before it is proven (245: no
        // imported root yet).
        function test_06b_error_not_final_claim() {
            compare(host.app.positions.deposits[0].state, "waiting for the root import", "nothing imported yet");
            host.app.call("claim", { leaf_index: 0, private: true });
            idle();
            refused("Not final yet: the deposit is not covered by an imported root. Wait for the relayer's next import, then claim.", "245 NOT_FOUND", "checked before sending: no imported root covers this leaf yet", 0);
            shot("13b-error-not-final-claim");
        }

        function test_07_private_claim() {
            tab("claim");
            click("claim.relay");
            idle();
            tryVerify(function () { return host.app.positions && host.app.positions.deposits[0].state === "claimable"; }, 60000, "imported");
            click("claim.pick.0");
            verify(host.app.claimMode === "private" && el("claim.mode.private").checked, "private claim pre-selected");
            verify(!el("claim.submit").enabled, "held by the random delay");
            verify(has(el("claim.leaks.summary").text, "Minimal"), el("claim.leaks.summary").text);
            verify(el("claim.delay").visible, "the delay is explained");
            shot("14-claim-held-by-delay");
            click("claim.wait");
            idle();
            tryVerify(function () { return !host.app.held(0); }, 10000, "the delay has passed on the devnet's clock");
            verify(!host.app.skipDelay, "the delay was waited out, not overridden");
            verify(el("claim.submit").enabled, "claimable once the delay has passed");
            shot("15-claim-delay-passed");
            click("claim.submit");
            approve("Private claim", "15b-claim-approval");
            tryVerify(function () { return has(host.app.claimText, "Claimed privately"); }, 60000, host.app.claimText);
            var r = host.app.claimReply;
            verify(r.settled && r.private, "settled as a private transaction");
            compare(r.balance, host.app.positions.deposits[0].amount, "the denomination is held privately by /0");
            console.log("PROOF claim " + JSON.stringify({ mode: r.proof.mode, prove_ms: r.proof.prove_ms, receipt_bytes: r.proof.receipt_bytes, tx_bytes: r.proof.tx_bytes }));
            tryVerify(function () { return host.app.positions.deposits[0].state === "claimed"; }, 60000, "the deposit shows as claimed (its nullifier's account, named by the private transaction)");
            compare(host.app.holding("private"), r.balance, "the private holding /0");
            shot("16-claim-private-settled");
        }

        function test_08_plaintext_claim() {
            click("claim.pick.1");
            click("claim.mode.plain");
            verify(!el("claim.submit").enabled, "plaintext needs the acknowledgement");
            verify(has(el("claim.leaks.summary").text, "High"), el("claim.leaks.summary").text);
            click("claim.plain.ack");
            click("claim.submit");
            approve("Plaintext claim", null);
            tryVerify(function () { return has(host.app.claimText, "Claimed:"); }, 60000, host.app.claimText);
            shot("17-claim-plaintext-settled");
        }

        // The view's steps first, so that a failed check of the direct call after them leaves the later tests their tab.
        function test_09_error_messages() {
            tab("redeem");
            click("burn.kind.public");
            click("burn.public.ack");
            click("burn.denom.2");
            click("burn.submit");
            approve("Burn", null);
            verify(has(el("error.message").text, "Insufficient balance"), el("error.message").text);
            shot("19-error-insufficient-balance");
            host.app.call("claim", { leaf_index: 1, private: false });
            idle();
            refused("Already claimed: this position was claimed before.", "250 ALREADY_CLAIMED", "checked before sending", 1);
            shot("18-error-already-claimed");
        }

        // Unsupported token (U.08): a deposit of a token the vault has not registered, refused before signing.
        function test_09b_error_unsupported_token() {
            host.app.call("deposit", { token: "0x000000000000000000000000000000000000dEaD", denomination: 0, kind: "private", account: 0 });
            idle();
            refused("Unsupported token: this token is not registered with the bridge.", "TokenNotRegistered", "checked before sending", 0);
            shot("19b-error-unsupported-token");
        }

        // Invalid amount (U.08): a denomination the token does not have, refused before signing.
        function test_09c_error_invalid_amount() {
            host.app.call("deposit", { denomination: 7, kind: "private", account: 0 });
            idle();
            refused("Invalid amount: only this token's fixed denominations can be bridged.", "InvalidDenominationIndex", "checked before sending", 7);
            shot("19c-error-invalid-amount");
        }

        // Cap exceeded (U.08): the vault's admin sets ETH's deposit cap to 1 wei; the deposit is refused by the vault's
        // eth_call before anything is signed; the cap is put back before the checks, so a failed check stops nothing after.
        function test_09d_error_cap_exceeded() {
            host.app.call("admin.set_cap", { vault: true, direction: "deposits", capacity: "1" });
            idle();
            verify(host.app.lastOk, "the vault's deposit cap set: " + host.app.lastError);
            host.app.call("deposit", { denomination: 0, kind: "private", account: 0 });
            idle();
            var ok = host.app.lastOk, raw = host.app.lastError;
            var shown = el("error.message").text, code = el("error.detail").text;
            shot("19d-error-cap-exceeded");
            host.app.call("admin.set_cap", { vault: true, direction: "deposits", capacity: "115792089237316195423570985008687907853269984665640564039457584007913129639935" });
            idle();
            verify(host.app.lastOk, "the cap put back: " + host.app.lastError);
            verify(!ok, "refused: " + raw);
            verify(has(raw, "eth_call before sending"), "refused before anything is signed: " + raw);
            verify(has(shown, "Cap exceeded: the rate limit for this token is used up."), "the message shown: " + shown + " / " + raw);
            verify(has(code, "TokenDepositCapExceeded"), "the code shown: " + code);
            verify(namesNothing(raw, 0), "names no deposit: " + raw);
            console.log("U8 TokenDepositCapExceeded | " + shown + " | " + code);
        }

        // Verification failure (U.08): a claim naming another account than the one the deposit commits to fails the
        // program's verify_claim (249), checked before anything is sent.
        function test_09e_error_verification_failed() {
            host.app.call("claim", { leaf_index: 1, private: false, to_account: 2 });
            idle();
            refused("Verification failed: the claim does not check against the imported root", "249 CLAIM_INVALID", "checked before sending with the program's verify_claim", 1);
            shot("19e-error-verification-failed");
        }

        // Verification failure (U.08): a deposit to a private account by the plaintext path (252), checked before sending.
        function test_09f_error_wrong_destination() {
            host.app.call("claim", { leaf_index: 0, private: false });
            idle();
            refused("Verification failed: a deposit to a private account can only be claimed privately.", "252 WRONG_DESTINATION", "checked before sending: a private-kind deposit is claimed privately", 0);
            shot("19f-error-wrong-destination");
        }

        function test_10_private_burn_on_defaults() {
            click("burn.kind.private");
            verify(host.app.burnKind === "private" && el("burn.kind.private").checked, "private holder selected (the default)");
            click("burn.denom.0");
            verify(host.app.releaseDelayOn && el("burn.delay").checked, "the release's random delay on by default");
            host.privateRecipient = host.app.recipient;
            verify(host.privateRecipient.toLowerCase() !== String(host.app.info.user.depositor).toLowerCase(), "a fresh recipient generated");
            verify(has(el("burn.leaks.summary").text, "Minimal"), el("burn.leaks.summary").text);
            verify(has(el("burn.holding").text, "Private holding /0: 0.1 ETH"), el("burn.holding").text);
            shot("20-burn-private-defaults");
            idle();
            click("burn.submit");
            approve("Burn", "20b-burn-approval");
            tryVerify(function () { return has(host.app.burnText, "Burned privately"); }, 60000, host.app.burnText);
            var r = host.app.burnReply;
            verify(r.settled && r.private, "settled as a private transaction");
            compare(r.burn_index, 0, "the first burn");
            compare(r.balance, "0", "the private holding is spent");
            compare(r.recipient.toLowerCase(), host.privateRecipient.toLowerCase(), "to the fresh recipient");
            console.log("PROOF burn " + JSON.stringify({ mode: r.proof.mode, prove_ms: r.proof.prove_ms, receipt_bytes: r.proof.receipt_bytes, tx_bytes: r.proof.tx_bytes }));
            shot("21-burn-private-settled");
        }

        // Insufficient finality on the way out (U.08): the burn's release before any checkpoint covers it, refused before
        // anything is signed (the vault's BurnRootNotAccepted).
        function test_10b_error_not_final_release() {
            host.app.call("release", { burn_index: 0 });
            idle();
            refused("Not final yet: no checkpoint covers this burn. Wait for the next checkpoint, then release.", "BurnRootNotAccepted", "checked before sending: no finalized checkpoint covers this burn yet", 0);
            shot("21a-error-not-final-release");
        }

        function test_11_checkpoint_and_private_release() {
            click("redeem.checkpoint");
            idle();
            click("redeem.refresh");
            idle();
            compare(host.app.positions.burns.length, 1, "the private burn found from the seed");
            click("release.pick.0");
            verify(!el("release.submit").enabled, "the private burn's release is held by its random delay");
            verify(el("release.delay").visible, "the delay is explained");
            shot("22-release-held-by-delay");
            click("release.wait");
            idle();
            tryVerify(function () { return !host.app.releaseHeld(0); }, 10000, "the delay has passed on the devnet's clock");
            verify(!host.app.releaseSkipDelay, "the delay was waited out, not overridden");
            verify(el("release.submit").enabled, "releasable once the delay has passed");
            click("release.submit");
            approve("Release", "22b-release-approval");
            tryVerify(function () { return has(host.app.releaseText, "Released:"); }, 60000, host.app.releaseText);
            verify(has(host.app.releaseText.toLowerCase(), host.privateRecipient.toLowerCase()), "paid to the private burn's fresh recipient: " + host.app.releaseText);
            verify(has(host.app.releaseText, "0.1 ETH paid"), host.app.releaseText);
            shot("23-released-private-round-trip");
            tab("positions");
            click("positions.recover");
            idle();
            verify(has(el("positions.summary").text, "0 not released"), el("positions.summary").text);
            shot("23c-positions-after-private-round-trip");
            tab("redeem");
        }

        // The public path keeps its warning: a public burn needs the acknowledgement.
        function test_11b_public_burn_and_checkpoint() {
            click("burn.kind.public");
            verify(!el("burn.submit").enabled, "a public burn needs the acknowledgement");
            verify(has(el("burn.leaks.summary").text, "High"), el("burn.leaks.summary").text);
            shot("21b-burn-public-needs-ack");
            click("burn.public.ack");
            click("burn.fresh");
            host.publicRecipient = host.app.recipient;
            click("burn.delay");
            verify(!host.app.releaseDelayOn, "release delay off for this one");
            idle();
            click("burn.submit");
            approve("Burn", null);
            tryVerify(function () { return has(host.app.burnText, "Burned 0.1 ETH"); }, 60000, host.app.burnText);
            compare(host.app.burnReply.burn_index, 1, "the second burn");
            shot("21c-burn-public-sent");
            click("redeem.checkpoint");
            idle();
            click("redeem.refresh");
            idle();
        }

        // Verification failure on the way out (U.08; an internal review's M1): the public burn's release with a proof the verifier
        // rejects (the devnet's wrong_proof: the mock verifier is not told of its journal, as for a seal made for another
        // recipient, relay fee or root), refused by the vault's eth_call before anything is signed.
        function test_11c_error_release_proof_rejected() {
            host.app.call("release", { burn_index: 1, wrong_proof: true });
            idle();
            refused("Verification failed: the release's proof does not check against the vault.", "VerificationFailed", "eth_call before sending", 1);
            shot("23a-error-release-proof-rejected");
        }

        // The same burn released with its proof.
        function test_11d_public_release() {
            click("release.pick.1");
            idle();
            click("release.submit");
            approve("Release", null);
            tryVerify(function () { return has(host.app.releaseText.toLowerCase(), host.publicRecipient.toLowerCase()); }, 60000, host.app.releaseText);
            host.app.call("release", { burn_index: 0 });
            idle();
            verify(has(el("error.message").text, "Already released"), "a second release: " + el("error.message").text);
            shot("23b-error-already-released");
            tab("positions");
            click("positions.recover");
            idle();
            verify(has(el("positions.summary").text, "0 not released"), el("positions.summary").text);
            shot("24-positions-after-round-trip");
        }

        function test_12_admin() {
            tab("admin");
            click("admin.refresh");
            idle();
            verify(has(el("admin.roles").text, "freeze authority"), el("admin.roles").text);
            shot("25-admin");
            el("admin.cap.capacity").text = "1";
            click("admin.setcap");
            approve("SetCap", null);
            // What the user sees: the banner (its message and code) and the admin view's result line. The banner stays: the
            // view refreshes the registry only after a change (an internal review's L3: an unconditional refresh, ffcf0da3, cleared it).
            verify(el("error.banner").visible, "the banner shows the refusal: " + host.app.lastError);
            verify(has(el("error.message").text, "Refused: a value is outside the bridge's bounds"), "the banner: " + el("error.message").text + " / " + host.app.lastError);
            verify(has(el("error.detail").text, "240 BAD_MESSAGE"), "the banner's code: " + el("error.detail").text);
            verify(has(el("admin.result").text, "Not done: refused: LEZ bridge program exit code 240 BAD_MESSAGE"), "a cap below the floor is refused: " + el("admin.result").text);
            shot("26-admin-cap-below-floor-refused");
            el("admin.cap.capacity").text = "11000000000";
            click("admin.setcap");
            approve("SetCap", null);
            tryVerify(function () { return has(el("admin.result").text, "Cap set"); }, 60000, el("admin.result").text);
            click("admin.freeze");
            approve("Freeze", "27-admin-freeze-approval");
            tryVerify(function () { return has(el("admin.result").text, "Frozen until"); }, 60000, el("admin.result").text);
            shot("28-admin-frozen");
            host.app.call("deposit", { denomination: 0, kind: "public", account: 1 });
            idle();
            host.app.call("relay", {});
            idle();
            host.app.call("claim", { leaf_index: 2, private: false });
            idle();
            refused("Frozen: the bridge or this token is frozen by its freeze authority", "242 FROZEN", "checked before sending", 2);
            shot("29-error-frozen");
            tab("registry");
            click("registry.refresh");
            idle();
            verify(!has(el("registry.lez.0").text, "mints frozen until not frozen"), el("registry.lez.0").text);
            shot("30-registry-after-admin");
            tab("admin");
            click("admin.unfreeze");
            approve("Lift the freeze", null);
            tryVerify(function () { return has(el("admin.result").text, "Freeze lifted"); }, 60000, el("admin.result").text);
            host.app.call("claim", { leaf_index: 2, private: false });
            idle();
            verify(host.app.lastOk, "the claim refused while frozen goes through once the freeze is lifted: " + host.app.lastError);
            verify(has(host.app.lastResult.what, "plaintext claim of leaf 2"), host.app.lastResult.what);
            shot("30b-admin-freeze-lifted");
        }

        function test_13_privacy() {
            tab("privacy");
            shot("31-privacy");
        }

        // The wallet seam: an LP-0021-shaped provider set as walletProvider replaces the built-in
        // account selection and approval prompt; nothing else in the view changes.
        function test_14_wallet_provider_seam() {
            var asked = [];
            host.app.walletProvider = {
                connect: function (dapp, accounts, done) { asked.push("connect: " + dapp); done(accounts[0]); },
                approve: function (req, done) { asked.push("approve: " + req.title); done(true); }
            };
            host.app.connectedAccount = "";
            tab("deposit");
            click("wallet.connect");
            tryVerify(function () { return host.app.connectedAccount !== ""; }, 5000, "connected through the provider");
            verify(!host.app.selecting, "the built-in selection was not shown");
            click("deposit.kind.private");
            host.app.depositText = "";
            idle();
            click("deposit.review");
            tryVerify(function () { return has(host.app.depositText, "Deposited"); }, 60000, "a deposit approved by the provider");
            verify(host.app.approval === null, "the built-in prompt was not used");
            compare(asked.length, 2, asked.join(" | "));
            verify(has(asked[1], "Deposit"), asked[1]);
            console.log("WALLET seam " + asked.join(" | "));
            shot("32-wallet-provider-seam");
            host.app.walletProvider = null;
        }
    }
}
