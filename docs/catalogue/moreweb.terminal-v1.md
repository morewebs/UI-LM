# `moreweb.terminal/v1` — the catalogue as built

What this vocabulary contains today, what each part guarantees, and which parts have been
seen on a recorded screen. It is the reference for the runtime and for the next plan; where
it and the code disagree, the code is right and this page is a bug.

## The envelope

A page is one A2UI v0.9.1 message:

```json
{
  "version": "v0.9.1",
  "createSurface": {
    "surfaceId": "session",
    "catalogId": "moreweb.terminal/v1",
    "root": "root",
    "components": [ … ]
  }
}
```

`version` is the envelope's and `catalogId` is the vocabulary's; they are pinned
independently, because A2UI is pre-1.0 and the terminal vocabulary must not move when it
does (ADR 0002). `root` names the component to render first; every component is reachable
from it.

## Provenance

Every component carries `source`:

| value | meaning |
|---|---|
| `seen` | every character came from the recorded cells |
| `carried` | on screen now, unchanged from an earlier frame of the same session |
| `guessed` | reconstructed from context rather than read — **never produced today** |

Describing one frame marks everything `seen`. Describing a session marks a component
`carried` once its content has already appeared, so a consumer can tell what actually
changed between two moments without diffing the JSON.

## Ids

A component's id is derived from its own text (FNV-1a), not from its position. A block that
scrolls keeps its id, which is what the plan's metric M5 needs. Two components with
different text cannot collide; two with the same text in the same page are a fault the
audit reports.

## Components

### `Column` — the root container

```json
{ "id": "root", "component": "Column", "source": "seen", "children": ["block-…"] }
```

Holds component ids in order. One page, one column.

### `Block` — a run of rows that reads as one thing

```json
{
  "id": "block-92e6342306f66f5b",
  "component": "Block",
  "source": "seen",
  "role": "text",
  "rows": [0, 1],
  "lines": ["Grok Build fixture"],
  "spans": [[]]
}
```

| field | meaning |
|---|---|
| `role` | `text`, `prompt`, `heading`, `composer` or `panel` |
| `rows` | half-open `[first, end)` row range on the screen |
| `lines` | the text, one entry per row, right-trimmed |
| `spans` | per-line styling, parallel to `lines`; only non-default cells appear |
| `tabs` | present when the block is a panel with a tab strip |
| `shortcuts` | present when the block holds label-and-key rows |
| `properties` | present when the block reports named values (`Model: Grok 4.7`) |

Roles mean what their evidence proves: `composer` is a bordered box holding the input
prompt, `panel` is a bordered box, `prompt` starts with `>`, `heading` with `#`. Everything
else is `text` rather than a guess.

A span is `{ "start": 0, "end": 2, "style": { … } }` with `bold`, `italic`, `underline`,
`reverse` and colour as `indexed:N` or `rgb:R,G,B`. Per-span attributes are the gap every
surveyed standard has and the thing a `Diff` is built from.

### `ToolCall` — what the program read, ran or edited

```json
{
  "id": "block-29aec41e1ea4737d",
  "component": "ToolCall",
  "source": "seen",
  "rows": [4, 5],
  "calls": [
    { "status": "done", "tool": "Read", "target": "notes.txt",
      "text": "  ♦ Read notes.txt" }
  ]
}
```

