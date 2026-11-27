"""What this repository's CI runs, read from the workflows and the scripts they call (standard library only).

Used by scripts/ci/check-row-tests.py (does a named CI job really run a named test?) and
scripts/ci/check-portable.py (which scripts does the macOS workflow run?).

- `load_yaml`: the YAML these workflows are written in (block mappings and sequences, plain and quoted
  scalars, literal and folded block scalars, comments, simple flow sequences and mappings). Not general
  YAML: no anchors, aliases, tags or multi-document streams. `python3 -I scripts/ci/ci_model.py --compare-yq`
  checks it against yq (mikefarah, v4) on every workflow of the repository.
- `jobs`: every job of .github/workflows/*.yml with its workflow's triggers, defaults, environment and steps.
- `run_steps` (`--steps JOB…`): the job's `run:` steps as a terminal can run them (the justfile's job recipes run
  exactly these); `uses:` steps are the runner's setup and are left out.
- `trace`: the commands a job runs, from its `run:` steps and, recursively, the shell scripts they call
  (`bash X.sh`, `./X.sh`, `source X`), each with the directory it runs in and the variables set before it.
  A static reading. Commands under a step's or job's `if:` (unless always true) or `continue-on-error`, run
  in an `if`/`while`/`until` condition or in the background, whose failure a `|| X` masks (X not failing),
  piped without pipefail, or not last in a text without errexit (GitHub's default step shell is `bash -e`,
  `shell: bash` adds pipefail; a script has what its `set` lines give it), are traced with a `guard` and count
  for nothing; tracing stops at an `exit` at the top level of a script or
  step, and at `[ A = B ] && exit` when A and B resolve (a script's arguments are $1, $2 ...). Any other
  condition is read as if it held (an over-approximation); a directory that cannot be resolved makes the
  commands after it count nowhere; a test that skips itself or fails without an environment variable is seen
  through the gates `rust_env_gates` finds.
- `cargo_runs`, `forge_runs`, `invokes`: whether one traced command runs a given Rust test file, Foundry
  test file or script. A Rust test compiled out by a feature the command leaves off does not count
  (`feature_gates`: a file-level `#![cfg(feature = "X")]`, `#[cfg(feature = "X")]` on the test or on the
  `mod` block holding it; other `cfg` forms are not read).

Not caught (scripts/ci/README.md, "What it does not catch"): a job that `needs:` a skipped job; `on:` filters that
match nothing; `--ignored` or `--list` on the test binary; `exec` before the test; an `exit` inside a function, a
compound command or an and-list; `trap 'exit 0' EXIT` or `ERR`; a subshell or brace group masked by `|| true`;
`cfg` forms other than a bare feature gate; un-fenced 4-space indented doc-tests, `#[doc = include_str!(...)]`,
macro-generated doc-tests, and item-level `#[cfg]` on doc-tests.
"""

from __future__ import annotations

import fnmatch
import json
import os
import pathlib
import re
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"


# ── YAML (the subset the workflows use) ────────────────────────────────────────────────────────────


class YamlError(ValueError):
    pass


def _strip_comment(s: str) -> str:
    """Drop a trailing `# comment` that is outside quotes and preceded by whitespace (or starts the line)."""
    quote = None
    for i, c in enumerate(s):
        if quote:
            if c == quote:
                if quote == "'" and i + 1 < len(s) and s[i + 1] == "'":
                    continue
                quote = None
            elif c == "\\" and quote == '"':
                continue
        elif c in "'\"" and (i == 0 or s[i - 1] in " \t:[{,"):
            quote = c
        elif c == "#" and (i == 0 or s[i - 1] in " \t"):
            return s[:i].rstrip()
    return s.rstrip()


def _scalar(s: str):
    s = s.strip()
    if s in ("", "~", "null"):
        return None if s else ""
    if s in ("true", "false"):
        return s == "true"
    if re.fullmatch(r"-?\d+", s):
        return int(s)
    if s.startswith('"') and s.endswith('"') and len(s) >= 2:
        return json.loads(s)
    if s.startswith("'") and s.endswith("'") and len(s) >= 2:
        return s[1:-1].replace("''", "'")
    if s.startswith("["):
        return _flow(s)
    if s.startswith("{"):
        return _flow(s)
    return s


def _flow(s: str):
    """A flow sequence or mapping of scalars, possibly nested: [a, 'b', {c: d}]."""
    pos = 0

    def ws():
        nonlocal pos
        while pos < len(s) and s[pos] in " \t":
            pos += 1

    def item():
        nonlocal pos
        ws()
        if s[pos] == "[":
            pos += 1
            out = []
            ws()
            if s[pos] == "]":
                pos += 1
                return out
            while True:
                out.append(item())
                ws()
                if s[pos] == ",":
                    pos += 1
                    continue
                if s[pos] == "]":
                    pos += 1
                    return out
                raise YamlError(f"bad flow sequence: {s}")
        if s[pos] == "{":
            pos += 1
            out = {}
            ws()
            if s[pos] == "}":
                pos += 1
                return out
            while True:
                k = word(":")
                pos += 1
                out[k] = item()
                ws()
                if s[pos] == ",":
                    pos += 1
                    continue
                if s[pos] == "}":
                    pos += 1
                    return out
                raise YamlError(f"bad flow mapping: {s}")
        return _scalar(word(",]}"))

    def word(stops):
        nonlocal pos
        ws()
        start = pos
        if s[pos] in "'\"":
            q = s[pos]
            pos += 1
            while s[pos] != q or (q == "'" and pos + 1 < len(s) and s[pos + 1] == "'"):
                pos += 2 if (q == "'" and s[pos] == "'") or (q == '"' and s[pos] == "\\") else 1
            pos += 1
            return s[start:pos]
        depth = 0
        while pos < len(s) and (depth or s[pos] not in stops):
            if s[pos : pos + 3] == "${{":
                depth += 1
            elif s[pos : pos + 2] == "}}" and depth:
                depth -= 1
                pos += 1
            pos += 1
        return s[start:pos].strip()

    value = item()
    ws()
    if pos != len(s):
        raise YamlError(f"trailing text after a flow collection: {s}")
    return value


class _Parser:
    def __init__(self, text: str):
        self.lines = text.replace("\t", "    ").split("\n")
        self.i = 0

    @staticmethod
    def indent(s: str) -> int:
        return len(s) - len(s.lstrip(" "))

    def skip(self):
        while self.i < len(self.lines):
            t = self.lines[self.i].strip()
            if t == "" or t.startswith("#") or t == "---":
                self.i += 1
                continue
            break

    def peek(self):
        self.skip()
        if self.i >= len(self.lines):
            return None, None
        s = self.lines[self.i]
        return self.indent(s), s

    def node(self, min_indent: int):
        n, s = self.peek()
        if s is None or n < min_indent:
            return None
        body = s[n:]
        if body == "-" or body.startswith("- "):
            return self.seq(n)
        return self.map(n)

    @staticmethod
    def is_entry(body: str) -> bool:
        return bool(re.match(r"""^(?:"[^"]*"|'[^']*'|[^\s#'"\[\]{},][^#]*?):(?:\s|$)""", body))

    @staticmethod
    def split_entry(body: str):
        m = re.match(r"""^("[^"]*"|'[^']*'|[^\s#'"\[\]{},][^#]*?):(?:\s+(.*))?$""", body)
        if not m:
            raise YamlError(f"not a mapping entry: {body}")
        key = m.group(1)
        if key[0] in "'\"":
            key = _scalar(key)
        return key, (m.group(2) or "")

    def map(self, ind: int):
        out = {}
        while True:
            n, s = self.peek()
            if s is None or n < ind:
                return out
            if n > ind:
                raise YamlError(f"line {self.i + 1}: unexpected indentation")
            body = s[n:]
            if body == "-" or body.startswith("- "):
                return out
            key, rest = self.split_entry(body)
            self.i += 1
            out[key] = self.value(rest, ind)

    def seq(self, ind: int):
        out = []
        while True:
            n, s = self.peek()
            if s is None or n != ind:
                return out
            body = s[n:]
            if not (body == "-" or body.startswith("- ")):
                return out
            rest = body[2:] if body.startswith("- ") else ""
            if _strip_comment(rest).strip() == "":
                self.i += 1
                out.append(self.node(ind + 1))
            elif self.is_entry(rest):
                lead = len(rest) - len(rest.lstrip(" "))
                self.lines[self.i] = " " * (ind + 2 + lead) + rest.lstrip(" ")
                out.append(self.map(ind + 2 + lead))
            else:
                self.i += 1
                out.append(self.value(rest, ind))

    def block_scalar(self, header: str, ind: int) -> str:
        style, chomp = header[0], ""
        for c in header[1:]:
            if c in "+-":
                chomp = c
        lines, block_indent = [], None
        while self.i < len(self.lines):
            s = self.lines[self.i]
            if s.strip() == "":
                lines.append("")
                self.i += 1
                continue
            n = self.indent(s)
            if n <= ind:
                break
            if block_indent is None:
                block_indent = n
            if n < block_indent:
                break
            lines.append(s[block_indent:])
            self.i += 1
        while lines and lines[-1] == "" and chomp != "+":
            lines.pop()
        if style == "|":
            text = "\n".join(lines)
        else:
            text, para = "", []
            for line in lines + [""]:
                if line == "":
                    if para:
                        text += " ".join(para) + "\n"
                        para = []
                    else:
                        text += "\n"
                else:
                    para.append(line)
            text = text.rstrip("\n")
        if chomp == "-" or not lines:
            return text
        return text + "\n"

    def value(self, rest: str, ind: int):
        raw = rest.strip()
        if raw.startswith(("|", ">")):
            return self.block_scalar(_strip_comment(raw), ind)
        rest = _strip_comment(rest).strip()
        if rest == "":
            n, s = self.peek()
            if s is not None and n > ind:
                return self.node(n)
            if s is not None and n == ind and (s[n:] == "-" or s[n:].startswith("- ")):
                return self.seq(n)
            return None
        if rest[0] in "'\"" and not (len(rest) >= 2 and rest[-1] == rest[0]):
            # A quoted scalar continued on the following lines (folded with spaces).
            parts = [rest]
            while self.i < len(self.lines):
                s = self.lines[self.i].strip()
                self.i += 1
                parts.append(s)
                if s.endswith(rest[0]):
                    break
            return _scalar(" ".join(parts))
        if rest[0] not in "[{'\"":
            # A plain scalar continued on more-indented lines.
            parts = [rest]
            while self.i < len(self.lines):
                s = self.lines[self.i]
                if s.strip() == "" or self.indent(s) <= ind or s.strip().startswith("#"):
                    break
                parts.append(_strip_comment(s.strip()))
                self.i += 1
            rest = " ".join(parts)
        return _scalar(rest)


