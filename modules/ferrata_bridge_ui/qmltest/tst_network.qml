// The mini-app's network session panel (Main.qml inside Logos) against a stand-in for ferrata_bridge_module, headless
// (Qt Quick Test, offscreen): the review of 24 Nov 2026's fixes, each a check that fails on the view before them.
//   H1  every call of the session carries a MAC under the key agreed at open, each counter once; a reply naming another
//       session's /0 is refused; a claim or burn is shown (the module's record) before `confirm`, which alone signs it.
//   M1  the phrase is sealed to the module's one-time key: open_network carries no phrase; the stand-in opens the seal
//       with its own secret and finds exactly the typed words; a new phrase's words come back sealed.
//   M2  the typed phrase is never a string in the view (its keys go into a byte buffer, shown as dots), and the buffer
//       is wiped when the phrase is sealed, before the module answers.
//   M3  in the network session the release address starts empty and nothing makes one up.
//   M4  the deposit button is off in the network session and the panel sends the person to the web wallet.
//   L1  every label shows its text as plain text.
//   L2  when open_network's answer is lost (Basecamp's 20 s), the view closes the session with its key.
//   M1  with Qt's debug logging on, the view says what its log holds before anything is typed and New phrase is off; the
//       phrase field takes each key's shortcut override, so Qt's key logging never sees its keys (round 2, L1).
// Run as `qmltestrunner -input qmltest/tst_network.qml` from the module's directory (scripts/network-ui-test.sh).
import QtQuick
import QtTest
import "../qml/js/seal.js" as Seal

