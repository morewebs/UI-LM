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

## Addendum, 2026-10-29: the case this ADR predicted has arrived

The original text says: *"A future application may draw a floating window with single rules. Then this predicate
will not fire… the honest response will be a second predicate tied to that screen's evidence."*

`whiptail` is that application. It draws

```
               ┌──────────────────┤ Confirm ├───────────────────┐
               │ Delete the build folder?                       │
               │                     <Ok>                       │
               └────────────────────────────────────────────────┘
```

a **centred box with single rules and its title embedded in the top border** — a floating window by any reading.
The page describes it as one `Block`, `floor 100%`, which is the project's warning that the score proves nothing.

So the second predicate is now the work: **a box that does not start at column zero, does not reach the last
column, and carries a title in its top border.** It must be narrow enough not to claim the two indented panels
already in the corpus, and the recording is kept as `cases/hard/whiptail-dialog` with a ceiling of **one
component** — the defect, written down — until it lands.

## The second predicate, landed

Both shapes are now one function, `as_frame`, and each answers a different question:

| shape | the test | where it came from |
|---|---|---|
| heavy rules | `╔`…`╗` above `╚`…`╝` | vim's `popup_create` |
| centred, titled | an indented `┌`, a `└` in the **same column**, and a title between `┤` and `├` | whiptail |

"Centred" is measured by **alignment, not width**: a row is right-trimmed, so "does it reach the last column"
cannot be asked of it — the first version asked exactly that and rejected whiptail's own dialog.

The counterweight was written before the code: **Freebuff's survey is a full-width sheet docked at the bottom**,
it fails the indented-`┌` test, and it stays a `Block`. Measured on the frame that draws it.

## The third shape: untitled, and centred in the screen

tmux 3.4's `display-popup` draws a light box with no title in its border. It was recorded in round 49 and the page called
it a `Block` - a panel - for a year, because the title was the only thing separating a window from Freebuff's panels:
dropping it promoted **642 blocks in 24 sessions**.

The replacement test is **vertical centring**, and it was measured before it was believed
(`scripts/measure-frame-centring.py`):

```
untitled boxes in the corpus: 646
centred within two rows: 6
  eval/apps/tmux-popup frame 3: 9 row(s) above, 11 below (Frame)
  eval/apps/tmux-popup frame 4: 9 row(s) above, 11 below (Frame)
  eval/apps/tmux-popup frame 5: 9 row(s) above, 11 below (Frame)
  eval/apps/tmux-popup frame 6: 9 row(s) above, 11 below (Frame)
  eval/tui/wsl-tmux-popup frame 2: 9 row(s) above, 11 below (Frame)
  eval/tui/wsl-tmux-popup frame 3: 9 row(s) above, 11 below (Frame)
```

Six boxes, all floating windows - tmux's popup, in two recordings. The other **640** untitled boxes are panels inside
transcripts, and none is centred. So `as_frame` grew a third branch: an indented light box whose bottom starts in the same
column, where the rows above the box and below it differ by at most **two**. The screen's height is all the rule needs, so
both builders compute it from the same text; the label path passes the whole screen's length and the reader passes the
frame's.

| shape | the test | where it came from |
|---|---|---|
| heavy rules | `╔`…`╗` above `╚`…`╝` | vim's `popup_create` |
| centred, titled | an indented `┌`, a `└` in the same column, and a title between `┤` and `├` | whiptail |
| centred, untitled | the same box geometry, no title, at most two rows between the box's top and bottom margins | tmux `display-popup` |

The fixture that guarded the old classification, `fixtures/screens/tmux-popup.txt`, now asserts one `Frame` instead of one
panel. The test's intent was always that the border and its contents stay **one** component - a tree-branch rule once split
the bottom border off and the model fell to 90.5% - and that intent is unchanged. The case is also kept as
`cases/hard/tmux-floating-popup` with a required `Frame`, so removing the centring fails a named check.
