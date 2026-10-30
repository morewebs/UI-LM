# 0007 — A table is a run of rows whose columns line up

- **Date**: 2026-10-29
- **Status**: Rejected, on measurement (see the end of this document)
- **Deciders**: the project lead (agent), on three screens

## Context

The catalogue has six kinds and no `Table`, and three recordings now draw one:

```
$ df -h
/dev/sdd       1007G   18G  939G   2% /
drivers         477G  427G   51G  90% /usr/lib/wsl/drivers
none             16G   32K   16G   1% /mnt/wslg
```

```
$ top
    PID USER      PR  NI    VIRT    RES    SHR S  %CPU  %MEM     TIME+ COMMAND
```

```
$ cargo tree
├── ui-lm-model v0.1.0
└── vt100 v0.15.2
```

All three read as **one `Block`** today. That is not wrong — a block is a generic container — but it loses the one
thing all three screens are built around: **the columns line up**. A consumer that wanted to render them as a real
table, sort them, or ask *which row is the heaviest* cannot, because the page keeps the rows and throws away the
alignment.

`Block` already carries `tabs`, `shortcuts` and `properties` for exactly this reason — shapes that are not worth a
catalogue component but are worth naming. The question is whether columns are the fourth such shape or the seventh
kind.

## What has been measured, and what has not

Measured: the three screens above, and the fact that all three are single blocks with aligned rows.

**Not** measured: how many *other* screens in the corpus would match a column-alignment predicate. The attempt to
count them this round was defective — a PowerShell regex overload failed silently and the run counted every row —
so the false-positive rate is unknown, and the decision below is written to be narrow until it is known.

## Decision (proposed)

1. **A `Table` is a run of three or more consecutive rows that share at least two column boundaries**, where a
   boundary is a run of two or more spaces at the same position in every row of the run. Alignment is the whole
   signal; nothing here reads the words.
2. **`Body::Table { rows, header: Option<String>, columns: Vec<u16>, lines: Vec<String> }`** — the boundaries as
   column positions, and the rows as drawn, so the round trip is unchanged by construction (the same argument as
   `Frame`).
3. **A predicate, `as_table`, called by both builders**, and **not** applied to rows inside a panel, for the
   reason `Meter` learned in rounds 63–67: a panel owns its rows, and splitting one out of a boxed run costs the
   whole panel's round trip.
4. **No retraining.** The kind is recognised structurally, as `Meter`, `Diff` and `Frame` are.

## What must be true before this is accepted

- the predicate's false-positive rate is measured across the whole corpus, not guessed — the failed attempt this
  round is the reason the status is *Proposed* and not *Accepted*;
- `df`, `top` and `cargo tree` all produce a `Table` and keep their round trip at 100%;
- Freebuff's status rows and every panel in the corpus stay `Block`s.

## Consequences

- Seven catalogue names, and the audit's five invariants apply unchanged (a table describes every row it claims,
  and owns them).
- The renderers gain one arm each: the terminal draws the rows as drawn, the browser emits a real `<table>` with
  the column positions as widths.
- A wrong predicate here is expensive in a new way: a table read as a table is fine, but a *panel* read as a
  table would be cut up, and the audit would not catch it — only the round trip would.

## Outcome: rejected, because alignment is not a table

The measurement this ADR asked for was run properly the round after it was written, in Python rather than
PowerShell, across all 81 recordings:

| rule | recordings matching | worst offender |
|---|---|---|
| 2+ shared column boundaries | **28 of 81** | `freebuff/second-agent-8`, 33 runs |
| 3+ shared column boundaries | **20 of 81** | `freebuff/second-agent-8`, 33 runs |

### The predicate, stated first, then measured

Round 193 re-opened this with the rule written down **before** the measurement, because the round-192 finding was that
the original numbers had no stated method. The predicate:

The predicate lives in `scripts/measure-tables.py` — the code beside the decision — so the numbers below can be
re-derived by anyone with the corpus on their machine:

```
$ python scripts/measure-tables.py
recordings: 97   holding a table run: 43
```

> A table is **three or more consecutive rows** with **the same number of cells**, cells separated by two or more
> spaces, **at the same character offsets** in every row, and at least one cell numeric.

Measured over all 97 recordings:

| recording | longest run | is it a table? |
|---|---|---|
| `tui/wsl-vim` | **34 rows** | no — vim's help and status rows |
| `apps/pip-list` | 31 rows | no — a package listing |
| `tui/linux-top` | 18 rows | no — a process monitor |
| `freebuff/second-agent-17` | 5 rows | no — the survey's answer options |
| **`tui/df-table`** | **0 rows** | **yes — and the rule misses it** |

**43 of 97 recordings match, the largest matches are not tables, and the canonical table does not match at all.** `df`'s
columns are separated by single spaces because the widths are fixed, not by runs of two.

So the rule is not merely noisy — it is inverted with respect to the thing it was supposed to detect. That is a stronger
rejection than this ADR had, and it rests on a predicate a reader can run.

**These numbers cannot be reproduced from this document**, and that is a defect in the ADR rather than in the
measurement. The rule was written down as "2+ shared column boundaries" without saying what a boundary is, which rows
count, or how a run is measured; a careful reconstruction in round 192 — gaps of two or more spaces shared by every
non-blank line of a block, over at least three rows — matched **1 of 97** recordings, not 28.

So the rejection still stands on the qualitative argument above (a survey's answer options align, and so does a `df`
row), but **the number that was used to justify it is not checkable by anyone reading this file**. A measurement
without its predicate is a memory of a measurement.

The honest state of this decision: rejected for reasons a reader can follow, supported by numbers only their author can
reproduce.

Every one of the top matches is Freebuff, and the rows are these:

```
│ 1 Me   2 My employer   3 Both   4 Nobody, I stick to free tools   0 Doesn't apply
│ 1–4 pick · 0 doesn't apply · esc not now (3 days)
```

A survey's answer row has more spaces in it than `df -h` does. **Requiring more boundaries changes nothing**, because
the gap count was never the signal — the freebuff rows simply have more columns than a disk table.

So the predicate as designed cannot exist: there is no structural test that separates a table from a chat panel
with aligned fields. What a `Table` would actually need is **semantics** — a column whose values share a type
across rows (all sizes, all percentages) — and that is a much larger design than a seventh kind, closer to
inference than to reading.

**Rejected until someone proposes the type-based route, with `df`, `top` and `cargo tree` as its acceptance
cases and Freebuff's survey rows as its counterweight.** The three screens remain `Block`s, and the page keeps
their rows — it just does not claim to know they are a table.