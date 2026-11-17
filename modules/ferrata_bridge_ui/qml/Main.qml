// Ferrata Bridge: the Logos mini-app (ui_qml module) of the Ferrata bridge, prototype, not audited.
// Deposit, claim, burn and release; recovery from the seed; the token registry with caps and
// utilisation; the admin functions; what is public and private at each step; the endpoints.
//
// Every call goes through call(): in Logos (a `logos` bridge is present) to the core module
// ferrata_bridge_module, otherwise to the devnet backend over HTTP (cli/examples/miniapp_devnet.rs),
// which serves the same methods from anvil and the LEZ harness. Every transaction is first shown in
// the wallet's approval prompt, the flow of the LP-0021 Wallet Provider SDK: connect, account
// selection, approval, transaction. LP-0021 has no released SDK yet: walletConnect() and propose()
// are the only two places the wallet is reached, and a provider object set as `walletProvider`
// replaces the built-in prompts there (see "wallet" below).
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "js/privacy.js" as Privacy
import "js/format.js" as Fmt

Rectangle {
    id: root
    objectName: "ferrata.root"
    width: 1180
    height: 1500
    color: "#f5f6f8"

    // ---- endpoints (U.09) and transport --------------------------------------------------------
    readonly property bool inLogos: typeof logos !== "undefined" && logos !== null
    property string transport: inLogos ? "module" : "devnet"
    property string backendUrl: "http://127.0.0.1:18600/rpc"
    // How long the view waits for the core module's answer through an asynchronous host bridge.
    property int moduleTimeoutMs: 600000
    property var callLog: []
    // A claim or burn through the module is a job (ferrata_bridge_module, calls/src/jobs.rs): a private one is proven for
    // minutes on a CPU, and Basecamp ends a view's call to a core module at 20 s. The call starts the job and is answered
    // at once; the view keeps the job's record (claimJob, burnJob), polls it every jobPollMs and applies its reply once it
    // is done. A view opened again picks the jobs up (resumeJobs).
    property var claimJob: null
    property var burnJob: null
    property int jobPollMs: 3000
    property var appliedJobs: ({})
    property bool jobPolling: false
    property bool positionsDeferred: false
    property string ethRpc: "http://127.0.0.1:18545"
    property string sequencer: "harness"
    property string endpointStatus: ""

    // ---- state ---------------------------------------------------------------------------------
    property var info: null
    property var registry: null
    property var positions: null
    property var admin: null
    property var lastResult: null
    property string lastMethod: ""
    property bool lastOk: true
    property string lastError: ""
    property int busy: 0
    property int calls: 0

    // ---- wallet (LP-0021-shaped adapter) --------------------------------------------------------
    // The one seam a Wallet Provider SDK replaces. LP-0021 (LEZ Wallet and Provider SDK) has no code
    // yet, so the built-in prompts below play its two calls. Set `walletProvider` to an object with
    //   connect(dapp, accounts, done)   account selection naming the dApp; done(account) or done("")
    //   approve(request, done)          the approval prompt of one transaction; done(true | false)
    // (request: { title, rows: [[label, value]], effects: [text] }) and those are used instead; nothing
    // else in the view changes. The test injects such a provider (test_14).
    property var walletProvider: null
    property string connectedAccount: ""
    property bool selecting: false
    property var approval: null

    // ---- choices: the defaults are the recommended ones (U.05, U.06) ----------------------------
    property string destKind: "private"
    property bool publicAcknowledged: false
    property int denomIndex: 0
    property int destAccount: 0
    property bool delayOn: true
    property int delayHours: 1 + Math.floor(Math.random() * 12)
    // The clock the delays are held against: the wall clock, moved on by the devnet's "let the delay
    // pass" (which also moves anvil's clock); in Logos it is the wall clock alone.
    property real clockOffset: 0
    property bool relayerOn: true
    property var heldUntil: ({})
    property string depositText: ""

    property int claimLeaf: -1
    property string claimMode: "private"
    property bool plainAcknowledged: false
    property bool skipDelay: false
    property string claimText: ""
    property var claimReply: null

    property string burnKind: "private"
    property bool burnPublicAcknowledged: false
    property int burnDenom: 0
    property string recipient: freshAddress()
    property string burnText: ""
    property var burnReply: null
    property bool releaseDelayOn: true
    property int releaseDelayHours: 1 + Math.floor(Math.random() * 12)
    property var releaseHeldUntil: ({})
    property bool releaseSkipDelay: false
    property int releaseBurn: -1
    property string releaseText: ""
    property string adminText: ""

    function freshAddress() {
        var h = "0x";
        for (var i = 0; i < 40; i++) h += "0123456789abcdef".charAt(Math.floor(Math.random() * 16));
        return h;
    }

    // `quiet`: a background call (a job's poll, the jobs listing) that neither marks the view busy nor sets its last
    // result or error; it is still in the call log.
    function call(method, params, done, quiet) {
        if (!quiet) busy += 1;
        calls += 1;
        var t0 = Date.now();
        var finish = function (ok, value) {
            if (!quiet) busy -= 1;
            // The view's own record of its calls (method, outcome, milliseconds), for a test or a support request.
            callLog.push({ method: method, ok: ok, ms: Date.now() - t0, transport: transport, error: ok ? "" : String(value).slice(0, 300) });
            if (callLog.length > 2000) callLog.shift();
            if (!quiet) {
                lastMethod = method;
                lastOk = ok;
                if (ok) { lastResult = value; lastError = ""; } else { lastError = String(value); }
            }
            if (done) done(ok, value);
        };
        if (transport === "module") {
            // One method of the module serves the view: `wallet`, with the call's name and parameters.
            var req = JSON.stringify({ method: method, params: params || {} });
            try {
                // Basecamp's bridge also calls asynchronously: the view then does not hold Basecamp's GUI thread
                // while the module works (a host without it, such as the Qt Quick Test runner, is called synchronously).
                if (typeof logos.callModuleAsync === "function")
                    logos.callModuleAsync("ferrata_bridge_module", "wallet", [req], function (payload) { moduleReply(payload, finish); }, moduleTimeoutMs);
                else
                    moduleReply(logos.callModule("ferrata_bridge_module", "wallet", [req]), finish);
            } catch (e) {
                finish(false, "ferrata_bridge_module: " + e);
            }
            return;
        }
        var xhr = new XMLHttpRequest();
        xhr.onreadystatechange = function () {
            if (xhr.readyState !== XMLHttpRequest.DONE) return;
            if (xhr.status !== 200) { finish(false, "backend unreachable (" + backendUrl + "): connection refused, HTTP " + xhr.status); return; }
            var r;
            try { r = JSON.parse(xhr.responseText); } catch (e) { finish(false, "bad reply: " + e); return; }
            finish(r.ok, r.ok ? r.result : r.error);
        };
        xhr.open("POST", backendUrl);
        xhr.setRequestHeader("Content-Type", "application/json");
        xhr.send(JSON.stringify({ method: method, params: params || {} }));
    }

    // The module's reply as the host hands it over: the reply object, its JSON text (logosctl's result), or that text
    // serialised once more as a JSON string (Basecamp's bridge serialises the module's string result). A host error
    // ({error, module, method, message}: the module not loaded or not reachable, a timeout) is reported as the host's;
    // after a timeout the call's outcome is unknown (the module may still complete it), so the message says to refresh
    // before repeating the step.
    function moduleReply(raw, finish) {
        var r = raw;
        try {
            for (var i = 0; i < 2 && typeof r === "string"; i++) r = JSON.parse(r);
        } catch (e) {
            finish(false, "ferrata_bridge_module: unreadable reply: " + String(raw).slice(0, 200));
            return;
        }
        if (r && r.ok === false) { finish(false, r.error); return; }
        if (r && r.ok === undefined && r.error !== undefined) {
            var what = "Logos host: " + r.error + (r.message ? ": " + r.message : "");
            if (/timed out|timeout/i.test(String(r.error) + " " + String(r.message)))
                what += " (the call's outcome is unknown: refresh the positions before repeating it)";
            finish(false, what);
            return;
        }
        finish(true, (r && r.result !== undefined) ? r.result : r);
    }
    function refreshInfo(done) { call("info", {}, function (ok, v) { if (ok) info = v; if (done) done(ok, v); }); }
    // While a job runs, the module answers neither (they would wait for LEZ's wallet, busy proving): the positions are read
    // once the job is done.
    function refreshRegistry(done) {
        if (jobRunning()) { if (done) done(false, "deferred: a job is running"); return; }
        call("registry", {}, function (ok, v) { if (ok) registry = v; if (done) done(ok, v); });
    }
    function refreshPositions(done) {
        if (jobRunning()) { positionsDeferred = true; if (done) done(false, "deferred: a job is running"); return; }
        call("scan", {}, function (ok, v) { if (ok) positions = v; if (done) done(ok, v); });
    }

    // ---- jobs (the module's claim and burn) ----
    function jobOver(j) { return !!j && (j.state === "settled" || j.state === "failed"); }
    function jobRunning() { return (!!claimJob && !jobOver(claimJob)) || (!!burnJob && !jobOver(burnJob)); }
    // A claim or burn: the module answers with a job, polled until it is done; the devnet backend answers with the
    // step's reply itself. Either way stepDone runs once with the reply.
    function step(kind, params) {
        call(kind, params, function (ok, v) {
            if (ok && v && v.job) adoptJob(v.job, false);
            else stepDone(kind, ok, v, null, false);
        });
    }
    function adoptJob(j, resumed) {
        if (j.kind === "claim") claimJob = j; else if (j.kind === "burn") burnJob = j; else return;
        if (jobOver(j)) applyJob(j, resumed);
    }
    // A finished job's reply, applied once, as the call's own reply was: the step's text, the release's hold.
    function applyJob(j, resumed) {
        if (appliedJobs[j.id]) return;
        var a = {}; for (var k in appliedJobs) a[k] = appliedJobs[k]; a[j.id] = true; appliedJobs = a;
        var hasReply = j.result !== null && j.result !== undefined;
        stepDone(j.kind, hasReply, hasReply ? j.result : j.error, j, resumed);
        if (positionsDeferred && !jobRunning()) { positionsDeferred = false; refreshPositions(); }
    }
    function stepDone(kind, ok, v, job, resumed) {
        // A failed job's error reaches the view's banner, as a failed call's does (not for a job found on reopening).
        if (!ok && job && !resumed) { lastError = String(v); lastOk = false; }
        if (kind === "claim") claimFinished(ok, v, job); else burnFinished(ok, v, job);
    }
    function pollJobs() {
        if (jobPolling) return;
        var j = (claimJob && !jobOver(claimJob)) ? claimJob : ((burnJob && !jobOver(burnJob)) ? burnJob : null);
        if (!j) return;
        jobPolling = true;
        call("job", { id: j.id }, function (ok, v) {
            jobPolling = false;
            if (ok) adoptJob(v, false);
            // The module no longer has it (restarted): the job is over, with the module's answer as its error.
            else if (/no job \d+/.test(String(v))) adoptJob({ id: j.id, kind: j.kind, state: "failed", error: String(v), elapsed_ms: j.elapsed_ms, result: null }, false);
        }, true);
    }
    // A view opened again, or one that missed a reply, picks up the module's newest claim and burn jobs: a running one
    // is polled again, a finished one is applied (its text; the release's hold from the burn's own delay).
    function resumeJobs(done) {
        if (transport !== "module") { if (done) done(false, "no module"); return; }
        call("jobs", {}, function (ok, v) {
            if (ok && v && v.jobs) {
                var newest = {};
                for (var i = 0; i < v.jobs.length; i++) newest[v.jobs[i].kind] = v.jobs[i];
                if (newest.claim && (!claimJob || claimJob.id <= newest.claim.id)) adoptJob(newest.claim, true);
                if (newest.burn && (!burnJob || burnJob.id <= newest.burn.id)) adoptJob(newest.burn, true);
            }
            if (done) done(ok, v);
        }, true);
    }
    function jobText(j) {
        if (!j) return "";
        var all = Math.round((j.elapsed_ms || 0) / 1000), proving = Math.round((j.proving_ms || 0) / 1000);
        var what = "the private " + j.kind;
        if (j.state === "queued") return "Job " + j.id + ": " + what + " is being prepared (the chains read, the transaction built), " + all + " s.";
        if (j.state === "proving") return "Job " + j.id + ": the proof of " + what + " is being made by the LEZ wallet, " + proving + " s so far (" + all + " s in all). It takes minutes on a CPU; this view may be closed meanwhile.";
        if (j.state === "sent") return "Job " + j.id + ": proven in " + proving + " s and sent (transaction " + Fmt.short(j.tx_hash) + "); waiting for the block that applies it, " + all + " s in all.";
        if (j.state === "settled") return "Job " + j.id + ": " + what + " settled after " + all + " s" + (j.proving_ms ? " (the proof " + proving + " s)" : "") + ".";
        return "Job " + j.id + ": " + what + " failed after " + all + " s: " + Fmt.friendly(j.error) + " (" + Fmt.technical(j.error) + ")";
    }
    Timer { id: jobTimer; interval: jobPollMs; repeat: true; running: jobRunning(); onTriggered: pollJobs() }
    function refreshAdmin(done) { call("admin.status", {}, function (ok, v) { if (ok) { admin = v; registry = v.registry; } if (done) done(ok, v); }); }
    function token() { return registry && registry.tokens && registry.tokens.length ? registry.tokens[0] : null; }
    function denomAmount(i) { var t = token(); return t && t.denominations[i] ? t.denominations[i].amount : "0"; }
    function denomText(i) { var t = token(); return t && t.denominations[i] ? Fmt.units(t.denominations[i].amount, t.decimals, t.symbol) : "–"; }
    function depositOf(leaf) {
        if (!positions) return null;
        for (var i = 0; i < positions.deposits.length; i++) if (positions.deposits[i].leaf_index === leaf) return positions.deposits[i];
        return null;
    }
    function now() { return Date.now() + clockOffset; }
    function held(leaf) { var t = heldUntil[leaf]; return t !== undefined && now() < t; }
    function releaseHeld(i) { var t = releaseHeldUntil[i]; return t !== undefined && now() < t; }
    function hoursLeft(t) { return t === undefined ? 0 : Math.max(0, Math.ceil((t - now()) / 3600000)); }
    // Devnet only: lets a held delay pass by moving anvil's clock and the view's clock past it. A user
    // of a real network waits instead.
    function letTimePass(until) {
        var ms = Math.max(0, until - now()) + 1000;
        call("advance_time", { seconds: Math.ceil(ms / 1000) }, function (ok) { if (ok) clockOffset += ms; });
    }
    function holding(kind) {
        if (!positions || !positions.holdings) return "0";
        for (var i = 0; i < positions.holdings.length; i++) if (positions.holdings[i].kind === kind && positions.holdings[i].account === 0) return positions.holdings[i].balance;
        return "0";
    }
    function burnHolding() { return holding(burnKind); }
    function proofText(p) { return p ? (p.mode === "real" ? "real proof, " + p.receipt_bytes + "-byte receipt in " + (p.prove_ms / 1000).toFixed(1) + " s, verified at settlement" : "dev mode (RISC0_DEV_MODE=1: a fake receipt)") : ""; }

    // The wallet's approval prompt: who asks, which account, the asset, amount, destination, and what
    // the call does, before anything is signed or sent (LP-0021's selection and approval flow).
    function propose(req, onApprove) {
        if (walletProvider) {
            walletProvider.approve({ title: req.title, rows: req.rows, effects: req.effects }, function (ok) { if (ok) onApprove(); else reject(); });
            return;
        }
        approval = { title: req.title, rows: req.rows, effects: req.effects, go: onApprove };
    }
    // Connect: the provider's account selection, or the built-in one.
    function walletConnect() {
        if (walletProvider) {
            walletProvider.connect("Ferrata Bridge (ferrata_bridge_ui)", info ? [info.user.depositor] : [], function (account) {
                if (account) { connectedAccount = account; refreshPositions(); }
            });
            return;
        }
        refreshInfo();
        resumeJobs();
        selecting = true;
    }
    function approve() { var a = approval; approval = null; if (a && a.go) a.go(); }
    function reject() { approval = null; lastError = "Rejected in the wallet: nothing was signed or sent."; lastOk = false; }

    // On the devnet the view loads its data at once. Inside Logos it calls ferrata_bridge_module on the
    // user's first action instead: a synchronous call to a core module that is not installed must not
    // hold the view before it is drawn.
    // Inside Basecamp (asynchronous calls) it picks up the module's jobs at once: a view opened again finds the proof it
    // left running, and a finished one's reply.
    Component.onCompleted: {
        if (transport === "devnet") { refreshInfo(); refreshRegistry(); }
        else if (typeof logos.callModuleAsync === "function") resumeJobs();
    }

    // ---- reusable pieces -----------------------------------------------------------------------
    component Heading: Label { font.pixelSize: 18; font.bold: true; color: "#1d2733"; Layout.topMargin: 6 }
    component Note: Label { wrapMode: Text.WordWrap; color: "#4a5563"; Layout.fillWidth: true; font.pixelSize: 13 }
    component Box: Rectangle {
        default property alias content: inner.data
        Layout.fillWidth: true
        implicitHeight: inner.implicitHeight + 20
        color: "white"; radius: 6; border.color: "#d9dee5"
        ColumnLayout { id: inner; anchors.fill: parent; anchors.margins: 10; spacing: 6 }
    }
    component StepTable: Box {
        property string step: "deposit"
        readonly property var s: Privacy.STEPS[step]
        Label { text: "What is public and what is private: " + s.title; font.bold: true }
        Label { visible: s.publicEth.length > 0; text: "Public on Ethereum: " + s.publicEth.join("; "); wrapMode: Text.WordWrap; Layout.fillWidth: true; color: "#8a4b00" }
        Label { visible: s.publicLez.length > 0; text: "Public on LEZ: " + s.publicLez.join("; "); wrapMode: Text.WordWrap; Layout.fillWidth: true; color: "#8a4b00" }
        Label { text: "Private: " + s.hidden.join("; "); wrapMode: Text.WordWrap; Layout.fillWidth: true; color: "#1b6b3a" }
    }
    component LeakList: Box {
        property var items: []
        property string summaryName: ""
        Label { objectName: summaryName; text: "Exposure: " + Privacy.exposure(items); font.bold: true }
        Repeater {
            model: items
            Label {
                text: (modelData.level === 0 ? "● " : modelData.level === 1 ? "▲ " : "■ ") + modelData.text
                color: modelData.level === 0 ? "#1b6b3a" : modelData.level === 1 ? "#8a6d00" : "#b3261e"
                wrapMode: Text.WordWrap; Layout.fillWidth: true
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 14
        spacing: 8

        // ---- header ----
        RowLayout {
            Layout.fillWidth: true
            ColumnLayout {
                Label { text: "Ferrata Bridge"; font.pixelSize: 24; font.bold: true; color: "#1d2733" }
                Label { text: "ETH and ERC-20s between Ethereum and LEZ, private by default. Prototype, not audited."; color: "#4a5563" }
            }
            Item { Layout.fillWidth: true }
            ColumnLayout {
                Label {
                    objectName: "header.network"
                    text: info ? info.network : (transport === "module" ? "Logos core: ferrata_bridge_module" : "devnet backend: " + backendUrl)
                    color: "#4a5563"; font.pixelSize: 12
                }
                Label {
                    objectName: "header.endpoints"
                    // Inside Logos the endpoints are the module's (its chain service's), read with `info`.
                    text: transport !== "module" ? "Ethereum RPC " + ethRpc + " · LEZ " + sequencer
                          : (info && info.eth_rpc ? "Ethereum RPC " + info.eth_rpc + " · LEZ sequencer " + info.sequencer
                             : "Endpoints: ferrata_bridge_module's")
                    color: "#4a5563"; font.pixelSize: 12
                }
                RowLayout {
                    Label { objectName: "wallet.status"; text: connectedAccount ? "Wallet: " + Fmt.short(connectedAccount) : "Wallet: not connected"; font.bold: true }
                    Button { objectName: "wallet.connect"; text: connectedAccount ? "Switch account" : "Connect wallet"; onClicked: walletConnect() }
                    BusyIndicator { running: busy > 0 || jobRunning(); visible: running; implicitWidth: 24; implicitHeight: 24 }
                }
                Label {
                    objectName: "header.job"
                    visible: jobRunning()
                    text: jobRunning() ? jobText(claimJob && !jobOver(claimJob) ? claimJob : burnJob) : ""
                    color: "#8a6d00"; font.pixelSize: 12; wrapMode: Text.WordWrap; Layout.maximumWidth: 560
                }
                // Both sides of the connected account, as the backend last read them: the Ethereum balance and block
                // (`info`), the LEZ holdings of account /0 (`scan`); refreshed on demand.
                RowLayout {
                    visible: connectedAccount !== "" && info !== null
                    Label {
                        objectName: "header.balances"
                        text: !info ? "" : ("Ethereum: " + (info.user && info.user.eth_balance_wei !== undefined ? Fmt.eth(info.user.eth_balance_wei) : "–")
                              + (info.eth_block !== undefined ? " (block " + info.eth_block + ")" : "")
                              + "  ·  LEZ /0: private " + Fmt.eth(holding("private")) + ", public " + Fmt.eth(holding("public"))
                              + (info.lez_head !== undefined ? " (block " + info.lez_head + ")" : ""))
                        color: "#1d2733"; font.pixelSize: 12
                    }
                    Button { objectName: "header.refresh"; text: "Refresh"; onClicked: { refreshInfo(); refreshPositions(); } }
                }
            }
        }

        Rectangle {
            objectName: "error.banner"
            visible: lastError !== ""
            Layout.fillWidth: true
            implicitHeight: errCol.implicitHeight + 16
            color: "#fdecea"; radius: 6; border.color: "#f5c2bd"
            ColumnLayout {
                id: errCol; anchors.fill: parent; anchors.margins: 8
                Label { objectName: "error.message"; text: Fmt.friendly(lastError); font.bold: true; color: "#b3261e"; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                Label { objectName: "error.detail"; text: Fmt.technical(lastError); color: "#6b2a24"; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            }
        }

        TabBar {
            id: bar
            Layout.fillWidth: true
            TabButton { objectName: "tab.deposit"; text: "Deposit" }
            TabButton { objectName: "tab.claim"; text: "Claim" }
            TabButton { objectName: "tab.redeem"; text: "Burn and release" }
            TabButton { objectName: "tab.positions"; text: "Positions" }
            TabButton { objectName: "tab.registry"; text: "Registry" }
            TabButton { objectName: "tab.admin"; text: "Admin" }
            TabButton { objectName: "tab.privacy"; text: "Privacy" }
            TabButton { objectName: "tab.settings"; text: "Endpoints" }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: bar.currentIndex

            // ---- Deposit ----
            ColumnLayout {
                objectName: "page.deposit"
                spacing: 8
                Heading { text: "Deposit: Ethereum to LEZ" }
                Note { text: "Only fixed amounts can be deposited, so every deposit of a denomination looks alike. Your LEZ destination is hidden inside the commitment C until you claim." }
                Box {
                    Label { text: "Token: " + (token() ? token().symbol + " (registered, deposits " + (token().deposits_enabled ? "open" : "closed") + ")" : "loading the registry…"); font.bold: true }
                    Label { text: "Denomination" }
                    RowLayout {
                        Repeater {
                            model: token() ? token().denominations : []
                            RadioButton {
                                objectName: "deposit.denom." + index
                                text: Fmt.units(modelData.amount, token().decimals, token().symbol)
                                checked: denomIndex === index
                                onClicked: { denomIndex = index; refreshPrice(); }
                            }
                        }
                    }
                    Label { objectName: "deposit.price"; text: priceText; color: "#4a5563" }
                    Label { text: "Destination on LEZ" }
                    RadioButton { objectName: "deposit.kind.private"; text: "Private account (recommended)"; checked: destKind === "private"; onClicked: { destKind = "private"; publicAcknowledged = false; depAck.checked = false; } }
                    RadioButton { objectName: "deposit.kind.public"; text: "Public account"; checked: destKind === "public"; onClicked: { destKind = "public"; publicAcknowledged = false; depAck.checked = false; } }
                    CheckBox {
                        id: depAck
                        objectName: "deposit.public.ack"
                        visible: destKind === "public"
                        text: "I understand: the account and its balance become public on LEZ when the deposit is claimed"
                        checked: false
                        onClicked: publicAcknowledged = checked
                    }
                    RowLayout {
                        Label { text: "Account" }
                        SpinBox { objectName: "deposit.account"; from: 0; to: 7; value: destAccount; onValueModified: destAccount = value }
                        Label { text: "(within the recovery window of 8 accounts)"; color: "#4a5563" }
                    }
                    CheckBox { objectName: "deposit.delay"; text: "Hold the claim for a random delay (recommended): " + delayHours + " h"; checked: delayOn; onClicked: delayOn = checked }
                    CheckBox { objectName: "deposit.relayer"; text: "Claim through a permissionless relayer (recommended): it pays the LEZ fee"; checked: relayerOn; onClicked: relayerOn = checked }
                }
                LeakList {
                    objectName: "deposit.leaks"; summaryName: "deposit.leaks.summary"
                    items: Privacy.leaks({ kind: destKind, delay: delayOn, delayHours: delayHours, relayer: relayerOn, account: destAccount, denominationText: denomText(denomIndex) })
                }
                StepTable { step: "deposit" }
                RowLayout {
                    Button {
                        objectName: "deposit.review"
                        text: "Review deposit"
                        enabled: busy === 0 && token() !== null && connectedAccount !== "" && (destKind === "private" || publicAcknowledged)
                        onClicked: reviewDeposit()
                    }
                    Label { visible: connectedAccount === ""; text: "Connect the wallet first."; color: "#8a6d00" }
                }
                Label { objectName: "deposit.result"; text: depositText; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                Item { Layout.fillHeight: true }
            }

            // ---- Claim ----
            ColumnLayout {
                objectName: "page.claim"
                spacing: 8
                Heading { text: "Claim on LEZ" }
                Note { text: "A deposit can be claimed once an imported root covers it. The private claim (recommended) proves the claim without saying which deposit it spends; a plaintext claim names the deposit." }
                Box {
                    RowLayout {
                        Button { objectName: "claim.refresh"; text: "Refresh positions"; onClicked: refreshPositions() }
                        Button { objectName: "claim.relay"; text: "Relayer step: import the vault's root (devnet stand-in)"; visible: transport === "devnet"; onClicked: call("relay", {}, function (ok) { if (ok) refreshPositions(); }) }
                    }
                    ColumnLayout {
                    Repeater {
                        model: positions ? positions.deposits : []
                        RadioButton {
                            objectName: "claim.pick." + modelData.leaf_index
                            enabled: modelData.state !== "claimed"
                            text: Fmt.eth(modelData.amount) + " to " + modelData.kind + " /" + modelData.account + ": " + modelData.state + (held(modelData.leaf_index) ? " (held by your delay)" : "")
                            checked: claimLeaf === modelData.leaf_index
                            onClicked: claimLeaf = modelData.leaf_index
                        }
                    }
                    }
                    Label { visible: !positions || positions.deposits.length === 0; text: "No deposits found for this wallet yet."; color: "#4a5563" }
                    RowLayout {
                        RadioButton { objectName: "claim.mode.private"; text: "Private claim (recommended)"; checked: claimMode === "private"; onClicked: { claimMode = "private"; plainAcknowledged = false; claimAck.checked = false; } }
                        RadioButton { objectName: "claim.mode.plain"; text: "Plaintext claim"; checked: claimMode === "plain"; onClicked: { claimMode = "plain"; plainAcknowledged = false; claimAck.checked = false; } }
                    }
                    CheckBox { id: claimAck; objectName: "claim.plain.ack"; visible: claimMode === "plain"; text: "I understand: a plaintext claim shows on LEZ which deposit paid which account"; checked: false; onClicked: plainAcknowledged = checked }
                    Label {
                        objectName: "claim.delay"
                        visible: claimLeaf >= 0 && held(claimLeaf)
                        text: claimLeaf >= 0 && heldUntil[claimLeaf] ? "Held for another " + hoursLeft(heldUntil[claimLeaf]) + " h (until " + new Date(heldUntil[claimLeaf]).toLocaleTimeString() + (clockOffset > 0 ? " on the devnet's clock" : "") + ") by the random delay you chose: a claim right after a deposit narrows which deposit it could be." : ""
                        color: "#8a6d00"; wrapMode: Text.WordWrap; Layout.fillWidth: true
                    }
                    CheckBox { objectName: "claim.skipDelay"; visible: claimLeaf >= 0 && held(claimLeaf); text: "Claim now anyway (less private)"; checked: false; onClicked: skipDelay = checked }
                    Button { objectName: "claim.wait"; visible: transport === "devnet" && claimLeaf >= 0 && held(claimLeaf); text: "Devnet: let the delay pass (moves the devnet clock " + hoursLeft(heldUntil[claimLeaf]) + " h on)"; onClicked: letTimePass(heldUntil[claimLeaf]) }
                    Button {
                        objectName: "claim.submit"
                        text: claimJob && !jobOver(claimJob) ? "Proof being made (job " + claimJob.id + ")" : (claimMode === "private" ? "Claim privately" : "Claim in plaintext")
                        enabled: busy === 0 && !jobRunning() && claimLeaf >= 0 && connectedAccount !== "" && (claimMode === "private" || plainAcknowledged) && (!held(claimLeaf) || skipDelay)
                        onClicked: reviewClaim()
                    }
                    RowLayout {
                        visible: claimJob !== null
                        Layout.fillWidth: true
                        BusyIndicator { running: claimJob !== null && !jobOver(claimJob); visible: running; implicitWidth: 24; implicitHeight: 24 }
                        Label { objectName: "claim.job"; text: jobText(claimJob); wrapMode: Text.WordWrap; Layout.fillWidth: true; color: claimJob && claimJob.state === "failed" ? "#b3261e" : "#1d2733" }
                    }
                }
                LeakList {
                    objectName: "claim.leaks"; summaryName: "claim.leaks.summary"
                    items: Privacy.claimLeaks({ mode: claimMode, kind: depositOf(claimLeaf) ? depositOf(claimLeaf).kind : "", held: claimLeaf >= 0 && held(claimLeaf), skipDelay: skipDelay, relayer: relayerOn })
                }
                Label { objectName: "claim.result"; text: claimText; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                StepTable { step: "claim" }
                Item { Layout.fillHeight: true }
            }

            // ---- Burn and release ----
            ColumnLayout {
                objectName: "page.redeem"
                spacing: 8
                Heading { text: "Burn on LEZ, release on Ethereum" }
                Note { text: "A burn publishes a note whose recipient is encrypted to you. After a checkpoint covers it, a relayer releases it on Ethereum: the release does not say which burn it pays." }
                Box {
                    Label { objectName: "burn.holding"; text: "Private holding /0: " + Fmt.eth(holding("private")) + "  ·  public holding /0: " + Fmt.eth(holding("public")); font.bold: true }
                    RowLayout {
                        RadioButton { objectName: "burn.kind.private"; text: "Burn from a private account (recommended)"; checked: burnKind === "private"; onClicked: { burnKind = "private"; burnPublicAcknowledged = false; burnAck.checked = false; } }
                        RadioButton { objectName: "burn.kind.public"; text: "Burn from a public account"; checked: burnKind === "public"; onClicked: { burnKind = "public"; burnPublicAcknowledged = false; burnAck.checked = false; } }
                    }
                    CheckBox { id: burnAck; objectName: "burn.public.ack"; visible: burnKind === "public"; text: "I understand: the holder, its balance and the burn are public on LEZ"; checked: false; onClicked: burnPublicAcknowledged = checked }
                    RowLayout {
                        Repeater {
                            model: token() ? token().denominations : []
                            RadioButton { objectName: "burn.denom." + index; text: Fmt.units(modelData.amount, token().decimals, token().symbol); checked: burnDenom === index; onClicked: burnDenom = index }
                        }
                    }
                    RowLayout {
                        Label { text: "Release to" }
                        TextField { objectName: "burn.recipient"; text: recipient; Layout.preferredWidth: 420; onEditingFinished: recipient = text }
                        Button { objectName: "burn.fresh"; text: "New fresh address"; onClicked: recipient = freshAddress() }
                    }
                    CheckBox { objectName: "burn.delay"; text: "Hold the release for a random delay (recommended): " + releaseDelayHours + " h"; checked: releaseDelayOn; onClicked: releaseDelayOn = checked }
                    Button {
                        objectName: "burn.submit"
                        text: burnJob && !jobOver(burnJob) ? "Proof being made (job " + burnJob.id + ")" : "Burn " + denomText(burnDenom)
                        enabled: busy === 0 && !jobRunning() && connectedAccount !== "" && (burnKind === "private" || burnPublicAcknowledged)
                        onClicked: reviewBurn()
                    }
                    RowLayout {
                        visible: burnJob !== null
                        Layout.fillWidth: true
                        BusyIndicator { running: burnJob !== null && !jobOver(burnJob); visible: running; implicitWidth: 24; implicitHeight: 24 }
                        Label { objectName: "burn.job"; text: jobText(burnJob); wrapMode: Text.WordWrap; Layout.fillWidth: true; color: burnJob && burnJob.state === "failed" ? "#b3261e" : "#1d2733" }
                    }
                }
                LeakList {
                    objectName: "burn.leaks"; summaryName: "burn.leaks.summary"
                    items: Privacy.burnLeaks({ kind: burnKind, freshRecipient: !info || recipient.toLowerCase() !== String(info.user.depositor).toLowerCase(), relayer: true, delay: releaseDelayOn, delayHours: releaseDelayHours })
                }
                Label { objectName: "burn.result"; text: burnText; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                Box {
                    RowLayout {
                        Button { objectName: "redeem.checkpoint"; text: "Checkpoint (devnet stand-in prover)"; visible: transport === "devnet"; onClicked: call("checkpoint", {}, function (ok) { if (ok) refreshPositions(); }) }
                        Button { objectName: "redeem.refresh"; text: "Refresh burns"; onClicked: refreshPositions() }
                    }
                    Repeater {
                        model: positions ? positions.burns : []
                        RadioButton {
                            objectName: "release.pick." + modelData.burn_index
                            enabled: !modelData.released
                            text: Fmt.eth(modelData.amount) + " to " + Fmt.short(modelData.recipient) + (modelData.released ? ": released" : ": not released") + (!modelData.released && releaseHeld(modelData.burn_index) ? " (held by your delay)" : "")
                            checked: releaseBurn === modelData.burn_index
                            onClicked: releaseBurn = modelData.burn_index
                        }
                    }
                    Label {
                        objectName: "release.delay"
                        visible: releaseBurn >= 0 && releaseHeld(releaseBurn)
                        text: releaseBurn >= 0 && releaseHeldUntil[releaseBurn] ? "Held for another " + hoursLeft(releaseHeldUntil[releaseBurn]) + " h (until " + new Date(releaseHeldUntil[releaseBurn]).toLocaleTimeString() + (clockOffset > 0 ? " on the devnet's clock" : "") + ") by the random delay you chose: a release right after a burn narrows which burn it could be." : ""
                        color: "#8a6d00"; wrapMode: Text.WordWrap; Layout.fillWidth: true
                    }
                    CheckBox { objectName: "release.skipDelay"; visible: releaseBurn >= 0 && releaseHeld(releaseBurn); text: "Release now anyway (less private)"; checked: false; onClicked: releaseSkipDelay = checked }
                    Button { objectName: "release.wait"; visible: transport === "devnet" && releaseBurn >= 0 && releaseHeld(releaseBurn); text: "Devnet: let the delay pass (moves the devnet clock " + hoursLeft(releaseHeldUntil[releaseBurn]) + " h on)"; onClicked: letTimePass(releaseHeldUntil[releaseBurn]) }
                    Button { objectName: "release.submit"; text: "Release through a relayer"; enabled: busy === 0 && !jobRunning() && releaseBurn >= 0 && connectedAccount !== "" && (!releaseHeld(releaseBurn) || releaseSkipDelay); onClicked: reviewRelease() }
                }
                Label { objectName: "release.result"; text: releaseText; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                RowLayout {
                    StepTable { step: "burn" }
                    StepTable { step: "release" }
                }
                Item { Layout.fillHeight: true }
            }

            // ---- Positions (recovery) ----
            ColumnLayout {
                objectName: "page.positions"
                spacing: 8
                Heading { text: "Positions, recovered from the seed" }
                Note { text: "Everything below is found from your seed alone: the whole deposit list (checked against the vault's depositRoot) and the zone's history, with no query about any single position." }
                Button { objectName: "positions.recover"; text: "Recover from the seed"; onClicked: refreshPositions() }
                Label {
                    objectName: "positions.summary"
                    text: positions ? "Found " + positions.deposits.length + " deposits of this wallet (" + positions.deposits.filter(function (d) { return d.state !== "claimed"; }).length + " not claimed) among " + positions.vault_deposits + " in the vault, and " + positions.burns.length + " burns (" + positions.burns.filter(function (b) { return !b.released; }).length + " not released)." : "Not scanned yet."
                    font.bold: true; wrapMode: Text.WordWrap; Layout.fillWidth: true
                }
                Box {
                    Label { text: "Deposits"; font.bold: true }
                    Repeater {
                        model: positions ? positions.deposits : []
                        Label { text: "#" + modelData.leaf_index + "  " + Fmt.eth(modelData.amount) + "  to " + modelData.kind + " /" + modelData.account + "  " + modelData.state }
                    }
                    Label { text: "Burns"; font.bold: true }
                    Repeater {
                        model: positions ? positions.burns : []
                        Label { text: "burn " + modelData.burn_index + "  " + Fmt.eth(modelData.amount) + "  to " + Fmt.short(modelData.recipient) + "  " + (modelData.released ? "released" : "not released") }
                    }
                    Label { text: "Holdings"; font.bold: true }
                    Repeater {
                        model: positions ? positions.holdings : []
                        Label { text: modelData.kind + " /" + modelData.account + "  " + Fmt.eth(modelData.balance) }
                    }
                }
                Item { Layout.fillHeight: true }
            }

            // ---- Registry ----
            ColumnLayout {
                objectName: "page.registry"
                spacing: 8
                Heading { text: "Token registry: amounts, caps and utilisation" }
                Button { objectName: "registry.refresh"; text: "Refresh"; onClicked: refreshRegistry() }
                Repeater {
                    model: registry ? registry.tokens : []
                    Box {
                        Label { objectName: "registry.token." + index; text: modelData.symbol + "  " + (modelData.registered ? "registered" : "not registered") + " · deposits " + (modelData.deposits_enabled ? "open" : "closed") + " · escrowed " + Fmt.units(modelData.escrowed, modelData.decimals, modelData.symbol); font.bold: true }
                        Repeater {
                            model: modelData.denominations
                            Label { text: "Denomination " + Fmt.eth(modelData.amount) + ": mint fee " + Fmt.eth(modelData.mint_fee) + ", relay fee " + Fmt.eth(modelData.relay_fee) + ", burn fee " + Fmt.eth(modelData.burn_fee) + ", checkpoint fee " + Fmt.eth(modelData.checkpoint_fee) }
                        }
                        Label {
                            text: "Ethereum, deposit cap: " + Fmt.cap(modelData.vault_deposit_cap.available, 18, "ETH") + " available of " + Fmt.cap(modelData.vault_deposit_cap.capacity, 18, "ETH") + " (used " + Fmt.pct(Number(modelData.vault_deposit_cap.capacity) - Number(modelData.vault_deposit_cap.available), modelData.vault_deposit_cap.capacity) + "), refill " + Fmt.cap(modelData.vault_deposit_cap.refill_per_second, 18, "ETH") + "/s"
                            wrapMode: Text.WordWrap; Layout.fillWidth: true; Layout.maximumWidth: 1120
                        }
                        Label {
                            text: "Ethereum, release cap: " + Fmt.cap(modelData.vault_release_cap.available, 18, "ETH") + " available of " + Fmt.cap(modelData.vault_release_cap.capacity, 18, "ETH") + " (used " + Fmt.pct(Number(modelData.vault_release_cap.capacity) - Number(modelData.vault_release_cap.available), modelData.vault_release_cap.capacity) + ")"
                            wrapMode: Text.WordWrap; Layout.fillWidth: true; Layout.maximumWidth: 1120
                        }
                        Label {
                            objectName: "registry.lez." + index
                            visible: modelData.lez !== null
                            text: modelData.lez ? "LEZ, mint cap: level " + modelData.lez.mint_cap.level + " of " + modelData.lez.mint_cap.capacity + " LEZ units (used " + Fmt.pct(Number(modelData.lez.mint_cap.capacity) - Number(modelData.lez.mint_cap.level), modelData.lez.mint_cap.capacity) + "); burn cap: level " + modelData.lez.burn_cap.level + " of " + modelData.lez.burn_cap.capacity + " (used " + Fmt.pct(Number(modelData.lez.burn_cap.capacity) - Number(modelData.lez.burn_cap.level), modelData.lez.burn_cap.capacity) + ");\nmints frozen until " + (modelData.lez.mints_frozen_until_ms > 0 ? new Date(modelData.lez.mints_frozen_until_ms).toISOString() : "not frozen") + "; burns frozen until " + (modelData.lez.burns_frozen_until_ms > 0 ? new Date(modelData.lez.burns_frozen_until_ms).toISOString() : "not frozen") : ""
                            wrapMode: Text.WordWrap; Layout.fillWidth: true; Layout.maximumWidth: 1120
                        }
                    }
                }
                Label {
                    objectName: "registry.vault"
                    text: registry ? "Vault: " + registry.vault.deposits + " deposits, " + registry.vault.checkpoints + " checkpoints, retired " + registry.vault.retired + ", deposits frozen until " + registry.vault.deposits_frozen_until + ", releases frozen until " + registry.vault.releases_frozen_until : ""
                    wrapMode: Text.WordWrap; Layout.fillWidth: true
                }
                Item { Layout.fillHeight: true }
            }

            // ---- Admin ----
            ColumnLayout {
                objectName: "page.admin"
                spacing: 8
                Heading { text: "Admin" }
                Note { text: "The admin sets caps, never below the immutable floor, and cannot freeze; the freeze authority freezes mints and burns for at most 30 days at a time and runs Sunset. Neither can move escrow or raise a fee past the ceiling. On the devnet the harness holds both roles' keys; a deployment uses its multisig." }
                Button { objectName: "admin.refresh"; text: "Load roles and controls"; onClicked: refreshAdmin() }
                Box {
                    Label { objectName: "admin.roles"; text: admin ? "Admin " + Fmt.short(admin.admin) + " · freeze authority " + Fmt.short(admin.freeze_authority) + " · fee recipient " + Fmt.short(admin.fee_recipient) + " · cap floor " + admin.floor.capacity + " LEZ units" : "Not loaded."; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                }
                Box {
                    Label { text: "SetCap (admin)"; font.bold: true }
                    RowLayout {
                        RadioButton { id: capMints; objectName: "admin.cap.mints"; text: "mints"; checked: true }
                        RadioButton { objectName: "admin.cap.burns"; text: "burns" }
                        Label { text: "capacity" }
                        TextField { id: capCapacity; objectName: "admin.cap.capacity"; text: "11000000000"; Layout.preferredWidth: 160 }
                        Label { text: "refill per second" }
                        TextField { id: capRefill; objectName: "admin.cap.refill"; text: "10000000"; Layout.preferredWidth: 120 }
                        Label { text: "LEZ units" }
                    }
                    Button {
                        objectName: "admin.setcap"; text: "Set cap"; enabled: busy === 0
                        onClicked: propose({
                            title: "SetCap on the bridge program (admin)",
                            rows: [["Requested by", "Ferrata Bridge (admin view)"], ["Account", "the configuration's admin"], ["Asset", "ETH"], ["Cap", capCapacity.text + " LEZ units, refill " + capRefill.text + "/s on " + (capMints.checked ? "mints" : "burns")], ["Program", "the bridge program, configuration " + Fmt.short(info ? info.config_id : "")]],
                            effects: ["Changes the token's rate limit; refused below the floor", "Moves no funds"]
                        }, function () {
                            call("admin.set_cap", { mints: capMints.checked, capacity: capCapacity.text, refill_per_second: capRefill.text }, function (ok, v) { adminText = ok ? "Cap set: " + v.capacity + " LEZ units on " + (v.mints ? "mints" : "burns") + "." : ""; if (ok) refreshRegistry(); });
                        })
                    }
                }
                Box {
                    Label { text: "Freeze (freeze authority)"; font.bold: true }
                    RowLayout {
                        CheckBox { id: frMints; objectName: "admin.freeze.mints"; text: "mints"; checked: true }
                        CheckBox { id: frBurns; objectName: "admin.freeze.burns"; text: "burns" }
                        Label { text: "for hours" }
                        SpinBox { id: frHours; objectName: "admin.freeze.hours"; from: 1; to: 720; value: 1 }
                    }
                    Button {
                        objectName: "admin.freeze"; text: "Freeze"; enabled: busy === 0
                        onClicked: propose({
                            title: "Freeze on the bridge program (freeze authority)",
                            rows: [["Requested by", "Ferrata Bridge (admin view)"], ["Account", "the configuration's freeze authority"], ["Asset", "ETH"], ["Freeze", (frMints.checked ? "mints " : "") + (frBurns.checked ? "burns " : "") + "for " + frHours.value + " h of attested Ethereum time"]],
                            effects: ["Claims and/or burns of ETH are refused until it lapses", "Positions stay claimable afterwards; moves no funds"]
                        }, function () {
                            call("admin.freeze", { mints: frMints.checked, burns: frBurns.checked, seconds: frHours.value * 3600 }, function (ok, v) { adminText = ok ? "Frozen until " + new Date(v.until_ms).toISOString() + "." : ""; if (ok) refreshRegistry(); });
                        })
                    }
                }
                Box {
                    Label { text: "Lift a freeze (freeze authority)"; font.bold: true }
                    Button {
                        objectName: "admin.unfreeze"; text: "Lift the freeze"; enabled: busy === 0
                        onClicked: propose({
                            title: "Lift the freeze on the bridge program (freeze authority)",
                            rows: [["Requested by", "Ferrata Bridge (admin view)"], ["Account", "the configuration's freeze authority"], ["Asset", "ETH"], ["Lift", (frMints.checked ? "mints " : "") + (frBurns.checked ? "burns " : "") + "now"]],
                            effects: ["Claims and/or burns of ETH are accepted again", "Moves no funds"]
                        }, function () {
                            call("admin.freeze", { mints: frMints.checked, burns: frBurns.checked, lift: true }, function (ok, v) { adminText = ok ? "Freeze lifted." : ""; if (ok) refreshRegistry(); });
                        })
                    }
                }
                Note { text: "On Ethereum the vault's own controls (fee changes announced seven days ahead, watchers, the 14-day role change, retirement) are listed in the registry and run from the operator's tools; this view sends the bridge program's." }
                Label { objectName: "admin.result"; text: adminText; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                Item { Layout.fillHeight: true }
            }

            // ---- Privacy ----
            ColumnLayout {
                objectName: "page.privacy"
                spacing: 8
                Heading { text: "What is public and what is private" }
                Note { text: "The defaults are the recommended choices: fixed denominations, a private destination, a random delay before the claim, a permissionless relayer, a fresh recipient. What stays linkable even then: the token, the denomination and the time window; a public account's history; a recipient you reuse." }
                RowLayout { StepTable { step: "deposit" } StepTable { step: "claim" } }
                RowLayout { StepTable { step: "burn" } StepTable { step: "release" } }
                Item { Layout.fillHeight: true }
            }

            // ---- Settings ----
            ColumnLayout {
                objectName: "page.settings"
                spacing: 8
                Heading { text: "Endpoints" }
                Box {
                    RowLayout {
                        Label { text: "Backend" }
                        RadioButton { objectName: "settings.transport.devnet"; text: "devnet backend (HTTP)"; checked: transport === "devnet"; onClicked: transport = "devnet" }
                        RadioButton { objectName: "settings.transport.module"; text: "Logos core module ferrata_bridge_module"; checked: transport === "module"; enabled: inLogos; onClicked: transport = "module" }
                    }
                    RowLayout { Label { text: "Devnet backend URL"; Layout.preferredWidth: 170 } TextField { id: fBackend; objectName: "settings.backend"; text: backendUrl; Layout.preferredWidth: 420 } }
                    RowLayout { Label { text: "Ethereum RPC"; Layout.preferredWidth: 170 } TextField { id: fRpc; objectName: "settings.ethRpc"; text: ethRpc; Layout.preferredWidth: 420 } }
                    RowLayout { Label { text: "LEZ sequencer or zone"; Layout.preferredWidth: 170 } TextField { id: fSeq; objectName: "settings.sequencer"; text: sequencer; Layout.preferredWidth: 420 } }
                    Button {
                        objectName: "settings.apply"; text: "Apply and test"; enabled: busy === 0
                        onClicked: {
                            backendUrl = fBackend.text; ethRpc = fRpc.text; sequencer = fSeq.text;
                            call("set_endpoints", { eth_rpc: ethRpc, sequencer: sequencer }, function (ok, v) {
                                if (!ok) { endpointStatus = "Not switched: " + Fmt.friendly(lastError); return; }
                                refreshInfo(function (ok2, i) {
                                    endpointStatus = ok2 ? "Connected: chain " + i.chain_id + ", block " + i.eth_block + " through " + v.eth_rpc + "; LEZ: " + v.sequencer
                                                         : "Switched to " + v.eth_rpc + ", which does not answer: " + Fmt.friendly(lastError);
                                });
                            });
                        }
                    }
                    Label { objectName: "settings.status"; text: endpointStatus; wrapMode: Text.WordWrap; Layout.fillWidth: true; font.bold: true }
                }
                Note {
                    objectName: "settings.sandboxNote"
                    visible: inLogos
                    color: "#8a6d00"
                    text: "Inside Logos a ui module may not use the network (Basecamp's sandbox refuses it): the devnet backend is for running this view outside Logos; here the calls go to ferrata_bridge_module."
                }
                Note { text: "The devnet backend is anvil (chain id 1, local) and the LEZ harness; in Logos the same calls go to ferrata_bridge_module, which reads Ethereum through eth_rpc_module and LEZ through lez_core at the endpoints set here." }
                Item { Layout.fillHeight: true }
            }
        }
    }

    // ---- flows -----------------------------------------------------------------------------------
    property string priceText: ""
    function refreshPrice() {
        call("price", { denomination: denomIndex }, function (ok, v) {
            priceText = ok ? "Amount " + Fmt.eth(v.denomination) + " + mint fee " + Fmt.eth(v.mint_fee) + " + relay fee " + Fmt.eth(v.relay_fee) + " = " + Fmt.eth(v.total) + " (fee schedule " + v.schedule + ")" : "";
        });
    }
    onRegistryChanged: if (registry && priceText === "") refreshPrice()

    function reviewDeposit() {
        call("price", { denomination: denomIndex }, function (ok, p) {
            if (!ok) return;
            propose({
                title: "Deposit to the Ferrata vault",
                rows: [["Requested by", "Ferrata Bridge (ferrata_bridge_ui)"], ["Account", info.user.depositor + " (Ethereum)"], ["Asset", "ETH"], ["Amount", Fmt.eth(p.denomination) + " + fees = " + Fmt.eth(p.total)], ["Destination", "the Ferrata vault " + info.vault + " on chain " + info.chain_id], ["Then on LEZ", destKind + " account /" + destAccount + ", hidden inside C"]],
                effects: ["Moves " + Fmt.eth(p.total) + " from your address into the vault's escrow", "Publishes a commitment C; the LEZ destination stays hidden", "Grants no token approval"]
            }, function () {
                call("deposit", { denomination: denomIndex, kind: destKind, account: destAccount }, function (ok2, v) {
                    if (!ok2) { depositText = ""; return; }
                    var h = {}; for (var k in heldUntil) h[k] = heldUntil[k];
                    if (delayOn) h[v.leaf_index] = now() + delayHours * 3600 * 1000;
                    heldUntil = h;
                    depositText = "Deposited " + Fmt.eth(v.amount) + " to a " + v.kind + " account (/" + v.account + "): transaction " + Fmt.short(v.tx_hash) + " in block " + v.block + ", position #" + v.leaf_index + ". Public on Ethereum: " + v.public_on_ethereum.join(", ") + ". Hidden: " + v.hidden.join(", ") + ".";
                    refreshPositions();
                });
            });
        });
    }

    function reviewClaim() {
        var d = depositOf(claimLeaf);
        propose({
            title: claimMode === "private" ? "Private claim on LEZ" : "Plaintext claim on LEZ",
            rows: [["Requested by", "Ferrata Bridge (ferrata_bridge_ui)"], ["Account", d ? d.kind + " LEZ account /" + d.account : "?"], ["Asset", "wrapped ETH"], ["Amount", d ? Fmt.eth(d.amount) : "?"], ["Destination", "the bridge program, configuration " + Fmt.short(info ? info.config_id : "")], ["Submitted by", relayerOn ? "a permissionless relayer" : "you"]],
            effects: claimMode === "private" ? ["Proves the claim in the LEZ wallet; spends the deposit's nullifier", "Mints the denomination to your account; does not say which deposit"] : ["Spends the deposit's nullifier in public", "Mints the denomination to your account, naming the deposit"]
        }, function () {
            step("claim", { leaf_index: claimLeaf, private: claimMode === "private" });
        });
    }
    // The claim's reply (the call's, or its job's once done).
    function claimFinished(ok, v, job) {
        if (!ok) { claimText = ""; return; }
        claimReply = v;
        if (v.settled && v.private) claimText = "Claimed privately: " + v.what + ". Private balance /" + v.account + ": " + Fmt.eth(v.balance) + ". Nullifier " + Fmt.short(v.nullifier) + ". Proof: " + proofText(v.proof) + ". Public on LEZ: " + v.public_on_lez.join(", ") + ". Hidden: " + v.hidden.join(", ") + ".";
        else if (v.settled) claimText = "Claimed: " + v.what + ". Balance " + Fmt.eth(v.balance) + ". Nullifier " + Fmt.short(v.nullifier) + ".";
        else claimText = "Private claim built (nullifier " + Fmt.short(v.nullifier) + ", root of " + v.root_leaf_count + " leaves). Public on LEZ once proven: " + v.public_on_lez.join(", ") + ". Not settled here: " + v.not_settled_because + ".";
        refreshPositions();
    }

    function reviewBurn() {
        propose({
            title: "Burn on LEZ",
            rows: [["Requested by", "Ferrata Bridge (ferrata_bridge_ui)"], ["Account", burnKind + " LEZ account /0"], ["Asset", "wrapped ETH"], ["Amount", denomText(burnDenom)], ["Destination", "release to " + recipient + " on Ethereum (encrypted in the note)"]],
            effects: ["Debits " + denomText(burnDenom) + " from the holder", "Publishes a burn note for a later release"]
        }, function () {
            // The release's random delay goes with the burn: the module keeps it with the job and returns it in the reply.
            step("burn", { account: 0, amount: denomAmount(burnDenom), recipient: recipient, private: burnKind === "private",
                           release_delay_ms: releaseDelayOn ? releaseDelayHours * 3600 * 1000 : 0 });
        });
    }
    // The burn's reply (the call's, or its job's once done). The release is held for the delay the burn was started with
    // (the reply's release_delay_ms; a devnet backend's reply has none: the view's own choice), from the burn's
    // settlement on this view's clock (the job's end, moved by the view's clock offset).
    function burnFinished(ok, v, job) {
        if (!ok) { burnText = ""; return; }
        burnReply = v;
        var delay = (v.release_delay_ms !== undefined && v.release_delay_ms !== null) ? Number(v.release_delay_ms) : (releaseDelayOn ? releaseDelayHours * 3600 * 1000 : 0);
        var from = job && job.finished_ms ? job.finished_ms + clockOffset : now();
        var h = {}; for (var k in releaseHeldUntil) h[k] = releaseHeldUntil[k];
        if (v.settled && delay > 0 && v.burn_index !== undefined) h[v.burn_index] = from + delay;
        releaseHeldUntil = h;
        var after = " Release to " + Fmt.short(v.recipient) + " after the next checkpoint" + (delay > 0 ? " and your random delay (" + Math.round(delay / 3600000) + " h)." : ".");
        if (v.settled && v.private) burnText = "Burned privately " + Fmt.eth(v.amount) + " (" + v.what + "); private balance /" + v.account + " " + Fmt.eth(v.balance) + ". Proof: " + proofText(v.proof) + ". Public on LEZ: " + v.public_on_lez.join(", ") + "." + after;
        else if (v.settled) burnText = "Burned " + Fmt.eth(v.amount) + " (" + v.what + "); balance " + Fmt.eth(v.balance) + "." + after;
        else burnText = "Private burn not settled here: " + v.not_settled_because + ".";
        refreshPositions();
    }

    function reviewRelease() {
        propose({
            title: "Release on Ethereum",
            rows: [["Requested by", "Ferrata Bridge (ferrata_bridge_ui)"], ["Account", "a permissionless relayer sends it and pays the gas"], ["Asset", "ETH"], ["Destination", "the recipient in your burn note"], ["Program", "the Ferrata vault " + (info ? info.vault : "")]],
            effects: ["The vault pays the burn's amount to the recipient", "Spends the burn's release nullifier NOut; does not say which burn"]
        }, function () {
            call("release", { burn_index: releaseBurn }, function (ok, v) {
                if (!ok) { releaseText = ""; return; }
                releaseText = "Released: " + Fmt.eth(v.paid_wei) + " paid to " + v.recipient + " in block " + v.block + " (transaction " + Fmt.short(v.tx_hash) + "), sent by " + v.sent_by + ".";
                refreshPositions();
            });
        });
    }

    // ---- the wallet's prompts (drawn in the window, above everything) ---------------------------
    Rectangle {
        objectName: "wallet.select"
        visible: selecting
        anchors.fill: parent
        color: "#80202833"
        z: 50
        MouseArea { anchors.fill: parent }
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 110
            width: 620; height: selCol.implicitHeight + 30
            radius: 8; color: "white"
            ColumnLayout {
                id: selCol; anchors.fill: parent; anchors.margins: 15; spacing: 8
                Label { text: "Ferrata Bridge asks to connect"; font.pixelSize: 18; font.bold: true }
                Label { text: "Choose the account it may use. It can read this account's positions and propose transactions; each one needs your approval."; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                Repeater {
                    model: info ? [info.user.depositor] : []
                    Button { objectName: "wallet.account." + index; text: "Ethereum " + modelData + " · LEZ accounts /0–/7 of the same seed"; Layout.fillWidth: true; onClicked: { connectedAccount = modelData; selecting = false; refreshPositions(); } }
                }
                Label { visible: !info; text: "Waiting for the backend…"; color: "#8a6d00" }
                Button { objectName: "wallet.cancel"; text: "Cancel"; onClicked: selecting = false }
            }
        }
    }

    Rectangle {
        objectName: "approval.panel"
        visible: approval !== null
        anchors.fill: parent
        color: "#80202833"
        z: 60
        MouseArea { anchors.fill: parent }
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 110
            width: 700; height: apCol.implicitHeight + 30
            radius: 8; color: "white"
            ColumnLayout {
                id: apCol; anchors.fill: parent; anchors.margins: 15; spacing: 6
                Label { objectName: "approval.title"; text: approval ? approval.title : ""; font.pixelSize: 18; font.bold: true }
                Repeater {
                    model: approval ? approval.rows : []
                    RowLayout {
                        Label { text: modelData[0]; Layout.preferredWidth: 130; color: "#4a5563" }
                        Label { text: modelData[1]; wrapMode: Text.WrapAnywhere; Layout.fillWidth: true; font.bold: true }
                    }
                }
                Label { text: "What it does"; font.bold: true; Layout.topMargin: 6 }
                Repeater {
                    model: approval ? approval.effects : []
                    Label { text: "• " + modelData; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                }
                RowLayout {
                    Layout.topMargin: 8
                    Button { objectName: "approval.approve"; text: "Approve"; highlighted: true; onClicked: approve() }
                    Button { objectName: "approval.reject"; text: "Reject"; onClicked: reject() }
                }
            }
        }
    }
}
