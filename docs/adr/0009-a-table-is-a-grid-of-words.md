# ADR 0009: a table is also a grid of single words

**Status:** Accepted.
**Date:** 2026-11-06.
**Extends:** [ADR 0008](0008-a-table-is-numeric-columns.md), whose two signals cannot see this shape.

## Context

ADR 0008 gave `Table` two signals: values in columns, and a heading the application underlined. `ls -C` has neither. It
draws two or three columns of file names, padded to alignment, with no digit in most rows and no dash row.

ADR 0007 measured the obvious rule — cells separated by two or more spaces at identical offsets — and rejected it because
it matched `vim`'s help rows and scored zero on `df`. That rejection stands; `df` still needs the values rule. But the
alignment rule's failure was not that it saw `ls -C`; it was that it **also** saw help pages. The difference is the
**cell**: a grid's cell is one word, and a help row's cell is a sentence.

## Decision

**A table is also three or more consecutive rows, each splitting on runs of two or more spaces into two or more cells,
where every cell is a single word (`[A-Za-z0-9._+\[\]-]+`), no row carries a dash underline, and every row starts its
cells on the grid's columns. A row may hold only the leading cells, down to one — a column-major listing's last row
stops where its shortest column does — but it may not skip one, and the grid itself needs two columns.**

The underline signal is asked first (ADR 0008); this rule refuses a block with a dash row, because that shape belongs
there.

## Evidence

Recorded for this question (`eval/apps/ls-grid-*`) and measured over the whole corpus with
`scripts/measure-word-grids.py`:

| screen | rows | columns | what it is |
|---|---|---|---|
| `ls-grid-usr-bin` | 30 | 2 | `ls -C /usr/bin` |
| `ls-grid-etc` | 30 | 3 | `ls -C /etc` |
| `ls-grid-usr-lib` | 23 | 3 | `ls -C /usr/lib` |
| `pip-list` | 31 | 2 | `pip list` |
| `pwsh-wide` | 8 | 3 | PowerShell's `Format-Wide` |
| `column-grid` | 4 | 2 | util-linux's `column -t` |
| `pr-grid` | 4 | 2 | util-linux's `pr -t -2` |

Seven screens, five applications, and **no prose block matches**: not vim's help, not an agent's listing, nothing. The
cell test is what makes the difference — the alignment rule matches 18 rows of one vim help screen, this rule zero.

### The rows are ragged, and the first rule required a rectangle

All seven screens above are rectangles: every row holds the same number of cells, and the rule as first written required
that. A real `ls -C` is not a rectangle. It fills columns top-down, so its last row stops where the shortest column does,
and the rule refused every untruncated listing; every grid it accepted was a rectangle: a listing that happened to end
on a full row, or the hard case's `head`ed one, cut off at one. A width where the last column holds a single name makes
the last row one cell wide, and the rule as first widened still refused that row; the minimum is about the grid, which
still needs two columns, not about every row of it.

Seven screens recorded for the question, at widths where the row stops early:

| screen | rows | columns | last row | what it is |
|---|---|---|---|---|
| `ls -C /usr/lib`, 40x120 | 14 | 5 | 4 cells | the finding |
| the same screen with `--color=always` | 14 | 5 | 4 cells | colour changes nothing |
| `ls -C /usr/bin`, 40x120 | 39 | 3 | 2 cells | |
| `ls -C /etc`, 30x100 | 29 | 4 | 3 cells | |
| `ls -C /usr/share`, 40x100 | 33 | 4 | 3 cells | |
| `ls -C /usr/lib`, 40x60 | 35 | 2 | 1 cell | the hard case; a column runs out entirely |
| `ls /usr/lib` piped to `column -c 120` | 14 | 5 | 4 cells | a row-major fill ends short too |

Every one satisfies the widened rule and none is a rectangle; every one read as one `Block` before the change and as one
`Table` after it. The whole corpus was re-rendered before and after both widenings: **1,684 pages, byte-identical** —
no component the rule already saw moved, and no prose block became a table. The cell test is untouched: a help row's
cell is still a sentence. The one-cell shape has a test of its own, and a block whose every row holds one word is still
refused — that is a column, not a grid.

## Consequences

- `ls -C` and `pip list` are `Table`s. The pages, both renderers and the audit carry them unchanged, because the rows
  were already one block.
- `scripts/measure-tables.py` implements the same rule for `--disagreements`, and `check-tables` compares the two. A rule
  that only the reader knew is exactly what that check exists to catch; with the reader demanding a rectangle while the
  specification allowed short rows, it named the missed component (`cases/hard/ls-ragged-grid`) by id.
- **A rectangle was a truncation check.** Requiring equal rows made the rule see only listings cut off by `head`, which is
  why its own evidence could not show the failure: `ls -C /etc | head -30` is a rectangle by construction. The rule now
  refuses a row that *skips* a column, which is the difference between a short row and a different grid.
  `cases/hard/ls-ragged-grid` holds the untruncated shape: its last row is one name, so demanding a rectangle again,
  or two cells in every row, each turns it back into a `Block` and fails `check-hard` by name.
- The rule has a cheap gate: a grid needs two or more cells in its widest row, so it needs a run of two spaces in a
  majority of rows. Prose pays one scan and no allocations, which is the budget ADR 0008's pre-filter already protects.

## Alternatives rejected

- **The alignment rule (ADR 0007).** Rejected by its own measurement: 18 vim help rows.
- **No third signal.** Then a grid is prose, which is what the reader said before this ADR; the page loses the fact that
  the screen held columns.
- **A rectangle of cells.** The first version of this rule, and wrong for the reason above: a column-major listing is
  ragged at the bottom, and the rule's evidence had been truncated into a rectangle by `head`.
- **A rule requiring no digits.** `pip list`'s version column has digits and the values rule still cannot see it (one
  numeric strip, not two); requiring "no digits" would lose the package listing for no gain.
