// The mini-app's labels (Main.qml inside Logos, a stand-in host bridge), headless (Qt Quick Test, offscreen): review
// of 24 Nov 2026, L1: every label shows its text as plain text (a check that fails on the view before the fix).
// Run as `qmltestrunner -input qmltest/tst_network.qml` from the module's directory (scripts/network-ui-test.sh).
import QtQuick
import QtTest

Item {
    id: host
    width: 1180
    height: 1600
    property var app: null

    // The host's bridge as the view sees it in Basecamp (`logos.callModuleAsync`): no module behind it.
    QtObject {
        id: logos
        function callModuleAsync(module, method, args, done, timeoutMs) { done(JSON.stringify({ ok: true, result: { jobs: [] } })); }
    }

    TestCase {
        name: "FerrataLabels"
        when: windowShown

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

    }
}
