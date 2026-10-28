# Full-screen applications in the page: a design

**Date:** 2026-10-28
**Status:** decided by the lead, since the owner asked not to be involved in decisions this month
**Evidence:** `crates/pages/tests/fixtures/screens/vim-split.txt`, recorded from vim through WSL

## Why

The project exists to take a terminal screen and describe it, so a runtime can draw it
somewhere else. For the screens it was built on — an agent's transcript, a CLI's output —
that works: 185 frames describe at 100%.

A full-screen application is a different object, and the recordings show the page cannot hold
it. A split-screen vim, with two panes, two status lines, a ruler and a column of tildes, is
described as **one `Block` spanning all 36 rows**. Everything a person sees as the
application's structure is inside an opaque wall of text.

So "TUI to GUI" currently means "a TUI's text to a GUI", which is not what the project set
out to do. This design is about the page holding application structure, not more text.

## What the recordings show

| what vim draws | what the page says |
|---|---|
| two panes divided by a status line | one block |
| which pane has focus | nothing |
| mode (`-- INSERT --`, `All`) | nothing |
| ruler `0,0-1` | nothing |
| file being edited, and its split position | a line of text |

The last row of the fixture holds two status lines side by side, which is the clearest
evidence that a row can carry two independent pieces of application state at once — something
no current component can express.

## Approaches considered

1. **A new component kind per shape** — `Frame`, `Pane`, `StatusLine`, `Ruler`. Honest, but
   bumps the catalogue to v0.10 and every consumer with it, for vocabulary invented against
   one application. The project already refused this once, in the `Block` tabs comment: it did
   not invent a seventh component for a tab strip on the strength of one screen.
2. **Put the shapes on `Block` as roles** — a block already carries `role`, and a role is a
   string, so `pane`, `status`, `ruler` cost no schema change. Cheap, and it lets the reader
   start finding structure this week. Its cost is that a pane's *relationships* (which pane
   is focused, which is inside which) have nowhere to live except more strings.
3. **A `Frame` component owning panes** — one new component that holds a list of panes, each
   with its rows, its focus flag and its own children. Correct shape for a windowed
   application; the biggest change, and the one most likely to be wrong before a second
   full-screen application is recorded.

## Decision

**Approach 2 now, towards 3 later.** Concretely:

1. The reader learns to split a screen into panes where an application divides it — a
   full-width status line, or a column of a box-drawing vertical that spans most rows.
2. Each pane becomes a `Block` with `role = "pane"`, carrying its own rows and text. A status
   line becomes a `Block` with `role = "status"`, and the ruler a property of the pane it
   belongs to.
3. Nothing is added to the catalogue version yet. When a second full-screen application is
   recorded — `htop`, `less`, a curses tool — the pane shapes are compared across all three,
   and only then does a `Frame` component get argued for, with three screens of evidence
   rather than one.

This keeps the promise the project already made to itself: no vocabulary from one screen.

## What "done" looks like for this slice

- The vim fixture describes as **more than one component**, each carrying its own rows.
- Every existing evaluation set stays at 100%: training batch (90 frames), failure states
  (73), purpose-built programs (8), real applications (14).
- `self-check` stays at 100% on all sets, so the label path agrees with the reader about the
  new shapes.
- `score` improves on the vim recordings only where the reader itself produces more than one
  component — the model can be no better than the reader here, and the report must say so.

## Non-goals

- Reproducing a full-screen application's colours, cursor position or key handling. The page
  describes a screen, not a running program.
- Replacing the catalogue's existing components. This adds a role vocabulary; it does not
  touch `Meter`, `Diff`, `ToolCall` or `Thought`.
- Making the reverse renderer redraw panes. That follows once the page holds them, and is
  measured by `round_trip` like everything else.
