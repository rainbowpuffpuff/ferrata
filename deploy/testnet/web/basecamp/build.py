#!/usr/bin/env python3
"""Builds the test network's Basecamp page (/basecamp/index.html) from the release record.

    python3 -I deploy/testnet/web/basecamp/build.py [--check]     (--check: compare with index.html, write nothing)

Reads release.json (written by `modules/scripts/basecamp_release.py assemble` after signing: every package's size,
SHA-256, rootHash and signer, the key's forms and fingerprints, where each platform was built), and, when present,
tests/install-test.json (the headless Logos Core install test) and tests/reproduce.json (the second build). Every
number on the page comes from those files; the page fills this host's address in with JavaScript, so no host name
is written here. Standard library only.
"""
import html
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = "https://github.com/rainbowpuffpuff/ferrata"
E = html.escape

# What each package does, in the words of its module's README (modules/README.md, modules/ferrata_bridge_ui/README.md).
PACKAGES = {
    "ferrata_bridge_ui": {
        "who": "users", "title": "Ferrata Bridge (the mini-app)",
        "does": "The app you open in Basecamp: deposit, claim, burn and release, your positions recovered from the seed, "
                "the token registry, the admin functions, what each step makes public, and the endpoints. Basecamp keeps "
                "a mini-app off the network, so every call goes to the bridge module below.",
        "needs": "ferrata_bridge_module", "readme": "modules/ferrata_bridge_ui/README.md"},
    "ferrata_bridge_module": {
        "who": "users", "title": "Ferrata bridge (core module)",
        "does": "The user's side of the bridge: the deposit commitment and the vault call, claims (private by default), "
                "burns, the release witness and journal, the release proof, recovery and status from the seed alone. "
                "The private claim and burn run as jobs the view polls, so a proof of several minutes is not cut off.",
        "needs": "", "readme": "modules/README.md#ferrata_bridge_module"},
    "ferrata_attest_module": {
        "who": "users", "title": "Ferrata attestation",
        "does": "Prove an Ethereum fact on LEZ (RFP-021): fetch a log or storage proof, dry-run it against the zone's "
                "header record, build the Attest transaction, read the program's verdict; decodes the light client's "
                "instance as LEZ holds it.",
        "needs": "", "readme": "modules/README.md#ferrata_attest_module"},
    "ferrata_relayer_module": {
        "who": "operators", "title": "Ferrata relayer",
        "does": "Finishes what users start: imports the vault's deposit roots on LEZ, checkpoints the zone's burns to the "
                "vault, wraps each release proof for Ethereum (Groth16) and sends the release, paying the gas.",
        "needs": "", "readme": "modules/README.md#ferrata_relayer_module"},
    "ferrata_updater_module": {
        "who": "operators", "title": "Ferrata updater (light client)",
        "does": "Keeps an attestation instance on LEZ following Ethereum: checks each beacon light-client update and "
                "posts it as a LEZ transaction; start, status, metrics, stop. Linux x86-64 only (it carries x86-64 "
                "builds of the circuits).",
        "needs": "", "readme": "modules/README.md#ferrata_updater_module"},
    "ferrata_attestor_module": {
        "who": "operators", "title": "Ferrata attestor (rule B)",
        "does": "Beside a LEZ sequencer's Logos Blockchain node: signs the zone's final tip for the checkpoint's rule B.",
        "needs": "", "readme": "modules/README.md#ferrata_attestor_module"},
    "ferrata_monitor_module": {
        "who": "operators", "title": "Ferrata monitor (watchers)",
        "does": "Follows a vault, checks every candidate checkpoint against its own node views and cancels a wrong one "
                "in time.",
        "needs": "", "readme": "modules/README.md#ferrata_monitor_module"},
}
ORDER = ["ferrata_bridge_ui", "ferrata_bridge_module", "ferrata_attest_module", "ferrata_relayer_module",
         "ferrata_updater_module", "ferrata_attestor_module", "ferrata_monitor_module"]
PLATFORMS = [("darwin-arm64", "macOS (Apple silicon)"), ("linux-amd64", "Linux x86-64")]


def load(name, required=True):
    path = os.path.join(HERE, name)
    if not os.path.exists(path):
        if required:
            sys.exit(f"build.py: {path} is missing")
        return None
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def mb(n):
    return f"{n / 1e6:.1f} MB" if n >= 1e6 else f"{n / 1e3:.1f} kB"


def gh(path, commit, label=None):
    return f'<a href="{REPO}/blob/{commit}/{E(path)}">{E(label or path)}</a>'


