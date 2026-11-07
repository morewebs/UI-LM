"""Measures the third table signal the vocabulary does not have: a grid of words with no underline and no numbers.

`ls -C` draws exactly that - two or more columns of file names, aligned by padding, with no dash row and no digit to
make the values rule fire. The reader calls the whole screen one `Block`. Before that becomes a defect, the question is
whether a rule can see the grid **without** seeing vim's help rows, which ADR 0007 rejected the column-alignment rule
for. This file is the measurement, not the decision.

The candidate rule: three or more consecutive non-blank rows, each split into two or more cells by two or more spaces,
where every cell is a single word (`[A-Za-z0-9._+-]+`), no row holds a dash underline, and every row starts its cells on
the grid's columns. A row may hold fewer cells than the grid is wide, down to one - `ls -C` fills columns top-down, so
its last row stops where the shortest column does - but it may not skip a column, and the grid needs two columns. It is narrower than ADR 0007's rule in one way
only: it refuses prose as a cell.

Run from the repository root:

    python scripts/measure-word-grids.py

It prints every recording with a block that satisfies the candidate, the longest run, and the block's first rows, so a
false positive can be looked at rather than argued about.
"""

import json
import os
import re
import subprocess
import sys

CAPTURE = os.path.join("target", "debug", "ui-lm-capture.exe")
WORD = re.compile(r"^[A-Za-z0-9._+\[\]-]+$")
UNDERLINE = re.compile(r"-{3,}")


def sessions(roots=("eval", "cases")):
    found = []
    for root in roots:
        for dirpath, _, filenames in os.walk(root):
            if "session.json" in filenames:
                found.append(dirpath.replace(os.sep, "/"))
    return sorted(found)


def cells(text):
    return [cell for cell in re.split(r"\s{2,}", text.strip()) if cell]


def offsets(text):
    found = []
    index = 0
    while index < len(text):
        if text[index] == " ":
            index += 1
            continue
        found.append(index)
        while index < len(text) and text[index:index + 2] != "  ":
            index += 1
    return tuple(found)


def grid_run(lines):
    """The longest run of rows that satisfies the candidate, and the rows themselves.

    A row may hold fewer cells than the grid is wide, down to one: `ls -C` fills columns top-down, so its last row
    stops where the shortest column does. What a row may not do is skip a column - its cells must start on the
    grid's columns - and the grid itself must still have two, which is the same rule the reader applies
    (`as_word_grid`).
    """
    best = 0
    best_rows = []
    run = []
    run_rows = []

    def valid(shapes):
        longest = max(shapes, key=len)
        return len(longest) >= 2 and all(longest[:len(shape)] == shape for shape in shapes)

    def close():
        nonlocal best, best_rows
        if len(run) >= 3 and len(run) > best and valid(run):
            best = len(run)
            best_rows = list(run_rows)

    for text in lines:
        if not text.strip() or UNDERLINE.search(text):
            close()
            run, run_rows = [], []
            continue
        row = cells(text)
        shape = offsets(text)
        if not row or not all(WORD.match(cell) for cell in row):
            close()
            run, run_rows = [], []
            continue
        run.append(shape)
        run_rows.append(text)
        while run and not valid(run):
            run.pop(0)
            run_rows.pop(0)
    close()
    return best, best_rows


def main():
    if not os.path.exists(CAPTURE):
        print("build first: cargo build --workspace")
        return 1
    hits = 0
    checked = 0
    for session in sessions():
        checked += 1
        out = subprocess.run(
            [CAPTURE, "page", "--session", session, "--all"],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
        ).stdout
        for raw in (line for line in out.splitlines() if line.startswith("{")):
            page = json.loads(raw)
            for component in page["createSurface"]["components"]:
                kind = component.get("component")
                if kind not in ("Block", "Table"):
                    continue
                text = component.get("lines")
                if not isinstance(text, list):
                    continue
                lines = [line.get("text", "") if isinstance(line, dict) else str(line) for line in text]
                run, rows = grid_run(lines)
                if run >= 3:
                    hits += 1
                    print(f"{session}  {kind}  run={run}  offset={offsets(rows[0]) if rows else ()}")
                    for row in rows[:3]:
                        print(f"    {row!r}")
    print()
    print(f"{checked} recording(s), {hits} block(s) satisfy the word-grid candidate")
    return 0


if __name__ == "__main__":
    sys.exit(main())