def load_yaml(text: str):
    parser = _Parser(text)
    value = parser.node(0)
    parser.skip()
    if parser.i < len(parser.lines):
        raise YamlError(f"line {parser.i + 1}: could not parse: {parser.lines[parser.i].strip()}")
    return value


# ── The workflows and their jobs ───────────────────────────────────────────────────────────────────


class Job:
    def __init__(self, workflow: pathlib.Path, name: str, spec: dict, doc: dict):
        self.workflow = workflow
        self.name = name
        self.spec = spec or {}
        on = doc.get("on", doc.get(True))  # YAML 1.1 readers turn `on` into True
        if isinstance(on, str):
            on = {on: None}
        elif isinstance(on, list):
            on = {k: None for k in on}
        self.triggers = sorted((on or {}).keys())
        self.env = {**(doc.get("env") or {}), **(self.spec.get("env") or {})}
        defaults = ((doc.get("defaults") or {}).get("run") or {}) | ((self.spec.get("defaults") or {}).get("run") or {})
        self.default_dir = defaults.get("working-directory")
        self.steps = self.spec.get("steps") or []

    @property
    def on_change(self) -> bool:
        """Runs on pushes or pull requests (CI proper), not only when started by hand or on a schedule."""
        return "push" in self.triggers or "pull_request" in self.triggers

    def __repr__(self):
        return f"{self.workflow.name}:{self.name}"


def jobs(root: pathlib.Path = ROOT) -> dict:
    out = {}
    for path in sorted((root / ".github" / "workflows").glob("*.yml")):
        doc = load_yaml(path.read_text(encoding="utf-8")) or {}
        for name, spec in (doc.get("jobs") or {}).items():
            if name in out:
                raise YamlError(f"job {name} is defined in {out[name].workflow.name} and {path.name}")
            out[name] = Job(path, name, spec, doc)
    return out


# ── The shell commands a job runs ──────────────────────────────────────────────────────────────────


class Cmd:
    """One simple command: its words, the directory it runs in (repository-relative, None when unknown),
    the variables set for it (exported or not), and where it comes from. `guard` says why the command cannot
    count as CI running a test even though it is in the trace: its step or job runs only under an `if:` or has
    `continue-on-error`, its failure is masked (`|| true`, run in the background), or it is a condition."""

    def __init__(self, argv, cwd, env, origin, guard=None):
        self.argv, self.cwd, self.env, self.origin, self.guard = argv, cwd, env, origin, guard

    def __repr__(self):
        return f"[{self.cwd}] {' '.join(self.argv)}  ({self.origin})"


_OPS = ("&&", "||", ";;", ";", "|", "&", "(", ")", "\n")
_KEYWORDS_LEAD = {"if", "then", "else", "elif", "do", "while", "until", "!", "{", "time"}
_KEYWORDS_ALONE = {"fi", "done", "esac", "}"}
_WRAPPERS = {"nice", "env", "timeout", "sudo", "command", "exec", "nohup", "stdbuf"}


def _tokens(text: str):
    """Shell words and operators. Quotes are kept on the words (removed later); `$(...)`, `${...}` and
    backticks are kept whole inside their word; heredoc bodies are dropped; comments are dropped."""
    lines, out_lines, i = text.split("\n"), [], 0
    while i < len(lines):
        line = lines[i]
        out_lines.append(line)
        i += 1
        for m in re.finditer(r"<<(-?)\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\2", line):
            if "'" in line[: m.start()].replace("\\'", "") and line[: m.start()].count("'") % 2:
                continue  # inside a single-quoted string
            delim, strip_tabs = m.group(3), m.group(1) == "-"
            while i < len(lines) and (lines[i].strip() if strip_tabs else lines[i]) != delim:
                i += 1
            i += 1
    src = "\n".join(out_lines).replace("\\\n", " ")
    toks, word, pos, n = [], "", 0, len(src)

    def flush():
        nonlocal word
        if word:
            toks.append(word)
            word = ""

    while pos < n:
        c = src[pos]
        if c == "'" :
            end = src.index("'", pos + 1) if "'" in src[pos + 1 :] else n
            word += src[pos : end + 1]
            pos = end + 1
        elif c == '"':
            j = pos + 1
            depth = 0
            while j < n and (src[j] != '"' or depth):
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j : j + 2] == "$(":
                    depth += 1
                    j += 2
                    continue
                if src[j] == ")" and depth:
                    depth -= 1
                j += 1
            word += src[pos : j + 1]
            pos = j + 1
        elif c == "\\" and pos + 1 < n:
            word += src[pos : pos + 2]
            pos += 2
        elif src.startswith("$(", pos) or src.startswith("${", pos) or c == "`":
            close = {"$(": ")", "${": "}"}.get(src[pos : pos + 2], "`")
            opener = src[pos + 1] if close != "`" else "`"
            j, depth = pos + (2 if close != "`" else 1), 1
            while j < n and depth:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] in "'\"" and close != "`":
                    q = src[j]
                    k = src.find(q, j + 1)
                    j = (k if k >= 0 else n - 1) + 1
                    continue
                if close == "`":
                    if src[j] == "`":
                        depth = 0
                elif src[j] == opener:
                    depth += 1
                elif src[j] == close:
                    depth -= 1
                j += 1
            word += src[pos:j]
            pos = j
        elif c == "#" and not word:
            while pos < n and src[pos] != "\n":
                pos += 1
        elif c in " \t":
            flush()
            pos += 1
        else:
            for op in _OPS:
                if src.startswith(op, pos):
                    flush()
                    toks.append(op)
                    pos += len(op)
                    break
            else:
                if c in "<>" or (c.isdigit() and pos + 1 < n and src[pos + 1] in "<>" and not word):
                    # a redirection: the operator, then (unless it is >&N) its target as the next word
                    flush()
                    m = re.match(r"\d?(>>|>&\d?|<&\d?|&>>|&>|<<<|<<-|<<|<|>\|?|>)", src[pos:])
                    op = m.group(0) if m else c
                    toks.append("REDIR:" + op)
                    pos += len(op)
                else:
                    word += c
                    pos += 1
    flush()
    return toks


