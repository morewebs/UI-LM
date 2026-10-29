# 5. A paned page is drawn by the reader's renderer

**Status: accepted, implemented in v0.12.0**

## The decision

When a page carries panes, `ui_lm_render::render_terminal` hands it to `ui_lm_pages::render_text` and
returns that drawing. For a page with no panes it keeps its own code path.

## Why

There are two terminal renderers in this project, and they answer different questions:

- `ui_lm_pages::render_text` draws a page **back onto its own screen**, placing every component at the
  row it claims — because the reader is what put those rows there, and the round trip is measured
  against it;
- `ui_lm_render::render_terminal` draws a page as **text a person can read**, in order, for the CLI and
  for the HTML exporter.

For a flat page the two agree. For a paned page they diverged: `render_terminal` took each pane's
components and *stacked* them, ignoring the rows they claim, so two full panes overlapped themselves and
**76% of lines came back in the wrong column** — 322 of 1,351.

The alternative was to duplicate row placement inside `render_terminal`. That is the same logic written
twice, in a project that has already paid for one divergence between two implementations of the same
rule (the pane-split gate, one-line fix, six rounds of confusion).

## The consequence

- `render_terminal`'s paned output is now *the screen*, not a readable approximation. For the HTML
  exporter this changes nothing: `render_page` builds its own structure and wraps panes in a grid.
- The stacking branch remains for a page whose panes are empty and whose components carry the older
  `pane` role, which is the shape recorded before the container existed.
- `check-hard` requires every hard recording to draw back at 100%, so this decision is now measured
  rather than remembered.

## What would change it

A page format that keeps column positions per component (rather than per pane row) would let the
readable renderer place rows itself without duplicating the reader. Until then, delegating is the only
version where the two renderers cannot disagree.