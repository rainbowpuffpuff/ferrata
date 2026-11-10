#!/usr/bin/env python3
"""Compare the result tables of two harness runs, table by table and cell by cell.

    python3 -I lez/scripts/compare-tables.py OLD_DIR NEW_DIR [OLD_LABEL NEW_LABEL] > compare.md

For every `*.md` of NEW_DIR that OLD_DIR also has: its table rows are paired in order (the
harness writes them in the same order on every run), and every cell that differs is listed with
both values and, where both are one integer, the difference. Text outside tables (titles, notes)
is compared line by line. Standard library only.
"""
import os
import re
import sys

NUMBER = re.compile(r"^-?[0-9][0-9,]*$")


def tables(path):
    """(header cells, [row cells]) of every table, and the text lines outside tables."""
    found, text, header, rows = [], [], None, []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.rstrip("\n")
            if line.startswith("|"):
                cells = [c.strip() for c in line.strip().strip("|").split("|")]
                if all(re.fullmatch(r":?-+:?", c) for c in cells if c):
                    continue
                if header is None:
                    header = cells
                else:
                    rows.append(cells)
            else:
                if header is not None:
                    found.append((header, rows))
                    header, rows = None, []
                text.append(line)
    if header is not None:
        found.append((header, rows))
    return found, text


def number(cell):
    return int(cell.replace(",", "")) if NUMBER.match(cell) else None


def main():
    old_dir, new_dir = sys.argv[1], sys.argv[2]
    old_label, new_label = (sys.argv[3], sys.argv[4]) if len(sys.argv) > 4 else ("old", "new")
    names = sorted(n for n in os.listdir(new_dir) if n.endswith(".md"))
    print(f"# {new_label} against {old_label}, table by table\n")
    print(f"Written by `lez/scripts/compare-tables.py`: every cell of {new_label}'s tables that differs from")
    print(f"{old_label}'s, rows paired in order.\n")
    print(f"| file | tables | rows | rows equal | rows that differ | text lines that differ |")
    print(f"|---|---:|---:|---:|---:|---:|")
    details = []
    for name in names:
        old_path = os.path.join(old_dir, name)
        if not os.path.exists(old_path):
            print(f"| `{name}` | only in {new_label} | | | | |")
            continue
        old_tables, old_text = tables(old_path)
        new_tables, new_text = tables(os.path.join(new_dir, name))
        text_diff = sum(1 for a, b in zip(old_text, new_text) if a != b) + abs(len(old_text) - len(new_text))
        rows = equal = 0
        lines = []
        if len(old_tables) != len(new_tables):
            lines.append(f"- {len(old_tables)} tables in {old_label}, {len(new_tables)} in {new_label}")
        for (old_header, old_rows), (new_header, new_rows) in zip(old_tables, new_tables):
            if old_header != new_header:
                lines.append(f"- header differs: `{' | '.join(old_header)}` / `{' | '.join(new_header)}`")
            if len(old_rows) != len(new_rows):
                lines.append(f"- {len(old_rows)} rows in {old_label}, {len(new_rows)} in {new_label}")
            for old_row, new_row in zip(old_rows, new_rows):
                rows += 1
                if old_row == new_row:
                    equal += 1
                    continue
                label = " / ".join(new_row[:2])
                cells = []
                for i, (a, b) in enumerate(zip(old_row, new_row)):
                    if a == b:
                        continue
                    column = new_header[i] if i < len(new_header) else f"column {i + 1}"
                    na, nb = number(a), number(b)
                    delta = f" ({nb - na:+,})" if na is not None and nb is not None else ""
                    cells.append(f"{column}: `{a}` → `{b}`{delta}")
                lines.append(f"- {label}: " + "; ".join(cells))
        print(f"| `{name}` | {len(new_tables)} | {rows} | {equal} | {rows - equal} | {text_diff} |")
        if lines:
            details.append((name, lines))
    for name in sorted(n for n in os.listdir(old_dir) if n.endswith(".md")):
        if not os.path.exists(os.path.join(new_dir, name)):
            print(f"| `{name}` | only in {old_label} | | | | |")
    for name, lines in details:
        print(f"\n## `{name}`\n")
        print("\n".join(lines))


if __name__ == "__main__":
    main()
