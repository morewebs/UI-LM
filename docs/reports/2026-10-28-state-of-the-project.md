# State of the project

Written after a long autonomous stretch, for the person who asked to be left alone with it. Plain
language, no jargon, and the gaps stated as plainly as the results.

## What it does

You give it a terminal program. It:

1. **records** what the program draws, screen by screen, with colour and position;
2. **describes** each screen as a structured page — text blocks, tool calls, meters, reasoning
   lines, diffs, and the panes of a full-screen application;
3. **draws that page again** somewhere else — a browser, a terminal, or a live view that updates as
   the program runs;
4. **lets you type back** into the program from the browser.

A small trained model decides what each row of a screen is. The code that watched the screen decides
what colour it is and how a component is shaped. That division is deliberate: the model judges,
the code constructs.

## What is proven

| claim | evidence |
|---|---|
| a screen's text becomes a page holding real components | every evaluation set at 100% |
| a page draws back into a terminal, every line of content | 100% on every recording, split screens included |
| the page carries the screen's colour | 6,303 spans on the corpus; reaches HTML, terminal and the live view |
| pages say honestly whether a component was read or carried | 688 components, 0 contradictions |
| the model works off its training application | trained on one; **141 frames from 13 other programs, every component** |
| new screens, recorded after training, read perfectly | five consecutive clean batches |
| a split screen's panes hold their own components | both paths, read out of the v0.7.0 archive |
| an agent running in one pane of a split reads perfectly | 100% on the two recordings that drove the container (53.9% and 96.0% when it was written) |
| the download works | **v0.7.0**, three platforms, verified by downloading it and running it |
| the viewer keeps up | typing, redraws, half-written files, cleared screens — each checked |
| exporting a session is cheap | 4.4 ms and 3.9 KB per page, linear to 1,510 pages, capped by a check |

## What is not proven, or cannot be

- **The training material is one agent's transcripts.** Only software that draws *state* produces
  the components this catalogue describes — seven other programs recorded for training produced
  eleven screens and not one structured component. A second agent's transcripts are the only way to
  test whether these shapes are general, and I don't have any.
- **Every screen came from this machine**, and most were chosen by me. Five clean batches is the
  strongest claim available; it is not the same as a stranger's workflow.
- **The model is at its ceiling, and that is measured three ways.** 60% more corpus: +0.1% row
  accuracy. A twentieth feature: worse on every screen family. Training on plain screens: three
  times worse. The limit is the vocabulary, not the data.
- **A perfect page score is partly the assembly's work.** The model's raw labels disagree with the
  reader on 2–12% of rows, and structural guards are what keep pages exact. That's by design, and it
  took a diagnostic to see it.
- **Some shapes are not representable.** A floating
  window with a border is a panel, one without is a line of words; a tab strip only when numbered.
- **Live is not continuous.** A frame exists when the screen settles, so a fast program yields fewer
  moments than it drew.

## Where the code is

Seven crates, one per job: capture, frames, pages, model, render, serve, validate. Eight check
scripts run every claim above in about six minutes. `HANDOFF.md` is the short version for whoever
picks this up next.

## The three things I would do next

1. **Give panes a container** so a pane can hold a diff or a meter rather than a wall of text.
   Designed in ADR 0004, costed at about half a day, waiting for a screen that demands it.
2. **Get a second agent's transcripts** — the only path to knowing whether the component shapes are
   general or this application's.
3. **Stop calling it a model success and start calling it a reader success.** The honest description
   of what works is that a small classifier plus a careful reader beats a big model alone, and the
   reader is where the remaining value is.
