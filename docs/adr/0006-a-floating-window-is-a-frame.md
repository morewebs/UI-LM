# 0006 — A floating window is a `Frame`

- **Date**: 2026-10-29
- **Status**: Accepted
- **Deciders**: the project lead (agent), on the evidence of `cases/hard/vim-floating-popup`

## Context

The design has carried a `Frame` since the first draft: a box that sits **over** the base layer rather than
beside it — a dialog, a popup, a modal. It has never had a screen to describe, because every application in the
corpus draws its panes inside one plain rectangle. The handoff has listed it as the first thing to do next for
three weeks.

On 2026-10-29 that changed. `vim -Nu NONE` with

```
call popup_create(["Floating dialog", "Press OK to continue"],
  #{pos: "center", border: [], padding: [1,2,1,2], title: "Confirm"})
```

draws

```
~                          ╔Confirm═════════════════╗
~                          ║                        ║
~                          ║  Floating dialog       ║
```

and the page describes the whole screen as **one `Block` of 23 lines, role `text`**. The dialog, its title and
its body are indistinguishable from the buffer behind them. The score for that recording is 100% with a floor of
100% — the text-only converter reproduces it exactly — which is the project's own warning that a perfect score
here proves nothing.

The recording is kept as `cases/hard/vim-floating-popup`, with a ceiling of **one component**: that number is
the defect, written down.

## Decision

**A floating window is a `Frame`, and it is the sixth kind.**

1. **`Body::Frame { title: String, rows: [u16; 2], lines: Vec<String> }`.** `lines` holds the rows exactly as
   the screen draws them — the box art included. This is deliberate: the round trip places a component's lines
   onto the screen, so a Frame whose lines are the raw rows draws back perfectly without a new renderer path.
   `title` is read off the top border, between the corners.

2. **The predicate is narrow: a double-ruled box.** The run's first row must contain `╔` … `╗` and its last row
   `╚` … `╝`. That is vim's popup — a box drawn with heavy rules over light ones, which is why it is
   distinguishable from the panes and panels already in the corpus, all of which use single rules (`╭╮╰╯`).
   A rule written for one screen shape must be as narrow as that shape; wider readings have cost this project
   points four times.

3. **Both builders call the predicate.** `as_frame` lives in `crates/pages/src/lib.rs` beside `as_meter` and
   `as_diff`, and the label path asks it rather than testing the glyphs again. One rule, one place.

4. **No retraining.** Like `Meter` and `Diff`, the kind is recognised structurally rather than learned. The model
   labels rows; a `Frame` is a decision about a run. `docs/adding-a-component.md` §3 says exactly this for rare
   kinds.

## Consequences

- **Seven places change**, as the procedure lists: the body, the reader, the label path, both renderers, the
  checks and the record. This ADR is step zero.
- **The corpus gains one recording** and the hard suite one case, whose ceiling falls from 1 component to 2 when
  the kind lands — the first ceiling in this project that measures a fix rather than guarding against drift.
- **`Frame` does not imply interaction.** It carries what a dialog *says*, not what pressing its buttons would
  do; actions are still out of scope for every kind.
- **A future application may draw a floating window with single rules.** Then this predicate will not fire, the
  page will describe the window as a block, and the honest response will be a second predicate tied to that
  screen's evidence — not a widened one that claims every single-ruled box is floating.

## What would make this wrong

If a non-floating panel in the corpus uses double rules, `as_frame` would lift it out of the page as a dialog.
The corpus was checked for `╔` before this decision was written: it appears in exactly one recording, the one
above. If that changes, this ADR is reopened.
