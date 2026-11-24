// RFP-022 U.9 inside Logos Core (e2e/endpoints/run.sh): the mini-app's Endpoints tab driven headless by Qt Quick Test,
// with ferrata_bridge_module and eth_rpc_module loaded in one headless Logos Core behind the view (the runner main.cpp:
// `logos.callModule` is `logosctl call`; `seqnet.ops` runs e2e/endpoints/ops.sh, which reads each endpoint's owner
// directly). "Apply and test" asks the module to switch (wallet set_endpoints); the module switches each endpoint where
// its connection is held (the chain service seqnet-chain: the LEZ sequencer, and Ethereum on the plain path; Logos
// Core's eth_rpc_module: Ethereum on the session's `eth` path), only after the new one has answered for the deployment.
//
// test_1_local: the session on a local zone of four LEZ sequencers (the bridge's configuration on it) and a local anvil,
//   the plain path. Switches between sequencers 0, 1 and 2 and between two anvil endpoints of the same chain; refusals
//   (a closed sequencer port, a closed Ethereum port, a pair of which one is bad; with U9_PUBLIC=1 also another zone's
//   sequencer: our public network, Sepolia's public RPC: no vault there, Hoodi's: another chain), each keeping both
//   endpoints in use, read back from the chain service itself.
// test_2_live (U9_PUBLIC=1, read only): the live deployment (the vault on Sepolia, its configuration on our public LEZ
//   network), Ethereum through eth_rpc_module: its endpoint for chain 11155111 switched between two public Sepolia RPCs
//   and back; refusals (Hoodi's RPC, a closed port, a local sequencer whose zone lacks the configuration), each keeping
//   the endpoints, read back from eth_rpc_module and the chain service.
import QtQuick
import QtTest