def _unquote(word: str, vars_: dict) -> str | None:
    """A word with its quotes removed and its variables expanded; None if it holds anything unknown."""
    out, pos, n, dq = "", 0, len(word), False
    while pos < n:
        c = word[pos]
        if c == "'" and not dq:
            end = word.index("'", pos + 1) if "'" in word[pos + 1 :] else n
            out += word[pos + 1 : end]
            pos = end + 1
            continue
        if c == '"':
            dq = not dq
            pos += 1
            continue
        if c == "\\" and pos + 1 < n:
            if dq and word[pos + 1] not in '$`"\\':
                out += c
                pos += 1
                continue
            out += word[pos + 1]
            pos += 2
            continue
        if c == "$":
            m = re.match(r"\$(?:\{([A-Za-z_][A-Za-z0-9_]*|\d)(:?[-=+?])?([^}]*)\}|([A-Za-z_][A-Za-z0-9_]*|\d))", word[pos:])
            if word.startswith("$(", pos):
                depth, j = 0, pos + 1
                while j < n:
                    if word[j] == "(":
                        depth += 1
                    elif word[j] == ")":
                        depth -= 1
                        if depth == 0:
                            break
                    j += 1
                value = _command_substitution(word[pos + 2 : j], vars_)
                if value is None:
                    return None
                out += value
                pos = j + 1
                continue
            if m:
                name = m.group(1) or m.group(4)
                if name in vars_ and vars_[name] is not None:
                    value = vars_[name]
                    if m.group(2) and m.group(2).endswith("+"):
                        value = m.group(3)
                elif m.group(2) and m.group(2).endswith(("-", "=")):
                    value = _unquote(m.group(3), vars_) if m.group(3) else ""
                    if value is None:
                        return None
                else:
                    return None
                out += value
                pos += len(m.group(0))
                continue
            out += c
            pos += 1
            continue
        out += c
        pos += 1
    return out


def _command_substitution(inner: str, vars_: dict) -> str | None:
    """The few command substitutions that name a directory: `cd X && pwd`, `dirname X`."""
    inner = inner.strip()
    m = re.fullmatch(r"cd\s+(.+?)\s*&&\s*pwd(?:\s+-P)?", inner)
    if m:
        target = _unquote(m.group(1), vars_)
        return os.path.normpath(target) if target is not None else None
    m = re.fullmatch(r"dirname\s+(.+)", inner)
    if m:
        target = _unquote(m.group(1), vars_)
        return os.path.dirname(target) or "." if target is not None else None
    return None


def _split_commands(toks):
    """Simple commands, each with the subshell depth it runs at and the operator that ends it (`&&`, `||`, `|`,
    `&`, `;`, a newline, or None); `case` patterns and function headers dropped, a `case` block marked."""
    cmds, cur, depth, in_case, after_case_in = [], [], 0, 0, False

    def end(op=None):
        nonlocal cur
        if cur:
            cmds.append((cur, depth, op))
        cur = []

    i = 0
    while i < len(toks):
        t = toks[i]
        if in_case and after_case_in:
            # a case pattern: words up to ")"
            j = i
            while j < len(toks) and toks[j] != ")":
                if toks[j] == "esac":
                    break
                j += 1
            if j < len(toks) and toks[j] == ")":
                i = j + 1
                after_case_in = False
                continue
            after_case_in = False
        if t in ("\n", ";", "&&", "||", "|", "&"):
            end(t)
        elif t == ";;":
            end(t)
            after_case_in = True
        elif t == "(":
            if cur and i + 1 < len(toks) and toks[i + 1] == ")":
                cur = []  # a function header: name ( )
                i += 2
                continue
            end()
            depth += 1
            cmds.append((["__SUBSHELL_OPEN__"], depth, None))
        elif t == ")":
            end(")")
            cmds.append((["__SUBSHELL_CLOSE__"], depth, None))
            depth = max(0, depth - 1)
        elif not cur and t == "case":
            in_case += 1
            cmds.append((["__CASE_OPEN__"], depth, None))
            while i < len(toks) and toks[i] not in ("in", "\n"):
                i += 1
            after_case_in = True
            i += 1
            continue
        elif not cur and t == "esac":
            end()
            cmds.append((["__CASE_CLOSE__"], depth, None))
            in_case = max(0, in_case - 1)
            after_case_in = False
        else:
            cur.append(t)
        i += 1
    end()
    return cmds


def _resolve(cwd, path: str):
    """A path relative to the repository, or None if it leaves the repository or cwd is unknown."""
    if os.path.isabs(path):
        try:
            rel = os.path.relpath(os.path.normpath(path), ROOT)
        except ValueError:
            return None
    else:
        if cwd is None:
            return None
        rel = os.path.normpath(os.path.join(cwd, path))
    return None if rel.startswith("..") else rel


# A step or job condition that always lets it run.
_ALWAYS = re.compile(r"^\s*(?:\$\{\{\s*)?(?:true|always\(\)|success\(\)|!\s*cancelled\(\))(?:\s*\}\})?\s*$")


def step_guard(job: Job, step: dict) -> str | None:
    """Why a step's commands cannot count as CI running a test: its job or the step runs only under an `if:` that
    is not always true, or has `continue-on-error` (a failure there does not fail the run). None if neither."""
    for where, spec in (("job", job.spec), ("step", step)):
        cond = spec.get("if")
        if cond is not None and cond is not True and not _ALWAYS.match(str(cond)):
            return f"the {where} runs only if {cond}"
        coe = spec.get("continue-on-error")
        if coe is not None and coe is not False and str(coe).strip().lower() not in ("false", "${{ false }}"):
            return f"the {where} has continue-on-error: {coe} (a failure there does not fail the run)"
    return None


def _matrix_combos(job: Job) -> list[dict]:
    """The matrix combinations of a job's `strategy.matrix`, or `[{}]` if it is not a matrix job."""
    matrix = (job.spec.get("strategy") or {}).get("matrix")
    if not isinstance(matrix, dict):
        return [{}]
    axes = [(k, v) for k, v in matrix.items() if k not in ("include", "exclude") and isinstance(v, list)]
    combos = [{}]
    for k, vals in axes:
        combos = [{**c, k: v} for c in combos for v in vals]
    for ex in matrix.get("exclude") or []:
        if isinstance(ex, dict):
            combos = [c for c in combos if not all(c.get(k) == v for k, v in ex.items())]
    for inc in matrix.get("include") or []:
        if isinstance(inc, dict):
            combos.append(dict(inc))
    return combos


def _subst_matrix(text: str, combo: dict) -> str:
    if not combo or "${{" not in text:
        return text
    def repl(m):
        key = m.group(1)
        return str(combo[key]) if key in combo else m.group(0)
    return re.sub(r"\$\{\{\s*matrix\.([A-Za-z0-9_-]+)\s*\}\}", repl, text)


def run_steps(job: Job) -> list[dict]:
    """The job's `run:` steps as a terminal can run them, one entry per matrix combination, in order: the
    step's text with the matrix values substituted, the repository-relative directory it runs in, the
    variables set before it (the workflow's and the step's) and the step's shell options (errexit,
    pipefail: GitHub's default step shell is `bash -e`, `shell: bash` adds pipefail). A step with `uses:`
    is the runner's setup, not a command, and is left out; `if:` and `continue-on-error:` stay in the
    entry's `guard`, for the caller to judge. `trace` reads the same entries, so the two cannot disagree."""
    out = []
    for combo in _matrix_combos(job):
        base_env = {k: _subst_matrix(str(v), combo) for k, v in job.env.items()}
        for number, step in enumerate(job.steps, start=1):
            if not isinstance(step, dict) or "run" not in step:
                continue
            step_eval = dict(step)
            if "if" in step_eval and isinstance(step_eval["if"], str):
                step_eval["if"] = _subst_matrix(step_eval["if"], combo)
            cwd = _subst_matrix(str(step.get("working-directory") or job.default_dir or "."), combo)
            cwd = os.path.normpath(cwd)
            env = {**base_env, **{k: _subst_matrix(str(v), combo) for k, v in (step.get("env") or {}).items()}}
            env.setdefault("GITHUB_WORKSPACE", str(ROOT))
            shell = str(step.get("shell") or (job.spec.get("defaults") or {}).get("run", {}).get("shell") or "")
            errexit = shell in ("", "bash", "sh") or bool(re.search(r"(?:^|\s)-[a-z]*e", shell))
            pipefail = shell == "bash" or "pipefail" in shell
            out.append({"text": _subst_matrix(str(step["run"]), combo), "cwd": cwd, "env": env, "combo": combo,
                        "step": number, "shell": shell, "errexit": errexit, "pipefail": pipefail,
                        "guard": step_guard(job, step_eval)})
    return out


def trace(job: Job, tracked: set | None = None):
    """Every command the job runs: its steps' `run:` text and, recursively, the scripts they call."""
    tracked = tracked if tracked is not None else tracked_paths()
    out = []
    for entry in run_steps(job):
        tag = (f"{job.workflow.name} job {job.name}"
               + (f" [{entry['combo']}]" if entry["combo"] else "") + f" step {entry['step']}")
        _run_text(entry["text"], entry["cwd"], entry["env"], tag, out, tracked, 0,
                  guard=entry["guard"], errexit=entry["errexit"], pipefail=entry["pipefail"])
    return out


# Commands after which a `||` keeps the failure: the fallback itself fails.
def _fails(words) -> bool:
    words = [w for w in words if not w.startswith("REDIR:")]
    while words and words[0] in ("{", "("):
        words = words[1:]
    if not words:
        return False
    if words[0] == "false":
        return True
    if words[0] in ("exit", "return"):
        return len(words) > 1 and words[1] not in ("0", '"0"', "'0'")
    return False


