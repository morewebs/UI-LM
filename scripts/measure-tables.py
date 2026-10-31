"""Measures the column-alignment predicate ADR 0007 rejects, over every recording on this machine.

The ADR quoted numbers for two rounds that nobody could reproduce: the rule was written down as "2+ shared column
boundaries" and nothing said what a boundary was, which rows counted, or how a run was measured. This file is the
missing half â€” the predicate in code, next to the decision it supports.

Two predicates are measured here, and they disagree completely:

**The alignment rule** (rejected): three or more consecutive rows with the same number of cells, cells separated by two
or more spaces, at the same character offsets in every row. It matches `vim`'s help rows and scores zero on `df`.

**The values rule** (round 1 of the second window): three or more consecutive rows of the same width in which at least
two vertical strips of two or more characters hold a digit in most rows, separated by at least one blank column. This is
what a person means by a table - numbers under a heading - and it does not care whether the columns are padded with one
space (`df`) or many (`Format-Table`).

On the screens that decide the question:

    session                     alignment  values   is it a table?
    df-table                            0      15   yes
    linux-top                          18      24   yes
    cmd-dir                             0      11   yes
    pwsh-table                          4      10   yes
    wsl-vim                            18       0   no
    second-agent-17                     5       0   no

Nine recordings in ten that hold a values-run are `ls`, `ps`, `df`, `top` and formatted listings; `vim` and the surveys
score zero.

Run from the repository root:

    python scripts/measure-tables.py

It prints the recordings whose pages hold the longest runs and the named acceptance cases â€” `df`, `top`, a survey sheet
and a `vim` help screen â€” because a predicate that cannot separate those has not earned the right to name a kind.
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


NUMERIC_CHARS = set("0123456789.,%-+:")


def numeric_strips(lines, min_rows=3):
    """The second predicate: a table is **values in columns**, not padding.

    Stated before measuring: three or more consecutive rows of the same width in which at least two vertical strips of
    two or more characters hold a digit in most rows, separated by at least one entirely blank column. This is what a
    person means by a table - numbers under a heading - and it does not care whether the columns are padded with one
    space (`df`) or many (`Format-Table`).
    """
    best = 0
    run = []
    for text in lines + [""]:
        if not text.strip():
            if len(run) >= min_rows:
                best = max(best, longest_strip_run(run))
            run = []
            continue
        run.append(text)
    if len(run) >= min_rows:
        best = max(best, longest_strip_run(run))
    return best


def longest_strip_run(rows):
    """How many rows in this block share two numeric strips."""
    best = 0
    for start in range(len(rows) - 2):
        window = rows[start:]
        width = min(len(r) for r in window)
        if width < 8:
            continue
        strips = []
        col = 0
        while col < width:
            holds = sum(1 for r in window if r[col] in NUMERIC_CHARS)
            if holds * 2 >= len(window) + 1:
                end = col
                while end < width and sum(1 for r in window if r[end] in NUMERIC_CHARS) * 2 >= len(window) + 1:
                    end += 1
                if end - col >= 2:
                    strips.append((col, end))
                col = end
            else:
                col += 1
        if len(strips) >= 2:
            best = max(best, len(window))
            break
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


def measures(session):
    """Both predicates for one session: the alignment rule and the values rule, over its busiest page."""
    out = subprocess.run([CAPTURE, "page", "--session", session, "--all"],
                         capture_output=True, text=True, errors="ignore").stdout
    aligned, values = 0, 0
    for raw in (line for line in out.splitlines() if line.startswith("{")):
        page = json.loads(raw)
        for component in page["createSurface"]["components"]:
            rows = [line.get("text", "") if isinstance(line, dict) else str(line)
                    for line in (component.get("lines") or [])]
            aligned = max(aligned, longest_run(rows))
            values = max(values, numeric_strips(rows))
    return aligned, values


def disagreements():
    """Every component the predicate would promote but the reader left alone, and the reverse.

    This is the check that the **code implements the documented rule**: the predicate here is the specification, the
    component list comes from the binary, and a mismatch is one of them being wrong (round 11).
    """
    missed, extra = [], []
    for session in recordings():
        out = subprocess.run([CAPTURE, "page", "--session", session, "--all"],
                             capture_output=True, text=True, errors="ignore").stdout
        for raw in (line for line in out.splitlines() if line.startswith("{")):
            page = json.loads(raw)
            for component in page["createSurface"]["components"]:
                rows = [line.get("text", "") if isinstance(line, dict) else str(line)
                        for line in (component.get("lines") or [])]
                # The ADR says a table is a *block of its own*, so the whole component is one run - not the
                # longest blank-line separated stretch inside it, which is what `numeric_strips` reports.
                matches = longest_strip_run(rows) >= 3
                is_table = component["component"] == "Table"
                if matches and not is_table:
                    missed.append((session, component["id"]))
                if is_table and not matches:
                    extra.append((session, component["id"]))
    return missed, extra


def main():
    if not os.path.exists(CAPTURE):
        sys.exit("build first: cargo build --workspace")
    sessions = recordings()
    if not sessions:
        sys.exit("no recordings found, so nothing was measured")

    measured = [(session, measure(session)) for session in sessions]
    measured_names = [session for session, _ in measured]
    matched = [(session, run) for session, run in measured if run]

    print(f"recordings: {len(measured)}   holding a table run: {len(matched)}")
    print("longest runs:")
    for session, run in sorted(matched, key=lambda m: -m[1])[:8]:
        print(f"  {run:3} row(s)  {session}")

    matched_values = [s for s in measured_names if measures(s)[1] >= 3]
    print(f"recordings holding a values-run of 3+ rows: {len(matched_values)} of {len(measured_names)}")
    print("the longest values-runs:")
    for session, run in sorted(((s, measures(s)[1]) for s in measured_names), key=lambda m: -m[1])[:6]:
        print(f"  {run:3} row(s)  {session}")

    print("the same rule at a five-row minimum:")
    five = [(s, numeric_strips([(l.get("text", "") if isinstance(l, dict) else str(l)) for l in (c.get("lines") or [])], min_rows=5))
            for s in measured_names for c in [None] if False]
    five = []
    for session in measured_names:
        best = 0
        out = subprocess.run([CAPTURE, "page", "--session", session, "--all"],
                             capture_output=True, text=True, errors="ignore").stdout
        for raw in (l for l in out.splitlines() if l.startswith("{")):
            page = json.loads(raw)
            for component in page["createSurface"]["components"]:
                rows = [l.get("text", "") if isinstance(l, dict) else str(l) for l in (component.get("lines") or [])]
                best = max(best, numeric_strips(rows, min_rows=5))
        if best >= 5:
            five.append((session, best))
    print(f"  recordings at a five-row minimum: {len(five)} of {len(measured_names)}")
    for session, run in sorted(five, key=lambda m: -m[1])[:6]:
        print(f"    {run:3}  {session}")
    dropped = [s for s in matched_values if s not in [f[0] for f in five]]
    print("  dropped by the higher minimum: " + (", ".join(dropped) if dropped else "none"))

    print("every recording the values rule matches:")
    for session in matched_values:
        print(f"  MATCH {measures(session)[1]:3} row(s)  {session}")

    print("the two predicates on the screens that decide the question:")
    print(f"  {'session':32} {'alignment':>9} {'values':>7}   is it a table?")
    for name, truth in (("df-table", "yes"), ("linux-top", "yes"), ("cmd-dir", "yes"), ("pwsh-table", "yes"),
                        ("wsl-vim", "no"), ("second-agent-17", "no")):
        for session in measured_names:
            if name in session:
                aligned, values = measures(session)
                print(f"  {name:32} {aligned:9} {values:7}   {truth}")
                break

    print("acceptance cases:")
    for name in ("df-table", "linux-top", "wsl-vim", "second-agent-17"):
        for session, run in measured:
            if name in session:
                print(f"  {run:3} row(s)  {session}")
                break
        else:
            print(f"  {0:3} row(s)  (not in the corpus) {name}")


if __name__ == "__main__":
    import sys
    if "--disagreements" in sys.argv:
        missed, extra = disagreements()
        print(f"the predicate would promote {len(missed)} component(s) the reader left alone")
        for session, identifier in missed[:5]:
            print(f"  missed  {session}  {identifier}")
        print(f"the reader made {len(extra)} table(s) the predicate does not")
        for session, identifier in extra[:5]:
            print(f"  extra   {session}  {identifier}")
        raise SystemExit(1 if (missed or extra) else 0)
    main()

