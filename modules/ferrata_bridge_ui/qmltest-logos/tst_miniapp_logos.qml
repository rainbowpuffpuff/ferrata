// The mini-app driven headless (Qt Quick Test, offscreen) with Logos Core behind it, on a running LEZ sequencer
// (e2e/seqnet/hook-miniapp.sh; the runner main.cpp gives the engine `logos`, the host's bridge, whose callModule
// goes to ferrata_bridge_module loaded in a headless Logos Core, and `seqnet`, the operator's and the relayer's steps
// run between the user's actions). The view is Main.qml unchanged in behaviour: inside Logos it calls the module
// (the module transport); every call is the module's `wallet` method, run with the module's own methods over the
// chains the chain service reads and sends to (seqnet-chain, the stand-in for Logos Core's own modules).
// The private round trip on the defaults: private deposit, the relayer's import (operator), a private claim once its
// random delay has passed, a private burn to a fresh address, the checkpoint (operator), the release by a relayer
// once its delays have passed. No default is changed and no delay is overridden; the user's waiting is the view's
// clock moved on (and Ethereum's, anvil's, where the vault counts it).
// With SEQNET_ETH=logos (hook-logoseth.sh) the session's Ethereum side goes through Logos Core's own modules: the vault
// read through eth_rpc_module, the deposit, the release and the vault's admin calls sent through tx_sender_module, each
// approved by the person at Logos's approver evm_signer_cli (ops.sh approve), the keys in keystore_module.
import QtQuick
import QtTest

