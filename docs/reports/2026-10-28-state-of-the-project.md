# State of the project

Written after a long autonomous stretch, for the person who asked to be left alone with it. Plain
language, no jargon, and the gaps stated as plainly as the results.

## What it does

**It is a model with a runtime around it.** The model looks at a terminal screen and decides what each row is; the
runtime draws that description as a GUI, or back as text. The rest of this page is what that pipeline can be held to.

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
| a second agent CLI reads perfectly without training | **323 frames of Freebuff at 100%** — Block, ToolCall, Thought and Diff — and it is the richest structured source in the corpus at 8.89 components a frame |
| the model works off its training application | trained on one; **every component of every batch recorded since**; and on the eight non-Grok recordings, three of which have a floor low enough to prove anything — all three perfect. Five of those eight have a floor of 100% and prove nothing |
| new screens, recorded after training, read perfectly | **every batch since training, every kind it held, nothing missed** — the per-batch numbers are in the report |
| a split screen's panes hold their own components | both paths ask the same question before splitting; audited on model pages too |
| a floating window is named | **`Frame`**, added in round 48 from a vim popup: the first kind added since the vocabulary settled, and the first that needed a boundary before it needed a predicate |
| the hard recordings hold their shapes | **15 hard recordings**, twelve at 100% on score and both round trips: an agent in a pane, an agent beside a shell, two agents, and two text buffers |
| an agent running in one pane of a split reads perfectly | 100% on the two recordings that drove the container (53.9% and 96.0% when it was written) |
| every word the page holds reaches the browser document | 105 recordings **on both paths**, checked; markup arrives escaped |
| every recording on this machine reads on either builder | **105 recordings, 3160 pages**, reader and model, no error |
| every page a builder makes is consistent with its screen |  frames audited on **each** path, no violations |
| the download works | **v0.36.0**, three platforms, verified by downloading it and running it |
| the checks can fail | **25 scripts and 4 fuzz properties, each shown failing with the output kept** (`docs/checks.md`); three refuse to pass on empty material and one has a clock |
| the viewer keeps up | typing, redraws, half-written files, cleared screens — each checked |
| exporting a session is cheap | 4.4 ms and 3.9 KB per page, linear to 1,510 pages, capped by a check |

## What is not proven, or cannot be

- **The training material is one agent's transcripts.** This machine has no other agent CLI — `claude`, `aider`, `codex`, `gemini`, `ollama` and `opencode` were all looked for, in WSL and on Windows, and none is installed. The claim is untested because the material does not exist here, not because it was not sought. Only software that draws *state* produces
  the components this catalogue describes — seven other programs recorded for training produced
  eleven screens and not one structured component. A second agent's transcripts are the only way to
  test whether these shapes are general, and I don't have any.
- **Every screen came from this machine**, and most were chosen by me. Eight clean batches is the
  strongest claim available; it is not the same as a stranger's workflow.
- **The model is at its ceiling, and that is measured three ways.** 60% more corpus: +0.1% row
  accuracy. A twentieth feature: worse on every screen family. Training on plain screens: three
  times worse. The limit is the vocabulary, not the data.
- **A perfect page score is partly the assembly's work.** The model's raw labels disagree with the
  reader on 2–12% of rows, and structural guards are what keep pages exact. That's by design, and it
  took a diagnostic to see it.
- **Some shapes are not representable — and the cost is measured.** A floating window is read as
  a block of text: vim's help window over a buffer round tripped at **74/74 lines (100%)**, so a
  consumer loses the *structure* of the overlay and none of its content. A tab strip is recognised
  only when numbered. **A divider-only pane row was on this list and is no longer**: a split
  with both panes blank round trips at **67/67 lines (100%)** with every row covered, so it is now
  a fifth hard recording rather than a claimed limitation.
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

## What the last week added, in one paragraph

The pane container — a split screen read one pane at a time, on both the reader and the model path —
is implemented and shipped. It took twelve rounds of attempts, seven reverted, before landing in five
consecutive green steps. The last defect it produced took six more rounds to find and was **one line**:
the two paths were asking different questions before deciding to split. What found it was a trace
printing what each path computed, after five plausible explanations had each been disproved by a small
test. That ratio — five theories, one trace — is the most useful thing this month taught about how the
work should be done.

## What the last week added, in one paragraph

The pane container — a split screen read one pane at a time, on both the reader and the model path —
is implemented and shipped in v0.8.0. It took twelve rounds of attempts, seven reverted, before landing
in five consecutive green steps. The last defect it produced took six more rounds to find and was **one
line**: the two paths were asking different questions before deciding to split. What found it was a
trace printing what each path computed, after five plausible explanations had each been disproved by a
small test. That ratio — five theories, one trace — is the most useful thing this month taught about how
the work should be done.
## One limitation worth stating plainly

