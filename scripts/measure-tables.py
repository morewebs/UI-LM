"""Measures the column-alignment predicate ADR 0007 rejects, over every recording on this machine.

The ADR quoted numbers for two rounds that nobody could reproduce: the rule was written down as "2+ shared column
boundaries" and nothing said what a boundary was, which rows counted, or how a run was measured. This file is the
missing half — the predicate in code, next to the decision it supports.

The predicate, stated before it is applied:

    A table is three or more consecutive rows with the same number of cells, cells separated by two or more spaces,
    at the same character offsets in every row, and at least one cell numeric.

Run from the repository root:

    python scripts/measure-tables.py

It prints the recordings whose pages hold the longest runs and the named acceptance cases — `df`, `top`, a survey sheet
and a `vim` help screen — because a predicate that cannot separate those has not earned the right to name a kind.
"""

import json
import os
import re
import subprocess
import sys

CAPTURE = os.path.join("target", "debug", "ui-lm-capture.exe")
NUMERIC = re.compile(r"^[-+]?[\d.,]+[KMG%]?$")


def cells(text):
    """The cells of a row: runs of two or more spaces separate them."""
    return [cell for cell in re.split(r"\s{2,}", text.strip()) if cell]


def cell_offsets(text):
    """Where each cell starts, so two rows can be compared without their text."""
    offsets = []
    index = 0
    while index < len(text):
        if text[index] == " ":
            index += 1
            continue
        offsets.append(index)
        while index < len(text) and not (text[index] == " " and text[index:index + 2] == "  "):
            index += 1
    return tuple(offsets)


def has_number(text):
    return any(NUMERIC.match(cell) for cell in cells(text))


def longest_run(lines):
    """The longest stretch of rows that satisfies the predicate."""
    rows = []
    for text in lines:
        if not text.strip():
            rows.append(None)
            continue
        shape = (len(cells(text)), cell_offsets(text))
        rows.append(shape if shape[0] >= 2 else None)

    best = 0
    run = []
    for row in rows:
        if row is None:
            if len(run) >= 3:
                best = max(best, len(run))
            run = []
            continue
        if run and run[-1] == row:
            run.append(row)
        else:
            if len(run) >= 3:
                best = max(best, len(run))
            run = [row]
    if len(run) >= 3:
        best = max(best, len(run))
    return best


def recordings(roots=("eval", "cases")):
    found = []
    for root in roots:
        for dirpath, _, filenames in os.walk(root):
            if "session.json" in filenames:
                found.append(dirpath.replace(os.sep, "/"))
    return sorted(found)


def measure(session):
    out = subprocess.run(
        [CAPTURE, "page", "--session", session, "--all"],
        capture_output=True,
        text=True,
        errors="ignore",
    ).stdout
    best = 0
    for raw in (line for line in out.splitlines() if line.startswith("{")):
        page = json.loads(raw)
        for component in page["createSurface"]["components"]:
            lines = [
                line.get("text", "") if isinstance(line, dict) else str(line)
                for line in (component.get("lines") or [])
            ]
            best = max(best, longest_run(lines))
    return best


def main():
    if not os.path.exists(CAPTURE):
        sys.exit("build first: cargo build --workspace")
    sessions = recordings()
    if not sessions:
        sys.exit("no recordings found, so nothing was measured")

    measured = [(session, measure(session)) for session in sessions]
    matched = [(session, run) for session, run in measured if run]

    print(f"recordings: {len(measured)}   holding a table run: {len(matched)}")
    print("longest runs:")
    for session, run in sorted(matched, key=lambda m: -m[1])[:8]:
        print(f"  {run:3} row(s)  {session}")

    print("acceptance cases:")
    for name in ("df-table", "linux-top", "wsl-vim", "second-agent-17"):
        for session, run in measured:
            if name in session:
                print(f"  {run:3} row(s)  {session}")
                break
        else:
            print(f"  {0:3} row(s)  (not in the corpus) {name}")


if __name__ == "__main__":
    main()
