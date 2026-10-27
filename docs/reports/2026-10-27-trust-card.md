# The folder-trust card, captured and structured

Date: 2026-10-27. The first of the four blocking cards the spec says the runtime has to
give back as a real widget, recorded and described by the page model without any change
to it.

## What was captured

`ui-lm-capture record` against Grok Build in a freshly created scratch directory, at
40x120 for five seconds. A directory the application has not been trusted before raises
the card on startup, so a new scratch directory raises it every time — which makes this
scenario reproducible rather than lucky.

## What the page model made of it

```
[Block/text] rows=10-12
   Do you trust the contents of this directory?
   C:\…\proj
[Block/text] rows=13-15
   Grok Build may run or modify contents in this directory,
   posing security risks.
[Block/text] rows=16-18
   Yes, proceed                 y
   No, quit                     n
   > Yes, proceed  =  y
   > No, quit  =  n
```

Three things are worth naming:

- **The card is not a bordered panel.** It is plain rows with a heading, an explanation
  and a choice list, so it is described by blocks — which is the correct answer, not a
  gap. The `panel` role from the overlay work does not fire here because there is no
  border, and nothing had to be invented to fit it.
- **Its choices are structured data.** `Yes, proceed = y` and `No, quit = n` come out as
  label-and-key pairs, because the two rows share a right edge. A runtime can render
  those as two real buttons and knows exactly which key each one stands for, which is
  what the specification's "four blocking cards as real widgets" requirement needs.
- **The question is separated from its consequence.** The heading names the decision and
  the second block explains the risk; a renderer that keeps them as separate blocks can
  lay them out as a title and a body rather than one paragraph.

## The declined path: there is nothing to see

The card was re-captured and answered with `n` instead of Enter. The application **exits**:
the recorder reports the program stopped, and the session's last frames hold a cleared
screen. There is no "declined" screen, no error message and no fallback prompt — the choice
is to trust the directory and start, or not to start.

Two consequences worth having on record:

- A capture script that does not accept the card records the card, not the application. The
  prologue keystroke is load-bearing, which is why every scenario in this project begins
  with one.
- A rig that wants to observe the declined path has nothing to observe; the interesting
  evidence is the exit, and the recorder already reports it on stderr without failing.

## What this does not do

- The card's own styling is not described beyond per-span attributes; whether the selected
  choice is drawn differently is not established, because the capture has no selection
  state to compare.
- The card blocks the session until answered, which is why it is worth having a scripted
  prologue: a capture without one measures the card, not the application.
