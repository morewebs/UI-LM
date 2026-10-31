# ADR 0008: a table is numeric columns, not padding

**Status:** Accepted.
**Date:** 2026-10-31.
**Supersedes:** the conclusion of [ADR 0007](0007-a-table-is-aligned-rows.md), which rejected a `Table` kind on an
alignment predicate that turned out to be inverted.

## Context

ADR 0007 rejected a `Table` kind because the only predicate it had — cells separated by two or more spaces at identical
offsets — was not merely noisy but **backwards**: it matched `vim`'s help rows and scored zero on `df`, the one screen in
the corpus that is unambiguously a table.

The rejection was right about what it measured. It was wrong about the question, and it took two rounds in a later window
to see that: `df` pads its columns with **single** spaces because its widths are fixed, while `top` and PowerShell's
`Format-Table` pad with many. Alignment is a property of the formatter, not of the data.

## Decision

**A `Table` is a run of three or more consecutive rows in which at least two vertical strips of two or more characters
hold a digit in a majority of those rows, the strips separated by at least one entirely blank column.**

Measured in `scripts/measure-tables.py`, on the corpus as it stands:

| session | alignment rule | values rule | a table? |
|---|---|---|---|
| `tui/df-table` | 0 | **15** | yes |
| `tui/linux-top` | 18 | **24** | yes |
| `apps/cmd-dir` | 0 | **11** | yes |
| `apps/pwsh-table` | 4 | **10** | yes |
| `tui/wsl-vim` | 18 | **0** | no |
| `freebuff/second-agent-17` | 5 | **0** | no |

**Nineteen of the corpus's 105 recordings match**, headed by `ls` and `ps` at twenty-nine rows each. Seventeen of the
nineteen are screens a person would call a table: `df`, `ls`, `ps`, `top`, `dir`, `gh run list`, `git log --stat`,
`Format-Table`.

## Consequences

**Two false positives, both inside a transcript.** Agent sessions `second-agent-2` and `second-agent-4` hold three and
four rows where prose happens to align into two numeric strips. The kind is therefore **not** "any run that matches the
predicate"; it is:

> A table is a run of rows that matches the values rule **and forms a block of its own** — it is not a stretch inside a
> larger block of prose.

That guard is what makes the majority rule at a three-row minimum the right default. The alternatives were measured and
both cost real tables:

| rule | recordings matched | what it loses |
|---|---|---|
| majority, 3-row minimum | **19** | admits two agent screens |
| majority, 5-row minimum | 13 | `diff-stat`, `log-stat`, `git-logstat`, the survey table |
| every row holds the strip, 3 rows | 10 | `cmd-dir` — `dir`'s `<DIR>` rows break the streak |

**What adding the kind touches**, in the order `docs/adding-a-component.md` gives: the catalogue constant
(`COMPONENTS`, six kinds becomes seven), the page format, the reader in `crates/pages`, both renderers in `crates/render`,
`check-kinds` (a seventh probe), the catalogue reference, and the model card's boundary table with a forbidden-kind guard
on a screen that must never hold one.

**What it does not touch:** the trained model. A `Table` is a *shaping* decision — the rows already exist and the reader
already groups them into blocks — so the model keeps its six labels and the kind carries no training data of its own.
That is the same split the project runs on everywhere: what a thing *is* is learned; how it is *shaped* is written.

**Why this is not "a vocabulary from three screens".** The bar the project set was three screens of evidence. This rests
on nineteen recordings, four applications, and a predicate whose errors were measured in both directions before the
decision was taken.

## Alternatives rejected

- **Keep rejecting.** Possible, but the rejection would have to argue that `df`, `ls` and `ps` are not tables, which is
  what the alignment rule effectively did.
- **A `Table` decided by the model.** The model's labels are per row, and a table is a property of a *run*; asking the
  model for it would put a structural decision into a classifier that is at its worst exactly on plain shell output
  (55% disagreement on the `structured` set).
- **A looser rule with no standalone guard.** Measured: it puts a table inside an agent's reasoning.