def _test_value(words, vars_):
    """The value of a `[ ... ]` / `test ...` / `[[ ... ]]` command whose operands resolve, else None."""
    if not words or words[0] not in ("[", "test", "[["):
        return None
    body = words[1:]
    if words[0] in ("[", "[[") and body and body[-1] in ("]", "]]"):
        body = body[:-1]
    vals = [_unquote(w, vars_) for w in body]
    if any(v is None for v in vals):
        return None
    if len(vals) == 3 and vals[1] in ("=", "=="):
        return vals[0] == vals[2]
    if len(vals) == 3 and vals[1] == "!=":
        return vals[0] != vals[2]
    if len(vals) == 2 and vals[0] == "-z":
        return vals[1] == ""
    if len(vals) == 2 and vals[0] == "-n":
        return vals[1] != ""
    return None


def _run_text(text, cwd, vars_, origin, out, tracked, level, script=None, guard=None, args=None, errexit=False,
              pipefail=False):
    """Trace one step's `run:` text or one script. Commands after an `exit` at the top level of the text (not
    inside a block, a function or a subshell) are not run; neither are those after `[ A = B ] && exit` when A
    and B resolve (with the script's arguments as $1, $2 ...) and are equal. A command followed by `|| X`, where
    X does not fail, has its failure masked, as has the &&-chain before it; so has a command run with `&`; a
    command in an `if`/`while`/`until` condition does not fail the text by failing. Without `set -e` (errexit) only
    the text's last command can fail it; without pipefail, a command piped into another cannot."""
    vars_ = dict(vars_)
    if script is not None:
        vars_["0"] = str(ROOT / script)
        for k in range(1, 10):
            vars_[str(k)] = ""
        for k, a in enumerate(args or [], start=1):
            if k < 10:
                vars_[str(k)] = a
        vars_["#"] = str(len(args or []))
    stack = []
    block = 0  # if/while/until/for/select/case/{ } nesting
    commands = _split_commands(_tokens(text))
    last_index = max((k for k, (w, _d, _o) in enumerate(commands) if not w[0].startswith("__")), default=-1)
    chain = []  # the commands of the current && chain (masked together by a later `|| true`)
    prev_op = None
    last_test = None  # (value, ...) of a `[ ... ]` just before `&&`
    for index, (words, depth, op) in enumerate(commands):
        if words == ["__SUBSHELL_OPEN__"]:
            stack.append(cwd)
            prev_op = None
            continue
        if words == ["__SUBSHELL_CLOSE__"]:
            cwd = stack.pop() if stack else cwd
            prev_op = None
            continue
        if words == ["__CASE_OPEN__"]:
            block += 1
            continue
        if words == ["__CASE_CLOSE__"]:
            block = max(0, block - 1)
            continue
        after = prev_op
        prev_op = op
        if after not in ("&&", "||"):
            chain = []
        # redirections and their targets out
        clean, skip = [], False
        for w in words:
            if skip:
                skip = False
                continue
            if w.startswith("REDIR:"):
                skip = not re.search(r"&\d$", w)
                continue
            clean.append(w)
        words = clean
        if words and words[0] in ("if", "while", "until", "for", "select", "{"):
            block += 1
        condition = bool(words) and words[0] in ("if", "elif", "while", "until", "!")
        if words and words[0] in ("fi", "done", "}"):
            block = max(0, block - 1)
        while words and words[0] in _KEYWORDS_LEAD:
            words = words[1:]
        if not words or words[0] in _KEYWORDS_ALONE:
            continue
        if words[0] in ("for", "select", "function"):
            continue
        # assignments (alone: they set variables; before a command: its environment)
        env = {}
        while words and re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", words[0]):
            name, value = words[0].split("=", 1)
            env[name] = _unquote(value, vars_)
            words = words[1:]
        if not words:
            vars_.update(env)
            continue
        if words[0] in ("export", "local", "declare", "readonly"):
            for w in words[1:]:
                if re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", w):
                    name, value = w.split("=", 1)
                    vars_[name] = _unquote(value, vars_)
            continue
        if words[0] == "set":
            for k, w in enumerate(words[1:], start=1):
                if re.fullmatch(r"[-+][A-Za-z]+", w):
                    on = w.startswith("-")
                    if "e" in w[1:]:
                        errexit = on
                    if "o" in w[1:] and k + 1 < len(words) and words[k + 1] in ("pipefail", "errexit"):
                        if words[k + 1] == "pipefail":
                            pipefail = on
                        else:
                            errexit = on
            continue
        # exit: unconditional at the top level, or after a test that resolves to true
        if words[0] == "exit" and depth == 0 and block == 0:
            if after not in ("&&", "||"):
                return
            if after == "&&" and last_test is True:
                return
        last_test = _test_value(words, {**vars_, **env}) if op == "&&" else None
        argv = [_unquote(w, {**vars_, **env}) for w in words]
        argv = [a if a is not None else "\0?" for a in argv]
        # wrappers: nice -n 19 X, env A=B X, timeout 60 X, sudo X
        while argv and os.path.basename(argv[0]) in _WRAPPERS:
            head = os.path.basename(argv[0])
            argv = argv[1:]
            if head == "nice" and argv[:1] == ["-n"]:
                argv = argv[2:]
            elif head == "timeout":
                while argv and argv[0].startswith("-"):
                    argv = argv[1:]
                argv = argv[1:]
            elif head == "env":
                while argv and ("=" in argv[0] or argv[0].startswith("-")):
                    if "=" in argv[0]:
                        k, v = argv[0].split("=", 1)
                        env[k] = v
                    argv = argv[1:]
        if not argv or (argv[0] in ("-v", "-V") and words[0] == "command"):
            continue
        if argv[0] == "cd":
            target = argv[1] if len(argv) > 1 else None
            cwd = _resolve(cwd, target) if target and "\0?" not in target else None
            continue
        cmd_env = {**vars_, **env}
        why = guard
        if why is None and condition:
            why = "it is a condition (if/while/until): its failure does not fail the script"
        if why is None and op == "&":
            why = "it runs in the background (&): its exit status is not waited for"
        if why is None and op == "|" and not pipefail:
            why = "it is piped into another command without pipefail: its failure is lost"
        if why is None and not errexit and not all(commands[k][2] in ("&&", "||", "|") for k in range(index, last_index)):
            why = "the text runs without set -e and this is not its last command: a failure here does not fail it"
        cmd = Cmd(argv, cwd, cmd_env, origin, why)
        out.append(cmd)
        chain.append(cmd)
        if op == "||":
            fallback = commands[index + 1][0] if index + 1 < len(commands) else []
            if not _fails(fallback):
                for c in chain:
                    if c.guard is None:
                        c.guard = f"its failure is masked (`|| {' '.join(fallback) or '...'}`)"
        # scripts this command runs: recurse into shell scripts
        if level < 6:
            target = script_invoked(cmd, tracked)
            before = len(out)
            if target == "scripts/ci/lez-harness.sh":
                sargs = _script_args(cmd, target)
                if _trace_lez_shard(sargs, cmd_env, f"{target} (from {origin})", out, tracked, level + 1, cmd.guard):
                    chain.extend(out[before:])
                    continue
            if target and (target.endswith(".sh") or _is_shell_script(target)) and target != script:
                text = (ROOT / target).read_text(encoding="utf-8", errors="replace")
                _run_text(text, cwd, cmd_env, f"{target} (from {origin})", out, tracked, level + 1, script=target,
                          guard=cmd.guard, args=_script_args(cmd, target))
                chain.extend(out[before:])


_SHARD_MOD = None


def _shard_planner():
    """Load `check-lez-shards.py` lazily so `ci_model.py` shares its single `build_shard_plan`."""
    global _SHARD_MOD
    if _SHARD_MOD is None:
        existing = sys.modules.get("check_lez_shards")
        if existing is not None and hasattr(existing, "build_shard_plan"):
            _SHARD_MOD = existing
        else:
            import importlib.util
            path = pathlib.Path(__file__).resolve().parent / "check-lez-shards.py"
            spec = importlib.util.spec_from_file_location("check_lez_shards", path)
            mod = importlib.util.module_from_spec(spec)
            sys.modules["check_lez_shards"] = mod
            spec.loader.exec_module(mod)
            _SHARD_MOD = mod
    return _SHARD_MOD


