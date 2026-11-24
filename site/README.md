# The build page

A static page for the bid, served at `/` of the test network's host beside `/explorer`, `/wallet`, `/agents`,
`/status` and `/rpc`, with the testers' start page at `/testers/`. It is generated from the repository, so it says what
the documents say at the commit it was built from, and nothing else:

| On the page | Read from |
|---|---|
| Every requirement's status, reason and evidence; the counts | [`docs/requirements-status.md`](../docs/requirements-status.md) |
| Each requirement in the RFP's own words | [`data/rfp-requirements.json`](data/rfp-requirements.json), extracted from logos-co/rfp by [`extract_rfp.py`](extract_rfp.py) |
| Each requirement's milestone | [`docs/traceability.md`](../docs/traceability.md) |
| The tests and records of each hard requirement, and whether CI runs them | [`scripts/ci/row-tests.toml`](../scripts/ci/row-tests.toml), checked against the workflows by [`check-row-tests.py`](../scripts/ci/check-row-tests.py)'s model |
| The evidence rows (the dialog and `evidence.html`) | [`docs/EVIDENCE.md`](../docs/EVIDENCE.md) |
| Milestones, payouts, done gates, budget, risks, post-delivery plan, the dated updates | [`data/proposal-257.json`](data/proposal-257.json), copied from the filed proposal by [`snapshot_proposal.py`](snapshot_proposal.py) |
| The security note and its correction | [`docs/security/risc0-groth16-advisory-2026-11-21/README.md`](../docs/security/risc0-groth16-advisory-2026-11-21/README.md) |
| "Specified but not built" | [`README.md`](../README.md) |
| Which runs, screenshots and commands to feature, with their captions | [`content.toml`](content.toml) (every path in it must be tracked) |
| The runs' Sepolia transactions, stage times and LEZ transactions | the run records `content.toml` names (`manifest.json`, `deploy.json`, the README's stage table, `txs.tsv`) |
| The testers' start page (`testers/index.html`, linked from the header and the hero) | [`testers.html`](testers.html), written by hand: what to try, the bridge's waiting times, the limits, safety, how to report; each number names its source in a `<!-- source: PATH \| TEXT -->` comment, which the build and `test_site.py` check against that file; the report form is [`.github/ISSUE_TEMPLATE/tester-report.yml`](../.github/ISSUE_TEMPLATE/tester-report.yml) |

## Commands

```sh
python3 -I site/build.py            # writes deploy/testnet/web/build/ (about 1.9 MB with the screenshots; ~0.2 MB compressed HTML)
python3 -I site/build.py --check    # exit 1 if the bundle differs from a fresh build
python3 -I site/test_site.py        # counts, every emitted link, freshness, privacy (also: --strict, --online)
python3 -I site/test_site.py --skip-freshness   # the same on a fresh build only: the mode for CI
python3 site/test_ui.py             # the page in Chromium (Playwright, optional): overview, sheet, deep links, table, layout, testers/
python3 -I site/extract_rfp.py RFP_CHECKOUT                   # after logos-co/rfp changes an RFP
gh issue view 257 -R logos-co/rfp --json title,state,createdAt,updatedAt,labels,url,body,comments > ISSUE.json
python3 -I site/snapshot_proposal.py ISSUE.json               # after the proposal is edited or updated
```

Standard library only (Python 3.11 or later, for `tomllib`). The build takes about two seconds.

**Keeping the bundle fresh.** The page is stamped with the last commit that changed anything but the bundle (not the
time of the build), and its links into this repository are pinned to the last commit that changed each file, so the
build is reproducible and `test_site.py` can compare the bundle with a fresh build. Any commit outside the bundle (a
document, the generator or anything else) therefore makes the committed bundle stale until it is rebuilt: commit the
sources first, then run `python3 -I site/build.py`, then commit `deploy/testnet/web/build/` on its own. The build needs
the whole history (a shallow clone is refused: use `fetch-depth: 0`). `--out DIR` writes only into an empty directory or
an earlier bundle (one holding `build.json`), and removes only the files that bundle's `build.json` lists.

## The requirements section

An overview first: one tile per requirement (137), grouped by RFP and section, each with its ID and its status mark,
a box in the status colour holding the status symbol: green with a check (shown), yellow with a dash (in part),
orange with an hourglass (waits on Logos), red with a cross (not yet). The legend gives the count of each and, with
JavaScript, highlights one status among the tiles. The overview with its heading fits one 1366 x 768 screen.

A click, Enter or Space on a tile opens that requirement's sheet beside the page: the RFP's words, the status and the
full reason, the tests and records (and whether CI runs them), the evidence, the milestone; previous and next (or the
arrow keys) step through the requirements; Escape closes it and focus returns to the tile. The sheet is built in the
browser from the requirement's row of the full table, so the two cannot differ. Deep links: `#req-RFP-021-F.12` opens
that sheet; `#rfp-021-f-12` opens the full table at that row; `?view=table` (or any filter, such as `?status=waits`)
opens the table. "Show the full table" swaps the overview for the table with its filters, and "Back to the overview"
swaps them back. Without JavaScript the table shows below the overview (`assets/nojs.css`) and each tile links to its
row.

