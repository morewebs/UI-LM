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
| `Table` | **not implemented** — nothing recorded has drawn one |
| `Thought` | **not implemented** — nothing recorded has drawn one |

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