Item {
    id: host
    width: 1180
    height: 1500
    property var app: null
    property string privateRecipient: ""

    TestCase {
        name: "FerrataMiniAppLogos"
        when: windowShown

        function wallet(method, params) {
            var raw = logos.callModule("ferrata_bridge_module", "wallet", [JSON.stringify({ method: method, params: params || {} })]);
            return JSON.parse(raw);
        }
        function initTestCase() {
            // The host opens the module's wallet session at the chains (in Logos the wallet SDK and Logos Core's
            // own modules would stand here): the module asks the chain service for the wallet's account.
            var r = wallet("open", { chain: seqnet.env("SEQNET_CHAIN_URL") });
            verify(r.ok, "the module's wallet session: " + r.error);
            console.log("SESSION " + JSON.stringify(r.result));
            var lezParams = seqnet.env("SEQNET_LEZ_PARAMS");
            if (lezParams !== "") {
                // LEZ through Logos Core's lez_core: the user's LEZ wallet opened inside it (hook-logoseth.sh).
                var lz = wallet("lez", JSON.parse(lezParams));
                verify(lz.ok, "the session's LEZ path through lez_core: " + lz.error);
                console.log("LEZ " + JSON.stringify(lz.result).substring(0, 400));
            }
            if (logosEth()) {
                var e = wallet("eth", { via: "eth_rpc_module", chain_id: 11155111, from_block: Number(seqnet.env("SEQNET_FROM_BLOCK")),
                                        release_sender: seqnet.env("SEQNET_RELEASE_SENDER"), admin: seqnet.env("SEQNET_ADMIN"),
                                        freeze_authority: seqnet.env("SEQNET_FREEZE_AUTHORITY") });
                verify(e.ok, "the session's Ethereum path through Logos Core: " + e.error);
                console.log("ETH " + JSON.stringify(e.result));
            }
            var c = Qt.createComponent(Qt.resolvedUrl("../qml/Main.qml"));
            if (c.status === Component.Error) fail(c.errorString());
            host.app = c.createObject(host);
            verify(host.app !== null, "Main.qml created");
            verify(host.app.inLogos, "inside Logos: the host's logos bridge is present");
            compare(host.app.transport, "module", "the view calls ferrata_bridge_module");
        }
        function el(name) { var it = findChild(host.app, name); verify(it !== null, "found " + name); return it; }
        // A private transaction proven for real on a CPU takes minutes.
        function idle() { tryVerify(function () { return host.app.busy === 0; }, 1800000, "calls finished"); }
        function click(name) { wait(80); var it = el(name); verify(it.visible, name + " visible"); verify(it.enabled, name + " enabled"); mouseClick(it); }
        function shot(name) { idle(); wait(200); var img = grabImage(host.app); img.save(seqnet.env("SEQNET_SHOTS") + "/" + name + ".png"); }
        function tab(name) { click("tab." + name); wait(150); }
        function has(text, part) { return String(text).indexOf(part) >= 0; }
        function approve(titlePart, shotName) {
            tryVerify(function () { return host.app.approval !== null; }, 30000, "the wallet's approval prompt");
            verify(has(host.app.approval.title, titlePart), "prompt: " + host.app.approval.title);
            if (shotName) shot(shotName);
            click("approval.approve");
            idle();
        }
        function ops(args) { var rc = seqnet.ops(args); verify(rc === 0, "ops " + args + ": " + seqnet.last()); }
        function logosEth() { return seqnet.env("SEQNET_ETH") === "logos"; }
        // The person at the Logos signer: the view shows the request its send waits for; the approver approves it.
        function signer(name, shotName) {
            if (!logosEth()) return;
            tryVerify(function () { return host.app.approvalText !== ""; }, 120000, "the send waits for the Logos signer: " + host.app.lastError);
            verify(has(el("header.job").text, "Approve this"), el("header.job").text);
            if (shotName) shot(shotName);
            ops("approve " + name);
        }
        // The user waits: the view's clock moves on to just past `until` (ms).
        function waitUntil(until) { var ms = Math.max(0, until - host.app.now()) + 1000; host.app.clockOffset += ms; return ms; }

        function test_01_connect() {
            shot("01-start");
            verify(!el("deposit.review").enabled, "no deposit before the wallet is connected");
            click("wallet.connect");
            tryVerify(function () { return host.app.selecting; }, 5000);
            idle();
            verify(host.app.info !== null && has(host.app.info.network, "LEZ"), "info from the module: " + JSON.stringify(host.app.info));
            shot("02-wallet-account-selection");
            click("wallet.account.0");
            tryVerify(function () { return host.app.connectedAccount !== ""; }, 5000);
            idle();
        }

        function test_02_registry() {
            tab("registry");
            click("registry.refresh");
            idle();
            verify(host.app.registry !== null, "registry: " + host.app.lastError);
            verify(has(el("registry.token.0").text, "ETH  registered"), el("registry.token.0").text);
            // The bridge's configuration and token reach LEZ with the operator's first import (Init, Register).
            verify(!el("registry.lez.0").visible, "no LEZ side before the operator's first import");
            shot("03-registry-before-the-first-import");
        }

        function test_03_deposit_private_by_default() {
            tab("deposit");
            verify(host.app.destKind === "private" && el("deposit.kind.private").checked, "private destination pre-selected");
            verify(host.app.delayOn && host.app.relayerOn, "delay and relayer on by default");
            verify(has(el("deposit.leaks.summary").text, "Minimal"), el("deposit.leaks.summary").text);
            shot("04-deposit-defaults");
            click("deposit.review");
            approve("Deposit", "05-deposit-approval");
            signer("deposit", "05b-deposit-awaits-the-logos-signer");
            tryVerify(function () { return has(host.app.depositText, "Deposited"); }, 120000, "deposit sent: " + host.app.lastError);
            verify(has(host.app.depositText, "private account"), host.app.depositText);
            console.log("DEPOSIT " + host.app.depositText);
            shot("06-deposit-private-sent");
        }

        function test_04_positions_before_import() {
            tab("positions");
            click("positions.recover");
            idle();
            compare(host.app.positions.deposits.length, 1, "the deposit found from the seed");
            compare(host.app.positions.deposits[0].state, "waiting for the root import");
            shot("07-positions-before-import");
        }

        function test_05_private_claim() {
            // The relayer's import, as on a real network: not the user's step (the view shows no button for it in
            // Logos). The operator's side, then the gas drop's stand-in (native LEZ for the private account's fees).
            ops("import");
            ops("fund");
            tab("registry");
            click("registry.refresh");
            idle();
            verify(el("registry.lez.0").visible, "the LEZ side of the token is shown (token(cid, ETH) decoded by the module)");
            shot("07b-registry-after-the-import");
            tab("positions");
            click("positions.recover");
            idle();
            tryVerify(function () { return host.app.positions && host.app.positions.deposits[0].state === "claimable"; }, 60000, "imported");
            tab("claim");
            click("claim.pick.0");
            verify(host.app.claimMode === "private" && el("claim.mode.private").checked, "private claim pre-selected");
            verify(!el("claim.submit").enabled, "held by the random delay");
            verify(el("claim.delay").visible, "the delay is explained");
            verify(has(el("claim.leaks.summary").text, "Minimal"), el("claim.leaks.summary").text);
            shot("08-claim-held-by-delay");
            var waited = waitUntil(host.app.heldUntil[0]);
            console.log("WAITED claim delay " + Math.round(waited / 60000) + " min (the view's clock)");
            tryVerify(function () { return !host.app.held(0); }, 10000, "the delay has passed");
            verify(!host.app.skipDelay, "the delay was waited out, not overridden");
            verify(el("claim.submit").enabled, "claimable once the delay has passed");
            shot("09-claim-delay-passed");
            click("claim.submit");
            approve("Private claim", "10-claim-approval");
            tryVerify(function () { return has(host.app.claimText, "Claimed privately"); }, 1800000, "claim: " + host.app.claimText + " " + host.app.lastError);
            var r = host.app.claimReply;
            verify(r.settled && r.private, "settled as a private transaction on the sequencer");
            compare(r.balance, host.app.positions.deposits[0].amount, "the denomination is held privately by /0");
            console.log("CLAIM " + JSON.stringify({ tx: r.tx_hash, block: r.block, proof: r.proof, variant: r.variant }));
            click("claim.refresh");
            idle();
            tryVerify(function () { return host.app.positions.deposits[0].state === "claimed"; }, 60000, "the deposit shows as claimed");
            compare(host.app.holding("private"), r.balance, "the private holding /0");
            shot("11-claim-private-settled");
        }

        function test_06_private_burn_on_defaults() {
            tab("redeem");
            click("burn.kind.private");
            verify(host.app.burnKind === "private" && el("burn.kind.private").checked, "private holder selected (the default)");
            click("burn.denom.0");
            verify(host.app.releaseDelayOn && el("burn.delay").checked, "the release's random delay on by default");
            host.privateRecipient = host.app.recipient;
            verify(host.privateRecipient.toLowerCase() !== String(host.app.info.user.depositor).toLowerCase(), "a fresh recipient generated");
            verify(has(el("burn.leaks.summary").text, "Minimal"), el("burn.leaks.summary").text);
            shot("12-burn-private-defaults");
            click("burn.submit");
            approve("Burn", "13-burn-approval");
            tryVerify(function () { return has(host.app.burnText, "Burned privately"); }, 1800000, "burn: " + host.app.burnText + " " + host.app.lastError);
            var r = host.app.burnReply;
            verify(r.settled && r.private, "settled as a private transaction on the sequencer");
            compare(r.burn_index, 0, "the first burn");
            compare(r.balance, "0", "the private holding is spent");
            compare(r.recipient.toLowerCase(), host.privateRecipient.toLowerCase(), "to the fresh recipient");
            console.log("BURN " + JSON.stringify({ tx: r.tx_hash, block: r.block, proof: r.proof }));
            shot("14-burn-private-settled");
        }

        function test_07_checkpoint_and_private_release() {
            // The checkpoint prover and the relayer: the operator's side (on a real network the user only waits).
            ops("checkpoint");
            click("redeem.refresh");
            idle();
            compare(host.app.positions.burns.length, 1, "the private burn found from the seed");
            click("release.pick.0");
            verify(!el("release.submit").enabled, "the private burn's release is held by its random delay");
            verify(el("release.delay").visible, "the delay is explained");
            shot("15-release-held-by-delay");
            var waited = waitUntil(host.app.releaseHeldUntil[0]);
            console.log("WAITED release delay " + Math.round(waited / 60000) + " min (the view's clock)");
            // The vault's own release delay after the checkpoint, on Ethereum's clock.
            ops("pass-time 121 the vault release delay after finalizeAttested");
            tryVerify(function () { return !host.app.releaseHeld(0); }, 10000, "the delay has passed");
            verify(!host.app.releaseSkipDelay, "the delay was waited out, not overridden");
            verify(el("release.submit").enabled, "releasable once the delay has passed");
            click("release.submit");
            approve("Release", "16-release-approval");
            signer("release", "16b-release-awaits-the-logos-signer");
            tryVerify(function () { return has(host.app.releaseText, "Released:"); }, 600000, "release: " + host.app.releaseText + " " + host.app.lastError);
            verify(has(host.app.releaseText.toLowerCase(), host.privateRecipient.toLowerCase()), "paid to the private burn's fresh recipient: " + host.app.releaseText);
            console.log("RELEASE " + host.app.releaseText);
            shot("17-released-private-round-trip");
            tab("positions");
            click("positions.recover");
            idle();
            verify(has(el("positions.summary").text, "0 not released"), el("positions.summary").text);
            verify(has(el("positions.summary").text, "0 not claimed"), el("positions.summary").text);
            shot("18-positions-after-private-round-trip");
        }

        // The vault's admin functions through the module and Logos Core's send stack (SEQNET_ETH=logos): the admin role
        // the deployer offered is accepted from the admin's account, the cap set from the admin tab, ETH deposits frozen
        // and the freeze lifted by the freeze authority; each send approved at the Logos signer.
        function test_07b_vault_admin_through_the_send_stack() {
            if (!logosEth()) skip("the vault's admin through the send stack runs with SEQNET_ETH=logos");
            var a = wallet("admin.vault", { action: "accept_admin", params: {} });
            verify(a.ok, "accept_admin started: " + a.error);
            ops("approve accept-admin");
            tryVerify(function () { var j = wallet("job", { id: a.result.job.id }); return j.ok && host.app.jobOver(j.result); }, 120000, "accept_admin done");
            var j = wallet("job", { id: a.result.job.id }).result;
            compare(j.state, "settled", "the admin role accepted through the send stack: " + j.error);
            tab("admin");
            click("admin.refresh");
            idle();
            tryVerify(function () { return host.app.admin && host.app.admin.vault_roles; }, 30000, "the vault's roles read through eth_rpc_module");
            compare(String(host.app.admin.vault_roles.admin).toLowerCase(), seqnet.env("SEQNET_ADMIN").toLowerCase(), "the admin is the keystore's account");
            shot("20-admin-vault-roles");
            click("admin.vault.setcap");
            approve("setTokenCap", "21-admin-setcap-approval");
            signer("admin-setcap", "22-admin-setcap-awaits-the-logos-signer");
            tryVerify(function () { return has(host.app.adminText, "Vault cap set"); }, 120000, "cap: " + host.app.adminText + " " + host.app.lastError);
            click("admin.vault.freeze");
            approve("setTokenDepositsFrozen", "");
            signer("admin-freeze", "");
            tryVerify(function () { return has(host.app.adminText, "ETH deposits frozen"); }, 120000, "freeze: " + host.app.adminText + " " + host.app.lastError);
            idle();
            tryVerify(function () { return host.app.admin && host.app.admin.vault && host.app.admin.vault.token.deposits_frozen_until !== "0"; }, 30000, "the freeze read back");
            shot("23-admin-vault-frozen");
            // A deposit while frozen: refused by the module before anything is signed (the vault's error by eth_call).
            tab("deposit");
            click("deposit.review");
            approve("Deposit", "");
            tryVerify(function () { return host.app.ethJob && host.app.jobOver(host.app.ethJob); }, 60000, "the frozen deposit's job ended");
            verify(has(host.app.lastError, "TokenDepositsAreFrozen"), "refused before signing: " + host.app.lastError);
            shot("24-deposit-refused-while-frozen");
            tab("admin");
            click("admin.vault.unfreeze");
            approve("setTokenDepositsFrozen(false)", "");
            signer("admin-unfreeze", "");
            tryVerify(function () { return has(host.app.adminText, "accepted again"); }, 120000, "unfreeze: " + host.app.adminText + " " + host.app.lastError);
            shot("25-admin-vault-unfrozen");
        }

        // The module's jobs (ferrata_bridge_module calls/src/jobs.rs): the claim and the burn above were jobs the view
        // started and polled. Then: a view opened afresh finds them (the burn's with its release delay); an unknown job
        // id; a failed job's error reaching the view.
        function test_08_jobs() {
            var l = wallet("jobs", {});
            verify(l.ok, "jobs: " + l.error);
            var kinds = l.result.jobs.map(function (j) { return j.kind + ":" + j.state; });
            console.log("JOBS " + JSON.stringify(kinds));
            verify(kinds.indexOf("claim:settled") >= 0 && kinds.indexOf("burn:settled") >= 0, "the claim's and the burn's jobs settled: " + kinds);
            // A second view, its state empty, as a view opened again: it picks the jobs up from the module.
            var c = Qt.createComponent(Qt.resolvedUrl("../qml/Main.qml"));
            var v2 = c.createObject(host, { visible: false });
            verify(v2 !== null && v2.claimJob === null && v2.burnJob === null && v2.burnText === "", "a fresh view knows no job");
            v2.resumeJobs();
            tryVerify(function () { return v2.burnJob !== null && v2.claimJob !== null; }, 30000, "the fresh view found the jobs");
            compare(v2.burnJob.state, "settled", "the burn's job, finished, found again");
            verify(has(v2.burnText, "Burned privately"), "its reply applied: " + v2.burnText);
            verify(v2.burnReply.release_delay_ms > 0, "the burn's release delay came back with the job: " + v2.burnReply.release_delay_ms);
            compare(v2.releaseHeldUntil[0], v2.burnJob.finished_ms + v2.burnReply.release_delay_ms, "the release held from the burn's settlement for its own delay");
            verify(has(v2.claimText, "Claimed privately"), "the claim's reply applied: " + v2.claimText);
            v2.destroy();
            // An unknown job id: the module says so; the view, polling it, ends the job with that answer.
            var r = wallet("job", { id: 987654 });
            verify(!r.ok && has(r.error, "no job 987654 in this module"), "unknown id: " + r.error);
            host.app.adoptJob({ id: 987654, kind: "claim", state: "proving", elapsed_ms: 0 }, false);
            tryVerify(function () { return host.app.claimJob.state === "failed"; }, 30000, "the view's poll of an unknown job");
            verify(has(host.app.lastError, "no job 987654"), "its error reached the view: " + host.app.lastError);
            verify(has(el("error.message").text, "Unknown job"), el("error.message").text);
            // A failed job's error reaches the view: a claim of a position that is not this wallet's.
            tab("claim");
            host.app.claimLeaf = 99;
            click("claim.submit");
            approve("claim", "");
            tryVerify(function () { return host.app.claimJob && host.app.claimJob.id !== 987654 && host.app.jobOver(host.app.claimJob); }, 60000, "the failing claim's job ended");
            compare(host.app.claimJob.state, "failed", "the job failed");
            verify(has(host.app.lastError, "not a deposit of this wallet"), "the job's error reached the view: " + host.app.lastError);
            verify(el("error.banner").visible, "shown in the error banner");
            verify(has(el("claim.job").text, "failed"), el("claim.job").text);
            compare(host.app.claimText, "", "no claim text for a failed claim");
            shot("19-a-failed-job-reaches-the-view");
            // Every call of the view returned at once: the longest.
            var longest = 0;
            for (var i = 0; i < host.app.callLog.length; i++) longest = Math.max(longest, host.app.callLog[i].ms);
            console.log("CALLS " + host.app.callLog.length + " longest " + longest + " ms");
        }
    }
}