## What the test checks

1. The rows on the page, counted by RFP and status, equal the Counts table of `docs/requirements-status.md` (read
   independently of the generator); every ID of `docs/traceability.md` appears once; the hero's figures and
   `build.json` agree. The overview has one tile per row, linking to it, with the row's status as its class, the
   status's symbol as its mark and "RFP-021 F.12, shown" as its accessible name; the legend's counts equal the tiles'.
2. Every `href` and `src` of `index.html`, `evidence.html` and `testers/index.html`: anchors exist; bundle files
   exist (relative links resolved from their page's directory); links into this repository name tracked files and
   directories at an existing line or heading; Actions links name real workflows; links into logos-co/rfp name the
   extracted commit and its files; Etherscan links name the recorded runs' transactions and contracts; the test
   network's paths are the eight it serves; the report form is a tracked issue form; any other external URL is one the
   repository's documents give (`--online` also requests each). A self-check feeds it links it must refuse.
3. No inline script or style attribute, no event-handler attribute, a Content-Security-Policy on every page, unique ids,
   every image with its alt text and size.
4. The bundle equals a fresh build; no e-mail address or home-directory path in it.
5. The testers' page: the build page's header and hero link to it; it loads no script; it has at least 30 source
   notes, and each names a tracked file that still holds the text it quotes (a self-check feeds it two that must fail).

## The live readings

In the browser, with JavaScript: each workflow's latest run on `main` and the proposal's state from GitHub's public
API (`api.github.com`, five calls, cached five minutes per tab; the page also loads GitHub's own four badge images from
`github.com`, with or without JavaScript), and from this host:

- `POST /rpc`, LEZ's JSON-RPC: `getLastBlockId` (the height) and `getBlock` (its `header.timestamp` in ms and
  `bedrock_status`).
- `GET /status/status.json` (then `/status.json`): the host's status, with the light-client updater's `/health`
  inside it (an array with one object per instance: `chain`, `status`, `reasons`, `finalized_slot`, `period`,
  `sources_head_slot`, `last_applied_utc`, …; the page finds the array wherever it is nested). With an `attestations`
  array (`statement` or `kind`, `address`, `slot`, `log`, `predicate`, `value`, `block`, `record_slot`, `tx`, `time`),
  the latest attested facts are listed, each transaction linked to the explorer (`#/tx/HASH`).
- `GET /status/programs.json` (then `/programs.json`): the network's name, its LEZ version, the attestation program's
  account and the Sepolia instance, which also fill the placeholders of the `ferrata attest` command.
- `GET /faucet` (its terms) and `POST /faucet` with `{"account": ID, "public_key": HEX}` (one transfer; the public
  key, 64 hex digits, is needed when the account has never been used), from the faucet's form.

The page shows what it could not read, as such. The test network is one we run for the bid, not LEZ testnet 0.4.

## Serving it

Serve `deploy/testnet/web/build/` as static files at `/`. The pages carry their Content-Security-Policy in a `meta`
element (`default-src 'self'`; scripts and styles from the host only; `connect-src 'self' https://api.github.com`;
images from the host and GitHub's badges); a server can add `frame-ancestors 'none'`, `X-Content-Type-Options:
nosniff` and `Referrer-Policy: strict-origin-when-cross-origin` as headers, which a `meta` element cannot. No cookies,
no analytics, no fonts or scripts from elsewhere.

## Design notes

System fonts, one accent (iron oxide, for the via ferrata's iron rungs), light and dark each chosen. The four status
colours are the owner's (green, yellow, orange, red), in steps far apart in lightness so that they stay distinct under
colour-vision deficiency; they were chosen with a colour-vision simulation during design, which is not part of this
repository. What the repository checks is contrast: `test_site.py` computes each status symbol's contrast on its box
in both themes and requires at least 3:1 (WCAG 1.4.11). Yellow and orange are lighter than a mark on the light surface
would need on its own, so every mark is a bordered box with its symbol, every count sits in text, and a status never
rests on colour alone: tiles carry their status in their name, chips and legends carry the word. The requirements
table becomes one card per requirement on narrow screens.

The hero gives each RFP one card (its name, the count shown, a bar of its four statuses and a legend with every count);
the two cards share their rows, so they align side by side. A count also sits inside its part of the bar, but only
while the bar is wide enough for it: `build.py` works out, from the part's share and the count's digits, the bar width
it needs and emits a container query for it, so this holds at every width and without JavaScript. On narrow screens
the tables of prose (milestones, what is not done, a run's Sepolia transactions) become one card per row, and the
timeline one cell, then one row, per milestone with its weeks. `test_ui.py` checks the layout at 320, 375, 768, 1024,
1450 and 1920 px, light and dark, with the live readings answered from fixtures (one fact with a long `statement`, "Full
values" open): no sideways scroll, no text over text or out of its box in the hero, nothing wider than its card in
Live, no box clipping its text; and the testers' page at the same widths and themes: its links from the build page's
header and hero, no sideways scroll, nothing wider than the screen, no table scrolling, no box clipping its text, and the page
whole without JavaScript (it has none).