A wide character — a CJK glyph, an emoji — fills two terminal cells, and the page records both. So the
text of a row reads `日 本 語` where a person would write `日本語`, and the **round trip is still exact**:
the page is faithful to the screen, not to the string.

That is the honest description of the current state. A consumer that lays out by column puts every
glyph exactly where it was; a consumer that treats the line as a string shows the gaps.

**The widths themselves are measured correctly** — a marker placed after two wide glyphs lands at
column 11, not 9, so the emulator advances two cells per wide character. The page is column-exact; the
only loss is that the string carries the continuation cells as spaces. Closing that needs a flagged
continuation cell in the frame format, which is a format change rather than a reader one, and it is
written down rather than wished away.
## What "100% on fresh material" does and does not mean

Eight batches recorded after the model was trained, **nothing missed in any of them**:
Block, Meter, Thought and ToolCall all perfect.

**`Diff` was missing until it was asked for.** The agent reports an *edit* as a tool line with
counts — `♦ Edit edit.txt +1/-1` — but asking it to run `git diff` makes the output block hold a real
two-sided diff, and the reader finds it: `Diff 1/1` on a recording made after training. So all six
kinds are perfect on new material.

The claim is now: *all seven kinds are perfect on new material from the training application* — with the
one caveat that a diff only appears when the application is asked to show one. Every row of that sentence has a number behind it,
and the sentence is narrower than the one it replaces.

## How to check any of this yourself

One command answers "is this tree sound", and takes about eight minutes:

```
powershell -File scripts/check-everything.ps1
```

It runs 19 checks and prints one table. With `-Quick` it skips the five that record new material
and answers in under a minute — that is the loop for working; the full run is the loop for shipping.
With `-Tag v0.11.1` it also downloads that release and proves the archive does what this document says.

**Every line in the table can fail**, and each was made to fail at least once before it was trusted:

- the audit fails when it inspected nothing, and was proven failing by planting a masked span outside a
  screen;
- `check-corpus` fails on a recording that produces no page;
- `check-sizes` fails when fewer than eight lines come back, so a perfect score on an empty screen is a
  failure, not a pass;
- `check-diff-fresh` fails when no diff appeared, even if every page it did see was perfect;
- `check-scrub` fails when the catalogue's name is scrubbed away.

If a run says "every check passed", that is what it means — and the number of checks is the number that
have been caught lying at least once.

## What I would do with another month

1. **A second agent's transcripts.** This is the only claim the project cannot test for itself: every
   screen ever recorded came from one application. The vocabulary's generality is assumed, not measured.
2. **A layout check for the renderers.** `check-html` compares words and — since this week — pane
   structure. It still cannot tell whether a page's *arrangement* matches the screen's.
3. **Wide-character strings.** The page is column-exact and the string is padded; a flagged continuation
   cell in the frame format would close the gap between the two.
4. **Distrust the suite a little more.** Twenty-one properties are proven; the ones that failed were
   always the ones nobody had pointed at material they could not handle. The next defect is most likely
   in a check that has only ever been given easy input.

## If you have material from another application

The one claim this project cannot test is that its vocabulary generalises beyond the application it was
trained on. If you have transcripts from another agent — or any program that draws *state* — here is the
whole procedure, and it takes minutes rather than rounds:

```
# 1. record it, or point at sessions you already have
ui-lm-capture record --rows 50 --cols 160 --out sessions/other -- <the program>

# 2. does the page describe its own screen, and do both builders agree?
ui-lm-capture audit -- sessions/other
ui-lm-capture audit --model models/page.json -- sessions/other
ui-lm-capture score --session sessions/other --model models/page.json
ui-lm-capture tui --session sessions/other --all --model models/page.json
```

Read the output in this order:

- **`score` prints a floor beside the model's figure.** A floor near 100% means the screen held almost no
  structure, so a perfect score proves nothing; a low floor and a high score is the result that matters.
- **`audit` must report no violations** on both paths. A violation means the page names a row the screen
  does not have — a description no renderer could draw.
- **`tui --all` must be 100.0%.** Below that, the page has lost content or put it in the wrong place.

Then add the recordings under `eval/` and run `scripts/check-all.ps1`: the new set joins the table, and
`check-docs` will keep the counts honest from then on.

What to look for in the score: which **kinds** appear. A program that shows panels, tabs, tool calls and
meters tests the vocabulary as it stands. A program with a shape the catalogue does not have — a calendar,
a chart, a form — is the interesting case, because it is where the vocabulary's limits become visible
rather than theoretical.
