# 0004: panes need a container, and now there is evidence

**Date:** 2026-10-28
**Status:** proposed — not implemented
**Supersedes:** the "approaches 2 now, towards 3 later" decision in
[the full-screen design](../superpowers/specs/2026-10-28-full-screen-applications-design.md)

## The question

A full-screen application can divide its window. How should a page say so?

## What the page does today

A screen that is a single block and carries a column of `|` down most of its rows is split into
two `Block`s whose `role` is `"pane"`. That was approach 2 in the design: a role on a block, costing
no schema change.

It works for the case it was built for, and each pane's text keeps its width so the screen draws
back exactly (89/89 lines on vim's split, 92/92 on tmux's).

## What it cannot do

**A pane cannot hold components.** A pane is one block of text. When a split screen also carries
anything else — a popup, a status line with structure, a meter — the pane rule is never consulted,
because it only runs when the whole screen came out as a single block. Measured on a real
popup-over-panes recording: the page holds `Block(text)`, `Block(panel)`, `Block(text)`, and the
split is simply absent.

A previous attempt to widen the rule (refuse only full-width frames, so a dialog would not mask a
split) changed nothing, because the obstacle was not the box guard but the single-block
precondition. It was reverted.

## The three screens

The design asked for three screens before adding vocabulary. They now exist:

| screen | recording | what the page says |
|---|---|---|
| vim, vertical split | `eval/tui/wsl-vim-vsplit` | two `Block`s, role `pane` |
| tmux, two panes | `eval/tui/wsl-tmux-split` | two `Block`s, role `pane` |
| tmux popup over two panes | `eval/tui/wsl-tmux-panes-popup` | three blocks — **the split is missing** |

The third is the one that shows the limit: two applications divide their window the same way, and an
overlay makes the division disappear.

## Proposal

Add one container component to the catalogue, and make a pane a container rather than a block:

```
Frame
  panes: [ Pane { id, focused, children: [Block, Meter, …] } ]
```

or, if a `Frame` is more than is needed, `Column` gains an optional `panes: [id]`, because `Column`
already exists, already holds children by id, and the page's root is one.

**The minimal version worth arguing for** is the second: panes as identified regions in the existing
container, with a pane's children being ordinary components. That needs no new component kind, only a
new field, and it answers the case the role cannot.

## What is deliberately not decided

- Whether a pane reports focus. vim and tmux both draw a focused pane, and neither page says which
  one is active; that is a second, smaller decision waiting for a screen that depends on it.
- Whether a floating window is a pane with a position. The popup screens suggest it might be, and
  one of them is borderless and therefore invisible to the reader today.

## Why write this down instead of building it

The change touches the page schema, the reader, the label path, both renderers and `compare`, and
`self-check` will hold every one of them to the same rule. That is a day's work with a failing test
at each step, not an afternoon's, and doing it fast on a month's evidence of my own error rate would
be the same mistake this project keeps recording.
## Second screen: a pane holds a diff, and the page cannot say so

Recorded `vimdiff` on a real file pair — two panes, each showing a diff of the same two files:

```
  apples 12                     |  apples 14
  pears 7                       |  pears 9
  cherries 41                   |  cherries 41
```

What the page says: **two `Block`s with role `pane`, each 26 lines of text**, model 100% against a
floor of 66.7%, `fields` 0 differing, round trip 28/28.

So the page knows there are two panes and cannot say that **each pane holds a diff** — the one
component the catalogue exists for. A diff inside a pane is, to the page, a paragraph.

This is the case the ADR predicted and could not previously show: panes are structure *around*
content, and today they are structure *instead of* content.

| screen | what the page cannot say |
|---|---|
| popup over two panes | the screen is split at all |
| `vimdiff` | each pane holds a diff |

Two screens, both real, both in `eval/tui`. The design's rule asks for three before new vocabulary;
this ADR's rule should be the same, so it stays **proposed** and the third screen is the remaining
requirement — a pane holding a meter, a tool call, or a tab strip.
## What it would cost, counted rather than guessed

Before deciding, the change was priced by counting its touch points:

| | |
|---|---|
| files mentioning `Body::Column` | 5 — `pages/lib.rs`, `pages/labeled.rs`, `render/lib.rs`, `render/terminal.rs`, `capture/main.rs` |
| occurrences | 19 |
| construction sites that would need a new field | 11 (8 of them in `pages/lib.rs`, several inside tests) |

So it is mechanical work with a wide blast radius: a new field on `Column`, eleven literals, plus
the reader learning to split before it segments, the renderer learning to draw a pane's children
side by side, the label path learning the same split, and `self-check` holding all of them to one
rule.

**Half a day, with a failing test at each step.** That estimate is the honest answer to "why not
this afternoon", and it belongs in the ADR next to the evidence.
## What the attempt found before it was reverted: panes carry no styling

Before building the container, one smaller question was tested on the `vimdiff` screen: do panes carry
the colour the screen drew? They do not — **both panes reported 0 styled lines**, because the pane
path builds its spans empty. A `vimdiff` window shows its changes in red and green, and the page holds
a plain block.

A fix was written: slice each row's spans at the divider, shifting the right pane's offsets to zero.
It worked — 25 of 26 lines styled in each pane — and then broke something more interesting:

```
fields tui: 119 component(s) compared, 12 differing
  Block role "pane"/"pane" ... spans 15/3
```

The **label path does not split panes the same way**. It only consults the pane rule when a screen
produced a single component, so for `vimdiff` it produces no panes at all, and the model's copy cannot
receive spans it has no components for. Grafting by row fails too: both panes claim the whole screen's
rows, so the row-keyed copy lands the left pane's styling on both; matching by text fixes the pairing
but not the absent panes.

**The whole attempt was reverted**, and the finding is the point: pane styling cannot be fixed
without giving the label path the same pane split, which is the container work this ADR describes.
Listed as another entry in its cost rather than a separate defect.
## Attempted, and the estimate held: it does not fit the tail of a round

Started the change and tried a cheaper shape than the ADR first proposed — the grouping on
`Surface` rather than on `Column`, since `Surface` has five construction sites against eleven.

It went the way the estimate said it would: the field rippled through both page builders, then
through `Surface` literals inside test modules, one at a time, each found by the compiler. At the
point where the round's remaining budget ran out, the tree was mid-change and **nothing useful was
committed**.

Everything was reverted. `cargo test` green, `fields` 0 differing, tree clean, HEAD unchanged.

**Two things are now established by trying rather than by arguing:** the change is mechanical and
does not fight the design — the compiler walked me through every site without a single conceptual
problem — and it is genuinely too large for the last third of a round. The next attempt should start
at the top of a round, or be split into "add the field everywhere, no behaviour" followed by
"use it" in a second.

That second shape is better than what I tried today, and it is what the next attempt will do.