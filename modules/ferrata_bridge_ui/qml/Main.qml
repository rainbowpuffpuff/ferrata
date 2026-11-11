// Ferrata Bridge: the Logos mini-app (ui_qml module) of the Ferrata bridge, prototype, not audited.
// Deposit, claim, burn and release; recovery from the seed; the token registry with caps and
// utilisation; the admin functions; what is public and private at each step; the endpoints.
//
// Every call goes through call(): in Logos (a `logos` bridge is present) to the core module
// ferrata_bridge_module, otherwise to the devnet backend over HTTP (cli/examples/miniapp_devnet.rs),
// which serves the same methods from anvil and the LEZ harness. Every transaction is first shown in
// the wallet's approval prompt (propose()), the flow of the LP-0021 Wallet Provider SDK: connect,
// account selection, approval, transaction. LP-0021 has no released SDK yet; propose() is the seam.
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
    property bool relayerOn: true
    property var heldUntil: ({})
    property string depositText: ""

    property int claimLeaf: -1
    property string claimMode: "private"
    property bool plainAcknowledged: false
    property bool skipDelay: false
    property string claimText: ""

    property string burnKind: "private"
    property bool burnPublicAcknowledged: false
    property int burnDenom: 0
    property string recipient: freshAddress()
    property string burnText: ""
    property int releaseBurn: -1
    property string releaseText: ""
    property string adminText: ""

    function freshAddress() {
        var h = "0x";
        for (var i = 0; i < 40; i++) h += "0123456789abcdef".charAt(Math.floor(Math.random() * 16));
        return h;
    }

    function call(method, params, done) {
        busy += 1;
        calls += 1;
        var finish = function (ok, value) {
            busy -= 1;
            lastMethod = method;
            lastOk = ok;
            if (ok) { lastResult = value; lastError = ""; } else { lastError = String(value); }
            if (done) done(ok, value);
        };
        if (transport === "module") {
            try {
                var raw = logos.callModule("ferrata_bridge_module", method, [JSON.stringify(params || {})]);
                var r = (typeof raw === "string") ? JSON.parse(raw) : raw;
                if (r && r.ok === false) finish(false, r.error);
                else finish(true, (r && r.result !== undefined) ? r.result : r);
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

    function refreshInfo(done) { call("info", {}, function (ok, v) { if (ok) info = v; if (done) done(ok, v); }); }
    function refreshRegistry(done) { call("registry", {}, function (ok, v) { if (ok) registry = v; if (done) done(ok, v); }); }
    function refreshPositions(done) { call("scan", {}, function (ok, v) { if (ok) positions = v; if (done) done(ok, v); }); }
    function refreshAdmin(done) { call("admin.status", {}, function (ok, v) { if (ok) { admin = v; registry = v.registry; } if (done) done(ok, v); }); }
    function token() { return registry && registry.tokens && registry.tokens.length ? registry.tokens[0] : null; }
    function denomAmount(i) { var t = token(); return t && t.denominations[i] ? t.denominations[i].amount : "0"; }
    function denomText(i) { var t = token(); return t && t.denominations[i] ? Fmt.units(t.denominations[i].amount, t.decimals, t.symbol) : "–"; }
    function depositOf(leaf) {
        if (!positions) return null;
        for (var i = 0; i < positions.deposits.length; i++) if (positions.deposits[i].leaf_index === leaf) return positions.deposits[i];
        return null;
    }
    function held(leaf) { var t = heldUntil[leaf]; return t !== undefined && Date.now() < t; }
    function burnHolding() {
        if (!positions || !positions.holdings) return "0";
        for (var i = 0; i < positions.holdings.length; i++) if (positions.holdings[i].kind === "public") return positions.holdings[i].balance;
        return "0";
    }

    // The wallet's approval prompt: who asks, which account, the asset, amount, destination, and what
    // the call does, before anything is signed or sent (LP-0021's selection and approval flow).
    function propose(req, onApprove) { approval = { title: req.title, rows: req.rows, effects: req.effects, go: onApprove }; }
    function approve() { var a = approval; approval = null; if (a && a.go) a.go(); }
    function reject() { approval = null; lastError = "Rejected in the wallet: nothing was signed or sent."; lastOk = false; }

    // On the devnet the view loads its data at once. Inside Logos it calls ferrata_bridge_module on the
    // user's first action instead: a synchronous call to a core module that is not installed must not
    // hold the view before it is drawn.
    Component.onCompleted: if (transport === "devnet") { refreshInfo(); refreshRegistry(); }

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
                Label { objectName: "header.endpoints"; text: "Ethereum RPC " + ethRpc + " · LEZ " + sequencer; color: "#4a5563"; font.pixelSize: 12 }
                RowLayout {
                    Label { objectName: "wallet.status"; text: connectedAccount ? "Wallet: " + Fmt.short(connectedAccount) : "Wallet: not connected"; font.bold: true }
                    Button { objectName: "wallet.connect"; text: connectedAccount ? "Switch account" : "Connect wallet"; onClicked: { refreshInfo(); selecting = true; } }
                    BusyIndicator { running: busy > 0; visible: busy > 0; implicitWidth: 24; implicitHeight: 24 }
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
                        text: claimLeaf >= 0 && heldUntil[claimLeaf] ? "Held until " + new Date(heldUntil[claimLeaf]).toLocaleTimeString() + " by the random delay you chose: a claim right after a deposit narrows which deposit it could be." : ""
                        color: "#8a6d00"; wrapMode: Text.WordWrap; Layout.fillWidth: true
                    }
                    CheckBox { objectName: "claim.skipDelay"; visible: claimLeaf >= 0 && held(claimLeaf); text: "Claim now anyway (less private)"; checked: false; onClicked: skipDelay = checked }
                    Button {
                        objectName: "claim.submit"
                        text: claimMode === "private" ? "Claim privately" : "Claim in plaintext"
                        enabled: busy === 0 && claimLeaf >= 0 && connectedAccount !== "" && (claimMode === "private" || plainAcknowledged) && (!held(claimLeaf) || skipDelay)
                        onClicked: reviewClaim()
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
                    Label { objectName: "burn.holding"; text: "Public holding /0: " + Fmt.eth(burnHolding()); font.bold: true }
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
                    Button {
                        objectName: "burn.submit"
                        text: "Burn " + denomText(burnDenom)
                        enabled: busy === 0 && connectedAccount !== "" && (burnKind === "private" || burnPublicAcknowledged)
                        onClicked: reviewBurn()
                    }
                }
                LeakList {
                    objectName: "burn.leaks"; summaryName: "burn.leaks.summary"
                    items: Privacy.burnLeaks({ kind: burnKind, freshRecipient: !info || recipient.toLowerCase() !== String(info.user.depositor).toLowerCase(), relayer: true })
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
                            text: Fmt.eth(modelData.amount) + " to " + Fmt.short(modelData.recipient) + (modelData.released ? ": released" : ": not released")
                            checked: releaseBurn === modelData.burn_index
                            onClicked: releaseBurn = modelData.burn_index
                        }
                    }
                    Button { objectName: "release.submit"; text: "Release through a relayer"; enabled: busy === 0 && releaseBurn >= 0 && connectedAccount !== ""; onClicked: reviewRelease() }
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
                        Label { text: "Ethereum, deposit cap: " + Fmt.eth(modelData.vault_deposit_cap.available) + " available of " + Fmt.eth(modelData.vault_deposit_cap.capacity) + " (used " + Fmt.pct(Number(modelData.vault_deposit_cap.capacity) - Number(modelData.vault_deposit_cap.available), modelData.vault_deposit_cap.capacity) + "), refill " + Fmt.eth(modelData.vault_deposit_cap.refill_per_second) + "/s" }
                        Label { text: "Ethereum, release cap: " + Fmt.eth(modelData.vault_release_cap.available) + " available of " + Fmt.eth(modelData.vault_release_cap.capacity) + " (used " + Fmt.pct(Number(modelData.vault_release_cap.capacity) - Number(modelData.vault_release_cap.available), modelData.vault_release_cap.capacity) + ")" }
                        Label {
                            objectName: "registry.lez." + index
                            visible: modelData.lez !== null
                            text: modelData.lez ? "LEZ, mint cap: level " + modelData.lez.mint_cap.level + " of " + modelData.lez.mint_cap.capacity + " LEZ units (used " + Fmt.pct(Number(modelData.lez.mint_cap.capacity) - Number(modelData.lez.mint_cap.level), modelData.lez.mint_cap.capacity) + "); burn cap: level " + modelData.lez.burn_cap.level + " of " + modelData.lez.burn_cap.capacity + "; mints frozen until " + (modelData.lez.mints_frozen_until_ms > 0 ? new Date(modelData.lez.mints_frozen_until_ms).toISOString() : "not frozen") + "; burns frozen until " + (modelData.lez.burns_frozen_until_ms > 0 ? new Date(modelData.lez.burns_frozen_until_ms).toISOString() : "not frozen") : ""
                            wrapMode: Text.WordWrap; Layout.fillWidth: true
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
                    if (delayOn) h[v.leaf_index] = Date.now() + delayHours * 3600 * 1000;
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
            call("claim", { leaf_index: claimLeaf, private: claimMode === "private" }, function (ok, v) {
                if (!ok) { claimText = ""; return; }
                if (v.settled) claimText = "Claimed: " + v.what + ". Balance " + Fmt.eth(v.balance) + ". Nullifier " + Fmt.short(v.nullifier) + ".";
                else claimText = "Private claim built (nullifier " + Fmt.short(v.nullifier) + ", root of " + v.root_leaf_count + " leaves). Public on LEZ once proven: " + v.public_on_lez.join(", ") + ". Not settled here: " + v.not_settled_because + ".";
                refreshPositions();
            });
        });
    }

    function reviewBurn() {
        propose({
            title: "Burn on LEZ",
            rows: [["Requested by", "Ferrata Bridge (ferrata_bridge_ui)"], ["Account", burnKind + " LEZ account /0"], ["Asset", "wrapped ETH"], ["Amount", denomText(burnDenom)], ["Destination", "release to " + recipient + " on Ethereum (encrypted in the note)"]],
            effects: ["Debits " + denomText(burnDenom) + " from the holder", "Publishes a burn note for a later release"]
        }, function () {
            call("burn", { account: 0, amount: denomAmount(burnDenom), recipient: recipient, private: burnKind === "private" }, function (ok, v) {
                if (!ok) { burnText = ""; return; }
                burnText = v.settled ? "Burned " + Fmt.eth(v.amount) + " (" + v.what + "); balance " + Fmt.eth(v.balance) + ". Release to " + Fmt.short(v.recipient) + " after the next checkpoint."
                                     : "Private burn not settled here: " + v.not_settled_because + ".";
                refreshPositions();
            });
        });
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