Item {
    id: host
    width: 1180
    height: 1500
    property var app: null

    TestCase {
        name: "FerrataEndpointsLogos"
        when: windowShown

        function env(n) { return seqnet.env(n); }
        function wallet(method, params) {
            return JSON.parse(logos.callModule("ferrata_bridge_module", "wallet", [JSON.stringify({ method: method, params: params || {} })]));
        }
        function el(name) { var it = findChild(host.app, name); verify(it !== null, "found " + name); return it; }
        function idle() { tryVerify(function () { return host.app.busy === 0; }, 180000, "calls finished"); }
        function click(name) { wait(80); var it = el(name); verify(it.visible, name + " visible"); verify(it.enabled, name + " enabled"); mouseClick(it); }
        function shot(name) { idle(); wait(200); var img = grabImage(host.app); img.save(env("SEQNET_SHOTS") + "/" + name + ".png"); }
        function has(text, part) { return String(text).indexOf(part) >= 0; }
        // An owner's own record of its endpoints (e2e/endpoints/ops.sh): the chain service's or eth_rpc_module's.
        function owner(args) {
            var rc = seqnet.ops(args);
            verify(rc === 0, "ops " + args + ": " + seqnet.last());
            var lines = String(seqnet.last()).trim().split("\n");
            return JSON.parse(lines[lines.length - 1]);
        }
        // The fields as the person types them, then "Apply and test"; the status line it leaves.
        function apply(eth, seq) {
            el("settings.ethRpc").text = eth;
            el("settings.sequencer").text = seq;
            host.app.endpointsEdited = true;   // what typing does (textEdited)
            click("settings.apply");
            idle();
            var st = el("settings.status").text;
            console.log("APPLY eth=" + eth + " sequencer=" + seq + " -> " + st);
            return st;
        }
        function inUse() {
            click("settings.reload");
            idle();
            return { eth: el("settings.ethRpc").text, seq: el("settings.sequencer").text };
        }

        function initTestCase() {
            // The host opens the module's wallet session at the local deployment's chain service.
            var r = wallet("open", { chain: env("U9_CHAIN_LOCAL") });
            verify(r.ok, "the session on the local deployment: " + r.error);
            compare(r.result.config_id, env("U9_LOCAL_CID"), "the session's configuration");
            var c = Qt.createComponent(Qt.resolvedUrl("../qml/Main.qml"));
            if (c.status === Component.Error) fail(c.errorString());
            host.app = c.createObject(host);
            verify(host.app !== null, "Main.qml created");
            verify(host.app.inLogos, "inside Logos: the host's logos bridge is present");
            compare(host.app.transport, "module", "the view calls ferrata_bridge_module");
        }

        function test_1_local() {
            click("tab.settings");
            wait(150);
            var u = inUse();
            compare(u.eth, env("U9_ETH_A"), "the Ethereum RPC in use, from the module");
            compare(u.seq, env("U9_SEQ0"), "the sequencer in use, from the module");
            verify(has(el("settings.via").text, "chain service"), el("settings.via").text);
            shot("01-local-in-use");

            var st = apply(env("U9_ETH_B"), env("U9_SEQ1"));
            verify(has(st, "Switched the Ethereum RPC and the LEZ sequencer") && has(st, "Connected: chain 11155111"), st);
            var o = owner("chain 18700");
            compare(o.sequencer, env("U9_SEQ1"), "the chain service reads LEZ through sequencer 1");
            compare(o.eth_rpc, env("U9_ETH_B"), "the chain service reads Ethereum through the second anvil");
            compare(host.app.info.endpoints.sequencer.url, env("U9_SEQ1"), "info names the sequencer in use");
            shot("02-local-switched-to-sequencer-1-and-anvil-fork");

            st = apply(env("U9_ETH_A"), env("U9_SEQ2"));
            verify(has(st, "Switched the Ethereum RPC and the LEZ sequencer"), st);
            o = owner("chain 18700");
            compare(o.sequencer, env("U9_SEQ2"));
            compare(o.eth_rpc, env("U9_ETH_A"));

            // Refused: the endpoints in use are kept (read back from the chain service and shown in the fields).
            var refusals = [
                [env("U9_ETH_A"), env("U9_SEQ_CLOSED"), "the LEZ sequencer " + env("U9_SEQ_CLOSED"), "03-local-refused-closed-sequencer"],
                [env("U9_CLOSED"), env("U9_SEQ2"), "the Ethereum RPC " + env("U9_CLOSED"), ""],
                [env("U9_CLOSED"), env("U9_SEQ0"), "the Ethereum RPC " + env("U9_CLOSED"), ""],   // one good, one bad: neither
            ];
            if (env("U9_PUBLIC") === "1") {
                refusals.push([env("U9_ETH_A"), env("U9_LIVE_LEZ"), "holds no configuration", "04-local-refused-another-zone"]);
                refusals.push([env("U9_SEPOLIA_A"), env("U9_SEQ2"), "no contract at the session's vault", ""]);
                refusals.push([env("U9_HOODI"), env("U9_SEQ2"), "serves chain 560048", ""]);
            }
            for (var k = 0; k < refusals.length; k++) {
                st = apply(refusals[k][0], refusals[k][1]);
                verify(has(st, "Not switched") && has(st, refusals[k][2]) && has(st, "Kept"), st);
                o = owner("chain 18700");
                compare(o.sequencer, env("U9_SEQ2"), "the sequencer kept after: " + refusals[k][2]);
                compare(o.eth_rpc, env("U9_ETH_A"), "the Ethereum RPC kept after: " + refusals[k][2]);
                compare(el("settings.sequencer").text, env("U9_SEQ2"), "the field shows the sequencer kept");
                compare(el("settings.ethRpc").text, env("U9_ETH_A"), "the field shows the Ethereum RPC kept");
                if (refusals[k][3] !== "") shot(refusals[k][3]);
            }

            st = apply(env("U9_ETH_A"), env("U9_SEQ0"));
            verify(has(st, "Switched the LEZ sequencer") && !has(st, "the Ethereum RPC and"), st);
            compare(owner("chain 18700").sequencer, env("U9_SEQ0"));
            shot("05-local-back-on-sequencer-0");
        }

        function test_2_live() {
            if (env("U9_PUBLIC") !== "1")
                skip("U9_PUBLIC=1 runs the live deployment's endpoints (public Sepolia RPCs, our public LEZ network), read only");
            var r = wallet("open", { chain: env("U9_CHAIN_LIVE") });
            verify(r.ok, "the session on the live deployment: " + r.error);
            compare(r.result.config_id, env("U9_LIVE_CID"), "the live configuration");
            r = wallet("eth", { via: "eth_rpc_module", chain_id: 11155111, release_sender: "0x000000000000000000000000000000000000bEEF" });
            verify(r.ok, "the session's Ethereum side through Logos Core's eth_rpc_module: " + r.error);
            click("tab.settings");
            wait(150);
            var u = inUse();
            compare(u.eth, env("U9_SEPOLIA_A"), "eth_rpc_module's endpoint for chain 11155111");
            compare(u.seq, env("U9_LIVE_LEZ"), "the chain service's sequencer");
            verify(has(el("settings.via").text, "eth_rpc_module"), el("settings.via").text);
            shot("06-live-in-use");

            var st = apply(env("U9_SEPOLIA_B"), env("U9_LIVE_LEZ"));
            verify(has(st, "Switched the Ethereum RPC") && has(st, "Connected: chain 11155111"), st);
            compare(owner("eth-rpc-module").config.endpoint, env("U9_SEPOLIA_B"), "eth_rpc_module holds the new endpoint");
            compare(owner("chain 18701").eth_rpc, env("U9_SEPOLIA_A"), "on this path the chain service's own Ethereum RPC is not the session's");
            shot("07-live-switched-to-the-second-sepolia-rpc");

            st = apply(env("U9_SEPOLIA_A"), env("U9_LIVE_LEZ"));
            verify(has(st, "Switched the Ethereum RPC"), st);
            compare(owner("eth-rpc-module").config.endpoint, env("U9_SEPOLIA_A"), "and back");

            var refusals = [
                [env("U9_HOODI"), env("U9_LIVE_LEZ"), "serves chain 560048", "08-live-refused-another-chain"],
                [env("U9_CLOSED"), env("U9_LIVE_LEZ"), "the Ethereum RPC " + env("U9_CLOSED"), ""],
                [env("U9_SEPOLIA_A"), env("U9_SEQ1"), "holds no configuration", "09-live-refused-a-local-sequencer"],
            ];
            for (var k = 0; k < refusals.length; k++) {
                st = apply(refusals[k][0], refusals[k][1]);
                verify(has(st, "Not switched") && has(st, refusals[k][2]) && has(st, "Kept"), st);
                compare(owner("eth-rpc-module").config.endpoint, env("U9_SEPOLIA_A"), "eth_rpc_module kept its endpoint after: " + refusals[k][2]);
                compare(owner("chain 18701").sequencer, env("U9_LIVE_LEZ"), "the chain service kept its sequencer after: " + refusals[k][2]);
                if (refusals[k][3] !== "") shot(refusals[k][3]);
            }
        }
    }
}
