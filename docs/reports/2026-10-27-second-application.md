# A second application goes through the whole pipeline

Date: 2026-10-27. Closes the standing gap that every claim about this rig rested on
one program: the W1 capture, the K5 measurement and every page produced so far were
Grok Build.

## What was run

`ui-lm-capture record` against **Antigravity CLI** (`agy`, 1.4.7) in a scratch project,
50x160, for 16 seconds, no keystrokes sent. Then `summarize`, `page` and `audit` on the
resulting session.

`opencode` (1.19.3) was tried first and drew **nothing** in 18 seconds: two frames,
each fifty blank runs. It starts and answers `--version` outside a pty, so whatever it
wants before painting is something the recorder did not supply. That is a gap to
explain later, not evidence about this rig, and it is recorded here so it is not
rediscovered.

## What the second application produced

```
1 session(s), 2 frame(s), 2 distinct screen(s)
5.1 s of recorded wall clock
masked cells: 0; frames naming an event range: 0; frames carrying kinds: 0
events labelled: none
```

The splash screen is a block-art banner, a welcome line, a colour-scheme chooser and a
boxed sample conversation. The page model described it without any change to its code:

```
Column  src=seen
Block   src=seen rows=1-6   role=text    (the banner art)
Block   src=seen rows=7-8   role=text    ("Welcome to Antigravity CLI!")
Block   src=seen rows=9-31  role=text    (the chooser and its sample box)
Block   src=seen rows=32-36 role=text
```

`audit` reports **no violations**, which on this codebase means three things held on a
program that is not Grok Build: every component is reachable, in the catalogue, with
spans inside their lines — and every page renders back into the exact text of its
frame.

## What it does and does not show

- **Shows:** the recorder, the mask, the page model and both audits are not
  application-specific. Nothing in the pipeline assumed a Grok Build screen, and a
  different program with different art, different spacing and a different palette went
  through unchanged.
- **Shows:** degradation is graceful. This screen has no tool line, no usage meter and
  no diff, and the model produced ordinary blocks rather than inventing components to
  fit them.
- **Does not show:** that the *interesting* components generalise. `ToolCall`, `Meter`
  and `Diff` were all derived from Grok Build's vocabulary; a second application with
  its own glyphs would need its own recognition. The colour-scheme chooser on this
  screen is a list, and it became a `Block` — correctly, because nothing here can yet
  describe a list.
- **Does not show:** masking on a second application, because it keeps no event stream
  this rig can tap. The mask stayed empty, which is the honest outcome: an unknown
  event source is never read as "no events".
