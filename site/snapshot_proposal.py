#!/usr/bin/env python3
"""Keep the filed proposal's milestones, budget and timeline in the repository: site/data/proposal-257.json.

The bid is public: https://github.com/logos-co/rfp/issues/257. The build page shows its milestones, payouts, done
gates, budget, risks and post-delivery plan as filed, and the dated updates posted on the issue, so this script
copies those sections of the issue (and only those: no contact or team details) with the time they were read.
Re-run it after the issue is edited; site/test_site.py checks the snapshot's totals.

Usage:
  gh issue view 257 -R logos-co/rfp --json title,state,createdAt,updatedAt,labels,url,body,comments > ISSUE.json
  python3 -I site/snapshot_proposal.py ISSUE.json [--out site/data/proposal-257.json]
Standard library only; it makes no network request itself.
"""

import datetime
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "site" / "data" / "proposal-257.json"

# Sections of the issue body copied verbatim (heading text -> key). Anything else (contact, team, consent) is left out.
SECTIONS = {
    "Milestones, Payout and Timeline": "milestones",
    "Total Requested Budget (USD)": "budget",
    "Post-Delivery Plan": "post_delivery",
    "Risks": "risks",
}


def sections(body):
    """{heading text: markdown under it} for every ### and #### heading of the body."""
    out, current, buf = {}, None, []
    for line in body.splitlines():
        m = re.match(r"^(#{3,5})\s+(.*?)\s*$", line)
        if m:
            if current is not None:
                out[current] = "\n".join(buf).strip()
            current, buf = m.group(2), []
            continue
        if current is not None:
            buf.append(line)
    if current is not None:
        out[current] = "\n".join(buf).strip()
    return out


def main(argv):
    if not argv or argv[0].startswith("-"):
        print(__doc__, file=sys.stderr)
        return 2
    issue = json.loads(pathlib.Path(argv[0]).read_text(encoding="utf-8"))
    out = pathlib.Path(argv[argv.index("--out") + 1]) if "--out" in argv else OUT
    body = issue["body"].replace("\r\n", "\n")
    found = sections(body)
    data = {
        "url": issue["url"],
        "title": issue["title"],
        "state": issue["state"],
        "labels": sorted(label["name"] for label in issue.get("labels", [])),
        "created": issue["createdAt"],
        "updated": issue["updatedAt"],
        "read_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "sections": {},
        "bid": "",
        "updates": [],
    }
    for heading, key in SECTIONS.items():
        if heading not in found:
            print(f"ERROR: the issue has no section {heading!r}", file=sys.stderr)
            return 1
        data["sections"][key] = found[heading]
    bid = re.search(r"^\*\*The bid is .*$", body, re.M)
    if not bid:
        print("ERROR: the summary's '**The bid is ...' paragraph is missing", file=sys.stderr)
        return 1
    data["bid"] = bid.group(0).strip()
    for c in issue.get("comments", []):
        if c.get("author", {}).get("login") != "rainbowpuffpuff":
            continue
        first = next((line for line in c["body"].replace("\r\n", "\n").splitlines() if line.strip()), "")
        title = re.sub(r"\*\*", "", first).strip()
        title = re.split(r"(?<=[.:])\s", title, maxsplit=1)[0] if len(title) > 90 else title
        data["updates"].append({"created": c["createdAt"], "url": c["url"], "title": title.rstrip(".:")})
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"snapshot_proposal.py: {data['state']}, updated {data['updated']}, {len(data['updates'])} updates -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
