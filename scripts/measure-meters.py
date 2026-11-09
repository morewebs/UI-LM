"""Measures the fraction shapes a `Meter` rule could accept, over every recording on this machine.

The model card and the catalogue both say a bare `N/M` is refused because the corpus carries many of them
and nearly all are dates - a number quoted for many rounds with no script behind it. This file is the
missing half: the predicate in code, beside the decision it supports, so the count can be taken again when
the corpus grows.

A page holds its text two ways: most components carry `lines`, and a `Meter` calls its drawn string `text`.
Both are read, because the shapes being counted are exactly the lines a page holds - the corpus's own
meters are the spaced fractions the rule accepts, and a measurement that skipped `text` would report zero
of them.

Three shapes are counted, as **distinct lines** and as **occurrences** - a distinct count says how many
different lines a rule would meet, an occurrence count says how often they appear across the frames of a
session:

- parenthesised  `(15006/15006)` - git's progress counters
- spaced         `1 / 3`          - the shape the `Meter` rule accepts
- bare           `10/17/2026`     - the shape it refuses; nearly all dates

Run from the repository root:

    python scripts/measure-meters.py
"""

import json
import pathlib
import re
import subprocess

CAPTURE = str(pathlib.Path("target/debug/ui-lm-capture.exe").resolve())
PARENTHESISED = re.compile(r"\(\d+/\d+\)")
SPACED = re.compile(r"\d+\s+/\s+\d+")
BARE = re.compile(r"\d+/\d+")


def sessions():
    for root in ("eval", "cases"):
        for marker in sorted(pathlib.Path(root).rglob("session.json")):
            yield marker.parent


def component_lines(component):
    for line in component.get("lines") or []:
        if isinstance(line, str):
            yield line
        elif isinstance(line, dict) and isinstance(line.get("text"), str):
            yield line["text"]
    text = component.get("text")
    if isinstance(text, str):
        yield text


def screens(session):
    out = subprocess.run(
        [CAPTURE, "page", "--session", str(session), "--all"],
        capture_output=True, text=True, encoding="utf-8",
    )
    for raw in out.stdout.splitlines():
        raw = raw.lstrip()
        if not raw.startswith("{"):
            continue
        page = json.loads(raw)
        for component in page.get("createSurface", {}).get("components", []):
            yield from component_lines(component)


def main():
    shapes = {"parenthesised": set(), "spaced": set(), "bare": set()}
    occurrences = {"parenthesised": 0, "spaced": 0, "bare": 0}
    measured = 0
    for session in sessions():
        measured += 1
        for line in screens(session):
            for name, pattern in (("parenthesised", PARENTHESISED), ("spaced", SPACED), ("bare", BARE)):
                if pattern.search(line):
                    shapes[name].add(line.strip())
                    occurrences[name] += 1
                    break
    print(f"{measured} recording(s)")
    for name in ("parenthesised", "spaced", "bare"):
        print(f"{name:14} distinct {len(shapes[name]):5}   occurrences {occurrences[name]:6}")
    for line in sorted(shapes["bare"])[:6]:
        print(f"  bare: {line[:90]}")


main()
