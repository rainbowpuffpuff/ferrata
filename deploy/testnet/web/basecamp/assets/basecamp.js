// Ferrata · Basecamp page: fills this host's address into the commands, copy buttons, the platform tabs, and the
// network's live state read from this host (status.json, the wallet's config). No other request is made.
(function () {
  "use strict";
  document.documentElement.classList.add("js");
  var origin = window.location.origin;

  // Commands are written with https://<this host>; put the real address in.
  Array.prototype.forEach.call(document.querySelectorAll("pre, [data-host]"), function (el) {
    if (el.textContent.indexOf("https://<this host>") >= 0) {
      el.textContent = el.textContent.split("https://<this host>").join(origin);
    }
  });

  // Copy buttons on every terminal block.
  Array.prototype.forEach.call(document.querySelectorAll(".term"), function (box) {
    var pre = box.querySelector("pre");
    if (!pre || !navigator.clipboard) return;
    var b = document.createElement("button");
    b.type = "button";
    b.className = "copy";
    b.textContent = "Copy";
    b.addEventListener("click", function () {
      var text = pre.textContent.split("\n").filter(function (l) { return l.indexOf("# ") !== 0; }).join("\n");
      navigator.clipboard.writeText(text).then(function () {
        b.textContent = "Copied";
        setTimeout(function () { b.textContent = "Copy"; }, 1400);
      });
    });
    box.appendChild(b);
  });

  // Platform tabs (Linux / macOS); the reader's platform first.
  Array.prototype.forEach.call(document.querySelectorAll("[data-tabs]"), function (group) {
    var tabs = group.querySelectorAll("[role=tab]");
    function select(id) {
      Array.prototype.forEach.call(tabs, function (t) {
        var on = t.getAttribute("aria-controls") === id;
        t.setAttribute("aria-selected", on ? "true" : "false");
        t.tabIndex = on ? 0 : -1;
        document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
      });
    }
    Array.prototype.forEach.call(tabs, function (t, i) {
      t.addEventListener("click", function () { select(t.getAttribute("aria-controls")); });
      t.addEventListener("keydown", function (e) {
        if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
        var n = tabs[(i + (e.key === "ArrowRight" ? 1 : tabs.length - 1)) % tabs.length];
        n.focus();
        select(n.getAttribute("aria-controls"));
      });
    });
    var mac = /Mac/.test(navigator.platform || "") || /Mac OS X/.test(navigator.userAgent || "");
    var want = group.getAttribute(mac ? "data-mac" : "data-linux");
    select(want || tabs[0].getAttribute("aria-controls"));
  });

  // The network's live state.
  var live = document.getElementById("live-state");
  if (!live || !window.fetch) return;
  function row(label, value, cls) {
    var dt = document.createElement("dt"), dd = document.createElement("dd");
    dt.textContent = label;
    if (cls) {
      var s = document.createElement("span");
      s.className = "st " + cls;
      s.textContent = value;
      dd.appendChild(s);
    } else {
      dd.textContent = value;
    }
    live.appendChild(dt);
    live.appendChild(dd);
  }
  function get(path) {
    return fetch(path, { cache: "no-store" }).then(function (r) { if (!r.ok) throw new Error(r.status); return r.json(); });
  }
  live.textContent = "";
  Promise.all([get("/status/status.json").catch(function () { return null; }),
               get("/wallet/config.json").catch(function () { return null; })]).then(function (res) {
    var st = res[0], cfg = res[1];
    if (st && st.lez) {
      var age = st.lez.head_age_s;
      row("LEZ chain", "block " + st.lez.head + (st.lez.final != null ? ", final " + st.lez.final : "") +
          (age != null ? " (newest " + age + " s ago)" : ""), age != null && age < 120 ? "ok" : "part");
    } else {
      row("LEZ chain", "status not readable from this page", "part");
    }
    var up = st && st.attestation && st.attestation.updater && st.attestation.updater[0];
    if (up) {
      row("Sepolia light client on the zone", "finalized slot " + up.finalized_slot + " (" + up.status + ", updated " +
          up.updated_utc + ")", up.status === "ok" ? "ok" : "part");
    }
    var b = cfg && cfg.bridge;
    if (b && b.configId && b.vault) {
      row("Bridge on this network", "paired with the Sepolia vault " + b.vault, "ok");
    } else if (b) {
      row("Bridge on this network", "program deployed; not yet paired with the Sepolia vault", "part");
    }
    if (st && st.alarms && st.alarms.length) {
      row("Alarms", st.alarms.map(function (x) { return x.name || x; }).join(", "), "no");
    }
    row("Read", new Date().toISOString().replace(/\.\d+Z$/, "Z") + " from /status/status.json and /wallet/config.json");
  });
})();
