#!/usr/bin/env python3
"""The shell that runs on macOS is portable: no GNU-only command or option, nothing newer than bash 3.2
(RFP-022 Supportability #2; GitHub's macOS runners have BSD tar, sed and find, no coreutils, and /bin/bash
3.2.57).

Checks the `run:` steps of .github/workflows/macos.yml, every shell script they run (followed through
scripts/ci/ci_model.py's trace: `bash X.sh`, `./X.sh`, `"$here/X.sh"`), and the scripts an owner runs on
their own machine to publish (modules/scripts/release-publish.sh and what it calls), line by line, for
the constructs below. A line may carry `# portable: <reason>` to accept one on purpose. Heredoc bodies
(Python, JSON) and comments are skipped. Exits 1 on any finding.

Standard library only. Usage: python3 -I scripts/ci/check-portable.py
"""

import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import ci_model  # noqa: E402

ROOT = ci_model.ROOT
ALSO = ["modules/scripts/release-publish.sh"]

# (pattern, why) - the pattern is searched in a line with its comment removed.
RULES = [
    (r"\bsed\b[^|;&]*\s-i\b", "sed -i: BSD sed reads the next argument as the backup suffix (write to a file and mv)"),
    (r"\bsed\b[^|;&]*\s-r\b", "sed -r: GNU only (sed -E is in both)"),
    (r"\bfind\b[^|;&]*\s-(printf|regextype|readable|writable|executable)\b", "find -printf / -regextype / -readable: GNU only"),
    (r"\btar\b[^|;&]*--(wildcards|transform|xform|anchored|no-wildcards-match-slash|wildcards-match-slash)\b",
     "tar --wildcards / --transform: GNU tar only (bsdtar refuses them)"),
    (r"(^|[;&|(]\s*|\bthen\s+|\bdo\s+|\bxargs\s+(-\S+\s+)*)(sha256sum|sha1sum|sha512sum|md5sum|b2sum)\b",
     "sha256sum and friends: absent on macOS (use shasum -a 256 when sha256sum is missing)"),
    (r"\breadlink\s+-[a-zA-Z]*f", "readlink -f: GNU only before macOS 12.3"),
    (r"\bstat\s+(-c|--format|--printf)\b", "stat -c: GNU only (BSD stat -f has other formats)"),
    (r"\bdate\s+(-d|--date)\b", "date -d: GNU only"),
    (r"\bgrep\b[^|;&]*\s-[a-zA-Z]*P", "grep -P: not in BSD grep"),
    (r"\bxargs\b[^|;&]*\s(-r\b|--no-run-if-empty)", "xargs -r: GNU only (and BSD xargs never runs on empty input)"),
    (r"\bcp\b[^|;&]*\s(--parents|-t\s|--target-directory)", "cp --parents / -t: GNU only"),
    (r"\bbase64\b[^|;&]*\s-w\b", "base64 -w: GNU only"),
    (r"\bhead\s+-[cn]\s*-\d", "head -n -N: GNU only"),
    (r"(^|[;&|(]\s*)(nproc|timeout|tac|numfmt)\b", "nproc / timeout / tac / numfmt: not on macOS"),
    (r"\binstall\s+-D\b", "install -D: GNU only"),
    (r"\bmktemp\s+--", "mktemp --option: GNU only"),
    (r"\b(declare|local|typeset)\s+-[a-zA-Z]*[An]", "declare -A / -n: bash 4 (macOS has bash 3.2)"),
    (r"(^|[;&|(]\s*)(mapfile|readarray|coproc)\b", "mapfile / readarray / coproc: bash 4"),
    (r"\$\{[A-Za-z_][A-Za-z0-9_]*(,,?|\^\^?)\}", "${x,,} / ${x^^}: bash 4"),
    (r"\|&|;;&|&>>", "|& ;;& &>>: bash 4"),
    (r"\[\[\s+-v\b", "[[ -v ]]: bash 4.2"),
    (r"\$\{[A-Za-z_][A-Za-z0-9_]*\[-\d+\]\}", "negative array index: bash 4.3"),
    (r"\bwait\s+-n\b|\$EPOCHSECONDS|\$EPOCHREALTIME", "wait -n / EPOCHSECONDS: bash 4.3 / 5"),
    (r"\bshopt\s+-s\s+(globstar|lastpipe|autocd)\b", "shopt globstar / lastpipe: bash 4"),
]


def code_lines(text):
    """(line number, code) of a shell text: comments and heredoc bodies removed, continuations kept apart."""
    lines = text.split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]
        number = i + 1
        i += 1
        heredoc = re.search(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1", line)
        code = re.sub(r"(^|\s)#.*$", "", line) if not line.lstrip().startswith("#!") else ""
        yield number, code, line
        if heredoc:
            delim = heredoc.group(2)
            while i < len(lines) and lines[i].strip() != delim:
                i += 1
            i += 1


def scan(name, text, findings):
    for number, code, raw in code_lines(text):
        if "# portable:" in raw:
            continue
        for pattern, why in RULES:
            m = re.search(pattern, code)
            if not m:
                continue
            if "sha256sum" in why and re.search(r"command\s+-v\s+(sha256sum|md5sum|sha1sum)|=\(\s*sha256sum", code):
                continue
            findings.append(f"{name}:{number}: {why}\n    {raw.strip()}")


def main():
    jobs = ci_model.jobs()
    tracked = ci_model.tracked_paths()
    mac = [j for j in jobs.values() if j.workflow.name == "macos.yml"]
    if not mac:
        print("check-portable.py: no macos.yml jobs", file=sys.stderr)
        return 1
    findings, scripts = [], set(ALSO)
    for job in mac:
        for number, step in enumerate(job.steps, start=1):
            if isinstance(step, dict) and "run" in step:
                scan(f".github/workflows/macos.yml job {job.name} step {number}", str(step["run"]), findings)
        for cmd in ci_model.trace(job, tracked):
            target = ci_model.script_invoked(cmd, tracked)
            if target:
                scripts.add(target)
    # what the owner's own scripts call, followed the same way
    for script in list(ALSO):
        fake = ci_model.Job(ROOT / ".github/workflows/macos.yml", "owner", {"steps": [{"run": f"bash {script} ORG REPO KEY"}]}, {})
        for cmd in ci_model.trace(fake, tracked):
            target = ci_model.script_invoked(cmd, tracked)
            if target:
                scripts.add(target)
    shell = sorted(s for s in scripts if s.endswith(".sh") or ci_model._is_shell_script(s))
    for script in shell:
        scan(script, (ROOT / script).read_text(encoding="utf-8", errors="replace"), findings)
    print("Shell run on macOS (macos.yml's steps and the scripts they run, and the owner's publish script):")
    for script in shell:
        print(f"  {script}")
    if findings:
        print()
        for f in findings:
            print(f"FINDING {f}")
        print(f"check-portable.py: {len(findings)} GNU-only or bash-4 construct(s)", file=sys.stderr)
        return 1
    print(f"check-portable.py: {len(shell)} scripts and {sum(len(j.steps) for j in mac)} steps, nothing GNU-only or newer than bash 3.2")
    return 0


if __name__ == "__main__":
    sys.exit(main())