`added` and `removed` appear for an edit (`+1 -1` in the application's own line). `status`
is `done` for every call this slice can see, because the glyph marks a call the application
has finished drawing.

### `Table` — numbers under headings

```json
{
  "id": "table-1a2b3c4d5e6f7788",
  "component": "Table",
  "source": "seen",
  "rows": [0, 12],
  "columns": [0, 18, 22, 27],
  "cells": [["/dev/sdb", "97M", "97M", "0"]],
  "lines": ["/dev/sdb         97M   97M     0 100% /usr/lib"]
}
```

| field | meaning |
|---|---|
| `rows` | half-open `[first, end)` row range on the screen |
| `columns` | where each column starts, in characters |
| `cells` | one entry per row, one cell per column, trimmed |
| `lines` | the rows as drawn, parallel to `cells` |

**The rule (ADR 0008): values, not padding.** `df` separates its columns with single spaces and PowerShell's
`Format-Table` with many, so what makes a table is **two vertical strips of two or more characters holding a digit in a
majority of the rows**, separated by a blank column - and the run must be a block of its own, not a streak inside prose.

Measured on this corpus: **49 components in 105 recordings**, headed by `ls`, `ps`, `df`, `top`, `dir` and formatted
listings. The alignment rule it replaced scored zero on `df` and matched `vim`'s help rows - see ADR 0007 for the
rejection and ADR 0008 for the decision.

A renderer that cannot use the columns can always draw `lines`: a table never loses text.

### `Meter` — a quantity against a limit

```json
{
  "id": "block-dc77df5d0194dc2c",
  "component": "Meter",
  "source": "seen",
  "rows": [11, 12],
  "used": "1.2K",
  "limit": "500K",
  "fraction": 0.0024,
  "label": "[Dashboard]",
  "text": "1.2K / 500K | [Dashboard]"
}
```

The drawn strings are kept verbatim beside the computed `fraction`, so a renderer that
wants a bar and a reader that wants the number need not parse the screen.

### `Diff` — an edit to a file

```json
{
  "id": "block-cfe59783634ac500",
  "component": "Diff",
  "source": "seen",
  "rows": [8, 10],
  "lines": [
    { "kind": "remove",  "text": "  - old line", "spans": [] },
    { "kind": "add",     "text": "  + new line", "spans": [] }
  ]
}
```

`kind` is `add`, `remove` or `context`. This is the component the ADR calls the thing no
surveyed standard can express: a per-line kind *and* per-span attributes at once.

## What is proven, and where

| Component | Evidence |
|---|---|
| `Column`, `Block` (text/prompt/heading) | recorded screens, checked in as a fixture, read by CI |
| `Block` role `composer` | recorded screens (11 instances across 44 pages) |
| `Block` role `panel` with `tabs` | a recorded `/usage` overlay |
| `Block` `shortcuts` | a recorded permission card (`Yes, proceed = y`) |
| `ToolCall` | recorded screens (`Read notes.txt`, `Hook … failed`) |
| `Meter` | recorded screens (`1.3K / 500K`) |
| `Diff` | **constructed frames only** — no recorded screen has drawn a diff body |
| `Thought` | a recorded screen with reasoning raised: `♦ Thought for 3.0s` |
| `Table` | **not implemented** — nothing recorded has drawn one |


## `Block` is carrying the structure

`Block` now holds `lines`, `spans`, `tabs`, `shortcuts` and `properties`. That is one field
per structure found on a recorded screen, and it is worth naming the tension: the block is
becoming a container for everything a panel can show. The alternative — a component per
structure — would version the catalogue once per screen, which ADR 0003 already declined
for blocking cards. A schema rework is justified when there is a consumer to break; there
is not one yet, and each field is individually evidenced.
### `Thought` — reasoning the application collapsed

```json
{
  "id": "block-…",
  "component": "Thought",
  "source": "seen",
  "rows": [7, 8],
  "text": "    ♦ Thought for 3.0s",
  "seconds": 3.0
}
```

Recorded with the CLI's reasoning effort raised: the application draws one line naming how
long the reasoning took and keeps the reasoning itself collapsed. The page therefore holds
the summary and **not** the hidden text — the spec's folding rule applied literally, since a
folded block's content is not on the page and so is not in the IR. `seconds` is absent when
a line does not report a duration.
## Why the command list is not a `Table`

A recorded `/help` overlay draws a command list — rows of `New Session … Ctrl+N`, grouped
under `Session`, `Context` and headings of their own. It is described as `shortcuts` on a
`Block`, not as a `Table`, for two reasons: the rows are label-and-key pairs, which is what
`shortcuts` exists for, and the overlay has no column headers — the group names are
headings over sections, not names for columns. `Table` stays unimplemented; a genuine grid
with `Table`'s shape has still not been drawn by anything recorded.

A side effect worth knowing: that overlay's bottom border is scrolled off, and the input
box touches it below. Blocks now split at every opening border, so the overlay and the
composer are two components rather than one thirty-three-line "composer" — which is what
the first attempt produced.
## The two masking rules

A frame records cells and marks the ones that are not content:

| reason | when |
|---|---|
| `no_semantic_change` | the cells changed with no event and no keystroke behind the change |
| `known_glyph` | the cell holds `U+FFFD`, a byte the pty host could not represent at all |

The second rule is narrow on purpose. A wider "glyphs that look wrong" list would be a
list a new font invalidates — and a real recording produced the case it covers: CP437
box-drawing bytes arrived as replacement characters, and a model taught that a row of them
is meaningful would learn nothing true. Verified on a capture: four replacement characters,
one span, `known_glyph`.
## Guarantees

Three properties are checked on every corpus (`ui-lm-capture audit`) and, for the
checked-in fixture, in CI:

1. **Soundness** — every child id resolves, no id repeats, every span lies inside its line,
   every component is one this catalogue defines, and the root exists.
2. **Losslessness** — rendering a page back to text reproduces the screen it described,
   row for row (`ui_lm_pages::render_text`). A page that drops a character fails.
3. **Reproducibility** — two captures of the same deterministic scenario score 100%
   structure and 100% text against each other (`ui-lm-capture compare`).

## Degradation

An unknown component is rendered as a labelled placeholder and never dropped. That rule
belongs to the runtime, and it is the reason this document names `Table` and `Thought` as
absent rather than leaving them implied: a page must not name a component a renderer of
this catalogue cannot draw.