def _trace_lez_shard(sargs: list[str], cmd_env: dict, origin: str, out: list, tracked: set, level: int, guard) -> bool:
    """Static trace of `scripts/ci/lez-harness.sh` when invoked with `--shard NAME` or `--dry-run`.
    Delegates command planning to `check-lez-shards.py:build_shard_plan` so there is only one planner.
    Returns True if handled (so the generic unsharded script trace is skipped), False for unsharded calls."""
    shard = None
    dry_run = False
    build_only = False
    i = 0
    while i < len(sargs):
        a = sargs[i]
        if a == "--shard" and i + 1 < len(sargs):
            shard = sargs[i + 1]
            i += 2
        elif a.startswith("--shard="):
            shard = a.split("=", 1)[1]
            i += 1
        elif a == "--dry-run":
            dry_run = True
            i += 1
        elif a == "build":
            build_only = True
            i += 1
        else:
            i += 1
    if dry_run:
        return True
    if shard is None:
        return False
    map_path = ROOT / "scripts" / "ci" / "lez-harness-shards.json"
    if not map_path.is_file():
        return True
    data = json.loads(map_path.read_text(encoding="utf-8"))
    threads = str(cmd_env.get("FERRATA_TEST_THREADS") or "4")
    try:
        planned = _shard_planner().build_shard_plan(
            data, shard, threads=threads, build_only=build_only, privacy_out="target/ci-privacy",
        )
    except (KeyError, ValueError):
        return True

    dev_env = {**cmd_env, "RISC0_DEV_MODE": "1"}
    for step in planned:
        env_for_step = dev_env if step["dev_mode"] else cmd_env
        cmd = Cmd(list(step["argv"]), step["cwd"], env_for_step, origin, guard)
        out.append(cmd)
        if level < 6:
            target = script_invoked(cmd, tracked)
            if target and (target.endswith(".sh") or _is_shell_script(target)):
                text = (ROOT / target).read_text(encoding="utf-8", errors="replace")
                _run_text(text, step["cwd"], env_for_step, f"{target} (from {origin})", out, tracked, level + 1,
                          script=target, guard=guard, args=_script_args(cmd, target))
    return True


def _script_args(cmd: Cmd, target: str):
    """The arguments a command passes to the script it runs (after the script's own path)."""
    argv = cmd.argv
    for k, a in enumerate(argv):
        if _resolve(cmd.cwd, a) == target:
            return argv[k + 1:]
    return []


def _is_shell_script(path: str) -> bool:
    try:
        first = (ROOT / path).read_text(encoding="utf-8", errors="replace").split("\n", 1)[0]
    except OSError:
        return False
    return first.startswith("#!") and ("bash" in first or first.rstrip().endswith("/sh"))


_INTERPRETERS = {"bash", "sh", "python3", "python", "node", "source", "."}


def script_invoked(cmd: Cmd, tracked: set) -> str | None:
    """The tracked script this command runs (as the command, or through an interpreter), if any."""
    argv = cmd.argv
    head = os.path.basename(argv[0])
    if head in _INTERPRETERS:
        rest = argv[1:]
        while rest and rest[0].startswith("-"):
            if rest[0] == "-c":
                return None
            rest = rest[1:]
        if not rest:
            return None
        cand = rest[0]
    else:
        cand = argv[0]
        if "/" not in cand:
            return None
    rel = _resolve(cmd.cwd, cand)
    return rel if rel in tracked else None


def tracked_paths() -> set:
    out = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z"], capture_output=True, check=True).stdout
    return {name for name in out.decode("utf-8", "surrogateescape").split("\0") if name}


# ── Does a command run a given test? ───────────────────────────────────────────────────────────────


def invokes(cmd: Cmd, path: str, tracked: set) -> bool:
    return cmd.guard is None and script_invoked(cmd, tracked) == path


def _toml(path: pathlib.Path) -> dict:
    with open(path, "rb") as handle:
        return tomllib.load(handle)


def _package_of(path: str):
    """The Cargo package a file belongs to: (package directory, its manifest)."""
    d = (ROOT / path).parent
    while d != ROOT.parent:
        manifest = d / "Cargo.toml"
        if manifest.is_file():
            data = _toml(manifest)
            if "package" in data:
                return d, data
        if d == ROOT:
            break
        d = d.parent
    return None, None


def _workspace_of(start: pathlib.Path):
    """The workspace root manifest above (or at) a directory, and its member package directories."""
    d = start
    while True:
        manifest = d / "Cargo.toml"
        if manifest.is_file():
            data = _toml(manifest)
            ws = data.get("workspace")
            if ws is not None:
                members = set()
                for pattern in ws.get("members", []):
                    for m in d.glob(pattern):
                        if (m / "Cargo.toml").is_file():
                            members.add(m.resolve())
                for pattern in ws.get("exclude", []):
                    for m in d.glob(pattern):
                        members.discard(m.resolve())
                if "package" in data:
                    members.add(d.resolve())
                default = {(d / m).resolve() for m in ws.get("default-members", [])} or None
                return d, data, members, default
        if d == ROOT or d == d.parent:
            return None, None, set(), None
        d = d.parent


def rust_env_gates(text: str) -> set:
    """Environment variables a test file must find set: it skips itself without them (`env::var("X").is_err()` and
    the like), or asserts their value (`assert_eq!(std::env::var("X").as_deref(), Ok("1"), ...)`) and fails."""
    gates = set(re.findall(r'env::var(?:_os)?\(\s*"([A-Z0-9_]+)"\s*\)\s*\.\s*(?:is_err|is_none)\(\)', text))
    gates |= set(re.findall(r'assert(?:_eq|_ne)?!\(\s*(?:std::)?env::var(?:_os)?\(\s*"([A-Z0-9_]+)"', text))
    return gates


_DOCTEST_TOKENS = {
    "", "rust", "no_run", "should_panic", "compile_fail", "standalone_crate",
    "allow_fail", "edition2015", "edition2018", "edition2021", "edition2024",
}


def _unescape_rust_string(raw: str) -> str:
    """Decode common Rust string escapes inside a literal `#[doc = "..."]` attribute."""
    out = []
    i, n = 0, len(raw)
    while i < n:
        if raw[i] == "\\" and i + 1 < n:
            nxt = raw[i + 1]
            if nxt == "n":
                out.append("\n")
                i += 2
            elif nxt == "r":
                out.append("\r")
                i += 2
            elif nxt == "t":
                out.append("\t")
                i += 2
            elif nxt in ('"', "'", "\\"):
                out.append(nxt)
                i += 2
            elif nxt == "\n":
                i += 2
                while i < n and raw[i] in " \t\n\r":
                    i += 1
            else:
                out.append(nxt)
                i += 2
        else:
            out.append(raw[i])
            i += 1
    return "".join(out)


_RE_DOC_ATTR = re.compile(r"(#!?)\[\s*doc\s*=\s*")
_RE_RAW_STR = re.compile(r'r(#*)"')
_RE_ANY_RAW_STR = re.compile(r'[bc]?r(#*)"')
_RE_CLOSE_BRACKET = re.compile(r"\s*\]")


def _extract_doc_blocks(text: str) -> list[list[str]]:
    """Extract contiguous Rust doc blocks (`///`, `//!`, `/** */`, `/*! */`, literal `#[doc = "..."]`)."""
    blocks: list[list[str]] = []
    cur: list[str] = []
    cur_kind: str | None = None

    def flush():
        nonlocal cur_kind
        if cur:
            blocks.append(list(cur))
            cur.clear()
        cur_kind = None

    def append_lines(kind: str, lines: list[str]):
        nonlocal cur_kind
        if cur_kind is not None and cur_kind != kind:
            flush()
        cur_kind = kind
        cur.extend(lines)

    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c in " \t\r":
            i += 1
            continue
        if c == "\n":
            j = i + 1
            while j < n and text[j] in " \t\r":
                j += 1
            if j < n and text[j] == "\n":
                flush()
            i += 1
            continue
        if c == "/" and i + 1 < n and text[i + 1] == "/":
            eol = text.find("\n", i)
            if eol == -1:
                eol = n
            line = text[i:eol]
            if line.startswith("///") and not line.startswith("////"):
                body = line[3:]
                append_lines("outer", [body[1:] if body.startswith((" ", "\t")) else body])
            elif line.startswith("//!"):
                body = line[3:]
                append_lines("inner", [body[1:] if body.startswith((" ", "\t")) else body])
            else:
                flush()
            i = eol
            continue
        if c == "/" and i + 1 < n and text[i + 1] == "*":
            start = i
            depth = 1
            i += 2
            while i < n and depth:
                if text[i : i + 2] == "/*":
                    depth += 1
                    i += 2
                elif text[i : i + 2] == "*/":
                    depth -= 1
                    i += 2
                else:
                    i += 1
            comment = text[start:i]
            is_inner = comment.startswith("/*!") and comment.endswith("*/") and len(comment) >= 5
            is_outer = (
                comment.startswith("/**")
                and not comment.startswith("/***")
                and comment.endswith("*/")
                and len(comment) > 4
            )
            if is_inner or is_outer:
                inner = comment[3:-2]
                out_lines = []
                for idx, bl in enumerate(inner.splitlines()):
                    if idx == 0:
                        out_lines.append(bl[1:] if bl.startswith((" ", "\t")) else bl)
                    else:
                        out_lines.append(re.sub(r"^[ \t]*\*[ \t]?", "", bl))
                append_lines("inner" if is_inner else "outer", out_lines)
            else:
                flush()
            continue
        if c == "#":
            m_attr = _RE_DOC_ATTR.match(text, i)
            if m_attr:
                kind = "inner" if m_attr.group(1) == "#!" else "outer"
                j = m_attr.end()
                doc_val = None
                m_raw = _RE_RAW_STR.match(text, j)
                if m_raw:
                    end_tok = '"' + m_raw.group(1)
                    k = m_raw.end()
                    end = text.find(end_tok, k)
                    if end != -1:
                        after = end + len(end_tok)
                        m_close = _RE_CLOSE_BRACKET.match(text, after)
                        if m_close:
                            doc_val = text[k:end]
                            i = m_close.end()
                elif j < n and text[j] == '"':
                    k = j + 1
                    while k < n and text[k] != '"':
                        if text[k] == "\\" and k + 1 < n:
                            k += 2
                            continue
                        k += 1
                    if k < n and text[k] == '"':
                        after = k + 1
                        m_close = _RE_CLOSE_BRACKET.match(text, after)
                        if m_close:
                            doc_val = _unescape_rust_string(text[j + 1 : k])
                            i = m_close.end()
                if doc_val is not None:
                    append_lines(kind, doc_val.splitlines() or [""])
                    continue
            flush()
            i += 1
            continue
        if c in ("r", "b", "c") and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            m_raw = _RE_ANY_RAW_STR.match(text, i)
            if m_raw:
                flush()
                end_tok = '"' + m_raw.group(1)
                k = m_raw.end()
                end = text.find(end_tok, k)
                i = n if end == -1 else end + len(end_tok)
                continue
        if c == '"':
            flush()
            i += 1
            while i < n and text[i] != '"':
                if text[i] == "\\" and i + 1 < n:
                    i += 2
                    continue
                i += 1
            if i < n:
                i += 1
            continue
        if c == "'" and i + 2 < n:
            if text[i + 1] == "\\":
                k = i + 2
                while k < n and text[k] != "'" and text[k] != "\n":
                    k += 1
                if k < n and text[k] == "'":
                    flush()
                    i = k + 1
                    continue
            elif text[i + 2] == "'" and text[i + 1] != "\n":
                flush()
                i += 3
                continue
        flush()
        i += 1
    flush()
    return blocks