def term(lines):
    out = []
    for line in lines:
        cls = ' class="c"' if line.startswith("# ") else ""
        out.append(f"<span{cls}>{E(line)}</span>")
    return '<div class="term"><pre>' + "\n".join(out) + "</pre></div>"


def built_text(info, commit):
    b = info.get("built_by", {})
    if b.get("kind") == "github-actions":
        return (f'Built by the repository\'s <code>{E(b["workflow"])}</code> workflow on GitHub\'s {E(b["runner"])} runner, run '
                f'<a href="{REPO}/actions/runs/{b["run"]}">{b["run"]}</a>, at commit <code>{commit[:8]}</code>; signed here afterwards '
                "(signing adds the signature and keeps every file's hash).")
    if b.get("kind") == "box":
        return (f"Built from a clean clone of commit <code>{commit[:8]}</code> on {E(b['where'])} with "
                f"<code>modules/scripts/basecamp-release.sh build</code> ({E(b.get('nix', ''))}).")
    return ""


def main():
    rel = load("release.json")
    inst = load("tests/install-test.json", required=False)
    repro = load("tests/reproduce.json", required=False)
    commit = rel["source_commit"]
    short = commit[:8]
    # A release re-signed in part (release.json written by basecamp_release.py, then annotated): source_commits {commit: [files]},
    # each package its source_commit. A release from one commit has neither, and the page reads as it always did.
    mixed = rel.get("source_commits") or {}
    from_text = " and ".join(f'<code>{c[:8]}</code> ({len(fs)} package{"s" if len(fs) != 1 else ""})'
                             for c, fs in sorted(mixed.items(), key=lambda kv: kv[0] != commit))
    key = rel["key"]
    pkgs = rel["packages"]
    by = {(p["name"], p["platform"]): p for p in pkgs}
    n_lin = sum(1 for p in pkgs if p["platform"] == "linux-amd64")
    n_mac = sum(1 for p in pkgs if p["platform"] == "darwin-arm64")
    plat = rel["platforms"]
    mac_run = plat.get("darwin-arm64", {}).get("run")
    page = []
    w = page.append

    w(f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'none'; object-src 'none'">
<meta name="referrer" content="strict-origin-when-cross-origin">
<title>Ferrata · Logos Basecamp and Logos Core packages, signed</title>
<meta name="description" content="Ferrata's Logos packages for Basecamp and Logos Core: {n_lin + n_mac} signed .lgx for Linux x86-64 and macOS (Apple silicon), built from the public repository, with SHA-256 values and the release key's fingerprint.">
<meta name="color-scheme" content="light dark">
<link rel="icon" href="assets/logo.svg" type="image/svg+xml">
<link rel="stylesheet" href="assets/basecamp.css">
<script src="assets/basecamp.js" defer></script>
</head>
<body>
<a class="skip" href="#main">Skip to content</a>
<header class="top"><div class="wrap top-in"><a class="brand" href="/"><img src="assets/logo.svg" alt="" width="28" height="28"><span>Ferrata</span></a>
<nav aria-label="Site"><ul><li><a href="/">Build</a></li><li><a href="/explorer/">Explorer</a></li><li><a href="/wallet/">Wallet</a></li><li><a href="/basecamp/" aria-current="page">Basecamp</a></li><li><a href="/agents">Agents</a></li><li><a href="/status/">Status</a></li></ul></nav>
</div></header>
<main id="main" class="wrap">
<section class="hero">
<p class="eyebrow">Logos Basecamp · Logos Core · LEZ v0.4.0-rc3 test network</p>
<h1>Ferrata in Logos Basecamp<span class="sub">The bridge's mini-app and Ferrata's Logos modules as signed packages for Linux and macOS, built from the public repository, installed and called in a headless Logos Core, two of the calls on data read from this network.</span></h1>
<ul class="chips">
<li class="chip"><span class="dot ok"></span><b>{n_lin + n_mac}</b> packages: {n_lin} Linux x86-64, {n_mac} macOS arm64</li>
<li class="chip"><span class="dot ok"></span>every one signed: in the package (Logos checks it) and beside it (ssh-keygen, minisign)</li>
<li class="chip"><span class="dot ok"></span>built from {" and ".join(f'<a href="{REPO}/commit/{c}"><code>{c[:8]}</code></a>' for c in sorted(mixed, key=lambda c: c != commit)) if mixed else f'<a href="{REPO}/commit/{commit}"><code>{short}</code></a>'}, public main</li>
<li class="chip"><span class="dot part"></span>prototype, not audited</li>
</ul>
<nav aria-label="On this page" class="onpage"><a href="#download">Download</a> · <a href="#verify">Verify</a> · <a href="#install">Install</a> · <a href="#covers">What each does</a> · <a href="#withdrawal">Withdrawal</a> · <a href="#tested">Tested</a> · <a href="#reproduce">Reproduce</a> · <a href="#live">Live or not</a></nav>
<div class="keycard" aria-label="Release key">
<strong>Release key</strong> <span class="small">(one Ed25519 key, made on 23 November 2026 on the build box that signs the release; its private half is kept, without a passphrase, only in that box's storage, which is saved to a cloud volume of the project; never on a laptop or in git)</span>
<dl>
<dt>ssh-keygen fingerprint</dt><dd class="fpr">{E(key["ssh_fingerprint"])}</dd>
<dt>Basecamp shows it as</dt><dd>key {E(key["basecamp_key"])}</dd>
<dt>Logos signer (DID)</dt><dd>{E(key["did"])}</dd>
<dt>minisign public key</dt><dd>{E(key["minisign_public"])}</dd>
</dl>
<p class="small">The same key in every form: check any one of them against the copy in the repository, <a href="{REPO}/tree/main/deploy/testnet/web/basecamp/dl/keys">deploy/testnet/web/basecamp/dl/keys/</a>, not only against this page.</p>
</div>
</section>
""")

    # Download
    w('<section id="download"><h2>Download</h2>')
    w('<p class="lede">Pick your platform. Every file is listed with its size and SHA-256; <a href="dl/SHA256SUMS">SHA256SUMS</a> '
      'lists them all and is itself signed (<a href="dl/SHA256SUMS.sig">.sig</a>, <a href="dl/SHA256SUMS.minisig">.minisig</a>). '
      'For the bridge in Basecamp you need two packages: the mini-app and the bridge module. The packages contain '
      'third-party code under its own licences (OpenSSL, Boost, Rust crates; the updater also GPL-3.0 and LGPL-3.0 code, '
      'so it is conveyed under the GPL version 3): <a href="dl/notices/NOTICES.md">dl/notices/NOTICES.md</a>, with every '
      'text and the source offer.</p>')
    w('<div data-tabs data-mac="p-darwin-arm64" data-linux="p-linux-amd64"><div class="tabs" role="tablist" aria-label="Platform">')
    for pid, label in PLATFORMS:
        w(f'<button class="tab" role="tab" id="t-{pid}" aria-controls="p-{pid}" aria-selected="false">{E(label)}</button>')
    w("</div>")
    for pid, label in PLATFORMS:
        info = plat.get(pid, {})
        w(f'<div class="panel" role="tabpanel" id="p-{pid}" aria-labelledby="t-{pid}"><h3>{E(label)}</h3>')
        w(f'<p class="small">{built_text(info, info.get("source_commit", commit))}</p>')
        w('<div class="scroll"><table><thead><tr><th>Package</th><th class="num">Size</th><th>SHA-256</th><th>Get</th></tr></thead><tbody>')
        for name in ORDER:
            p = by.get((name, pid))
            if not p:
                if name in PACKAGES and pid == "darwin-arm64":
                    why = ("Linux x86-64 only (it carries x86-64 builds of the circuits)" if name == "ferrata_updater_module"
                           else "not built for macOS yet")
                    w(f'<tr><td><span class="pkg-name">{E(PACKAGES[name]["title"])}</span><br><span class="pkg-mod">{E(name)}</span></td>'
                      f'<td class="num">–</td><td class="small" colspan="2">{E(why)}</td></tr>')
                continue
            f = p["file"]
            origin = (f'<br><span class="small">{E(p["origin"])}; its variant\'s hash (<code>lgx manifest --json</code>) equals '
                      "the Linux package's.</span>") if p.get("origin") else ""
            pc = p.get("source_commit", info.get("source_commit", commit))
            if pc != info.get("source_commit", commit):
                origin += (f'<br><span class="small">From commit <a href="{REPO}/commit/{pc}"><code>{pc[:8]}</code></a>, '
                           f'signed {E(rel.get("signed_utc", ""))}; the other packages here are from '
                           f'<code>{info.get("source_commit", commit)[:8]}</code>. '
                           + (built_text({"built_by": p["built_by"]}, pc) if p.get("built_by") else "") + "</span>")
            w(f'<tr><td><span class="pkg-name">{E(PACKAGES.get(name, {}).get("title", name))}</span><br>'
              f'<span class="pkg-mod">{E(name)} {E(p["version"])}</span>{origin}</td>'
              f'<td class="num" title="{p["size"]:,} bytes">{mb(p["size"])}</td>'
              f'<td class="hash">{E(p["sha256"])}</td>'
              f'<td class="dl"><a href="dl/{E(f)}" download>.lgx</a><a href="dl/{E(f)}.sig">.sig</a><a href="dl/{E(f)}.minisig">.minisig</a></td></tr>')
        w("</tbody></table></div></div>")
    w("</div>")
    w("<p class=\"small\">Each package's rootHash (the Merkle root over its files, which Logos checks at install) is in "
      "<a href=\"release.json\">release.json</a> with the signer and where it was built.</p></section>")

    # Verify
    fp = key["ssh_fingerprint"]
    w('<section id="verify"><h2>Verify before you install</h2>')
    w('<p class="lede">Three ways to tie a package to the release key. Each counts only when its output says what is shown '
      'under it: the key it names must be the one above (and the repository\'s copy), since a key file fetched from the same '
      'host as the packages proves nothing by itself. <code>modules/scripts/basecamp-release.sh verify DIR</code> runs all '
      'three on a downloaded release and stops at the first failure.</p>')
    w('<h3>1. The signed list, then the files (ssh-keygen, sha256)</h3>')
    w(term([
        "# download each package into a folder named as in SHA256SUMS (linux-amd64/ or darwin-arm64/), then:",
        "curl -fsSLO https://<this host>/basecamp/dl/SHA256SUMS -O https://<this host>/basecamp/dl/SHA256SUMS.sig",
        "curl -fsSLO https://<this host>/basecamp/dl/keys/allowed_signers",
        "ssh-keygen -Y verify -f allowed_signers -I ferrata-release -n file -s SHA256SUMS.sig < SHA256SUMS",
        "# must print: Good \"file\" signature for ferrata-release with ED25519 key " + fp,
        "shasum -a 256 -c --ignore-missing SHA256SUMS     # Linux: sha256sum -c --ignore-missing SHA256SUMS",
        "# must print \"<file>: OK\" for every package you downloaded (with none in place it checks nothing)",
    ]))
    w('<p class="small">Or one file at a time, with its own <code>.sig</code> (the same \"Good … ' + E(fp) + '\" line):</p>')
    w(term(["ssh-keygen -Y verify -f allowed_signers -I ferrata-release -n file -s ferrata_bridge_module-0.1.0.lgx.sig < ferrata_bridge_module-0.1.0.lgx"]))
    w("<h3>2. minisign</h3>")
    w(term([f"minisign -Vm ferrata_bridge_module-0.1.0.lgx -P {key['minisign_public']}",
            "# must print: Signature and comment signature verified"]))
    w("<h3>3. Logos's own check: the signature inside the package</h3>")
    w("<p>Each package carries <code>manifest.sig</code>: an Ed25519 signature over its manifest, which holds the hash of every "
      "file. Logos Core and Basecamp check it at install and name the signer. With <code>lgx</code> (logos-package) and a "
      "keyring that holds the release key only:</p>")
    w(term([f"lgx keyring add ferrata {key['did']} --dir ./keyring",
            "lgx verify ferrata_bridge_module-0.1.0.lgx --keyring-dir ./keyring",
            "# must print both: \"Signature is valid\" and \"Signer is trusted: ferrata\"",
            "# lgx verify exits 0 for an unsigned package and for an unknown signer too: read its words, not its exit code"]))
    w("</section>")

    # Install
    w('<section id="install"><h2>Install</h2><div class="grid2">')
    w('<div class="card"><h3>In Logos Basecamp (desktop)</h3><ol class="steps">'
      "<li>Basecamp <b>0.4.0-rc.2</b>, a pre-release: tag <code>pre-release-4cea4bb-1433</code> on "
      "github.com/logos-co/logos-basecamp/releases. For Linux x86-64, <code>logos-basecamp-x86_64.AppImage</code> "
      "(293,951,992 bytes, SHA-256 <code>e59e14cddd211bed763c49e6a9bdfea462b91123b368386bdb8f700527b747a0</code>): make it "
      "executable and run it (without FUSE: <code>--appimage-extract-and-run</code>). The stable 0.3.2 installed the previous "
      "release's packages too, but names no signer.</li>"
      "<li>Package Manager (the cube in the sidebar) → <b>Install Local Package</b>: <code>ferrata_bridge_module-0.1.0.lgx</code> "
      "first, then <code>ferrata_bridge_ui-0.1.0.lgx</code>.</li>"
      f"<li>The first dialog names the signer: “Ferrata release (test network)” · key {E(key['basecamp_key'])}. Compare the "
      "key with the one at the top of this page, then choose <b>Trust &amp; install</b>: Basecamp adds the key to its "
      "keyring (Settings → Trusted Publishers), the mini-app then installs as “signed by Ferrata release (test network)”, "
      "and an update signed by another key will ask again.</li>"
      "<li>Open <b>Ferrata Bridge</b> from the sidebar. The mini-app names <code>ferrata_bridge_module</code> as an optional "
      "dependency, so Basecamp 0.4.0-rc.2 starts the module before it opens the view (if yours does not: Settings → Module "
      "Inspector → <b>Ferrata bridge</b> → <b>Load</b>).</li>"
      "<li>In the panel at the top: this network's address (<code data-host>https://&lt;this host&gt;</code>), a Sepolia RPC "
      "(a public one is filled in) and a recovery phrase made for this test network, or <b>New phrase</b>; then "
      "<b>Open with this phrase</b>. Before it derives anything the module checks the deployment it knows: this network's "
      "<code>/wallet/config.json</code>, the chain, the vault's configuration, the bridge program's image id and its paired "
      "configuration on LEZ; it refuses any other.</li>"
      "</ol><p class=\"small\">Run by us on 24 November 2026 with the 0.4.0-rc.2 AppImage on Linux x86-64, with builds made "
      "before this release and signed by a test key (the mini-app byte-identical to this one): Basecamp started the module "
      "when the app opened, and the session opened on this network with a test phrase, also with Qt's debug logging on (the "
      "phrase in none of Basecamp's log lines). The previous release's files, signed with this key, installed in 0.4.0-rc.2, "
      "which showed the dialog above and refuses to start an installed app whose files changed after the install, and in "
      "0.3.2, which names no signer (its dialog only says the request came from its unsigned Package Manager) and installs "
      "on <b>Install</b>.</p>"
      "<p class=\"small\"><b>What the mini-app does there:</b> with the session open it reads the registry and the vault, finds "
      "your positions from the phrase, shows the faucet and the LGO balances, makes the plaintext claim of a deposit to one of "
      "the phrase's public accounts and the public burn to an address you enter in its checksummed form, and hands the release "
      "to the <code>ferrata</code> tool. A claim or burn is shown to you before anything is signed, and the module signs "
      "exactly what you approved. Deposits are made in the <a href=\"/wallet/\">web wallet</a> with the same phrase "
      "(Basecamp 0.4.0-rc.2 has no Logos signer for Sepolia, so the deposit button is off), and a private claim or burn with "
      "the <code>ferrata</code> tool. The phrase reaches the module sealed, never in clear on Logos Core's connection, and the "
      f"session's keys are wiped when it ends ({gh('modules/ferrata_bridge_ui/README.md', commit, 'the mini-app’s README')} "
      "says what protects them). We ran the whole round trip inside Basecamp on our own LEZ sequencer with a stand-in for "
      "Logos's Ethereum and LEZ modules (<a href=\"#withdrawal\">below</a>).</p></div>")
    w('<div class="card"><h3>In a headless Logos Core (<code>logosctl</code>)</h3>')
    w(term([
        "printf 'signature_policy: require\\n' > node.yaml     # refuse unsigned packages and unknown signers",
        "logosctl daemon config set node.yaml",
        f"DID={key['did']}",
        "logosctl daemon start --detach",
        'logosctl key add ferrata --did "$DID"     # the daemon must be running',
        "logosctl package install ferrata_attest_module-0.1.0.lgx -y",
        "logosctl module load ferrata_attest_module",
        "logosctl call ferrata_attest_module version --json",
    ]))
    w('<p class="small">Our run of these steps, with the refusals first, is <code>modules/scripts/basecamp-install-test.sh</code>; '
      'its replies and timings are in <a href="#tested">Tested</a>.</p></div>')
    w("</div>")
    w('<h3>This network</h3><p>LEZ v0.4.0-rc3, anchored to its own Logos Blockchain node. RPC <code data-host>https://&lt;this host&gt;/rpc</code>; '
      'programs and the Sepolia light client in <a href="/programs.json">/programs.json</a>; test tokens from the '
      '<a href="/wallet/">wallet</a>\'s faucet; blocks in the <a href="/explorer/">explorer</a>.</p>')
    w('<div class="live"><dl id="live-state"><dt>Live state</dt><dd>needs JavaScript: see <a href="/status/">/status/</a></dd></dl></div>')
    w("</section>")

    # What each does
    w('<section id="covers"><h2>What each package does</h2>')
    w('<div class="scroll"><table><thead><tr><th>Package</th><th>For</th><th>What it does</th><th>Linux</th><th>macOS</th></tr></thead><tbody>')
    for name in ORDER:
        d = PACKAGES[name]
        cells = []
        for pid, _ in [("linux-amd64", ""), ("darwin-arm64", "")]:
            p = by.get((name, pid))
            cells.append('<span class="st ok">yes</span>' if p else '<span class="st no">no</span>')
        w(f'<tr><td><span class="pkg-name">{E(d["title"])}</span><br><span class="pkg-mod">{E(name)}</span></td>'
          f'<td>{E(d["who"])}</td><td>{E(d["does"])} <span class="small">({gh(d["readme"].split("#")[0], commit, "readme")})</span></td>'
          f"<td>{cells[0]}</td><td>{cells[1]}</td></tr>")
    w("</tbody></table></div></section>")

    # Withdrawal
    w('<section id="withdrawal"><h2>The withdrawal, to the end</h2>')
    w("<p class=\"lede\">Getting tokens back to Ethereum is the hard half of a bridge: the user burns on LEZ, the zone's burns "
      "are checkpointed to the vault with a proof, and the vault pays out only against a proof of that burn. All of it is in these packages.</p>")
    w('<ol class="flow">'
      "<li><b>1. Burn</b>Private by default, to a fresh Ethereum address.<span class=\"who\"> mini-app → bridge module (<code>burn</code>), proven by LEZ's wallet</span></li>"
      "<li><b>2. Checkpoint</b>The zone's burn root goes to the vault with a RISC Zero proof.<span class=\"who\"> relayer</span></li>"
      "<li><b>3. Release proof</b>The burn's release witness and journal; the proof wrapped for Ethereum (Groth16).<span class=\"who\"> bridge module (<code>release</code>), relayer</span></li>"
      "<li><b>4. Release</b>The vault verifies and pays the fresh address; the relayer pays the gas.<span class=\"who\"> relayer → vault</span></li>"
      "</ol>")
    w('<div class="scroll"><table><thead><tr><th>Where</th><th>When</th><th>What finished</th><th>Record</th></tr></thead><tbody>')
    w("<tr><td>Inside Logos Basecamp, on a LEZ sequencer of our own</td><td>17 Nov 2026</td>"
      "<td><span class=\"st ok\">all 13 steps</span> The whole private round trip in the mini-app: deposit, claim, burn, checkpoint, "
      "and the release (0.000997 ETH to the fresh address). The claim and burn proven for real inside Basecamp and verified "
      "by the sequencer; Ethereum was a local chain with a stand-in verifier.</td>"
      f"<td>{gh('modules/ferrata_bridge_ui/results-basecamp-roundtrip/real-proofs-jobs/final/README.md', commit, 'the run')}</td></tr>")
    w("<tr><td>Sepolia and a LEZ v0.4.0-rc3 zone</td><td>18 Nov 2026</td>"
      "<td><span class=\"st ok\">both releases</span> Ethereum to LEZ and back, public and private each way, real proofs, every seal "
      "verified on Sepolia (with the command-line tools, not Basecamp). That vault's verifier, RISC Zero 3.0, is affected "
      "by GHSA-mm6c-35cm-g4x2 (the note); this network's vault is a new one on RISC Zero 3.1 "
      f"({gh('docs/design/risc0-3.1-migration.md', commit, 'the migration')}).</td>"
      f"<td>{gh('e2e/runs/crown-live-4/crown.json', commit, 'crown-live-4')}, "
      f"{gh('docs/security/risc0-groth16-advisory-2026-11-21/README.md', commit, 'the note')}</td></tr>")
    lb = (inst or {}).get("live", {}).get("bridge") or {}
    paired = (f"this network's wallet configuration names the vault <code>{E(lb['vault'])}</code> on chain {E(str(lb['evmChainId']))} "
              f"and the bridge configuration <code>{E(lb['configId'][:10])}…</code> (read {E(inst['live']['read_utc'])}, "
              '<a href="tests/install-test/live-wallet-config.json">the read</a>)') if lb.get("vault") and lb.get("configId") else \
             "this network's bridge configuration was not read with the install test"
    w("<tr><td>This network, with the RISC Zero 3.1 vault on Sepolia</td><td>from 23 Nov 2026</td>"
      f"<td><span class=\"st part\">paired, no withdrawal recorded</span> Paired: {paired}. No withdrawal on this network "
      "is recorded in this repository yet; the live state above and <a href=\"/status/\">/status/</a> say where the "
      "network stands. This network's relayer is the operator's RISC Zero 3.1 build (on main since <code>b9057be6</code>), not "
      "the relayer package on this page, which is main's 3.0.5 build.</td><td><a href=\"/status/\">/status/</a></td></tr>")
    w("</tbody></table></div></section>")

    # Tested
    w('<section id="tested"><h2>Tested here</h2>')
    if inst:
        w(f'<p class="lede"><span class="st ok">{inst["passed"]} of {inst["steps"]} steps as expected</span> A headless Logos Core '
          f'({E(inst["logosctl"])}; {E(inst["where"])}) with <code>signature_policy: require</code> and the release key as its '
          "only trusted signer, run by <code>modules/scripts/basecamp-install-test.sh</code>: first what it must refuse, each "
          "for its own reason; then every Linux package installed from its signed file, each core module loaded and called. "
          "Two calls are on data the test script read from this network (the modules made no network call). The macOS "
          "packages were checked with <code>lgx verify</code> on Linux; none has been installed on a Mac by us.</p>")
        w("<h3>Refused, as it should be</h3><ul>")
        for r in inst["refusals"]:
            w(f'<li>{E(r["case"])}: <span class="st {"ok" if r["refused"] else "no"}">{"refused" if r["refused"] else "installed"}</span> '
              f'<span class="small">“{E(r["said"])}”</span></li>')
        w("</ul><h3>Installed, loaded, called</h3>")
        w('<div class="scroll"><table><thead><tr><th>Package</th><th>Install</th><th>Load</th><th>Calls</th></tr></thead><tbody>')
        for r in inst["packages"]:
            mark = lambda v: ('<span class="st ok">ok</span>' if v else '<span class="st no">failed</span>') if v is not None else "–"
            calls = "<br>".join(f'<code>{E(c["method"])}</code> {E(c["note"])}' for c in r["calls"])
            w(f'<tr><td><span class="pkg-mod">{E(r["name"])}</span></td><td>{mark(r["install"])}</td><td>{mark(r["load"])}</td><td>{calls}</td></tr>')
        w("</tbody></table></div>")
        w('<p class="small">The record: every step with its exit code and time (<a href="tests/install-test/steps.tsv">steps.tsv</a>), '
          'each reply in <code>tests/install-test/out/&lt;step&gt;.txt</code> (for example '
          '<a href="tests/install-test/out/live-read-instance.txt">live-read-instance.txt</a>), the requests in '
          '<code>req/</code>, the network data as read, and the summary (<a href="tests/install-test.json">install-test.json</a>).</p>')
    else:
        w('<p class="lede"><span class="st part">not yet recorded</span> The headless Logos Core install test is being run; its record will be here.</p>')
    w("</section>")

    # Reproduce
    w('<section id="reproduce"><h2>Reproduce</h2>')
    w("<p class=\"lede\">The packages are built from a clean checkout of public main by one script, the way the repository's "
      "macOS workflow builds them: each module exported as its own repository, <code>nix build .#lgx-portable</code>, "
      "<code>lgx verify</code>. Every input is pinned, so the same commit should give the same bytes; compare yours with "
      "the <em>unsigned</em> SHA-256 in <a href=\"release.json\">release.json</a> (signing adds the signature file, so "
      "the signed package's SHA-256 differs; its rootHash does not). Needs Nix with flakes, git, curl, jq and python3; on "
      "Linux the updater also needs LEZ's guests built first, with Rust and RISC Zero 3.0.5 and a RISC Zero home holding "
      "guest Rust 1.91.1 (<code>RISC0_HOME</code>, as <code>lez/RESULTS.md</code> says). On a Mac the updater is left "
      "out and <code>RISC0_HOME</code> is not needed.</p>")
    if mixed:
        w(f"<p>These packages come from more than one commit: {from_text}. Check out the commit of the package you "
          "rebuild: release.json gives each package its <code>source_commit</code>.</p>")
    w(term([
        f"git clone {REPO} && cd ferrata && git checkout {commit}",
        "scripts/fetch-upstream.sh",
        "RISC0_HOME=<RISC Zero home with guest Rust 1.91.1> modules/scripts/basecamp-release.sh build . ../out",
        "jq -r '.packages[] | \"\\(.sha256)  \\(.file)\"' ../out/build.json     # compare with release.json's unsigned_sha256",
    ]))
    if repro and repro.get("source_commit", commit) not in {commit, *mixed}:
        # The second build is a previous release's: none of these packages was built twice.
        n = len(repro["packages"])
        w(f'<p><span class="st part">not built twice yet</span> These packages have not been built a second time to compare '
          f'bytes. The previous release\'s {n} Linux packages, from commit <code>{repro["source_commit"][:8]}</code>, were '
          f'built twice, in two Modal containers of two workspaces, each with its own Nix store ({E(repro["a"])}; '
          f'{E(repro["b"])}): {repro["same"]} of {n} byte-identical (<a href="tests/reproduce.json">tests/reproduce.json</a>).</p>')
    elif repro:
        n = len(repro["packages"])
        rshort = repro.get("source_commit", commit)[:8]
        if repro["differ"] == 0:
            w(f'<p><span class="st ok">{repro["same"]} of {n} byte-identical</span> The Linux packages were built twice from '
              f'commit <code>{rshort}</code> in two Modal containers of two workspaces, each with its own Nix store '
              f'({E(repro["a"])}; {E(repro["b"])}; both fetch from cache.nixos.org and the same pinned sources): every '
              'package came out with the same SHA-256 (<a href="tests/reproduce.json">tests/reproduce.json</a>).</p>')
        else:
            diff = ", ".join(E(r["file"].split("/")[-1]) for r in repro["packages"] if not r["same"])
            w(f'<p><span class="st part">{repro["same"]} of {n} byte-identical</span> Two builds of commit <code>{rshort}</code> '
              f'({E(repro["a"])}; {E(repro["b"])}) differ in: {diff} (<a href="tests/reproduce.json">tests/reproduce.json</a>).</p>')
    else:
        w('<p class="small"><span class="st part">second build not yet compared</span></p>')
    if mac_run:
        w(f'<p class="small">macOS: the packages are the artifact of the repository\'s <code>macos</code> workflow, run '
          f'<a href="{REPO}/actions/runs/{mac_run}">{mac_run}</a> on GitHub\'s <code>macos-15</code> runner (Apple silicon), '
          f"at commit <code>{plat['darwin-arm64'].get('source_commit', commit)[:8]}</code>.</p>")
    w("</section>")

    # Live or not
    w('<section id="live"><h2>What is live, and what is not</h2><div class="grid2">')
    w('<div class="card"><h3>Live</h3><ul>'
      "<li>This LEZ v0.4.0-rc3 network: blocks, the faucet, the explorer, the web wallet.</li>"
      "<li>The Sepolia light client on the zone, kept up by the updater (the attestation module reads it).</li>"
      f"<li>These {n_lin + n_mac} packages, signed, with their SHA-256 and the key in four forms.</li>"
      "<li>The RISC Zero 3.1 vault on Sepolia, named in this network's bridge configuration (see the withdrawal table).</li></ul></div>")
    w('<div class="card"><h3>Not yet</h3><ul>'
      "<li>A withdrawal on this network recorded in this repository: the bridge is paired; <a href=\"/status/\">/status/</a> says where the network stands.</li>"
      "<li>Sending to Sepolia from Basecamp: a deposit needs Logos's signer for Sepolia (<code>tx_sender_module</code>, "
      "<code>keystore_module</code>), which Basecamp 0.4.0-rc.2 does not have, so deposits are made in the "
      "<a href=\"/wallet/\">web wallet</a>; a private claim or burn, and the release, need the <code>ferrata</code> tool "
      "(<a href=\"#install\">Install</a>).</li>"
      "<li>The mini-app for macOS built by the macOS workflow: the one here is assembled from the Linux build's files (it has no native code), and nobody has opened it in Basecamp on a Mac yet.</li>"
      + ("<li>A second build of these packages to compare bytes with (the previous release's Linux packages were built twice, "
         "byte-identical: <a href=\"#reproduce\">Reproduce</a>).</li>"
         if repro and repro.get("source_commit", commit) not in {commit, *mixed} else
         "<li>A second macOS build to compare bytes with (the Linux packages were built twice, byte-identical).</li>") +
      "<li>A published Logos catalog: these packages are installed from files, not from a catalog.</li>"
      "<li>An audit: everything here is a prototype.</li></ul></div>")
    w("</div></section>")
    w("</main>")
    w(f'<footer class="foot"><div class="wrap"><p>Built by <code>deploy/testnet/web/basecamp/build.py</code> from '
      f'<a href="release.json">release.json</a>, which <code>modules/scripts/basecamp_release.py</code> wrote from the signed '
      f'packages; packages from {('commits ' + from_text) if mixed else f'commit <code>{short}</code>'}; SHA256SUMS written {E(rel.get("signed_utc", ""))}. No cookies, no '
      "analytics; the page reads only this host.</p><p>This page's code and text: MIT OR Apache-2.0. The packages contain "
      'third-party code under its own licences: <a href="dl/notices/NOTICES.md">dl/notices/NOTICES.md</a>.</p></div></footer>')
    w("</body>\n</html>\n")
    html_text = "\n".join(page)
    out = os.path.join(HERE, "index.html")
    if "--check" in sys.argv:
        same = os.path.exists(out) and open(out, encoding="utf-8").read() == html_text
        print(f"build.py --check: index.html {'is' if same else 'is NOT'} the output for release.json and tests/")
        sys.exit(0 if same else 1)
    with open(out, "w", encoding="utf-8") as f:
        f.write(html_text)
    print(f"build.py: wrote {out} ({os.path.getsize(out):,} bytes), {len(pkgs)} packages")


main()
