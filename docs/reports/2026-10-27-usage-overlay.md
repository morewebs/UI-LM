# What the usage overlay draws, and why it is not a `Table`

Date: 2026-10-27. Recorded while looking for real evidence for the `Table` component.

## What was captured

`ui-lm-capture record` against Grok Build in a scratch project, driving `/usage` and then
15 frames were produced. The overlay is a bordered panel:

```
                     ┌──────────────────────────── [x] ─┐
                     │  Context usage  Usage limit  Session info     │
                     │───────────────────────────────────────────────│
                     │  Usage limits are managed by your team.       │
                     │                                               │
                     │  Session usage: no model calls yet in this session.
```

## Why this is not a table

- The header row is a **tab strip**, not column names: the three words name views, and
  the divider under them is the tab rule, not a header separator.
- The body is **prose**, not rows of cells. There is exactly one line per statement and
  nothing lines up beneath the header's columns.
- A detector that fired on "a header row and a horizontal rule" would label this a table
  while a session list, a diff, and any bordered panel with a rule would also match. That
  is the mislabelling direction this project refuses: the page would claim a structure the
  screen does not have.

So `Table` stays unimplemented. The catalogue defines it, and nothing recorded so far has
drawn one; the honest state is "the component exists in the vocabulary and has no evidence
yet", which is what the W1 report says about `Diff` too.

## What this does establish

- The overlay opens on `/usage`, so its screens are reachable by a scripted capture when
  the vocabulary has something to say about a tabbed panel.
- A **tab strip** is a real, recurring structure (`Context usage`, `Usage limit`,
  `Session info`) and the catalogue has no component for it. That is a vocabulary gap
  worth naming before the catalogue is versioned, rather than after.
- The panel shows that this application's overlays are drawn inside a rounded border with
  a `[x]` close affordance in the top rule — useful for the runtime, which will have to
  give the same overlay back as a real panel.

## Method

Everything above was read from recorded pages produced by
`ui-lm-capture page --session <dir> --all`, so it is evidence from the path the model will
be trained on.
