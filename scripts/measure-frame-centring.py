"""Measures the discriminator that lets an untitled box be a `Frame`: is it centred in the screen?

ADR 0006's second shape required a title in the border, because dropping that test promoted **642 blocks in 24
sessions** - Freebuff draws its whole interface with boxes, and every panel in a transcript looked like a window. The
title was doing the work of telling a window from a panel, and tmux's `display-popup` has none.

This file measures the alternative: a floating window is centred in the screen, and a panel in a transcript is not. For
every component that is an untitled light box (`┌...┐` over `└...┘`, vertical borders on every row) it prints the rows
above and below. The rule the reader implements asks for at most two rows of difference.

Run from the repository root:

    python scripts/measure-frame-centring.py

It prints the centred boxes, their margins and sessions, and the total number of untitled boxes, so the claim "8 centred,
all floating windows" can be reproduced rather than believed.
"""

import json
import os
import subprocess
import sys

CAPTURE = os.path.join("target", "debug", "ui-lm-capture.exe")


def sessions(roots=("eval", "cases")):
    found = []
    for root in roots:
        for dirpath, _, filenames in os.walk(root):
            if "session.json" in filenames:
                found.append(dirpath.replace(os.sep, "/"))
    return sorted(found)


def untitled_box(lines):
    """The box's left and right edge columns, or None for a titled box or not a box."""
    if len(lines) < 3:
        return None
    first, bottom = lines[0], lines[-1]
    open_at = first.find("┌")
    if open_at <= 0:
        return None
    close_at = first.find("┐", open_at)
    if close_at < 0:
        return None
    if bottom.find("└") != open_at or "┘" not in bottom:
        return None
    for row in lines[1:-1]:
        if len(row) <= close_at or row[open_at] != "│" or row[close_at] != "│":
            return None
    if "┤" in first[open_at:close_at] or "├" in first[open_at:close_at]:
        return None  # a titled box belongs to ADR 0006's second shape
    return open_at, close_at


def main():
    if not os.path.exists(CAPTURE):
        print("build first: cargo build --workspace")
        return 1
    total_boxes = 0
    centred = []
    for session in sessions():
        try:
            with open(os.path.join(session, "session.json"), encoding="utf-8") as handle:
                height = int(json.load(handle).get("rows") or 0)
        except (OSError, ValueError):
            continue
        if height <= 0:
            continue
        out = subprocess.run(
            [CAPTURE, "page", "--session", os.path.abspath(session), "--all"],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
        ).stdout
        for frame, raw in enumerate(line for line in out.splitlines() if line.startswith("{")):
            page = json.loads(raw)
            for component in page["createSurface"]["components"]:
                if component.get("component") not in ("Block", "Frame"):
                    continue
                lines = [
                    line.get("text", "") if isinstance(line, dict) else str(line)
                    for line in (component.get("lines") or [])
                ]
                if untitled_box(lines) is None:
                    continue
                total_boxes += 1
                rows = component.get("rows")
                if not isinstance(rows, list) or len(rows) != 2:
                    continue
                top, end = rows
                bottom = height - end
                if abs(top - bottom) <= 2:
                    centred.append((session, frame, top, bottom, component.get("component")))

    print(f"untitled boxes in the corpus: {total_boxes}")
    print(f"centred within two rows: {len(centred)}")
    for session, frame, top, bottom, kind in centred:
        print(f"  {session} frame {frame}: {top} row(s) above, {bottom} below ({kind})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
