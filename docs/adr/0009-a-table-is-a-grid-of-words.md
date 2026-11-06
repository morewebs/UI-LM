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

**A table is also three or more consecutive rows, each splitting on runs of two or more spaces into two or more cells at
the same character offsets, where every cell is a single word (`[A-Za-z0-9._+\[\]-]+`), and no row carries a dash
underline.**

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

Four screens, two applications, and **no prose block matches**: not vim's help, not an agent's listing, nothing. The
cell test is what makes the difference — the alignment rule matches 18 rows of one vim help screen, this rule zero.

## Consequences

- `ls -C` and `pip list` are `Table`s. The pages, both renderers and the audit carry them unchanged, because the rows
  were already one block.
- `scripts/measure-tables.py` implements the same rule for `--disagreements`, and `check-tables` compares the two. A rule
  that only the reader knew is exactly what that check exists to catch.
- The rule has a cheap gate: a grid needs two or more cells, so it needs a run of two spaces in a majority of rows.
  Prose pays one scan and no allocations, which is the budget ADR 0008's pre-filter already protects.

## Alternatives rejected

- **The alignment rule (ADR 0007).** Rejected by its own measurement: 18 vim help rows.
- **No third signal.** Then a grid is prose, which is what the reader said before this ADR; the page loses the fact that
  the screen held columns.
- **A rule requiring no digits.** `pip list`'s version column has digits and the values rule still cannot see it (one
  numeric strip, not two); requiring "no digits" would lose the package listing for no gain.
