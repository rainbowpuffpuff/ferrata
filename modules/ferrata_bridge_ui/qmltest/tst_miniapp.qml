// The mini-app driven headless (Qt Quick Test, offscreen) against the devnet backend
// (cli/examples/miniapp_devnet.rs: anvil + the LEZ harness), screen by screen, with a screenshot of
// each state in results/screenshots/. Run by scripts/box-test.sh from the module's directory.
import QtQuick
import QtTest

Item {
    id: host
    width: 1180
    height: 1500
    property var app: null

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
        function idle() { tryVerify(function () { return host.app.busy === 0; }, 180000, "calls finished"); }
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

        function test_07_private_claim() {
            tab("claim");
            click("claim.relay");
            idle();
            tryVerify(function () { return host.app.positions && host.app.positions.deposits[0].state === "claimable"; }, 60000, "imported");
            click("claim.pick.0");
            verify(!el("claim.submit").enabled, "held by the random delay");
            verify(has(el("claim.leaks.summary").text, "Minimal"), el("claim.leaks.summary").text);
            verify(el("claim.delay").visible, "the delay is explained");
            shot("14-claim-held-by-delay");
            click("claim.skipDelay");
            click("claim.submit");
            approve("Private claim", "15-claim-approval");
            tryVerify(function () { return has(host.app.claimText, "Private claim built"); }, 60000, host.app.claimText);
            shot("16-claim-private-built");
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

        function test_09_error_messages() {
            host.app.call("claim", { leaf_index: 1, private: false });
            idle();
            verify(has(el("error.message").text, "Already claimed"), el("error.message").text);
            verify(!/0x[0-9a-fA-F]{8,}/.test(el("error.detail").text), "no hash in the message: " + el("error.detail").text);
            shot("18-error-already-claimed");
            tab("redeem");
            click("burn.kind.public");
            click("burn.public.ack");
            click("burn.denom.2");
            click("burn.submit");
            approve("Burn", null);
            verify(has(el("error.message").text, "Insufficient balance"), el("error.message").text);
            shot("19-error-insufficient-balance");
        }

        function test_10_burn() {
            click("burn.kind.private");
            click("burn.denom.0");
            click("burn.submit");
            approve("Burn", null);
            verify(has(host.app.burnText, "Private burn not settled here"), host.app.burnText);
            click("burn.kind.public");
            click("burn.public.ack");
            click("burn.submit");
            approve("Burn", "20-burn-approval");
            tryVerify(function () { return has(host.app.burnText, "Burned"); }, 60000, host.app.burnText);
            shot("21-burn-sent");
        }

        function test_11_checkpoint_and_release() {
            click("redeem.checkpoint");
            idle();
            click("redeem.refresh");
            idle();
            click("release.pick.0");
            click("release.submit");
            approve("Release", "22-release-approval");
            tryVerify(function () { return has(host.app.releaseText, "Released:"); }, 60000, host.app.releaseText);
            verify(has(host.app.releaseText.toLowerCase(), host.app.recipient.toLowerCase()), "paid to the fresh recipient");
            shot("23-released");
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
            verify(host.app.lastError !== "", "a cap below the floor is refused");
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
            verify(has(el("error.message").text, "Frozen"), "claim while frozen: " + el("error.message").text + " / " + host.app.lastError);
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
    }
}
