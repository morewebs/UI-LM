"""A program drawn to exercise the page model, not to be realistic.

It draws the shapes a page has to recognise — a bordered panel, a meter, a tool line and a
right-aligned shortcut list — then waits, so the screen settles and gets recorded.

Run by `scripts/build-corpus.ps1`; not part of the product.
"""

import sys
import time

out = sys.stdout.buffer


def line(text):
    out.write(text.encode("utf-8") + b"\r\n")
    out.flush()


WIDTH = 64
TOP = "\u256d" + "\u2500" * 2 + " Status " + "\u2500" * (WIDTH - 11) + "\u256e"
BAR = "\u2502"


def row(text):
    line(BAR + " " + text.ljust(WIDTH - 3) + BAR)


line(TOP)
row("1.2K / 500K | [Dashboard]")
row("\u2666 Read notes.txt    +12 -3")
row("Resume Session".ljust(24) + "Ctrl+R".rjust(10))
row("Agent Dashboard".ljust(24) + "/dashboard".rjust(10))
line("\u2570" + "\u2500" * (WIDTH - 1) + "\u256f")
line("")
line("waiting")
time.sleep(3)