def _block_has_doctest(lines: list[str], include_ignored: bool = False) -> bool:
    """Whether a single extracted doc block contains a runnable fenced code block (``` or ~~~)."""
    in_fence = False
    fence_char = ""
    fence_len = 0
    for line in lines:
        body = line.lstrip()
        if not in_fence:
            fm = re.match(r"^(`{3,}|~{3,})(.*)$", body)
            if not fm:
                continue
            marker, info = fm.group(1), fm.group(2).strip()
            if marker[0] == "`" and "`" in info:
                continue
            in_fence = True
            fence_char = marker[0]
            fence_len = len(marker)
            cleaned = re.sub(r"\([^)]*\)", "", info)
            tokens = [t.strip().lower() for t in re.split(r"[\s,]+", cleaned) if t.strip()]
            if any(t == "ignore" or t.startswith("ignore-") for t in tokens):
                if not include_ignored:
                    continue
                tokens = [t for t in tokens if t != "ignore" and not t.startswith("ignore-")]
            if all(t in _DOCTEST_TOKENS for t in tokens):
                return True
        elif re.match(rf"^{re.escape(fence_char)}{{{fence_len},}}[ \t]*$", body):
            in_fence = False
            fence_char = ""
            fence_len = 0
    return False


def has_doctest(text: str, include_ignored: bool = False) -> bool:
    """Whether `text` contains a runnable Rust doc-test fenced code block (``` or ~~~) in `///`, `//!`, `/** */`,
    `/*! */`, or literal `#[doc = "..."]` / `#![doc = "..."]` comments."""
    return any(_block_has_doctest(block, include_ignored=include_ignored) for block in _extract_doc_blocks(text))


def feature_gates(text: str, enabled: set) -> tuple:
    """What features gate a Rust test file's tests off when only `enabled` are on: (the feature of a file-level
    `#![cfg(feature = "X")]` if X is off, else None; the off features that gate some test; the number of tests no
    off feature gates). A test is gated by `#[cfg(feature = "X")]` among its attributes or by an enclosing
    `#[cfg(feature = "X")] mod m { ... }` (the block found by counting braces). Other cfg forms are not read."""
    gate = re.search(r'^\s*#!\[cfg\(feature\s*=\s*"([^"]+)"\)\]', text, re.M)
    if gate and gate.group(1) not in enabled:
        return gate.group(1), set(), 0
    lines = text.splitlines()
    module_gate = {}
    for m in re.finditer(r'^\s*#\[cfg\(feature\s*=\s*"([^"]+)"\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{', text, re.M):
        depth, k = 0, m.end() - 1
        while k < len(text):
            depth += {"{": 1, "}": -1}.get(text[k], 0)
            if depth == 0:
                break
            k += 1
        for n in range(text.count("\n", 0, m.start()), text.count("\n", 0, k) + 1):
            module_gate.setdefault(n, set()).add(m.group(1))
    gated, ungated = set(), 0
    for i, line in enumerate(lines):
        if not re.match(r"\s*#\[(?:[a-z_]+::)?test\b", line):
            continue
        off = module_gate.get(i, set()) - enabled
        attrs, j = [], i - 1
        while j >= 0 and re.match(r"\s*(#\[|///|//)", lines[j]):
            attrs.append(lines[j])
            j -= 1
        j = i + 1
        while j < len(lines) and re.match(r"\s*(#\[|///|//)", lines[j]):
            attrs.append(lines[j])
            j += 1
        off |= {f for a in attrs for f in re.findall(r'^\s*#\[cfg\(feature\s*=\s*"([^"]+)"\)\]', a)} - enabled
        if off:
            gated |= off
        else:
            ungated += 1
    return None, gated, ungated


