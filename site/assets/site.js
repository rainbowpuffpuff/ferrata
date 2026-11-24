// Ferrata build page: the requirements overview and sheet, filters, tooltips, the evidence dialog, copy buttons
// and the live readings. No dependencies. Requests, besides the page's own files: api.github.com (each workflow's
// latest run on main and the proposal's state; five calls, cached five minutes per tab) and this host (POST /rpc,
// /status/status.json then /status.json, /status/programs.json then /programs.json, GET and POST /faucet); the
// page's HTML also loads four badge images from github.com. Data from the network is inserted with textContent;
// HTML is inserted only from this site's own generated evidence.json.
(function () {
  "use strict";
  var cfgEl = document.getElementById("site-config");
  var cfg = cfgEl ? JSON.parse(cfgEl.textContent) : null;
  var $ = function (sel, root) { return (root || document).querySelector(sel); };
  var $$ = function (sel, root) { return Array.prototype.slice.call((root || document).querySelectorAll(sel)); };
  var live = function (key) { return $('[data-live="' + key + '"]'); };
  function el(tag, attrs, text) {
    var e = document.createElement(tag);
    if (attrs) Object.keys(attrs).forEach(function (k) { e.setAttribute(k, attrs[k]); });
    if (text !== undefined && text !== null) e.textContent = String(text);
    return e;
  }
  function ago(ms) {
    var s = Math.round((Date.now() - ms) / 1000);
    if (!isFinite(s)) return "";
    if (s < 0) s = 0;
    if (s < 90) return s + " s ago";
    if (s < 5400) return Math.round(s / 60) + " min ago";
    if (s < 172800) return (s / 3600).toFixed(1) + " h ago";
    return Math.round(s / 86400) + " days ago";
  }
  function dur(sec) {
    if (!isFinite(sec)) return "";
    if (sec < 90) return Math.round(sec) + " s";
    if (sec < 5400) return Math.round(sec / 60) + " min";
    return (sec / 3600).toFixed(1) + " h";
  }
  function num(n) { return Number(n).toLocaleString("en-US"); }

  // ---- Requirements: the overview of tiles, one requirement's sheet, the full table and its filters.
  // Without JavaScript the full table shows (assets/nojs.css) and each tile links to its row.
  var form = $("#filters");
  var table = $("#req-table");
  var full = $("#req-full");
  var grid = $("#ov-grid");
  var legend = $(".ov-legend");
  var toggle = $("#view-toggle");
  var sheet = $("#req-dialog");
  var FILTERS = ["rfp", "status", "section", "ci", "ms", "q"];
  var tiles = $$("a.tile[data-req]");
  var order = tiles.map(function (t) { return t.getAttribute("data-req"); });
  var rowId = function (req) { return req.toLowerCase().split(".").join("-"); };
  var tileFor = function (req) { return $('a.tile[data-req="' + req + '"]'); };
  var applyFilters = function () { syncURL(); };

  function syncURL() {
    var params = new URLSearchParams();
    if (full && !full.hidden) {
      params.set("view", "table");
      if (form) FILTERS.forEach(function (k) { var v = form[k].value.trim(); if (v) params.set(k, v); });
    }
    var qs = params.toString();
    history.replaceState(history.state, "", location.pathname + (qs ? "?" + qs : "") + location.hash);
  }

  if (form && table) {
    form.hidden = false;
    var rows = $$("tbody tr", table);
    var count = $("#req-count");
    applyFilters = function () {
      var f = {};
      FILTERS.forEach(function (k) { f[k] = form[k].value.trim(); });
      f.q = f.q.toLowerCase();
      var shown = 0;
      rows.forEach(function (tr) {
        var d = tr.dataset;
        var ok = (!f.rfp || d.rfp === f.rfp) && (!f.status || d.status === f.status) &&
          (!f.section || d.section === f.section) && (!f.ms || (" " + d.ms + " ").indexOf(" " + f.ms + " ") >= 0) &&
          (!f.ci || (f.ci === "ci" ? d.ci === "ci" : d.ci === "not")) &&
          (!f.q || tr.textContent.toLowerCase().indexOf(f.q) >= 0);
        tr.hidden = !ok;
        if (ok) shown++;
      });
      count.textContent = shown + " of " + rows.length + " requirements";
      syncURL();
    };
    form.addEventListener("input", function () { applyFilters(); });
    form.addEventListener("change", function () { applyFilters(); });
    form.addEventListener("submit", function (e) { e.preventDefault(); });
    form.addEventListener("reset", function () { setTimeout(applyFilters, 0); });
  }

  function showTable(on) {
    if (!full) return;
    full.hidden = !on;
    if (grid) grid.hidden = on;
    if (legend) legend.hidden = on;
    if (toggle) {
      toggle.textContent = on ? "Back to the overview" : "Show the full table";
      toggle.setAttribute("aria-expanded", String(on));
    }
    if (on) applyFilters(); else syncURL();
  }

  var init = new URLSearchParams(location.search);
  if (form) FILTERS.forEach(function (k) { if (init.get(k) !== null && form[k]) form[k].value = init.get(k); });
  if (toggle && full) {
    toggle.hidden = false;
    toggle.addEventListener("click", function () {
      showTable(full.hidden);
      toggle.scrollIntoView({ block: "nearest" });
    });
  }
  showTable(init.get("view") === "table" || FILTERS.some(function (k) { return init.get(k); }));

  $$("a[data-filter]").forEach(function (a) {
    a.addEventListener("click", function (e) {
      if (!form) return;
      e.preventDefault();
      if (sheet && sheet.open) sheet.close();
      form.reset();
      new URLSearchParams(a.getAttribute("data-filter")).forEach(function (v, k) { if (form[k]) form[k].value = v; });
      showTable(true);
      $("#requirements").scrollIntoView();
      form.status.focus({ preventScroll: true });
    });
  });

  function revealRow(id) {
    var tr = document.getElementById(id);
    if (!tr || !table || !table.contains(tr)) return false;
    showTable(true);
    if (tr.hidden && form) { form.reset(); FILTERS.forEach(function (k) { form[k].value = ""; }); applyFilters(); }
    tr.scrollIntoView();
    tr.classList.add("hl");
    setTimeout(function () { tr.classList.remove("hl"); }, 2500);
    return true;
  }

  // Legend entries highlight their requirements among the tiles
  var ov = $("#overview");
  $$(".ov-legend .lg[data-hl]").forEach(function (sp) {
    var b = el("button", { type: "button", "class": "lg", "data-hl": sp.getAttribute("data-hl"), "aria-pressed": "false" });
    b.appendChild(el("span", { "class": "vh" }, "Highlight: "));
    while (sp.firstChild) b.appendChild(sp.firstChild);
    sp.parentNode.replaceChild(b, sp);
    b.addEventListener("click", function () {
      var on = b.getAttribute("aria-pressed") !== "true";
      $$(".ov-legend .lg").forEach(function (x) { x.setAttribute("aria-pressed", "false"); });
      b.setAttribute("aria-pressed", String(on));
      if (on) ov.setAttribute("data-hl", b.getAttribute("data-hl")); else ov.removeAttribute("data-hl");
    });
  });

  // One requirement's sheet, built from its row of the full table (the table stays the single source)
  var currentReq = null;
  function fillSheet(req) {
    var tr = document.getElementById(rowId(req));
    if (!tr) return false;
    var th = tr.querySelector("th"), tds = tr.querySelectorAll("td");
    var body = $("#rq-body");
    body.textContent = "";
    $("#rq-title").textContent = req.replace(/^(RFP-02\d)-/, "$1 ");
    var sec = th.querySelector(".sec");
    var head = el("div", { "class": "rq-head" });
    head.appendChild(el("span", { "class": "sec" }, sec ? sec.textContent : ""));
    var chipEl = tds[0].querySelector(".st");
    if (chipEl) head.appendChild(chipEl.cloneNode(true));
    body.appendChild(head);
    var block = function (title, cellOrNodes) {
      var s = el("section", { "class": "rq-block" });
      s.appendChild(el("h3", null, title));
      var nodes = cellOrNodes.nodeType ? [].slice.call(cellOrNodes.cloneNode(true).childNodes) : cellOrNodes;
      nodes.forEach(function (n) { if (n) s.appendChild(n); });
      $$("details", s).forEach(function (d) { d.open = true; });
      body.appendChild(s);
    };
    var words = th.querySelector(".words"), rfpLink = th.querySelector("a.small");
    var p = el("p", { "class": "small" });
    if (rfpLink) p.appendChild(rfpLink.cloneNode(true));
    block("In the RFP’s words", [words ? words.cloneNode(true) : null, p]);
    var why = tds[0].cloneNode(true);
    var st = why.querySelector(".st");
    if (st) st.parentNode.removeChild(st);
    var short = why.querySelector(".why");
    if (short && why.querySelector("details.more[data-full]")) short.parentNode.removeChild(short);
    block("Status and why", why);
    block("Shown by", tds[1]);
    block("Milestone", tds[2]);
    var foot = el("p", { "class": "rq-foot" });
    var a = el("a", { href: "#" + rowId(req) }, "Show it in the full table");
    a.addEventListener("click", function (e) {
      e.preventDefault();
      sheet.close();
      revealRow(rowId(req));
    });
    foot.appendChild(a);
    body.appendChild(foot);
    var i = order.indexOf(req);
    sheet.querySelector('[data-step="-1"]').disabled = i <= 0;
    sheet.querySelector('[data-step="1"]').disabled = i < 0 || i >= order.length - 1;
    body.scrollTop = 0;
    currentReq = req;
    return true;
  }
  function openSheet(req, how) {
    if (!sheet || !sheet.showModal || !fillSheet(req)) return false;
    var t = tileFor(req);
    if (!sheet.open) {
      if (t && full.hidden) t.scrollIntoView({ block: "center" });
      sheet.showModal();
    }
    $("#rq-title").focus();
    if (how === "push") history.pushState({ req: req }, "", "#req-" + req);
    else if (how === "replace") history.replaceState({ req: req }, "", "#req-" + req);
    return true;
  }
  if (sheet && sheet.showModal) {
    $("#rq-title").setAttribute("tabindex", "-1");
    tiles.forEach(function (t) {
      t.addEventListener("click", function (e) {
        if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button !== 0) return;
        e.preventDefault();
        openSheet(t.getAttribute("data-req"), "push");
      });
      t.addEventListener("keydown", function (e) {
        if (e.key === " " || e.key === "Spacebar") { e.preventDefault(); openSheet(t.getAttribute("data-req"), "push"); }
      });
    });
    var step = function (d) {
      var i = order.indexOf(currentReq) + d;
      if (i >= 0 && i < order.length) openSheet(order[i], "replace");
    };
    $$("[data-step]", sheet).forEach(function (b) {
      b.addEventListener("click", function () { step(parseInt(b.getAttribute("data-step"), 10)); });
    });
    $("[data-close]", sheet).addEventListener("click", function () { sheet.close(); });
    sheet.addEventListener("click", function (e) { if (e.target === sheet) sheet.close(); });
    sheet.addEventListener("keydown", function (e) {
      if (e.target.closest && e.target.closest("input, select, textarea")) return;
      if (e.key === "ArrowRight") { e.preventDefault(); step(1); }
      if (e.key === "ArrowLeft") { e.preventDefault(); step(-1); }
    });
    sheet.addEventListener("close", function () {
      if (/^#req-/.test(location.hash)) history.replaceState(null, "", location.pathname + location.search + "#requirements");
      var t = currentReq && tileFor(currentReq);
      if (t && full.hidden) t.focus({ preventScroll: true });
      currentReq = null;
    });
  }

  // Deep links: #req-RFP-021-F.12 opens that requirement's sheet; #rfp-021-f-12 its row of the full table
  function route() {
    var hash = location.hash;
    try { hash = decodeURIComponent(hash); } catch (e) { /* a malformed hash: read it as it is */ }
    var m = /^#req-(RFP-02[12]-[A-Za-z]+\.\d+)$/.exec(hash);
    if (m) { openSheet(m[1], null); return; }
    if (sheet && sheet.open) sheet.close();
    if (/^#rfp-02[12]-/.test(hash)) revealRow(hash.slice(1));
  }
  window.addEventListener("hashchange", route);
  window.addEventListener("popstate", route);
  route();

  // ---- Tooltips for the segmented bars (the same text is each part's accessible name)
  var tip = $("#tip");
  if (tip) {
    var showTip = function (e) {
      var t = e.currentTarget, r = t.getBoundingClientRect();
      tip.textContent = t.getAttribute("data-tip");
      tip.hidden = false;
      var w = tip.offsetWidth;
      tip.style.left = Math.max(6, Math.min(window.innerWidth - w - 6, r.left + r.width / 2 - w / 2)) + "px";
      tip.style.top = Math.max(6, r.top - tip.offsetHeight - 8) + "px";
    };
    var hideTip = function () { tip.hidden = true; };
    $$(".seg-part[data-tip]").forEach(function (p) {
      p.addEventListener("pointerenter", showTip);
      p.addEventListener("focus", showTip);
      p.addEventListener("pointerleave", hideTip);
      p.addEventListener("blur", hideTip);
    });
    window.addEventListener("scroll", hideTip, { passive: true });
  }

  // ---- Evidence rows in a dialog (the link itself opens the evidence page)
  var dialog = $("#ev-dialog");
  var evidence = null;
  if (dialog && dialog.showModal) {
    document.addEventListener("click", function (e) {
      var a = e.target.closest && e.target.closest("a.ev[data-ev]");
      if (!a || e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
      e.preventDefault();
      var n = a.getAttribute("data-ev");
      var open = function () {
        var row = evidence && evidence[n];
        var body = $("#ev-body");
        body.textContent = "";
        if (!row) { location.href = a.href; return; }
        body.appendChild(el("h2", { id: "ev-title" }, "Evidence row " + n + ": " + row.title));
        var dl = el("dl", { "class": "kv kv-tight" });
        [["Where it ran", row.where], ["Proofs", row.proofs], ["Re-check", row.recheck]].forEach(function (p) {
          var d = el("div");
          d.appendChild(el("dt", null, p[0]));
          var dd = el("dd");
          dd.innerHTML = p[1]; // generated by site/build.py from docs/EVIDENCE.md, escaped there
          d.appendChild(dd);
          dl.appendChild(d);
        });
        body.appendChild(dl);
        var more = el("p");
        more.appendChild(el("a", { href: "evidence.html#e" + n }, "The full row: what it shows, its numbers and its files"));
        body.appendChild(more);
        dialog.showModal();
      };
      if (evidence) { open(); return; }
      fetch("evidence.json").then(function (r) { return r.json(); })
        .then(function (j) { evidence = j; open(); })
        .catch(function () { location.href = a.href; });
    });
    dialog.addEventListener("click", function (e) { if (e.target === dialog) dialog.close(); });
  }

  // ---- Copy buttons and this host's RPC in the commands
  var origin = /^https?:$/.test(location.protocol) ? location.origin : null;
  $$("code[data-rpc]").forEach(function (c) {
    if (origin && cfg) c.textContent = c.textContent.split("{rpc}").join(origin + cfg.rpc).split("{origin}").join(origin);
  });
  if (navigator.clipboard) {
    $$("button.copy").forEach(function (b) {
      b.hidden = false;
      b.addEventListener("click", function () {
        var code = b.parentNode.querySelector("code");
        navigator.clipboard.writeText(code.textContent).then(function () {
          b.classList.add("done");
          b.setAttribute("aria-label", "Copied");
          setTimeout(function () { b.classList.remove("done"); b.setAttribute("aria-label", "Copy the commands"); }, 1600);
        });
      });
    });
  }

  if (!cfg || !live("ci-list")) return;

  // ---- GitHub: CI on main and the proposal's state (unauthenticated API, cached for five minutes)
  function cached(url) {
    var key = "gh:" + url;
    try {
      var c = JSON.parse(sessionStorage.getItem(key) || "null");
      if (c && Date.now() - c.t < 300000) return Promise.resolve(c.v);
    } catch (e) { /* storage unavailable */ }
    return fetch(url, { headers: { Accept: "application/vnd.github+json" } }).then(function (r) {
      if (!r.ok) throw new Error("GitHub answered " + r.status);
      return r.json();
    }).then(function (v) {
      try { sessionStorage.setItem(key, JSON.stringify({ t: Date.now(), v: v })); } catch (e) { /* full */ }
      return v;
    });
  }
  var ciList = live("ci-list");
  Promise.all(cfg.workflows.map(function (wf) {
    return cached(cfg.api_repo + "/actions/workflows/" + wf + "/runs?branch=main&per_page=1")
      .then(function (j) { return { wf: wf, run: (j.workflow_runs || [])[0] || null }; })
      .catch(function (e) { return { wf: wf, error: e.message }; });
  })).then(function (results) {
    var failing = [], running = 0, green = 0, errors = 0;
    if (results.every(function (x) { return x.error; })) {
      setText("ci-chip", "see GitHub");
      live("ci-note").textContent = "GitHub's API did not answer (" + results[0].error + "); the badges are GitHub's own.";
      return;
    }
    ciList.textContent = "";
    results.forEach(function (x) {
      var li = el("li", { "class": "ci-run" });
      li.appendChild(el("span", { "class": "wf" }, x.wf));
      var run = el("span", { "class": "run" });
      if (x.error || !x.run) {
        errors++;
        li.appendChild(el("span", { "class": "small" }, x.error ? "not read (" + x.error + ")" : "not run on main yet"));
        run.appendChild(el("a", { href: cfg.repo + "/actions/workflows/" + x.wf }, "runs"));
      } else {
        var r = x.run;
        var state = r.status !== "completed" ? r.status.replace("_", " ") : r.conclusion;
        var cls = r.status !== "completed" ? "dot-warn" : r.conclusion === "success" ? "dot-ok" : "dot-fail";
        if (r.status !== "completed") running++; else if (r.conclusion === "success") green++; else failing.push(x.wf);
        var pill = el("span", { "class": "pill" });
        pill.appendChild(el("span", { "class": "dot " + cls, "aria-hidden": "true" }));
        pill.appendChild(document.createTextNode(state));
        li.appendChild(pill);
        run.appendChild(el("a", { href: r.html_url }, "run " + r.run_number + " at " + r.head_sha.slice(0, 8)));
        run.appendChild(el("span", { "class": "when" }, ago(Date.parse(r.updated_at || r.run_started_at))));
      }
      li.appendChild(run);
      ciList.appendChild(li);
    });
    var chip = live("ci-chip"), dot = live("ci-dot");
    if (failing.length) { chip.textContent = "failing: " + failing.join(", "); dot.className = "dot dot-fail"; }
    else if (running) { chip.textContent = "running"; dot.className = "dot dot-warn"; }
    else if (errors === results.length) { chip.textContent = "not read"; }
    else if (errors) { chip.textContent = green + " of " + results.length + " green, " + errors + " not read"; dot.className = "dot dot-warn"; }
    else { chip.textContent = "green (all " + green + " workflows)"; dot.className = "dot dot-ok"; }
    live("ci-note").textContent = "Each workflow's latest run on main, from GitHub's API.";
  });
  cached(cfg.issue_api).then(function (j) {
    var s = live("issue-state");
    if (s) s.textContent = j.state;
    var line = live("issue-line");
    if (line) line.textContent = j.state + ", " + j.comments + " comments, last updated " + ago(Date.parse(j.updated_at)) +
      " (read now from GitHub)";
  }).catch(function () { /* the built-in state stays */ });

  // ---- The test network on this host: block height from /rpc, the light client from /status
  var GENESIS = { sepolia: 1655733600, mainnet: 1606824023 };
  function rpc(method, params) {
    var ctl = window.AbortController ? new AbortController() : null;
    var t = ctl && setTimeout(function () { ctl.abort(); }, 8000);
    return fetch(cfg.rpc, {
      method: "POST", headers: { "content-type": "application/json" }, signal: ctl ? ctl.signal : undefined,
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: method, params: params || [] })
    }).then(function (r) {
      return r.text().then(function (body) {
        var j = null;
        try { j = JSON.parse(body); } catch (e) { /* not JSON */ }
        // The gateway answers an outage with a JSON-RPC error and its own words: show them, not just the code
        if (!r.ok || !j) throw new Error((j && j.error && j.error.message) || "HTTP " + r.status);
        return j;
      });
    }).then(function (j) {
      if (t) clearTimeout(t);
      if (j.error) throw new Error(j.error.message || "RPC error");
      return j.result;
    });
  }
  function getJSON(url) {
    return fetch(url, { headers: { Accept: "application/json" } }).then(function (r) {
      return r.text().then(function (body) {
        var j = null;
        try { j = JSON.parse(body); } catch (e) { throw new Error("not JSON (HTTP " + r.status + ")"); }
        return j; // the updater answers 503 with its JSON when an instance is down
      });
    });
  }
  function instances(st) {
    if (Array.isArray(st)) return st.filter(function (x) { return x && x.finalized_slot !== undefined; });
    if (st && typeof st === "object") {
      var keys = Object.keys(st);
      for (var i = 0; i < keys.length; i++) {
        var v = st[keys[i]];
        if (Array.isArray(v) && v.length && v[0] && v[0].finalized_slot !== undefined) return v;
        if (v && typeof v === "object" && !Array.isArray(v)) {
          var inner = instances(v);
          if (inner.length) return inner;
        }
      }
    }
    return [];
  }
  function chainOf(x) {
    var n = String(x.chain || "").toLowerCase();
    return n.indexOf("mainnet") >= 0 ? "mainnet" : n.indexOf("sepolia") >= 0 ? "sepolia" : null;
  }
  function setText(key, text) { var e = live(key); if (e) e.textContent = text; }
  function firstJSON(urls) {
    var i = 0;
    var next = function (err) {
      if (i >= urls.length) return Promise.reject(err || new Error("not served"));
      return getJSON(urls[i++]).catch(next);
    };
    return next();
  }
  // A long value (an address, a slot, a word) is shortened in the middle, in monospace; its title and the fact's
  // "Full values" hold it whole.
  function shortValue(v) { return v.length > 24 ? v.slice(0, 12) + "…" + v.slice(-8) : v; }
  function factRows(f, whole) {
    var dl = el("dl", { "class": "fact-kv" }), long = false;
    ["address", "slot", "log", "predicate", "value", "block", "record_slot"].forEach(function (k) {
      var v = f[k];
      if (v === undefined || v === null || v === "") return;
      v = typeof v === "object" ? JSON.stringify(v) : String(v);
      if (whole && v.length <= 24) return;
      if (v.length > 24) long = true;
      var d = el("div");
      d.appendChild(el("dt", null, k.replace("_", " ")));
      var dd = el("dd");
      dd.appendChild(el("code", whole || v.length <= 24 ? { "class": "hex" } : { "class": "hex", title: v }, whole ? v : shortValue(v)));
      d.appendChild(dd);
      dl.appendChild(d);
    });
    return { dl: dl, long: long };
  }
  function renderFacts(st) {
    var box = live("facts");
    var list = st && (st.attestations || st.facts || (st.status && st.status.attestations));
    if (!Array.isArray(list) || !list.length) return;
    box.textContent = "";
    var ul = el("ul", { "class": "facts" });
    list.slice(0, 6).forEach(function (f) {
      var li = el("li", { "class": "fact" });
      var head = el("p", { "class": "fact-head" });
      head.appendChild(el("b", null, f.statement || f.kind || "statement"));
      li.appendChild(head);
      var rows = factRows(f, false);
      li.appendChild(rows.dl);
      if (rows.long) {
        var full = el("details", { "class": "fact-full" });
        full.appendChild(el("summary", null, "Full values"));
        full.appendChild(factRows(f, true).dl);
        li.appendChild(full);
      }
      var tx = f.tx || f.tx_hash || f.hash;
      if (tx && /^(0x)?[0-9a-fA-F]{64}$/.test(tx)) {
        var p = el("p", { "class": "small fact-tx" });
        if (cfg.explorer_tx) p.appendChild(el("a", { href: cfg.explorer_tx.split("{hash}").join(tx), title: tx }, "transaction " + tx.slice(0, 12) + "…"));
        else p.textContent = "transaction " + tx.slice(0, 12) + "…";
        if (f.time || f.utc) p.appendChild(document.createTextNode(" · " + ago(Date.parse(f.time || f.utc))));
        li.appendChild(p);
      }
      ul.appendChild(li);
    });
    box.appendChild(ul);
  }
  var netBox = live("net");
  function readNet() {
    if (!origin) { setText("net-status", "open this page from its host to read the test network"); return; }
    var dot = live("net-dot");
    rpc("getLastBlockId").then(function (height) {
      netBox.classList.remove("stale");
      var hEl = live("net-height");
      hEl.textContent = "";
      if (cfg.explorer_block) hEl.appendChild(el("a", { href: cfg.explorer_block.split("{n}").join(String(height)) }, num(height)));
      else hEl.textContent = num(height);
      setText("net-status", "answering");
      setText("net-chip", "block " + num(height));
      dot.className = "dot dot-ok";
      return rpc("getBlock", [height]).then(function (b) {
        var ts = b && b.header && b.header.timestamp;
        var fin = b && b.bedrock_status;
        setText("net-block", (ts ? ago(ts) : "time not reported") + (fin ? " · " + String(fin).toLowerCase() : ""));
      }).catch(function () { setText("net-block", "not read"); });
    }).catch(function (e) {
      netBox.classList.add("stale");
      setText("net-status", "not reachable (" + e.message + ")");
      setText("net-chip", "not reachable");
      dot.className = "dot dot-fail";
    });
    firstJSON(cfg.status_json || []).then(function (st) {
      var xs = instances(st);
      if (!xs.length) {
        var up = st && st.attestation && st.attestation.updater;
        var why = up && up.unavailable ? "the light-client updater is not answering yet" : "no light-client instance in the status yet";
        if (st && st.lez && st.lez.error) why += "; the host reports its LEZ sequencer not answering";
        setText("net-lc", why);
        return;
      }
      var lc = [], lag = [];
      xs.forEach(function (x) {
        var chain = chainOf(x);
        var g = GENESIS[chain];
        var head = x.sources_head_slot, fin = x.finalized_slot;
        lc.push((x.chain || "instance") + ": " + (x.status || "?") + ", finalized slot " + num(fin) + " (period " + x.period + ")" +
          (x.reasons && x.reasons.length ? " · " + x.reasons.slice(0, 2).join("; ") : ""));
        var parts = [];
        if (head && fin && head >= fin) parts.push((head - fin) + " slots (" + dur((head - fin) * 12) + ") behind its sources");
        if (g && fin) parts.push("newest header " + dur(Date.now() / 1000 - (g + fin * 12)) + " old");
        if (x.last_applied_utc) parts.push("last update applied " + ago(Date.parse(x.last_applied_utc)));
        lag.push((x.chain || "instance") + ": " + (parts.join(" · ") || "not reported"));
      });
      setText("net-lc", lc.join(" | "));
      setText("net-lag", lag.join(" | "));
      renderFacts(st);
    }).catch(function (e) { setText("net-lc", "status not read (" + e.message + ")"); });
  }
  // The deployed programs and the Sepolia instance, for the network's name and the commands of "Try it"
  if (origin && cfg.programs_json) {
    firstJSON(cfg.programs_json).then(function (pj) {
      var att = pj.programs && pj.programs.ferrata_attestation;
      var inst = pj.instances && pj.instances.sepolia;
      setText("net-name", [pj.network, pj.lez ? "LEZ " + pj.lez : "", !pj.programs && pj.status ? "programs " + pj.status : ""]
        .filter(Boolean).join(" · "));
      $$("code[data-rpc]").forEach(function (c) {
        var t = c.textContent;
        if (att && att.program_account) t = t.split("0x<program>").join(att.program_account);
        if (inst && inst.instance_id) t = t.split("0x<instance>").join(inst.instance_id);
        c.textContent = t;
      });
    }).catch(function () { /* the placeholders stay */ });
  }

  // The faucet: its terms, and a request from this page
  var ff = $("#faucet-form");
  if (ff && origin && cfg.faucet) {
    getJSON(cfg.faucet).then(function (terms) {
      var bits = [];
      Object.keys(terms || {}).slice(0, 8).forEach(function (k) {
        var v = terms[k];
        if (v !== null && typeof v !== "object") bits.push(k.split("_").join(" ") + ": " + v);
      });
      if (bits.length) setText("faucet-terms", "The faucet's terms as it states them: " + bits.join(" · "));
      ff.hidden = false;
    }).catch(function () { /* not served: the command stays */ });
    ff.addEventListener("submit", function (e) {
      e.preventDefault();
      var out = ff.querySelector("output");
      var btn = ff.querySelector("button");
      btn.disabled = true;
      out.textContent = "Sending…";
      fetch(cfg.faucet, {
        method: "POST", headers: { "content-type": "application/json" },
        body: JSON.stringify((function () {
          var b = { account: ff.account.value.trim() };
          var pk = ff.public_key ? ff.public_key.value.trim().toLowerCase().replace(/^0x/, "") : "";
          if (pk) b.public_key = pk;
          return b;
        })())
      }).then(function (r) {
        return r.text().then(function (body) {
          var msg = body;
          try { var j = JSON.parse(body); msg = j.message || j.error || j.result || j.status || body; } catch (x) { /* text */ }
          var text = typeof msg === "string" ? msg : JSON.stringify(msg);
          out.textContent = (r.ok ? "" : "Refused (HTTP " + r.status + "): ") + String(text).slice(0, 300);
        });
      }).catch(function (err) { out.textContent = "Not sent: " + err.message; })
        .then(function () { btn.disabled = false; });
    });
  }

  readNet();
  var timer = setInterval(function () { if (document.visibilityState === "visible") readNet(); }, 20000);
  window.addEventListener("pagehide", function () { clearInterval(timer); });
})();
