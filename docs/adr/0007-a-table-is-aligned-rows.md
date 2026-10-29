# 0007 — A table is a run of rows whose columns line up

- **Date**: 2026-10-29
- **Status**: Proposed
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