def cargo_runs(cmd: Cmd, path: str, text: str | None = None) -> tuple[bool, str]:
    """Whether `cargo test` as traced runs the tests of a Rust file; (False, why) otherwise. `text`, if given, is
    read in place of the file's contents (the tests of this module use it)."""
    if cmd.guard:
        return False, cmd.guard
    argv = cmd.argv
    if os.path.basename(argv[0]) != "cargo":
        return False, "not cargo"
    args = argv[1:]
    if args and args[0].startswith("+"):
        args = args[1:]
    if not args or args[0] != "test":
        return False, "not cargo test"
    args = args[1:]
    if "--" in args:
        cut = args.index("--")
        args, harness = args[:cut], args[cut + 1 :]
    else:
        harness = []
    opts = {"package": [], "exclude": [], "test": [], "bin": [], "features": []}
    flags, positional = set(), []
    manifest_path = None
    takes_value = {"-p", "--package", "--exclude", "--test", "--bin", "--example", "--bench", "-F", "--features",
                   "--manifest-path", "-j", "--jobs", "--target", "--target-dir", "--profile", "--color",
                   "--config", "-Z", "--message-format"}
    i = 0
    while i < len(args):
        a = args[i]
        value = None
        if "=" in a and a.startswith("--") and a.split("=", 1)[0] in takes_value:
            a, value = a.split("=", 1)
        elif a.startswith("-p") and len(a) > 2 and not a.startswith("--"):
            a, value = "-p", a[2:]
        elif a in takes_value:
            value = args[i + 1] if i + 1 < len(args) else ""
            i += 1
        if a in ("-p", "--package"):
            opts["package"].append(value)
        elif a == "--exclude":
            opts["exclude"].append(value)
        elif a == "--test":
            opts["test"].append(value)
        elif a == "--bin":
            opts["bin"].append(value)
        elif a in ("-F", "--features"):
            opts["features"] += re.split(r"[ ,]+", value)
        elif a == "--manifest-path":
            manifest_path = value
        elif a.startswith("-"):
            flags.add(a)
        else:
            positional.append(a)
        i += 1
    if "--no-run" in flags:
        return False, "cargo test --no-run runs nothing"
    if cmd.cwd is None:
        return False, "the directory it runs in is not known"
    pkg_dir, pkg = _package_of(path)
    if pkg_dir is None:
        return False, "no Cargo package contains it"
    name = pkg["package"]["name"]
    start = (ROOT / cmd.cwd)
    if manifest_path:
        start = (start / manifest_path).parent
    ws_dir, ws, members, default_members = _workspace_of(start.resolve())
    here_manifest = start / "Cargo.toml"
    here = _toml(here_manifest) if here_manifest.is_file() else None
    if here is None:
        # cargo looks upward for the manifest
        d = start
        while d != ROOT.parent and not (d / "Cargo.toml").is_file():
            d = d.parent
        here = _toml(d / "Cargo.toml") if (d / "Cargo.toml").is_file() else None
        start = d
    if here is None:
        return False, "no Cargo.toml where it runs"
    pkg_dir = pkg_dir.resolve()
    if opts["package"]:
        if name not in opts["package"]:
            return False, f"-p selects {', '.join(opts['package'])}, not {name}"
        if pkg_dir not in members and pkg_dir != start.resolve():
            return False, f"{name} is not in the workspace it runs in"
    elif "--workspace" in flags or "--all" in flags:
        if pkg_dir not in members:
            return False, f"{name} is not a member of the workspace at {os.path.relpath(ws_dir or start, ROOT)}"
        if name in opts["exclude"]:
            return False, f"--exclude {name}"
    elif "package" in here:
        if pkg_dir != start.resolve():
            return False, f"it runs the package at {os.path.relpath(start, ROOT)}, not {name}"
    else:
        selected = default_members if default_members else members
        if pkg_dir not in selected:
            return False, f"{name} is not among the workspace's default members"
    # the target the file belongs to
    rel_in_pkg = (ROOT / path).resolve().relative_to(pkg_dir).as_posix()
    tests_decl = pkg.get("test", [])
    target, required = None, []
    for t in tests_decl:
        tpath = pathlib.PurePosixPath(t["path"]).as_posix() if t.get("path") else None
        if tpath == rel_in_pkg or (tpath is None and rel_in_pkg in (f"tests/{t.get('name')}.rs", f"tests/{t.get('name')}/main.rs")):
            target, required = ("test", t.get("name")), t.get("required-features", [])
            break
    if target is None:
        m = re.fullmatch(r"tests/([^/]+)\.rs|tests/([^/]+)/main\.rs", rel_in_pkg)
        if m:
            if pkg["package"].get("autotests") is False:
                return False, "autotests = false and no [[test]] names it"
            target = ("test", m.group(1) or m.group(2))
    if target is None:
        for b in pkg.get("bin", []):
            bname = b.get("name")
            bpath = pathlib.PurePosixPath(b["path"]).as_posix() if b.get("path") else None
            if (
                (bpath is not None and bpath == rel_in_pkg)
                or (bpath is None and rel_in_pkg in (f"src/bin/{bname}.rs", f"src/bin/{bname}/main.rs"))
                or (bpath is None and bname == name and rel_in_pkg == "src/main.rs")
            ):
                target, required = ("bin", bname), b.get("required-features", [])
                break
    if target is None and pkg["package"].get("autobins") is not False:
        if rel_in_pkg == "src/main.rs":
            target = ("bin", name)
        else:
            mb = re.fullmatch(r"src/bin/([^/]+)\.rs|src/bin/([^/]+)/.*", rel_in_pkg)
            if mb:
                target = ("bin", mb.group(1) or mb.group(2))
    if target is None:
        lib_decl = pkg.get("lib") or {}
        lib_path = pathlib.PurePosixPath(lib_decl.get("path") or "src/lib.rs").as_posix()
        if (pkg_dir / lib_path).is_file() and (
            rel_in_pkg == lib_path
            or (rel_in_pkg.startswith("src/") and rel_in_pkg != "src/main.rs" and not rel_in_pkg.startswith("src/bin/"))
        ):
            target = ("lib", None)
        elif rel_in_pkg.startswith("src/") and (pkg_dir / "src" / "main.rs").is_file():
            target = ("bin", name)
        else:
            return False, "not a test target of its package"
    selectors = {"--lib", "--bins", "--tests", "--all-targets", "--examples", "--benches", "--doc"} & flags
    if "--doc" in flags and (opts["test"] or opts["bin"] or (selectors - {"--doc"})):
        return False, "cargo test --doc cannot be combined with other target selectors"
    kind, tname = target
    if opts["test"] or opts["bin"] or selectors:
        runs_unit = (
            (kind == "test" and (tname in opts["test"] or "--tests" in flags or "--all-targets" in flags))
            or (kind == "lib" and ("--lib" in flags or "--tests" in flags or "--all-targets" in flags))
            or (kind == "bin" and (tname in opts["bin"] or "--bins" in flags or "--tests" in flags or "--all-targets" in flags))
        )
        runs_doc = (kind == "lib" and "--doc" in flags)
        if not runs_unit and not runs_doc:
            return False, f"its target ({kind} {tname or ''}) is not among those selected"
    else:
        runs_unit = True
        runs_doc = (kind == "lib")
    # features
    if required:
        defaults = set(pkg.get("features", {}).get("default", [])) if "--no-default-features" not in flags else set()
        enabled = set(opts["features"]) | defaults
        if "--all-features" not in flags and not set(required) <= enabled:
            return False, f"needs features {required}"
    if text is None:
        text = (ROOT / path).read_text(encoding="utf-8", errors="replace")
    # filters and harness options
    filters = [p for p in positional]
    harness_flags = set(a for a in harness if a.startswith("-"))
    exact = "--exact" in harness_flags
    include_ignored = bool({"--ignored", "--include-ignored"} & harness_flags)
    hpos, skips, skip_next = [], [], None
    for a in harness:
        if skip_next:
            if skip_next == "--skip":
                skips.append(a)
            skip_next = None
            continue
        if a.startswith("--skip="):
            skips.append(a.split("=", 1)[1])
            continue
        if a in ("--test-threads", "--skip", "--format", "--logfile", "--color", "-Z"):
            skip_next = a
            continue
        if not a.startswith("-"):
            hpos.append(a)
    filters += hpos
    file_has_unit = bool(re.search(r"#\[(?:[a-z_]+::)?test\b", text) or "proptest!" in text)
    file_has_doc = runs_doc and has_doctest(text, include_ignored=include_ignored)

    def _doc_ok() -> tuple[bool, str]:
        if "--all-features" not in flags:
            enabled = set(opts["features"]) | (set(pkg.get("features", {}).get("default", []))
                                               if "--no-default-features" not in flags else set())
            file_gate, _gated, _ungated = feature_gates(text, enabled)
            if file_gate:
                return False, f"the whole file is compiled only with feature {file_gate!r}, which the command does not enable"
        doc_targets = [rel_in_pkg] + re.findall(r"\b(?:fn|struct|enum|trait|mod)\s+([A-Za-z0-9_]+)", text)
        for f in filters:
            if not any((f == p if exact else f in p) for p in doc_targets):
                return False, f"the name filter {f!r} matches none of its doc-tests"
        for k in skips:
            if any((k == p if exact else k in p) for p in doc_targets):
                return False, f"--skip {k!r} skips doc-tests of this file"
        missing = [g for g in sorted(rust_env_gates(text)) if not cmd.env.get(g)]
        if missing:
            return False, f"it skips itself or fails unless {', '.join(missing)} is set, and the command does not set it"
        return True, ""

    if runs_doc and not runs_unit:
        if not file_has_doc:
            return False, "cargo test --doc runs doc-tests only, and the file has none"
        return _doc_ok()
    if not file_has_unit:
        if file_has_doc:
            return _doc_ok()
        return False, "the file has no #[test]"
    # Tests compiled out by a feature the command does not enable (feature_gates).
    if "--all-features" not in flags:
        enabled = set(opts["features"]) | (set(pkg.get("features", {}).get("default", []))
                                           if "--no-default-features" not in flags else set())
        file_gate, gated, ungated = feature_gates(text, enabled)
        if file_gate:
            return False, f"the whole file is compiled only with feature {file_gate!r}, which the command does not enable"
        if gated and not ungated and "proptest!" not in text:
            return False, f"every test in it is compiled only with feature(s) {sorted(gated)}, which the command does not enable"
    # A name filter matches a test's full path: for a file under src/, the module path the file gives (src/relayer.rs:
    # `relayer::...`), then the file's inner modules, then the function; for an integration test file, the inner
    # modules and the function. With `--exact`, a filter or `--skip` must equal the full path rather than be a substring.
    fns = re.findall(r"\bfn\s+([A-Za-z0-9_]+)", text)
    mods = re.findall(r"\bmod\s+([A-Za-z0-9_]+)\s*\{", text)
    module = ""
    if target[0] in ("lib", "bin"):
        parts = pathlib.PurePosixPath(rel_in_pkg).with_suffix("").parts[1:]  # without src/
        parts = [q for q in parts if q != "mod"]
        if parts and parts[0] == "bin":
            parts = parts[2:]  # src/bin/NAME.rs is the root of its own crate
        if parts in (["lib"], ["main"]):
            parts = []
        module = "::".join(parts)
    paths = [f"{module}::{fn}" if module else fn for fn in fns]
    paths += [f"{module}::{m}::{fn}" if module else f"{m}::{fn}" for m in mods for fn in fns]
    for f in filters:
        if not any((f == p if exact else f in p) for p in paths):
            mode = "exact " if exact else ""
            return False, f"the {mode}name filter {f!r} matches none of its tests' paths"
    # `--skip P` leaves out every test whose path matches P: a file any of whose tests it could skip does not count.
    for k in skips:
        if any((k == p if exact else k in p) for p in paths):
            return False, f"--skip {k!r} skips tests of this file"
    n_tests = len(re.findall(r"#\[(?:[a-z_]+::)?test\b", text))
    if n_tests and n_tests <= text.count("#[ignore") and not include_ignored:
        return False, "every test in it is #[ignore]d"
    missing = [g for g in sorted(rust_env_gates(text)) if not cmd.env.get(g)]
    if missing:
        return False, f"it skips itself or fails unless {', '.join(missing)} is set, and the command does not set it"
    return True, ""