Item {
    id: host
    width: 1180
    height: 1600
    property var app: null

    // The host's bridge as the view sees it in Basecamp (`logos.callModuleAsync`), answering for ferrata_bridge_module.
    QtObject {
        id: logos
        property var secret: null
        property string publicKey: ""
        property var keys: null
        property int lastN: 0
        property var requests: []
        property var methods: []
        property int macsChecked: 0
        property string opened: ""
        property string account0: "AKRYNTU8c4MZ4fSr5FHcn6BGE6T4dfRCzn1ziQCNKFBu"
        property string answerAs: ""
        property bool open: false
        property bool loseOpen: false
        property var held: null
        property string pendingKind: ""
        property bool debugLog: false
        property bool noEntropy: false
        property int opens: 0
        property int sealedBytes: 0
        property int jobId: 7

        function reply(result) {
            var r = { ok: true, result: result };
            if (open) r.session_account = answerAs !== "" ? answerAs : account0;
            return JSON.stringify(r);
        }
        function refuse(e) {
            var r = { ok: false, error: e };
            if (open) r.session_account = account0;
            return JSON.stringify(r);
        }
        function callModuleAsync(module, method, args, done, timeoutMs) {
            var text = String(args[0]);
            requests.push(text);
            var answer = handle(text);
            if (answer === null) return;          // never answered (a lost answer)
            done(answer);
        }
        function handle(text) {
            var r = JSON.parse(text);
            var method = r.method, params = r.params;
            if (open) {
                // The session's calls: the MAC under the agreed key, each counter once.
                if (r.call === undefined || r.mac !== Seal.macCall(keys, r.n, r.call) || !(r.n > lastN)) return refuse("refused: the call's MAC does not hold");
                lastN = r.n; macsChecked += 1;
                var inner = JSON.parse(r.call);
                method = inner.method; params = inner.params;
            }
            methods.push(method);
            if (method === "jobs") return reply({ jobs: [], qt_debug_logging: debugLog });
            if (method === "seal_key") {
                secret = new Uint8Array(32); for (var i = 0; i < 32; i++) secret[i] = 0x42;
                publicKey = Seal.hex(Seal.x25519Base(secret));
                var r = { key_id: 1, public_key: publicKey, scheme: "x25519-hmac-sha256/v1", caller: "module:ferrata_bridge_ui" };
                if (!noEntropy) r.entropy = "33".repeat(32);
                return reply(r);
            }
            if (method === "open_network") {
                opens += 1;
                if (params.phrase !== undefined) return refuse("refused: a phrase in clear");
                var viewPk = Seal.unhex(params.view_key), modulePk = Seal.unhex(publicKey);
                keys = Seal.derive(Seal.x25519(secret, viewPk), viewPk, modulePk);
                var summary = { network: "Ferrata's test network (stand-in)", address: params.network, account0: account0, checks: ["(a stand-in)"],
                                public_accounts: [{ index: 0, id: account0 }], depositor: null };
                if (params.generate) {
                    var words = Seal.utf8("legal winner thank year wave sausage worth useful legal winner thank yellow");
                    Seal.streamXor(keys.wordsEnc, words);
                    summary.words_sealed = { ct: Seal.hex(words), mac: Seal.hex(Seal.hmac(keys.wordsMac, words)) };
                } else {
                    var ct = Seal.unhex(params.phrase_sealed.ct);
                    if (Seal.hex(Seal.hmac(keys.phraseMac, ct)) !== params.phrase_sealed.mac) return refuse("the sealed phrase does not verify");
                    Seal.streamXor(keys.phraseEnc, ct);
                    var s = ""; for (var k = 0; k < ct.length; k++) s += String.fromCharCode(ct[k]);
                    sealedBytes = ct.length;
                    opened = s.replace(/ +$/, "");
                }
                lastN = 0;
                if (loseOpen) { open = true; return JSON.stringify({ error: "timeout", message: "the call timed out after 20000 ms" }); }
                open = true;
                return reply(summary);
            }
            if (method === "close") { open = false; keys = null; return JSON.stringify({ ok: true, result: { closed: true, keys: "wiped" } }); }
            if (method === "info") return reply({ network: "stand-in", chain_id: 11155111, eth_rpc: "https://rpc.example", sequencer: "https://n.example/rpc",
                                                  user: { depositor: null, public_accounts: [], private_accounts: [] },
                                                  network_session: { public_accounts: [account0] }, backend: "ferrata_bridge_module (stand-in)" });
            if (method === "registry") return reply({ tokens: [{ symbol: "ETH", decimals: 18, registered: true, deposits_enabled: true, escrowed: "0", token: "0x0000000000000000000000000000000000000000",
                                                                denominations: [{ amount: "1000000000000000", mint_fee: "0", relay_fee: "2000000000000", burn_fee: "0", checkpoint_fee: "0" }],
                                                                vault_deposit_cap: { available: "1", capacity: "1", refill_per_second: "0" },
                                                                vault_release_cap: { available: "1", capacity: "1" }, lez: null }],
                                                      vault: { deposits: 1, checkpoints: 0, retired: false, deposits_frozen_until: 0, releases_frozen_until: 0 } });
            if (method === "price") return reply({ denomination: "1000000000000000", mint_fee: "0", relay_fee: "2000000000000", total: "1002000000000000", schedule: 0 });
            if (method === "scan") return reply({ deposits: [{ leaf_index: 0, j: 0, amount: "1000000000000000", kind: "public", account: 0, state: "claimable", nullifier: "0x" + "ab".repeat(32), dest: "0x" + "cd".repeat(32) }],
                                                  burns: [], holdings: [{ kind: "public", account: 0, balance: "1000000000000000" }], vault_deposits: 1 });
            if (method === "claim" || method === "burn") {
                pendingKind = method;
                var rec = { pending_id: "p-" + method, kind: method, what: method === "claim" ? "Claim of leaf 0" : "Burn (public) of /0 with tag counter 0",
                            payer: "public /0 (" + account0 + ")", signers: ["public /0 (" + account0 + ")"], gas_limit: 1000000, max_fee: "64032000", fee_unit: "10^-9 LGO",
                            payer_balance: "10000000000", message_hash: "0x" + "5e".repeat(32), signed: false, account: 0, amount: params.amount || "1000000000000000" };
                if (method === "burn") { rec.recipient = params.recipient; rec.recipient_note = "check it is yours"; rec.i = 0; }
                else { rec.leaf_index = params.leaf_index; rec.destination = "your public account /0"; }
                return reply({ to_approve: rec });
            }
            if (method === "confirm") {
                var res = pendingKind === "burn"
                    ? { settled: true, private: false, what: "Burn (public) of /0 (block 6, transaction 0x01)", i: 0, account: 0, amount: "1000000000000000", recipient: "0x9858EfFD232B4033E47d90003D41EC34EcaEda94", release_delay_ms: 0, balance: "0", approved: { pending_id: params.pending_id } }
                    : { settled: true, private: false, what: "Claim of leaf 0 (block 6, transaction 0x02)", account: 0, nullifier: "0x" + "ab".repeat(32), balance: "1000000000000000", approved: { pending_id: params.pending_id } };
                jobId += 1;
                return reply({ job: { id: jobId, kind: pendingKind, state: "settled", result: res, error: null, elapsed_ms: 10, phases: [] }, started: true });
            }
            if (method === "faucet") return reply({ account: 0, balance: "10000000000", balance_before: "0", tx_hash: "0x03", seen: true });
            if (method === "balances") return reply({ accounts: [{ index: 0, balance: "10000000000" }] });
            return refuse("unknown method " + method);
        }
    }

    TextInput { id: clip; visible: false }

    TestCase {
        name: "FerrataNetworkSession"
        when: windowShown
        readonly property string phrase: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"

        function initTestCase() {
            var c = Qt.createComponent(Qt.resolvedUrl("../qml/Main.qml"));
            if (c.status === Component.Error) fail(c.errorString());
            host.app = c.createObject(host);
            verify(host.app !== null, "Main.qml created");
            verify(host.app.inLogos, "the view sees the host's bridge (logos)");
            compare(host.app.transport, "module");
        }
        function el(name) { var it = findChild(host.app, name); verify(it !== null, "found " + name); return it; }
        function click(name) { wait(30); var it = el(name); verify(it.visible, name + " visible"); verify(it.enabled, name + " enabled"); mouseClick(it); }
        function tab(name) { click("tab." + name); wait(60); }
        function texts(item, out) {
            if (item.text !== undefined && typeof item.text === "string") out.push(item.text);
            var kids = item.children || [];
            for (var i = 0; i < kids.length; i++) texts(kids[i], out);
            return out;
        }
        function typePhrase(p) {
            mouseClick(el("net.phrase"));
            for (var i = 0; i < p.length; i++) keyClick(p.charAt(i));
        }
        function openWith(p) {
            el("net.address").text = "https://n.example";
            typePhrase(p);
            click("net.open");
            tryVerify(function () { return host.app.netOpen; }, 5000, "the session opened");
        }
        function closeIt() { if (host.app.netOpen) click("net.close"); tryVerify(function () { return !host.app.netOpen; }, 3000); }

        // L1: every label of the view shows its text as plain text (controls' own content items aside).
        function test_00_every_label_is_plain_text() {
            var bad = [], checked = 0;
            var walk = function (it) {
                // A control's own content (a button's icon label, a check box's text, a text field's placeholder) is the
                // control's, with texts of this view's own: not walked.
                var p = it.parent;
                if (p && p.padding !== undefined && (p.contentItem === it || p.placeholderText !== undefined)) return;
                if (it.textFormat !== undefined && it.text !== undefined) {
                    checked += 1;
                    if (it.textFormat !== Text.PlainText) bad.push(String(it.objectName || it.text).slice(0, 30));
                }
                var kids = it.children || [];
                for (var i = 0; i < kids.length; i++) walk(kids[i]);
            };
            walk(host.app);
            verify(checked > 60, "labels walked: " + checked);
            compare(bad.length, 0, "labels not plain text, e.g.: " + bad.slice(0, 3).join(" | "));
        }

        // M2 and M1: typed, the phrase is never a string of the view; opening seals it, and the buffer is wiped before
        // the module answers; the stand-in module opens exactly the typed words; open_network carries no phrase.
        function test_01_the_phrase_is_typed_into_bytes_and_sealed() {
            el("net.address").text = "https://n.example";
            typePhrase("Abandon  abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about ");
            compare(host.app.phraseWords, 12);
            compare(host.app.phraseLen, phrase.length + 1, "one space between words, a trailing one");
            var shown = texts(host.app, []).join("\n");
            verify(shown.indexOf("abandon") < 0, "no text of the view holds the phrase");
            var before = logos.requests.length;
            click("net.open");
            tryVerify(function () { return host.app.netOpen; }, 5000, "opened");
            compare(host.app.phraseLen, 0);
            var zero = true;
            for (var i = 0; i < host.app.phraseBuf.length; i++) if (host.app.phraseBuf[i] !== 0) zero = false;
            verify(zero, "the typed bytes are wiped");
            compare(logos.opened, phrase, "the module opened exactly the typed words, normalised");
            compare(logos.sealedBytes, 256, "sealed at a fixed length (round 2, L5)");
            for (var k = before; k < logos.requests.length; k++)
                verify(logos.requests[k].indexOf("abandon") < 0, "no request carries the phrase in clear: " + logos.requests[k].slice(0, 120));
            compare(host.app.netAccount0, logos.account0);
            verify(String(host.app.netStatus).indexOf("module:ferrata_bridge_ui") >= 0, "the caller Logos Core named is shown: " + host.app.netStatus);
        }

        // H1: every later call carries the MAC under the session's key, each counter once.
        function test_02_calls_carry_the_sessions_mac() {
            var n = logos.macsChecked;
            tab("positions");
            click("positions.recover");
            tryVerify(function () { return host.app.positions !== null; }, 3000, "positions read");
            verify(logos.macsChecked > n, "the module checked the MACs");
            verify(host.app.netN === logos.lastN, "the view's counter is the module's last");
        }

        // H1: a reply naming another session's /0 (a session not this view's) is refused by the view.
        function test_03_a_reply_from_another_session_is_refused() {
            logos.answerAs = "Ax1xBfb3Y4TVnotthesameaccount";
            tab("registry");
            click("registry.refresh");
            tryVerify(function () { return !host.app.lastOk; }, 3000);
            verify(String(host.app.lastError).indexOf("not the one this view opened") >= 0, host.app.lastError);
            logos.answerAs = "";
        }

        // M4: no deposit is sent from the network session; the panel names the web wallet.
        function test_04_deposits_point_to_the_web_wallet() {
            tab("deposit");
            verify(!el("deposit.review").enabled, "the deposit button is off");
            verify(el("deposit.net").visible && el("deposit.net").text.indexOf("web wallet") >= 0, el("deposit.net").text);
        }

        // H1 and M3: the burn's address starts empty (nothing made up); the prompt shows the module's record; Approve
        // alone sends confirm, with that record's id.
        function test_05_a_burn_is_shown_then_confirmed() {
            host.app.connectedAccount = logos.account0;
            tab("redeem");
            compare(el("burn.recipient").text, "", "no address made up");
            verify(!el("burn.fresh").visible, "no fresh random address in the network session");
            click("burn.kind.public");
            click("burn.public.ack");
            verify(!el("burn.submit").enabled, "a burn needs an address first");
            el("burn.recipient").forceActiveFocus();
            el("burn.recipient").text = "";
            var a = "0x9858EfFD232B4033E47d90003D41EC34EcaEda94";
            for (var i = 0; i < a.length; i++) keyClick(a.charAt(i));
            keyClick(Qt.Key_Return);
            compare(host.app.recipient, a);
            click("burn.submit");
            tryVerify(function () { return host.app.approval !== null; }, 3000, "the approval prompt");
            var rows = host.app.approval.rows.map(function (r) { return r[0] + ": " + r[1]; }).join("\n");
            verify(rows.indexOf(a) >= 0 && rows.indexOf("Message hash") >= 0 && rows.indexOf("LEZ fee") >= 0, rows);
            verify(logos.methods.indexOf("confirm") < 0, "nothing confirmed before the approval");
            click("approval.approve");
            tryVerify(function () { return host.app.burnText !== ""; }, 3000);
            var sent = logos.requests.filter(function (r) { return r.indexOf("confirm") >= 0; });
            compare(sent.length, 1, "one confirm");
            verify(sent[0].indexOf("p-burn") >= 0, sent[0].slice(0, 200));
            verify(host.app.burnText.indexOf("Burned") === 0, host.app.burnText);
        }

        // H1: the claim, the same way.
        function test_06_a_claim_is_shown_then_confirmed() {
            tab("claim");
            click("claim.pick.0");
            click("claim.mode.plain");
            click("claim.plain.ack");
            click("claim.submit");
            tryVerify(function () { return host.app.approval !== null; }, 3000, "the approval prompt");
            compare(host.app.approval.title, "Plaintext claim on LEZ");
            click("approval.approve");
            tryVerify(function () { return host.app.claimText !== ""; }, 3000);
            verify(host.app.claimText.indexOf("Claimed") === 0, host.app.claimText);
        }

        // Close: the module closes, the view wipes its key and shows nothing of the session.
        function test_07_close_wipes_the_views_side() {
            var k = host.app.netKeys.call;
            closeIt();
            compare(host.app.netKeys, null);
            var zero = true;
            for (var i = 0; i < k.length; i++) if (k[i] !== 0) zero = false;
            verify(zero, "the call key is wiped");
            compare(host.app.recipient, "");
        }

        // M1: a new phrase's words come back sealed and are shown once.
        function test_08_a_new_phrase_comes_back_sealed() {
            el("net.address").text = "https://n.example";
            click("net.generate");
            tryVerify(function () { return host.app.netOpen; }, 5000, "opened");
            verify(el("net.words").text.indexOf("1. legal") === 0, el("net.words").text);
            click("net.wordsDone");
            compare(host.app.netWordsText, "");
            closeIt();
        }

        // L2: open_network's answer lost (Basecamp's 20 s): the view closes the session with its key.
        function test_09_a_lost_open_is_closed() {
            logos.loseOpen = true;
            openWithoutWait();
            tryVerify(function () { return !logos.open; }, 5000, "the view closed it");
            logos.loseOpen = false;
            compare(logos.methods[logos.methods.length - 1], "close");
            verify(!host.app.netOpen);
        }
        // M1: Qt's debug logging on: said what its log holds, New phrase off; typing and pasting the phrase both work.
        function test_10_with_qt_debug_logging_the_view_says_what_its_log_holds() {
            logos.debugLog = true;
            host.app.resumeJobs();
            tryVerify(function () { return host.app.qtDebugLog; }, 2000, "the module said so");
            verify(el("net.qtlog").visible && el("net.qtlog").text.indexOf("holds this session's calls in clear") >= 0, el("net.qtlog").text);
            verify(!el("net.generate").enabled, "no New phrase while Qt debug logging is on (round 2, MF2)");
            el("net.address").text = "https://n.example";
            typePhrase("abandon");
            compare(host.app.phraseLen, 7, "typed (its keys are kept from Qt's key logging: test_12)");
            click("net.phraseClear");
            clip.text = phrase; clip.selectAll(); clip.copy(); clip.text = "";
            click("net.paste");
            compare(host.app.phraseWords, 12, "pasted");
            click("net.phraseClear");
            logos.debugLog = false;
            host.app.resumeJobs();
            tryVerify(function () { return !host.app.qtDebugLog; }, 2000);
            verify(!el("net.qtlog").visible);
        }
        // Round 2, MF2: without the module's 32 bytes of randomness the view makes no key and opens nothing.
        function test_11_no_open_without_the_modules_randomness() {
            logos.noEntropy = true;
            var opensBefore = logos.opens;
            el("net.address").text = "https://n.example";
            typePhrase(phrase);
            click("net.open");
            tryVerify(function () { return !host.app.netOpening; }, 3000);
            verify(!host.app.netOpen && String(host.app.netStatus).indexOf("the module's key is not usable") >= 0, host.app.netStatus);
            compare(logos.opens, opensBefore, "no open_network sent");
            logos.noEntropy = false;
        }
        // Round 2, L1: the phrase field takes each key's shortcut override, so the key never goes through Qt's shortcut
        // map (whose debug logging writes its text): a shortcut on the same key does not fire, and the key reaches the field.
        function test_12_the_phrase_fields_keys_never_reach_the_shortcut_map() {
            var sc = Qt.createQmlObject('import QtQuick; Shortcut { property int hits: 0; sequence: "q"; context: Qt.ApplicationShortcut; onActivated: hits += 1 }', host);
            host.app.phraseClear();
            mouseClick(el("net.phrase"));
            keyClick("q");
            compare(sc.hits, 0, "the shortcut map never saw the key");
            compare(host.app.phraseLen, 1, "the field got it");
            host.app.phraseClear();
            sc.destroy();
        }
        function openWithoutWait() {
            el("net.address").text = "https://n.example";
            typePhrase(phrase);
            click("net.open");
        }
    }
}