def _foundry_root(start: pathlib.Path):
    d = start
    while True:
        if (d / "foundry.toml").is_file():
            return d
        if d == ROOT or d == d.parent:
            return None
        d = d.parent


def _solidity_index(test_dir: pathlib.Path) -> dict:
    """Every contract under a Foundry test directory: name -> (abstract, bases, test functions, file)."""
    index = {}
    for sol in sorted(test_dir.rglob("*.sol")):
        text = re.sub(r"//[^\n]*|/\*.*?\*/", "", sol.read_text(encoding="utf-8", errors="replace"), flags=re.S)
        heads = list(re.finditer(r"^\s*(abstract\s+)?(contract|library|interface)\s+([A-Za-z0-9_]+)([^{]*)\{", text, flags=re.M))
        for k, m in enumerate(heads):
            if m.group(2) != "contract":
                continue
            body = text[m.end(): heads[k + 1].start() if k + 1 < len(heads) else len(text)]
            bases = []
            inherit = re.match(r"\s*is\s+(.*)", m.group(4), flags=re.S)
            if inherit:
                depth, cur = 0, ""
                for c in inherit.group(1):
                    if c == "(":
                        depth += 1
                    elif c == ")":
                        depth -= 1
                    elif c == "," and depth == 0:
                        bases.append(cur.strip())
                        cur = ""
                        continue
                    if depth == 0 and c != ")":
                        cur += c
                if cur.strip():
                    bases.append(cur.strip())
                bases = [re.match(r"[A-Za-z0-9_]+", b_).group(0) for b_ in bases if re.match(r"[A-Za-z0-9_]+", b_)]
            fns = re.findall(r"function\s+((?:test|invariant)[A-Za-z0-9_]*)\s*\(", body)
            index[m.group(3)] = (bool(m.group(1)), bases, fns, sol.resolve())
    return index


def _test_functions(index: dict, name: str, seen: set) -> list:
    if name in seen or name not in index:
        return []
    seen.add(name)
    _abstract, bases, fns, _file = index[name]
    out = list(fns)
    for base in bases:
        out += _test_functions(index, base, seen)
    return out


def forge_runs(cmd: Cmd, path: str) -> tuple[bool, str]:
    """Whether `forge test` as traced runs the tests of a Solidity test file; (False, why) otherwise."""
    if cmd.guard:
        return False, cmd.guard
    argv = cmd.argv
    if os.path.basename(argv[0]) != "forge" or argv[1:2] != ["test"]:
        return False, "not forge test"
    if cmd.cwd is None:
        return False, "the directory it runs in is not known"
    args = argv[2:]
    opts = {}
    alias = {"--mp": "--match-path", "--nmp": "--no-match-path", "--mc": "--match-contract",
             "--nmc": "--no-match-contract", "--mt": "--match-test", "--nmt": "--no-match-test"}
    i = 0
    root = ROOT / cmd.cwd
    while i < len(args):
        a = args[i]
        value = None
        if "=" in a and a.startswith("--"):
            a, value = a.split("=", 1)
        a = alias.get(a, a)
        if a in ("--match-path", "--no-match-path", "--match-contract", "--no-match-contract", "--match-test",
                 "--no-match-test", "--root"):
            if value is None:
                value = args[i + 1] if i + 1 < len(args) else ""
                i += 1
            if a == "--root":
                root = root / value
            else:
                opts[a] = value
        i += 1
    root = _foundry_root(root.resolve())
    if root is None:
        return False, "no foundry.toml where it runs"
    config = _toml(root / "foundry.toml").get("profile", {}).get("default", {})
    test_dir = (root / config.get("test", "test")).resolve()
    file = (ROOT / path).resolve()
    if test_dir not in file.parents:
        return False, f"not under the project's test directory {os.path.relpath(test_dir, ROOT)}"
    rel = file.relative_to(root).as_posix()
    if "--match-path" in opts and not (fnmatch.fnmatch(rel, opts["--match-path"]) or fnmatch.fnmatch(str(file), opts["--match-path"])):
        return False, f"--match-path {opts['--match-path']} leaves it out"
    if "--no-match-path" in opts and (fnmatch.fnmatch(rel, opts["--no-match-path"]) or fnmatch.fnmatch(str(file), opts["--no-match-path"])):
        return False, f"--no-match-path {opts['--no-match-path']} leaves it out"
    index = _solidity_index(test_dir)
    runs = 0
    for name, (abstract, _bases, _fns, where) in index.items():
        if where != file or abstract:
            continue
        if "--match-contract" in opts and not re.search(opts["--match-contract"], name):
            continue
        if "--no-match-contract" in opts and re.search(opts["--no-match-contract"], name):
            continue
        for fn in _test_functions(index, name, set()):
            if "--match-test" in opts and not re.search(opts["--match-test"], fn):
                continue
            if "--no-match-test" in opts and re.search(opts["--no-match-test"], fn):
                continue
            runs += 1
    if runs == 0:
        return False, "no test function of it is selected (contracts filtered, abstract, or no tests)"
    return True, ""


# ── Self-check against yq ──────────────────────────────────────────────────────────────────────────


def _projection(doc):
    """What the checks read from a workflow: triggers, and per job its defaults, env and steps' run/dir/env."""
    on = doc.get("on", doc.get(True))
    if isinstance(on, str):
        on = {on: None}
    elif isinstance(on, list):
        on = {k: None for k in on}
    out = {"on": sorted((on or {}).keys()), "jobs": {}}
    for name, spec in (doc.get("jobs") or {}).items():
        spec = spec or {}
        out["jobs"][name] = {
            "uses": spec.get("uses"),
            "defaults": json.dumps(spec.get("defaults"), sort_keys=True),
            "env": json.dumps({k: str(v) for k, v in (spec.get("env") or {}).items()}, sort_keys=True),
            "steps": [
                {k: (str(s.get(k)) if s.get(k) is not None else None) for k in ("run", "working-directory", "uses", "shell", "name")}
                | {"env": json.dumps({k: str(v) for k, v in (s.get("env") or {}).items()}, sort_keys=True)}
                for s in (spec.get("steps") or [])
            ],
        }
    return out


def compare_yq() -> int:
    files = sorted(WORKFLOWS.glob("*.yml")) + sorted((ROOT / "modules" / "catalog" / ".github" / "workflows").glob("*.yml"))
    bad = 0
    for path in files:
        mine = _projection(load_yaml(path.read_text(encoding="utf-8")) or {})
        got = subprocess.run(["yq", "-o=json", ".", str(path)], capture_output=True, text=True, check=True).stdout
        theirs = _projection(json.loads(got) or {})
        if mine != theirs:
            bad += 1
            print(f"DIFFERS: {path.relative_to(ROOT)}")
            for job in sorted(set(mine["jobs"]) | set(theirs["jobs"])):
                if mine["jobs"].get(job) != theirs["jobs"].get(job):
                    print(f"  job {job}:\n    mine   {mine['jobs'].get(job)}\n    yq     {theirs['jobs'].get(job)}")
            if mine["on"] != theirs["on"]:
                print(f"  on: mine {mine['on']} yq {theirs['on']}")
        else:
            print(f"same: {path.relative_to(ROOT)} ({len(mine['jobs'])} jobs)")
    print(f"ci_model.py --compare-yq: {len(files)} workflows, {bad} differ")
    return 1 if bad else 0


def print_steps(names: list[str]) -> int:
    """Print each job's `run:` steps as a terminal can run them (what the justfile's job recipes run)."""
    all_jobs = jobs()
    for name in names or sorted(all_jobs):
        for entry in run_steps(all_jobs[name]):
            where = f" [cwd={entry['cwd']}]" if entry["cwd"] != "." else ""
            combo = f" [{entry['combo']}]" if entry["combo"] else ""
            print(f"{name}: step {entry['step']}{combo}{where}:")
            for line in entry["text"].splitlines():
                if line.strip():
                    print(f"    {line.rstrip()}")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--compare-yq"]:
        sys.exit(compare_yq())
    if sys.argv[1:2] == ["--steps"]:
        sys.exit(print_steps(sys.argv[2:]))
    if sys.argv[1:2] == ["--trace"]:
        all_jobs = jobs()
        tracked = tracked_paths()
        for name in sys.argv[2:] or sorted(all_jobs):
            for cmd in trace(all_jobs[name], tracked):
                print(f"{name}: {cmd}")
        sys.exit(0)
    print(__doc__)
    sys.exit(2)
